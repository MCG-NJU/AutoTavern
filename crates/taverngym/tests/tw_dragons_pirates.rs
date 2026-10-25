//! Tests for the Timewarp cards of `content/tw_dragons_pirates.rs`.

mod common;
#[allow(unused_imports)]
use common::*;

use taverngym::combat::{attack_now, run_rally};

const BUSKER: &str = "BG34_Giant_001";
const HOOKTAIL: &str = "BG34_Giant_015";
const DRAGONLING: &str = "BG34_Giant_029";
const GREENSKEEPER: &str = "BG34_Giant_041";
const PROMO_DRAKE: &str = "BG34_Giant_088";
const RED_WHELP: &str = "BG34_Giant_091";
const PAGLE: &str = "BG34_Giant_208";
const PEGGY: &str = "BG34_Giant_327";
const SAILOR: &str = "BG34_Giant_589";
const COLLECTOR: &str = "BG34_Giant_680";
const DUSKMAW: &str = "BG34_PreMadeChamp_020";
const PRISMSCALE: &str = "BG34_PreMadeChamp_022";
const RAIDER: &str = "BG34_PreMadeChamp_065";
const PLUNDERER: &str = "BG34_PreMadeChamp_067";
const CENTURION: &str = "BG34_PreMadeChamp_200";

/// Vanilla bodies.
const DEVILSAUR: &str = "BG22_001t2"; // 8/8 Beast
const GOLEM: &str = "BG29_864t"; // 5/5
/// Tavern spells without a target.
const STRIKE_OIL: &str = "BG28_805"; // Tier 2: +1 maximum Gold
const AZERITE: &str = "BG28_169"; // Tier 6: minions +2/+2 twice

fn start_of_combat(c: &mut Ctx, side: usize, eid: u32) {
    c.run_minion_hook(side, eid, |s| s.start_of_combat, 1, "start of combat");
}

fn hand_count(g: &Game, p: PlayerId, id: &str) -> usize {
    let card = cid(id);
    g.players[p]
        .hand
        .iter()
        .filter(|h| h.item.card() == card)
        .count()
}

#[test]
fn busker_gold_next_turn_on_battlecry_and_deathrattle() {
    let mut g = game(1);
    give(&mut g, 0, BUSKER);
    play_last(&mut g, 0);
    assert_eq!(g.players[0].gold_next_turn, 1);
    let eid = g.players[0].board[0].eid;
    {
        let mut c = combat_ctx(&mut g);
        c.destroy(eid);
        c.resolve_deaths();
    }
    assert_eq!(g.players[0].gold_next_turn, 2);
    // Golden: 2 Gold each.
    give(&mut g, 0, "BG34_Giant_001_G");
    play_last(&mut g, 0);
    assert_eq!(g.players[0].gold_next_turn, 4);
}

#[test]
fn hooktail_buffs_all_minions_per_tavern_spell() {
    let mut g = game(2);
    set_board(&mut g, 0, &[HOOKTAIL, DEVILSAUR]);
    give(&mut g, 0, STRIKE_OIL);
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0), vec![(8, 14), (10, 10)]);
    // Golden: twice.
    set_board(&mut g, 0, &["BG34_Giant_015_G", DEVILSAUR]);
    give(&mut g, 0, STRIKE_OIL);
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0), vec![(16, 28), (12, 12)]);
}

#[test]
fn dragonling_gives_tier_stats_to_itself_and_neighbors() {
    let mut g = game(3);
    g.players[0].tier = 3;
    set_board(
        &mut g,
        0,
        &[
            DEVILSAUR,
            DRAGONLING,
            DEVILSAUR,
            DEVILSAUR,
            "BG34_Giant_029_G",
        ],
    );
    let (a, b) = (g.players[0].board[1].eid, g.players[0].board[4].eid);
    let mut c = combat_ctx(&mut g);
    start_of_combat(&mut c, 0, a);
    let st = |c: &Ctx| -> Vec<(i32, i32)> {
        c.friendly(0)
            .iter()
            .map(|e| c.minion(*e).map(|m| (m.atk, m.hp)).unwrap())
            .collect()
    };
    assert_eq!(
        st(&c),
        vec![(11, 11), (6, 6), (11, 11), (8, 8), (6, 6)],
        "+3/+3 = Tier 3"
    );
    // Golden: twice the Tier, to itself and its only neighbor.
    start_of_combat(&mut c, 0, b);
    assert_eq!(st(&c), vec![(11, 11), (6, 6), (11, 11), (14, 14), (12, 12)]);
}

