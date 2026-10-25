# Timewarp

The Timewarped Tavern is the global system of patch 34.2: on a visit, a player buys Timewarped
cards with Chronum instead of Gold. The rules follow the official patch notes 34.2 and the
Battlegrounds Developer Insight of 2025-12-01 (official forum topic 154893); card data comes from
client 36.6.3.253932. Where those sources are silent, the simulator makes its own choice, marked
"assumption" below. Code: `crates/taverngym/src/content/timewarp.rs`.

## When a visit happens

- The global system is off by default (`timewarp_enabled = false`): the global systems of 36.6.3
  are trinkets and Dark Gifts. When it is on, every player visits the Minor Timewarp on turn 6
  and the Major Timewarp on turn 9 (`timewarp_turns`, default `[6, 9]`).
- Two heroes visit whatever the setting (extra visits when the system is on): Morchie (Warped
  Conflux: "On Turn 5, visit the Minor Timewarp") and Murozond, Unbounded (Alternate Timeline:
  "On Turn 8, visit the Major Timewarp"). A player who gains one of these Hero Powers later still
  visits on that turn, or not at all if that turn has already started (assumption).

## Card pool

The Minor Timewarp offers the Tier 3 Timewarp cards, the Major Timewarp the Tier 5 ones:

| | Minor | Major |
| --- | --- | --- |
| Minions | 48 | 62 |
| Spells | 9 | 14 |
| Second Hero Powers ("Power of ...") | 10 | none |

`tools/gen_carddb.py` takes the content pack's Timewarp assets without Naga (rotated out) and the
temporarily banned Timewarped Vaelastrasz. The overlay `tools/overlays/36.6.3.json` keeps only
the cards the client data marks as currently available (this smaller set, not the pack's larger
list) and drops Timewarped Thief and Master Thief ("Discover a minion from your warband last
game"): a simulated game has no previous game. Timewarped Investment stays in without the flag.

Each pool card has three attributes from client tags (`CardDef` fields in
`crates/taverngym/src/cards.rs`, stored in `data/cards.json`):

| Field | Meaning |
| --- | --- |
| `tw_types` | Types the card is tied to for offers; can differ from its own type (Timewarped Tide Razor: Pirate; Timewarped Greenskeeper, a Dragon: none) |
| `tw_weight` | Offer weight, 100 by default (Timewarped Secrecy: 50) |
| `tw_min_minions` | Minimum warband size: 4 for Timewarped Mothership, Recycler and Geomancer |

## A visit

- It opens at the start of the turn, before the normal recruit phase, as a pending choice
  (`Pending::Timewarp`) with `timewarp_chronum` Chronum (default 2, from the 34.2 notes). The
  player takes a card or leaves; all other actions wait until the visit ends.
- Cards cost Chronum only (1 or 2; 3 for Timewarped Secrecy), never Gold. "Casts When Bought"
  spells are cast at once (a "Power of X" spell gives that Hero Power as a second one); other
  cards go to the hand, or wait for a free slot if it is full. The board, hand and Hero Power do
  not work inside the visit, so purchases trigger no "after you buy" effects.
- The visit ends when no remaining card is affordable or the player leaves (the last option).
  Unspent Chronum is lost (assumption); only Timewarped Investment carries Chronum over. No visit
  opens if no card can be offered.

## Offer rules

A card can be offered only if:

1. it is in this visit's pool (Tier 3 for Minor, Tier 5 for Major);
2. when tied to types, one of them is in the lobby and the player is "in" it: at least 2 minions
   of that type (Minor) or 3 (Major) across warband and hand (`timewarp_type_min`, default
   `[2, 3]`; a minion of all types counts for every type). A player is always "in" their hero's
   associated type and their most common type;
3. the warband holds at least `tw_min_minions` minions;
4. it meets the Developer Insight's card conditions: Timewarped Lei needs a hero with a Buddy,
   Whirl-O-Tron a Deathrattle and Murk-Eye a Battlecry minion on the board; Saurolisk is not for
   N'Zoth, nor Skipper for heroes whose minions cost or sell differently (Trade Prince Gallywix,
   Millhouse Manastorm, Sindragosa, Fungalmancer Flurgl, Cap'n Hoggarr, Kael'thas Sunstrider);
5. for a second Hero Power: the player has a single Hero Power, the offered one is not it, and it
   belongs to a hero in this lobby's hero pool.

The five cards, sorted by cost, are 3 minions and 2 non-minions (spells or second Hero Powers);
three cost 1 Chronum and two cost 2 or more. They are filled in this order:

- The most common type is the lobby type with the most minions across warband and hand (ties
  random). The simulator draws two of the three minions from cards tied to it (fewer if there are
  not enough) and fits the rest of the offer around their costs.
- The three 1-Chronum cards are split between minions and non-minions as 3+0, 2+1 or 1+2, tried
  in random order: the first split that fills all five slots wins, else the fullest (assumption).
- A Minor offer includes one second Hero Power if one is eligible and fits, never more. The other
  slots are drawn by `tw_weight` without repeats, at most one card per type besides the most
  common one.

## Special cards

- Timewarped Investment ("Gain 1 extra Chronum at your next Timewarp") is offered, by the
  simulator's choice, only in Minor visits of the global system, to save a Chronum for the Major.
- Timewarped Chimera, Nalaa and Mythrax ("each type", "different types") need minions of three
  types in warband and hand (Developer Insight); the data has no tag, so they are listed by hand.
- Timewarped Jungle King and Timewarped Tamuzo: the Developer Insight's condition "summoned a
  minion in the previous combat" is not implemented; only the Beast rule applies (assumption).
- Timeworn Candelabra (2-cost minion; Lesser: Minor, Greater: Major), Timewarped Beanstalk (1-cost
  card, Major) and Timewarped Revelation (1-cost and 2-cost minion, Minor) Discover by the visit's
  rules: only cards the player could be offered, two of three tied to the most common type when
  possible, other types at most once, never a second Hero Power (assumption: no official source).

## Scripts, interface and settings

Every pool card is scripted (`content/tw_*.rs`, second Hero Powers in `content/hero_powers.rs`;
tests in `crates/taverngym/tests/tw_*.rs`). Where text is silent, untiered random Gets and
Discovers are capped at the Tavern Tier and non-permanent combat buffs last for that combat only.

The observation shows a pending choice of kind "timewarp" (the cards with their Chronum costs,
then "leave") and a global feature with the Chronum of the open visit, or else the Chronum saved
for the next. `DebugCmd::OpenTimewarp { p, major }` (`src/debug.rs`) forces a visit.

| Field | Default | Meaning |
| --- | --- | --- |
| `timewarp_enabled` | `false` | Every player visits on `timewarp_turns` |
| `timewarp_turns` | `[6, 9]` | Turns of the Minor and Major visits |
| `timewarp_chronum` | `2` | Chronum received at each visit |
| `timewarp_type_min` | `[2, 3]` | Minions needed to be "in" a type (Minor, Major) |

Enable it through the rules config, e.g. `taverngym.BatchEnv(8, config={"timewarp_enabled": True})`.
`experimental_mode` (off by default) also turns visits on, with other opt-in systems, and keeps
Chronum between visits; this page covers the default rules.
