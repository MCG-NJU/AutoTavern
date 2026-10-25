//! Run one bot game with the event log enabled and print it.
//! Usage: cargo run --release -p taverngym --example play -- [seed]

use taverngym::RulesConfig;
use taverngym::bot::run_bot_game;
use taverngym::cards::db;

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let cfg = RulesConfig {
        record_log: true,
        ..RulesConfig::default()
    };
    let g = run_bot_game(seed, cfg);
    for line in &g.log {
        println!("{line}");
    }
    println!("--- final (turn {}) ---", g.turn);
    for p in &g.players {
        let hero = p.hero.map(|h| db().get(h).name.clone()).unwrap_or_default();
        let board: Vec<String> = p
            .last_board
            .iter()
            .map(|m| {
                format!(
                    "{}{} {}/{}",
                    if m.golden { "*" } else { "" },
                    m.name(),
                    m.atk,
                    m.hp
                )
            })
            .collect();
        println!(
            "#{} p{} {:<28} tier {} hp {:>3}  {}",
            p.placement,
            p.id,
            hero,
            p.tier,
            p.health,
            board.join(", ")
        );
    }
}
