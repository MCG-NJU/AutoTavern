//! Tests for the Timewarp cards of `content/tw_elementals.rs`.

mod common;
#[allow(unused_imports)]
use common::*;

use taverngym::state::ShopSlot;

const MOLTEN_ROCK: &str = "BG34_Giant_006";
const CYCLONE: &str = "BG34_Giant_012";
const ICHORON: &str = "BG34_Giant_040";
const LUBBER: &str = "BG34_Giant_066";
const TW_SELLEMENTAL: &str = "BG34_Giant_067";
const RAGNAROS: &str = "BG34_Giant_580";
const SNOW: &str = "BG34_Giant_586";
const STONE_DRAKE: &str = "BG34_Giant_675";
const TRUMPETER: &str = "BG34_Giant_676";
const BEHEMOTH: &str = "BG34_Giant_777";
const STORMCLOUD: &str = "BG34_PreMadeChamp_031";
const SUBSTRATE: &str = "BG34_PreMadeChamp_032";

/// Sellemental: a plain 3/3 Elemental (no Battlecry).
const SELLEMENTAL: &str = "BGS_115";
/// Skeleton: a plain 1/1 that is not an Elemental.
const SKELETON: &str = "BG_ICC_026t";
const TAVERN_TEMPEST: &str = "BGS_123";
const TEMPERATURE_SHIFT: &str = "BG31_819";

/// A game whose lobby has Elementals (first such seed), player 0 on Tavern Tier 6 with Gold.
fn elemental_game() -> Game {
    let mut g = (1..)
        .map(game)
        .find(|g| g.lobby_races.intersects(Races::ELEMENTAL))
        .unwrap();
    g.players[0].tier = 6;
    g.players[0].gold = 10;
    g
}

fn hand_count(g: &Game, p: PlayerId, id: &str) -> usize {
    g.players[p]
        .hand
        .iter()
        .filter(|h| h.item.card() == cid(id))
        .count()
}

/// Run the Start of Combat hooks of player 0's warband against player 1's; returns the
/// two boards as (attack, health) lists.
fn start_of_combat(g: &mut Game) -> [Vec<(i32, i32)>; 2] {
    let mut c = combat_ctx(g);
    c.run_board_hook(0, |s| s.start_of_combat, 1, "Start of Combat");
    [0, 1].map(|s| c.boards[s].iter().map(|m| (m.atk, m.hp)).collect())
}

#[test]
fn molten_rock_gains_and_improves() {
    for (id, want) in [
        (MOLTEN_ROCK, [(8, 8), (10, 10), (13, 13)]),
        ("BG34_Giant_006_G", [(16, 16), (20, 20), (26, 26)]),
    ] {
        let mut g = game(1);
        set_board(&mut g, 0, &[id]);
        for w in want {
            give(&mut g, 0, SELLEMENTAL);
            play_last(&mut g, 0);
            assert_eq!(stats(&g, 0)[0], w);
        }
        // A non-Elemental does nothing.
        give(&mut g, 0, SKELETON);
        play_last(&mut g, 0);
        assert_eq!(stats(&g, 0)[0], want[2]);
    }
}

#[test]
fn cyclone_is_keywords_only() {
    let mut g = game(1);
    set_board(&mut g, 0, &[CYCLONE]);
    let m = &g.players[0].board[0];
    assert!(m.has(Kw::DIVINE_SHIELD) && m.has(Kw::WINDFURY) && m.has(Kw::REBORN));
    assert!(taverngym::script::implemented(cid(CYCLONE)));
}

#[test]
fn ichoron_shields_played_minions() {
    let mut g = game(1);
    set_board(&mut g, 0, &[ICHORON]);
    give(&mut g, 0, SKELETON);
    play_last(&mut g, 0);
    assert!(g.players[0].board[1].has(Kw::DIVINE_SHIELD));
    // Permanent: still there next turn.
    end_turn_all(&mut g);
    assert!(g.players[0].board[1].has(Kw::DIVINE_SHIELD));
}

#[test]
fn lubber_offers_extra_tavern_spells() {
    let spells = |g: &Game| {
        g.players[0]
            .shop
            .iter()
            .filter(|s| s.item.as_minion().is_none())
            .count()
    };
    for (id, extra) in [(LUBBER, 1), ("BG34_Giant_066_G", 2)] {
        let mut g = game(1);
        g.players[0].gold = 10;
        g.apply(0, Action::Refresh).unwrap();
        let base = spells(&g);
        set_board(&mut g, 0, &[id]);
        g.apply(0, Action::Refresh).unwrap();
        assert_eq!(spells(&g), base + extra);
    }
}

#[test]
fn end_of_turn_cards() {
    for (id, got, n) in [
        (TW_SELLEMENTAL, SELLEMENTAL, 1),
        ("BG34_Giant_067_G", SELLEMENTAL, 2),
        (SUBSTRATE, TEMPERATURE_SHIFT, 1),
        ("BG34_PreMadeChamp_032_G", TEMPERATURE_SHIFT, 2),
    ] {
        let mut g = game(1);
        set_board(&mut g, 0, &[id]);
        end_turn_all(&mut g);
        assert_eq!(hand_count(&g, 0, got), n, "{id}");
    }
}

#[test]
fn ragnaros_hits_the_highest_health_enemy() {
    // 8 damage to the 9/11 Behemoth, not to the 3/3.
    let mut g = game(1);
    set_board(&mut g, 0, &[RAGNAROS]);
    set_board(&mut g, 1, &[SELLEMENTAL, BEHEMOTH]);
    assert_eq!(start_of_combat(&mut g)[1], vec![(3, 3), (9, 3)]);
    // Golden: double its 16 Attack kills a 32-Health minion.
    let mut g = game(1);
    set_board(&mut g, 0, &["BG34_Giant_580_G"]);
    set_board(&mut g, 1, &[SELLEMENTAL, BEHEMOTH]);
    g.players[1].board[1].hp = 32;
    assert_eq!(start_of_combat(&mut g)[1], vec![(3, 3)]);
}

