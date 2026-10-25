//! Whole-game smoke tests with bots in every seat.

use taverngym::bot::run_bot_game;
use taverngym::engine::Phase;
use taverngym::state::Item;
use taverngym::{Game, RulesConfig};

fn check_invariants(g: &Game) {
    let mut eids = std::collections::HashSet::new();
    for p in &g.players {
        assert!(p.board.len() <= g.cfg.board_size, "board overflow");
        // Trinket / Quest Reward cards can exceed the hand limit.
        assert!(p.hand.len() <= g.cfg.hand_size + 30, "hand overflow");
        assert!(p.tier >= 1 && p.tier <= 7);
        for m in &p.board {
            assert!(eids.insert(m.eid), "duplicate eid {}", m.eid);
            assert!(m.hp > 0, "dead minion on board: {:?}", m);
        }
        for h in &p.hand {
            if let Item::Minion(m) = &h.item {
                assert!(eids.insert(m.eid), "duplicate eid {}", m.eid);
            }
        }
    }
}

#[test]
fn bot_games_complete() {
    let mut turns = vec![];
    for seed in 0..50u64 {
        let g = run_bot_game(seed, RulesConfig::default());
        assert_eq!(
            g.phase,
            Phase::GameOver,
            "seed {seed} did not finish (turn {})",
            g.turn
        );
        check_invariants(&g);
        let mut places: Vec<u8> = g.players.iter().map(|p| p.placement).collect();
        places.sort();
        assert_eq!(
            places,
            (1..=8).collect::<Vec<u8>>(),
            "seed {seed} placements {places:?}"
        );
        turns.push(g.turn);
    }
    let avg = turns.iter().sum::<u32>() as f64 / turns.len() as f64;
    assert!(avg > 8.0 && avg < 40.0, "average game length {avg}");
}

#[test]
fn deterministic_given_seed() {
    let a = run_bot_game(7, RulesConfig::default());
    let b = run_bot_game(7, RulesConfig::default());
    let pa: Vec<u8> = a.players.iter().map(|p| p.placement).collect();
    let pb: Vec<u8> = b.players.iter().map(|p| p.placement).collect();
    assert_eq!(pa, pb);
    assert_eq!(a.turn, b.turn);
    assert_eq!(
        serde_json::to_string(&a.players).unwrap(),
        serde_json::to_string(&b.players).unwrap()
    );
}

#[test]
fn all_vanilla_content_mode_runs() {
    let cfg = RulesConfig {
        content: taverngym::config::ContentMode::AllVanilla,
        ..RulesConfig::default()
    };
    for seed in 0..10u64 {
        let g = run_bot_game(seed, cfg.clone());
        assert_eq!(g.phase, Phase::GameOver);
        check_invariants(&g);
    }
}

#[test]
fn snapshot_roundtrip() {
    let g = run_bot_game(3, RulesConfig::default());
    let s = serde_json::to_string(&g).unwrap();
    let back: Game = serde_json::from_str(&s).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), s);
}

/// Random legal actions in every seat (the full run is `examples/fuzz.rs`): the state invariants
/// hold after every action and the pool balances whenever no choice is open.
#[test]
fn random_play_keeps_invariants() {
    use taverngym::actions::Action;
    use taverngym::invariants;
    use taverngym::rng::Rng;
    for seed in 0..24u64 {
        let cfg = RulesConfig {
            aberration_forced: seed % 2 == 0,
            ..RulesConfig::default()
        };
        let mut g = Game::new(seed, cfg);
        let mut rng = Rng::new(seed ^ 0x5EED_F00D);
        for _ in 0..40_000 {
            if g.is_over() {
                break;
            }
            let seats: Vec<usize> = (0..8).filter(|p| g.needs_action(*p)).collect();
            let p = seats[rng.below(seats.len())];
            let legal = g.legal_actions(p);
            let end = Action::EndTurn.encode();
            let a = if legal.contains(&end) && rng.chance(1, 10) {
                end
            } else {
                legal[rng.below(legal.len())]
            };
            g.apply_index(p, a).unwrap();
            let bad = invariants::check(&g);
            assert!(bad.is_empty(), "seed {seed} turn {}: {bad:?}", g.turn);
            if !g.is_over() && g.players.iter().all(|pl| pl.pending.is_empty()) {
                let (held, initial) = invariants::pool_balance(&g);
                assert_eq!(held, initial, "seed {seed} turn {}: pool", g.turn);
            }
        }
        assert!(g.is_over(), "seed {seed}");
    }
}

/// The turn cap ends a game nobody can win: survivors are ranked by Health.
#[test]
fn turn_cap_ranks_survivors_by_health() {
    use taverngym::actions::Action;
    let cfg = RulesConfig {
        max_turns: 3,
        ..RulesConfig::default()
    };
    let mut g = Game::new(5, cfg);
    for p in 0..8 {
        g.apply(p, Action::Choose(0)).unwrap();
    }
    // Nobody buys anything: every combat is a tie between empty warbands.
    for _ in 0..3 {
        for p in 0..8 {
            g.players[p].pending.clear();
            g.players[p].board.clear();
            if g.needs_action(p) {
                g.apply(p, Action::EndTurn).unwrap();
            }
        }
    }
    assert!(g.is_over());
    assert_eq!(g.turn, 3);
    let mut places: Vec<u8> = g.players.iter().map(|p| p.placement).collect();
    places.sort();
    assert_eq!(places, (1..=8).collect::<Vec<u8>>());
    let best = g.players.iter().max_by_key(|p| p.health + p.armor).unwrap();
    let first = g.players.iter().find(|p| p.placement == 1).unwrap();
    assert_eq!(first.health + first.armor, best.health + best.armor);
}
