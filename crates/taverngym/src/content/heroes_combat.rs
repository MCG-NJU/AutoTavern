//! Heroes: Start of Combat powers and starting tokens.

use super::*;

const ELEMENTS: [&str; 4] = [
    "BG22_HERO_001p_t1_s",
    "BG22_HERO_001p_t2_s",
    "BG22_HERO_001p_t3_s",
    "BG22_HERO_001p_t4_s",
];

fn passive(s: Script) -> Script {
    Script {
        flags: s.flags | flag::PASSIVE,
        hero_power_uses: 0,
        ..s
    }
}

/// "Start the game with <token>": summon it on the board.
fn start_with(c: &mut Ctx, side: Side, token: &str) {
    let m = c.new_minion(side, cid(token));
    let pos = c.boards[side].len();
    c.summon(side, pos, m);
}

fn random_enemies(c: &mut Ctx, side: Side) -> Vec<EntityId> {
    c.boards[1 - side]
        .iter()
        .filter(|m| m.alive())
        .map(|m| m.eid)
        .collect()
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Tamsin Roame: Start of Combat: give your lowest-Attack minion "Deathrattle: give
        // your other minions this minion's stats".
        (
            "BG20_HERO_282p",
            passive(Script {
                start_of_combat: Some(|c, me| {
                    let keyed: Vec<(EntityId, i64)> = c.boards[me.side]
                        .iter()
                        .filter(|m| m.alive())
                        .map(|m| (m.eid, -(m.atk as i64)))
                        .collect();
                    let t = c.pick_max(keyed);
                    if let Some(t) = t
                        && let Some(m) = c.minion_mut(t)
                    {
                        m.gained_dr.push(me.card);
                    }
                }),
                deathrattle: Some(|c, me| {
                    let Some((a, h)) = c.deathrattle_stats(me.side, me.eid) else {
                        return;
                    };
                    for t in c.friendly_where(me.side, |m| m.eid != me.eid) {
                        c.buff(t, a, h);
                    }
                }),
                ..E
            }),
        ),
        // Tavish Stormpike: remove a minion in the Tavern; next combat, when you have space
        // (after Deathrattles / Reborn), summon it and fire it at a random enemy minion. It is
        // owed until it gets summoned.
        (
            "BG22_HERO_000p_Alt",
            Script {
                hero_power: Some(|c, me, t| {
                    let Some(mut m) = c.take_from_shop(me.side, t) else {
                        return;
                    };
                    c.g.pool.return_minion(&m);
                    m.pool_copies = 0;
                    // Crabby: get a plain copy (golden: 2) of the removed minion.
                    for _ in 0..buddy_power(c, me.side, "BG22_HERO_000_Buddy") {
                        c.give_card(me.side, db().normal_of(m.card));
                    }
                    if let Some(p) = c.player(me.side) {
                        p.owed.push((me.card, m, true));
                    }
                }),
                hero_power_target: TargetSpec::Tavern,
                start_of_combat: Some(|c, me| {
                    let owed: Vec<Minion> = c
                        .player_ref(me.side)
                        .map(|p| {
                            p.owed
                                .iter()
                                .filter(|x| x.0 == me.card)
                                .map(|x| x.1.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    for mut m in owed {
                        m.combat_only = true;
                        c.when_space(me.side, crate::engine::SpaceReq::Owed(m, true));
                    }
                }),
                ..E
            },
        ),
        // Bru'kan: choose an Element; Start of Combat: call upon it. The choice is used up by the
        // next combat even when the Element finds nothing to act on.
        (
            "BG22_HERO_001p",
            Script {
                hero_power: Some(|c, me, _| {
                    let options = ELEMENTS.iter().map(|e| Item::spell(cid(e))).collect();
                    c.offer_discover(me.side, me.card, options);
                }),
                start_of_combat: Some(|c, me| {
                    let k = c.power_mut(me.side, me.card).map_or(0, |h| h.counter);
                    if (1..=4).contains(&k) {
                        c.cast_spell(me.side, cid(ELEMENTS[k as usize - 1]), 0, 0, 0);
                        // Spirit Raptors remember the Element (also on the warband minion).
                        let raptor = cid("BG22_HERO_001_Buddy");
                        let bit = 1 << (k - 1);
                        let mark = |m: &mut Minion| {
                            if db().normal_of(m.card) == raptor {
                                m.counter |= bit;
                            }
                        };
                        c.boards[me.side].iter_mut().for_each(mark);
                        if let Some(p) = c.owner[me.side] {
                            c.g.players[p].board.iter_mut().for_each(mark);
                        }
                        if let Some(h) = c.power_mut(me.side, me.card) {
                            h.counter = 0;
                        }
                    }
                }),
                ..E
            },
        ),
        // Earth: 4 random friendly minions get "Deathrattle: summon a 1/1 Elemental".
        (
            ELEMENTS[0],
            Script {
                spell: Some(|c, me, _| {
                    let mut f = c.friendly(me.side);
                    c.g.rng.shuffle(&mut f);
                    for t in f.into_iter().take(4) {
                        if let Some(m) = c.minion_mut(t) {
                            m.gained_dr.push(me.card);
                        }
                    }
                }),
                deathrattle: Some(|c, me| summon_n(c, me, "BG22_HERO_001p_t1et", 1, false)),
                ..E
            },
        ),
        // Fire: double your left-most minion's Attack.
        (
            ELEMENTS[1],
            Script {
                spell: Some(|c, me, _| {
                    if let Some(t) = c.leftmost(me.side)
                        && let Some(a) = c.minion(t).map(|m| m.atk)
                    {
                        c.buff(t, a, 0);
                    }
                }),
                ..E
            },
        ),
        // Water: your right-most minion gets Divine Shield and Taunt.
        (
            ELEMENTS[2],
            Script {
                spell: Some(|c, me, _| {
                    if let Some(t) = c.rightmost(me.side) {
                        c.give_kw(t, Kw::DIVINE_SHIELD);
                        c.give_kw(t, Kw::TAUNT);
                    }
                }),
                ..E
            },
        ),
        // Lightning: deal 1 damage to 5 random enemy minions.
        (
            ELEMENTS[3],
            Script {
                spell: Some(|c, me, _| {
                    let mut foes = random_enemies(c, me.side);
                    c.g.rng.shuffle(&mut foes);
                    for t in foes.into_iter().take(5) {
                        c.damage(0, t, 1);
                    }
                    c.resolve_deaths();
                }),
                ..E
            },
        ),
        // Onyxia: Avenge (4): summon a 1/1 Whelp that attacks immediately; improve this by
        // +1/+1 (counter = friendly deaths this combat, level = improvements).
        (
            "BG22_HERO_305p",
            passive(Script {
                start_of_combat: Some(|c, me| {
                    if let Some(h) = c.power_mut(me.side, me.card) {
                        h.counter = 0;
                    }
                }),
                on_event: Some(|c, me, e| {
                    if !c.combat || !matches!(e, Event::Died { side, .. } if *side == me.side) {
                        return;
                    }
                    let Some(h) = c.power_mut(me.side, me.card) else {
                        return;
                    };
                    h.counter += 1;
                    if h.counter % 4 != 0 {
                        return;
                    }
                    let k = h.level.saturating_add(1);
                    h.level = h.level.saturating_add(1);
                    let mut w = c.new_minion(me.side, cid("BG22_HERO_305t"));
                    w.set_stats(k, k);
                    c.when_space(me.side, crate::engine::SpaceReq::AttackNow(w));
                }),
                listens: ev::DIED,
                // "Avenge (4): Summon a 1/1 Whelp that attacks immediately. Improve this by
                // +1/+1."
                shown: Some(|c, sh| {
                    let k = 1 + c
                        .player_ref(sh.side)
                        .and_then(|p| p.power_of(sh.card))
                        .map_or(0, |h| h.level);
                    vec![4, k, k, 1, 1]
                }),
                ..E
            }),
        ),
        // Teron Gorefiend: choose a friendly minion; Start of Combat: destroy it, then resummon
        // an exact copy when you have space.
        (
            "BG25_HERO_103p",
            Script {
                hero_power: Some(|c, me, t| {
                    if let Some(h) = c.power_mut(me.side, me.card) {
                        h.counter = t as i32;
                    }
                }),
                hero_power_target: TargetSpec::Friendly,
                start_of_combat: Some(|c, me| {
                    let t = c
                        .power_mut(me.side, me.card)
                        .map_or(0, |h| std::mem::take(&mut h.counter))
                        as EntityId;
                    let Some(mut copy) = c.minion(t).filter(|m| m.alive()).cloned() else {
                        return;
                    };
                    // The copy comes back where the minion stood, not at the far right.
                    let (index, right) = c.locate(t).map_or((usize::MAX, 0), |(s, i)| {
                        (i, c.boards[s].get(i + 1).map_or(0, |m| m.eid))
                    });
                    c.destroy(t);
                    c.resolve_deaths();
                    copy.eid = c.g.alloc_eid();
                    copy.combat_only = true;
                    copy.pool_copies = 0;
                    c.when_space(
                        me.side,
                        crate::engine::SpaceReq::InPlace(copy, right, index),
                    );
                }),
                ..E
            },
        ),
        // Y'Shaarj: Start of Combat: summon and get a minion of your Tier (armed by the power).
        (
            "TB_BaconShop_HP_103",
            Script {
                hero_power: Some(|c, me, _| {
                    if let Some(h) = c.power_mut(me.side, me.card) {
                        h.counter = 1;
                    }
                }),
                start_of_combat: Some(|c, me| {
                    let armed = c
                        .power_mut(me.side, me.card)
                        .map_or(0, |h| std::mem::take(&mut h.counter));
                    if armed == 0 {
                        return;
                    }
                    let tier = c.tier(me.side);
                    if let Some(card) = c.random_minion(|d| d.tier == tier) {
                        let m = c.new_minion(me.side, card);
                        let pos = c.boards[me.side].len();
                        c.summon(me.side, pos, m);
                        c.give_card(me.side, card);
                    }
                }),
                ..E
            },
        ),
        // Arch-Villain Rafaam: next combat, get a plain copy of the first minion you kill.
        (
            "TB_BaconShop_HP_053",
            Script {
                hero_power: Some(|c, me, _| {
                    if let Some(h) = c.power_mut(me.side, me.card) {
                        h.counter = 1;
                    }
                }),
                // "Next combat" only: an unused arming doesn't carry into later combats.
                start_of_turn: Some(|c, me| {
                    if let Some(h) = c.power_mut(me.side, me.card) {
                        h.counter = 0;
                    }
                }),
                on_event: Some(|c, me, e| {
                    let Event::Killed { side, victim, .. } = e else {
                        return;
                    };
                    // A friendly minion killed it (it may have died too).
                    if !c.combat || *side != me.side {
                        return;
                    }
                    if c.power_mut(me.side, me.card).is_none_or(|h| h.counter == 0) {
                        return;
                    }
                    let Some(card) = c
                        .dead_record(1 - me.side, *victim)
                        .map(|d| d.card)
                        .or_else(|| c.minion(*victim).map(|m| m.card))
                    else {
                        return;
                    };
                    if let Some(h) = c.power_mut(me.side, me.card) {
                        h.counter = 0;
                    }
                    c.give_card(me.side, db().normal_of(card));
                }),
                listens: ev::KILLED,
                ..E
            },
        ),
        // N'Zoth: start the game with a 2/2 Fish that gains your Deathrattles in combat.
        (
            "TB_BaconShop_HP_105",
            passive(Script {
                on_attach: Some(|c, me| start_with(c, me.side, "TB_BaconShop_HP_105t")),
                ..E
            }),
        ),
        // Fish of N'Zoth: after a different friendly Deathrattle minion dies in combat, gain its
        // Deathrattle (golden: twice). Like everything gained in combat, it is gone after the
        // combat.
        (
            "TB_BaconShop_HP_105t",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Died { side, eid, .. } = e else {
                        return;
                    };
                    if !c.combat || *side != me.side || *eid == me.eid {
                        return;
                    }
                    let deathrattles = c
                        .dead_record(me.side, *eid)
                        .map(|d| d.deathrattle_cards.clone())
                        .unwrap_or_default();
                    for _ in 0..me.g() {
                        if let Some(m) = c.minion_mut(me.eid) {
                            m.gained_dr.extend(deathrattles.iter().copied());
                        }
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Sneed: start the game with a 2/1 Shredder.
        (
            "BG21_HERO_030p",
            passive(Script {
                on_attach: Some(|c, me| start_with(c, me.side, "BG21_HERO_030t")),
                ..E
            }),
        ),
        // Sneed's New Shredder: Deathrattle: summon the 1 (2) highest-Health minions from your
        // hand (for this combat) with Divine Shield.
        (
            "BG21_HERO_030t",
            Script {
                deathrattle: Some(|c, me| {
                    if !c.combat {
                        return;
                    }
                    let Some(p) = c.owner[me.side] else { return };
                    let mut hand: Vec<(i32, EntityId)> = c.g.players[p]
                        .hand
                        .iter()
                        .filter(|h| h.locked == 0 && !c.cs[me.side].from_hand.contains(&h.hid))
                        .filter_map(|h| h.item.as_minion().map(|m| (m.hp, h.hid)))
                        .collect();
                    c.g.rng.shuffle(&mut hand);
                    hand.sort_by_key(|x| std::cmp::Reverse(x.0));
                    for (i, (_, hid)) in hand.into_iter().take(me.g() as usize).enumerate() {
                        if let Some(e) = c.summon_from_hand(me.side, hid, me.pos + i) {
                            c.give_kw(e, Kw::DIVINE_SHIELD);
                        }
                    }
                }),
                ..E
            },
        ),
        // The Curator: start the game with a 2/2 Venomous Amalgam with all minion types.
        (
            "TB_BaconShop_HP_033",
            passive(Script {
                on_attach: Some(|c, me| start_with(c, me.side, "TB_BaconShop_HP_033t")),
                ..E
            }),
        ),
        (
            "TB_BaconShop_HP_033t",
            Script {
                on_create: Some(|m| m.races = Races::ALL),
                ..E
            },
        ),
    ]
}
