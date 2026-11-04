//! 37.0.2 integration regressions: public choices, instance lifetimes and real engine events.
mod common;
use common::*;
use taverngym::{
    content,
    debug::{DebugCmd, run_debug},
    state::ShopSlot,
};

fn fixture(seed: u64) -> Game {
    let mut g = Game::new(
        seed,
        RulesConfig {
            lobby_races: 6,
            lobby_types: [
                "MURLOC",
                "MECHANICAL",
                "PIRATE",
                "NAGA",
                "ELEMENTAL",
                "QUILBOAR",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            hero_pool: vec!["TB_BaconShop_HERO_34".into()],
            start_gold: 100,
            gold_refill_cap: 100,
            triple_copies: 99,
            ..RulesConfig::default()
        },
    );
    for p in 0..8 {
        g.apply(p, Action::Choose(0)).unwrap();
    }
    g.players[0].hand.clear();
    g.players[0].board.clear();
    g
}
fn trinket(g: &mut Game, id: &str) {
    run_debug(
        g,
        DebugCmd::GiveTrinket {
            p: 0,
            card: id.into(),
        },
    )
    .unwrap();
}
fn shop(g: &mut Game, ids: &[&str]) {
    g.with_recruit(0, |c| c.clear_shop(0));
    for id in ids {
        let m = minion(g, 0, id);
        g.players[0]
            .shop
            .push(ShopSlot::new(m.eid, Item::Minion(m)));
    }
}
fn buy_trinket(g: &mut Game, id: &str) {
    g.players[0].pending.push(Pending::Trinket {
        options: vec![cid(id)],
        free: false,
        replace: None,
    });
    g.apply(0, Action::Choose(0)).unwrap();
}
fn choose_id(g: &mut Game, id: &str) {
    let Some(Pending::Trinket { options, .. }) = g.players[0].pending.last() else {
        panic!("trinket menu missing")
    };
    let i = options
        .iter()
        .position(|c| *c == cid(id))
        .expect("trinket offered");
    g.apply(0, Action::Choose(i)).unwrap();
}

#[test]
fn kiri_pays_once_initializes_two_copies_and_preserves_second_power() {
    let mut g = fixture(370201);
    let bob = cid("BG30_MagicItem_998");
    g.trinket_universe = vec![bob, cid("BG36_MagicItem_414")];
    g.with_recruit(0, |c| c.set_hero_power(0, 1, cid("TB_BaconShop_HP_035")));
    let second = g.players[0].powers[1].card;
    let gold = g.players[0].gold;
    buy_trinket(&mut g, "BG36_MagicItem_412");
    let Some(Pending::Trinket { options, free, .. }) = g.players[0].pending.last() else {
        panic!()
    };
    assert!(!free);
    assert_eq!(options, &[bob]); // Greater trinkets never enter the lesser purchase menu.
    choose_id(&mut g, "BG30_MagicItem_998");
    assert_eq!(
        g.players[0].gold,
        gold - db().get(cid("BG36_MagicItem_412")).cost - db().get(bob).cost + 6
    );
    assert_eq!(g.players[0].trinkets, vec![bob]);
    assert_eq!(g.players[0].powers[0].card, bob.instance(1));
    assert_eq!(g.players[0].powers[1].card, second);
    assert_eq!(g.players[0].trinket_count(bob), 2);
    assert!(g.players[0].pending.is_empty());
}

#[test]
fn kiri_batteries_have_two_independent_purchases_each_and_reset_next_turn() {
    let mut g = fixture(370202);
    let batteries = cid("BG32_MagicItem_278");
    g.trinket_universe = vec![batteries];
    buy_trinket(&mut g, "BG36_MagicItem_412");
    choose_id(&mut g, "BG32_MagicItem_278");
    shop(&mut g, &["BG36_345", "BG36_345", "BG36_345"]);
    for expected in [3, 6, 7] {
        g.apply(0, Action::Buy(0)).unwrap();
        assert_eq!(g.players[0].hand.len(), expected);
    }
    g.players[0].hand.clear();
    g.start_turn();
    g.players[0].pending.clear();
    shop(&mut g, &["BG36_345"]);
    g.apply(0, Action::Buy(0)).unwrap();
    assert_eq!(g.players[0].hand.len(), 3);
}

#[test]
fn stand_and_greater_kiri_keep_three_distinct_growing_instances() {
    let mut g = fixture(370203);
    let gauntlet = cid("BG36_MagicItem_414");
    g.trinket_universe = vec![gauntlet];
    trinket(&mut g, "BG30_MagicItem_888");
    buy_trinket(&mut g, "BG36_MagicItem_412t2");
    assert_eq!(g.players[0].pending.len(), 2);
    choose_id(&mut g, "BG36_MagicItem_414");
    // The other Kiri menu was generated before either purchase and remains payable.
    choose_id(&mut g, "BG36_MagicItem_414");
    let copies: Vec<_> = g.players[0].trinket_effects().collect();
    assert_eq!(copies.len(), 3);
    assert!(copies.iter().all(|card| card.base() == gauntlet));
    let unique: std::collections::HashSet<_> = copies.iter().collect();
    assert_eq!(unique.len(), 3);
    for card in &copies {
        assert_eq!(db().get(*card).cid, gauntlet);
    }
    // A serializer round trip preserves every instance key.
    g = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
    assert_eq!(g.players[0].trinket_effects().collect::<Vec<_>>(), copies);
    g.with_recruit(0, |c| {
        for (i, card) in copies.iter().enumerate() {
            c.set_card_data(0, *card, 7 << i);
        }
    });
    for (i, card) in copies.iter().enumerate() {
        let shown = g.with_recruit(0, |c| {
            let state = taverngym::script::Shown {
                side: 0,
                card: *card,
                golden: false,
                minion: None,
                data: 0,
            };
            taverngym::script::script(*card).unwrap().shown.unwrap()(c, &state)
        });
        assert_eq!(shown, vec![7 << i, 7 << i]);
    }
}

#[test]
fn losing_a_power_trinket_removes_held_auras_but_keeps_purchased_stats() {
    let mut g = fixture(370204);
    let gauntlet = cid("BG30_MagicItem_841");
    g.with_recruit(0, |c| c.set_hero_power(0, 0, gauntlet));
    assert_eq!(g.players[0].stats.tavern_trinket_bonus, 3);
    g.with_recruit(0, |c| c.refresh_shop(0));
    let before = g.players[0].shop[0].item.as_minion().unwrap().clone();
    g.apply(0, Action::Buy(0)).unwrap();
    g.with_recruit(0, |c| c.set_hero_power(0, 0, cid("TB_BaconShop_HP_035")));
    assert_eq!(g.players[0].stats.tavern_trinket_bonus, 0);
    let bought = g.players[0]
        .hand
        .iter()
        .find_map(|h| h.item.as_minion())
        .unwrap();
    assert_eq!((bought.atk, bought.max_hp), (before.atk, before.max_hp));
    g.with_recruit(0, |c| c.refresh_shop(0));
    for m in g.players[0].shop.iter().filter_map(|s| s.item.as_minion()) {
        assert_eq!(
            (m.atk, m.max_hp),
            (db().get(m.card).atk, db().get(m.card).hp)
        );
    }
    // Dynamic Gem auras disappear, while permanent growth remains intact.
    g.players[0].stats.gem_atk = 11;
    g.players[0].stats.gem_hp = 13;
    g.with_recruit(0, |c| {
        c.set_hero_power(0, 0, cid("BG30_MagicItem_988"));
        assert_eq!(c.gem_stats(0), (14, 15));
        c.set_hero_power(0, 0, cid("TB_BaconShop_HP_035"));
        assert_eq!(c.gem_stats(0), (12, 14));
    });
}

#[test]
fn ashen_and_living_nightmare_mix_asymmetric_bonuses_without_losing_purchase_stats() {
    let mut g = fixture(370205);
    set_board(&mut g, 0, &["BG32_873", "BG20_HERO_280_Buddy"]);
    g.with_recruit(0, |c| {
        c.hero_damage(0, 1);
    });
    g.apply(0, Action::Buy(0)).unwrap(); // Living Nightmare triggers on a purchase.
    g.with_recruit(0, |c| c.refresh_shop(0));
    for m in g.players[0].shop.iter().filter_map(|s| s.item.as_minion()) {
        assert_eq!((m.temp_atk, m.temp_hp), (3, 4));
    }
    let bought = g.players[0].shop[0].item.as_minion().unwrap().clone();
    g.apply(0, Action::Buy(0)).unwrap();
    g.apply(0, Action::Freeze).unwrap();
    g.start_turn();
    assert_eq!(g.players[0].tavern_turn_health, 0);
    let m = g.players[0]
        .hand
        .iter()
        .find(|h| h.hid == bought.eid)
        .unwrap()
        .item
        .as_minion()
        .unwrap();
    assert_eq!(
        (m.atk, m.max_hp, m.temp_atk, m.temp_hp),
        (bought.atk, bought.max_hp, 0, 0)
    );
    assert!(
        g.players[0]
            .shop
            .iter()
            .filter_map(|s| s.item.as_minion())
            .all(|m| (m.temp_atk, m.temp_hp) == (0, 0))
    );
}

#[test]
fn new_gift_windows_override_old_observations_and_pair_bans_cover_golden() {
    let mut g = fixture(370206);
    for (id, lo, hi) in [
        ("BG36_MidGameEffect_000t52", 4, 5),
        ("BG36_MidGameEffect_000t28", 7, 9),
        ("BG36_MidGameEffect_000t3", 7, 10),
        ("BG36_MidGameEffect_000t61", 9, 12),
        ("BG36_MidGameEffect_000t75", 3, 5),
        ("BG36_MidGameEffect_000t74", 3, 5),
        ("BG36_MidGameEffect_000t89", 11, 20),
    ] {
        // Force membership to test the turn window independently of lobby exclusions.
        g.gift_universe = vec![cid(id)];
        for turn in [lo - 1, lo, hi, hi + 1] {
            g.turn = turn;
            let expected = turn >= lo && (id.ends_with("t89") || turn <= hi);
            assert_eq!(
                g.with_recruit(0, |c| content::window_gifts(c).contains(&cid(id))),
                expected,
                "{id} turn {turn}"
            );
        }
    }
    for (gift, minion) in [
        ("BG36_MidGameEffect_000t16", "BG23_318"),
        ("BG36_MidGameEffect_000t15", "BG36_360t5"),
        ("BG36_MidGameEffect_000t60", "BG36_849"),
        ("BG36_MidGameEffect_000t79", "BG34_405"),
    ] {
        for card in [cid(minion), db().golden_of(cid(minion))] {
            assert!(
                !g.with_recruit(0, |c| content::gift_fits(
                    c,
                    0,
                    cid(gift),
                    db().get(card),
                    7
                )),
                "{gift} {minion}"
            );
        }
    }
}

#[test]
fn murcules_uses_random_hand_minion_and_golden_uses_six() {
    let mut recipients = std::collections::HashSet::new();
    for seed in 370210..370230 {
        for (card, bonus) in [("BG27_023", 3), ("BG27_023_G", 6)] {
            let mut g = fixture(seed);
            set_board(&mut g, 0, &[card]);
            set_board(&mut g, 1, &["BG36_345"]);
            g.players[1].board[0].set_stats(1, 1);
            give(&mut g, 0, "BG36_345");
            give(&mut g, 0, "BG36_345");
            give(&mut g, 0, "BG20_GEM");
            let base = g.players[0].hand[0].item.as_minion().unwrap().atk;
            {
                let mut c = combat_ctx(&mut g);
                let eid = c.boards[0][0].eid;
                taverngym::combat::attack_now(&mut c, 0, eid);
            }
            let deltas: Vec<_> = g.players[0]
                .hand
                .iter()
                .filter_map(|h| h.item.as_minion())
                .map(|m| m.atk - base)
                .collect();
            assert_eq!(deltas.iter().sum::<i32>(), bonus);
            recipients.insert(deltas.iter().position(|x| *x == bonus).unwrap());
        }
    }
    assert_eq!(recipients.len(), 2);
}

#[test]
fn nellies_real_discover_buffs_hooktusk_itself_without_creating_a_hand_card() {
    let mut g = fixture(370231);
    g.lobby_races = Races(g.lobby_races.0 | Races::PIRATE.0);
    trinket(&mut g, "BG30_MagicItem_801");
    set_board(&mut g, 0, &["BG34_Giant_074t", "BG36_344"]);
    let before = (g.players[0].board[1].atk, g.players[0].board[1].hp);
    g.players[0].stats.golden_played = 2;
    g.with_recruit(0, |c| c.run_board_hook(0, |s| s.start_of_turn, 1, "turn"));
    assert!(matches!(
        g.players[0].pending.last(),
        Some(Pending::Discover { .. })
    ));
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].hand.is_empty());
    assert_eq!(
        (g.players[0].board[1].atk, g.players[0].board[1].hp),
        (before.0 + 3, before.1 + 3)
    );
    assert_eq!(
        g.with_recruit(0, |c| c.turn_data(0, cid("BG30_MagicItem_801"))),
        0
    );
    for _ in 0..2 {
        g.with_recruit(0, |c| {
            c.discover_minion(0, cid("BG30_MagicItem_801"), 3, |_| true)
        });
        g.apply(0, Action::Choose(0)).unwrap();
    }
    assert_eq!(g.players[0].hand.len(), 4);
    assert_eq!(
        g.with_recruit(0, |c| c.turn_data(0, cid("BG30_MagicItem_801"))),
        2
    );
}

