//! Tavern spell identity, acquired Tavern stats, and atomic recruit summon batches.
mod common;
use common::*;
use taverngym::content;
use taverngym::state::ShopSlot;

fn fixture(seed: u64) -> Game {
    let mut g = game(seed);
    g.cfg.experimental_mode = true;
    g.players[0].gold = 100;
    g.cfg.lesser_trinket_turn = 0;
    g.cfg.greater_trinket_turn = 0;
    g
}

fn shop(g: &mut Game, ids: &[&str]) {
    g.players[0].shop.clear();
    for id in ids {
        let m = minion(g, 0, id);
        g.players[0]
            .shop
            .push(ShopSlot::new(m.eid, Item::Minion(m)));
    }
}

fn count_hand(g: &Game, id: &str) -> usize {
    g.players[0]
        .hand
        .iter()
        .filter(|h| h.item.card() == cid(id))
        .count()
}

#[test]
fn generated_spell_kind_does_not_make_a_goldenizer_a_tavern_spell() {
    for id in [
        "BG26_813t",
        "BG34_BlackMarket_Skip",
        "BG31_HERO_801pta",
        "BG20_GEM",
        "TB_BaconShop_HP_008a",
    ] {
        assert!(!content::is_tavern_spell(cid(id)), "{id}");
    }
    for id in ["BG28_810", "BG28_503", "BG28_830", "BG34_Treasure_955"] {
        assert!(content::is_tavern_spell(cid(id)), "{id}");
    }
}

#[test]
fn sphere_harbinger_and_nobundo_remember_the_last_actual_tavern_spell() {
    for (harbinger, n) in [("BG35_123", 1), ("BG35_123_G", 2)] {
        let mut g = fixture(4700);
        set_board(&mut g, 0, &["BG36_345", harbinger]);
        g.players[0].trinkets.push(cid("BG36_MagicItem_372"));
        g.with_recruit(0, |c| c.set_hero_power(0, 0, cid("BG31_HERO_003p")));
        give(&mut g, 0, "BG28_810");
        play_last(&mut g, 0);
        give(&mut g, 0, "BG26_813t");
        cast_on(&mut g, 0, 0);
        assert_eq!(g.players[0].stats.spells_cast, 2);
        assert_eq!(g.players[0].stats.tavern_spells_cast, 1);
        assert_eq!(g.players[0].stats.last_spell_cast, Some(cid("BG28_810")));
        g.apply(0, Action::HeroPower(0)).unwrap();
        assert_eq!(count_hand(&g, "BG28_810"), 1);
        g.apply(0, Action::EndTurn).unwrap();
        assert_eq!(count_hand(&g, "BG28_810"), 1 + 3 + n);
        assert_eq!(count_hand(&g, "BG26_813t"), 0);
    }
}

#[test]
fn sphere_gives_nothing_when_only_a_goldenizer_was_cast() {
    let mut g = fixture(4701);
    set_board(&mut g, 0, &["BG36_345"]);
    g.players[0].trinkets.push(cid("BG36_MagicItem_372"));
    give(&mut g, 0, "BG26_813t");
    cast_on(&mut g, 0, 0);
    g.apply(0, Action::EndTurn).unwrap();
    assert!(g.players[0].hand.is_empty());
    assert_eq!(g.players[0].stats.tavern_spells_cast, 0);
}

#[test]
fn temporal_tampering_does_not_recast_the_goldenizer() {
    let mut g = fixture(4702);
    set_board(&mut g, 0, &["BG36_345"]);
    g.players[0].rewards.push(cid("BG28_Reward_501"));
    give(&mut g, 0, "BG26_813t");
    cast_on(&mut g, 0, 0);
    assert_eq!(g.players[0].stats.spells_cast, 1);
    assert_eq!(g.players[0].stats.tavern_spells_cast, 0);
    give(&mut g, 0, "BG28_810");
    play_last(&mut g, 0);
    assert_eq!(g.players[0].stats.spells_cast, 3);
    assert_eq!(g.players[0].stats.tavern_spells_cast, 2);
}

#[test]
fn last_tavern_spell_copies_preserve_bloodbinders_special_gem_data() {
    let mut g = fixture(4703);
    set_board(&mut g, 0, &["BG36_345", "BG35_123"]);
    g.players[0].trinkets.push(cid("BG36_MagicItem_372"));
    g.with_recruit(0, |c| {
        c.set_hero_power(0, 0, cid("BG31_HERO_003p"));
        let target = c.boards[0][0].eid;
        c.you_cast(0, cid("BG20_GEM"), 4, target, 0);
    });
    assert_eq!(g.players[0].stats.tavern_spells_cast, 1);
    g = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
    g.apply(0, Action::HeroPower(0)).unwrap();
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(g.players[0].hand.len(), 5);
    for h in &g.players[0].hand {
        assert!(matches!(h.item, Item::Spell { card, data: 4 } if card == cid("BG20_GEM")));
    }
}

