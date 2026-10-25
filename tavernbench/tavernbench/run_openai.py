"""Answer a TavernBench set with a model behind an OpenAI-compatible gateway.

    python -m tavernbench.run_openai --set data/eval --model MODEL --out RUN/answers.jsonl \
        [--base-url URL] [--key-env OPENAI_API_KEY | --env FILE] [--rpm 36] [--max-tokens 4096] \
        [--extra '{"thinking": {...}}'] [--only R1,B3]

One fresh call per item with the benchmark's system prompt, streamed (long non-streamed
calls are cut by the gateway). Requests leave on a timer (every 60/rpm s, however many are
still in flight; the gateway counts requests per key in fixed 60 s windows), a 429 holds all
sends until the window resets and requeues the item without counting it as a failure. The
key is read from the environment variable --key-env (the address from --base-url or
OPENAI_BASE_URL), or from an --env file (API_KEY / API_BASE lines); it is never written anywhere,
and an error is recorded as its type and HTTP status only.
Resumable: ids already answered without an error are skipped.
"""

from __future__ import annotations

import argparse
import json
import os
import queue
import threading
import time
from pathlib import Path

DEFAULT_ENV = ""
DEFAULT_BASE = "https://api.openai.com/v1"


def read_env(path):
    out = {}
    for line in open(path):
        line = line.strip()
        if "=" in line and not line.startswith("#"):
            k, v = line.split("=", 1)
            out[k.strip().removeprefix("export ").strip()] = v.strip().strip('"').strip("'")
    return out


def safe_error(e, status=None):
    """An error as recorded in an answer file: its type and HTTP status only. The text of an
    exception or of an error response can echo the request (URL, headers, key)."""
    status = status or getattr(e, "status_code", None) or getattr(
        getattr(e, "response", None), "status_code", None)
    return (type(e).__name__ if e is not None else "HTTPError") + (
        f" (HTTP {status})" if status else "")


def api_key_and_base(a):
    """(key, base URL): from an --env file (API_KEY / API_BASE lines), else from the
    environment variable --key-env and --base-url / OPENAI_BASE_URL."""
    if a.no_proxy:  # e.g. a gateway inside the network: do not go through the shell's proxy
        for k in ("http_proxy", "https_proxy", "HTTP_PROXY", "HTTPS_PROXY"):
            os.environ.pop(k, None)
    if a.env:
        env = read_env(a.env)
        return env.get("API_KEY"), (a.base_url or env.get("API_BASE") or DEFAULT_BASE).rstrip("/")
    base = a.base_url or os.environ.get("OPENAI_BASE_URL") or DEFAULT_BASE
    return os.environ[a.key_env], base.rstrip("/")