#[test]
fn held_tavern_aura_saturation_and_set_stats_remove_only_actual_applied_stats() {
    for initial in [20, i32::MAX - 1, i32::MAX] {
        let mut g = fixture(370240);
        shop(&mut g, &["BG36_345"]);
        let eid = g.players[0].shop[0].sid;
        g.with_recruit(0, |c| c.set_stats(eid, initial, initial));
        g.with_recruit(0, |c| c.set_hero_power(0, 0, cid("BG30_MagicItem_841")));
        let m = g.players[0].shop[0].item.as_minion().unwrap();
        assert_eq!(
            (m.atk, m.max_hp),
            (initial.saturating_add(3), initial.saturating_add(3))
        );
        // Setters reapply the held aura and refresh its actual contribution.
        g.with_recruit(0, |c| c.set_stats(eid, initial, initial));
        g.with_recruit(0, |c| c.set_hero_power(0, 0, cid("TB_BaconShop_HP_035")));
        let m = g.players[0].shop[0].item.as_minion().unwrap();
        assert_eq!((m.atk, m.max_hp), (initial, initial));
    }
}

#[test]
fn permanent_tavern_aura_at_cap_survives_held_aura_removal_after_set() {
    let mut g = fixture(370240);
    shop(&mut g, &["BG36_345"]);
    let eid = g.players[0].shop[0].sid;
    g.with_recruit(0, |c| c.set_hero_power(0, 0, cid("BG30_MagicItem_841")));
    g.players[0].stats.tavern_atk = i32::MAX;
    g.players[0].stats.tavern_hp = i32::MAX;
    g.with_recruit(0, |c| c.set_stats(eid, 0, 1));
    g.with_recruit(0, |c| c.set_hero_power(0, 0, cid("TB_BaconShop_HP_035")));
    let m = g.players[0].shop[0].item.as_minion().unwrap();
    assert_eq!((m.atk, m.max_hp), (i32::MAX, i32::MAX));
}

