# TavernBench guide

[Back to AutoTavern](../README.md) · [Task data](../tavernbench/data) · [Released answers](../tavernbench/results)

TavernBench contains 960 tasks in 40 scenarios across eight categories. The paper evaluates
all systems on a fixed 368-task subset. All paths below are relative to the repository root.

## Files and labels

```text
tavernbench/
  tavernbench/          scoring, chance levels, report, model runners, tool-use harness
  data/full/            all 960 tasks: prompts.jsonl, labels.jsonl, states.jsonl.gz
  data/eval/            the 368 tasks every system answers in the paper: eight from each of 37
                        scenarios, and all tasks of the play-out scenarios D4 (16), N1 (29), N2 (27)
  results/eval/<system>/answers.jsonl    the paper's main table (TavernAgent, ten LLMs, Jev)
  results/tools/<model>/answers.jsonl    the paper's tool-use runs
```

- `prompts.jsonl`: what a model sees (`system`, `prompt`, `answer_format`, `scenario`,
  `category`, `max_output_tokens`). Each prompt holds a rules summary, the player's view and
  the rules text of every card in it.
- `labels.jsonl`: answers and scoring rules (`exact`, `set`, `brier`, `numeric_rel`, value maps
  with a tolerance for simulation noise). Simulation labels were computed with TavernGym: rule tasks by
  executing the stated actions; combat tasks from 4,096 simulated combats (the
  damage-distribution scenario from 400 single combats), arrangement tasks from 2,048 combats
  per arrangement, and short-term decisions from 1,024 combats after every option; strategy
  tasks are turn plans cut from natural games, each played out 128 times.
- `states.jsonl.gz`: for the 30 play scenarios (720 of the 960 tasks, 288 of the 368), the
  simulator state each task was cut from (`taverngym.Game.from_json`) and, where the answer is
  a play, the concrete actions behind each answer. The ten rule-execution scenarios (R1, R3-R7,
  E1, E2, E4, E5) need no state.

Hero and trinket choices are scored against top human players, as described in the paper.

## Evaluate a model

Evaluate a model and score it (run from `tavernbench/`; the runners need the optional
dependencies, `pip install ".[bench]"` from the repository root):

```bash
python -m tavernbench.run_openai --set data/eval --model MODEL --out answers/MODEL/answers.jsonl \
    --base-url https://api.example.com/v1 --key-env API_KEY
python -m tavernbench.report --set data/eval --runs answers --systems MODEL
```

`run_claude` uses the Claude Code CLI (installed and signed in separately), `run_jev` the Jev
API, and `run_tools` gives an OpenAI-compatible model two tools over the task's state in
TavernGym (`test_play`, `inspect`). Scoring and `report` need no model access.

## Score the released answers

From the repository root, run:

```bash
cd tavernbench
python -m tavernbench.report --set data/eval --runs results/eval \
    --systems tavernagent,claude-opus-5.5,claude-sonnet-5.5,gpt-6-astra,gpt-6.1-sol,gemini-3.8-flash,grok-4.7,glm-5.3,kimi-k2.6,intern-s2-preview,deepseek-v4.1-flash,jev
```

Play is the mean accuracy over the 30 play scenarios; rules covers the ten rule-execution
scenarios. A single answer file can also be scored per scenario with
`python -m tavernbench.score --set data/eval --answers results/eval/<system>/answers.jsonl`.

## Notes

Every result file has one answer per task, except `results/eval/gemini-3.8-flash`, which has
no answer for `P2-000`; a missing answer counts as wrong, as in the paper.
The item generators are not part of this release. The tasks were labelled with TavernGym as of
the paper; the simulator here includes later rule fixes, so recomputing a label with it can
differ in rare cases.

