"""Tool-augmented LLMs on TavernBench: the model gets TavernGym as tools.

    python -m tavernbench.run_tools --set data/eval --model MODEL --out RUN/answers.jsonl \
        [--base-url URL] [--key-env OPENAI_API_KEY | --env FILE] [--only P1,O2] [--per 4] \
        [--calls 12] [--sims 256] [--extra '{"reasoning_effort":"low"}']

Same prompt as the text-only protocol, plus two tools over the task's own state in TavernGym:
  test_play(actions)  from the task's state apply recruit actions (buy / sell / play / move /
                      refresh / freeze / upgrade / hero_power / activate / magnetize / choose),
                      then fight the next combat --sims times (the built system's budget per
                      candidate); returns the warband, win / tie / loss and damage, or the error
  inspect(actions)    the same without the combat (to see a pending choice's options)
Tool calls are capped at --calls per task; the final answer is the JSON object of the task.
Tasks: the scenarios whose answer is a play that can be tested in the engine (P, O, T, E3, D3,
D5); the task states come from the set's states.jsonl.gz. --per takes the first k tasks of each
scenario. Resumable by id. The key handling follows run_openai.py (never written anywhere).
"""

from __future__ import annotations

import argparse
import concurrent.futures as cf
import gzip
import json
import os
import re
import threading
import time
from pathlib import Path

from . import engine as E
from . import render as R
from .run_openai import DEFAULT_ENV, api_key_and_base, safe_error

TOOL_SCENARIOS = {f"{c}{k}" for c in "POT" for k in range(1, 6)} | {"E3", "D3", "D5"}
ACTIONS_DOC = (
    "'buy <tavern position>', 'play <hand position> at <board position>' (a minion; the board "
    "position it takes, 0 = leftmost), 'play <hand position>' (a spell or a card that then asks "
    "for a target), 'choose <k>' (the k-th listed target or option of a pending choice), 'sell "
    "<board position>', 'move <board position> to <board position>', 'magnetize <hand position> "
    "onto <board position>', 'activate <board position>', 'hero_power', 'refresh', 'freeze', "
    "'upgrade'. Positions refer to the state at that moment: after a hand card is played, the "
    "cards to its right shift left by one, and a bought minion joins the hand at the end")
TOOLS = [
    {"type": "function", "function": {
        "name": "test_play", "description":
            "Starting from the task's state, apply recruit actions in order in the real game "
            "engine, then fight the next combat many times. Returns the resulting warband, the "
            "win / tie / loss probabilities and the expected damage taken, or the first illegal "
            "action. Actions: " + ACTIONS_DOC + ". An empty list tests the state as it is.",
        "parameters": {"type": "object", "properties": {"actions": {
            "type": "array", "items": {"type": "string"}}}, "required": ["actions"]}}},
    {"type": "function", "function": {
        "name": "inspect", "description":
            "Starting from the task's state, apply recruit actions in order and return the "
            "resulting state without fighting (use it to see a pending choice's options or the "
            "effect of a play). Same action vocabulary as test_play.",
        "parameters": {"type": "object", "properties": {"actions": {
            "type": "array", "items": {"type": "string"}}}, "required": ["actions"]}}},
]
TOOL_NOTE = ("\n\n## Tools\nYou may call test_play and inspect (at most {calls} calls in total) "
             "to test candidate plays in the real game engine before answering; every call starts "
             "from the state above. When done, answer with the JSON object only.")
FINAL_NOTE = "No more tool calls. Answer now with the JSON object only, in the requested format."


def state_text(g):
    v = E.view(g)
    me = v["me"]
    lines = [f"Turn {v['turn']}. Gold {me['gold']}. Tavern tier {me['tier']}."
             f"{' Tavern frozen.' if me.get('frozen') else ''}",
             R.zone("Your warband (left to right)", me["board"]),
             R.zone("Your hand", me["hand"]),
             R.zone("Tavern (positions 0..)", me["shop"])]
    pend = me.get("pending")
    if pend:
        names = {}
        for zone in (me["board"], me["hand"], me["shop"]):
            for m in zone:
                names[m.get("eid")] = m.get("name", "?")
        opts = pend.get("targets") or pend.get("options") or []
        lines.append("Pending choice (" + str(pend.get("kind", "choose")) + "): " + "; ".join(
            f"{k}: {names.get(o, o) if not isinstance(o, dict) else o.get('name', o)}"
            for k, o in enumerate(opts)))
    return "\n".join(lines)


