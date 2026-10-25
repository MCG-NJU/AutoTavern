//! Buddies part A (P3-7a).

use taverngym::actions::Action;
use taverngym::cards::{cid, db};
use taverngym::state::{Minion, Pending};
use taverngym::types::PlayerId;
use taverngym::{Game, RulesConfig};

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

fn give(g: &mut Game, p: PlayerId, id: &str) {
    g.with_recruit(p, |c| c.give_card(0, cid(id)));
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
fn sous_chef_extra_hero_power_use() {
    let mut g = game(500);
    g.players[0].gold = 10;
    g.players[0].set_hero_power(0, cid("TB_BaconShop_HP_010")); // George: give Divine Shield
    set_board(&mut g, 0, &["BG21_HERO_020_Buddy", "BG36_345"]);
    let t = g.players[0].board[1].eid;
    for k in 0..2 {
        g.apply(0, Action::HeroPower(0)).unwrap();
        if let Some(Pending::Target { options, .. }) = g.players[0].pending.last().cloned() {
            let i = options.iter().position(|e| *e == t).unwrap_or(0);
            g.apply(0, Action::Choose(i)).unwrap();
        }
        assert_eq!(g.players[0].powers[0].uses_turn, k + 1);
    }
    assert!(!g.can_hero_power(0, 0), "only one extra use");
}

#[test]
fn deaths_head_sage_copies_blood_gems_once() {
    let mut g = game(501);
    set_board(&mut g, 0, &["BG20_HERO_103_Buddy_G"]);
    give(&mut g, 0, "BG20_GEM");
    assert_eq!(
        g.players[0].hand.len(),
        3,
        "gem + 2 copies, copies don't chain"
    );
}

#[test]
fn dranosh_and_medico_on_buy() {
    let mut g = game(502);
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG20_HERO_102_Buddy"]);
    let slot = g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_some())
        .unwrap();
    let (a, h) = g.players[0].shop[slot]
        .item
        .as_minion()
        .map(|m| (m.atk, m.hp))
        .unwrap();
    g.apply(0, Action::Buy(slot)).unwrap();
    assert_eq!(stats(&g, 0)[0], (6 + a / 2, 6 + h / 2));
}

#[test]
fn baby_elekk_improves_by_base() {
    let mut g = game(503);
    set_board(&mut g, 0, &["BG20_HERO_101_Buddy"]); // 4/4
    for expect in [2, 3] {
        give(&mut g, 0, "BG_ICC_026t"); // 1/1 Skeleton
        play_last(&mut g, 0);
        assert_eq!(g.players[0].board.last().unwrap().atk, expect);
    }
}

#[test]
fn lieutenants_end_of_turn() {
    let mut g = game(504);
    set_board(
        &mut g,
        0,
        &["BG36_345", "BG22_HERO_002_Buddy", "BG22_HERO_003_Buddy_G"],
    );
    g.apply(0, Action::EndTurn).unwrap();
    let s = stats(&g, 0);
    assert_eq!(s[0].0, 3 + 10);
    assert_eq!(s[2].1, db().get(cid("BG22_HERO_003_Buddy_G")).hp + 20);
}

#[test]
fn talent_scout_makes_a_buddy_golden() {
    let mut g = game(505);
    set_board(&mut g, 0, &["BG22_HERO_002_Buddy"]);
    give(&mut g, 0, "BG25_HERO_105_Buddy");
    play_last(&mut g, 0);
    // A targeted Battlecry (client targeting arrow): the player chooses the Buddy.
    assert!(matches!(
        g.players[0].pending.last(),
        Some(Pending::Target { options, .. }) if options.len() == 1
    ));
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].board[0].golden);
}

#[test]
fn shady_aristocrat_quest_pays_a_coin_pouch() {
    let mut g = game(506);
    g.players[0].gold = 0;
    set_board(&mut g, 0, &["BG24_HERO_100_Buddy"]);
    g.apply(0, Action::Sell(0)).unwrap();
    let Some(Pending::Quest { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("quest choice")
    };
    assert!(
        options
            .iter()
            .all(|(_, r)| *r == cid("BG24_HERO_100_Buddyt"))
    );
    g.apply(0, Action::Choose(0)).unwrap();
    let (q, goal) = (g.players[0].quests[0].card, g.players[0].quests[0].goal);
    g.with_recruit(0, |c| c.quest_progress(0, q, goal));
    assert!(
        g.players[0].rewards.is_empty(),
        "the pouch is a card, not a passive"
    );
    let gold = g.players[0].gold;
    play_last(&mut g, 0);
    assert_eq!(g.players[0].gold, gold + 8);
}

#[test]
fn tamuzo_doubles_combat_summons_and_gadrin() {
    let mut g = game(507);
    let a = vec![minion(&mut g, 0, "BG23_HERO_201_Buddy")];
    let mut ctx = taverngym::engine::Ctx::new(&mut g, [a, vec![]], [Some(0), None], true);
    let m = ctx.new_minion(0, cid("BG36_345"));
    ctx.summon(0, 1, m);
    assert_eq!((ctx.boards[0][1].atk, ctx.boards[0][1].hp), (6, 6));
    drop(ctx);
    // Master Gadrin (golden): both neighbours gain Health equal to their Attack.
    let mut g = game(508);
    g.cfg.record_log = true;
    set_board(
        &mut g,
        0,
        &["BG36_345", "BG20_HERO_201_Buddy_G", "BG20_HERO_102_Buddy"],
    );
    set_board(&mut g, 1, &["BG36_345"]);
    g.simulate_combat(0, Some(1), None);
    let b = &g.last_frames[1].boards[0];
    assert_eq!((b[0].hp, b[2].hp), (6, 12));
}

#[test]
fn veranus_transforms_left_neighbour_up() {
    let mut g = game(509);
    set_board(&mut g, 0, &["BG36_345", "BG27_HERO_801_Buddy"]);
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(g.players[0].board[0].tier, 2);
}