#[test]
fn replacement_menu_closes_if_an_earlier_purchase_spends_its_budget() {
    let mut g = fixture(370242);
    let target = db()
        .iter()
        .find(|d| {
            d.kind == taverngym::cards::CardKind::TrinketGreater
                && d.cost == 4
                && d.id == "BG32_MagicItem_367"
        })
        .unwrap()
        .cid;
    g.players[0].gold = 4;
    g.players[0].trinkets = vec![
        cid("BG36_MagicItem_412t2"),
        cid("BG36_MagicItem_412t2").instance(1),
    ];
    for old in g.players[0].trinkets.clone() {
        g.players[0].pending.push(Pending::Trinket {
            options: vec![target],
            free: false,
            replace: Some(old),
        });
    }
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].pending.is_empty());
    assert_eq!(g.players[0].gold, 0);
    assert!(g.apply(0, Action::EndTurn).is_ok());
}

#[test]
fn two_tapestries_charge_separately_and_lost_cube_power_stays_lost() {
    let mut g = fixture(370243);
    let tapestry = cid("BG35_MagicItem_152");
    g.with_recruit(0, |c| {
        content::grant_trinket(c, 0, tapestry);
        c.set_hero_power(0, 0, tapestry.instance(1));
    });
    for _ in 0..4 {
        g.apply(0, Action::Refresh).unwrap();
    }
    shop(&mut g, &["BG36_345", "BG36_345", "BG36_345"]);
    for _ in 0..2 {
        assert!(g.slot_price(0, 0).unwrap().1);
        g.apply(0, Action::Buy(0)).unwrap();
    }
    assert!(!g.slot_price(0, 0).unwrap().1);
    // A Cube's own replacement transfers its source; an external skill swap terminates it.
    let cube = cid("BG30_MagicItem_703");
    g.with_recruit(0, |c| c.set_hero_power(0, 0, cube));
    g.players[0].pending.clear();
    assert!(g.players[0].cube_sources.contains(&cube));
    g.with_recruit(0, |c| {
        content::replace_trinket(c, 0, cube, tapestry.instance(1))
    });
    let key = g.players[0].powers[0].card;
    assert!(g.players[0].cube_sources.contains(&key));
    g.with_recruit(0, |c| c.set_hero_power(0, 0, cid("TB_BaconShop_HP_035")));
    assert!(!g.players[0].cube_sources.contains(&key));
    g.start_turn();
    assert_eq!(g.players[0].powers[0].card, cid("TB_BaconShop_HP_035"));
}

