//! Neutral minions (phase 1).

use super::*;

fn spell_stats(c: &mut Ctx, side: Side, a: i32, h: i32) {
    if let Some(p) = c.player(side) {
        p.stats.spell_atk = p.stats.spell_atk.saturating_add(a);
        p.stats.spell_hp = p.stats.spell_hp.saturating_add(h);
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Intrepid Botanist: Choose One - Tavern spells give an extra +1 (+2) Attack this
        // game; or +1 (+2) Health.
        (
            "BG32_237",
            Script {
                choose_one: true,
                battlecry: Some(|c, me, _| {
                    if me.opt_a() {
                        spell_stats(c, me.side, me.g(), 0);
                    }
                    if me.opt_b() {
                        spell_stats(c, me.side, 0, me.g());
                    }
                }),
                ..E
            },
        ),
        // Decoy Conjurer: Activate (2): steal the 1 (2) highest-Attack minions in the Tavern.
        (
            "BG36_354",
            Script {
                activate: Some((2, TargetSpec::None, |c, me, _| {
                    let Some(p) = c.owner[me.side] else { return };
                    for _ in 0..me.g() {
                        if c.g.players[p].hand.len() >= c.g.cfg.hand_size {
                            return;
                        }
                        let keyed: Vec<(EntityId, i64)> = c.g.players[p]
                            .shop
                            .iter()
                            .filter_map(|s| s.item.as_minion().map(|m| (s.sid, m.atk as i64)))
                            .collect();
                        let Some(sid) = c.pick_max(keyed) else { return };
                        if let Some(m) = c.take_from_shop(me.side, sid) {
                            c.add_to_hand(me.side, Item::Minion(m));
                        }
                    }
                })),
                ..E
            },
        ),
        // Iron Groundskeeper: Battlecry: get 2 (4) Fortify.
        (
            "BG27_000",
            Script {
                battlecry: Some(|c, me, _| give_n(c, me.side, "BG28_503", 2 * me.g() as usize)),
                ..E
            },
        ),
        // Disguised Graverobber: Battlecry: destroy a friendly Undead to get 1 (2) plain copies.
        (
            "BG28_303",
            Script {
                battlecry: Some(|c, me, t| {
                    let Some(card) = c.minion(t).map(|m| db().normal_of(m.card)) else {
                        return;
                    };
                    c.destroy(t);
                    c.resolve_deaths();
                    for _ in 0..me.g() {
                        c.give_card(me.side, card);
                    }
                }),
                battlecry_target: TargetSpec::FriendlyRaceOther(Races::UNDEAD),
                ..E
            },
        ),
        // Fruit Vendor: Activate (1): get 2 (4) Tavern Dish Bananas.
        (
            "BG36_346",
            Script {
                activate: Some((1, TargetSpec::None, |c, me, _| {
                    give_n(c, me.side, "BG28_897", 2 * me.g() as usize)
                })),
                ..E
            },
        ),
        // Sin'dorei Straight Shot: Rally: remove Reborn and Taunt from the target.
        (
            "BG25_016",
            Script {
                rally: Some(|c, _, t| {
                    if let Some(m) = c.minion_mut(t) {
                        m.remove(Kw::REBORN);
                        m.remove(Kw::TAUNT);
                    }
                }),
                ..E
            },
        ),
        // Humon'gozz: Tavern spells give an extra +1/+2 (+2/+4) (`Ctx::spell_extra`).
        ("BG32_341", E),
        // Heroic Underdog: Rally: gain the target's Attack (double).
        (
            "BG34_604",
            Script {
                rally: Some(|c, me, t| {
                    if let Some(a) = c.minion(t).map(|m| m.atk) {
                        c.buff(me.eid, a.saturating_mul(me.g()), 0);
                    }
                }),
                ..E
            },
        ),
        // Rodeo Performer: Battlecry: Discover 1 (2) Tavern spells.
        (
            "BG28_550",
            Script {
                battlecry: Some(|c, me, _| {
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        c.discover_spell(me.side, me.card, 3, |d| d.tier <= tier);
                    }
                }),
                ..E
            },
        ),
        // Cataclysmic Harbinger: end of turn, get 1 (2) copies of the last Tavern spell cast.
        (
            "BG35_123",
            Script {
                end_of_turn: Some(|c, me| {
                    give_last_tavern_spell(c, me.side, me.g());
                }),
                ..E
            },
        ),
        // Balinda Stonehearth: spells on friendly minions cast twice (three times)
        // (`spell_cast_times`).
        ("BG35_883", E),
    ]
}
