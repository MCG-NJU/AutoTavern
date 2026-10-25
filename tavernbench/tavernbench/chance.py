"""Chance level of a TavernBench set: the expected accuracy of an answer drawn uniformly from
each task's options (choice and decision tasks; open answers - numbers, stat lists, action
sets - count 0; probability triples are scored for the uniform triple).

    python -m tavernbench.chance --set DIR [--json]
"""

from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path

from .score import score_item

CHOICES = {"B1": ["you", "opponent", "random"], "B2": ["win", "tie", "loss"],
           "R2": ["win", "tie", "loss"], "E4": ["yes", "no"]}


def expected(label, prompt):
    kind = label["scoring"]
    sid = prompt["scenario"]
    if kind in ("value_map", "value_map_str", "value_json"):
        vals = list(label["values"].values())
        best = label["answer"]
        tol = label.get("tol", 0)
        return sum(v >= best - tol for v in vals) / len(vals)
    if kind == "brier":
        return score_item(label, {"win": 1 / 3, "tie": 1 / 3, "loss": 1 / 3})[0]
    if kind != "exact":
        return 0.0
    if sid in CHOICES:
        return 1 / len(CHOICES[sid])
    if sid.startswith("P"):
        return 1 / 5
    if sid.startswith("S"):
        return 1 / 2
    if sid.startswith("N"):  # four-way next-move items
        return 1 / 4
    if sid in ("D1",):
        return 1 / 4
    if sid == "T1":
        return 1 / len(label["meta"])
    if sid in ("D2", "D4"):
        return 1 / max(2, len(label.get("meta", {}).get("mean_place", label.get("meta", {}))))
    return 0.0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", required=True)
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()
    labels = {d["id"]: d for d in map(json.loads, open(Path(a.set) / "labels.jsonl"))}
    by_s = defaultdict(list)
    for p in map(json.loads, open(Path(a.set) / "prompts.jsonl")):
        by_s[p["scenario"]].append(expected(labels[p["id"]], p))
    res = {k: sum(v) / len(v) for k, v in by_s.items()}
    if a.json:
        print(json.dumps(res, indent=1))
    else:
        for k, v in sorted(res.items()):
            print(f"  {k:5s} chance acc {v:.2f}")


if __name__ == "__main__":
    main()