#[test]
fn snow_elemental_offers_frozen_elementals() {
    for (id, n) in [(SNOW, 1), ("BG34_Giant_586_G", 2)] {
        let mut g = elemental_game();
        set_board(&mut g, 0, &[id]);
        g.apply(0, Action::Refresh).unwrap();
        let frozen: Vec<_> = g.players[0].shop.iter().filter(|s| s.frozen).collect();
        assert_eq!(frozen.len(), n);
        assert!(
            frozen
                .iter()
                .all(|s| s.item.as_minion().is_some_and(|m| m.is(Races::ELEMENTAL)))
        );
    }
}

#[test]
fn stone_drake_gains_sold_stats() {
    for (id, want) in [(STONE_DRAKE, (18, 20)), ("BG34_Giant_675_G", (36, 40))] {
        let mut g = game(1);
        // Two Drakes: each sale is counted once.
        set_board(&mut g, 0, &[id, id, SELLEMENTAL, BEHEMOTH]);
        g.apply(0, Action::Sell(2)).unwrap(); // 3/3
        g.apply(0, Action::Sell(2)).unwrap(); // 9/11
        let b = start_of_combat(&mut g);
        assert_eq!(b[0], vec![want, want]);
    }
    // Nothing sold: no gain. The tally is cleared at the start of the next turn.
    let mut g = game(1);
    set_board(&mut g, 0, &[STONE_DRAKE, SELLEMENTAL]);
    assert_eq!(start_of_combat(&mut g)[0][0], (6, 6));
    g.apply(0, Action::Sell(1)).unwrap();
    assert_eq!(start_of_combat(&mut g)[0][0], (9, 9));
    end_turn_all(&mut g);
    set_board(&mut g, 0, &[STONE_DRAKE]);
    assert_eq!(start_of_combat(&mut g)[0][0], (6, 6));
}

#[test]
fn stone_drake_counts_sales_from_hand() {
    let mut g = game(1);
    set_board(&mut g, 0, &[SELLEMENTAL]);
    give(&mut g, 0, STONE_DRAKE);
    g.apply(0, Action::Sell(0)).unwrap();
    // Play the Drake (the Sellemental's Water Droplet came after it in hand).
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert_eq!(start_of_combat(&mut g)[0][0], (9, 9));
}

#[test]
fn trumpeter_rewards_five_elementals_sold() {
    for (id, n) in [(TRUMPETER, 1), ("BG34_Giant_676_G", 2)] {
        let mut g = elemental_game();
        set_board(
            &mut g,
            0,
            &[id, SKELETON, CYCLONE, CYCLONE, CYCLONE, CYCLONE, CYCLONE],
        );
        // The Skeleton doesn't count.
        for _ in 0..5 {
            g.apply(0, Action::Sell(1)).unwrap();
        }
        assert!(g.players[0].hand.is_empty());
        assert_eq!(g.players[0].board[0].counter, 4);
        g.apply(0, Action::Sell(1)).unwrap();
        assert_eq!(g.players[0].hand.len(), n);
        assert!(
            g.players[0]
                .hand
                .iter()
                .all(|h| h.item.as_minion().is_some_and(|m| m.is(Races::ELEMENTAL)))
        );
        assert_eq!(g.players[0].board[0].counter, 0);
    }
}

#[test]
fn behemoth_gains_bought_elemental_stats() {
    for (id, want) in [(BEHEMOTH, (12, 14)), ("BG34_Giant_777_G", (24, 28))] {
        let mut g = game(1);
        g.players[0].gold = 10;
        set_board(&mut g, 0, &[id]);
        for card in [SKELETON, SELLEMENTAL] {
            let m = minion(&mut g, 0, card);
            g.players[0]
                .shop
                .insert(0, ShopSlot::new(m.eid, Item::Minion(m)));
        }
        // The 3/3 Elemental (slot 0), then the non-Elemental.
        g.apply(0, Action::Buy(0)).unwrap();
        assert_eq!(stats(&g, 0)[0], want);
        g.apply(0, Action::Buy(0)).unwrap();
        assert_eq!(stats(&g, 0)[0], want);
        assert_eq!(g.players[0].hand.len(), 2);
        assert!(g.players[0].board[0].has(Kw::TAUNT));
    }
}

#[test]
fn stormcloud_deathrattle_and_avenge() {
    for (id, n) in [(STORMCLOUD, 1), ("BG34_PreMadeChamp_031_G", 2)] {
        let mut g = game(1);
        set_board(&mut g, 0, &[id, SKELETON, SKELETON, SKELETON]);
        let ids: Vec<u32> = g.players[0].board.iter().map(|m| m.eid).collect();
        {
            // Two deaths: no Avenge yet.
            let mut c = combat_ctx(&mut g);
            for e in &ids[1..3] {
                c.destroy(*e);
            }
            c.resolve_deaths();
        }
        assert_eq!(hand_count(&g, 0, TAVERN_TEMPEST), 0);
        {
            let mut c = combat_ctx(&mut g);
            for e in &ids[1..] {
                c.destroy(*e);
            }
            c.resolve_deaths();
            // Avenge (3) fired; now the Deathrattle.
            c.destroy(ids[0]);
            c.resolve_deaths();
        }
        assert_eq!(hand_count(&g, 0, TAVERN_TEMPEST), 2 * n);
    }
}
