# TavernGym 37.0.2 update

[Back to AutoTavern](../README.md) · [中文说明](../README.zh-CN.md)

TavernGym 1.1.0 targets Hearthstone Battlegrounds client **37.0.2.256589** and server rules
**37.0.2**. This update changes game content and rules while retaining the existing Rust
engine structure, Python `Game` / `BattlegroundsEnv` / `BatchEnv` interfaces, 245 actions,
3,227 numeric features and 124 card indices per observation.

## Game changes

- Updated minion stats, tiers, keywords and normal/Golden effects, including Murcules,
  Ashen Corruptor, Living Azerite, Spark Snapper, Hooktusk and Last One Standing.
- Updated Tavern spell costs and effects, Trinket costs, sizes, counters and offers, and
  Dark Gift bonuses, offer windows, pair exclusions and turn 7–9 type guarantees.
- Updated pool membership and implemented the returned Kiri's Eclipse and Minion Bait.
  Copies keep independent counters; held auras are removable, while acquired minions keep
  their purchase stats. Minion Bait displays its selected type before purchase.
- Nellie's crew selections count as real Discovers without adding a card to hand. Tavern
  refill effects respect frozen slots, the seven-slot limit and shared-pool returns.

The card table retains all **3,983 base IDs in the same order**. The current candidate and
implementation counts are generated in [CONTENT.md](CONTENT.md); candidate membership is
still subject to lobby and configuration filters.

## Benchmark and saved states

TavernBench's original 36.6.3 prompts, labels, saved states and model answers are unchanged.
Offline scoring therefore remains comparable with the paper. Executing a scene in the
current engine uses 37.0.2 behavior, including changed card effects and offer rules; it is
not a replay of the original benchmark runtime. Use the **`v1.0.0`** release for historical
simulator-based reproduction.

Keep the original runtime with a saved experiment. An unchanged card-index table does not
guarantee that results are unchanged across rules patches. No training framework, trained
agent, model weights or benchmark item generators are added in this release.

## Data sources

- [Official 37.0.2 patch notes](https://hearthstone.blizzard.com/en-us/news/24307311).
- [Official 37.0.1 hotfix notes](https://us.forums.blizzard.com/en/hearthstone/t/3701-hotfix-patch/166622)
  and [pool ban list](https://us.forums.blizzard.com/en/hearthstone/t/bans-what-and-where/151289),
  as reviewed for this release on 2026-11-04.
- Client fields use the pinned [HearthSim/hsdata revision
  `31b034c26f24b8a66ece845ffd03c1ca397b9a02`](https://github.com/HearthSim/hsdata/tree/31b034c26f24b8a66ece845ffd03c1ca397b9a02).
  The server overlay is [tools/overlays/37.0.2.json](../tools/overlays/37.0.2.json).

The generated `cards.json` is included and is sufficient for building. Regenerating it
requires a separately supplied resource bundle (`tools/gen_carddb.py --bundle DIR`), as in
the original release. Undocumented offer probabilities remain simulator assumptions;
registry coverage and regression tests do not establish exact server behavior.
