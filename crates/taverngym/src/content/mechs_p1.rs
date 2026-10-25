//! Mechs (phase 1).

use super::*;

// The client's Satellite (Spark Snapper's and Reusable Batteries' BG31_171te enchantment); its
// stats are always set.
const SATELLITE: &str = "BG31_171t";

/// Magnetize a Satellite with stats `a`/`h` to `host`.
fn satellite(c: &mut Ctx, side: Side, host: EntityId, a: i32, h: i32) {
    let mut m = c.new_minion(side, cid(SATELLITE));
    m.set_stats(a, h);
    crate::recruit::magnetize_onto(c, side, host, m);
}

/// Stats that go into the Magnetization a host has just taken (no card attached of their own).
fn fold_into_magnet(c: &mut Ctx, host: EntityId, a: i32, h: i32) {
    let Some(m) = c.minion_mut(host) else { return };
    m.add_stats(a, h);
    c.stats_changed(host, a.max(0), h.max(0));
}

fn mech_played(_c: &Ctx, me: Me, e: &Event) -> Option<EntityId> {
    match e {
        Event::Played { side, eid, played }
            if *side == me.side
                && played.source != me.eid
                && played.races.intersects(Races::MECH) =>
        {
            Some(*eid)
        }
        _ => None,
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Mechagnome Interpreter: whenever you play or Magnetize a Mech, give it +3/+1 (+6/+2).
        (
            "BG31_177",
            Script {
                on_event: Some(|c, me, e| {
                    let t = match e {
                        Event::Magnetized { side, host, card }
                            if *side == me.side
                                && db().get(*card).races.intersects(Races::MECH) =>
                        {
                            Some(*host)
                        }
                        // "Play or Magnetize": one trigger per action.
                        _ if c.magnet_play => None,
                        _ => mech_played(c, me, e),
                    };
                    if let Some(t) = t {
                        c.buff(t, 3 * me.g(), me.g());
                    }
                }),
                listens: ev::PLAYED | ev::MAGNETIZED,
                ..E
            },
        ),
        // Rescue Bot: Deathrattle: get 1 (2) Repair Jobs.
        (
            "BG36_854",
            Script {
                deathrattle: Some(|c, me| give_n(c, me.side, "BG36_624", me.g() as usize)),
                ..E
            },
        ),
        // Enchanted Sentinel: Tavern spells give an extra +1/+1 (+2/+2) (`Ctx::spell_extra`).
        ("BG35_341", E),
        // Drone Duplicator: Activate (1): the next Magnetization to this this turn happens 1 (2)
        // extra times (`magnetize_onto`).
        (
            "BG36_506",
            Script {
                activate: Some((1, TargetSpec::None, |c, me, _| {
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = me.g();
                    }
                })),
                start_of_turn: Some(|c, me| {
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = 0;
                    }
                }),
                ..E
            },
        ),
        // Gearfin: end of turn, get 2 (4) random 1-Cost Tavern spells.
        (
            "BG36_764",
            Script {
                end_of_turn: Some(|c, me| {
                    let tier = c.tier(me.side);
                    for _ in 0..2 * me.g() {
                        if let Some(s) = c.random_spell(|d| d.cost == 1 && d.tier <= tier) {
                            c.give_card(me.side, s);
                        }
                    }
                }),
                ..E
            },
        ),
        // Glambot: whenever you cast a spell on a Mech, Magnetize a 4/4 Satellite to it (twice).
        (
            "BG36_853",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::SpellCast { side, target, .. } = e else {
                        return;
                    };
                    if *side != me.side || !c.on_board(*target) {
                        return;
                    }
                    if c.minion(*target).is_some_and(|m| {
                        m.is(Races::MECH) && c.locate(m.eid).map(|l| l.0) == Some(me.side)
                    }) {
                        for _ in 0..me.g() {
                            satellite(c, me.side, *target, 4, 4);
                        }
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Charging Czarina: whenever you cast a Tavern spell, your Divine Shield minions get
        // +4 (+8) Attack.
        (
            "BG28_741",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, tavern: true, .. } if *side == me.side)
                    {
                        // Czarina Portrait: they also give Health.
                        let h = if has_trinket(c, me.side, "BG32_MagicItem_283") {
                            4 * me.g()
                        } else {
                            0
                        };
                        for t in c.friendly_where(me.side, |m| m.has(Kw::DIVINE_SHIELD)) {
                            c.buff(t, 4 * me.g(), h);
                        }
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Spark Snapper: whenever you play a Mech, Magnetize a 2/3 (4/6) Satellite to it and
        // improve this (each improvement adds the base stats again).
        (
            "BG36_851",
            Script {
                on_event: Some(|c, me, e| {
                    let Some(t) = mech_played(c, me, e) else {
                        return;
                    };
                    let k = me
                        .g()
                        .saturating_add(c.minion(me.eid).map_or(0, |m| m.counter));
                    if c.magnet_play {
                        // The Mech was played by Magnetizing it: "it" is the magnet, already part
                        // of the host. The client puts the Satellite's stats into that
                        // Magnetization, no Satellite of its own (the host's attached cards and
                        // Utility Drone's count show it).
                        fold_into_magnet(c, t, 2 * k, 3 * k);
                    } else {
                        satellite(c, me.side, t, 2 * k, 3 * k);
                    }
                    c.counter_perm(me.side, me.eid, me.g());
                }),
                listens: ev::PLAYED,
                flags: flag::IMPROVES,
                // "Whenever you play a Mech, Magnetize a 2/3 Satellite to it and improve this."
                shown: Some(|_, sh| {
                    let k = sh.g().saturating_add(sh.counter());
                    vec![2 * k, 3 * k]
                }),
                ..E
            },
        ),
        // Utility Drone: end of turn, give your minions +4/+5 (+8/+10) per Magnetization.
        (
            "BG26_152",
            Script {
                end_of_turn: Some(|c, me| {
                    let targets: Vec<(EntityId, i32)> = c.boards[me.side]
                        .iter()
                        .map(|m| {
                            let n = m
                                .attached
                                .iter()
                                .filter(|a| db().get(**a).kind == crate::cards::CardKind::Minion)
                                .count() as i32;
                            (m.eid, n)
                        })
                        .collect();
                    for (t, n) in targets {
                        if n > 0 {
                            c.buff(t, 4 * me.g() * n, 5 * me.g() * n);
                        }
                    }
                }),
                ..E
            },
        ),
        // Falling Sky Golem: +4/+2 (+8/+4) per Deathrattle triggered (`apply_bonuses`).
        ("BG35_342", E),
    ]
}
