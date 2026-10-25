//! Quilboar minions and Blood Gem cards.

use super::*;

pub const GEM_DAY: &str = "BG31_893";

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Tusked Camper: Rally: plays a Blood Gem (golden: 2) on itself.
        (
            "BG33_886",
            Script {
                rally: Some(|c, me, _| gems_on(c, me.side, me.eid, me.g())),
                ..E
            },
        ),
        // Crater Miner: Choose One - get 2 Blood Gems (4); or get a Gem Day (2).
        (
            "BG31_320",
            Script {
                choose_one: true,
                battlecry: Some(|c, me, _| {
                    if me.opt_a() {
                        give_n(c, me.side, BLOOD_GEM, 2 * me.g() as usize);
                    }
                    if me.opt_b() {
                        give_n(c, me.side, GEM_DAY, me.g() as usize);
                    }
                }),
                ..E
            },
        ),
        // Gem Day: Choose One - Blood Gems give an extra +1 Attack this game; or +1 Health.
        (
            GEM_DAY,
            Script {
                choose_one: true,
                spell: Some(|c, me, _| {
                    if me.opt_a() {
                        gem_bonus(c, me.side, 1, 0);
                    }
                    if me.opt_b() {
                        gem_bonus(c, me.side, 0, 1);
                    }
                }),
                ..E
            },
        ),
        // Prodigious Tusker: whenever another friendly minion attacks, play a Blood Gem (2) on it.
        (
            "BG33_430",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Attacks { side, attacker, .. } = e
                        && *side == me.side
                        && *attacker != me.eid
                    {
                        gems_on(c, me.side, *attacker, me.g());
                    }
                }),
                listens: ev::ATTACKS,
                ..E
            },
        ),
        // Quilled Cabbie: after the Tavern is Refreshed, play 2 (4) Blood Gems on its
        // right-most minion.
        (
            "BG34_681",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Refreshed { side, .. } if *side == me.side)
                        && let Some(&t) = c.tavern_minions(me.side).last()
                    {
                        gems_on(c, me.side, t, 2 * me.g());
                    }
                }),
                listens: ev::REFRESHED,
                ..E
            },
        ),
        // Fearless Foodie: Choose One - Blood Gems give an extra +1/+1 (+2/+2) this game; or get
        // 4 (8) Blood Gems.
        (
            "BG30_123",
            Script {
                choose_one: true,
                battlecry: Some(|c, me, _| {
                    if me.opt_a() {
                        gem_bonus(c, me.side, me.g(), me.g());
                    }
                    if me.opt_b() {
                        give_n(c, me.side, BLOOD_GEM, 4 * me.g() as usize);
                    }
                }),
                ..E
            },
        ),
        // Bonker: Windfury. Rally: play a Blood Gem (2) on all your other minions.
        (
            "BG20_104",
            Script {
                rally: Some(|c, me, _| {
                    for t in c.friendly(me.side) {
                        if t != me.eid {
                            gems_on(c, me.side, t, me.g());
                        }
                    }
                }),
                ..E
            },
        ),
        // Geomagus Roogug: whenever a Blood Gem is played on this, play one (2) on a different
        // friendly minion.
        (
            "BG28_583",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::GemPlayed { side, target } = e
                        && *side == me.side
                        && *target == me.eid
                        // Never onto another Roogug.
                        && let Some(t) = c.random_friendly_other(me, |m| {
                            db().normal_of(m.card) != db().normal_of(me.card)
                        })
                    {
                        gems_on(c, me.side, t, me.g());
                    }
                }),
                listens: ev::GEM_PLAYED,
                ..E
            },
        ),
        // Hot-Air Surveyor: handled in recruit::spell_cast_times.
        ("BG30_121", E),
        // Razorfen Vineweaver: Rally: play 4 (8) permanent Blood Gems on itself.
        (
            "BG33_883",
            Script {
                rally: Some(|c, me, _| {
                    for _ in 0..4 * me.g() {
                        c.play_gem_perm(me.side, me.eid);
                    }
                }),
                ..E
            },
        ),
        // Sanguine Refiner: Rally: Blood Gems give an extra +1/+2 (+2/+4) this game.
        (
            "BG33_885",
            Script {
                rally: Some(|c, me, _| gem_bonus(c, me.side, me.g(), 2 * me.g())),
                ..E
            },
        ),
        // Sanguine Champion: Battlecry and Deathrattle: Blood Gems give an extra +2/+1 (+4/+2).
        (
            "BG23_017",
            Script {
                battlecry: Some(|c, me, _| gem_bonus(c, me.side, 2 * me.g(), me.g())),
                deathrattle: Some(|c, me| gem_bonus(c, me.side, 2 * me.g(), me.g())),
                ..E
            },
        ),
        // Gem Confiscation: play 3 Blood Gems on a minion; it steals all Blood Gems from its
        // neighbours (their gem stats move to the target).
        (
            "BG28_698",
            Script {
                spell: Some(|c, me, t| {
                    gems_on(c, me.side, t, 3);
                    // Neighbours in the warband, or in the Tavern for a Tavern target.
                    let mut around = c.neighbors(t);
                    if c.locate(t).is_none()
                        && let Some(pl) = c.player_ref(me.side)
                        && let Some(i) = pl.shop.iter().position(|s| s.sid == t)
                    {
                        let at = |j: usize| pl.shop.get(j).filter(|s| s.item.as_minion().is_some());
                        around = [i.checked_sub(1), Some(i + 1)]
                            .into_iter()
                            .flatten()
                            .filter_map(at)
                            .map(|s| s.sid)
                            .collect();
                    }
                    for n in around {
                        let Some(m) = c.find_any_mut(n) else { continue };
                        let (g, a, h) = (m.gems, m.gem_atk, m.gem_hp);
                        if g == 0 {
                            continue;
                        }
                        m.add_stats(-a, -h);
                        m.gems = 0;
                        m.gem_atk = 0;
                        m.gem_hp = 0;
                        c.buff(t, a, h);
                        if let Some(tm) = c.find_any_mut(t) {
                            tm.gems += g;
                            tm.gem_atk += a;
                            tm.gem_hp += h;
                        }
                    }
                }),
                spell_target: TargetSpec::Friendly,
                ..E
            },
        ),
        // Blood Gem variants: +1/+1 (with gem bonuses) and, on a Quilboar, a keyword.
        ("BG20_GEM_DivineShield", gem_variant()),
        ("BG20_GEM_Reborn", gem_variant()),
        ("BG20_GEM_Taunt", gem_variant()),
        // Mass Blood Gem: play a Blood Gem on every friendly minion ("+0/+0" = gem stats).
        (
            "BG20_GEMt",
            Script {
                spell: Some(|c, me, _| {
                    for t in c.friendly(me.side) {
                        c.play_gem(me.side, t);
                    }
                }),
                ..E
            },
        ),
        // Internal no-effect gem entities.
        ("BG20_GEM_No_Impact", E),
        ("BG20_GEM_No_Impact_No_Target", E),
    ]
}

fn gem_variant() -> Script {
    Script {
        spell: Some(|c, me, t| {
            c.play_gem(me.side, t);
            let k = match db().get(me.card).id.as_str() {
                "BG20_GEM_DivineShield" => Kw::DIVINE_SHIELD,
                "BG20_GEM_Reborn" => Kw::REBORN,
                _ => Kw::TAUNT,
            };
            if c.minion(t).is_some_and(|m| m.is(Races::QUILBOAR)) {
                c.give_kw(t, k);
            }
        }),
        spell_target: TargetSpec::Friendly,
        ..E
    }
}
