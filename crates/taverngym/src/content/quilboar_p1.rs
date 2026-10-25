//! Quilboar (phase 1).

use super::*;

const BARRAGE: &str = "BG34_689"; // Blood Gem Barrage

fn gems_on_all(c: &mut Ctx, side: Side, n: i32, f: impl Fn(&Minion) -> bool) {
    for t in c.friendly_where(side, f) {
        gems_on(c, side, t, n);
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Gem Rat: end of turn, get 1 (2) Gem Days.
        (
            "BG31_326",
            Script {
                end_of_turn: Some(|c, me| give_n(c, me.side, quilboar::GEM_DAY, me.g() as usize)),
                ..E
            },
        ),
        // Briarback Drummer: Battlecry: get 1 (2) Blood Gem Barrages.
        (
            "BG34_683",
            Script {
                battlecry: Some(|c, me, _| give_n(c, me.side, BARRAGE, me.g() as usize)),
                ..E
            },
        ),
        // Razorfen Flapper: Battlecry and Deathrattle: get 1 (2) Blood Gem Barrages.
        (
            "BG34_682",
            Script {
                battlecry: Some(|c, me, _| give_n(c, me.side, BARRAGE, me.g() as usize)),
                deathrattle: Some(|c, me| give_n(c, me.side, BARRAGE, me.g() as usize)),
                ..E
            },
        ),
        // (Blood Gem Barrage itself is in spells.rs.)
        // Trench Fighter: end of turn, get 1 (2) Gem Confiscations.
        (
            "BG34_684",
            Script {
                end_of_turn: Some(|c, me| give_n(c, me.side, "BG28_698", me.g() as usize)),
                ..E
            },
        ),
        // Sly Infiltrator: Choose One - gain 2 (4) free Refreshes; or get 3 (6) Blood Gems.
        (
            "BG36_330",
            Script {
                choose_one: true,
                battlecry: Some(|c, me, _| {
                    if me.opt_a()
                        && let Some(p) = c.player(me.side)
                    {
                        p.free_refreshes += 2 * me.g();
                    }
                    if me.opt_b() {
                        give_n(c, me.side, BLOOD_GEM, 3 * me.g() as usize);
                    }
                }),
                ..E
            },
        ),
        // Bramble Tunneler: Rally: get 1 (2) random Choose One cards.
        (
            "BG36_331",
            Script {
                rally: Some(|c, me, _| {
                    let cands = choose_one_cards(c, me.side);
                    for _ in 0..me.g() {
                        if let Some(&card) = c.g.rng.pick(&cands) {
                            let mut item = choose_one_item(c, me.side, card);
                            // A generated minion holds one pool copy when one is left.
                            if let Item::Minion(m) = &mut item {
                                c.take_pool_copy(m);
                            }
                            c.add_to_hand(me.side, item);
                        }
                    }
                }),
                ..E
            },
        ),
        // Snare Trapper: Choose One - get 1 (2) random Quilboar; or +1 (+2) maximum Gold.
        (
            "BG36_332",
            Script {
                choose_one: true,
                battlecry: Some(|c, me, _| {
                    if me.opt_a() {
                        let tier = c.tier(me.side);
                        for _ in 0..me.g() {
                            if let Some(q) = c.random_minion(|d| {
                                d.races.intersects(Races::QUILBOAR) && d.tier <= tier
                            }) {
                                c.give_card(me.side, q);
                            }
                        }
                    }
                    if me.opt_b() {
                        c.add_max_gold(me.side, me.g());
                    }
                }),
                ..E
            },
        ),
        // Turbo Hogrider: after you play a Choose One card, play 2 (4) Blood Gems on all your
        // Quilboar (36.6.3: itself included).
        (
            "BG31_323",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::CardPlayed { side, card, eid } = e else {
                        return;
                    };
                    if *side != me.side
                        || *eid == me.eid
                        || !db().get(*card).mech.has(Mech::CHOOSE_ONE)
                    {
                        return;
                    }
                    gems_on_all(c, me.side, 2 * me.g(), |m| m.is(Races::QUILBOAR));
                }),
                listens: ev::CARD_PLAYED,
                ..E
            },
        ),
        // Veteran Brigand: Choose One - play 3 (6) Blood Gems on all your minions; or cast Blood
        // Gem Barrage 3 (6) times.
        (
            "BG36_341",
            Script {
                choose_one: true,
                battlecry: Some(|c, me, _| {
                    if me.opt_a() {
                        gems_on_all(c, me.side, 3 * me.g(), |_| true);
                    }
                    if me.opt_b() {
                        for _ in 0..3 * me.g() {
                            c.you_cast(me.side, cid(BARRAGE), 0, 0, 0);
                        }
                    }
                }),
                ..E
            },
        ),
        // Victorious Geomant: Activate (2): get 6 (12) Blood Gems; cast the ones that don't fit
        // on your left-most minion.
        (
            "BG36_370",
            Script {
                activate: Some((2, TargetSpec::None, |c, me, _| {
                    // The Gems that don't fit are cast by the effect, not by you: the client counts
                    // none of them as your spells - no
                    // "after you cast", no spell count, no Hot-Air Surveyor ("played from your
                    // hand"). Balinda's "your spells that target friendly minions cast twice" still
                    // does: a golden Balinda cast one three times.
                    for _ in 0..6 * me.g() {
                        if c.give_card(me.side, cid(BLOOD_GEM)).is_none()
                            && let Some(t) = c.leftmost(me.side)
                        {
                            for _ in 0..c.balinda_times(me.side, t) {
                                c.cast_spell(me.side, cid(BLOOD_GEM), 0, t, 0);
                            }
                        }
                    }
                })),
                ..E
            },
        ),
    ]
}
