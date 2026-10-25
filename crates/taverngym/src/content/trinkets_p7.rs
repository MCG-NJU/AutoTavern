//! Trinkets: keywords, Magnetic and Deity trinkets. Designer Eyepatch is in `Ctx::triple_need`;
//! Evil Experiment in `deity_awakened`.

use super::*;

// The client's Satellite; its stats are always set.
const SATELLITE: &str = "BG31_171t";
const FROSTLING: &str = "BG26_537";

fn kw_random(c: &mut Ctx, side: Side, r: Races, n: usize, kw: Kw) {
    let mut v = c.friendly_where(side, |m| m.is(r));
    c.g.rng.shuffle(&mut v);
    for e in v.into_iter().take(n) {
        c.give_kw(e, kw);
    }
}

fn satellite(c: &mut Ctx, side: Side, a: i32, h: i32) {
    let mut m = c.new_minion(side, cid(SATELLITE));
    m.set_stats(a, h);
    c.add_to_hand(side, Item::Minion(m));
}

fn beetles(c: &mut Ctx, side: Side, n: usize) {
    for _ in 0..n {
        let mut b = c.new_minion(side, cid(BEETLE));
        b.give(Kw::TAUNT);
        let pos = c.boards[side].len();
        c.summon(side, pos, b);
    }
}

