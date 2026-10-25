//! Tests for the Timewarp cards of `content/tw_murlocs_mechs.rs`.

mod common;
#[allow(unused_imports)]
use common::*;

const ANNOY: &str = "BG34_Giant_007"; // 6/6 Mech
const SCOURFIN: &str = "BG34_Giant_017"; // 7/7 Murloc
const BASSGILL: &str = "BG34_Giant_071"; // 7/4 Murloc
const INTERPRETER: &str = "BG34_Giant_120"; // 6/8 Mech
const MURKY: &str = "BG34_Giant_206"; // 5/5 Murloc
const MURCULES: &str = "BG34_Giant_207"; // 10/5 Murloc
const SENSEI: &str = "BG34_Giant_209"; // 6/6 Mech
const COPTER: &str = "BG34_Giant_302"; // 4/6 Mech
const PAINTER: &str = "BG34_Giant_319"; // 4/9 Murloc
const MYSTIC: &str = "BG34_Giant_320"; // 6/6 Murloc
const MRRRGLR: &str = "BG34_Giant_321"; // 8/8 Murloc
const ACOLYTE: &str = "BG34_Giant_591"; // 4/6 Murloc
const IMMORTAL: &str = "BG34_Giant_597"; // 8/8 Mech
const MOTHERSHIP: &str = "BG34_Giant_598"; // 5/7 Mech
const WARGEAR: &str = "BG34_Giant_677"; // 5/5 Magnetic Mech
const SQUALLFIN: &str = "BG34_PreMadeChamp_049"; // 6/14 Murloc

/// (attack, health) of the minions in player `p`'s hand, left to right.
fn hand_stats(g: &Game, p: PlayerId) -> Vec<(i32, i32)> {
    g.players[p]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion().map(|m| (m.atk, m.hp)))
        .collect()
}

/// Kill player 0's minion at `idx` in a hand-driven combat and return the resulting side-0
/// board as (attack, health, has Divine Shield).
fn kill_in_combat(g: &mut Game, idx: usize) -> Vec<(i32, i32, bool)> {
    let mut c = combat_ctx(g);
    let eid = c.boards[0][idx].eid;
    c.destroy(eid);
    c.resolve_deaths();
    c.boards[0]
        .iter()
        .map(|m| (m.atk, m.hp, m.kw.has(Kw::DIVINE_SHIELD)))
        .collect()
}

#[test]
fn scourfin_buffs_and_summons_a_hand_minion() {
    for (id, k) in [(SCOURFIN, 1), ("BG34_Giant_017_G", 2)] {
        let mut g = game(700);
        set_board(&mut g, 0, &[id]);
        give(&mut g, 0, SENSEI);
        let board = kill_in_combat(&mut g, 0);
        // The hand minion keeps the stats; a copy fights for this combat only.
        assert_eq!(board, vec![(6 + 7 * k, 6 + 7 * k, false)]);
        assert_eq!(hand_stats(&g, 0), vec![(6 + 7 * k, 6 + 7 * k)]);
        assert_eq!(g.players[0].board.len(), 1);
    }
}

#[test]
fn bassgill_summons_the_highest_health_hand_minions_with_divine_shield() {
    for (id, n) in [(BASSGILL, 1), ("BG34_Giant_071_G", 2)] {
        let mut g = game(701);
        set_board(&mut g, 0, &[id]);
        give(&mut g, 0, MURKY); // 5/5
        give(&mut g, 0, MRRRGLR); // 8/8
        give(&mut g, 0, SCOURFIN); // 7/7
        let board = kill_in_combat(&mut g, 0);
        let expect = [(8, 8, true), (7, 7, true)];
        assert_eq!(board, expect[..n].to_vec());
        // The hand is untouched (no Divine Shield on the cards themselves).
        assert_eq!(hand_stats(&g, 0), vec![(5, 5), (8, 8), (7, 7)]);
        assert!(g.players[0].hand.iter().all(|h| {
            h.item
                .as_minion()
                .is_some_and(|m| !m.kw.has(Kw::DIVINE_SHIELD))
        }));
    }
}

#[test]
fn interpreter_buffs_mechs_on_play_and_once_on_magnetize() {
    let mut g = game(702);
    set_board(&mut g, 0, &[INTERPRETER, ANNOY, SCOURFIN]);
    give(&mut g, 0, SENSEI);
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0), vec![(9, 11), (9, 9), (7, 7), (9, 9)]);
    // A Murloc is not a Mech.
    give(&mut g, 0, MURKY);
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0)[..2], [(9, 11), (9, 9)]);
    // Magnetizing Wargear (6/5) on the Annoy-o-Tron: one trigger, not two.
    give(&mut g, 0, "BG_BOT_563");
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Magnetize { hand, board: 1 }).unwrap();
    assert_eq!(
        stats(&g, 0),
        vec![(12, 14), (9 + 6 + 3, 9 + 5 + 3), (7, 7), (12, 12), (5, 5)]
    );
    // Golden: +6/+6.
    let mut g = game(702);
    set_board(&mut g, 0, &["BG34_Giant_120_G", ANNOY]);
    give(&mut g, 0, SENSEI);
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0), vec![(18, 22), (12, 12), (12, 12)]);
}

