"""Score answers:  python -m tavernbench.score --set DIR --answers RUN/answers.jsonl [--json]

Per item: correct (0/1) and a graded score in [0, 1]:
  exact       answer equals the label (lists compared exactly, strings case-insensitively)
  brier       probability triple; graded score = 1 - Brier/2, correct if the most likely
              outcome matches and Brier <= 0.1
  numeric_rel correct if |answer - label| <= max(1, tol * |label|); graded 1 - min(1, relerr)
  value_map   (value_map_str, value_json) answer is a key of `values`; graded score = value / best, correct if within
              `tol` of the best (sampling noise of simulated values)
Unparseable answers score 0. Prints per-scenario and per-category means and token use.
"""

from __future__ import annotations

import argparse
import json
import re
from collections import defaultdict
from pathlib import Path


def parse(resp: str):
    if resp is None:
        return None
    s = resp.strip()
    s = re.sub(r"^```(?:json)?\s*|\s*```$", "", s)
    m = re.search(r"\{.*\}", s, re.S)
    if not m:
        return None
    try:
        return json.loads(m.group(0))
    except json.JSONDecodeError:
        return None


MAX_OUTPUT_TOKENS = 4096  # per task, reasoning included (the benchmark's budget)


def over_budget(row: dict) -> bool:
    """Generated text beyond the budget. Structured answers (Jev's typed choices) generate no
    text; their billed output tokens count the option probabilities, so the budget does not
    apply."""
    return row.get("mode") != "structured" and (row.get("output_tokens") or 0) > MAX_OUTPUT_TOKENS


def score_answer(label: dict, row: dict) -> tuple[int, float]:
    """Score a completed answer; a valid JSON prefix does not rescue a truncated call.

    Older Claude logs do not have finish_reason, so absence alone is not a failure.
    """
    if row.get("error") or row.get("is_error"):
        return 0, 0.0
    if row.get("finish_reason") in {
        "length", "incomplete", "content_filter", "tool_calls", "timeout", "error",
    }:
        return 0, 0.0
    if over_budget(row):
        # over the per-task budget; some gateways do not stop generation at max_tokens
        return 0, 0.0
    return score_item(label, parse(row.get("response")))


def load_answers(path):
    """One final record per id, matching the resumable runners and report module."""
    got = {}
    with open(path) as source:
        for line in source:
            if line.strip():
                row = json.loads(line)
                got[row["id"]] = row
    return got


def score_item(label: dict, obj) -> tuple[int, float]:
    if obj is None:
        return 0, 0.0
    kind = label["scoring"]
    try:
        if kind == "exact":
            a, b = obj.get("answer"), label["answer"]
            if isinstance(b, str):
                ok = isinstance(a, str) and a.strip().lower() == b.lower()
            else:
                ok = a == b
            return int(ok), float(ok)
        if kind == "brier":
            p = {k: float(obj.get(k, 0)) for k in ("win", "tie", "loss")}
            z = sum(p.values()) or 1.0
            p = {k: v / z for k, v in p.items()}
            q = label["answer"]
            brier = sum((p[k] - q[k]) ** 2 for k in q)
            ok = max(p, key=p.get) == max(q, key=q.get) and brier <= 0.1
            return int(ok), 1 - brier / 2
        if kind == "numeric_rel":
            a, b = float(obj.get("answer")), float(label["answer"])
            err = abs(a - b)
            ok = err <= max(1.0, label.get("tol", 0.25) * abs(b))
            return int(ok), 1 - min(1.0, err / max(1.0, abs(b)))
        if kind == "set":
            got = {str(x).strip().upper() for x in obj.get("answer", [])}
            ok = got == {str(x).upper() for x in label["answer"]}
            return int(ok), float(ok)
        if kind == "numeric_abs":
            err = abs(float(obj.get("answer")) - float(label["answer"]))
            return int(err <= label.get("tol", 0.1)), 1 - min(1.0, err)
        if kind == "value_map_str":
            key = str(obj.get("answer", "")).strip().upper().replace(",", "").replace(" ", "")
            v = label["values"].get(key)
            if v is None:
                return 0, 0.0
            best = label["answer"]
            return int(v >= best - label.get("tol", 0)), (v / best if best else 1.0)
        if kind == "value_json":
            v = label["values"].get(json.dumps(obj.get("answer")))
            if v is None:
                return 0, 0.0
            best = label["answer"]
            return int(v >= best - label.get("tol", 0)), (v / best if best else 1.0)
        if kind == "value_map":
            key = ",".join(str(int(x)) for x in obj.get("answer", []))
            v = label["values"].get(key)
            if v is None:
                return 0, 0.0
            best = label["answer"]
            return int(v >= best - label.get("tol", 0)), (v / best if best else 1.0)
    except (TypeError, ValueError, AttributeError):
        return 0, 0.0
    raise ValueError(kind)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", required=True)
    ap.add_argument("--answers", required=True)
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()
    labels = {d["id"]: d for d in map(json.loads, open(Path(a.set) / "labels.jsonl"))}
    prompts = {d["id"]: d for d in map(json.loads, open(Path(a.set) / "prompts.jsonl"))}
    by_s, by_c = defaultdict(list), defaultdict(list)
    tok_in = tok_out = lat = 0.0
    rows = []
    for ans in load_answers(a.answers).values():
        lab = labels.get(ans["id"])
        if lab is None:
            continue
        ok, sc = score_answer(lab, ans)
        p = prompts[ans["id"]]
        by_s[p["scenario"]].append((ok, sc))
        by_c[p["category"]].append((ok, sc))
        tok_in += ans.get("input_tokens") or 0
        tok_out += ans.get("output_tokens") or 0
        lat += ans.get("latency_s") or 0
        rows.append({"id": ans["id"], "correct": ok, "score": round(sc, 4)})
    n = len(rows)
    summ = {
        "n": n,
        "accuracy": sum(r["correct"] for r in rows) / max(n, 1),
        "score": sum(r["score"] for r in rows) / max(n, 1),
        "by_scenario": {k: {"n": len(v), "acc": sum(o for o, _ in v) / len(v),
                            "score": sum(s for _, s in v) / len(v)} for k, v in by_s.items()},
        "by_category": {k: {"n": len(v), "acc": sum(o for o, _ in v) / len(v),
                            "score": sum(s for _, s in v) / len(v)} for k, v in by_c.items()},
        "tokens_in": tok_in, "tokens_out": tok_out, "mean_latency_s": lat / max(n, 1),
    }
    if a.json:
        print(json.dumps({"summary": summ, "items": rows}, indent=1))
        return
    print(f"items {n}  accuracy {summ['accuracy']:.3f}  score {summ['score']:.3f}  "
          f"tokens in/out {tok_in:.0f}/{tok_out:.0f}  latency {summ['mean_latency_s']:.1f}s")
    for k, v in sorted(summ["by_scenario"].items()):
        print(f"  {k:5s} n={v['n']:3d}  acc {v['acc']:.2f}  score {v['score']:.2f}")


if __name__ == "__main__":
    main()
