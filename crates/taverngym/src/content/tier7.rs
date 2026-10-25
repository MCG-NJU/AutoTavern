//! Tier 7 minions (special routes).

use super::*;

/// "Give minions in the Tavern +a/+h this game."
fn tavern_this_game(c: &mut Ctx, side: Side, a: i32, h: i32) {
    if let Some(p) = c.player(side) {
        p.stats.tavern_atk = p.stats.tavern_atk.saturating_add(a);
        p.stats.tavern_hp = p.stats.tavern_hp.saturating_add(h);
    }
    c.buff_tavern(side, a, h, Races::NONE);
}

fn tier6(c: &mut Ctx, me: Me) {
    for _ in 0..me.g() {
        if let Some(m) = c.random_minion(|d| d.tier == 6) {
            c.give_card(me.side, m);
        }
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Captain Sanders: Battlecry: make a friendly minion from Tier 6 or below Golden. A
        // targeted Battlecry (hsdata TARGETING_ARROW_TEXT); the golden's second minion is random
        // (an assumption), as is a target-less trigger.
        (
            "BG25_034",
            Script {
                battlecry: Some(|c, me, t| {
                    let eligible = |m: &crate::state::Minion| {
                        m.eid != me.eid
                            && !m.golden
                            && m.tier <= 6
                            && db().get(m.card).golden.is_some()
                    };
                    // The chosen target takes its place also when it is no longer eligible: Brann's
                    // repeat of the Battlecry finds it Golden already and makes no other minion
                    // Golden (Elite Navigator and Captain Sanders with Brann); only the Golden
                    // card's second minion is random.
                    let mut picks = vec![];
                    if t != 0 && c.minion(t).is_some_and(eligible) {
                        picks.push(t);
                    }
                    let mut rest = c.friendly_where(me.side, |m| eligible(m) && m.eid != t);
                    c.g.rng.shuffle(&mut rest);
                    let n = (me.g() as usize).saturating_sub(usize::from(t != 0));
                    picks.extend(rest.into_iter().take(n));
                    for t in picks {
                        if let Some(m) = c.minion_mut(t) {
                            make_golden(m);
                        }
                    }
                }),
                battlecry_target: TargetSpec::FriendlyNonGoldenMaxTier(6),
                ..E
            },
        ),
        // Polarizing Beatboxer: echoes Magnetizations to other minions (`magnetize_onto`).
        ("BG26_149", E),
        // Champion of Sargeras: Battlecry and Deathrattle: minions in the Tavern +8/+8
        // (+16/+16) this game.
        (
            "BG27_016",
            Script {
                battlecry: Some(|c, me, _| tavern_this_game(c, me.side, 8 * me.g(), 8 * me.g())),
                deathrattle: Some(|c, me| tavern_this_game(c, me.side, 8 * me.g(), 8 * me.g())),
                ..E
            },
        ),
        // Obsidian Ravager: Rally: deal damage equal to its Attack to the target and an
        // adjacent minion (golden: both neighbours).
        (
            "BG27_017",
            Script {
                rally: Some(|c, me, t| {
                    let Some(a) = c.minion(me.eid).map(|m| m.atk) else {
                        return;
                    };
                    let mut adj = c.neighbors(t);
                    if !me.golden && adj.len() > 1 {
                        let i = c.g.rng.below(adj.len());
                        adj = vec![adj[i]];
                    }
                    c.damage(me.eid, t, a);
                    for n in adj {
                        c.damage(me.eid, n, a);
                    }
                }),
                ..E
            },
        ),
        // Stitched Salvager: Start of Combat: destroy the minion to the left (golden: both
        // neighbours). Deathrattle: summon exact copies of them (except Stitched Salvagers).
        (
            "BG31_999",
            Script {
                start_of_combat: Some(|c, me| {
                    let Some((s, i)) = c.locate(me.eid) else {
                        return;
                    };
                    let mut victims = vec![];
                    if i > 0 {
                        victims.push(c.boards[s][i - 1].eid);
                    }
                    if me.golden && i + 1 < c.boards[s].len() {
                        victims.push(c.boards[s][i + 1].eid);
                    }
                    let mut copies = vec![];
                    for v in victims {
                        let Some(m) = c.minion(v).filter(|m| m.alive()) else {
                            continue;
                        };
                        if !m.is_card("BG31_999") {
                            copies.push(m.clone());
                        }
                        c.destroy(v);
                    }
                    c.cs[me.side].stored.insert(me.eid, copies);
                    c.resolve_deaths();
                }),
                deathrattle: Some(|c, me| {
                    let copies = c.cs[me.side]
                        .stored
                        .get(&me.eid)
                        .cloned()
                        .unwrap_or_default();
                    let mut pos = if me.pos == 0 && c.on_board(me.eid) {
                        c.pos_right_of(me.eid)
                    } else {
                        me.pos
                    };
                    for mut m in copies {
                        m.eid = c.g.alloc_eid();
                        m.combat_only = true;
                        m.pool_copies = 0;
                        m.attacked = false;
                        if c.summon(me.side, pos, m).is_some() {
                            pos += 1;
                        }
                    }
                }),
                ..E
            },
        ),
        // Futurefin: end of turn, give its stats (double) to the left-most minion in hand.
        (
            "BG34_145",
            Script {
                end_of_turn: Some(|c, me| {
                    let Some((a, h)) = c.minion(me.eid).map(|m| (m.atk, m.hp)) else {
                        return;
                    };
                    let first = c
                        .player_ref(me.side)
                        .and_then(|p| p.hand.iter().find(|h| h.item.as_minion().is_some()))
                        .map(|h| h.hid);
                    if let Some(t) = first {
                        c.buff(t, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                    }
                }),
                ..E
            },
        ),
        // Highkeeper Ra: Battlecry, Deathrattle and Rally: get 1 (2) random Tier 6 minions.
        (
            "BG34_319",
            Script {
                battlecry: Some(|c, me, _| tier6(c, me)),
                deathrattle: Some(tier6),
                rally: Some(|c, me, _| tier6(c, me)),
                ..E
            },
        ),
        // The Last One Standing: Rally: give a friendly minion of each type +15/+15
        // permanently (golden: twice).
        (
            "BG34_320",
            Script {
                rally: Some(|c, me, _| {
                    // One pick; the golden gives the same minions +30/+30.
                    {
                        for (t, _) in one_minion_per_type(c, me.side, |_| true) {
                            c.buff_perm(me.side, t, 15 * me.g(), 15 * me.g());
                        }
                    }
                }),
                ..E
            },
        ),
        // Stalwart Kodo: after you summon a minion in combat, give it this minion's maximum
        // stats (double); 3 times per combat (counter on the combat copy).
        (
            "BG34_322",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Summoned { side, eid } = e else {
                        return;
                    };
                    if !c.combat || *side != me.side || *eid == me.eid {
                        return;
                    }
                    let Some((a, h, used)) = c.minion(me.eid).map(|m| (m.atk, m.max_hp, m.counter))
                    else {
                        return;
                    };
                    if used >= 3 {
                        return;
                    }
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter += 1;
                    }
                    c.buff(*eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                }),
                listens: ev::SUMMONED,
                ..E
            },
        ),
        // Stone Age Slab: once per turn, after you buy a minion, give it +20/+20 and double
        // (triple) its stats (counter = used this turn).
        (
            "BG34_950",
            Script {
                flags: flag::TURN_USES,
                on_event: Some(|c, me, e| {
                    let Event::Bought { side, eid, .. } = e else {
                        return;
                    };
                    if *side != me.side || c.minion(me.eid).is_none_or(|m| m.counter > 0) {
                        return;
                    }
                    let Some((a, h)) = c
                        .player_ref(me.side)
                        .and_then(|p| p.hand.iter().find(|x| x.hid == *eid))
                        .and_then(|x| x.item.as_minion())
                        .map(|m| (m.atk, m.hp))
                    else {
                        return;
                    };
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = 1;
                    }
                    let mult = if me.golden { 3 } else { 2 };
                    let extra = |v: i32| {
                        20i32.saturating_add(v.saturating_add(20).saturating_mul(mult - 1))
                    };
                    c.buff(*eid, extra(a), extra(h));
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
        // Sha of Fear: whenever you cast a Tavern spell, your minions and Deity +3/+3 (+6/+6).
        (
            "BG36_111",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, tavern: true, .. } if *side == me.side)
                    {
                        let deity = c.cs[me.side].deity_eid;
                        for t in c.friendly(me.side) {
                            if t != deity {
                                c.buff(t, 3 * me.g(), 3 * me.g());
                            }
                        }
                        buff_deity(c, me.side, 3 * me.g(), 3 * me.g());
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Jailbird Juggernaut: Rally: summon a Golem with its stats (double) to its right that
        // attacks the target first.
        (
            "BG36_333",
            Script {
                rally: Some(|c, me, t| {
                    let Some((a, h)) = c.minion(me.eid).map(|m| (m.atk, m.hp)) else {
                        return;
                    };
                    let Some((_, i)) = c.locate(me.eid) else {
                        return;
                    };
                    let mut golem = c.new_minion(me.side, cid(GOLEM));
                    golem.set_stats(a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                    if let Some(e) = c.summon(me.side, i + 1, golem)
                        && c.minion(t).is_some_and(|m| m.alive())
                    {
                        crate::combat::do_attack(c, me.side, e, t);
                    }
                }),
                ..E
            },
        ),
    ]
}
