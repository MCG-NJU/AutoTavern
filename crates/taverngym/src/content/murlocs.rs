//! Murloc minions (hand-zone mechanics live here: summon from hand, while-in-hand triggers).

use super::*;

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Flighty Scout: SoC if in hand, summon a copy (golden: with double stats).
        (
            "BG32_330",
            Script {
                hand_start_of_combat: Some(flighty_scout),
                ..E
            },
        ),
        // Expert Aviator: Rally summon the highest-Attack hand Murloc (golden: 2) for this combat
        // (36.6.3: Murlocs only).
        (
            "BG34_140",
            Script {
                rally: Some(|c, me, _| {
                    let pos = c.pos_right_of(me.eid);
                    for (i, hid) in c
                        .highest_attack_in_hand(me.side, me.g() as usize, |r| {
                            r.intersects(Races::MURLOC)
                        })
                        .into_iter()
                        .enumerate()
                    {
                        c.summon_from_hand(me.side, hid, pos + i);
                    }
                }),
                ..E
            },
        ),
        // Diremuck Forager: SoC summon the highest-Attack Murloc(s) from hand for this combat; the
        // Murloc is chosen when there is space.
        (
            "BG27_556",
            Script {
                start_of_combat: Some(|c, me| {
                    for _ in 0..me.g() {
                        c.when_space(
                            me.side,
                            crate::engine::SpaceReq::FromHandBest(Races::MURLOC),
                        );
                    }
                }),
                ..E
            },
        ),
        // Bream Counter: while in hand, after you play a Murloc, gain +6/+6 (golden +12/+12).
        (
            "BG26_137",
            Script {
                on_event: Some(bream_counter),
                listens: ev::PLAYED,
                listens_in_hand: true,
                ..E
            },
        ),
        // Costume Enthusiast: SoC gain the Attack of the highest-Attack hand minion (golden 2x).
        (
            "BG34_142",
            Script {
                start_of_combat: Some(|c, me| {
                    let best = c
                        .hand_minions(me.side)
                        .into_iter()
                        .map(|(_, a, _)| a)
                        .max()
                        .unwrap_or(0);
                    c.buff(me.eid, best.saturating_mul(me.g()), 0);
                }),
                ..E
            },
        ),
        // Choral Mrrrglr: SoC gain the stats of all hand minions (golden: twice).
        (
            "BG26_354",
            Script {
                start_of_combat: Some(|c, me| {
                    let Some(p) = c.player_ref(me.side) else {
                        return;
                    };
                    let used = &c.cs[me.side].from_hand;
                    let (a, h) = p
                        .hand
                        .iter()
                        .filter(|x| !used.contains(&x.hid))
                        .filter_map(|x| x.item.as_minion())
                        .fold((0, 0), |(a, h), m| (a + m.atk, h + m.max_hp));
                    c.buff(me.eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                }),
                ..E
            },
        ),
    ]
}

fn flighty_scout(c: &mut Ctx, me: Me) {
    let Some(m) = c
        .player_ref(me.side)
        .and_then(|p| p.hand.iter().find(|h| h.hid == me.eid))
        .and_then(|h| h.item.as_minion())
        .cloned()
    else {
        return;
    };
    let mut copy = m;
    copy.eid = c.g.alloc_eid();
    copy.combat_only = true;
    copy.pool_copies = 0;
    if me.golden {
        let (a, h) = (copy.atk, copy.max_hp);
        copy.set_stats(a.saturating_mul(2), h.saturating_mul(2));
    }
    let pos = c.boards[me.side].len();
    c.summon(me.side, pos, copy);
}

fn bream_counter(c: &mut Ctx, me: Me, e: &Event) {
    let Event::Played { side, played, .. } = e else {
        return;
    };
    // Only while in hand, and only for a different, friendly Murloc.
    if *side != me.side || played.source == me.eid || c.on_board(me.eid) {
        return;
    }
    if played.races.intersects(Races::MURLOC) {
        c.buff(me.eid, 6 * me.g(), 6 * me.g());
    }
}
