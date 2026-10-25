"""Answer a TavernBench set with Jev (TypeSafe AI's decision model, /v1/systemone).

    python -m tavernbench.run_jev --set data/eval --out RUN/answers.jsonl \
        [--env FILE --key-name jev-api | JEV_API_KEY in the environment] [--model jev-latest] \
        [--jobs 4]
    python -m tavernbench.run_jev --set data/eval --out RUN/answers.jsonl --redecode

Jev does not write text: it takes a state and typed questions and returns, per question, the
chosen option with a probability for every option. Each task is sent with the text a language
model sees: the rules summary, the player view and the card rules as the state, the task's
question as the instructions. Every task becomes one or more `choice` questions:

  options (who attacks first, win/tie/loss, arrangement, target, order, purchase, A/B, ...)
      -> the candidates: the options the task lists, every warband (hand) position for a
         target, every order of the listed actions, every purchase sequence the Gold allows
  win / tie / loss probabilities  -> win, tie, loss; the answer is Jev's probabilities
  probability of dying           -> die, survive; the answer is the probability of "die"
  expected damage                -> the integers 0..30; the answer is the mean
  gold, stats                    -> the integers 0..30 (gold) or 0..60 (Attack, Health; one
                                    question each per minion); the answer is the choice
  legal actions (a set)          -> legal, illegal for each listed action

Numbers are choices over integers because a score scale holds at most 10 levels (a choice up to
255 options). Order and purchase candidates are the keys of the task's option table, which
enumerates every order / every affordable sequence (all of them, so it carries no hint; never
their values). Jev's raw answers are kept with each row; --redecode rebuilds the benchmark
answers from them without asking again. Resumable by id.
"""

from __future__ import annotations

import argparse
import concurrent.futures as cf
import json
import os
import re
import threading
import time
from pathlib import Path

from .run_openai import safe_error

URL = "https://api.typesafe.ai/v1/systemone"


def read_key(path, name):
    for line in open(path):
        line = line.strip()
        if "=" in line and not line.startswith("#"):
            k, v = line.split("=", 1)
            if k.strip().removeprefix("export ").strip() == name:
                return v.strip().strip('"').strip("'")
    raise KeyError(name)


def question_text(prompt):
    q = prompt.split("## Question", 1)[1].split("## Answer format", 1)[0].strip()
    return q


def listed(q, pattern=r"^\s+([A-Z]|\d+):\s"):
    return [m.group(1) for m in re.finditer(pattern, q, re.M)]


def zone_size(prompt, zone="Your warband"):
    """Number of entries listed under a zone of the state ("  0. ..." lines)."""
    n = 0
    for line in prompt.split(zone, 1)[1].splitlines()[1:]:
        if re.match(r"^\s+\d+\.\s", line):
            n += 1
        elif line.strip() and not line.startswith("  "):
            break
    return n


def warband_size(prompt):
    return zone_size(prompt)


def numbers(text, top):
    """A number as a choice over the integers 0..top (a score scale holds at most 10 levels)."""
    return {"type": "choice", "instructions": text,
            "criteria": {str(i): f"the answer is {i}" for i in range(top + 1)}}


def mean(ans):
    return sum(int(k) * float(v) for k, v in ans["probabilities"].items())


def build(item, label):
    """(questions, decode) for one task; decode(answers) -> the benchmark answer object."""
    sid, prompt = item["scenario"], item["prompt"]
    q = question_text(prompt)
    kind = label["scoring"]

    def choice(opts, describe=None):
        crit = {str(o): (describe(o) if describe else f"answer {o}") for o in opts}
        return {"answer": {"type": "choice", "instructions": q, "criteria": crit}}

    def pick(conv=lambda x: x):
        return lambda a: {"answer": conv(a["answer"]["choice"])}

    if kind == "brier":
        qs = {"answer": {"type": "choice", "instructions": q, "criteria": {
            "win": "you win this combat", "tie": "the combat is a tie",
            "loss": "you lose this combat"}}}
        return qs, lambda a: {k: float(a["answer"]["probabilities"].get(k, 0.0))
                              for k in ("win", "tie", "loss")}
    if sid == "B1":
        return choice(["you", "opponent", "random"]), pick()
    if sid in ("B2", "R2"):
        return choice(["win", "tie", "loss"]), pick()
    if sid == "E4":
        return choice(["yes", "no"]), pick()
    if sid[0] in "PSN":  # lettered options listed in the question
        return choice(listed(q)), pick()
    if sid in ("D1", "D2", "D3", "D4"):  # the listed heroes / trinkets / discover options
        return choice(listed(q)), pick(int)
    if sid == "D5":  # a minion of the hand
        return choice(range(zone_size(prompt, "Your hand"))), pick(int)
    if sid.startswith("T"):  # a minion of the warband: every warband position, as the LLM sees
        return choice(range(warband_size(prompt))), pick(int)
    if kind == "value_map_str":  # an order of the listed actions
        return choice(label["values"].keys()), pick()
    if kind == "value_map":  # a purchase sequence of tavern positions
        return choice(label["values"].keys(), lambda o: f"buy tavern positions [{o}] in this order"
                      if o else "buy nothing"), \
            pick(lambda x: [int(v) for v in x.split(",") if v != ""])
    if sid == "B5":  # a probability: the probability of the "die" option
        qs = {"die": {"type": "choice", "instructions": q, "criteria": {
            "die": "you die in this combat", "survive": "you survive this combat"}}}
        return qs, lambda a: {"answer": float(a["die"]["probabilities"].get("die", 0.0))}
    if sid == "B4":  # an expectation: the mean of the distribution over 0..30
        return {"dmg": numbers(q, 30)}, lambda a: {"answer": round(mean(a["dmg"]), 3)}
    if sid in ("E1", "E2", "E5"):
        return {"n": numbers(q, 30)}, lambda a: {"answer": int(a["n"]["choice"])}
    if sid in ("R5", "R7"):
        qs = {"attack": numbers(q + " (the Attack)", 60),
              "health": numbers(q + " (the Health)", 60)}
        return qs, lambda a: {"answer": [int(a["attack"]["choice"]), int(a["health"]["choice"])]}
    if sid in ("R3", "R4", "R6"):
        n = warband_size(prompt) + (1 if sid == "R3" else 0)  # The played minion joins
        qs = {}
        for i in range(n):
            qs[f"atk{i}"] = numbers(q + f" (the Attack of the minion in position {i})", 60)
            qs[f"hp{i}"] = numbers(q + f" (the Health of the minion in position {i})", 60)
        return qs, lambda a: {"answer": [[int(a[f"atk{i}"]["choice"]), int(a[f"hp{i}"]["choice"])]
                                         for i in range(n)]}
    if sid == "R1":  # a set: one legal / illegal choice per listed action
        acts = re.findall(r"^\s+([A-H]):\s(.+)$", q, re.M)
        qs = {f"legal_{k}": {"type": "choice",
                             "instructions": f"Is this action legal right now? {t}",
                             "criteria": {"legal": "the action is legal now",
                                          "illegal": "the action is not legal now"}}
              for k, t in acts}
        return qs, lambda a: {"answer": [k for k, _ in acts
                                         if a[f"legal_{k}"]["choice"] == "legal"]}
    raise ValueError(f"no Jev mapping for {sid} / {kind}")


