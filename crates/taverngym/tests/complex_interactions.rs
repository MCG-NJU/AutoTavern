//! Scenarios of complex card interactions under the 36.6.3 rules.
mod common;
use common::*;
use taverngym::state::ShopSlot;

fn shop(g: &mut Game, ids: &[&str]) {
    g.players[0].shop.clear();
    for id in ids {
        let m = minion(g, 0, id);
        g.players[0]
            .shop
            .push(ShopSlot::new(m.eid, Item::Minion(m)));
    }
}

fn rewards(g: &Game) -> Vec<i32> {
    g.players[0]
        .hand
        .iter()
        .filter_map(|h| match h.item {
            Item::Spell { card, data } if card == cid("TB_BaconShop_Triples_01") => Some(data),
            _ => None,
        })
        .collect()
}

#[test]
fn golden_touch_goggles_magnet_reward_uses_player_tier() {
    let mut g = game(17000);
    g.players[0].tier = 5;
    set_board(&mut g, 0, &["BG31_859"]);
    shop(&mut g, &["BG31_859"]);
    give(&mut g, 0, "BG28_830");
    play_last(&mut g, 0);
    give(&mut g, 0, "BGS_Treasures_039");
    play_last(&mut g, 0);
    assert!(g.players[0].hand[0].item.as_minion().unwrap().golden);
    g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
    assert_eq!(g.players[0].board.len(), 1);
    assert_eq!(rewards(&g), vec![6]);
}

#[test]
fn silent_deliverer_goldens_give_no_triple_reward() {
    // "Battlecry: Get a random Golden Tier 4 minion. It doesn't give a Triple Reward":
    // a Golden minion made by an effect gives a reward by default, this card's does not.
    let mut g = game(17100);
    g.players[0].tier = 6;
    give(&mut g, 0, "BG36_343");
    play_last(&mut g, 0);
    let made: Vec<bool> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .filter(|m| m.golden)
        .map(|m| m.triple_reward)
        .collect();
    assert_eq!(made, vec![false]);
    // Played (whatever it asks for answered with the first option), no reward comes.
    play_last(&mut g, 0);
    for _ in 0..4 {
        if g.players[0].pending.is_empty() {
            break;
        }
        g.apply(0, Action::Choose(0)).unwrap();
    }
    assert!(rewards(&g).is_empty());
}

#[test]
fn triple_magnetic_reward_does_not_use_the_magnets_tier() {
    let mut g = game(17001);
    g.players[0].tier = 5;
    set_board(&mut g, 0, &["BG31_859"]);
    let mut m = minion(&mut g, 0, "BG31_859_G");
    m.triple_reward = true;
    g.with_recruit(0, |c| {
        c.add_to_hand(0, Item::Minion(m));
    });
    g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
    assert_eq!(rewards(&g), vec![6]);
}

#[test]
fn elemental_magnet_on_mech_triggers_vine_climber_and_ichoron() {
    let mut g = game(17002);
    set_board(&mut g, 0, &["BG31_177", "BG33_334", "BG31_812"]);
    give(&mut g, 0, "BG31_859");
    let before = stats(&g, 0)[0];
    let module = g.players[0].hand[0].item.as_minion().unwrap();
    let (a, h) = (module.atk, module.hp);
    g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
    // Interpreter's +3/+1 once, Climber's first-play +4/+2 once.
    assert_eq!(stats(&g, 0)[0], (before.0 + a + 7, before.1 + h + 3));
    assert!(g.players[0].board[0].has(Kw::DIVINE_SHIELD));
}

#[test]
fn mech_magnet_on_elemental_does_not_fake_an_elemental_play() {
    let mut g = game(17003);
    set_board(&mut g, 0, &["BG31_859", "BG31_812", "BG31_177"]);
    give(&mut g, 0, "BG26_146");
    g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
    assert!(!g.players[0].board[0].has(Kw::DIVINE_SHIELD));
    assert_eq!(g.players[0].stats.elementals_played, 0);
}

#[test]
fn butchering_multicast_requires_a_live_target_for_each_buff() {
    for balinda in ["BG35_883", "BG35_883_G"] {
        let mut g = game(17004);
        set_board(&mut g, 0, &["BG_ICC_026t", "BG_ICC_026t", balinda]);
        give(&mut g, 0, "BG28_604");
        cast_on(&mut g, 0, 0);
        assert_eq!(g.players[0].stats.undead_atk, 8, "{balinda}");
    }
}

#[test]
fn two_apprentices_do_not_treat_one_play_as_two_plays() {
    let mut g = game(17005);
    g.players[0].tier = 4;
    set_board(
        &mut g,
        0,
        &["TB_BaconShop_HERO_71_Buddy", "TB_BaconShop_HERO_71_Buddy_G"],
    );
    let before = stats(&g, 0);
    give(&mut g, 0, "BG31_859");
    play_last(&mut g, 0);
    assert_eq!(&stats(&g, 0)[..2], before.as_slice());
    give(&mut g, 0, "BG31_859_G");
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0)[0], (before[0].0 + 12, before[0].1 + 12));
}

