//! Trinkets: trinkets that give, copy or replace trinkets, and change the trinket offers.
//! `grant_trinket` / `replace_trinket` run the new trinket's immediate effect as if it was bought;
//! replacement offers are `Pending::Trinket { replace: Some(..) }` (declinable).

use super::*;
use crate::cards::CardKind;

/// The key trinket `t` is kept under: a second one of a trinket the player already has (two
/// Mystery Cubes that both turned into it) is a copy (`CardId::copy`) with its own counters,
/// like Souvenir Stand's. `old` is the trinket it replaces (not another one).
fn instance_key(p: &crate::state::Player, t: CardId, old: Option<CardId>) -> CardId {
    let taken = |k: CardId| p.trinkets.iter().any(|x| *x == k && Some(*x) != old);
    if taken(t) && !taken(t.copy()) {
        t.copy()
    } else {
        t
    }
}

/// Add trinket `t` to `side`'s player and run its immediate effect.
pub fn grant_trinket(c: &mut Ctx, side: Side, t: CardId) {
    let Some(p) = c.player(side) else { return };
    let t = instance_key(p, t, None);
    p.trinkets.push(t);
    attach(c, side, t);
}

/// Replace trinket `old` (same position; appended if it is gone) with `t`.
pub fn replace_trinket(c: &mut Ctx, side: Side, old: CardId, t: CardId) {
    let Some(p) = c.player(side) else { return };
    let t = instance_key(p, t, Some(old));
    // Each acquired Cube keeps its own current replacement, including repeated Cubes.
    if let Some(source) = p.cube_sources.iter_mut().find(|source| **source == old) {
        *source = t;
    }
    match p.trinkets.iter().position(|x| *x == old) {
        Some(i) => p.trinkets[i] = t,
        None => p.trinkets.push(t),
    }
    let (from, to) = (&db().get(old).name, &db().get(t).name);
    let who = c.who(side);
    c.g.log(|| format!("{who}'s {from} becomes {to}"));
    attach(c, side, t);
}

fn attach(c: &mut Ctx, side: Side, t: CardId) {
    if let Some(fx) = script(t).and_then(|s| s.on_attach) {
        let me = Me {
            side,
            eid: 0,
            card: t,
            golden: false,
            opt: 0,
            pos: 0,
            data: 0,
        };
        let prev = c.src.replace(t);
        fx(c, me);
        c.src = prev;
    }
}

/// Trinkets of `kind` that `side`'s player could be offered (lobby universe, eligible).
fn eligible(
    c: &Ctx,
    side: Side,
    kind: CardKind,
    f: impl Fn(&crate::cards::CardDef) -> bool,
) -> Vec<CardId> {
    let Some(p) = c.owner[side] else {
        return vec![];
    };
    c.g.trinket_universe
        .iter()
        .copied()
        .filter(|t| {
            let d = db().get(*t);
            d.kind == kind && f(d) && c.g.trinket_eligible(p, *t)
        })
        .collect()
}

const CUBE: &str = "BG30_MagicItem_703";

