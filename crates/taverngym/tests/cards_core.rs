//! Card rules: single cards and their interactions.
mod common;
use common::*;

fn game(seed: u64) -> Game {
    let mut g = common::game(seed);
    g.cfg.experimental_mode = true;
    g
}

#[test]
fn stats_saturate_at_signed_int_max_and_temp_expiry_restores_real_gain() {
    let mut g = game(4100);
    set_board(&mut g, 0, &["BG21_015"]);
    let eid = g.players[0].board[0].eid;
    g.players[0].board[0].set_stats(i32::MAX - 5, i32::MAX - 5);
    g.with_recruit(0, |c| c.buff_temp(eid, 20, 20));
    let m = &g.players[0].board[0];
    assert_eq!((m.atk, m.max_hp), (i32::MAX, i32::MAX));
    assert_eq!((m.temp_atk, m.temp_hp), (5, 5));
    g.start_turn();
    assert_eq!(stats(&g, 0), vec![(i32::MAX - 5, i32::MAX - 5)]);
}

#[test]
fn set_stats_keeps_player_aura_and_no_stat_gain_covers_shared_paths() {
    let mut g = game(4101);
    set_board(&mut g, 0, &["BG36_372"]);
    g.players[0].health = 10;
    g.with_recruit(0, |c| {
        let eid = c.boards[0][0].eid;
        c.set_stats(eid, 2, 2);
        assert_eq!((c.boards[0][0].atk, c.boards[0][0].max_hp), (32, 32));
    });
    let fish = db()
        .iter()
        .find(|d| {
            !d.is_golden()
                && taverngym::script::script(d.cid)
                    .is_some_and(|s| s.has_flag(taverngym::script::flag::NO_STAT_GAIN))
        })
        .unwrap()
        .cid;
    g.with_recruit(0, |c| {
        let mut m = c.new_minion(0, fish);
        let before = (m.atk, m.max_hp);
        m.add_stats(10, 10);
        m.set_stats(20, 20);
        assert_eq!((m.atk, m.max_hp), before);
        let eid = m.eid;
        c.boards[0] = vec![m];
        c.set_stats(eid, 40, 40);
        c.buff_temp(eid, 20, 20);
        c.buff_perm(0, eid, 20, 20);
        assert_eq!((c.boards[0][0].atk, c.boards[0][0].max_hp), before);
    });
}

#[test]
fn gold_tarecgosa_keeps_temp_and_explicit_permanent_stats_exactly_twice() {
    let mut g = game(4102);
    let card = db().golden_of(cid("BG21_015"));
    let m = g.with_recruit(0, |c| c.new_minion(0, card));
    let eid = m.eid;
    let before = (m.atk, m.max_hp);
    g.players[0].board = vec![m.clone()];
    let mut c = Ctx::new(&mut g, [vec![m], vec![]], [Some(0), None], true);
    c.buff_temp(eid, 3, 5);
    c.buff_perm(0, eid, 7, 11);
    let persistent = &c.g.players[0].board[0];
    assert_eq!(
        (persistent.atk, persistent.max_hp),
        (before.0 + 20, before.1 + 32)
    );
    // The fighting minion has the doubled gains at once (Training Certificate on a golden Tarecgosa
    // tripled it before the first attack); the warband keeps the same.
    assert_eq!(
        (c.boards[0][0].atk, c.boards[0][0].max_hp),
        (before.0 + 20, before.1 + 32)
    );
    assert_eq!((persistent.temp_atk, persistent.temp_hp), (0, 0));
    assert!(persistent.mods.is_empty());
}

#[test]
fn moira_takes_maximum_while_titus_adds_extra_triggers() {
    let mut g = game(4103);
    set_board(&mut g, 0, &["BG27_518", "BG27_518"]);
    g.with_recruit(0, |c| assert_eq!(c.deathrattle_mult(0), 2));
    let gold = db().golden_of(cid("BG27_518"));
    let m = g.with_recruit(0, |c| c.new_minion(0, gold));
    g.players[0].board.push(m);
    g.with_recruit(0, |c| {
        assert_eq!(c.deathrattle_mult(0), 3);
        let titus = c.new_minion(0, cid("BG25_354"));
        c.boards[0].push(titus);
        assert_eq!(c.deathrattle_mult(0), 4);
    });
}

#[test]
fn bobblehead_excludes_first_two_tiers_even_in_tier_one_tavern() {
    let mut g = game(4104);
    g.players[0].tier = 1;
    g.players[0].trinkets = vec![cid("BG30_MagicItem_998")];
    g.with_recruit(0, |c| {
        c.refresh_shop(0);
        let minions: Vec<_> = c
            .player_ref(0)
            .unwrap()
            .shop
            .iter()
            .filter_map(|s| s.item.as_minion())
            .collect();
        assert!(!minions.is_empty());
        assert!(minions.iter().all(|m| m.tier == 3));
    });
}