#[test]
fn bait_fixes_a_public_menu_type_and_respects_full_frozen_tavern_and_empty_pool() {
    let mut g = fixture(370244);
    set_board(&mut g, 0, &["BG32_860", "BG32_860"]);
    let bait = cid("BG30_MagicItem_973");
    g.trinket_universe = vec![bait];
    g.offer_trinkets(0, taverngym::cards::CardKind::TrinketLesser);
    assert_eq!(
        g.with_recruit(0, |c| c.card_data(0, bait)),
        i32::from(Races::MURLOC.0)
    );
    let before = serde_json::to_string(&g).unwrap();
    let shown = taverngym::debug::view(&g, 0);
    let text = shown["me"]["pending"]["options"][0]["text"]
        .as_str()
        .unwrap();
    assert!(text.contains("Murloc") && !text.contains("92"), "{text}");
    assert_eq!(
        serde_json::to_string(&g).unwrap(),
        before,
        "viewing consumes no RNG"
    );
    choose_id(&mut g, "BG30_MagicItem_973");
    g.apply(0, Action::Refresh).unwrap();
    assert!(g.players[0].shop.len() <= 7);
    let bonus: Vec<_> = g.players[0].shop.iter().rev().take(2).collect();
    assert!(
        bonus
            .iter()
            .all(|s| s.item.as_minion().is_some_and(|m| m.is(Races::MURLOC)))
    );
    // Pool depletion does not remove a default offer before realizing no card exists.
    g.pool.counts.fill(0);
    let before = g.players[0].shop.clone();
    assert_eq!(
        g.with_recruit(0, |c| c.add_shop_minion(0, 1, 6, Races::MURLOC)),
        None
    );
    assert_eq!(g.players[0].shop, before);
    // Every kept slot is protected even when an eligible copy exists in the pool.
    shop(&mut g, &["BG36_345"; 7]);
    for s in &mut g.players[0].shop {
        s.frozen = true;
    }
    let card = g
        .universe
        .iter()
        .copied()
        .find(|c| db().get(*c).races.intersects(Races::MURLOC))
        .unwrap();
    g.pool.put(card, 1);
    let pool = g.pool.clone();
    let shop = serde_json::to_value(&g.players[0].shop).unwrap();
    assert_eq!(
        g.with_recruit(0, |c| c.add_shop_minion(0, 1, 7, Races::MURLOC)),
        None
    );
    assert_eq!(serde_json::to_value(&g.players[0].shop).unwrap(), shop);
    assert_eq!(g.pool, pool);
}

