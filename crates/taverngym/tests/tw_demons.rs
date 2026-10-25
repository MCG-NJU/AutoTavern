//! Tests for the Timewarp cards of `content/tw_demons.rs`.

mod common;
#[allow(unused_imports)]
use common::*;

const IMP: &str = "BG_BRM_006t"; // 1/1 Demon
const SNAKE: &str = "AV_113t3t2"; // 2/2 Beast
const SCOUT: &str = "EX1_506a"; // 1/1 Murloc
const MONKEY: &str = "BG23_353t"; // 6/6, no type

/// A game whose lobby has Demons (first such seed from `seed` on).
fn demon_game(seed: u64) -> Game {
    (seed..)
        .map(game)
        .find(|g| g.lobby_races.intersects(Races::DEMON))
        .unwrap()
}

fn total_hp(g: &Game) -> i32 {
    g.players[0].health + g.players[0].armor
}

fn hand_cards(g: &Game) -> Vec<CardId> {
    g.players[0].hand.iter().map(|h| h.item.card()).collect()
}

fn shop_stats(g: &Game) -> Vec<(i32, i32)> {
    g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| (m.atk, m.hp))
        .collect()
}

/// Tavern Demons whose stats are exactly their card's +`v`/+`v`.
fn boosted_demons(g: &Game, v: i32) -> usize {
    g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .filter(|m| {
            let d = db().get(m.card);
            m.races.intersects(Races::DEMON) && m.atk == d.atk + v && m.hp == d.hp + v
        })
        .count()
}

#[test]
fn rewinder_and_archimonde_rewind_hero_damage() {
    let mut g = game(9100);
    set_board(&mut g, 0, &["BG34_Giant_300", "BG34_Giant_596", SNAKE]);
    let hp = total_hp(&g);
    g.with_recruit(0, |c| c.hero_damage(0, 3));
    assert_eq!(total_hp(&g), hp, "rewound");
    assert_eq!(
        stats(&g, 0),
        vec![(5, 5), (5, 6), (2, 2)],
        "Demons +1 Health"
    );
    assert_eq!(g.players[0].stats.spell_discount_next, 1);
    // Golden: +2 Health, (2) less.
    set_board(&mut g, 0, &["BG34_Giant_300_G", "BG34_Giant_596_G"]);
    g.with_recruit(0, |c| c.hero_damage(0, 3));
    assert_eq!(total_hp(&g), hp);
    assert_eq!(stats(&g, 0), vec![(10, 10), (10, 12)]);
    assert_eq!(g.players[0].stats.spell_discount_next, 3);
}

#[test]
fn kiljaeden_offers_two_boosted_demons_and_upgrades_each_turn() {
    let mut g = demon_game(9110);
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG34_Giant_313"]);
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(boosted_demons(&g, 7), 2);
    // Next turn: upgraded once, +14/+14.
    end_turn_all(&mut g);
    assert_eq!(g.players[0].board[0].counter, 1);
    g.players[0].gold = 10;
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(boosted_demons(&g, 14), 2);
    // Golden: +14/+14 from the start.
    set_board(&mut g, 0, &["BG34_Giant_313_G"]);
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(boosted_demons(&g, 14), 2);
}

#[test]
fn shadowdancer_casts_staff_of_enrichment_at_end_of_turn() {
    for (id, v) in [("BG34_Giant_360", 2), ("BG34_Giant_360_G", 4)] {
        let mut g = game(9120);
        set_board(&mut g, 0, &[id]);
        g.apply(0, Action::EndTurn).unwrap();
        let st = &g.players[0].stats;
        assert_eq!((st.tavern_atk, st.tavern_hp), (v, v));
    }
}

#[test]
fn kilrek_gets_random_demons() {
    for (id, n) in [("BG34_Giant_584", 1), ("BG34_Giant_584_G", 2)] {
        let mut g = demon_game(9130);
        set_board(&mut g, 0, &[id]);
        let eid = g.players[0].board[0].eid;
        let mut c = combat_ctx(&mut g);
        c.destroy(eid);
        c.resolve_deaths();
        drop(c);
        let hand = hand_cards(&g);
        assert_eq!(hand.len(), n);
        assert!(
            hand.iter()
                .all(|c| db().get(*c).races.intersects(Races::DEMON))
        );
    }
}

#[test]
fn icky_imp_summons_imps_with_its_maximum_stats() {
    for (id, n, base) in [("BG34_Giant_674", 2, 12), ("BG34_Giant_674_G", 4, 24)] {
        let mut g = game(9140);
        set_board(&mut g, 0, &[id]);
        let eid = g.players[0].board[0].eid;
        let mut c = combat_ctx(&mut g);
        c.buff(eid, 3, 5);
        c.damage(0, eid, 4); // damage doesn't lower the maximum Health
        c.destroy(eid);
        c.resolve_deaths();
        let imps: Vec<(CardId, i32, i32)> =
            c.boards[0].iter().map(|m| (m.card, m.atk, m.hp)).collect();
        assert_eq!(imps, vec![(cid(IMP), base + 3, base + 5); n]);
    }
}