#[test]
fn maxwell_after_power_swap_does_not_give_another_maxwell() {
    let mut g = game(17006);
    g.players[0].hero = Some(cid("TB_BaconShop_HERO_40"));
    g.players[0].set_hero_power(0, cid("TB_BaconShop_HP_035"));
    set_board(&mut g, 0, &["TB_BaconShop_HERO_40_Buddy"]);
    g.apply(0, Action::Sell(0)).unwrap();
    assert!(
        g.players[0]
            .hand
            .iter()
            .any(|h| h.item.card() == cid("TB_BaconShop_HERO_34_Buddy"))
    );
    assert!(
        !g.players[0]
            .hand
            .iter()
            .any(|h| h.item.card() == cid("TB_BaconShop_HERO_40_Buddy"))
    );
}

#[test]
fn triple_reward_with_exhausted_tier_gets_a_three_gold_pouch() {
    let mut g = game(17007);
    g.universe.retain(|c| db().get(*c).tier != 6);
    g.with_recruit(0, |c| {
        c.add_to_hand(
            0,
            Item::Spell {
                card: cid("TB_BaconShop_Triples_01"),
                data: 6,
            },
        );
    });
    play_last(&mut g, 0);
    assert!(g.players[0].pending.is_empty());
    assert!(
        g.players[0]
            .hand
            .iter()
            .any(|h| h.item.card() == cid("BG27_Anomaly_574t2"))
    );
}

#[test]
fn golden_maw_caster_with_golden_brann_discovers_only_for_one_destroy() {
    let mut g = game(17008);
    g.players[0].tier = 6;
    set_board(&mut g, 0, &["BG_ICC_026t", "TB_BaconUps_045"]);
    give(&mut g, 0, "BG32_340_G");
    cast_on(&mut g, 0, 1);
    assert_eq!(g.players[0].pending.len(), 2);
    assert_eq!(g.players[0].stats.battlecries_triggered, 3);
}

#[test]
fn embalmer_counts_magnetic_plays_once_and_respects_existing_reborn() {
    let mut g = game(17009);
    set_board(&mut g, 0, &["BG31_177", "BG34_Giant_332_G"]);
    for expected in [1, 1] {
        give(&mut g, 0, "BG26_146");
        g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
        assert!(g.players[0].board[0].has(Kw::REBORN));
        assert_eq!(g.players[0].board[1].counter, expected);
    }
    give(&mut g, 0, "BG31_177");
    play_last(&mut g, 0);
    assert!(g.players[0].board[2].has(Kw::REBORN));
    assert_eq!(g.players[0].board[1].counter, 2);
}

#[test]
fn souvenir_aura_counts_both_copies_and_is_idempotent() {
    let mut g = game(17010);
    let t = cid("BG30_MagicItem_989t");
    g.players[0].trinkets = vec![t, t.copy()];
    set_board(&mut g, 0, &["BG_ICC_026t"]);
    assert_eq!(
        g.players[0].board[0].atk,
        db().get(cid("BG_ICC_026t")).atk + 40
    );
    let before = stats(&g, 0);
    g.with_recruit(0, |c| c.bump_player_stat(0, |_| {}));
    assert_eq!(stats(&g, 0), before);
}

#[test]
fn souvenir_free_purchases_consume_independent_allowances() {
    let mut g = game(17011);
    let t = cid("BG36_MagicItem_202");
    g.players[0].trinkets = vec![t, t.copy()];
    shop(
        &mut g,
        &["BG25_309", "BG25_309", "BG25_309", "BG25_309", "BG25_309"],
    );
    for _ in 0..4 {
        assert_eq!(g.slot_price(0, 0).unwrap().0, 0);
        g.apply(0, Action::Buy(0)).unwrap();
    }
    assert_eq!(g.slot_price(0, 0).unwrap().0, 3);
}

#[test]
fn souvenir_extra_triggers_stack_with_rewards_and_brann() {
    let mut g = game(17012);
    let sticker = cid("BG32_MagicItem_367");
    g.players[0].trinkets = vec![sticker, sticker.copy()];
    g.players[0].rewards.push(cid("BG24_Reward_130"));
    set_board(&mut g, 0, &["TB_BaconUps_045"]);
    g.with_recruit(0, |c| assert_eq!(c.end_of_turn_mult(0), 4));
    let eye = cid("BG36_MagicItem_215");
    g.players[0].trinkets = vec![eye, eye.copy()];
    give(&mut g, 0, "BG34_634t");
    play_last(&mut g, 0);
    assert_eq!(g.players[0].stats.battlecries_triggered, 5);
}