#[test]
fn hyena_only_combats_terminate_and_recruit_consumed_reborn_does_not_return() {
    for seed in 370250..370270 {
        let mut g = fixture(seed);
        set_board(&mut g, 0, &["BG36_210"]);
        set_board(&mut g, 1, &["BG36_210"]);
        let outcome = fight(&mut g);
        assert!(!outcome.truncated, "seed {seed}");
    }
    let mut g = fixture(370270);
    set_board(&mut g, 0, &["BG25_010t"]);
    let original = g.players[0].board[0].eid;
    g.with_recruit(0, |c| {
        c.destroy(original);
        c.resolve_deaths();
    });
    assert_eq!(g.players[0].board.len(), 1);
    assert_ne!(g.players[0].board[0].eid, original);
    assert!(!g.players[0].board[0].has(Kw::REBORN));
    set_board(&mut g, 1, &["BG36_345"]);
    fight(&mut g);
    assert!(!g.players[0].board[0].has(Kw::REBORN));
    g.start_turn();
    assert!(!g.players[0].board[0].has(Kw::REBORN));
}

#[test]
fn ancient_mask_keeps_earned_growth_when_the_golden_deity_awakens() {
    let mut g = common::game(370271);
    let old = g.players[0].deity.as_ref().unwrap().clone();
    let normal = db().get(old.card);
    let golden = db().get(db().golden_of(old.card));
    g.players[0].deity.as_mut().unwrap().atk = normal.atk + 31;
    g.players[0].deity.as_mut().unwrap().hp = normal.hp + 47;
    trinket(&mut g, "BG36_MagicItem_602");
    let wanted = (golden.atk + 31, golden.hp + 47);
    assert_eq!(
        (
            g.players[0].deity.as_ref().unwrap().atk,
            g.players[0].deity.as_ref().unwrap().hp
        ),
        wanted
    );
    set_board(&mut g, 0, &["BG36_345"; 4]);
    for m in &mut g.players[0].board {
        m.races = Races::ABERRATION;
    }
    let mut c = combat_ctx(&mut g);
    let ids: Vec<_> = c.boards[0].iter().map(|m| m.eid).collect();
    for eid in ids {
        c.destroy(eid);
        c.resolve_deaths();
    }
    let deity = c.boards[0].iter().find(|m| m.card == golden.cid).unwrap();
    assert!(deity.golden);
    assert_eq!((deity.atk, deity.max_hp), wanted);
}

