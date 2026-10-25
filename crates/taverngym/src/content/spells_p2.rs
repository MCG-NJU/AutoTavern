//! Tavern spells (phase 2).

use super::*;

const ANY: TargetSpec = TargetSpec::FriendlyOrTavern;

fn target_spell(spec: TargetSpec, f: crate::script::FxT) -> Script {
    Script {
        spell: Some(f),
        spell_target: spec,
        ..E
    }
}

fn plain(f: crate::script::FxT) -> Script {
    Script {
        spell: Some(f),
        ..E
    }
}

/// Next-combat "Start of Combat:" spell: the cast adds a stack, Start of Combat uses it.
fn next_combat(soc: crate::script::Fx) -> Script {
    Script {
        spell: Some(|c, me, _| c.add_player_effect(me.side, me.card, 1)),
        start_of_combat: Some(soc),
        flags: flag::NEXT_COMBAT,
        ..E
    }
}

fn one_pending(s: Script) -> Script {
    Script {
        flags: s.flags | flag::ONE_PENDING,
        ..s
    }
}

fn races_of(c: &mut Ctx, t: EntityId) -> Races {
    let r = c.find_any(t).map(|m| m.races).unwrap_or(Races::NONE);
    target_type(c, r)
}

fn discover_where(c: &mut Ctx, me: Me, f: impl Fn(&crate::cards::CardDef, u8) -> bool) {
    let tier = c.tier(me.side);
    c.discover_minion(me.side, me.card, 3, |d| f(d, tier));
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Chef's Choice: choose a minion; get a different minion of the same type. By hand
        // only a minion with a type can be chosen; an effect's cast can land on a typeless
        // one (`effect_target_spec`) and then gives a Tavern Coin.
        (
            "BG28_518",
            target_spell(TargetSpec::AnyTyped, |c, me, t| {
                let (r, card) = match c.find_any(t) {
                    Some(m) => (m.races, db().normal_of(m.card)),
                    None => return,
                };
                let r = target_type(c, r);
                if r.is_none() {
                    c.give_card(me.side, cid("BG28_521t"));
                    return;
                }
                let tier = c.tier(me.side);
                if let Some(x) =
                    c.random_minion(|d| d.races.intersects(r) && d.cid != card && d.tier <= tier)
                {
                    c.give_card(me.side, x);
                } else {
                    c.give_card(me.side, cid("BG28_521t"));
                }
            }),
        ),
        // Hasty Excavation: gain 1 Gold; costs Health to buy instead of Gold.
        (
            "BG28_571",
            Script {
                spell: Some(|c, me, _| c.gain_gold(me.side, 1)),
                flags: flag::HEALTH_COST,
                ..E
            },
        ),
        // Search Through Time: Discover a minion of your Tier; lock it for 1 turn
        // (`after_discover`).
        (
            "BG34_330",
            plain(|c, me, _| discover_where(c, me, |d, t| d.tier == t)),
        ),
        // Tricky Trousers: give a minion +1/+2 and Taunt; if it already has Taunt, remove it.
        (
            "BG28_520",
            Script {
                // "Give a minion +1/+2 and Taunt. If it already has Taunt, remove it."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 1, 2)),
                ..target_spell(ANY, |c, me, t| {
                    let taunt = c.find_any(t).is_some_and(|m| m.has(Kw::TAUNT));
                    spell_buff(c, me, t, 1, 2);
                    if taunt {
                        if let Some(m) = c.find_any_mut(t) {
                            m.remove(Kw::TAUNT);
                        }
                    } else {
                        c.give_kw(t, Kw::TAUNT);
                    }
                })
            },
        ),
        // Planar Telescope: Discover a minion of your most common type.
        (
            "BG28_521",
            plain(|c, me, _| {
                let Some(r) = c.most_common_race(me.side) else {
                    c.give_card(me.side, cid("BG28_521t"));
                    return;
                };
                let tier = c.tier(me.side);
                let options =
                    c.minion_options(me.side, 3, |d| d.races.intersects(r) && d.tier <= tier);
                if options.is_empty() {
                    c.give_card(me.side, cid("BG28_521t"));
                } else {
                    c.offer_discover(me.side, me.card, options);
                }
            }),
        ),
        ("BG28_521t", plain(|c, me, _| c.gain_gold(me.side, 1))),
        // Robust Evolution: transform a minion into a random minion of a higher Tier; it keeps
        // its stats.
        (
            "BG30_804",
            target_spell(ANY, |c, me, t| {
                let Some((tier, a, h, pool)) = c
                    .find_any(t)
                    .map(|m| (m.tier, m.atk, m.max_hp, m.pool_entries()))
                else {
                    return;
                };
                // At most Tier 6 (a Tier 7 minion becomes a Tier 6 one).
                let Some(new) = c.random_minion(|d| d.tier == (tier + 1).min(6)) else {
                    return;
                };
                for (card, copies) in pool {
                    c.g.pool.put(card, copies);
                }
                let mut m = c.new_minion(me.side, new);
                m.set_stats(a, h);
                c.take_pool_copy(&mut m);
                if let Some(slot) = c.find_any_mut(t) {
                    m.eid = slot.eid;
                    *slot = m;
                }
                c.unfreeze_shop(t); // a transformed Tavern card isn't frozen
            }),
        ),
        // Cloning Conch: get a random Murloc and a copy of it.
        (
            "BG28_601",
            plain(|c, me, _| {
                let tier = c.tier(me.side);
                if let Some(m) =
                    c.random_minion(|d| d.races.intersects(Races::MURLOC) && d.tier <= tier)
                {
                    c.give_card(me.side, m);
                    c.give_card(me.side, m);
                }
            }),
        ),
        // Boon of Beetles: when you have space in combat, summon two 2/2 Beetles with Taunt
        // (stacks = Beetles still owed; they carry over until summoned).
        (
            "BG28_603",
            Script {
                spell: Some(|c, me, _| c.add_player_effect(me.side, me.card, 2)),
                flags: flag::ONE_PENDING,
                start_of_combat: Some(|c, me| {
                    for _ in 0..me.data.max(0) {
                        let mut b = c.new_minion(me.side, cid(BEETLE));
                        b.give(Kw::TAUNT);
                        c.when_space(me.side, crate::engine::SpaceReq::Tracked(b, me.card));
                    }
                }),
                // "When you have space in combat, summon two 2/2 Beetles and give them Taunt.
                // (2 left!)": the Beetles with their "this game" bonus (`apply_bonuses`). A
                // card that isn't cast yet owes both.
                shown: Some(|c, sh| {
                    let (a, h) = c
                        .player_ref(sh.side)
                        .map_or((0, 0), |p| (p.stats.beetle_atk, p.stats.beetle_hp));
                    vec![2 + a, 2 + h, 2]
                }),
                ..E
            },
        ),
        // Natural Blessing: choose a minion; all minions that share a type with it (yours and
        // the Tavern's) get +2/+1.
        (
            "BG28_845",
            Script {
                // "Choose a minion. Give all minions that share a type with it +2/+1." The
                // client offers only minions with a type (a typeless Balinda is never a
                // target).
                shown: Some(|c, sh| shown_spell_stats(c, sh, 2, 1)),
                ..target_spell(TargetSpec::AnyTyped, |c, me, t| {
                    // Every type it has: aimed at an Amalgam, every minion with a type shares one
                    // with it (Gatekeeper Amalgam; it was narrowed to one random type like "its
                    // type").
                    let r = c.find_any(t).map(|m| m.races).unwrap_or(Races::NONE);
                    if r.is_none() {
                        return;
                    }
                    let mut all = c.friendly_where(me.side, |m| m.races.intersects(r));
                    all.extend(
                        c.tavern_minions(me.side)
                            .into_iter()
                            .filter(|s| c.find_any(*s).is_some_and(|m| m.races.intersects(r))),
                    );
                    // Blessing Portrait: minions in hand too.
                    if has_trinket(c, me.side, "BG32_MagicItem_894") {
                        all.extend(
                            c.hand_minions(me.side)
                                .into_iter()
                                .filter(|x| x.2.intersects(r))
                                .map(|x| x.0),
                        );
                    }
                    for x in all {
                        spell_buff(c, me, x, 2, 1);
                    }
                })
            },
        ),
        // Temperature Shift: get a Fire Baller and a Snow Baller.
        (
            "BG31_819",
            plain(|c, me, _| {
                give_n(c, me.side, "BG31_816", 1);
                give_n(c, me.side, "BG31_818", 1);
            }),
        ),
        // Boundless Potential: Choose One - Discover a minion of your Tier; or a Tavern spell of
        // your Tier.
        (
            "BG31_890",
            Script {
                choose_one: true,
                spell: Some(|c, me, _| {
                    let tier = c.tier(me.side);
                    if me.opt_a() {
                        c.discover_minion(me.side, me.card, 3, |d| d.tier == tier);
                    }
                    if me.opt_b() {
                        c.discover_spell(me.side, me.card, 3, |d| d.tier == tier);
                    }
                }),
                ..E
            },
        ),
        // Tomb Turning: Discover an Undead; it dies if you play it this turn (`after_discover`).
        (
            "BG34_888",
            plain(|c, me, _| {
                discover_where(c, me, |d, t| {
                    d.races.intersects(Races::UNDEAD) && d.tier <= t
                })
            }),
        ),
        // Eonar's Favor: choose a minion; minions of its type in the Tavern have +3/+3 this game.
        (
            "BG35_912",
            Script {
                // "Choose a minion. Give minions of its type in the Tavern +3/+3 this game."
                // Only minions with a type can be chosen (client targets, as Natural Blessing).
                shown: Some(|c, sh| shown_spell_stats(c, sh, 3, 3)),
                ..target_spell(TargetSpec::AnyTyped, |c, me, t| {
                    let r = races_of(c, t);
                    if r.is_none() {
                        return;
                    }
                    // With the Tavern-spell extra.
                    let (ea, eh) = c.spell_extra(me.side);
                    if let Some(p) = c.player(me.side) {
                        p.stats
                            .tavern_race
                            .push((r.0, ea.saturating_add(3), eh.saturating_add(3)));
                    }
                    c.buff_tavern(me.side, ea.saturating_add(3), eh.saturating_add(3), r);
                })
            },
        ),
        // Weapons Forge: get 3 Pointy Arrows.
        (
            "BG36_884",
            plain(|c, me, _| give_n(c, me.side, "EBG_Spell_014", 3)),
        ),
        // Pointy Arrow: give a minion +4 Attack.
        (
            "EBG_Spell_014",
            Script {
                // "Give a minion +4 Attack."
                shown: Some(|c, sh| vec![c.spell_extra(sh.side).0.saturating_add(4)]),
                ..target_spell(ANY, |c, me, t| spell_buff(c, me, t, 4, 0))
            },
        ),
        // Upper Hand: Start of Combat: set a random enemy minion's Health to 1. One at a time: cast
        // again while it is pending (a Magicfin Apprentice with Brann) it does nothing, as Boon of
        // Beetles.
        (
            "BG28_573",
            one_pending(next_combat(|c, me| {
                let foes: Vec<EntityId> = c.boards[1 - me.side]
                    .iter()
                    .filter(|m| m.alive())
                    .map(|m| m.eid)
                    .collect();
                if let Some(&t) = c.g.rng.pick(&foes)
                    && let Some(m) = c.minion_mut(t)
                {
                    m.hp = 1;
                    m.max_hp = 1;
                }
            })),
        ),
        // Saloon's Finest: Refresh the Tavern with Tavern spells.
        (
            "BG28_849",
            plain(|c, me, _| c.refresh_shop_with(me.side, Races::NONE, true)),
        ),
        // Contracted Corpse / Hired Headhunter: Discover a Deathrattle / Battlecry minion.
        (
            "BG28_882",
            plain(|c, me, _| {
                discover_where(c, me, |d, t| d.mech.has(Mech::DEATHRATTLE) && d.tier <= t)
            }),
        ),
        (
            "BG28_GIL_836",
            plain(|c, me, _| {
                discover_where(c, me, |d, t| d.mech.has(Mech::BATTLECRY) && d.tier <= t)
            }),
        ),
        // Forest's Bounty: Choose One - give a minion +6/+6 twice; or your minions +2/+2.
        (
            "BG31_886",
            Script {
                choose_one: true,
                spell: Some(|c, me, t| {
                    if me.opt_a() && t != 0 {
                        spell_buff(c, me, t, 6, 6);
                        spell_buff(c, me, t, 6, 6);
                    }
                    if me.opt_b() {
                        for x in c.friendly(me.side) {
                            spell_buff(c, me, x, 2, 2);
                        }
                    }
                }),
                spell_target: ANY,
                // "Choose One - Give a minion +6/+6 twice; or Give your minions +2/+2."
                shown: Some(|c, sh| super::spells::shown_spell_pairs(c, sh, &[(6, 6), (2, 2)])),
                ..E
            },
        ),
        // Brood of Nozdormu: Start of Combat: double your left-most minion's Attack.
        (
            "BG34_889",
            next_combat(|c, me| {
                if let Some(t) = c.leftmost(me.side)
                    && let Some(a) = c.minion(t).map(|m| m.atk)
                {
                    c.buff(t, a, 0);
                }
            }),
        ),
        // Channel the Devourer: sell a friendly minion; give its stats to a random friendly
        // minion.
        (
            "EBG_Spell_032",
            target_spell(TargetSpec::Friendly, |c, me, t| {
                let Some((_, i)) = c.locate(t) else { return };
                let (a, h) = (c.boards[me.side][i].atk, c.boards[me.side][i].max_hp);
                c.sell_minion(me.side, i);
                if let Some(x) = c.random_friendly(me.side, |_| true) {
                    spell_buff(c, me, x, a, h);
                }
            }),
        ),
        // Eyes of the Earth Mother: make a friendly minion from Tier 4 or below Golden.
        (
            "EBG_Spell_017",
            target_spell(TargetSpec::FriendlyNonGoldenMaxTier(4), |c, _, t| {
                if let Some(m) = c.minion_mut(t) {
                    make_golden(m);
                }
            }),
        ),
        // Lost Staff of Hamuul: choose a minion; Refresh the Tavern with minions of its type.
        (
            "EBG_Spell_038",
            // Lost Staff must target a minion with a type.
            target_spell(TargetSpec::AnyTyped, |c, me, t| {
                let r = races_of(c, t);
                c.refresh_shop_with(me.side, r, false);
            }),
        ),
        // Sacred Gift: give a minion Divine Shield.
        (
            "BG28_507",
            target_spell(ANY, |c, _, t| c.give_kw(t, Kw::DIVINE_SHIELD)),
        ),
        // Sharing is Caring: Start of Combat: your left-most minion gains the stats of the
        // nearest (left-most) enemy minion.
        (
            "BG31_889",
            next_combat(|c, me| {
                let foe = c.boards[1 - me.side]
                    .iter()
                    .find(|m| m.alive())
                    .map(|m| (m.atk, m.hp));
                if let (Some(t), Some((a, h))) = (c.leftmost(me.side), foe) {
                    c.buff(t, a, h);
                }
            }),
        ),
        // Hallowed Ritual: Discover a Tier 7 minion.
        (
            "BG31_896",
            plain(|c, me, _| c.discover_tier7(me.side, me.card, 3)),
        ),
        // Menagerie Tableware: give your minions +3/+3; repeat for each different friendly
        // minion type.
        (
            "BG34_272",
            Script {
                // "Give your minions +3/+3. Repeat for each different friendly minion type."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 3, 3)),
                ..plain(|c, me, _| {
                    let n = 1 + distinct_type_count(c, me.side);
                    for _ in 0..n {
                        for x in c.friendly(me.side) {
                            spell_buff(c, me, x, 3, 3);
                        }
                    }
                })
            },
        ),
    ]
}
