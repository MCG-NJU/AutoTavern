//! Timewarp cards (dragons / pirates): the Timewarped Tavern's own pool (docs/TIMEWARP.md).

use super::*;
use crate::script::{Hook, minion_has};

/// Azerite Empowerment (Tavern spell): give your minions +2/+2 twice.
const AZERITE_EMPOWERMENT: &str = "BG28_169";

/// Timewarped Pagle's per-combat state in `Minion::counter` (combat copy only).
const PAGLE_ATTACKING: i32 = 1;
const PAGLE_USED: i32 = 2;

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Timewarped Busker: Battlecry and Deathrattle: gain 1 (2) Gold next turn.
        (
            "BG34_Giant_001",
            Script {
                battlecry: Some(|c, me, _| c.gold_next_turn(me.side, me.g())),
                deathrattle: Some(|c, me| c.gold_next_turn(me.side, me.g())),
                ..E
            },
        ),
        // Timewarped Hooktail: whenever you cast a Tavern spell, give your minions +2/+2
        // (golden: twice).
        (
            "BG34_Giant_015",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::SpellCast {
                        side, tavern: true, ..
                    } = e
                        && *side == me.side
                    {
                        for _ in 0..me.g() {
                            for t in c.friendly(me.side) {
                                c.buff(t, 2, 2);
                            }
                        }
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Timewarped Dragonling: Start of Combat: give this minion and its neighbors stats
        // equal to (golden: twice) your Tier.
        (
            "BG34_Giant_029",
            Script {
                start_of_combat: Some(|c, me| {
                    let k = c.tier(me.side) as i32 * me.g();
                    let mut ts = c.neighbors(me.eid);
                    ts.push(me.eid);
                    for t in ts {
                        c.buff(t, k, k);
                    }
                }),
                ..E
            },
        ),
        // Timewarped Greenskeeper: Rally: trigger your right-most Battlecry and your right-most
        // Deathrattle (golden: twice). They may be on two different minions; a targeted Battlecry
        // takes a random legal target.
        (
            "BG34_Giant_041",
            Script {
                rally: Some(|c, me, _| {
                    for _ in 0..me.g() {
                        if let Some(t) = rightmost_with(c, me.side, Hook::Battlecry) {
                            c.trigger_battlecry_ext(me.side, t);
                        }
                        if let Some(t) = rightmost_with(c, me.side, Hook::Deathrattle) {
                            c.trigger_deathrattles(me.side, t);
                        }
                    }
                }),
                ..E
            },
        ),
        // Timewarped Promo-Drake: Start of Combat: give your minions +5/+5 (+10/+10). At the end of
        // your turn, improve this (by the base amount).
        (
            "BG34_Giant_088",
            Script {
                start_of_combat: Some(|c, me| {
                    let k = 5i32.saturating_mul(
                        me.g()
                            .saturating_add(c.minion(me.eid).map_or(0, |m| m.counter)),
                    );
                    for t in c.friendly(me.side) {
                        c.buff(t, k, k);
                    }
                }),
                end_of_turn: Some(|c, me| {
                    c.counter_perm(me.side, me.eid, me.g());
                }),
                // "Start of Combat: Give your minions +5/+5. At the end of your turn, improve
                // this."
                shown: Some(|_, sh| {
                    let k = 5i32.saturating_mul(sh.g().saturating_add(sh.counter()));
                    vec![k, k]
                }),
                flags: flag::IMPROVES,
                ..E
            },
        ),
        // Timewarped Red Whelp: Start of Combat: deal 3 damage to two random enemy minions (golden
        // text: 2 damage to four). Improves after you play a Dragon: the damage grows by its base
        // amount, the number of targets doesn't.
        (
            "BG34_Giant_091",
            Script {
                start_of_combat: Some(|c, me| {
                    let (base, n): (i32, usize) = if me.golden { (2, 4) } else { (3, 2) };
                    let dmg = base.saturating_add(c.minion(me.eid).map_or(0, |m| m.counter));
                    let mut foes = c.friendly(1 - me.side);
                    c.g.rng.shuffle(&mut foes);
                    for t in foes.into_iter().take(n) {
                        c.damage(me.eid, t, dmg);
                    }
                    c.resolve_deaths();
                }),
                on_event: Some(|c, me, e| {
                    if let Event::Played { side, played, .. } = e
                        && *side == me.side
                        && played.source != me.eid
                        && played.races.intersects(Races::DRAGON)
                    {
                        c.counter_perm(me.side, me.eid, if me.golden { 2 } else { 3 });
                    }
                }),
                listens: ev::PLAYED,
                // "Start of Combat: Deal 3 damage to two random enemy minions." (golden:
                // "Deal 2 damage to four")
                shown: Some(|_, sh| {
                    let base: i32 = if sh.golden { 2 } else { 3 };
                    vec![base.saturating_add(sh.counter())]
                }),
                flags: flag::IMPROVES,
                ..E
            },
        ),
        // Timewarped Pagle: once per combat, when this attacks and kills a minion, get 1 (2) Triple
        // Reward(s). "Attacks and kills": a kill made during its own attack (not while defending);
        // a Pagle that dies in that attack still gets it.
        (
            "BG34_Giant_208",
            Script {
                on_event: Some(pagle),
                listens: ev::ATTACKS | ev::KILLED,
                ..E
            },
        ),
        // Timewarped Poet: Divine Shield. All your Dragons keep Bonus Keywords and stats
        // (golden: double stats) gained in combat: `Ctx::keep_combat_gain`.
        ("BG34_Giant_314", E),
        // Timewarped Peggy: whenever a card is added to your hand, give your Pirates +1/+1
        // (golden: twice).
        (
            "BG34_Giant_327",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::CardAdded { side, .. } if *side == me.side) {
                        for _ in 0..me.g() {
                            buff_pirates(c, me.side, 1, 1);
                        }
                    }
                }),
                listens: ev::CARD_ADDED,
                ..E
            },
        ),
        // Timewarped Sailor: Divine Shield. The Tavern offers 1 (2) extra Pirate(s) whenever it
        // is Refreshed (up to your Tier, as Shining Sailor).
        (
            "BG34_Giant_589",
            Script {
                on_fill_shop: Some(|c, me| {
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        c.add_shop_minion(me.side, 1, tier, Races::PIRATE);
                    }
                }),
                ..E
            },
        ),
        // Timewarped Collector: also damages adjacent minions. Rally: if you control 4 Golden
        // minions, gain Divine Shield (same golden text). "4" = at least 4, itself included.
        (
            "BG34_Giant_680",
            Script {
                innate: Kw::CLEAVE,
                rally: Some(|c, me, _| {
                    if c.friendly_where(me.side, |m| m.golden).len() >= 4 {
                        c.give_kw(me.eid, Kw::DIVINE_SHIELD);
                    }
                }),
                ..E
            },
        ),
        // Timewarped Duskmaw: Avenge (1): give your Dragons +6/+4 (+12/+8).
        (
            "BG34_PreMadeChamp_020",
            Script {
                avenge: Some((1, 1, |c, me| {
                    for t in c.friendly_where(me.side, |m| m.is(Races::DRAGON)) {
                        c.buff(t, 6 * me.g(), 4 * me.g());
                    }
                })),
                ..E
            },
        ),
        // Timewarped Prismscale: Avenge (2): get 1 (2) Azerite Empowerment(s).
        (
            "BG34_PreMadeChamp_022",
            Script {
                avenge: Some((2, 2, |c, me| {
                    give_n(c, me.side, AZERITE_EMPOWERMENT, me.g() as usize)
                })),
                ..E
            },
        ),
        // Timewarped Raider: after you play a card from Tier 4 or above, give your Pirates
        // +3/+2 (golden: twice). Any card with a Tier counts (minion, Tavern spell), by its
        // printed Tier; playing the Raider itself doesn't.
        (
            "BG34_PreMadeChamp_065",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::CardPlayed { side, card, eid } = e
                        && *side == me.side
                        && *eid != me.eid
                        && db().get(*card).tier >= 4
                    {
                        for _ in 0..me.g() {
                            buff_pirates(c, me.side, 3, 2);
                        }
                    }
                }),
                listens: ev::CARD_PLAYED,
                ..E
            },
        ),
        // Timewarped Plunderer: Deathrattle: increase your maximum Gold by 2 (4).
        (
            "BG34_PreMadeChamp_067",
            Script {
                deathrattle: Some(|c, me| c.add_max_gold(me.side, 2 * me.g())),
                ..E
            },
        ),
        // Timewarped Centurion: after you cast a Tavern spell, get 1 (2) extra copies of it
        // (3 times per turn). The limit is per Centurion (`counter`, reset at the start of the
        // turn) and counts triggers, not copies; casts in combat count toward the same turn.
        (
            "BG34_PreMadeChamp_200",
            Script {
                flags: flag::TURN_USES,
                on_event: Some(|c, me, e| {
                    let Event::SpellCast {
                        side,
                        card,
                        tavern: true,
                        data,
                        ..
                    } = e
                    else {
                        return;
                    };
                    if *side != me.side || c.minion(me.eid).is_none_or(|m| m.counter >= 3) {
                        return;
                    }
                    c.counter_perm(me.side, me.eid, 1);
                    for _ in 0..me.g() {
                        c.add_to_hand(
                            me.side,
                            Item::Spell {
                                card: *card,
                                data: *data,
                            },
                        );
                    }
                }),
                listens: ev::SPELL_CAST,
                start_of_turn: Some(|c, me| {
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = 0;
                    }
                }),
                // A fresh golden starts with a full allowance.
                ..E
            },
        ),
    ]
}

