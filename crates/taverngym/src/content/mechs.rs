//! Mechs: Magnetic Volumizers and the cards that make them.

use super::*;

/// A Volumizer's "this game" bonus when first played or Magnetized.
fn volumizer_bonus(c: &mut Ctx, me: Me) {
    let g = me.g();
    let (a, h) = match db().get(db().normal_of(me.card)).id.as_str() {
        "BG34_170t" => (3 * g, 0),
        "BG34_170t2" => (0, 3 * g),
        _ => (g, g),
    };
    c.bump_player_stat(me.side, |s| {
        s.volumizer_atk = s.volumizer_atk.saturating_add(a);
        s.volumizer_hp = s.volumizer_hp.saturating_add(h);
    });
}

fn volumizer() -> Script {
    Script {
        // Magnetized: `magnetize_onto` runs on_attach. Played: the Played event on itself.
        on_attach: Some(volumizer_bonus),
        on_event: Some(|c, me, e| {
            if matches!(e, Event::Played { eid, .. } if *eid == me.eid) && !c.magnet_play {
                let fresh = c.minion_mut(me.eid).is_some_and(|m| {
                    if m.first_play_reward_used {
                        return false;
                    }
                    // A host played from hand (a copy carrying its Magnetized parts) fires the
                    // bonus of the Volumizer Magnetized last, once per host, at that Volumizer's
                    // own size (five host plays - R,B,G attached +1/+1; B +0/+3; G,B,R,G,R,B +0/+3,
                    // a golden host too).
                    if m.card != me.card && last_volumizer(m) != Some(me.card) {
                        return false;
                    }
                    m.first_play_reward_used = true;
                    true
                });
                if fresh {
                    volumizer_bonus(c, me);
                }
            }
        }),
        listens: ev::PLAYED,
        ..E
    }
}

/// The Volumizer Magnetized to `m` last, if any.
fn last_volumizer(m: &Minion) -> Option<CardId> {
    m.attached
        .iter()
        .rev()
        .copied()
        .find(|a| crate::content::is_volumizer(*a))
}

fn random_volumizer(c: &mut Ctx) -> CardId {
    let i = c.g.rng.below(VOLUMIZERS.len());
    cid(VOLUMIZERS[i])
}

/// Magnetize a new random Volumizer to `host`; returns its card.
pub fn magnetize_volumizer(c: &mut Ctx, side: Side, host: EntityId) -> CardId {
    let card = random_volumizer(c);
    let m = c.new_minion(side, card);
    crate::recruit::magnetize_onto(c, side, host, m);
    card
}

/// "Magnetize a random Volumizer to this. Get a copy (golden: `copies`) of it."
fn magnetize_and_copy(c: &mut Ctx, me: Me, copies: i32) {
    let card = magnetize_volumizer(c, me.side, me.eid);
    for _ in 0..copies {
        c.give_card(me.side, card);
    }
}

fn apexis(c: &mut Ctx, me: Me) {
    let mut mechs = c.friendly_where(me.side, |m| m.is(Races::MECH) && m.eid != me.eid);
    c.g.rng.shuffle(&mut mechs);
    for t in mechs.into_iter().take(me.g() as usize) {
        magnetize_volumizer(c, me.side, t);
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        ("BG34_170t", volumizer()),
        ("BG34_170t2", volumizer()),
        ("BG34_170t3", volumizer()),
        // Auto Accelerator: Battlecry: get a random Magnetic Volumizer (2).
        (
            "BG34_170",
            Script {
                battlecry: Some(|c, me, _| give_random_of(c, me.side, &VOLUMIZERS, me.g())),
                ..E
            },
        ),
        // Conveyor Construct: Deathrattle: get a random Magnetic Volumizer (2).
        (
            "BG34_171",
            Script {
                deathrattle: Some(|c, me| give_random_of(c, me.side, &VOLUMIZERS, me.g())),
                ..E
            },
        ),
        // Metal Dispenser: Avenge (4): magnetize a random Volumizer to this; get 1 (2) copies.
        (
            "BG34_176",
            Script {
                avenge: Some((4, 4, |c, me| magnetize_and_copy(c, me, me.g()))),
                ..E
            },
        ),
        // Timewarped Steamer: end of turn, get one (two) of each Magnetic Volumizer.
        (
            "BG34_PreMadeChamp_038",
            Script {
                end_of_turn: Some(|c, me| {
                    for v in VOLUMIZERS {
                        for _ in 0..me.g() {
                            c.give_card(me.side, cid(v));
                        }
                    }
                }),
                ..E
            },
        ),
        // Resourceful Robot: end of turn, magnetize 1 (2) random Volumizers to this; get
        // copies of them.
        (
            "BG36_366",
            Script {
                end_of_turn: Some(|c, me| {
                    for _ in 0..me.g() {
                        magnetize_and_copy(c, me, 1);
                    }
                }),
                ..E
            },
        ),
        // Apexis Guardian: Deathrattle and Rally: magnetize a random Volumizer to another
        // friendly Mech (2 other Mechs).
        (
            "BG34_173",
            Script {
                deathrattle: Some(apexis),
                rally: Some(|c, me, _| apexis(c, me)),
                ..E
            },
        ),
        // Auto Reveille: after you buy 3 cards, Magnetize a random Volumizer to this and get 1
        // (2) copies of it (36.6.3).
        (
            "BG36_367",
            Script {
                on_event: Some(|c, me, e| {
                    if !matches!(e, Event::Bought { side, .. } if *side == me.side) {
                        return;
                    }
                    let Some(m) = c.minion_mut(me.eid) else {
                        return;
                    };
                    m.counter += 1;
                    if m.counter >= 3 {
                        m.counter = 0;
                        let vol = cid(VOLUMIZERS[c.g.rng.below(VOLUMIZERS.len())]);
                        let magnet = c.new_minion(me.side, vol);
                        crate::recruit::magnetize_onto(c, me.side, me.eid, magnet);
                        for _ in 0..me.g() {
                            c.give_card(me.side, vol);
                        }
                    }
                }),
                listens: ev::BOUGHT,
                // "After you buy 3 cards, Magnetize a random Volumizer to this and get a copy
                // of it. (3 left!)"
                shown: Some(|_, sh| vec![3, (3 - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Dr. Boom's Monster (only from Boom's Monster Portrait): Magnetic. Has +2/+2 (+4/+4) for
        // each time you've Magnetized this game, wherever it is (`engine::apply_bonuses`, counted
        // in `recruit::magnetize_onto`). Magnetized to a host it is worth its stats at that moment,
        // like a Volumizer.
        ("BG31_176", E),
        // Volumizer Portrait: get 1 (upgraded: 2) random Volumizers now and every turn start.
        (
            "BG36_MagicItem_423",
            Script {
                on_attach: Some(|c, me| give_random_of(c, me.side, &VOLUMIZERS, 1)),
                start_of_turn: Some(|c, me| give_random_of(c, me.side, &VOLUMIZERS, 1)),
                ..E
            },
        ),
        (
            "BG36_MagicItem_423t",
            Script {
                on_attach: Some(|c, me| give_random_of(c, me.side, &VOLUMIZERS, 2)),
                start_of_turn: Some(|c, me| give_random_of(c, me.side, &VOLUMIZERS, 2)),
                ..E
            },
        ),
    ]
}