/// Lesser Trinkets Mystery Cube does not offer: itself; a trinket that acts on other trinkets
/// (gives one, turns into one, changes the next offer); one that counts toward something or grows
/// over time; one whose benefit only comes turns later. A trinket that gives something now and
/// again every few turns (Tickatus Sticker, Conch Portrait) or every turn (Goblin Wallet) is
/// offered.
const CUBE_EXCLUDED: [&str; 25] = [
    "BG30_MagicItem_703", // Mystery Cube
    // Other trinkets.
    "BG35_MagicItem_816", // Orb of the Unknown: get a random Lesser Trinket
    "BG35_MagicItem_818", // Mysterious Orb: your next Trinket is a Lesser Trinket
    "BG30_MagicItem_888", // Souvenir Stand: becomes a copy of the Greater Trinket you buy
    "BG32_MagicItem_271", // Ornate Clock: next turn's Greater Trinket
    "BG30_MagicItem_891", // Trip Vouchers: in 2 turns, a Greater Trinket
    "BG36_MagicItem_407", // Dragonhead Staff: replaced after 13 attacks
    "BG36_MagicItem_408", // Quartermaster's Hook: replaced after 50 Gold
    "BG36_MagicItem_409", // Ruby Tusk: replaced after 10 Blood Gems
    "BG36_MagicItem_410", // Soul Battery: replaced after 25 summons
    "BG36_MagicItem_411", // Reinvigorating Light: Discover a second Hero Power
    // Counts toward something, or grows.
    "BG30_MagicItem_713", // Bleeding Heart: after 8 friendly minions die
    "BG30_MagicItem_931", // Lucky Kitty: after 7 friendly minions die
    "BG35_MagicItem_302", // Stormcoil Sticker: after 6 friendly minions die
    "BG32_MagicItem_808", // Bloodbound Earrings: after you cast 4 spells
    "BG36_MagicItem_307", // Wand of Divination: after you cast 3 spells on minions
    "BG35_MagicItem_152", // Demonic Tapestry: after you Refresh 4 times
    "BG36_MagicItem_302", // Gold Mallet: improved by Golden minions played
    "BG36_MagicItem_403", // Hammer of Twilight: improved by discards
    "BG36_MagicItem_414", // Weighted Gauntlet: doubles each turn
    // Pays off turns later.
    "BG32_MagicItem_428", // Worn Treasure Map: in two turns, 10 Gold
    "BG36_MagicItem_301", // Lockbox Portrait: a Lockbox that opens turns later
    "BG30_MagicItem_425", // Azeroth Model Globe: every two turns
    "BG30_MagicItem_435", // Goldenizer Supply: every 3 turns
    "BG35_MagicItem_814", // Bulging Wallet: when you reach Tier 6
];

/// Mystery Cube at the start of a turn: the trinket it became is replaced again -- choose from 2
/// new Lesser Trinkets, every turn (like Ethereal Evidence). This runs after the start of turn
/// effects, so the trinket being replaced still acts this turn and the new one's "when you get it"
/// effect follows.
pub fn cube_turn_start(c: &mut Ctx, side: Side) {
    let cube = cid(CUBE);
    // Migrate old saves that only kept a single Cube pointer.
    let legacy = c.card_data(side, cube);
    if let Some(p) = c.player(side)
        && p.cube_sources.is_empty()
        && legacy > 0
    {
        p.cube_sources.push(CardId((legacy - 1) as u16));
    }
    let Some(p) = c.player(side) else { return };
    p.cube_sources.retain(|cur| p.trinkets.contains(cur));
    let sources = p.cube_sources.clone();
    for cur in sources {
        offer_swap(c, side, cur, CardKind::TrinketLesser, 2, true);
    }
}

/// Offer `n` trinkets of `kind` to replace `old` (free or paid; can't be declined).
fn offer_swap(c: &mut Ctx, side: Side, old: CardId, kind: CardKind, n: usize, free: bool) {
    let Some(p) = c.owner[side] else { return };
    let gold = c.g.players[p].gold;
    // A free swap is Mystery Cube's.
    let cands = c.board_home(side, |c| {
        eligible(c, side, kind, |d| {
            if free {
                !CUBE_EXCLUDED.contains(&d.id.as_str())
            } else {
                d.cost <= gold
            }
        })
    });
    let picks = c.g.rng.sample_indices(cands.len(), n);
    let options: Vec<CardId> = picks.into_iter().map(|i| cands[i]).collect();
    if !options.is_empty() {
        c.g.players[p].pending.push(Pending::Trinket {
            options,
            free,
            replace: Some(old),
        });
    }
}