#[test]
fn reborn_awakened_deity_remains_connected_to_permanent_growth() {
    let mut g = game(4105);
    let mut m = minion(&mut g, 0, "BGFYM_011");
    m.give(Kw::REBORN);
    let old = m.eid;
    let mut c = Ctx::new(&mut g, [vec![m], vec![]], [Some(0), None], true);
    c.cs[0].deity_eid = old;
    c.boards[0][0].destroyed = true;
    c.resolve_deaths();
    assert_eq!(c.boards[0].len(), 1);
    let eid = c.boards[0][0].eid;
    assert_ne!(old, eid);
    assert_eq!(c.cs[0].deity_eid, eid);
    let before = (c.boards[0][0].atk, c.boards[0][0].max_hp);
    taverngym::content::buff_deity(&mut c, 0, 2, 1);
    assert_eq!(
        (c.boards[0][0].atk, c.boards[0][0].max_hp),
        (before.0 + 2, before.1 + 1)
    );
}

#[test]
fn triple_keeps_each_attachment_state_instead_of_highest_stage() {
    let mut g = game(4106);
    g.cfg.triple_copies = 3;
    let gift = cid("BG36_MidGameEffect_000t51");
    g.with_recruit(0, |c| {
        for stage in [0, 1, 2] {
            let mut m = c.new_minion(0, cid("BG21_015"));
            m.attach(gift, stage);
            c.add_to_hand(0, Item::Minion(m));
        }
    });
    let gold = g.players[0]
        .hand
        .iter()
        .find_map(|h| h.item.as_minion())
        .unwrap();
    assert!(gold.golden);
    assert_eq!(gold.attachment_data, vec![0, 1, 2]);
}

#[test]
fn timewarp_purchase_pauses_board_even_when_it_automatically_closes_the_visit() {
    for (price, another_card) in [(1, true), (2, false)] {
        let mut g = game(4200 + price as u64);
        set_board(&mut g, 0, &["BG34_Giant_327", "BG34_Giant_327"]);
        let before = stats(&g, 0);
        let card = g.with_recruit(0, |c| Item::Minion(c.new_minion(0, cid("BG21_015"))));
        let mut options = vec![(card, price)];
        if another_card {
            options.push((Item::spell(cid("BG28_810")), 1));
        }
        g.players[0].pending.push(Pending::Timewarp {
            options,
            chronum: 2,
            major: true,
        });
        g.apply(0, Action::Choose(0)).unwrap();
        assert_eq!(stats(&g, 0), before);
        assert!(
            g.players[0]
                .hand
                .iter()
                .any(|h| h.item.card() == cid("BG21_015"))
        );
    }
}

#[test]
fn navigator_requires_an_eligible_target_using_its_printed_tier_limit() {
    let mut g = game(4300);
    let pirate = db()
        .iter()
        .find(|d| {
            d.kind == taverngym::cards::CardKind::Minion
                && d.normal.is_none()
                && d.golden.is_some()
                && d.tier == 4
                && d.races.intersects(Races::PIRATE)
        })
        .unwrap()
        .cid;
    let m = g.with_recruit(0, |c| c.new_minion(0, pirate));
    let eid = m.eid;
    g.players[0].board = vec![m];
    // The card says Tier 4 or below, independently of the owner's Tavern Tier.
    g.players[0].tier = 1;
    give(&mut g, 0, "BG32_231");
    play_last(&mut g, 0);
    assert!(
        matches!(g.players[0].pending.last(), Some(Pending::Target { options, .. }) if options == &vec![eid])
    );
    assert!(!g.legal_mask(0)[taverngym::actions::A_CANCEL]);
    assert!(g.apply(0, Action::Cancel).is_err());
    assert_eq!(g.players[0].stats.battlecries_triggered, 0);
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].board[0].golden);
    assert_eq!(g.players[0].stats.battlecries_triggered, 1);

    let mut empty = game(4301);
    give(&mut empty, 0, "BG32_231");
    play_last(&mut empty, 0);
    assert!(empty.players[0].pending.is_empty());
    assert_eq!(empty.players[0].stats.battlecries_triggered, 1);
}

#[test]
fn temporary_gains_clipped_by_the_golden_stat_cap_do_not_erase_permanent_stats() {
    let mut g = game(4400);
    g.cfg.triple_copies = 3;
    g.with_recruit(0, |c| {
        for _ in 0..3 {
            let mut m = c.new_minion(0, cid("BG21_015"));
            m.set_stats(1_200_000_000, 1_200_000_000);
            m.add_stats(10, 10);
            m.temp_atk = 10;
            m.temp_hp = 10;
            c.add_to_hand(0, Item::Minion(m));
        }
    });
    let m = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((m.atk, m.max_hp), (i32::MAX, i32::MAX));
    assert_eq!((m.temp_atk, m.temp_hp), (0, 0));
    g.start_turn();
    let m = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((m.atk, m.max_hp), (i32::MAX, i32::MAX));
}