_PAT = [
    (r"^buy (\d+)$", lambda g, m: E.A_BUY + int(m[1])),
    (r"^play (\d+) at (\d+)$", lambda g, m: E.play_index(int(m[1]), int(m[2]))),
    (r"^play (\d+)$", lambda g, m: _play_any(g, int(m[1]))),
    (r"^choose (\d+)$", lambda g, m: E.A_CHOOSE + int(m[1])),
    (r"^sell (\d+)$", lambda g, m: E.A_SELL + int(m[1])),
    (r"^move (\d+) to (\d+)$", lambda g, m: E.move_index(int(m[1]), int(m[2]))),
    (r"^magnetize (\d+) onto (\d+)$", lambda g, m: E.A_MAGNETIZE + int(m[1]) * E.N_BOARD + int(m[2])),
    (r"^activate (\d+)$", lambda g, m: E.A_ACTIVATE + int(m[1])),
    (r"^hero_power$", lambda g, m: E.A_HERO_POWER),
    (r"^refresh$", lambda g, m: E.A_REFRESH),
    (r"^freeze$", lambda g, m: E.A_FREEZE),
    (r"^upgrade$", lambda g, m: E.A_UPGRADE),
]


def _play_any(g, h):
    """A spell or targeting card: position 0; a minion: the rightmost position."""
    legal = set(E.legal(g))
    for pos in (0, len(E.me(g)["board"])):
        a = E.play_index(h, pos)
        if a in legal:
            return a
    return E.play_index(h, 0)


def do_apply(g, actions):
    for s in actions:
        s = s.strip().lower().replace("hero power", "hero_power")
        a = None
        for pat, fn in _PAT:
            m = re.match(pat, s)
            if m:
                a = fn(g, m)
                break
        if a is None:
            return f"unknown action '{s}'"
        if a not in E.legal(g):
            return f"illegal action '{s}' in this state"
        g.step(E.ME, a)
    return None


