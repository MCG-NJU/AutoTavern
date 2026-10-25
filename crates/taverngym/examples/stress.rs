//! Stress run: many bot games in both content modes; reports panics, unfinished games and
//! extreme scales. `cargo run --release --example stress -- [games]`
use taverngym::bot::run_bot_game;
use taverngym::config::ContentMode;
use taverngym::{Game, RulesConfig};

fn main() {
    let n: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);
    for mode in [ContentMode::Implemented, ContentMode::AllVanilla] {
        let (mut unfinished, mut panics, mut max_stat, mut max_turn) = (0, 0, 0i64, 0);
        let t = std::time::Instant::now();
        for seed in 0..n {
            let cfg = RulesConfig {
                content: mode,
                ..RulesConfig::default()
            };
            let r = std::panic::catch_unwind(|| run_bot_game(seed, cfg));
            match r {
                Ok(g) => {
                    let g: Game = g;
                    if !g.is_over() {
                        unfinished += 1;
                        eprintln!("{mode:?} seed {seed} unfinished at turn {}", g.turn);
                    }
                    max_turn = max_turn.max(g.turn);
                    for p in &g.players {
                        for m in p.board.iter().chain(p.last_board.iter()) {
                            max_stat = max_stat.max(m.atk as i64).max(m.hp as i64);
                        }
                    }
                }
                Err(_) => {
                    panics += 1;
                    eprintln!("{mode:?} seed {seed} panicked");
                }
            }
        }
        println!(
            "{mode:?}: {n} games in {:.1}s, panics {panics}, unfinished {unfinished}, max turn {max_turn}, max stat {max_stat}",
            t.elapsed().as_secs_f64()
        );
    }
}