#[test]
fn holy_vanguard_threshold_copy_and_healing_preserve_only_real_buffs() {
    let mut g = game(17013);
    g.players[0].health = 16;
    g.players[0].armor = 0;
    set_board(&mut g, 0, &["BG36_372_G"]);
    let base = stats(&g, 0)[0];
    g.with_recruit(0, |c| {
        let e = c.boards[0][0].eid;
        c.buff(e, 7, 9);
        c.hero_damage(0, 1);
        assert_eq!(
            (c.boards[0][0].atk, c.boards[0][0].max_hp),
            (base.0 + 67, base.1 + 69)
        );
        let copy = c.full_copy(&c.boards[0][0].clone());
        c.add_to_hand(0, Item::Minion(copy));
    });
    assert_eq!(
        g.players[0].hand[0].item.as_minion().unwrap().atk,
        base.0 + 67
    );
    g.with_recruit(0, |c| {
        c.player(0).unwrap().health = 16;
    });
    assert_eq!(stats(&g, 0)[0], (base.0 + 7, base.1 + 9));
    assert_eq!(
        g.players[0].hand[0].item.as_minion().unwrap().atk,
        base.0 + 7
    );
    g.with_recruit(0, |c| {
        c.player(0).unwrap().health = 15;
    });
    let mut c = combat_ctx(&mut g);
    c.bump_player_stat(0, |_| {});
    assert_eq!(c.boards[0][0].atk, base.0 + 67);
}

#[test]
fn sinestra_health_gain_reaches_titanic_guardian_without_recursive_growth() {
    let mut g = game(17014);
    set_board(
        &mut g,
        0,
        &[
            "BG_ICC_026t",
            "TB_BaconShop_HERO_39_Buddy_G",
            "TB_BaconShop_HERO_52_Buddy",
        ],
    );
    let mut c = combat_ctx(&mut g);
    let base = c.boards[0][1].hp;
    let target = c.boards[0][0].eid;
    c.buff(target, 3, 4);
    assert_eq!(c.boards[0][1].hp, base + 10);
    assert_eq!(
        c.g.players[0].board[0].hp,
        db().get(cid("BG_ICC_026t")).hp + 1
    );
}

#[test]
fn cleave_venom_is_not_consumed_when_every_target_has_divine_shield() {
    let mut g = game(17015);
    set_board(&mut g, 0, &["BG_ICC_026t"]);
    set_board(&mut g, 1, &["BG_ICC_026t", "BG_ICC_026t", "BG_ICC_026t"]);
    g.players[0].board[0].give(Kw(Kw::CLEAVE.0 | Kw::VENOMOUS.0));
    g.players[0].board[0].add_stats(0, 100);
    for m in &mut g.players[1].board {
        m.give(Kw::DIVINE_SHIELD);
    }
    let mut c = combat_ctx(&mut g);
    let attacker = c.boards[0][0].eid;
    taverngym::combat::attack_now(&mut c, 0, attacker);
    assert!(c.minion(attacker).unwrap().has(Kw::VENOMOUS));
    taverngym::combat::attack_now(&mut c, 0, attacker);
    assert!(!c.minion(attacker).unwrap().has(Kw::VENOMOUS));
}

#[test]
fn advanced_construction_itself_does_not_spend_the_free_upgrade() {
    let mut g = game(17016);
    g.players[0].tier = 7;
    g.players[0].set_hero_power(0, cid("BG31_HERO_801p"));
    set_board(&mut g, 0, &["BG31_HERO_801pt"]);
    g.players[0].board[0].attached.push(cid("BG31_HERO_801ptf"));
    g.players[0].shop.clear();
    let sid = g.alloc_eid();
    g.players[0]
        .shop
        .push(ShopSlot::new(sid, Item::spell(cid("BG31_HERO_801ptf4"))));
    g.apply(0, Action::Buy(0)).unwrap();
    assert_eq!(g.spell_price(0, cid("BG31_HERO_801pta7")), 0);
}

#[test]
fn upgraded_matrix_preserves_a_three_hit_shield() {
    use taverngym::event::Event;
    let mut g = game(17017);
    set_board(&mut g, 0, &["BG_ICC_026t"]);
    g.players[0].secrets.push(cid("TB_Bacon_Secrets_07b"));
    g.players[0].board[0].shield_max = 3;
    g.players[0].board[0].give(Kw::DIVINE_SHIELD);
    let mut c = combat_ctx(&mut g);
    let defender = c.boards[0][0].eid;
    c.emit(Event::Attacks {
        side: 1,
        attacker: 0,
        defender,
    });
    assert_eq!(c.minion(defender).unwrap().shield_hits, 3);
    assert_eq!(c.minion(defender).unwrap().shield_max, 3);
}