class Session:
    def __init__(self, spec, sims):
        self.spec, self.sims = spec, sims

    def call(self, name, args):
        if name not in ("test_play", "inspect"):
            return {"error": f"unknown tool {name}"}
        g = E.from_state(self.spec["state"])
        err = do_apply(g, list(args.get("actions") or []))
        out = {"state": state_text(g)}
        if err:
            out["error"] = err
            return out
        if name == "test_play":
            if E.me(g).get("pending"):
                out["error"] = "a choice is pending; add 'choose <k>' to the actions"
                return out
            r = E.preview(g, n=self.sims)
            out.update({k: round(float(r[k]), 3) for k in ("win", "tie", "loss")})
            out["expected_damage_taken"] = round(float(r.get("dmg_taken", 0.0)), 2)
        return out


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

    def ask(self, item, spec):
        a = self.a
        sess = Session(spec, a.sims)
        msgs = [{"role": "system", "content": item["system"]},
                {"role": "user", "content": item["prompt"] + TOOL_NOTE.format(calls=a.calls)}]
        t0 = time.time()
        calls, tin, tout, text, finish, err = 0, 0, 0, None, None, None
        trace = []
        final = False
        for step in range(a.calls + 3):
            kw = dict(model=a.model, messages=msgs, max_tokens=a.max_tokens)
            if calls < a.calls and not final:
                kw["tools"], kw["tool_choice"] = TOOLS, "auto"
            if self.extra:
                kw["extra_body"] = self.extra
            r = None
            for attempt in range(6):  # a 429 waits for the gateway's window, then retries
                try:
                    r = self.client.chat.completions.create(**kw)
                    break
                except Exception as e:  # noqa: BLE001 - recorded
                    import openai
                    if isinstance(e, openai.RateLimitError) and attempt < 5:
                        body = e.body if isinstance(e.body, dict) else {}
                        body = body.get("error", body) if isinstance(body.get("error"), dict) else body
                        ra = (body.get("details") or {}).get("retry_after")
                        time.sleep(float(ra or 5) + 1)
                        continue
                    err = safe_error(e)
                    break
            if r is None:
                break
            if r.usage:
                tin += r.usage.prompt_tokens or 0
                tout += r.usage.completion_tokens or 0
            if not getattr(r, "choices", None):  # gateway reply without choices: an error
                err = "empty reply (no choices)"
                break
            ch = r.choices[0]
            finish = ch.finish_reason
            msg = ch.message
            tcs = msg.tool_calls or []
            if not tcs:
                text = msg.content or ""
                if not final and not re.search(r"\{.*\}", text, re.S):
                    # prose or nothing: one more turn without tools, asking for the JSON
                    msgs.append({"role": "assistant", "content": text})
                    msgs.append({"role": "user", "content": FINAL_NOTE})
                    final = True
                    continue
                break
            msgs.append({"role": "assistant", "content": msg.content or "",
                         "tool_calls": [{"id": t.id, "type": "function", "function": {
                             "name": t.function.name, "arguments": t.function.arguments}}
                             for t in tcs]})
            for t in tcs:
                calls += 1
                try:
                    args = json.loads(t.function.arguments or "{}")
                except json.JSONDecodeError:
                    args = {}
                res = sess.call(t.function.name, args) if calls <= a.calls else {
                    "error": "tool budget exhausted; answer now"}
                if calls >= a.calls:
                    final = True
                trace.append({"tool": t.function.name, "args": args,
                              "result": {k: v for k, v in res.items() if k != "state"}})
                msgs.append({"role": "tool", "tool_call_id": t.id, "content": json.dumps(res)})
            if final:
                msgs.append({"role": "user", "content": FINAL_NOTE})
        return {"id": item["id"], "model": a.label or a.model, "response": text,
                "input_tokens": tin, "output_tokens": tout, "tool_calls": calls,
                "finish_reason": finish, "latency_s": round(time.time() - t0, 2), "error": err,
                "trace": trace, "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S")}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--label", default="")
    ap.add_argument("--out", required=True)
    ap.add_argument("--env", default=DEFAULT_ENV, help="a file with API_KEY / API_BASE lines")
    ap.add_argument("--key-env", default="OPENAI_API_KEY")
    ap.add_argument("--base-url", default="")
    ap.add_argument("--no-proxy", action="store_true", help="ignore the shell's proxy settings")
    ap.add_argument("--extra", default="")
    ap.add_argument("--max-tokens", type=int, default=4096)
    ap.add_argument("--read-timeout", type=float, default=300)
    ap.add_argument("--calls", type=int, default=12)
    ap.add_argument("--sims", type=int, default=256)
    ap.add_argument("--only", default="")
    ap.add_argument("--per", type=int, default=0, help="first k tasks of each scenario")
    ap.add_argument("--jobs", type=int, default=4)
    a = ap.parse_args()
    items = [json.loads(l) for l in open(Path(a.set) / "prompts.jsonl")]
    with gzip.open(Path(a.set) / "states.jsonl.gz", "rt", encoding="utf-8") as f:
        specs = {d["id"]: d for d in map(json.loads, f)}
    only = [x for x in a.only.split(",") if x]
    sel, per = [], {}
    for it in items:
        if it["scenario"] not in TOOL_SCENARIOS or it["id"] not in specs:
            continue
        if only and it["scenario"] not in only and it["id"] not in only:
            continue
        per[it["scenario"]] = per.get(it["scenario"], 0) + 1
        if a.per and per[it["scenario"]] > a.per:
            continue
        sel.append(it)
    out = Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    done = {json.loads(l)["id"] for l in open(out) if not json.loads(l).get("error")} \
        if out.exists() else set()
    todo = [d for d in sel if d["id"] not in done]
    print(f"{a.model}: {len(todo)} tasks to ask ({len(done)} done)", flush=True)
    runner = Runner(a)
    lock = threading.Lock()
    with cf.ThreadPoolExecutor(a.jobs) as ex, open(out, "a") as fo:
        futs = [ex.submit(runner.ask, d, specs[d["id"]]) for d in todo]
        for f in cf.as_completed(futs):
            r = f.result()
            with lock:
                fo.write(json.dumps(r) + "\n")
                fo.flush()
            print(f"{r['id']}: {str(r['response'])[:50]!r} calls={r['tool_calls']} "
                  f"out={r['output_tokens']} {r['latency_s']}s"
                  + (f" ERR {r['error']}" if r["error"] else ""), flush=True)


if __name__ == "__main__":
    main()