#[test]
fn greenskeeper_triggers_rightmost_battlecry_and_deathrattle() {
    let mut g = game(4);
    // Right-most Battlecry: Busker (+1 Gold next turn); right-most Deathrattle: Plunderer
    // (+2 maximum Gold).
    set_board(&mut g, 0, &[BUSKER, GREENSKEEPER, PLUNDERER]);
    set_board(&mut g, 1, &[DEVILSAUR]);
    let gk = g.players[0].board[1].eid;
    let foe = g.players[1].board[0].eid;
    {
        let mut c = combat_ctx(&mut g);
        run_rally(&mut c, 0, gk, foe);
        assert_eq!(c.friendly(0).len(), 3, "nothing died");
    }
    assert_eq!(g.players[0].gold_next_turn, 1);
    assert_eq!(g.players[0].max_gold_bonus, 2);

    // One minion with both; golden Greenskeeper: twice.
    let mut g = game(4);
    set_board(&mut g, 0, &[PLUNDERER, "BG34_Giant_041_G", BUSKER]);
    set_board(&mut g, 1, &[DEVILSAUR]);
    let gk = g.players[0].board[1].eid;
    let foe = g.players[1].board[0].eid;
    {
        let mut c = combat_ctx(&mut g);
        run_rally(&mut c, 0, gk, foe);
    }
    assert_eq!(
        g.players[0].gold_next_turn, 4,
        "Battlecry + Deathrattle, twice"
    );
    assert_eq!(g.players[0].max_gold_bonus, 0);
}

#[test]
fn promo_drake_improves_at_end_of_turn() {
    let mut g = game(5);
    set_board(&mut g, 0, &[PROMO_DRAKE, DEVILSAUR]);
    let drake = g.players[0].board[0].eid;
    {
        let mut c = combat_ctx(&mut g);
        start_of_combat(&mut c, 0, drake);
        assert_eq!(c.minion(drake).map(|m| (m.atk, m.hp)), Some((11, 11)));
    }
    end_turn_all(&mut g);
    assert_eq!(g.players[0].board[0].counter, 1);
    let dev = g.players[0].board[1].eid;
    {
        let mut c = combat_ctx(&mut g);
        start_of_combat(&mut c, 0, drake);
        assert_eq!(c.minion(dev).map(|m| (m.atk, m.hp)), Some((18, 18)));
    }
    // Golden: +10/+10 base.
    set_board(&mut g, 0, &["BG34_Giant_088_G", DEVILSAUR]);
    g.players[0].board[0].counter = 2;
    let drake = g.players[0].board[0].eid;
    let dev = g.players[0].board[1].eid;
    {
        let mut c = combat_ctx(&mut g);
        start_of_combat(&mut c, 0, drake);
        // Golden base 10 plus retained growth 2 * 5, on an 8/8 body.
        assert_eq!(c.minion(dev).map(|m| (m.atk, m.hp)), Some((28, 28)));
    }
    end_turn_all(&mut g);
    assert_eq!(
        g.players[0].board[0].counter, 4,
        "golden improves by two normal units"
    );
    let mut c = combat_ctx(&mut g);
    start_of_combat(&mut c, 0, drake);
    assert_eq!(c.minion(dev).map(|m| (m.atk, m.hp)), Some((38, 38)));
}