class Runner:
    def __init__(self, a):
        import httpx
        from openai import OpenAI
        key, base = api_key_and_base(a)
        self.client = OpenAI(api_key=key, base_url=base, max_retries=0,
                             timeout=httpx.Timeout(connect=20, read=a.read_timeout, write=30,
                                                   pool=None),
                             http_client=httpx.Client(trust_env=not a.no_proxy))
        self.a = a
        self.extra = json.loads(a.extra) if a.extra else None
        self.hold_until = 0.0
        self.lock = threading.Lock()
        self.n429 = 0

    def ask(self, item):
        """One streamed call. Returns (row, retry_after or None)."""
        import openai
        a = self.a
        msgs = [{"role": "system", "content": item["system"]},
                {"role": "user", "content": item["prompt"]}]
        kw = dict(model=a.model, messages=msgs, max_tokens=a.max_tokens, stream=True,
                  stream_options={"include_usage": True})
        if a.temperature is not None:
            kw["temperature"] = a.temperature
        if self.extra:
            kw["extra_body"] = self.extra
        t0 = time.time()
        text, reasoning, usage, finish = [], [], None, None
        try:
            for ch in self.client.chat.completions.create(**kw):
                if ch.usage:
                    usage = ch.usage
                for c in ch.choices or []:
                    d = c.delta
                    if d is not None:
                        if d.content:
                            text.append(d.content)
                        r = getattr(d, "reasoning_content", None) or getattr(d, "reasoning", None)
                        if r:
                            reasoning.append(r)
                    if c.finish_reason:
                        finish = c.finish_reason
        except openai.RateLimitError as e:
            body = e.body if isinstance(e.body, dict) else {}
            body = body.get("error", body) if isinstance(body.get("error"), dict) else body
            ra = (body.get("details") or {}).get("retry_after")
            if ra is None and e.response is not None:
                ra = e.response.headers.get("retry-after")
            return None, float(ra or 5)
        except Exception as e:  # noqa: BLE001 - recorded, the item is retried later
            return self.row(item, None, None, None, t0, error=safe_error(e)), None
        return self.row(item, "".join(text), usage, finish, t0,
                        reasoning_chars=sum(map(len, reasoning))), None

    def row(self, item, text, usage, finish, t0, error=None, reasoning_chars=0):
        det = getattr(usage, "completion_tokens_details", None) if usage else None
        return {
            "id": item["id"], "model": self.a.label or self.a.model, "response": text,
            "input_tokens": getattr(usage, "prompt_tokens", None),
            "output_tokens": getattr(usage, "completion_tokens", None),
            "reasoning_tokens": getattr(det, "reasoning_tokens", None) if det else None,
            "reasoning_chars": reasoning_chars, "finish_reason": finish,
            "latency_s": round(time.time() - t0, 2), "error": error,
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S"),
        }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--label", default="", help="model name written in the answers")
    ap.add_argument("--out", required=True)
    ap.add_argument("--env", default=DEFAULT_ENV, help="a file with API_KEY / API_BASE lines")
    ap.add_argument("--key-env", default="OPENAI_API_KEY",
                    help="read the API key from this environment variable")
    ap.add_argument("--base-url", default="")
    ap.add_argument("--no-proxy", action="store_true", help="ignore the shell's proxy settings")
    ap.add_argument("--rpm", type=float, default=36)
    ap.add_argument("--inflight", type=int, default=80)
    ap.add_argument("--max-tokens", type=int, default=4096)
    ap.add_argument("--temperature", type=float, default=None)
    ap.add_argument("--extra", default="", help="JSON merged into the request body")
    ap.add_argument("--read-timeout", type=float, default=180)
    ap.add_argument("--retries", type=int, default=3, help="per item, errors only (not 429)")
    ap.add_argument("--only", default="")
    ap.add_argument("--limit", type=int, default=0)
    a = ap.parse_args()

    items = [json.loads(l) for l in open(Path(a.set) / "prompts.jsonl")]
    only = [s for s in a.only.split(",") if s]
    if only:
        items = [d for d in items if d["scenario"] in only or d["id"] in only]
    out = Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    done = set()
    if out.exists():
        for l in open(out):
            r = json.loads(l)
            if not r.get("error"):
                done.add(r["id"])
    todo = [d for d in items if d["id"] not in done]
    if a.limit:
        todo = todo[: a.limit]
    print(f"{a.model}: {len(todo)} items to ask ({len(done)} already answered)", flush=True)

    run = Runner(a)
    work = queue.Queue()
    for d in todo:
        work.put((d, 0))
    pending = [len(todo)]
    inflight = threading.Semaphore(a.inflight)
    wlock = threading.Lock()
    fo = open(out, "a")

    def task(item, tries):
        try:
            row, retry_after = run.ask(item)
            if retry_after is not None:
                with run.lock:
                    run.n429 += 1
                    run.hold_until = max(run.hold_until, time.monotonic() + retry_after + 0.5)
                work.put((item, tries))  # a 429 is not a failure of the item
                return
            if row["error"] and tries + 1 < a.retries:
                work.put((item, tries + 1))
                return
            with wlock:
                fo.write(json.dumps(row, ensure_ascii=False) + "\n")
                fo.flush()
                pending[0] -= 1
            print(f"{row['id']}: {str(row['response'])[-50:]!r} out={row['output_tokens']} "
                  f"{row['latency_s']}s{' ERR ' + row['error'] if row['error'] else ''}",
                  flush=True)
        finally:
            inflight.release()

    gap = 60.0 / a.rpm
    nxt = time.monotonic()
    while pending[0] > 0:
        try:
            item, tries = work.get(timeout=1.0)
        except queue.Empty:
            continue
        now = time.monotonic()
        wait = max(nxt - now, run.hold_until - now, 0.0)
        if wait:
            time.sleep(wait)
        inflight.acquire()
        threading.Thread(target=task, args=(item, tries), daemon=True).start()
        nxt = time.monotonic() + gap
    fo.close()
    print(f"done; {run.n429} rate-limit holds", flush=True)


if __name__ == "__main__":
    main()
