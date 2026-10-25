//! Fuzz run: random legal actions in every seat, state invariants checked after every action, pool
//! conservation whenever no choice is pending, snapshot round trips.
//! `cargo run --release --example fuzz -- [games per configuration] [first seed]`
use taverngym::actions::Action;
use taverngym::config::ContentMode;
use taverngym::rng::Rng;
use taverngym::{Game, RulesConfig, invariants};

struct Tally {
    games: u64,
    actions: u64,
    finished: u64,
    problems: Vec<String>,
}

fn run(seed: u64, cfg: &RulesConfig, t: &mut Tally) {
    let mut g = Game::new(seed, cfg.clone());
    let mut rng = Rng::new(seed ^ 0x5EED_F00D);
    let note = |g: &Game, what: String, t: &mut Tally| {
        if t.problems.len() < 40 {
            t.problems
                .push(format!("seed {seed} turn {}: {what}", g.turn));
        }
    };
    // Log length at the last moment the pool was balanced (the trace starts there).
    let mut balanced_at = 0;
    for step in 0..40_000u64 {
        if g.is_over() {
            t.finished += 1;
            break;
        }
        if step == 39_999 {
            note(&g, "not over after 40000 actions".into(), t);
        }
        let seats: Vec<usize> = (0..g.players.len())
            .filter(|p| g.needs_action(*p))
            .collect();
        if seats.is_empty() {
            note(&g, "nobody can act but the game is not over".into(), t);
            break;
        }
        let p = seats[rng.below(seats.len())];
        let legal = g.legal_actions(p);
        if legal.is_empty() {
            note(&g, format!("p{p} needs an action but none is legal"), t);
            break;
        }
        let end = Action::EndTurn.encode();
        let a = if legal.contains(&end) && rng.chance(1, 10) {
            end
        } else {
            legal[rng.below(legal.len())]
        };
        let pending = format!("{:?}", g.players[p].pending.last());
        if let Err(e) = g.apply_index(p, a) {
            note(
                &g,
                format!("p{p}: legal action {:?} rejected: {e}", Action::decode(a)),
                t,
            );
            break;
        }
        t.actions += 1;
        let bad = invariants::check(&g);
        if !bad.is_empty() {
            note(
                &g,
                format!("after p{p} {:?}: {}", Action::decode(a), bad.join("; ")),
                t,
            );
            break;
        }
        // (At the turn cap the ranked survivors keep their cards: no balance after the end.)
        if !g.is_over() && g.players.iter().all(|pl| pl.pending.is_empty()) {
            let (held, initial) = invariants::pool_balance(&g);
            if held != initial {
                let what = format!(
                    "pool {held} != {initial} after p{p} {:?}",
                    Action::decode(a)
                );
                note(&g, what, t);
                if cfg.record_log {
                    println!("pending before: {pending}");
                    for l in g.log.iter().skip(balanced_at) {
                        println!("   {l}");
                    }
                }
                break;
            }
            balanced_at = g.log.len();
            let species = invariants::pool_species_violations(&g);
            if !species.is_empty() {
                note(&g, species.join("; "), t);
                break;
            }
        }
        if step % 997 == 0 {
            let s = serde_json::to_string(&g).unwrap();
            let back: Game = serde_json::from_str(&s).unwrap();
            if serde_json::to_string(&back).unwrap() != s {
                note(&g, "snapshot round trip differs".into(), t);
                break;
            }
        }
    }
    t.games += 1;
}

fn main() {
    let mut args = std::env::args().skip(1);
    let n: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(200);
    let first: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    // A third argument turns the event log on (to trace one failing seed).
    let trace = args.next().is_some();
    let mut failed = false;
    for mode in [ContentMode::Implemented, ContentMode::AllVanilla] {
        // (forced Aberrations, Timewarped Tavern system on)
        for (forced, timewarp) in [(false, false), (true, false), (false, true)] {
            let cfg = RulesConfig {
                content: mode,
                aberration_forced: forced,
                timewarp_enabled: timewarp,
                record_log: trace,
                ..RulesConfig::default()
            };
            let mut t = Tally {
                games: 0,
                actions: 0,
                finished: 0,
                problems: vec![],
            };
            let start = std::time::Instant::now();
            let mut panics = 0;
            for seed in first..first + n {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(seed, &cfg, &mut t)
                }));
                if r.is_err() {
                    panics += 1;
                    eprintln!("{mode:?} forced={forced} seed {seed} panicked");
                }
            }
            println!(
                "{mode:?} aberration_forced={forced} timewarp={timewarp}: {} games, {} actions, {} finished, \
                 {panics} panics, {} problems, {:.1}s",
                t.games,
                t.actions,
                t.finished,
                t.problems.len(),
                start.elapsed().as_secs_f64()
            );
            for p in &t.problems {
                println!("  {p}");
            }
            failed |= panics > 0 || !t.problems.is_empty();
        }
    }
    if failed {
        std::process::exit(1);
    }
}