/// The right-most living friendly minion that has hook `h`.
fn rightmost_with(c: &Ctx, side: Side, h: Hook) -> Option<EntityId> {
    c.boards[side]
        .iter()
        .rev()
        .find(|m| m.alive() && minion_has(m, h))
        .map(|m| m.eid)
}

fn buff_pirates(c: &mut Ctx, side: Side, atk: i32, hp: i32) {
    for t in c.friendly_where(side, |m| m.is(Races::PIRATE)) {
        c.buff(t, atk, hp);
    }
}

fn pagle(c: &mut Ctx, me: Me, e: &Event) {
    if !c.combat {
        return;
    }
    match e {
        // Its own attack opens the window; any other attack closes it.
        Event::Attacks { attacker, .. } => {
            if let Some(m) = c.minion_mut(me.eid) {
                if *attacker == me.eid {
                    m.counter |= PAGLE_ATTACKING;
                } else {
                    m.counter &= !PAGLE_ATTACKING;
                }
            }
        }
        Event::Killed { side, killer, .. } if *side == me.side && *killer == me.eid => {
            // (A Pagle that died in the exchange is read from its death record.)
            let counter = match c.minion(me.eid) {
                Some(m) => m.counter,
                None => c.dead_record(me.side, me.eid).map_or(0, |d| d.counter),
            };
            if counter & PAGLE_ATTACKING == 0 || counter & PAGLE_USED != 0 {
                return;
            }
            if let Some(m) = c.minion_mut(me.eid) {
                m.counter |= PAGLE_USED;
            }
            for _ in 0..me.g() {
                give_triple_reward(c, me.side);
            }
        }
        _ => {}
    }
}
