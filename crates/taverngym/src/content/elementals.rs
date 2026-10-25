//! Elementals (phase 1).

use super::*;

fn tavern_elementals(c: &mut Ctx, side: Side, a: i32, h: i32) {
    if let Some(p) = c.player(side) {
        p.stats.tavern_elemental_atk = p.stats.tavern_elemental_atk.saturating_add(a);
        p.stats.tavern_elemental_hp = p.stats.tavern_elemental_hp.saturating_add(h);
    }
    c.buff_tavern(side, a, h, Races::ELEMENTAL);
}

fn highest_tavern_stats(c: &mut Ctx, side: Side) -> Option<(i32, i32)> {
    let sid = c.highest_health_tavern(side)?;
    let p = c.owner[side]?;
    c.g.players[p]
        .shop
        .iter()
        .find(|s| s.sid == sid)
        .and_then(|s| s.item.as_minion())
        .map(|m| (m.atk, m.hp))
}

fn played_elemental(_c: &Ctx, me: Me, e: &Event) -> Option<EntityId> {
    match e {
        Event::Played { side, eid, played }
            if *side == me.side
                && played.source != me.eid
                && played.races.intersects(Races::ELEMENTAL) =>
        {
            Some(*eid)
        }
        _ => None,
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Wildfire Elemental: excess damage on kill (combat.rs `do_attack`).
        ("BGS_126", E),
        // Vine Climber: the current play contributes to the turn's improvement.
        (
            "BG33_334",
            Script {
                on_event: Some(|c, me, e| {
                    if let Some(t) = played_elemental(c, me, e) {
                        let k = me.g()
                            * (1 + c
                                .player_ref(me.side)
                                .map_or(0, |p| p.stats.minions_played_turn));
                        c.buff(t, 2 * k, k);
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Ichoron: whenever you play an Elemental, give it Divine Shield until next turn
        // (golden: permanently).
        (
            "BG31_812",
            Script {
                on_event: Some(|c, me, e| {
                    if let Some(t) = played_elemental(c, me, e) {
                        if me.golden {
                            c.give_kw(t, Kw::DIVINE_SHIELD);
                        } else {
                            c.give_kw_temp(t, Kw::DIVINE_SHIELD);
                        }
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Leyline Surfacer: Battlecry and Deathrattle: get 1 (2) Arcane Absorptions.
        (
            "BG35_881",
            Script {
                battlecry: Some(|c, me, _| give_n(c, me.side, "BG35_911", me.g() as usize)),
                deathrattle: Some(|c, me| give_n(c, me.side, "BG35_911", me.g() as usize)),
                ..E
            },
        ),
        // Arcane Absorption: give a friendly Elemental half the stats of the highest-Health
        // minion in the Tavern (rounded down).
        (
            "BG35_911",
            Script {
                spell: Some(|c, me, t| {
                    if let Some((a, h)) = highest_tavern_stats(c, me.side) {
                        spell_buff(c, me, t, a / 2, h / 2);
                    }
                }),
                spell_target: TargetSpec::FriendlyRace(Races::ELEMENTAL),
                ..E
            },
        ),
        // Living Prison: Activate (1): gain the stats (double) of the next minion you buy this
        // turn (counter = armed).
        (
            "BG36_180",
            Script {
                activate: Some((1, TargetSpec::None, |c, me, _| {
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter += 1;
                    }
                })),
                on_event: Some(|c, me, e| {
                    let Event::Bought { side, eid, .. } = e else {
                        return;
                    };
                    if *side != me.side {
                        return;
                    }
                    let Some(armed) = c.minion(me.eid).map(|m| m.counter).filter(|n| *n > 0) else {
                        return;
                    };
                    // The bought card is in hand now (`eid` = its hand id).
                    let Some((a, h)) = c
                        .player_ref(me.side)
                        .and_then(|p| p.hand.iter().find(|h| h.hid == *eid))
                        .and_then(|h| h.item.as_minion())
                        .map(|m| (m.atk, m.hp))
                    else {
                        return;
                    };
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = 0;
                    }
                    c.buff(
                        me.eid,
                        a.saturating_mul(me.g()).saturating_mul(armed),
                        h.saturating_mul(me.g()).saturating_mul(armed),
                    );
                }),
                listens: ev::BOUGHT,
                start_of_turn: Some(|c, me| {
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = 0;
                    }
                }),
                ..E
            },
        ),
        // Air Baller: when sold, give your minions +2/+2 (+4/+4); improve future Ballers.
        (
            "BG36_181",
            Script {
                on_sell: Some(|c, me| {
                    let bonus = c.player_ref(me.side).map_or(0, |p| p.stats.baller_bonus);
                    let v = 2 * me.g() * (1 + bonus);
                    for e in c.friendly(me.side) {
                        if e != me.eid {
                            c.buff(e, v, v);
                        }
                    }
                    if let Some(p) = c.player(me.side) {
                        p.stats.baller_bonus = p.stats.baller_bonus.saturating_add(1);
                    }
                }),
                // "When you sell this, give your minions +2/+2. Improve your future Ballers."
                shown: Some(|c, sh| {
                    let bonus = c.player_ref(sh.side).map_or(0, |p| p.stats.baller_bonus);
                    let v = 2 * sh.g() * (1 + bonus);
                    vec![v, v]
                }),
                ..E
            },
        ),
        // Flourishing Frostling: +2/+1 (+4/+2) per Elemental played (`apply_bonuses`).
        ("BG26_537", E),
        // Living Azerite: whenever you cast a Tavern spell, Elementals in the Tavern +4/+3 this
        // game (twice).
        (
            "BG28_707",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, tavern: true, .. } if *side == me.side)
                    {
                        for _ in 0..me.g() {
                            tavern_elementals(c, me.side, 4, 3);
                            // Azerite Portrait: friendly Elementals get the stats too.
                            if has_trinket(c, me.side, "BG30_MagicItem_431") {
                                for t in c.friendly_where(me.side, |m| m.is(Races::ELEMENTAL)) {
                                    c.buff(t, 4, 3);
                                }
                            }
                        }
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Air Revenant: after you spend 7 Gold, cast Easterly Winds (two).
        (
            "BG34_858",
            Script {
                on_event: Some(|c, me, e| {
                    gold_counter(c, me, e, 7, |c, me| {
                        for _ in 0..me.g() {
                            c.you_cast(me.side, cid("BG34_444"), 0, 0, 0);
                        }
                    })
                }),
                listens: ev::GOLD_SPENT,
                // "After you spend 7 Gold, cast Easterly Winds. (7 left!)"
                shown: Some(|_, sh| vec![7, (7 - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Firelands Fugitive: Battlecry: get 1 (2) Conflagrations.
        (
            "BG35_882",
            Script {
                battlecry: Some(|c, me, _| give_n(c, me.side, "BG35_910", me.g() as usize)),
                ..E
            },
        ),
        // Conflagration: give a minion +4/+4, improved (by the base amount) for each Elemental
        // you played this turn.
        (
            "BG35_910",
            Script {
                spell: Some(|c, me, t| {
                    let n = c
                        .player_ref(me.side)
                        .map_or(0, |p| p.stats.elementals_played_turn);
                    let k = 4 * (1 + n);
                    spell_buff(c, me, t, k, k);
                }),
                spell_target: TargetSpec::Friendly,
                // "Give a minion +4/+4. (Improved by each Elemental you played this turn!)"
                shown: Some(|c, sh| {
                    let n = c
                        .player_ref(sh.side)
                        .map_or(0, |p| p.stats.elementals_played_turn);
                    let k = 4 * (1 + n);
                    shown_spell_stats(c, sh, k, k)
                }),
                ..E
            },
        ),
        // Elemental of Surprise: triples with any Elemental (`check_triples`).
        ("BG26_175", E),
        // Ultraviolet Ascendant: Start of Combat: your other Elementals +3/+3 (+6/+6); improves
        // (by the base amount) after you play an Elemental.
        (
            "BG31_810",
            Script {
                start_of_combat: Some(|c, me| {
                    let k = me
                        .g()
                        .saturating_add(c.minion(me.eid).map_or(0, |m| m.counter));
                    for t in
                        c.friendly_where(me.side, |m| m.is(Races::ELEMENTAL) && m.eid != me.eid)
                    {
                        c.buff(t, 3 * k, 3 * k);
                    }
                }),
                on_event: Some(|c, me, e| {
                    if played_elemental(c, me, e).is_some()
                        && let Some(m) = c.minion_mut(me.eid)
                    {
                        m.counter += me.g();
                    }
                }),
                listens: ev::PLAYED,
                flags: flag::IMPROVES,
                // "Start of Combat: Give your other Elementals +3/+3. (Improves after you play
                // an Elemental!)"
                shown: Some(|_, sh| {
                    let k = 3 * (sh.g().saturating_add(sh.counter()));
                    vec![k, k]
                }),
                ..E
            },
        ),
        // Unbound Tempest: after you play 4 Elementals, gain the stats (double) of the
        // highest-Health minion in the Tavern.
        (
            "BG36_352",
            Script {
                on_event: Some(|c, me, e| {
                    if played_elemental(c, me, e).is_none() {
                        return;
                    }
                    let Some(m) = c.minion_mut(me.eid) else {
                        return;
                    };
                    m.counter += 1;
                    if m.counter < 4 {
                        return;
                    }
                    m.counter = 0;
                    if let Some((a, h)) = highest_tavern_stats(c, me.side) {
                        c.buff(me.eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                    }
                }),
                listens: ev::PLAYED,
                // "After you play 4 Elementals, gain the stats of the highest-Health minion in
                // the Tavern. (4 left!)"
                shown: Some(|_, sh| vec![4, (4 - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Arcane Behemoth (only from Behemoth Portrait): Taunt. After you sell an Elemental,
        // gain its stats (golden: double). The sold minion's card types decide, as for the
        // other "after you sell a <type>" effects.
        (
            "BG31_360",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Sold {
                        side,
                        card,
                        atk,
                        hp,
                        ..
                    } = e
                        && *side == me.side
                        && db().get(*card).races.intersects(Races::ELEMENTAL)
                    {
                        c.buff(
                            me.eid,
                            (*atk).max(0).saturating_mul(me.g()),
                            (*hp).max(0).saturating_mul(me.g()),
                        );
                    }
                }),
                listens: ev::SOLD,
                ..E
            },
        ),
    ]
}