/// Health lost by each of player 1's (8/8) minions after the Whelp's Start of Combat.
fn whelp_damage(g: &mut Game) -> Vec<i32> {
    let whelp = g.players[0].board[0].eid;
    let mut c = combat_ctx(g);
    start_of_combat(&mut c, 0, whelp);
    let mut lost: Vec<i32> = c
        .friendly(1)
        .iter()
        .map(|e| 8 - c.minion(*e).unwrap().hp)
        .collect();
    lost.sort();
    lost
}

#[test]
fn red_whelp_hits_random_enemies_and_improves_per_dragon_played() {
    let mut g = game(6);
    set_board(&mut g, 0, &[RED_WHELP]);
    set_board(&mut g, 1, &[DEVILSAUR; 5]);
    assert_eq!(whelp_damage(&mut g), vec![0, 0, 0, 3, 3]);
    // A non-Dragon doesn't improve it; a Dragon does.
    give(&mut g, 0, BUSKER);
    play_last(&mut g, 0);
    assert_eq!(g.players[0].board[0].counter, 0);
    give(&mut g, 0, DRAGONLING);
    play_last(&mut g, 0);
    assert_eq!(g.players[0].board[0].counter, 3, "normal damage growth");
    assert_eq!(whelp_damage(&mut g), vec![0, 0, 0, 6, 6]);
    // Golden: 2 damage to four.
    set_board(&mut g, 0, &["BG34_Giant_091_G"]);
    assert_eq!(whelp_damage(&mut g), vec![0, 2, 2, 2, 2]);
    give(&mut g, 0, BUSKER);
    play_last(&mut g, 0);
    assert_eq!(
        g.players[0].board[0].counter, 0,
        "non-Dragon does not improve golden"
    );
    give(&mut g, 0, DRAGONLING);
    play_last(&mut g, 0);
    assert_eq!(
        g.players[0].board[0].counter, 2,
        "own golden-text damage growth"
    );
    assert_eq!(whelp_damage(&mut g), vec![0, 4, 4, 4, 4]);
}

#[test]
fn pagle_triple_reward_once_per_combat_only_when_attacking() {
    let reward = "TB_BaconShop_Triples_01";
    for (id, n) in [(PAGLE, 1), ("BG34_Giant_208_G", 2)] {
        let mut g = game(7);
        set_board(&mut g, 0, &[id]);
        g.players[0].board[0].hp = 100;
        set_board(&mut g, 1, &[GOLEM, GOLEM, GOLEM]);
        let pagle = g.players[0].board[0].eid;
        let foe = g.players[1].board[0].eid;
        {
            let mut c = combat_ctx(&mut g);
            // Killing an attacker while defending doesn't count.
            attack_now(&mut c, 1, foe);
            assert_eq!(c.friendly(1).len(), 2);
            assert_eq!(c.player_ref(0).unwrap().hand.len(), 0);
            attack_now(&mut c, 0, pagle);
            assert_eq!(c.friendly(1).len(), 1);
            assert_eq!(c.player_ref(0).unwrap().hand.len(), n);
            // Once per combat.
            attack_now(&mut c, 0, pagle);
            assert_eq!(c.friendly(1).len(), 0);
            assert_eq!(c.player_ref(0).unwrap().hand.len(), n);
        }
        assert_eq!(hand_count(&g, 0, reward), n);
        assert_eq!(g.players[0].board[0].counter, 0, "state is per combat");
    }
}

#[test]
fn peggy_buffs_pirates_when_a_card_is_added() {
    let mut g = game(8);
    set_board(&mut g, 0, &[PEGGY, BUSKER, DEVILSAUR]);
    give(&mut g, 0, STRIKE_OIL);
    assert_eq!(stats(&g, 0), vec![(10, 6), (5, 3), (8, 8)]);
    set_board(&mut g, 0, &["BG34_Giant_327_G", BUSKER, DEVILSAUR]);
    give(&mut g, 0, STRIKE_OIL);
    assert_eq!(stats(&g, 0), vec![(20, 12), (6, 4), (8, 8)]);
}