#[test]
fn the_tavern_spell_exception_uses_instance_data_for_extra_casts() {
    let mut g = fixture(4704);
    set_board(&mut g, 0, &["BG36_345"]);
    g.players[0].rewards.push(cid("BG28_Reward_501"));
    g.with_recruit(0, |c| {
        c.add_to_hand(
            0,
            Item::Spell {
                card: cid("BG20_GEM"),
                data: 4,
            },
        );
    });
    cast_on(&mut g, 0, 0);
    assert_eq!(g.players[0].stats.spells_cast, 2);
    assert_eq!(g.players[0].stats.tavern_spells_cast, 2);
    give(&mut g, 0, "BG20_GEM");
    cast_on(&mut g, 0, 0);
    assert_eq!(g.players[0].stats.spells_cast, 3);
    assert_eq!(g.players[0].stats.tavern_spells_cast, 2);
}

#[test]
fn kthir_discards_actual_tavern_spells_and_leaves_the_goldenizer() {
    let mut g = fixture(4705);
    set_board(&mut g, 0, &["BG36_320"]);
    let base = (g.players[0].board[0].atk, g.players[0].board[0].max_hp);
    give(&mut g, 0, "BG20_GEM");
    give(&mut g, 0, "BG26_813t");
    g.with_recruit(0, |c| {
        c.add_to_hand(
            0,
            Item::Spell {
                card: cid("BG20_GEM"),
                data: 4,
            },
        );
    });
    give(&mut g, 0, "BG28_810");
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(g.players[0].hand.len(), 2);
    assert!(
        matches!(g.players[0].hand[0].item, Item::Spell { card, data: 0 } if card == cid("BG20_GEM"))
    );
    assert_eq!(g.players[0].hand[1].item.card(), cid("BG26_813t"));
    assert_eq!(stats(&g, 0), vec![(base.0 + 14, base.1 + 14)]);
}

#[test]
fn bought_ashen_stats_persist_but_frozen_tavern_stats_expire() {
    for (corruptor, k) in [("BG32_873", 2), ("BG32_873_G", 4)] {
        let mut g = fixture(4710);
        set_board(&mut g, 0, &[corruptor]);
        shop(&mut g, &["BG36_345", "BG36_345"]);
        let base = (db().get(cid("BG36_345")).atk, db().get(cid("BG36_345")).hp);
        g.with_recruit(0, |c| c.hero_damage(0, 1));
        g = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        let bought = g.players[0].shop[0].sid;
        let kept = g.players[0].shop[1].sid;
        g.apply(0, Action::Buy(0)).unwrap();
        let m = g.players[0]
            .hand
            .iter()
            .find(|h| h.hid == bought)
            .unwrap()
            .item
            .as_minion()
            .unwrap();
        assert_eq!((m.atk, m.max_hp), (base.0 + k, base.1 + k));
        assert_eq!((m.temp_atk, m.temp_hp), (0, 0));
        g.apply(0, Action::Freeze).unwrap();
        g.start_turn();
        let m = g.players[0]
            .hand
            .iter()
            .find(|h| h.hid == bought)
            .unwrap()
            .item
            .as_minion()
            .unwrap();
        assert_eq!((m.atk, m.max_hp), (base.0 + k, base.1 + k));
        let m = g.players[0]
            .shop
            .iter()
            .find(|s| s.sid == kept)
            .unwrap()
            .item
            .as_minion()
            .unwrap();
        assert_eq!((m.atk, m.max_hp), base);
    }
}

#[test]
fn a_refreshed_tavern_minion_keeps_the_turn_bonus_when_bought() {
    let mut g = fixture(4711);
    set_board(&mut g, 0, &["BG32_873"]);
    g.with_recruit(0, |c| {
        c.hero_damage(0, 1);
        c.refresh_shop(0);
    });
    let before = g.players[0].shop[0].item.as_minion().unwrap().clone();
    assert_eq!((before.temp_atk, before.temp_hp), (2, 2));
    g.apply(0, Action::Buy(0)).unwrap();
    g.start_turn();
    let m = g.players[0]
        .hand
        .iter()
        .find(|h| h.hid == before.eid)
        .unwrap()
        .item
        .as_minion()
        .unwrap();
    assert_eq!((m.atk, m.max_hp), (before.atk, before.max_hp));
}

#[test]
fn acquisition_commits_only_stats_granted_in_the_tavern() {
    let mut g = fixture(4712);
    let mut m = minion(&mut g, 0, "BG36_345");
    let base = (m.atk, m.max_hp);
    // A pre-existing personal temporary buff follows the minion into the Tavern.
    m.add_stats(3, 5);
    (m.temp_atk, m.temp_hp) = (3, 5);
    let eid = m.eid;
    g.players[0].shop = vec![ShopSlot::new(eid, Item::Minion(m))];
    g.with_recruit(0, |c| c.buff_tavern_temp(0, 2, 4));
    g.apply(0, Action::Buy(0)).unwrap();
    let m = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((m.temp_atk, m.temp_hp), (3, 5));
    g.start_turn();
    let m = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((m.atk, m.max_hp), (base.0 + 2, base.1 + 4));
}

