//! Buddies part C (P3-7c).

use taverngym::actions::Action;
use taverngym::cards::{cid, db};
use taverngym::state::{Item, Minion};
use taverngym::types::{Kw, PlayerId, Races};
use taverngym::{Game, RulesConfig};

/// A game past hero selection, every seat pinned to Patchwerk with start effects undone.
fn game(seed: u64) -> Game {
    let mut g = Game::new(seed, RulesConfig::default());
    for p in 0..8 {
        g.apply(p, Action::Choose(0)).unwrap();
    }
    for p in 0..8 {
        g.players[p].hero = Some(cid("TB_BaconShop_HERO_34"));
        g.players[p].pending.clear();
        g.players[p].hand.clear();
        g.players[p].stats = Default::default();
        g.players[p].effects.clear();
        g.players[p].quests.clear();
        g.players[p].counters.clear();
        g.players[p].max_gold_bonus = 0;
        g.players[p].board.clear();
        g.players[p].powers.clear();
        g.players[p].set_hero_power(0, cid("TB_BaconShop_HP_035"));
    }
    g.cfg.triple_copies = 99;
    g
}

fn minion(g: &mut Game, p: PlayerId, id: &str) -> Minion {
    g.with_recruit(p, |c| c.new_minion(0, cid(id)))
}

fn set_board(g: &mut Game, p: PlayerId, ids: &[&str]) {
    let b: Vec<Minion> = ids.iter().map(|id| minion(g, p, id)).collect();
    g.players[p].board = b;
}

fn give(g: &mut Game, p: PlayerId, id: &str) -> u32 {
    g.with_recruit(p, |c| c.give_card(0, cid(id)).unwrap())
}

fn play_last(g: &mut Game, p: PlayerId) {
    let hand = g.players[p].hand.len() - 1;
    let pos = g.players[p].board.len();
    g.apply(p, Action::Play { hand, pos }).unwrap();
}

fn stats(g: &Game, p: PlayerId) -> Vec<(i32, i32)> {
    g.players[p].board.iter().map(|m| (m.atk, m.hp)).collect()
}

#[test]
fn titanic_guardian_copies_health_gains_without_looping() {
    let mut g = game(500);
    // Two Guardians (5/2) and a Prisonguard (3/3).
    set_board(
        &mut g,
        0,
        &[
            "TB_BaconShop_HERO_39_Buddy",
            "TB_BaconShop_HERO_39_Buddy",
            "BG36_345",
        ],
    );
    let t = g.players[0].board[2].eid;
    g.with_recruit(0, |c| c.buff(t, 1, 3));
    assert_eq!(stats(&g, 0), vec![(5, 5), (5, 5), (4, 6)]);
    // A hand minion gaining Health counts too.
    let h = give(&mut g, 0, "BG36_345");
    g.with_recruit(0, |c| c.buff(h, 0, 2));
    assert_eq!(stats(&g, 0)[0], (5, 7));
}

#[test]
fn maxwell_gives_the_hero_power_buddy() {
    let mut g = game(501);
    set_board(&mut g, 0, &["TB_BaconShop_HERO_40_Buddy"]);
    g.apply(0, Action::Sell(0)).unwrap();
    let buddy = db().get(cid("TB_BaconShop_HERO_34")).buddy.unwrap();
    assert_eq!(g.players[0].hand[0].item.card(), buddy);
}

#[test]
fn sparkfin_soothsayer_turns_the_tavern_into_murlocs() {
    let seed = (502..700)
        .find(|s| {
            Game::new(*s, RulesConfig::default())
                .lobby_races
                .intersects(Races::MURLOC)
        })
        .unwrap();
    let mut g = game(seed);
    set_board(&mut g, 0, &[]);
    let tiers: Vec<u8> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| m.tier)
        .collect();
    give(&mut g, 0, "TB_BaconShop_HERO_55_Buddy");
    play_last(&mut g, 0);
    let shop: Vec<&Minion> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .collect();
    assert_eq!(shop.iter().map(|m| m.tier).collect::<Vec<_>>(), tiers);
    assert!(shop.iter().all(|m| m.is(Races::MURLOC)));
}

