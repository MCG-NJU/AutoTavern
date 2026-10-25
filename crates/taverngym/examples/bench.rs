//! Throughput benchmark: full bot games and isolated combats.
//! Usage: cargo run --release -p taverngym --example bench -- [games]

use std::time::Instant;

use taverngym::RulesConfig;
use taverngym::bot::run_bot_game;

fn main() {
    let n: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    let cfg = RulesConfig::default();

    let t = Instant::now();
    let mut turns = 0u64;
    let mut combats = 0u64;
    let mut snapshot = None;
    for seed in 0..n {
        let g = run_bot_game(seed, cfg.clone());
        turns += g.turn as u64;
        combats += g
            .players
            .iter()
            .map(|p| p.stats.combats_won + p.stats.combats_lost)
            .sum::<i32>() as u64;
        if seed == 0 {
            snapshot = Some(g);
        }
    }
    let dt = t.elapsed().as_secs_f64();
    println!(
        "full games: {n} in {dt:.2}s -> {:.0} games/s, avg {:.1} turns, {:.0} decided combats/s (single thread, incl. bots)",
        n as f64 / dt,
        turns as f64 / n as f64,
        combats as f64 / dt
    );

    // Isolated combat throughput on a late-game snapshot: re-simulate turn-final boards.
    let mut g = snapshot.unwrap();
    for p in g.players.iter_mut() {
        p.alive = true;
        p.board = p.last_board.clone();
    }
    let reps = 20_000u64;
    let t = Instant::now();
    let mut decided = 0;
    for i in 0..reps {
        let a = (i % 8) as usize;
        let b = ((i + 1 + i / 8 % 7) % 8) as usize;
        if a == b {
            continue;
        }
        let out = g.simulate_combat(a, Some(b), None);
        decided += (out.winner != 2) as u64;
    }
    let dt = t.elapsed().as_secs_f64();
    println!(
        "combats: {reps} in {dt:.2}s -> {:.0} combats/s ({decided} decided)",
        reps as f64 / dt
    );
}