#[test]
fn surprise_triple_inherits_only_extra_stats_and_returns_each_original_species() {
    let main = cid("BG26_537");
    let surprise = cid("BG26_175");
    let mut g = game(17018);
    // These legacy Elementals are debug-reachable. Add real pool copies to exercise the
    // generic mixed-triple accounting without changing the production 36.6.3 pool.
    for card in [main, surprise] {
        if !g.universe.contains(&card) {
            g.universe.push(card);
            let n = g.cfg.pool_copies_by_tier[(db().get(card).tier as usize - 1).min(6)];
            g.pool.put(card, n as u16);
        }
    }
    g.cfg.triple_copies = 3;
    let before = (g.pool.get(main), g.pool.get(surprise));
    g.with_recruit(0, |c| {
        for card in [main, main, surprise] {
            let mut m = c.new_minion(0, card);
            c.take_pool_copy(&mut m);
            assert_eq!(m.pool_copies, 1);
            m.add_stats(3, 5);
            c.add_to_hand(0, Item::Minion(m));
        }
    });
    let m = g.players[0].hand[0].item.as_minion().unwrap();
    let base = db().get(db().golden_of(main));
    assert_eq!((m.atk, m.max_hp), (base.atk + 9, base.hp + 15));
    assert_eq!(m.pool_copies, 3);
    assert_eq!(m.pool_parts, vec![(main, 2), (surprise, 1)]);
    let snapshot = serde_json::to_string(&g).unwrap();
    let mut g: Game = serde_json::from_str(&snapshot).unwrap();
    play_last(&mut g, 0);
    g.apply(0, Action::Sell(0)).unwrap();
    assert_eq!((g.pool.get(main), g.pool.get(surprise)), before);
    assert!(taverngym::invariants::pool_species_violations(&g).is_empty());
}

#[test]
fn magnetic_enchantments_preserve_multi_shield_gems_and_gained_deathrattles() {
    let mut g = game(17019);
    set_board(&mut g, 0, &["BG31_177"]);
    let mut module = minion(&mut g, 0, "BG26_146");
    module.shield_max = 3;
    module.give(Kw::DIVINE_SHIELD);
    module.gained_dr.push(cid("BGS_018"));
    module.gems = 2;
    module.gem_atk = 8;
    module.gem_hp = 10;
    module.add_stats(8, 10);
    g.with_recruit(0, |c| {
        c.add_to_hand(0, Item::Minion(module));
    });
    g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
    let host = &g.players[0].board[0];
    assert_eq!(host.shield_hits, 3);
    // The magnet's Blood Gems go in with its stats but stay in its Magnetization: the host's own
    // Gems are unchanged.
    assert_eq!((host.gems, host.gem_atk, host.gem_hp), (0, 0, 0));
    assert!(taverngym::script::minion_has(
        host,
        taverngym::script::Hook::Deathrattle
    ));
    assert_eq!(host.gained_dr, vec![cid("BGS_018")]);
}

#[test]
fn stat_history_saturates_after_large_repeated_effects() {
    use taverngym::state::ModKind;
    let mut m = Minion::new(999, cid("BG_ICC_026t"));
    m.record(None, ModKind::Add, i32::MAX, i32::MAX);
    m.record(None, ModKind::Add, i32::MAX, i32::MAX);
    assert_eq!((m.mods[0].atk, m.mods[0].hp), (i32::MAX, i32::MAX));
}

#[test]
fn timewarp_triples_keep_improvement_growth_but_reset_ordinary_progress() {
    // 10-13: improvement growth is inherited in normal-card units; the Mystic's
    // partial sell-three progress remains an ordinary, non-inherited counter.
    for (id, expected) in [
        ("BG34_Giant_088", 6),
        ("BG34_Giant_091", 6),
        ("BG34_Giant_319", 6),
        ("BG34_Giant_320", 0),
    ] {
        let mut g = game(17020);
        g.cfg.triple_copies = 3;
        g.with_recruit(0, |c| {
            for n in [1, 2, 3] {
                let mut m = c.new_minion(0, cid(id));
                m.counter = n;
                c.add_to_hand(0, Item::Minion(m));
            }
        });
        assert_eq!(g.players[0].hand.len(), 1);
        let merged = g.players[0].hand[0].item.as_minion().unwrap();
        assert!(merged.golden, "{id}");
        assert_eq!(merged.card, db().golden_of(cid(id)), "{id}");
        assert_eq!(merged.counter, expected, "{id}");
    }
}

#[test]
fn mukla_uses_independent_big_banana_rolls_for_every_living_recipient() {
    use taverngym::script::{Me, script};
    let mut kinds = std::collections::HashSet::new();
    for seed in 17021..17041 {
        let mut g = game(seed);
        let mut rng = g.rng.clone();
        let expected: Vec<CardId> = (0..9)
            .map(|_| {
                cid(if rng.chance(33, 100) {
                    "BG_TB_006"
                } else {
                    "BG_EX1_014t"
                })
            })
            .collect();
        let power = cid("TB_BaconShop_HP_038");
        g.with_recruit(0, |c| {
            script(power).unwrap().start_of_turn.unwrap()(
                c,
                Me {
                    side: 0,
                    eid: 0,
                    card: power,
                    golden: false,
                    opt: 0,
                    pos: 0,
                    data: 0,
                },
            );
        });
        let actual: Vec<CardId> = g
            .players
            .iter()
            .flat_map(|p| p.hand.iter().map(|h| h.item.card()))
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(g.players[0].hand.len(), 2);
        assert!(g.players.iter().skip(1).all(|p| p.hand.len() == 1));
        kinds.extend(actual);
    }
    assert_eq!(kinds.len(), 2);
}