/// Transform a board minion in place into `card` (old copies return; the new minion takes a pool
/// copy when one is left).
fn transform(c: &mut Ctx, side: Side, eid: EntityId, card: CardId) {
    let Some((s, i)) = c.locate(eid) else { return };
    let old = c.boards[s][i].clone();
    if !c.combat {
        c.g.pool.return_minion(&old);
    }
    let mut m = c.new_minion(side, card);
    m.eid = old.eid;
    if !c.combat {
        c.take_pool_copy(&mut m);
    }
    c.boards[s][i] = m;
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Staff of the Scourge: Avenge (5): give a random friendly Undead Reborn.
        (
            "BG30_MagicItem_437",
            Script {
                // Not an Undead that already has Reborn.
                avenge: Some((5, 5, |c, me| {
                    if let Some(t) =
                        c.random_friendly(me.side, |m| m.is(Races::UNDEAD) && !m.has(Kw::REBORN))
                    {
                        c.give_kw(t, Kw::REBORN);
                    }
                })),
                ..E
            },
        ),
        // Designer Eyepatch: Pirates triple with 2 copies (`triple_need`); pairs you already
        // hold merge now.
        (
            "BG30_MagicItem_439",
            Script {
                on_attach: Some(|c, me| c.check_triples(me.side)),
                ..E
            },
        ),
        // Jarred Frostling: SoC give 3 random friendly Elementals "Deathrattle: summon a
        // Flourishing Frostling" (the trinket is attached as that enchantment).
        (
            "BG30_MagicItem_952",
            Script {
                start_of_combat: Some(|c, me| {
                    if me.eid != 0 {
                        return;
                    }
                    let mut v = c.friendly_where(me.side, |m| m.is(Races::ELEMENTAL));
                    c.g.rng.shuffle(&mut v);
                    for e in v.into_iter().take(3) {
                        if let Some(m) = c.minion_mut(e) {
                            m.attached.push(me.card);
                        }
                    }
                }),
                deathrattle: Some(|c, me| {
                    let m = c.new_minion(me.side, cid(FROSTLING));
                    c.summon(me.side, me.pos, m);
                }),
                ..E
            },
        ),
        // Reusable Batteries: the first minion you buy each turn gives a Magnetic Satellite
        // with its stats.
        (
            "BG32_MagicItem_278",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Bought {
                        side, card, eid, ..
                    } = e
                    else {
                        return;
                    };
                    if *side != me.side
                        || db().get(*card).kind != crate::cards::CardKind::Minion
                        || c.turn_data(me.side, me.card) > 0
                    {
                        return;
                    }
                    c.add_turn_data(me.side, me.card, 1);
                    // A purchase that completed a triple no longer exists: no Satellite.
                    let Some((a, h)) = c.find_any(*eid).map(|m| (m.atk, m.max_hp)) else {
                        return;
                    };
                    satellite(c, me.side, a, h);
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Baleful Incense: SoC your left- and right-most Undead get Reborn.
        (
            "BG32_MagicItem_360",
            Script {
                start_of_combat: Some(|c, me| {
                    let v = c.friendly_where(me.side, |m| m.is(Races::UNDEAD));
                    let mut ends: Vec<EntityId> =
                        v.first().into_iter().chain(v.last()).copied().collect();
                    ends.dedup();
                    for e in ends {
                        c.give_kw(e, Kw::REBORN);
                    }
                }),
                ..E
            },
        ),
        // Faerie Dragon Scale: whenever a friendly Dragon attacks, give it Divine Shield
        // (3 times per combat).
        (
            "BG32_MagicItem_363",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Attacks { side, attacker, .. } = e
                        && *side == me.side
                        && c.combat
                        && c.combat_data(me.side, me.card) < 3
                        && c.minion(*attacker).is_some_and(|m| m.is(Races::DRAGON))
                    {
                        c.add_combat_data(me.side, me.card, 1);
                        c.give_kw(*attacker, Kw::DIVINE_SHIELD);
                    }
                }),
                listens: ev::ATTACKS,
                ..E
            },
        ),
        // Shrine of Evolution: transform all your minions into random Tier 4 minions.
        (
            "BG32_MagicItem_400",
            Script {
                on_attach: Some(|c, me| {
                    for e in c.friendly(me.side) {
                        if let Some(x) = c.random_minion(|d| d.tier == 4) {
                            transform(c, me.side, e, x);
                        }
                    }
                }),
                ..E
            },
        ),
        // Beetle Band: Avenge (5) (greater: 7): summon one (two) 2/2 Taunt Beetle(s).
        (
            "BG32_MagicItem_860",
            Script {
                avenge: Some((5, 5, |c, me| beetles(c, me.side, 1))),
                ..E
            },
        ),
        (
            "BG32_MagicItem_860t",
            Script {
                avenge: Some((7, 7, |c, me| beetles(c, me.side, 2))),
                ..E
            },
        ),
        // Gold Pendant: make a random friendly minion from Tier 4 or below Golden; again each
        // turn.
        (
            "BG32_MagicItem_951",
            Script {
                on_attach: Some(gold_pendant),
                start_of_turn: Some(gold_pendant),
                ..E
            },
        ),
        // Copper Coil: whenever you Magnetize a minion, give it +2/+2 and improve this.
        (
            "BG35_MagicItem_300t",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Magnetized { side, host, .. } = e
                        && *side == me.side
                    {
                        let k = 2 * (1 + c.card_level(me.side, me.card));
                        c.buff(*host, k, k);
                        c.improve_card(me.side, me.card);
                    }
                }),
                listens: ev::MAGNETIZED,
                // "Whenever you Magnetize a minion, give it +2/+2 and improve this."
                shown: Some(|c, sh| {
                    let k = 2 * (1 + c.card_level(sh.side, sh.card));
                    vec![k, k]
                }),
                ..E
            },
        ),
        // Protective Ring: SoC give 4 random friendly Pirates Divine Shield.
        (
            "BG35_MagicItem_711",
            Script {
                start_of_combat: Some(|c, me| {
                    kw_random(c, me.side, Races::PIRATE, 4, Kw::DIVINE_SHIELD)
                }),
                ..E
            },
        ),
        // Lightfeather Sticker: SoC your Rally minions get Divine Shield.
        (
            "BG36_MagicItem_213",
            Script {
                start_of_combat: Some(|c, me| {
                    let v = c.friendly_where(me.side, |m| {
                        crate::script::minion_has(m, crate::script::Hook::Rally)
                    });
                    for e in v {
                        c.give_kw(e, Kw::DIVINE_SHIELD);
                    }
                }),
                ..E
            },
        ),
        // Corrupted Baton: after you cast a Tavern spell, your Deity +4/+4 (+10/+10).
        (
            "BG36_MagicItem_404",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, tavern: true, .. } if *side == me.side)
                    {
                        buff_deity(c, me.side, 4, 4);
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        (
            "BG36_MagicItem_404t",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, tavern: true, .. } if *side == me.side)
                    {
                        buff_deity(c, me.side, 10, 10);
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Evil Experiment: `deity_awakened`.
        ("BG36_MagicItem_416", E),
        // Mask of Ancient Ones: your Deity is Golden this game (its golden base stats replace
        // the normal ones; gains are kept).
        (
            "BG36_MagicItem_602",
            Script {
                on_attach: Some(|c, me| {
                    if let Some(p) = c.player(me.side)
                        && let Some(d) = p.deity.as_mut()
                        && !d.golden
                    {
                        let base = db().get(d.card);
                        let gold = db().get(db().golden_of(d.card));
                        d.atk = d.atk.saturating_add(gold.atk - base.atk);
                        d.hp = d.hp.saturating_add(gold.hp - base.hp);
                        d.golden = true;
                    }
                }),
                offer_ok: Some(|g, p| g.players[p].deity.as_ref().is_some_and(|d| !d.golden)),
                ..E
            },
        ),
        // Inductive Gyroblade: at the end of your turn, get a 4/4 (12/12) Magnetic Satellite,
        // improved by each spell you cast this turn.
        (
            "BG36_MagicItem_810",
            Script {
                end_of_turn: Some(|c, me| gyroblade(c, me, 4)),
                // "At the end of your turn, get a 4/4 Magnetic Satellite. (Improved by each
                // Tavern spell you've cast this turn!)"
                shown: Some(|c, sh| shown_gyroblade(c, sh, 4)),
                ..E
            },
        ),
        (
            "BG36_MagicItem_810t",
            Script {
                end_of_turn: Some(|c, me| gyroblade(c, me, 12)),
                // "At the end of your turn, get a 12/12 Magnetic Satellite. (Improved by each
                // Tavern spell you've cast this turn!)"
                shown: Some(|c, sh| shown_gyroblade(c, sh, 12)),
                ..E
            },
        ),
    ]
}

fn gold_pendant(c: &mut Ctx, me: Me) {
    let v = c.friendly_where(me.side, |m| {
        !m.golden && m.tier <= 4 && db().get(m.card).golden.is_some()
    });
    if let Some(&e) = c.g.rng.pick(&v)
        && let Some(m) = c.minion_mut(e)
    {
        super::spells::make_golden(m);
    }
}

/// Inductive Gyroblade: improved by each Tavern spell cast this turn.
fn gyroblade(c: &mut Ctx, me: Me, base: i32) {
    let k = 1 + c
        .player_ref(me.side)
        .map_or(0, |p| p.stats.tavern_spells_cast_turn);
    satellite(c, me.side, base * k, base * k);
}

/// The Satellite's stats if the turn ended now (the formula of `gyroblade`).
fn shown_gyroblade(c: &Ctx, sh: &crate::script::Shown, base: i32) -> Vec<i32> {
    let k = 1 + c
        .player_ref(sh.side)
        .map_or(0, |p| p.stats.tavern_spells_cast_turn);
    vec![base * k, base * k]
}
