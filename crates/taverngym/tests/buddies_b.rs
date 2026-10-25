//! Buddies part B: representative behaviour tests.

use taverngym::actions::Action;
use taverngym::cards::{cid, db};
use taverngym::state::{Item, Minion, Pending};
use taverngym::types::{Kw, PlayerId};
use taverngym::{Game, RulesConfig};

/// A game past hero selection with every seat pinned to Patchwerk and a clean slate.
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

fn first_minion_slot(g: &Game) -> usize {
    g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_some())
        .unwrap()
}

#[test]
fn si7_scout_and_malorne_grow_from_buying() {
    let mut g = game(500);
    g.players[0].gold = 10;
    set_board(
        &mut g,
        0,
        &["TB_BaconShop_HERO_01_Buddy", "BG32_HERO_001_Buddy"],
    ); // 2/2, 1/1
    g.apply(0, Action::Buy(first_minion_slot(&g))).unwrap(); // 3 Gold spent
    assert_eq!(stats(&g, 0), vec![(4, 4), (2, 2)]);
}

#[test]
fn lucifron_repeats_end_of_turn_effects() {
    let mut g = game(501);
    set_board(
        &mut g,
        0,
        &["TB_BaconShop_HERO_10_Buddy", "TB_BaconShop_HERO_11_Buddy_G"],
    );
    g.apply(0, Action::EndTurn).unwrap();
    // Bilgewater Mogul (+1) triggers 1 + 2 extra times.
    assert_eq!(g.players[0].max_gold_bonus, 3);
}

#[test]
fn unearthed_underling_rewinds_and_grows() {
    let mut g = game(502);
    set_board(&mut g, 0, &["TB_BaconShop_HERO_25_Buddy"]); // 5/5
    let total = g.players[0].health + g.players[0].armor;
    g.with_recruit(0, |c| c.hero_damage(0, 3));
    assert_eq!(g.players[0].health + g.players[0].armor, total);
    assert_eq!(stats(&g, 0), vec![(8, 8)]);
}

#[test]
fn tentacle_and_mishmash_watch_stat_gains() {
    let mut g = game(503);
    // Tentacle 3/3, Mishmash 4/4, the Curator's Amalgam 2/2.
    set_board(
        &mut g,
        0,
        &[
            "TB_BaconShop_HERO_29_Buddy",
            "TB_BaconShop_HERO_33_Buddy",
            "TB_BaconShop_HP_033t",
        ],
    );
    let amalgam = g.players[0].board[2].eid;
    g.with_recruit(0, |c| c.buff(amalgam, 2, 3));
    let b = stats(&g, 0);
    // Tentacle: +1/+1 once (the Amalgam's gain; Mishmash's copy doesn't retrigger it).
    assert_eq!(b[0], (4, 4));
    assert_eq!(b[1], (6, 7), "Mishmash copies the Amalgam's gain");
    assert_eq!(g.players[0].board[0].temp_atk, 1, "until next turn");
}

#[test]
fn probius_makes_its_mech_host_golden() {
    let mut g = game(504);
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG36_854"]); // Rescue Bot (Mech)
    give(&mut g, 0, "BG31_HERO_802_Buddy");
    g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
    assert!(g.players[0].board[0].golden);
}

#[test]
fn karl_buffs_shielded_minions_after_hero_power() {
    let mut g = game(505);
    g.players[0].gold = 10;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_010")); // George: give a minion Divine Shield
    set_board(&mut g, 0, &["TB_BaconShop_HERO_15_Buddy", "BGS_034"]); // Karl 3/4, Bronze Warden (DS)
    let karl = g.players[0].board[0].eid;
    g.apply(0, Action::HeroPower(0)).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("target")
    };
    let i = options.iter().position(|e| *e == karl).unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
    // Both now have Divine Shield and get +2 Attack.
    assert_eq!(g.players[0].board[0].atk, 3 + 2);
    assert_eq!(g.players[0].board[1].atk, 2 + 2);
}

#[test]
fn stoneshell_guardian_upgrades_the_first_triple_reward() {
    let mut g = game(506);
    set_board(&mut g, 0, &["BG33_HERO_000_Buddy"]);
    for k in 0..2 {
        g.with_recruit(0, |c| {
            c.add_to_hand(
                0,
                Item::Spell {
                    card: cid("TB_BaconShop_Triples_01"),
                    data: 2,
                },
            );
        });
        play_last(&mut g, 0);
        let Some(Pending::Discover { options, .. }) = g.players[0].pending.last().cloned() else {
            panic!("discover")
        };
        let golden = options
            .iter()
            .all(|o| o.as_minion().is_some_and(|m| m.golden));
        assert_eq!(golden, k == 0, "only the first each turn");
        g.apply(0, Action::Choose(0)).unwrap();
    }
}

#[test]
fn eclipsion_illidari_first_attacker_is_immune_once() {
    let mut g = game(507);
    let a = vec![
        minion(&mut g, 0, "TB_BaconShop_HERO_08_Buddy"),
        minion(&mut g, 0, "BG35_155"), // 8/8
    ];
    let b = vec![minion(&mut g, 1, "BG34_322")]; // Stalwart Kodo 16/32 survives both hits
    let (att, def) = (a[1].eid, b[0].eid);
    let mut ctx = taverngym::engine::Ctx::new(&mut g, [a, b], [Some(0), Some(1)], true);
    taverngym::combat::do_attack(&mut ctx, 0, att, def);
    let m = ctx.boards[0].iter().find(|m| m.eid == att).unwrap();
    assert_eq!(m.hp, 8, "no damage taken");
    assert!(!m.has(Kw::IMMUNE_ATTACKING), "one attack only");
    taverngym::combat::do_attack(&mut ctx, 0, att, def);
    let m = ctx.boards[0].iter().find(|m| m.eid == att).unwrap();
    assert!(m.hp < 8, "second attack is not immune");
    let _ = db();
}

#[test]
fn apostle_of_galakrond_raises_tavern_tiers() {
    let mut g = game(508);
    let before: Vec<u8> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| m.tier)
        .collect();
    give(&mut g, 0, "TB_BaconShop_HERO_02_Buddy");
    play_last(&mut g, 0);
    let after: Vec<u8> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| m.tier)
        .collect();
    assert_eq!(before.len(), after.len());
    assert!(before.iter().zip(&after).all(|(b, a)| *a == (b + 1).min(6)));
}