#[test]
fn bananas_with_balinda_apply_normal_spell_stats_without_tavern_spell_extra() {
    let mut g = game(17041);
    set_board(&mut g, 0, &["BG35_883_G", "BG_ICC_026t"]);
    g.players[0].stats.spell_atk = 99;
    g.players[0].stats.spell_hp = 99;
    let before = stats(&g, 0)[1];
    for id in ["BG_EX1_014t", "BG_TB_006"] {
        give(&mut g, 0, id);
        cast_on(&mut g, 0, 1);
    }
    assert_eq!(stats(&g, 0)[1], (before.0 + 9, before.1 + 9));
    assert_eq!(g.players[0].stats.spells_cast, 6);
    assert_eq!(g.players[0].stats.tavern_spells_cast, 0);
}

#[test]
fn chef_and_telescope_provide_consolation_when_no_type_is_available() {
    let mut g = game(17042);
    // No type: by hand Chef's Choice cannot be aimed at it, but an effect's cast can land there and
    // then gives the consolation coin.
    set_board(&mut g, 0, &["BG_LOE_077"]);
    let brann = g.players[0].board[0].eid;
    g.with_recruit(0, |c| c.you_cast(0, cid("BG28_518"), 0, brann, 0));
    give(&mut g, 0, "BG28_521");
    play_last(&mut g, 0);
    assert_eq!(
        g.players[0]
            .hand
            .iter()
            .filter(|h| h.item.card() == cid("BG28_521t"))
            .count(),
        2
    );
    let before = g.players[0].gold;
    play_last(&mut g, 0);
    assert_eq!(g.players[0].gold, before + 1);
}

#[test]
fn radio_star_copies_full_enchantments_when_the_killer_dies_in_the_same_exchange() {
    let mut g = game(17043);
    set_board(&mut g, 0, &["BG34_Giant_330"]);
    set_board(&mut g, 1, &["BG_ICC_026t"]);
    let mut c = combat_ctx(&mut g);
    let star = c.boards[0][0].eid;
    let killer = c.boards[1][0].eid;
    c.boards[1][0]
        .attached
        .push(cid("BG36_MidGameEffect_000t18"));
    c.boards[1][0].gained_dr.push(cid("BGS_018"));
    c.boards[1][0].gems = 3;
    c.boards[1][0].set_stats(20, 20);
    c.boards[1][0].give(Kw::TAUNT);
    c.boards[0][0].killed_by = killer;
    c.destroy(star);
    c.destroy(killer);
    c.resolve_deaths();
    let copy = c.g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((copy.atk, copy.hp), (20, 20));
    assert!(copy.has(Kw::TAUNT));
    assert_eq!(copy.gems, 3);
    assert_eq!(copy.attached, vec![cid("BG36_MidGameEffect_000t18")]);
    assert_eq!(copy.gained_dr, vec![cid("BGS_018")]);
    assert!(!copy.destroyed);
}

#[test]
fn tumbling_disaster_starts_at_one_and_improves_by_one_per_four_deaths() {
    let mut g = game(17044);
    g.players[0].rewards.push(cid("BG28_Reward_505"));
    set_board(
        &mut g,
        0,
        &["BG_ICC_026t", "BG_ICC_026t", "BG_ICC_026t", "BG_ICC_026t"],
    );
    let mut c = combat_ctx(&mut g);
    let first = c.new_minion(0, cid("BG_ICC_026t"));
    let base = (first.atk, first.hp);
    let e = c.summon(0, 4, first).unwrap();
    assert_eq!(
        (c.minion(e).unwrap().atk, c.minion(e).unwrap().hp),
        (base.0 + 1, base.1 + 1)
    );
    let dead: Vec<_> = c.boards[0][..4].iter().map(|m| m.eid).collect();
    for eid in dead {
        c.destroy(eid);
        c.resolve_deaths();
    }
    let next = c.new_minion(0, cid("BG_ICC_026t"));
    let e = c.summon(0, 1, next).unwrap();
    assert_eq!(
        (c.minion(e).unwrap().atk, c.minion(e).unwrap().hp),
        (base.0 + 2, base.1 + 2)
    );
}

#[test]
fn battlecry_transformation_preserves_the_played_identity_across_a_snapshot() {
    let mut g = game(17045);
    set_board(&mut g, 0, &["BG31_859_G"]);
    let target = g.players[0].board[0].eid;
    give(&mut g, 0, "BG_EX1_564");
    g.apply(0, Action::Play { hand: 0, pos: 1 }).unwrap();
    let snapshot = serde_json::to_string(&g).unwrap();
    let mut g: Game = serde_json::from_str(&snapshot).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
        panic!("target choice")
    };
    let choice = options.iter().position(|eid| *eid == target).unwrap();
    g.apply(0, Action::Choose(choice)).unwrap();
    assert_eq!(g.players[0].board[1].card, cid("BG31_859_G"));
    assert_eq!(g.players[0].stats.cards_played, 1);
    assert_eq!(g.players[0].stats.elementals_played, 0);
    assert_eq!(g.players[0].stats.golden_played, 0);
    assert_eq!(g.players[0].stats.battlecries_triggered, 1);
}