/// Replace `me` with a random Greater Trinket associated with type `r` (from the lobby's
/// trinkets; any such trinket if none is available there).
fn upgrade_to_greater(c: &mut Ctx, me: Me, r: Races) {
    // Earned in combat (Dragonhead Staff's attacks, Soul Battery's summons): the trinket is
    // replaced at the start of the next turn; the level marks it.
    if c.combat {
        if c.card_level(me.side, me.card) == 0 {
            c.improve_card(me.side, me.card);
        }
        return;
    }
    let mut cands = eligible(c, me.side, CardKind::TrinketGreater, |d| {
        d.assoc.intersects(r)
    });
    if cands.is_empty() {
        cands = db()
            .iter()
            .filter(|d| {
                d.kind == CardKind::TrinketGreater
                    && d.pools.trinket
                    && d.assoc.intersects(r)
                    && script(d.cid).is_some()
            })
            .map(|d| d.cid)
            .collect();
    }
    if let Some(&t) = c.g.rng.pick(&cands) {
        // The mark and the count go with the trinket: the same one got again later starts from
        // nothing.
        if let Some(p) = c.player(me.side) {
            p.levels.retain(|x| x.0 != me.card);
            p.counters.retain(|x| x.0 != me.card);
        }
        replace_trinket(c, me.side, me.card, t);
    }
}

use crate::state::Pending;