def ask(session, key, model, item, label, timeout):
    qs, decode = build(item, label)
    body = {"model": model, "state": item["prompt"].split("## Question", 1)[0].strip(),
            "questions": qs}
    t0 = time.time()
    err, resp, raw, usage = None, None, None, {}
    for attempt in range(4):
        try:
            r = session.post(URL, json=body, timeout=timeout,
                             headers={"Authorization": f"Bearer {key}"})
            if r.status_code == 429:
                err = "429"
                time.sleep(5 * (attempt + 1))
                continue
            if r.status_code >= 400:
                err = safe_error(None, r.status_code)
                if r.status_code < 500:
                    break
                time.sleep(2)
                continue
            data = r.json()
            usage = data.get("usage") or {}
            raw = data.get("answers")
            resp = json.dumps(decode(raw))
            err = None
            break
        except Exception as e:  # noqa: BLE001 - recorded
            err = safe_error(e)
            time.sleep(2)
    return {"id": item["id"], "model": model, "mode": "structured", "response": resp, "raw": raw,
            "input_tokens": usage.get("input_tokens"), "output_tokens": usage.get("output_tokens"),
            "n_questions": len(qs), "latency_s": round(time.time() - t0, 3), "error": err,
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S")}


def redecode(set_dir, out):
    """Rebuild the answers from the kept raw Jev answers (after a decoding fix)."""
    items = {d["id"]: d for d in map(json.loads, open(Path(set_dir) / "prompts.jsonl"))}
    labels = {d["id"]: d for d in map(json.loads, open(Path(set_dir) / "labels.jsonl"))}
    rows = [json.loads(l) for l in open(out)]
    for r in rows:
        r["mode"] = "structured"
        if r.get("raw"):
            _, decode = build(items[r["id"]], labels[r["id"]])
            try:
                r["response"], r["error"] = json.dumps(decode(r["raw"])), None
            except Exception as e:  # noqa: BLE001 - recorded
                r["response"], r["error"] = None, f"decode {type(e).__name__}: {e}"[:300]
    with open(out, "w") as fo:
        fo.writelines(json.dumps(r) + "\n" for r in rows)
    print(f"redecoded {sum(bool(r.get('raw')) for r in rows)} of {len(rows)} rows")


def main():
    import requests
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--env", help="a .env file holding the key (else $JEV_API_KEY)")
    ap.add_argument("--key-name", default="jev-api")
    ap.add_argument("--model", default="jev-latest")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--timeout", type=float, default=60)
    ap.add_argument("--only", default="")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--redecode", action="store_true")
    a = ap.parse_args()
    if a.redecode:
        return redecode(a.set, a.out)
    key = read_key(a.env, a.key_name) if a.env else os.environ["JEV_API_KEY"]
    items = [json.loads(l) for l in open(Path(a.set) / "prompts.jsonl")]
    labels = {d["id"]: d for d in map(json.loads, open(Path(a.set) / "labels.jsonl"))}
    only = [x for x in a.only.split(",") if x]
    if only:
        items = [d for d in items if d["scenario"] in only or d["id"] in only]
    out = Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    done = {json.loads(l)["id"] for l in open(out) if not json.loads(l).get("error")} \
        if out.exists() else set()
    todo = [d for d in items if d["id"] not in done]
    if a.limit:
        todo = todo[: a.limit]
    print(f"{a.model}: {len(todo)} tasks to ask ({len(done)} done)", flush=True)
    session = requests.Session()
    lock = threading.Lock()
    with cf.ThreadPoolExecutor(a.jobs) as ex, open(out, "a") as fo:
        futs = [ex.submit(ask, session, key, a.model, d, labels[d["id"]], a.timeout) for d in todo]
        for f in cf.as_completed(futs):
            r = f.result()
            with lock:
                fo.write(json.dumps(r) + "\n")
                fo.flush()
            print(f"{r['id']}: {str(r['response'])[:60]} {r['latency_s']}s"
                  + (f" ERR {r['error']}" if r["error"] else ""), flush=True)


if __name__ == "__main__":
    main()