#[test]
fn permanent_shop_gain_at_cap_precedes_a_removable_held_aura() {
    for generic in [false, true] {
        let mut g = fixture(370281);
        shop(&mut g, &["BG36_345"]);
        let eid = g.players[0].shop[0].sid;
        g.with_recruit(0, |c| c.set_hero_power(0, 0, cid("BG30_MagicItem_841")));
        g.with_recruit(0, |c| {
            if generic {
                c.buff(eid, i32::MAX, i32::MAX);
            } else {
                c.player(0).unwrap().stats.tavern_atk = i32::MAX;
                c.player(0).unwrap().stats.tavern_hp = i32::MAX;
                c.buff_tavern(0, i32::MAX, i32::MAX, Races::NONE);
            }
            c.set_hero_power(0, 0, cid("TB_BaconShop_HP_035"));
        });
        let m = g.players[0].shop[0].item.as_minion().unwrap();
        assert_eq!((m.atk, m.max_hp), (i32::MAX, i32::MAX));
    }
}

#[test]
fn all_trinket_instances_preserve_progress_and_counter_readers() {
    for ordinal in 0..CardId::INSTANCES {
        for id in ["BG32_MagicItem_801t", "BG36_MagicItem_810"] {
            let mut g = fixture(370282);
            let card = cid(id).instance(ordinal);
            g.players[0].trinkets = vec![card];
            g.players[0].armor = 5;
            give(&mut g, 0, "BG28_500");
            let action = Action::Play { hand: 0, pos: 0 };
            assert!(g.legal_mask(0)[action.encode()]);
            g.apply(0, action).unwrap();
            assert_eq!(g.players[0].armor, 5);
            if id == "BG32_MagicItem_801t" {
                assert_eq!(g.with_recruit(0, |c| c.card_data(0, card)), 1);
            } else {
                assert_eq!(g.players[0].stats.tavern_spells_cast_turn, 1);
            }
        }
    }
}