#[test]
fn magnus_and_sklibb_shape_refreshes() {
    let mut g = game(503);
    set_board(
        &mut g,
        0,
        &["TB_BaconShop_HERO_49_Buddy", "TB_BaconShop_HERO_59_Buddy"],
    );
    // (Not the fixture's first Tavern: that one was rolled for the hero picked at the start.)
    let before = g.cfg.shop_minion_slots[0];
    g.players[0].gold = 1;
    g.apply(0, Action::Refresh).unwrap();
    let after: Vec<u8> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| m.tier)
        .collect();
    assert_eq!(after.len(), before + 1, "Sklibb adds one");
    assert!(after.contains(&2));
    // Magnus: two free Refreshes each turn (a per-turn allowance, not banked).
    for p in 0..8 {
        if g.needs_action(p) {
            g.apply(p, Action::EndTurn).unwrap();
        }
    }
    let banked = g.players[0].free_refreshes;
    g.players[0].gold = 10;
    for _ in 0..2 {
        assert_eq!(g.refresh_price(0), 0);
        g.apply(0, Action::Refresh).unwrap();
    }
    assert!(g.refresh_price(0) > 0 || banked > 0);
    assert_eq!(g.players[0].free_refreshes, banked, "nothing banked");
}

#[test]
fn jandices_apprentice_repeat_play() {
    let mut g = game(504);
    set_board(&mut g, 0, &["TB_BaconShop_HERO_71_Buddy"]);
    give(&mut g, 0, "BG36_345");
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0)[0], (5, 6));
    give(&mut g, 0, "BG36_345");
    play_last(&mut g, 0);
    // Tier 1: +1/+1 to all.
    assert_eq!(stats(&g, 0), vec![(6, 7), (4, 4), (4, 4)]);
}

#[test]
fn golden_baby_nzoth_goldens_every_deathrattle_minion() {
    let mut g = game(505);
    set_board(&mut g, 0, &["BG19_010", "BG19_010", "BG36_345"]);
    give(&mut g, 0, "TB_BaconShop_HERO_93_Buddy_G");
    play_last(&mut g, 0);
    let b = &g.players[0].board;
    assert!(b[0].golden && b[1].golden && !b[2].golden);
}

#[test]
fn wandering_treant_and_sinestra_in_combat() {
    let mut g = game(506);
    let a = vec![
        minion(&mut g, 0, "TB_BaconShop_HERO_95_Buddy"),
        minion(&mut g, 0, "TB_BaconShop_HERO_52_Buddy"),
    ];
    let mut taunt = minion(&mut g, 0, "BG36_345");
    taunt.give(Kw::TAUNT);
    let t_eid = taunt.eid;
    let mut a = a;
    a.push(taunt);
    g.players[0].board = a.clone();
    let b = vec![minion(&mut g, 1, "BG36_345")];
    let att = b[0].eid;
    let mut ctx = taverngym::engine::Ctx::new(&mut g, [a, b], [Some(0), Some(1)], true);
    taverngym::combat::do_attack(&mut ctx, 1, att, t_eid);
    drop(ctx);
    // Treant: +1/+1 permanently to every friendly minion; Sinestra: each +1 Attack gain in
    // combat also gives +1 Health permanently.
    let w: Vec<(i32, i32)> = g.players[0].board.iter().map(|m| (m.atk, m.hp)).collect();
    assert_eq!(w[0], (3 + 1, 9 + 1 + 1), "Treant");
    assert_eq!(w[1], (2 + 1, 6 + 1 + 1), "Sinestra");
}

#[test]
fn nexus_lord_upgrades_arcane_alteration() {
    let mut g = game(507);
    g.players[0].gold = 10;
    g.players[0].set_hero_power(0, cid("TB_BaconShop_HP_052"));
    set_board(&mut g, 0, &["TB_BaconShop_HERO_58_Buddy", "BG36_345"]);
    let t = g.players[0].board[1].eid;
    g.apply(0, Action::HeroPower(0)).unwrap();
    let Some(taverngym::state::Pending::Target { options, .. }) =
        g.players[0].pending.last().cloned()
    else {
        panic!("target")
    };
    let i = options.iter().position(|e| *e == t).unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
    assert_eq!(g.players[0].board[1].tier, 2);
}

#[test]
fn ticket_collector_discovers_prizes() {
    let mut g = game(508);
    set_board(&mut g, 0, &["TB_BaconShop_HERO_94_Buddy"]);
    g.apply(0, Action::Sell(0)).unwrap();
    let Some(taverngym::state::Pending::Discover { options, .. }) =
        g.players[0].pending.last().cloned()
    else {
        panic!("discover")
    };
    assert!(!options.is_empty());
    assert!(
        options
            .iter()
            .all(|o| matches!(o, Item::Spell { .. }) || db().get(o.card()).id == "BG33_300")
    );
}
