# TavernGym guide

[Back to AutoTavern](../README.md) · [Architecture](ARCHITECTURE.md) · [Card coverage](CONTENT.md)

TavernGym exposes one eight-player game, a single-seat environment and a batched environment
through Python bindings over a Rust engine. See the [quick start](../README.md#quick-start)
for installation.

## One game

```python
import json, taverngym

g = taverngym.Game(seed=1)               # eight seats; RulesConfig defaults
while not g.is_over():
    for p in range(g.num_players):
        if g.needs_action(p):
            g.step(p, g.bot_action(p))   # the scripted bot; or any legal action index
print(list(g.placements()))
view = json.loads(g.view(0))             # what seat 0 sees (debugging, rendering)
saved = g.state_json()                   # exact save; taverngym.Game.from_json(saved) restores
```

## Gym-style environment

`BattlegroundsEnv` is one learning seat against seven scripted bots:

```python
import taverngym

env = taverngym.BattlegroundsEnv(seed=0)
obs, info = env.reset()
action = int(obs["action_mask"].nonzero()[0][0])  # choose one legal action
obs, reward, terminated, truncated, info = env.step(action)
```

## Batched environment (for training frameworks)

`BatchEnv` steps many full eight-player games together, in Rust threads with the GIL released.
You choose which seats you control (`controlled`); scripted bots play the rest, so the same
interface covers one learner against bots and self-play (`controlled=range(8)`). A *slot* is
one controlled seat of one game: slot `i` is seat `controlled[i % k]` of game `i // k`.
In the rollout sketch below, `policy` is your own function that selects one action per slot.

```python
from taverngym import BatchEnv

env = BatchEnv(n_games=64, seed=0, controlled=(0,), threads=8)
obs = env.observe()
while True:
    # features float32 [n_slots, OBS_FLOATS], cards int32 [n_slots, OBS_CARDS],
    # action_mask bool [n_slots, N_ACTIONS], ready bool [n_slots]
    actions = policy(obs["features"], obs["cards"], obs["action_mask"])  # int per slot
    rewards, dones, info = env.step(actions)  # info["placement"], info["rejected"]
    obs = env.observe()
```

- Only slots with `ready` set act; the actions of the other slots are ignored. Several seats of
  one game can be ready at once (the recruit phase is simultaneous).
- The reward is the placement reward when a seat's episode ends (1st = 1, 8th = -1, linear),
  plus `illegal_penalty` (default -0.01) for a rejected action, which changes nothing.
- A game whose controlled seats have all finished starts again with a seed derived from
  `(seed, game, episode)`. Results do not depend on `threads`.
- `env.save(i)` / `env.load(i, s)` save and restore game `i` exactly; `env.game(i)` returns an
  independent copy to inspect or search from.

**Observation.** A player view with only what the real client shows: `features` (3,227 floats:
global state, warband, hand, Tavern, pending choice, quests, opponents' last seen warbands)
and `cards` (124 card-embedding indices into the `N_CARDS` card vocabulary, 0 = empty;
`taverngym.card_info(i)` describes index `i`). `taverngym.layout()` lists the sizes of every
block, so a model can slice the vector without hard-coding offsets.

**Actions.** 245 discrete actions with an exact legal mask (`taverngym.action_name(a)` names
one):

| range | action |
| --- | --- |
| 0 / 1 / 2 / 3 | end turn / refresh / freeze / upgrade the Tavern |
| 4-5 | hero power (slot 0, 1) |
| 6 / 7 | Dark Gift button / back out of a spell, hero power or Activate target choice |
| 8-15 | buy Tavern card 0-7 |
| 16-22 | sell warband minion 0-6 |
| 23-29 | activate warband minion 0-6 |
| 30-45 | choose option 0-15 of a pending choice (Discover, target, hero, trinket...) |
| 46-125 | play hand card 0-9 at warband position 0-7 |
| 126-195 | magnetize hand card 0-9 onto warband minion 0-6 |
| 196-244 | move warband minion 0-6 to position 0-6 |

## Rules configuration

Every rule that is not a published server value (pool sizes, offer weights, lobby setup, Dark
Gift and trinket parameters, safety cutoffs) is a field of `RulesConfig`, documented in
[the configuration source](../crates/taverngym/src/config.rs). Pass a JSON object of the fields to change:

```python
cfg = json.loads(taverngym.default_config_json())   # every field with its default
g = taverngym.Game(seed=1, config_json=json.dumps({"hero_choices": 4}))
env = BatchEnv(8, config={"timewarp_enabled": True})
```

Unknown keys are an error. `experimental_mode` enables an opt-in rule set (a global turn-four
Quest, Buddies and Timewarp) with extra actions outside the fixed action space.

## Speed

On one core a complete eight-player game with scripted bots takes a few milliseconds and a
late-game combat tens of microseconds (the paper reports 190-280 games and 52-76 thousand
combats per second). Measure on your machine with

```bash
cargo run --release -p taverngym --example bench -- 2000
```

`--example fuzz` plays random legal actions and checks state invariants and pool conservation
after every action; `--example play` prints the event log of one bot game.

## Card data

`crates/taverngym/data/cards.json` is generated by `tools/gen_carddb.py` from a content pack
compiled from the 36.6.1 client data (253216), with the 36.6.3 changes in
`tools/overlays/36.6.3.json`. The content pack itself is not distributed; [the content report](CONTENT.md)
lists the implemented cards.

