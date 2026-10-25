#![recursion_limit = "256"]
//! TavernGym: a Hearthstone Battlegrounds solo-mode simulator engine (client 36.6.3.253932,
//! server rules 36.6.3) with a reinforcement-learning interface.
//!
//! Module map:
//! - `cards`   static card database (`data/cards.json`)
//! - `config`  rules configuration and simulator options
//! - `state`   minions, hand/shop items, players, pool, pending choices
//! - `engine`  `Game` container and the effect context `Ctx` (summon/buff/damage/death...)
//! - `recruit` tavern and player actions; `combat` automatic combat; `flow` turn lifecycle
//! - `script`  card behaviour hooks; `content` the implemented cards
//! - `actions` flat action space and legality masks; `obs` observation encoding
//! - `env`     single-agent environment (one seat against scripted bots, `bot`)
//! - `batch`   batched environment: many games, any set of controlled seats
//! - `debug`   JSON views of a game, the card catalog and debug commands
//! - `invariants` state checks for fuzzing (`examples/fuzz.rs`)

pub mod actions;
pub mod batch;
pub mod bot;
pub mod cards;
pub mod combat;
pub mod config;
pub mod content;
pub mod debug;
pub mod engine;
pub mod env;
pub mod event;
pub mod experimental;
pub mod flow;
pub mod guard;
pub mod invariants;
pub mod obs;
pub mod recruit;
pub mod rng;
pub mod script;
pub mod state;
pub mod types;
pub mod workers;

pub use actions::{Action, N_ACTIONS};
pub use batch::{BatchEnv, SlotResult};
pub use config::RulesConfig;
pub use engine::Game;
pub use env::SoloEnv;

/// Simulator library version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Hearthstone client build the simulator replicates.
pub const GAME_VERSION: &str = "36.6.3.253932";
/// Server-side rule overlay applied on top of the client data.
pub const SERVER_RULES_VERSION: &str = "36.6.3";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(GAME_VERSION, "36.6.3.253932");
        assert_eq!(SERVER_RULES_VERSION, "36.6.3");
        assert!(!VERSION.is_empty());
    }
}
