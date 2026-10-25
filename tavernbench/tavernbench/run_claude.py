"""Answer a TavernBench set with Claude through the Claude Code CLI (`claude -p`).

    python -m tavernbench.run_claude --set DIR --model sonnet --out RUN/answers.jsonl [--jobs 3]

Each item is one fresh, tool-less call with the benchmark's own system prompt (it replaces
Claude Code's), low effort and no project settings, run from an empty directory so that no
local configuration is loaded. Resumable: ids already in --out are skipped.
"""

from __future__ import annotations

import argparse
import concurrent.futures as cf
import json
import os
import subprocess
import tempfile
import time
from pathlib import Path

NEUTRAL = Path(tempfile.gettempdir()) / "tavernbench-neutral-cwd"


def ask(item: dict, model: str, effort: str, timeout: int) -> dict:
    NEUTRAL.mkdir(exist_ok=True)
    cmd = ["claude", "-p", item["prompt"], "--model", model, "--system-prompt", item["system"],
           "--tools", "", "--effort", effort, "--output-format", "json",
           "--setting-sources", ""]
    t0 = time.time()
    try:
        out = subprocess.run(cmd, cwd=NEUTRAL, capture_output=True, text=True, timeout=timeout)
        raw = out.stdout.strip()
        data = json.loads(raw[raw.find("{"):]) if "{" in raw else {}
    except (subprocess.TimeoutExpired, json.JSONDecodeError) as e:
        data = {"is_error": True, "result": None, "error": repr(e)}
    usage = data.get("usage") or {}
    return {
        "id": item["id"], "model": f"claude-{model}", "response": data.get("result"),
        "input_tokens": (usage.get("input_tokens") or 0)
        + (usage.get("cache_read_input_tokens") or 0)
        + (usage.get("cache_creation_input_tokens") or 0),
        "output_tokens": usage.get("output_tokens"),
        "latency_s": round(time.time() - t0, 2),
        "cost_usd": data.get("total_cost_usd"),
        "is_error": data.get("is_error"),
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S"),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", required=True)
    ap.add_argument("--model", default="sonnet")
    ap.add_argument("--effort", default="low")
    ap.add_argument("--out", required=True)
    ap.add_argument("--jobs", type=int, default=3)
    ap.add_argument("--timeout", type=int, default=300)
    ap.add_argument("--only", default="")
    a = ap.parse_args()
    items = [json.loads(l) for l in open(Path(a.set) / "prompts.jsonl")]
    only = [s for s in a.only.split(",") if s]
    if only:
        items = [d for d in items if d["scenario"] in only]
    out = Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    done = {json.loads(l)["id"] for l in open(out)} if out.exists() else set()
    todo = [d for d in items if d["id"] not in done]
    print(f"{len(todo)} items to ask ({len(done)} already answered)", flush=True)
    with cf.ThreadPoolExecutor(a.jobs) as ex, open(out, "a") as fo:
        futs = [ex.submit(ask, d, a.model, a.effort, a.timeout) for d in todo]
        for f in cf.as_completed(futs):
            r = f.result()
            fo.write(json.dumps(r, ensure_ascii=False) + "\n")
            fo.flush()
            print(f"{r['id']}: {str(r['response'])[:60]!r} {r['latency_s']}s", flush=True)
    os.sync()


if __name__ == "__main__":
    main()