/// `Script::shown` of a player-level "After N...: X. (N left!)" card that counts in its card data
/// (`Ctx::tick_counter`, `gold_counter`): the goal, then what is left.
pub(super) fn shown_left(c: &Ctx, sh: &crate::script::Shown, every: i32) -> Vec<i32> {
    vec![every, (every - c.card_data(sh.side, sh.card)).max(0)]
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Mystery Cube: choose from 2 new Lesser Trinkets to replace this for free (the swap can't
        // be declined); at the start of each turn what it became is replaced again
        // (`cube_turn_start`).
        (
            "BG30_MagicItem_703",
            Script {
                on_attach: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.cube_sources.push(me.card);
                    }
                    offer_swap(c, me.side, me.card, CardKind::TrinketLesser, 2, true)
                }),
                ..E
            },
        ),
        // Souvenir Stand: `Game::take_trinket`.
        ("BG30_MagicItem_888", E),
        // Trip Vouchers: in 2 turns, choose a Greater Trinket to buy and replace this.
        (
            "BG30_MagicItem_891",
            Script {
                start_of_turn: Some(|c, me| {
                    if c.tick_counter(me.side, me.card, 2) {
                        let n = c.g.cfg.trinket_choices;
                        offer_swap(c, me.side, me.card, CardKind::TrinketGreater, n, false);
                    }
                }),
                // "In 2 turns, choose a Greater Trinket to buy and replace this. (2 turns
                // left!)"
                shown: Some(|c, sh| shown_left(c, sh, 2)),
                ..E
            },
        ),
        // Ornate Clock: gain 2 Gold; your Greater Trinket comes next turn instead of Turn 9.
        (
            "BG32_MagicItem_271",
            Script {
                on_attach: Some(|c, me| {
                    c.gain_gold(me.side, 2);
                    let next = c.g.turn + 1;
                    if let Some(p) = c.player(me.side)
                        && !p.greater_offered
                    {
                        p.greater_turn = next;
                    }
                }),
                offer_ok: Some(|g, p| !g.players[p].greater_offered),
                ..E
            },
        ),
        // Orb of the Unknown: get a random Lesser Trinket.
        (
            "BG35_MagicItem_816",
            Script {
                on_attach: Some(|c, me| {
                    let cands = eligible(c, me.side, CardKind::TrinketLesser, |_| true);
                    if let Some(&t) = c.g.rng.pick(&cands) {
                        grant_trinket(c, me.side, t);
                    }
                }),
                ..E
            },
        ),
        // Mysterious Orb: gain 10 Gold; your next Trinket is a Lesser Trinket.
        (
            "BG35_MagicItem_818",
            Script {
                on_attach: Some(|c, me| {
                    c.gain_gold(me.side, 10);
                    if let Some(p) = c.player(me.side) {
                        p.next_trinket_lesser = true;
                    }
                }),
                ..E
            },
        ),
        // Corrupted Tome: get a Triple Prize; Triple Rewards become Triple Prizes
        // (`triple_reward_item`).
        (
            "BG35_MagicItem_812",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG35_MagicItem_812t", 1)),
                ..E
            },
        ),
        // Triple Prize: Discover a Tier 3 Darkmoon Prize.
        (
            "BG35_MagicItem_812t",
            Script {
                spell: Some(|c, me, _| super::darkmoon::discover_prize(c, me.side, me.card, 3)),
                ..E
            },
        ),
        // Dragonhead Staff: after 13 friendly minions attack, becomes a random Greater Dragon
        // Trinket.
        (
            "BG36_MagicItem_407",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Attacks { side, .. } if *side == me.side)
                        && c.tick_counter(me.side, me.card, 13)
                    {
                        upgrade_to_greater(c, me, Races::DRAGON);
                    }
                }),
                listens: ev::ATTACKS,
                start_of_turn: Some(|c, me| {
                    if c.card_level(me.side, me.card) > 0 {
                        upgrade_to_greater(c, me, Races::DRAGON);
                    }
                }),
                // "After 13 friendly minions attack, replace this with a random Greater Dragon
                // Trinket. (13 left!)"
                shown: Some(|c, sh| {
                    if c.card_level(sh.side, sh.card) > 0 {
                        vec![13, 0]
                    } else {
                        shown_left(c, sh, 13)
                    }
                }),
                ..E
            },
        ),
        // Quartermaster's Hook: after you spend 50 Gold, a random Greater Pirate Trinket.
        (
            "BG36_MagicItem_408",
            Script {
                on_event: Some(|c, me, e| {
                    gold_counter(c, me, e, 50, |c, me| {
                        upgrade_to_greater(c, me, Races::PIRATE)
                    })
                }),
                listens: ev::GOLD_SPENT,
                // "After you spend 50 Gold, replace this with a random Greater Pirate Trinket.
                // (50 Gold left!)"
                shown: Some(|c, sh| shown_left(c, sh, 50)),
                ..E
            },
        ),
        // Ruby Tusk: after you play 10 Blood Gems from hand, a random Greater Quilboar Trinket.
        (
            "BG36_MagicItem_409",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::GemPlayed { side, .. } if *side == me.side)
                        && c.cast_from_hand
                        && c.cast_card == Some(cid("BG20_GEM"))
                        && c.tick_counter(me.side, me.card, 10)
                    {
                        upgrade_to_greater(c, me, Races::QUILBOAR);
                    }
                }),
                listens: ev::GEM_PLAYED,
                // "After you play 10 Blood Gems from hand, replace this with a random Greater
                // Quilboar Trinket. (10 left!)"
                shown: Some(|c, sh| shown_left(c, sh, 10)),
                ..E
            },
        ),
        // Soul Battery: after you summon 25 minions, a random Greater Undead Trinket.
        (
            "BG36_MagicItem_410",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Summoned { side, .. } if *side == me.side)
                        && c.tick_counter(me.side, me.card, 25)
                    {
                        upgrade_to_greater(c, me, Races::UNDEAD);
                    }
                }),
                listens: ev::SUMMONED,
                start_of_turn: Some(|c, me| {
                    if c.card_level(me.side, me.card) > 0 {
                        upgrade_to_greater(c, me, Races::UNDEAD);
                    }
                }),
                // "After you summon 25 minions, replace this with a random Greater Undead
                // Trinket. (25 left!)"
                shown: Some(|c, sh| {
                    if c.card_level(sh.side, sh.card) > 0 {
                        vec![25, 0]
                    } else {
                        shown_left(c, sh, 25)
                    }
                }),
                ..E
            },
        ),
    ]
}