#[test]
fn sailor_adds_pirates_to_each_refresh() {
    let seed = (0..200)
        .find(|s| game(*s).lobby_races.contains(Races::PIRATE))
        .expect("a lobby with Pirates");
    let shop_after_refresh = |board: &[&str]| -> (usize, usize) {
        let mut g = game(seed);
        g.players[0].tier = 2; // a small Tavern: the extras fit under the 7-card cap
        set_board(&mut g, 0, board);
        g.apply(0, Action::Refresh).unwrap();
        let pirates = g.players[0]
            .shop
            .iter()
            .filter(|s| matches!(&s.item, Item::Minion(m) if m.is(Races::PIRATE)))
            .count();
        (g.players[0].shop.len(), pirates)
    };
    let (base, _) = shop_after_refresh(&[DEVILSAUR]);
    let (n, pirates) = shop_after_refresh(&[SAILOR]);
    assert_eq!(n, base + 1);
    assert!(pirates >= 1);
    let (n, pirates) = shop_after_refresh(&["BG34_Giant_589_G"]);
    assert_eq!(n, base + 2);
    assert!(pirates >= 2);
    let mut g = game(seed);
    assert!(minion(&mut g, 0, SAILOR).has(Kw::DIVINE_SHIELD));
}

#[test]
fn collector_cleaves_and_shields_with_four_goldens() {
    let mut g = game(9);
    let golden_busker = "BG34_Giant_001_G";
    set_board(
        &mut g,
        0,
        &[COLLECTOR, golden_busker, golden_busker, golden_busker],
    );
    set_board(&mut g, 1, &[DEVILSAUR]);
    assert!(g.players[0].board[0].has(Kw::CLEAVE));
    let col = g.players[0].board[0].eid;
    let foe = g.players[1].board[0].eid;
    {
        let mut c = combat_ctx(&mut g);
        run_rally(&mut c, 0, col, foe);
        assert!(
            !c.minion(col).unwrap().has(Kw::DIVINE_SHIELD),
            "only 3 Golden"
        );
    }
    // A Golden Collector counts itself.
    set_board(
        &mut g,
        0,
        &[
            "BG34_Giant_680_G",
            golden_busker,
            golden_busker,
            golden_busker,
        ],
    );
    let col = g.players[0].board[0].eid;
    let mut c = combat_ctx(&mut g);
    run_rally(&mut c, 0, col, foe);
    assert!(c.minion(col).unwrap().has(Kw::DIVINE_SHIELD));
}

#[test]
fn duskmaw_and_prismscale_avenge() {
    let mut g = game(10);
    set_board(
        &mut g,
        0,
        &[DUSKMAW, PRISMSCALE, DEVILSAUR, DEVILSAUR, DEVILSAUR],
    );
    let ids: Vec<u32> = g.players[0].board.iter().map(|m| m.eid).collect();
    {
        let mut c = combat_ctx(&mut g);
        c.destroy(ids[4]);
        c.resolve_deaths();
        let st = |c: &Ctx, e: u32| c.minion(e).map(|m| (m.atk, m.hp)).unwrap();
        // Avenge (1): Dragons +6/+4 (both are Dragons), not the Beast.
        assert_eq!(st(&c, ids[0]), (12, 18));
        assert_eq!(st(&c, ids[1]), (14, 16));
        assert_eq!(st(&c, ids[2]), (8, 8));
        assert_eq!(c.player_ref(0).unwrap().hand.len(), 0, "Avenge (2)");
        c.destroy(ids[3]);
        c.resolve_deaths();
        assert_eq!(st(&c, ids[0]), (18, 22));
    }
    assert_eq!(hand_count(&g, 0, AZERITE), 1);

    // Golden: +12/+8 and 2 Azerite Empowerments.
    let mut g = game(10);
    set_board(
        &mut g,
        0,
        &[
            "BG34_PreMadeChamp_020_G",
            "BG34_PreMadeChamp_022_G",
            DEVILSAUR,
            DEVILSAUR,
        ],
    );
    let ids: Vec<u32> = g.players[0].board.iter().map(|m| m.eid).collect();
    {
        let mut c = combat_ctx(&mut g);
        c.destroy(ids[3]);
        c.destroy(ids[2]);
        c.resolve_deaths();
        // Two deaths: Avenge (1) twice.
        assert_eq!(c.minion(ids[0]).map(|m| (m.atk, m.hp)), Some((36, 44)));
    }
    assert_eq!(hand_count(&g, 0, AZERITE), 2);
}