#[test]
fn murky_grows_with_battlecries_triggered() {
    for (id, base, gain) in [(MURKY, 5, 2), ("BG34_Giant_206_G", 10, 4)] {
        let mut g = game(703);
        set_board(&mut g, 0, &[id]);
        g.players[0].stats.battlecries_triggered = 2;
        g.apply(0, Action::EndTurn).unwrap();
        assert_eq!(stats(&g, 0), vec![(base + 3 * gain, base + 3 * gain)]);
    }
}

#[test]
fn murcules_buffs_the_leftmost_hand_minion_on_a_kill() {
    for (id, k) in [(MURCULES, 1), ("BG34_Giant_207_G", 2)] {
        let mut g = game(704);
        set_board(&mut g, 0, &[id]);
        set_board(&mut g, 1, &[MURKY]);
        give(&mut g, 0, SENSEI);
        give(&mut g, 0, MRRRGLR);
        {
            let mut c = combat_ctx(&mut g);
            let eid = c.boards[0][0].eid;
            taverngym::combat::attack_now(&mut c, 0, eid);
            assert!(c.boards[1].is_empty());
        }
        assert_eq!(hand_stats(&g, 0), vec![(6 + 4 * k, 6 + 4 * k), (8, 8)]);
    }
}

#[test]
fn sensei_buffs_adjacent_mechs_at_end_of_turn() {
    for (id, k) in [(SENSEI, 1), ("BG34_Giant_209_G", 2)] {
        let mut g = game(705);
        set_board(&mut g, 0, &[ANNOY, id, SCOURFIN, ANNOY]);
        g.apply(0, Action::EndTurn).unwrap();
        let s = stats(&g, 0);
        assert_eq!(s[0], (6 + 3 * k, 6 + 3 * k));
        // Not a Mech; not adjacent.
        assert_eq!(s[2..], [(7, 7), (6, 6)]);
    }
}

/// A game whose lobby has a Tier 1 Mech (the lobby's minion types depend on the seed).
fn mech_game() -> Game {
    (710..)
        .map(game)
        .find(|g| {
            g.universe.iter().any(|c| {
                let d = db().get(*c);
                d.tier == 1 && d.races.intersects(Races::MECH)
            })
        })
        .unwrap()
}

#[test]
fn copter_avenge_gets_random_mechs() {
    for (id, n) in [(COPTER, 1), ("BG34_Giant_302_G", 2)] {
        let mut g = mech_game();
        set_board(&mut g, 0, &[id, MURKY, MURKY, MURKY]);
        {
            let mut c = combat_ctx(&mut g);
            for _ in 0..2 {
                let eid = c.boards[0][1].eid;
                c.destroy(eid);
                c.resolve_deaths();
            }
        }
        assert!(g.players[0].hand.is_empty());
        {
            let mut c = combat_ctx(&mut g);
            for _ in 0..3 {
                let eid = c.boards[0][1].eid;
                c.destroy(eid);
                c.resolve_deaths();
            }
        }
        let hand = &g.players[0].hand;
        assert_eq!(hand.len(), n);
        let tier = g.player_tier(0);
        for h in hand {
            let m = h.item.as_minion().unwrap();
            assert!(m.races.intersects(Races::MECH) && m.tier <= tier);
        }
    }
}

#[test]
fn mothership_avenge_gets_protoss_minions() {
    for (id, n) in [(MOTHERSHIP, 1), ("BG34_Giant_598_G", 2)] {
        let mut g = game(706);
        set_board(&mut g, 0, &[id, MURKY, MURKY, MURKY, MURKY]);
        {
            let mut c = combat_ctx(&mut g);
            for i in 0..4 {
                assert!(c.g.players[0].hand.is_empty(), "after {i} deaths");
                let eid = c.boards[0][1].eid;
                c.destroy(eid);
                c.resolve_deaths();
            }
        }
        let hand = &g.players[0].hand;
        assert_eq!(hand.len(), n);
        for h in hand {
            let id = &db().get(h.item.card()).id;
            assert!(id.starts_with("BG31_HERO_802pt"), "{id}");
        }
    }
}

#[test]
fn painter_improves_on_low_tier_cards() {
    for (id, k) in [(PAINTER, 1), ("BG34_Giant_319_G", 2)] {
        let mut g = game(707);
        set_board(&mut g, 0, &[ANNOY, id, SCOURFIN]);
        // Tier 3: improves. Tier 5: doesn't.
        give(&mut g, 0, ANNOY);
        play_last(&mut g, 0);
        assert_eq!(g.players[0].board[1].counter, k, "eligible low-Tier card");
        give(&mut g, 0, IMMORTAL);
        play_last(&mut g, 0);
        assert_eq!(
            g.players[0].board[1].counter, k,
            "high-Tier card adds no growth"
        );
        g.apply(0, Action::EndTurn).unwrap();
        let s = stats(&g, 0);
        assert_eq!(s[0], (6 + 6 * k, 6 + 4 * k));
        assert_eq!(s[2], (7 + 6 * k, 7 + 4 * k));
        assert_eq!(s[3], (6, 6));
    }
}