#[test]
fn chimera_buffs_one_minion_per_type_when_damaged() {
    for (id, k) in [("BG34_Giant_679", 1), ("BG34_Giant_679_G", 2)] {
        let mut g = game(9150);
        set_board(&mut g, 0, &[id, SNAKE, IMP, MONKEY]);
        let eid = g.players[0].board[0].eid;
        let mut c = combat_ctx(&mut g);
        c.damage(0, eid, 1);
        c.flush_deferred();
        drop(c);
        // Three types among Chimera (all), Snake and Imp; the typeless Monkey gets nothing.
        // Permanent: the warband itself has the stats (and not the damage).
        assert_eq!(
            stats(&g, 0),
            vec![
                (k + 2 * k, 8 * k + k),
                (2 + 2 * k, 2 + k),
                (1 + 2 * k, 1 + k),
                (6, 6)
            ]
        );
    }
}

#[test]
fn mythrax_gains_the_stats_of_three_minions_of_different_types() {
    for (id, k) in [("BG34_Giant_684", 1), ("BG34_Giant_684_G", 2)] {
        let mut g = game(9160);
        // The other Mythrax and the typeless Monkey don't count: Snake + Imp + Scout = 4/4.
        set_board(
            &mut g,
            0,
            &[id, "BG34_Giant_684", SNAKE, IMP, SCOUT, MONKEY],
        );
        let eid = g.players[0].board[0].eid;
        let mut c = combat_ctx(&mut g);
        c.run_minion_hook(0, eid, |s| s.start_of_combat, 1, "start of combat");
        let m = c.minion(eid).unwrap();
        assert_eq!((m.atk, m.hp), (8 * k + 4 * k, 9 * k + 4 * k));
    }
    // Four types: only three are taken (all four candidates are 1/1).
    let mut g = game(9161);
    set_board(&mut g, 0, &["BG34_Giant_684", IMP, SCOUT, SCOUT, SCOUT]);
    g.players[0].board[3].races = Races::BEAST;
    g.players[0].board[4].races = Races::DRAGON;
    let eid = g.players[0].board[0].eid;
    let mut c = combat_ctx(&mut g);
    c.run_minion_hook(0, eid, |s| s.start_of_combat, 1, "start of combat");
    let m = c.minion(eid).unwrap();
    assert_eq!((m.atk, m.hp), (11, 12));
}

#[test]
fn overfiend_buffs_demons_after_buying_a_card() {
    let mut g = game(9170);
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG34_PreMadeChamp_011", IMP, SNAKE]);
    g.apply(0, Action::Buy(0)).unwrap();
    assert_eq!(stats(&g, 0), vec![(11, 17), (5, 5), (2, 2)]);
    set_board(&mut g, 0, &["BG34_PreMadeChamp_011_G", IMP]);
    g.apply(0, Action::Buy(0)).unwrap();
    assert_eq!(stats(&g, 0), vec![(22, 34), (9, 9)]);
}

#[test]
fn imp_filtrator_buffs_the_tavern_per_8_gold_spent() {
    for (id, v) in [("BG34_PreMadeChamp_013", 3), ("BG34_PreMadeChamp_013_G", 6)] {
        let mut g = game(9180);
        set_board(&mut g, 0, &[id]);
        g.players[0].gold = 20;
        let before = shop_stats(&g);
        assert!(!before.is_empty());
        g.with_recruit(0, |c| c.spend_gold(0, 5));
        assert_eq!(g.players[0].stats.tavern_atk, 0);
        assert_eq!(g.players[0].board[0].counter, 5, "3 Gold left");
        g.with_recruit(0, |c| c.spend_gold(0, 11)); // 16 in all: twice
        let st = &g.players[0].stats;
        assert_eq!((st.tavern_atk, st.tavern_hp), (2 * v, 2 * v));
        let want: Vec<(i32, i32)> = before.iter().map(|b| (b.0 + 2 * v, b.1 + 2 * v)).collect();
        assert_eq!(shop_stats(&g), want);
    }
}

#[test]
fn calligrapher_gets_tavern_spells_on_battlecry_rally_and_deathrattle() {
    for (id, n) in [("BG34_PreMadeChamp_091", 1), ("BG34_PreMadeChamp_091_G", 2)] {
        let mut g = game(9190);
        set_board(&mut g, 0, &[]);
        give(&mut g, 0, id);
        play_last(&mut g, 0);
        let reward = cid("TB_BaconShop_Triples_01");
        assert_eq!(
            g.players[0]
                .hand
                .iter()
                .filter(|h| h.item.card() == reward)
                .count(),
            usize::from(db().get(cid(id)).is_golden())
        );
        g.players[0].hand.retain(|h| h.item.card() != reward);
        assert_eq!(hand_cards(&g).len(), n, "Battlecry");
        let eid = g.players[0].board[0].eid;
        let mut c = combat_ctx(&mut g);
        c.trigger_rally(0, eid, 0);
        c.destroy(eid);
        c.resolve_deaths();
        drop(c);
        let hand = hand_cards(&g);
        assert_eq!(hand.len(), 3 * n, "Rally and Deathrattle");
        assert!(hand.iter().all(|c| {
            let d = db().get(*c);
            d.kind == taverngym::cards::CardKind::TavernSpell && d.tier <= 1
        }));
    }
}