#[test]
fn timewarp_skippers_generate_only_for_tier_two_sales_with_independent_counts() {
    let mut g = game(17046);
    set_board(
        &mut g,
        0,
        &[
            "BG34_Giant_072",
            "BG34_Giant_072_G",
            "BG21_015",
            "BG_ICC_026t",
        ],
    );
    g.apply(0, Action::Sell(2)).unwrap();
    assert_eq!(g.players[0].hand.len(), 3);
    assert!(
        g.players[0]
            .hand
            .iter()
            .all(|h| db().get(h.item.card()).tier == 1)
    );
    g.apply(0, Action::Sell(2)).unwrap();
    assert_eq!(g.players[0].hand.len(), 3);
}

#[test]
fn timewarp_saurolisks_count_repeated_deathrattles_permanently_for_the_owner() {
    let mut g = game(17047);
    set_board(
        &mut g,
        0,
        &["BG34_Giant_202", "BG34_Giant_202_G", "BGS_018", "BG25_354"],
    );
    let before = stats(&g, 0);
    let mut c = combat_ctx(&mut g);
    let dying = c.boards[0][2].eid;
    c.destroy(dying);
    c.resolve_deaths();
    assert_eq!(c.g.players[0].board[0].atk, before[0].0 + 6);
    assert_eq!(c.g.players[0].board[0].max_hp, before[0].1 + 4);
    assert_eq!(c.g.players[0].board[1].atk, before[1].0 + 12);
    assert_eq!(c.g.players[0].board[1].max_hp, before[1].1 + 8);
    let after = c.g.players[0].board[0].atk;
    c.emit(taverngym::event::Event::DeathrattleTriggered {
        side: 1,
        eid: dying,
    });
    assert_eq!(c.g.players[0].board[0].atk, after);
}

#[test]
fn golden_timewarp_murk_eye_stacks_with_brann_for_all_friendly_battlecries() {
    let mut g = game(17048);
    set_board(
        &mut g,
        0,
        &["BG34_Giant_318_G", "BG_LOE_077", "BG25_011", "BG20_100"],
    );
    let before = stats(&g, 0)[2];
    g.with_recruit(0, |c| {
        c.run_board_hook(0, |s| s.end_of_turn, 1, "end of turn")
    });
    assert_eq!(g.players[0].stats.battlecries_triggered, 8);
    assert_eq!(stats(&g, 0)[2].0, before.0 + 4);
    assert_eq!(stats(&g, 0)[2].1, before.1);
    assert_eq!(g.players[0].hand.len(), 8);
}

#[test]
fn timewarp_whirl_copies_attached_and_gained_deathrattles_without_recursing() {
    let mut g = game(17049);
    set_board(
        &mut g,
        0,
        &[
            "BG34_Giant_599_G",
            "BG34_Giant_599",
            "BGS_018",
            "BG_ICC_026t",
        ],
    );
    g.players[0].board[2].attached.push(cid("BG36_854"));
    g.players[0].board[3].gained_dr.push(cid("BGS_018"));
    let mut c = combat_ctx(&mut g);
    c.run_board_hook(0, |s| s.start_of_combat, 1, "start of combat");
    let expected = vec![cid("BGS_018"), cid("BG36_854"), cid("BGS_018")];
    assert_eq!(c.boards[0][0].gained_dr, expected.repeat(2));
    assert_eq!(c.boards[0][1].gained_dr, expected);
    assert!(c.g.players[0].board[0].gained_dr.is_empty());
}

#[test]
fn golden_timewarp_lei_follows_the_primary_power_after_finley_swaps_it() {
    let mut g = game(17050);
    g.players[0].hero = Some(cid("TB_BaconShop_HERO_40"));
    g.players[0].set_hero_power(0, cid("TB_BaconShop_HP_038"));
    set_board(&mut g, 0, &["BG34_Giant_602_G"]);
    g.with_recruit(0, |c| {
        c.run_board_hook(0, |s| s.start_of_turn, 1, "start of turn")
    });
    assert_eq!(g.players[0].hand.len(), 2);
    assert!(
        g.players[0]
            .hand
            .iter()
            .all(|h| h.item.card() == cid("TB_BaconShop_HERO_38_Buddy"))
    );
}

#[test]
fn unlocked_aranna_power_survives_snapshot_and_only_first_minion_is_free() {
    let mut g = game(17051);
    g.players[0].set_hero_power(0, cid("TB_BaconShop_HP_065t2"));
    g.players[0].gold = 10;
    shop(&mut g, &["BG31_859", "BG31_859"]);
    let mut g: Game = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
    g.apply(0, Action::Buy(0)).unwrap();
    assert_eq!(g.players[0].gold, 10);
    g.apply(0, Action::Buy(0)).unwrap();
    assert_eq!(g.players[0].gold, 7);
}

