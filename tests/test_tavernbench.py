"""TavernBench: tasks, labels, released answers and saved states are consistent, and the
tool-use harness runs on the released task states."""

import gzip
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tavernbench"))

from tavernbench import score  # noqa: E402

SET = ROOT / "tavernbench" / "data" / "eval"

RESULTS = ROOT / "tavernbench" / "results"


def test_released_answers_cover_their_tasks():
    ids = {json.loads(line)["id"] for line in open(SET / "prompts.jsonl")}
    for table, n in (("eval", 368), ("tools", 144)):
        for f in sorted((RESULTS / table).glob("*/answers.jsonl")):
            rows = [json.loads(line) for line in open(f)]
            got = [r["id"] for r in rows]
            assert len(got) == len(set(got)) and set(got) <= ids, f
            missing = n - len(got)
            assert missing == (1 if f.parent.name == "gemini-3.8-flash" else 0), f
            assert not any(r.get("error") for r in rows), f


def test_every_task_has_a_label_and_the_labels_score_themselves():
    prompts = [json.loads(line) for line in open(SET / "prompts.jsonl")]
    labels = {d["id"]: d for d in map(json.loads, open(SET / "labels.jsonl"))}
    assert len(prompts) == 368 and set(labels) == {p["id"] for p in prompts}
    for p in prompts:
        lab = labels[p["id"]]
        assert "labeller" not in (lab.get("meta") or {})
        if lab["scoring"] in ("exact", "set"):
            row = {"id": p["id"], "response": json.dumps({"answer": lab["answer"]})}
            assert score.score_answer(lab, row)[0] == 1, p["id"]


def test_the_tool_harness_plays_on_a_released_state():
    from tavernbench import engine as E
    from tavernbench.run_tools import TOOL_SCENARIOS, Session

    with gzip.open(SET / "states.jsonl.gz", "rt", encoding="utf-8") as f:
        states = {d["id"]: d for d in map(json.loads, f)}
    prompts = [json.loads(line) for line in open(SET / "prompts.jsonl")]
    tool_ids = [p["id"] for p in prompts if p["scenario"] in TOOL_SCENARIOS]
    assert len(tool_ids) == 144 and all(i in states for i in tool_ids)
    spec = states[next(i for i in tool_ids if i.startswith("P1"))]
    s = Session(spec, sims=16)
    out = s.call("test_play", {"actions": []})
    assert "error" not in out and abs(out["win"] + out["tie"] + out["loss"] - 1) < 1e-6
    assert "unknown action" in s.call("inspect", {"actions": ["dance"]}).get("error", "")
    for spec in states.values():
        for st in spec.get("states") or [spec["state"]]:
            # every saved state loads in this build and holds nothing this build does not write
            saved = json.loads(st)
            assert set(_paths(saved)) <= set(_paths(json.loads(E.from_state(st).state_json())))
            assert "hero_bans" not in saved["cfg"] and saved["cfg"]["turn_action_caps"] == []


def _paths(x, path=""):
    if isinstance(x, dict):
        for k, v in x.items():
            p = f"{path}.{'#' if k.isdigit() else k}"
            yield p
            yield from _paths(v, p)
    elif isinstance(x, list):
        for v in x:
            yield from _paths(v, path + "[]")