#[test]
fn raider_buffs_pirates_after_a_tier_4_plus_card() {
    let mut g = game(11);
    set_board(&mut g, 0, &[RAIDER, DEVILSAUR]);
    // Tier 3 minion, Tier 2 spell: nothing.
    give(&mut g, 0, BUSKER);
    play_last(&mut g, 0);
    give(&mut g, 0, STRIKE_OIL);
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0), vec![(14, 6), (8, 8), (4, 2)]);
    // Tier 5 minion (a Pirate: it is in play when the effect resolves).
    give(&mut g, 0, PLUNDERER);
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0), vec![(17, 8), (8, 8), (7, 4), (18, 7)]);
    // Playing a Raider doesn't trigger itself, only the one already in play.
    give(&mut g, 0, "BG34_PreMadeChamp_065_G");
    play_last(&mut g, 0);
    assert_eq!(
        stats(&g, 0),
        vec![(20, 10), (8, 8), (10, 6), (21, 9), (31, 14)]
    );
    // Tier 6 spell (Azerite: +2/+2 twice to all); normal Raider once, golden twice.
    give(&mut g, 0, AZERITE);
    play_last(&mut g, 0);
    assert_eq!(
        stats(&g, 0),
        vec![(33, 20), (12, 12), (23, 16), (34, 19), (44, 24)]
    );
}

#[test]
fn plunderer_deathrattle_raises_maximum_gold() {
    let mut g = game(12);
    set_board(&mut g, 0, &[PLUNDERER, "BG34_PreMadeChamp_067_G"]);
    let ids: Vec<u32> = g.players[0].board.iter().map(|m| m.eid).collect();
    {
        let mut c = combat_ctx(&mut g);
        c.destroy(ids[0]);
        c.resolve_deaths();
    }
    assert_eq!(g.players[0].max_gold_bonus, 2);
    {
        let mut c = combat_ctx(&mut g);
        c.destroy(ids[1]);
        c.resolve_deaths();
    }
    assert_eq!(g.players[0].max_gold_bonus, 6);
}

#[test]
fn centurion_copies_tavern_spells_three_times_per_turn() {
    let mut g = game(13);
    set_board(&mut g, 0, &[CENTURION]);
    give(&mut g, 0, STRIKE_OIL);
    for _ in 0..3 {
        play_last(&mut g, 0);
        assert_eq!(hand_count(&g, 0, STRIKE_OIL), 1);
    }
    play_last(&mut g, 0);
    assert_eq!(g.players[0].hand.len(), 0, "3 times per turn");
    assert_eq!(g.players[0].max_gold_bonus, 4);
    // The allowance comes back next turn.
    end_turn_all(&mut g);
    assert_eq!(g.players[0].board[0].counter, 0);
    give(&mut g, 0, STRIKE_OIL);
    play_last(&mut g, 0);
    assert_eq!(hand_count(&g, 0, STRIKE_OIL), 1);

    // Golden: 2 copies per trigger, still 3 triggers.
    let mut g = game(13);
    set_board(&mut g, 0, &["BG34_PreMadeChamp_200_G"]);
    give(&mut g, 0, STRIKE_OIL);
    for n in [2, 3, 4, 3] {
        play_last(&mut g, 0);
        assert_eq!(hand_count(&g, 0, STRIKE_OIL), n);
    }
}