#[test]
fn sneeds_new_shredder_deathrattle_does_not_summon_outside_combat() {
    let mut g = game(17052);
    set_board(&mut g, 0, &["BG21_HERO_030t", "BG21_HERO_030t_G"]);
    give(&mut g, 0, "BGS_018");
    give(&mut g, 0, "BG31_859");
    g.with_recruit(0, |c| {
        let first = c.boards[0][0].clone();
        let second = c.boards[0][1].clone();
        c.run_deathrattles(0, &first, 0);
        c.run_deathrattles(0, &second, 1);
        assert_eq!(c.boards[0].len(), 2);
    });
}

#[test]
fn locket_cannot_bypass_earth_mothers_tier_and_non_golden_target_restrictions() {
    let mut g = game(17053);
    set_board(&mut g, 0, &["BG31_859", "BG_LOE_077", "BG31_859_G"]);
    g.players[0].trinkets.push(cid("BG36_MagicItem_211"));
    give(&mut g, 0, "EBG_Spell_017");
    cast_on(&mut g, 0, 0);
    assert!(g.players[0].board[0].golden);
    assert!(!g.players[0].board[1].golden);
    assert!(g.players[0].board[2].golden);
}

#[test]
fn triggered_mummifier_cannot_give_reborn_to_an_already_dying_minion() {
    let mut g = game(17054);
    set_board(&mut g, 0, &["BG28_309", "BG25_008", "BG_ICC_026t"]);
    let mut c = combat_ctx(&mut g);
    let dying = c.boards[0][1].eid;
    let living = c.boards[0][2].eid;
    c.destroy(dying);
    c.trigger_deathrattles(0, c.boards[0][0].eid);
    assert!(!c.on_board(dying));
    assert!(c.minion(living).unwrap().has(Kw::REBORN));
    assert_eq!(c.boards[0].len(), 2);
}

#[test]
fn timewarp_offers_recognize_gained_deathrattles_and_unlocked_primary_powers() {
    let mut g = game(17055);
    g.lobby_races = Races::MECH;
    g.players[0].set_hero_power(0, cid("TB_BaconShop_HP_065t2"));
    set_board(&mut g, 0, &["BG31_177", "BG31_177", "BG31_177"]);
    g.players[0].board[0].gained_dr.push(cid("BGS_018"));
    g.with_recruit(0, |c| {
        let options = taverngym::content::timewarp::discover_minions(c, 0, true, 2, 500);
        assert!(options.contains(&cid("BG34_Giant_599")));
        let minor = taverngym::content::timewarp::discover_minions(c, 0, false, 2, 500);
        assert!(minor.contains(&cid("BG34_Giant_602")));
        c.boards[0][0].gained_dr.clear();
        let options = taverngym::content::timewarp::discover_minions(c, 0, true, 2, 500);
        assert!(!options.contains(&cid("BG34_Giant_599")));
    });
}

#[test]
fn a_triple_of_extremely_large_minions_saturates_instead_of_wrapping() {
    let mut g = game(17056);
    g.cfg.triple_copies = 3;
    set_board(&mut g, 0, &["BG31_859", "BG31_859"]);
    for m in &mut g.players[0].board {
        m.set_stats(2_000_000_000, 2_000_000_000);
    }
    give(&mut g, 0, "BG31_859");
    let merged = g.players[0].hand[0].item.as_minion().unwrap();
    assert!(merged.golden);
    assert_eq!((merged.atk, merged.max_hp), (i32::MAX, i32::MAX));
}

#[test]
fn golden_consume_with_portrait_and_shivarra_saturates_each_recipient() {
    let mut g = game(17057);
    set_board(
        &mut g,
        0,
        &[
            "BG34_500_G",
            "BG34_Giant_311_G",
            "TB_BaconShop_HERO_39_Buddy_G",
        ],
    );
    g.players[0].trinkets.push(cid("BG35_MagicItem_156"));
    shop(&mut g, &["BG31_859"]);
    g.players[0].shop[0]
        .item
        .as_minion_mut()
        .unwrap()
        .set_stats(2_000_000_000, 2_000_000_000);
    g.with_recruit(0, |c| {
        c.run_board_hook(0, |s| s.end_of_turn, 1, "end of turn")
    });
    assert!(
        g.players[0].board[..2]
            .iter()
            .all(|m| (m.atk, m.max_hp) == (i32::MAX, i32::MAX))
    );
    assert_eq!(g.players[0].board[2].max_hp, i32::MAX);
}

#[test]
fn golden_timewarp_poet_keeps_large_combat_gains_without_overflow() {
    let mut g = game(17058);
    set_board(&mut g, 0, &["BG34_Giant_314_G", "BGS_034"]);
    let mut c = combat_ctx(&mut g);
    let target = c.boards[0][1].eid;
    c.buff(target, 1_500_000_000, 1_500_000_000);
    assert_eq!(
        (c.g.players[0].board[1].atk, c.g.players[0].board[1].max_hp),
        (i32::MAX, i32::MAX)
    );
}

