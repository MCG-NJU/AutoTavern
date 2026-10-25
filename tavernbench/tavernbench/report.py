"""TavernBench results table: systems x categories, play score and rule score, cost.

    python -m tavernbench.report --set data/eval --runs results/eval --systems a,b,c \
        [--out results.json]

Rule scenarios (RULE) ask how the rules execute; the other scenarios are play scenarios. A
category's accuracy is the mean of its scenarios' accuracies; the play and rule scores are means
over scenarios (each scenario weighs the same). A task a system did not answer counts as wrong.
Two-way strategy scenarios (ids S*) would use balanced accuracy (chance 0.5).
"""

from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path

from .chance import expected
from .score import load_answers, over_budget, score_answer

RULE = {"R1", "R3", "R4", "R5", "R6", "R7", "E1", "E2", "E4", "E5"}
CATS = {"R": "rules", "E": "economy", "B": "combat", "P": "positioning", "O": "ordering",
        "T": "targets", "D": "selection", "S": "strategy", "N": "strategy"}


def load(set_dir):
    labels = {d["id"]: d for d in map(json.loads, open(Path(set_dir) / "labels.jsonl"))}
    prompts = {d["id"]: d for d in map(json.loads, open(Path(set_dir) / "prompts.jsonl"))}
    return labels, prompts


REAL_TIME_S = 1.0  # the real-time score counts only answers given within this many seconds


def evaluate(labels, prompts, answers_path, latency_fn=None):
    """`latency_fn(row)`: the seconds a task counts for (default: the row's measured latency_s;
    the paper uses a standardized latency)."""
    got = {}
    toks_in = toks_out = lat = 0.0
    n_ans = trunc = 0
    lats = []
    if answers_path and Path(answers_path).exists():
        for r in load_answers(answers_path).values():
            if r["id"] in labels:
                got[r["id"]] = r
    per_s = defaultdict(list)
    per_rt = defaultdict(list)
    for iid, p in prompts.items():
        r = got.get(iid)
        ok = 0
        if r is not None:
            ok, _ = score_answer(labels[iid], r)
            n_ans += 1
            toks_in += r.get("input_tokens") or 0
            toks_out += r.get("output_tokens") or 0
            lat += (latency_fn(r) if latency_fn else (r.get("latency_s") or 0))
            lats.append(latency_fn(r) if latency_fn else (r.get("latency_s") or 0))
            trunc += r.get("finish_reason") == "length" or over_budget(r)
        per_s[p["scenario"]].append((labels[iid]["answer"], ok))
        sec = None if r is None else (latency_fn(r) if latency_fn else (r.get("latency_s") or 0))
        in_time = r is not None and sec <= REAL_TIME_S
        per_rt[p["scenario"]].append((labels[iid]["answer"], ok if in_time else 0))
    acc = {s: scenario_acc(s, v) for s, v in per_s.items()}
    rt = {s: scenario_acc(s, v) for s, v in per_rt.items()}
    rt_play = [v for s, v in rt.items() if s not in RULE]
    return acc, {"real_time_play": sum(rt_play) / max(1, len(rt_play)), "rt_by_scenario": rt,
                 "answered": n_ans, "tokens_in": toks_in / max(1, n_ans),
                 "tokens_out": toks_out / max(1, n_ans), "latency_s": lat / max(1, n_ans),
                 "latency_median_s": sorted(lats)[len(lats) // 2] if lats else 0.0,
                 "truncated": trunc / max(1, n_ans)}


def scenario_acc(sid, rows):
    """Accuracy; for the two-way strategy scenarios the balanced accuracy (the mean of the
    accuracies on the tasks whose answer is A and on those whose answer is B), so that always
    giving the more frequent answer scores 0.5."""
    if sid.startswith("S"):
        by = defaultdict(list)
        for lab, ok in rows:
            by[lab].append(ok)
        if len(by) > 1:
            return sum(sum(v) / len(v) for v in by.values()) / len(by)
    return sum(ok for _, ok in rows) / len(rows)


def summarize(acc):
    play = [v for s, v in acc.items() if s not in RULE]
    rule = [v for s, v in acc.items() if s in RULE]
    cats = defaultdict(list)
    for s, v in acc.items():
        if s not in RULE:
            cats[CATS[s[0]]].append(v)
    out = {c: sum(v) / len(v) for c, v in cats.items()}  # play part of each category
    out.update({"play": sum(play) / max(1, len(play)), "rule_score": sum(rule) / max(1, len(rule)),
                "n_play": len(play), "n_rules": len(rule)})
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", required=True)
    ap.add_argument("--runs", default="results/eval")
    ap.add_argument("--systems", required=True, help="run directory names, comma separated")
    ap.add_argument("--out", default="")
    a = ap.parse_args()
    labels, prompts = load(a.set)
    res = {}
    ch = defaultdict(list)
    for iid, p in prompts.items():
        ch[p["scenario"]].append(expected(labels[iid], p))
    chance = {s: (0.5 if s.startswith("S") else sum(v) / len(v)) for s, v in ch.items()}
    res["chance"] = {"by_scenario": chance, **summarize(chance)}
    for name in a.systems.split(","):
        acc, cost = evaluate(labels, prompts, Path(a.runs) / name / "answers.jsonl")
        res[name] = {"by_scenario": acc, **summarize(acc), **cost}
    if a.out:
        Path(a.out).write_text(json.dumps(res, indent=1))
    cols = ["rules", "combat", "positioning", "ordering", "targets", "selection", "strategy",
            "economy"]
    print(f"{'system':28s} play  rule  " + " ".join(f"{('1v1' if c == 'rules' else c[:5]):>5s}" for c in cols)
          + "  out_tok trunc")
    for name, r in res.items():
        print(f"{name:28s} {r['play']:.2f}  {r['rule_score']:.2f}  "
              + " ".join(f"{r.get(c, float('nan')):5.2f}" for c in cols)
              + f"  {r.get('tokens_out', 0):7.0f} {r.get('truncated', 0):.2f}")


if __name__ == "__main__":
    main()