#[test]
fn living_nightmare_does_not_remove_bought_stats_next_turn() {
    let mut g = fixture(4713);
    set_board(&mut g, 0, &["BG20_HERO_280_Buddy"]);
    shop(&mut g, &["BG36_345", "BG36_345"]);
    let base = (db().get(cid("BG36_345")).atk, db().get(cid("BG36_345")).hp);
    g.apply(0, Action::Buy(0)).unwrap();
    g.apply(0, Action::Buy(0)).unwrap();
    g.start_turn();
    let values: Vec<_> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .map(|m| (m.atk, m.max_hp))
        .collect();
    assert_eq!(values, vec![base, (base.0 + 2, base.1 + 2)]);
}

#[test]
fn a_capped_tavern_gain_keeps_only_the_actual_five_points_on_acquisition() {
    let mut g = fixture(4714);
    shop(&mut g, &["BG36_345"]);
    let sid = g.players[0].shop[0].sid;
    g.players[0].shop[0]
        .item
        .as_minion_mut()
        .unwrap()
        .set_stats(i32::MAX - 5, i32::MAX - 5);
    g.with_recruit(0, |c| c.buff_tavern_temp(0, 100, 100));
    g.apply(0, Action::Buy(0)).unwrap();
    g.start_turn();
    let m = g.players[0]
        .hand
        .iter()
        .find(|h| h.hid == sid)
        .unwrap()
        .item
        .as_minion()
        .unwrap();
    assert_eq!((m.atk, m.max_hp), (i32::MAX, i32::MAX));
    assert_eq!((m.temp_atk, m.temp_hp), (0, 0));
}

#[test]
fn combat_summons_still_stop_at_seven_without_forming_triples() {
    let mut g = fixture(4715);
    g.cfg.triple_copies = 3;
    set_board(
        &mut g,
        0,
        &["BG36_345", "BG24_715", "BG32_237", "BG30_125_G"],
    );
    let eid = g.players[0].board[3].eid;
    let mut c = combat_ctx(&mut g);
    c.destroy(eid);
    c.resolve_deaths();
    assert_eq!(c.boards[0].len(), 7);
    assert_eq!(
        c.boards[0]
            .iter()
            .filter(|m| m.card == cid("BG_ICC_026t"))
            .count(),
        4
    );
    assert!(c.g.players[0].hand.is_empty());
}

#[test]
fn a_deathrattle_finishes_all_summons_before_a_triple_frees_slots() {
    for (titus, source) in [(false, "BG30_125_G"), (true, "BG30_125")] {
        let mut g = fixture(4720);
        g.cfg.triple_copies = 3;
        let first = if titus { "BG25_354" } else { "BG36_345" };
        set_board(&mut g, 0, &[first, "BG24_715", "BG32_237", source]);
        let caretaker = g.players[0].board[3].eid;
        g.with_recruit(0, |c| {
            c.destroy(caretaker);
            c.resolve_deaths();
        });
        // Only four Skeletons fit; the triple leaves one and three other bodies.
        assert_eq!(g.players[0].board.len(), 4);
        assert_eq!(
            g.players[0]
                .board
                .iter()
                .filter(|m| m.card == cid("BG_ICC_026t"))
                .count(),
            1
        );
        assert_eq!(count_hand(&g, "BG_ICC_026t_G"), 1);
        assert_eq!(
            g.players[0].stats.deathrattles_triggered,
            if titus { 2 } else { 1 }
        );
    }
}

#[test]
fn repeated_battlecries_cannot_reuse_slots_freed_by_a_mid_batch_triple() {
    let mut g = fixture(4721);
    g.cfg.triple_copies = 3;
    set_board(
        &mut g,
        0,
        &["TB_BaconUps_045", "BG36_345", "BG24_715", "BG_ICC_026t"],
    );
    g.with_recruit(0, |c| {
        // A supported Putricide Creation with Grim Necromancer's summoning Battlecry.
        let mut m = c.new_minion(0, cid("BG25_HERO_100pt"));
        m.attach(cid("BG25_005"), 0);
        m.attach(cid("BG26_ICC_026"), 0);
        c.add_to_hand(0, Item::Minion(m));
    });
    play_last(&mut g, 0);
    assert_eq!(g.players[0].stats.battlecries_triggered, 3);
    assert_eq!(g.players[0].board.len(), 4);
    assert!(
        !g.players[0]
            .board
            .iter()
            .any(|m| m.card == cid("BG_ICC_026t"))
    );
    assert_eq!(count_hand(&g, "BG_ICC_026t_G"), 1);
}

#[test]
fn summon_card_itself_is_a_batch_and_retains_outer_triple_suppression() {
    for outer in [false, true] {
        let mut g = fixture(4722);
        g.cfg.triple_copies = 3;
        set_board(&mut g, 0, &["BG36_345", "BG24_715", "BG32_237"]);
        g.with_recruit(0, |c| {
            c.no_triples = outer;
            let spawned = c.summon_card(0, 3, cid("BG_ICC_026t"), 8);
            assert_eq!(spawned.len(), 4);
            assert_eq!(c.no_triples, outer);
            assert_eq!(c.boards[0].len(), if outer { 7 } else { 4 });
            c.no_triples = false;
            c.check_triples(0);
        });
        assert_eq!(count_hand(&g, "BG_ICC_026t_G"), 1);
    }
}