#[test]
fn electrode_weights_remaining_copies_and_respects_bobbleheads_floor_and_price() {
    let (a, b, low) = (cid("BG26_147"), cid("BG35_341"), cid("BG26_146"));
    let mut sentinels = 0;
    for seed in 17060..17140 {
        let mut g = game(seed);
        g.players[0].tier = 4;
        g.players[0].trinkets = vec![cid("BG35_MagicItem_743"), cid("BG30_MagicItem_998")];
        g.players[0].shop.clear();
        // Controlled 1:9 remaining supply; the 200 ineligible Tier-1 copies must not enter
        // the sample. This fixture checks the offer, not the game's total pool balance.
        g.universe = vec![a, b, low];
        g.pool.counts[a.0 as usize] = 1;
        g.pool.counts[b.0 as usize] = 9;
        g.pool.counts[low.0 as usize] = 200;
        g.with_recruit(0, |c| c.run_player_hook(0, |s| s.on_fill_shop, "fill shop"));
        assert_eq!(g.players[0].shop.len(), 1);
        let card = g.players[0].shop[0].item.card();
        assert!(card == a || card == b);
        sentinels += usize::from(card == b);
        assert_eq!(g.slot_price(0, 0).unwrap().0, 2);
    }
    assert!(
        sentinels >= 60,
        "1:9 supply must not become a uniform card-kind sample: {sentinels}/80"
    );
    assert!(
        sentinels < 80,
        "both eligible cards retain a nonzero probability"
    );
}

#[test]
fn golden_tychus_excludes_all_purchased_single_purchase_upgrade_families() {
    let mut g = game(17140);
    g.players[0].tier = 6;
    g.players[0].set_hero_power(0, cid("BG31_HERO_801p"));
    for family in ['f', 'h', 'i', 'j'] {
        let prefix = format!("BG31_HERO_801pt{family}");
        let upgrade = db()
            .iter()
            .find(|d| d.id.starts_with(&prefix) && d.tier <= 6)
            .unwrap()
            .cid;
        g.players[0].powers[0].memory.push(upgrade);
    }
    set_board(&mut g, 0, &["BG31_HERO_801_Buddy_G"]);
    for _ in 0..6 {
        give(&mut g, 0, "BG28_897");
        cast_on(&mut g, 0, 0);
    }
    assert_eq!(g.players[0].hand.len(), 6);
    for h in &g.players[0].hand {
        let d = db().get(h.item.card());
        let family =
            d.id.strip_prefix("BG31_HERO_801pt")
                .unwrap()
                .chars()
                .next()
                .unwrap();
        assert!(!['f', 'h', 'i', 'j'].contains(&family));
        assert!(d.tier <= 6);
    }
}

#[test]
fn golden_baby_nzoth_recognizes_gained_deathrattles_without_affecting_plain_hosts() {
    let mut g = game(17141);
    set_board(&mut g, 0, &["BG31_859", "BG31_859"]);
    g.players[0].board[0].gained_dr.push(cid("BGS_018"));
    give(&mut g, 0, "TB_BaconShop_HERO_93_Buddy_G");
    play_last(&mut g, 0);
    assert!(g.players[0].board[0].golden);
    assert!(!g.players[0].board[1].golden);
    assert_eq!(g.players[0].board[0].gained_dr, vec![cid("BGS_018")]);
}

#[test]
fn two_wardens_skip_disabled_opponent_buddies_then_grant_three_valid_copies() {
    let mut g = game(17142);
    set_board(&mut g, 0, &["BG21_HERO_010_Buddy", "BG21_HERO_010_Buddy_G"]);
    g.players[0].next_opponent = Some(1);
    g.players[1].hero = Some(cid("TB_BaconShop_HERO_56"));
    g.with_recruit(0, |c| {
        c.run_board_hook(0, |s| s.start_of_turn, 1, "start of turn")
    });
    assert!(g.players[0].hand.is_empty());
    g.players[1].hero = Some(cid("TB_BaconShop_HERO_34"));
    g.with_recruit(0, |c| {
        c.run_board_hook(0, |s| s.start_of_turn, 1, "start of turn")
    });
    assert_eq!(g.players[0].hand.len(), 3);
    assert!(
        g.players[0]
            .hand
            .iter()
            .all(|h| h.item.card() == cid("TB_BaconShop_HERO_34_Buddy"))
    );
}

#[test]
fn huge_tavern_spell_bonus_saturates_with_stacked_board_and_magnetic_auras() {
    for with_auras in [false, true] {
        let mut g = game(17143);
        set_board(&mut g, 0, &["BG31_859", "BG34_Giant_066"]);
        if with_auras {
            g.players[0].board[0].attached.push(cid("BG35_341_G"));
        } else {
            g.players[0].board.pop();
        }
        g.players[0].stats.spell_atk = i32::MAX - 1;
        g.players[0].stats.spell_hp = i32::MAX - 1;
        give(&mut g, 0, "BG28_897");
        cast_on(&mut g, 0, 0);
        assert_eq!(stats(&g, 0)[0], (i32::MAX, i32::MAX));
    }
}
