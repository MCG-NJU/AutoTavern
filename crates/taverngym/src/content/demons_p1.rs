//! Demons (phase 1): hero-damage synergies and Tavern buffs.

use super::*;

fn hero_hit(me: Me, e: &Event) -> Option<i32> {
    match e {
        Event::HeroDamaged { side, amount, .. } if *side == me.side && *amount > 0 => Some(*amount),
        _ => None,
    }
}

/// "Give minions in the Tavern +a/+h this game."
fn tavern_this_game(c: &mut Ctx, side: Side, a: i32, h: i32) {
    if let Some(p) = c.player(side) {
        p.stats.tavern_atk = p.stats.tavern_atk.saturating_add(a);
        p.stats.tavern_hp = p.stats.tavern_hp.saturating_add(h);
    }
    c.buff_tavern(side, a, h, Races::NONE);
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Ominous Seer: Battlecry: the next Tavern spell you buy costs (1) (2) less.
        (
            "BG31_330",
            Script {
                battlecry: Some(|c, me, _| {
                    if let Some(p) = c.player(me.side) {
                        p.stats.spell_discount_next =
                            p.stats.spell_discount_next.saturating_add(me.g());
                    }
                }),
                ..E
            },
        ),
        // Soul Rewinder: after your hero takes damage, rewind it and give this +2 (+4) Health.
        (
            "BG26_174",
            Script {
                on_event: Some(|c, me, e| {
                    if hero_hit(me, e).is_some() {
                        c.rewind_hero_damage(me.side);
                        // Rewinder Portrait: Soul Rewinders also gain Attack.
                        let a = if has_trinket(c, me.side, "BG30_MagicItem_868") {
                            2 * me.g()
                        } else {
                            0
                        };
                        c.buff(me.eid, a, 2 * me.g());
                    }
                }),
                listens: ev::HERO_DAMAGED,
                ..E
            },
        ),
        // Malchezaar: two (four) Refreshes each turn cost Health instead of Gold (`refresh`).
        (
            "BG26_524",
            Script {
                // Counter = health-paid Refreshes left this turn.
                on_create: Some(|m| m.counter = if m.golden { 4 } else { 2 }),
                start_of_turn: Some(|c, me| {
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = 2 * me.g();
                    }
                }),
                // "Two Refreshes each turn cost Health instead of Gold. (2 left!)"
                shown: Some(|_, sh| vec![sh.minion.map_or(2 * sh.g(), |m| m.counter).max(0)]),
                ..E
            },
        ),
        // Devout Hellcaller: after another friendly Demon deals damage, gain +2/+2 (+4/+4)
        // permanently.
        (
            "BG33_155",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Damaged { source, amount, .. } = e else {
                        return;
                    };
                    if *amount <= 0 || *source == me.eid {
                        return;
                    }
                    // A Demon dying in the same exchange still dealt the damage: dead minions not
                    // yet removed count.
                    let friendly_demon = c.boards[me.side]
                        .iter()
                        .any(|m| m.eid == *source && m.is(Races::DEMON));
                    if friendly_demon {
                        c.buff_perm(me.side, me.eid, 2 * me.g(), 2 * me.g());
                    }
                }),
                listens: ev::DAMAGED,
                ..E
            },
        ),
        // Imposing Percussionist: Battlecry: Discover 1 (2) Demons; your hero takes damage
        // equal to each one's Tier (`after_discover`).
        (
            "BG26_525",
            Script {
                battlecry: Some(|c, me, _| {
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        c.discover_minion(me.side, me.card, 3, |d| {
                            d.races.intersects(Races::DEMON) && d.tier <= tier
                        });
                    }
                }),
                ..E
            },
        ),
        // Ashen Corruptor: after your hero takes damage, rewind it and give minions in the Tavern
        // +2/+2 (+4/+4) this turn. Like "this game" with a shorter life: the minions in the Tavern
        // get it now, and the ones that enter it later in the turn get it on arrival
        // (`apply_tavern_buffs`).
        (
            "BG32_873",
            Script {
                on_event: Some(|c, me, e| {
                    if hero_hit(me, e).is_some() {
                        c.rewind_hero_damage(me.side);
                        let k = 2 * me.g();
                        c.add_turn_data(me.side, crate::cardid!("BG32_873"), k);
                        c.buff_tavern_temp(me.side, k, k);
                    }
                }),
                listens: ev::HERO_DAMAGED,
                ..E
            },
        ),
        // Sacrificial Wrathguard: Deathrattle: minions in the Tavern +2/+2 (+4/+4) this game.
        // Activate (1): improve this (each improvement adds the base again).
        (
            "BG36_362",
            Script {
                deathrattle: Some(|c, me| {
                    // A dead Wrathguard is off the board: its improvements are in the death
                    // record (a triggered living one still has them on the board).
                    let n = match c.minion(me.eid) {
                        Some(m) => m.counter,
                        None => c.dead_record(me.side, me.eid).map_or(0, |d| d.counter),
                    };
                    let k = 2 * (me.g() + n);
                    tavern_this_game(c, me.side, k, k);
                }),
                activate: Some((1, TargetSpec::None, |c, me, _| {
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter += me.g();
                    }
                })),
                flags: flag::IMPROVES,
                // "Deathrattle: Give minions in the Tavern +2/+2 this game. Activate (1):
                // Improve this."
                shown: Some(|_, sh| {
                    let k = 2 * (sh.g().saturating_add(sh.counter()));
                    vec![k, k, 1]
                }),
                ..E
            },
        ),
        // Imp-lusionist: Deathrattle: get 1 (2) Methodical Madness.
        (
            "BG36_731",
            Script {
                deathrattle: Some(|c, me| give_n(c, me.side, "BG36_880", me.g() as usize)),
                ..E
            },
        ),
        // Tichondrius: after your hero takes damage, give your Demons +4/+4 (+8/+8).
        (
            "BG26_523",
            Script {
                on_event: Some(|c, me, e| {
                    if hero_hit(me, e).is_some() {
                        for t in c.friendly_where(me.side, |m| m.is(Races::DEMON)) {
                            c.buff(t, 4 * me.g(), 4 * me.g());
                        }
                    }
                }),
                listens: ev::HERO_DAMAGED,
                ..E
            },
        ),
        // Deft Deserter: Activate (1): give all minions in the Tavern +8/+8 (+16/+16) and
        // Taunt, Divine Shield, or Windfury (random per minion).
        (
            "BG36_621",
            Script {
                activate: Some((1, TargetSpec::None, |c, me, _| {
                    for sid in c.tavern_minions(me.side) {
                        c.buff(sid, 8 * me.g(), 8 * me.g());
                        let k = [Kw::TAUNT, Kw::DIVINE_SHIELD, Kw::WINDFURY][c.g.rng.below(3)];
                        c.give_kw(sid, k);
                    }
                })),
                ..E
            },
        ),
        // Devilish Distractor: whenever you cast a spell on this, minions in the Tavern +1/+2
        // (+2/+4) this game.
        (
            "BG36_762",
            Script {
                on_event: Some(|c, me, e| {
                    if aimed_at_me(c, me, e) {
                        tavern_this_game(c, me.side, me.g(), 2 * me.g());
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Eredar Escapist: after your hero takes 4 damage, get 1 (2) Corrupted Cupcakes.
        (
            "BG36_733",
            Script {
                on_event: Some(|c, me, e| {
                    let Some(n) = hero_hit(me, e) else { return };
                    let total = c.counter_perm(me.side, me.eid, n);
                    let fired = total / 4;
                    if fired > 0 {
                        c.counter_perm(me.side, me.eid, -4 * fired);
                        give_n(c, me.side, "BG28_607", (fired * me.g()) as usize);
                    }
                }),
                listens: ev::HERO_DAMAGED,
                // "After your hero takes 4 damage, get a copy of Corrupted Cupcakes.
                // (4 left!)"
                shown: Some(|_, sh| vec![4, (4 - sh.counter()).max(0)]),
                ..E
            },
        ),
    ]
}