#[test]
fn mystic_counts_sold_murlocs() {
    for (id, k) in [(MYSTIC, 1), ("BG34_Giant_320_G", 2)] {
        let mut g = game(708);
        set_board(&mut g, 0, &[id, MURKY, MURKY, MURKY, ANNOY]);
        // A Mech doesn't count.
        g.apply(0, Action::Sell(4)).unwrap();
        g.apply(0, Action::Sell(1)).unwrap();
        g.apply(0, Action::Sell(1)).unwrap();
        let st = &g.players[0].stats;
        assert_eq!((st.spell_atk, st.spell_hp), (0, 0));
        g.apply(0, Action::Sell(1)).unwrap();
        let st = &g.players[0].stats;
        assert_eq!((st.spell_atk, st.spell_hp), (2 * k, k));
        assert_eq!(g.players[0].board[0].counter, 0);
    }
}

#[test]
fn mrrrglr_gives_hand_stats_to_adjacent_murlocs() {
    for (id, k) in [(MRRRGLR, 1), ("BG34_Giant_321_G", 2)] {
        let mut g = game(709);
        set_board(&mut g, 0, &[SCOURFIN, id, ANNOY]);
        give(&mut g, 0, SENSEI); // 6/6
        give(&mut g, 0, COPTER); // 4/6
        let mut c = combat_ctx(&mut g);
        c.run_board_hook(0, |s| s.start_of_combat, 1, "Start of Combat");
        let s: Vec<(i32, i32)> = c.boards[0].iter().map(|m| (m.atk, m.hp)).collect();
        assert_eq!(s[0], (7 + 10 * k, 7 + 12 * k));
        assert_eq!(s[1], (8 * k, 8 * k));
        assert_eq!(s[2], (6, 6));
    }
}

#[test]
fn immortal_gains_adjacent_stats() {
    for (id, k) in [(IMMORTAL, 1), ("BG34_Giant_597_G", 2)] {
        let mut g = game(710);
        set_board(&mut g, 0, &[SCOURFIN, id, ANNOY]);
        let mut c = combat_ctx(&mut g);
        c.run_board_hook(0, |s| s.start_of_combat, 1, "Start of Combat");
        let m = &c.boards[0][1];
        assert_eq!((m.atk, m.hp), (8 * k + 13 * k, 8 * k + 13 * k));
    }
}

#[test]
fn acolyte_spins_the_wheel_at_turn_start() {
    for id in [ACOLYTE, "BG34_Giant_591_G"] {
        // Control: without the Acolyte the start-of-turn hooks draw no random number.
        for (ids, spins) in [(vec![MURKY], false), (vec![MURKY, id], true)] {
            let mut g = game(711);
            set_board(&mut g, 0, &ids);
            let mut before = g.rng.clone();
            g.with_recruit(0, |c| {
                c.run_board_hook(0, |s| s.start_of_turn, 1, "start of turn")
            });
            assert_eq!(g.rng.next_u64() != before.next_u64(), spins);
        }
    }
}

#[test]
fn wargear_multiplies_the_host_stats() {
    for (id, own, mult) in [(WARGEAR, 5, 2), ("BG34_Giant_677_G", 10, 3)] {
        let mut g = game(712);
        set_board(&mut g, 0, &[ANNOY]);
        give(&mut g, 0, id);
        g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
        assert_eq!(stats(&g, 0), vec![((6 + own) * mult, (6 + own) * mult)]);
        // Played as a minion: nothing is Magnetized.
        give(&mut g, 0, id);
        play_last(&mut g, 0);
        assert_eq!(stats(&g, 0)[1], (own, own));
    }
}

#[test]
fn squallfin_buffs_hand_and_board_after_a_murloc() {
    for (id, base, k) in [
        (SQUALLFIN, (6, 14), 1),
        ("BG34_PreMadeChamp_049_G", (12, 28), 2),
    ] {
        let mut g = game(713);
        set_board(&mut g, 0, &[id, ANNOY]);
        give(&mut g, 0, SENSEI);
        give(&mut g, 0, SCOURFIN);
        play_last(&mut g, 0);
        let d = 2 * k;
        assert_eq!(
            stats(&g, 0),
            vec![(base.0 + d, base.1 + d), (6 + d, 6 + d), (7 + d, 7 + d)]
        );
        assert_eq!(hand_stats(&g, 0), vec![(6 + d, 6 + d)]);
        // A Mech doesn't trigger it.
        play_last(&mut g, 0);
        assert_eq!(stats(&g, 0)[1], (6 + d, 6 + d));
    }
}