#[test]
fn wand_lobby_ban_and_schematic_minimum_cost_apply_to_real_offers_and_purchases() {
    let mut g = fixture(370283);
    let wand = cid("BG36_MagicItem_307");
    assert!(!g.trinket_eligible(0, wand));
    g.lobby_races = Races(g.lobby_races.0 & !Races::QUILBOAR.0);
    assert!(g.trinket_eligible(0, wand));
    trinket(&mut g, "BG36_MagicItem_840");
    g.players[0].tier = 6;
    // Include both one-Gold and two-Gold Tavern spells; only the latter is eligible.
    g.spell_universe = vec![cid("BG28_810"), cid("BG28_838"), cid("BG28_888")];
    for _ in 0..4 {
        g.players[0].hand.clear();
        shop(&mut g, &["BG36_851"]);
        g.apply(0, Action::Buy(0)).unwrap();
        let spells: Vec<_> = g.players[0]
            .hand
            .iter()
            .filter(|h| matches!(h.item, Item::Spell { .. }))
            .collect();
        assert_eq!(spells.len(), 1);
        assert_eq!(spells[0].item.card(), cid("BG28_888"));
        let d = db().get(spells[0].item.card());
        assert!(d.kind == taverngym::cards::CardKind::TavernSpell && d.cost >= 2);
    }
}

#[test]
fn golden_last_one_repeats_on_one_target_group_and_does_not_keep_combat_improvement() {
    for seed in 370300..370320 {
        let mut g = fixture(seed);
        set_board(&mut g, 0, &["BG34_320_G", "BG36_703", "BG36_703"]);
        set_board(&mut g, 1, &["BG36_345"]);
        g.players[1].board[0].set_stats(0, 10_000);
        let initial: Vec<_> = g.players[0].board.iter().map(|m| m.atk).collect();
        {
            let mut c = combat_ctx(&mut g);
            let eid = c.boards[0][0].eid;
            taverngym::combat::attack_now(&mut c, 0, eid);
            let mut gains: Vec<_> = (1..3).map(|i| c.boards[0][i].atk - initial[i]).collect();
            gains.sort();
            assert_eq!(gains, [0, 30]);
            assert_eq!(c.boards[0][0].counter, 2);
            assert_eq!(c.g.players[0].board[0].counter, 0);
        }
    }
}

#[test]
fn last_one_recruit_improvement_survives_triple_and_turn_but_combat_growth_does_not() {
    let mut g = fixture(370321);
    g.cfg.triple_copies = 3;
    set_board(&mut g, 0, &["BG34_320"]);
    let eid = g.players[0].board[0].eid;
    g.with_recruit(0, |c| {
        c.trigger_rally(0, eid, 0);
        c.trigger_rally(0, eid, 0);
    });
    assert_eq!(g.players[0].board[0].counter, 2);
    give(&mut g, 0, "BG34_320");
    give(&mut g, 0, "BG34_320");
    let golden = g.players[0].hand[0].item.as_minion().unwrap();
    assert!(golden.golden);
    assert_eq!(golden.counter, 2);
    play_last(&mut g, 0);
    g.players[0].pending.clear();
    set_board(&mut g, 1, &["BG36_345"]);
    g.players[1].board[0].set_stats(0, 10_000);
    let initial = g.players[0].board[0].atk;
    {
        let mut c = combat_ctx(&mut g);
        let eid = c.boards[0][0].eid;
        taverngym::combat::attack_now(&mut c, 0, eid);
        assert_eq!(c.boards[0][0].counter, 4);
        assert_eq!(c.boards[0][0].atk, initial + 70);
    }
    assert_eq!(g.players[0].board[0].counter, 2);
    assert_eq!(g.players[0].board[0].atk, initial + 70);
    g.start_turn();
    assert_eq!(g.players[0].board[0].counter, 2);
    g.players[0].board[0].counter = i32::MAX;
    let eid = g.players[0].board[0].eid;
    g.with_recruit(0, |c| c.trigger_rally(0, eid, 0));
    assert_eq!(g.players[0].board[0].atk, i32::MAX);
    assert_eq!(g.players[0].board[0].counter, i32::MAX);
}
