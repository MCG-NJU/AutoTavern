//! Single-agent environment: one learning seat, the other seats played by `bot`.
//!
//! Protocol: `reset(seed)` -> agent decision point; `step(action)` applies the agent's
//! action, lets bots act whenever the agent is waiting, and returns at the agent's next
//! decision point (or when the agent is eliminated / the game ends).

use crate::bot;
use crate::config::RulesConfig;
use crate::engine::{Game, StoppedReason};
use crate::obs::{Observation, observe};
use crate::types::PlayerId;

pub struct StepResult {
    pub reward: f32,
    pub done: bool,
    /// Error message if the action was illegal (state unchanged, small penalty applied).
    pub illegal: Option<&'static str>,
}

pub struct SoloEnv {
    pub game: Game,
    pub agent: PlayerId,
    pub cfg: RulesConfig,
    /// Reward for an illegal action (the action is ignored).
    pub illegal_penalty: f32,
}

/// Placement-based terminal reward in [-1, 1]: 1st = 1, 8th = -1, linear.
pub fn placement_reward(place: u8, n: usize) -> f32 {
    if place == 0 || n < 2 {
        return 0.0;
    }
    1.0 - 2.0 * (place as f32 - 1.0) / (n as f32 - 1.0)
}

impl SoloEnv {
    pub fn new(seed: u64, cfg: RulesConfig, agent: PlayerId) -> SoloEnv {
        let mut e = SoloEnv {
            game: Game::new(seed, cfg.clone()),
            agent,
            cfg,
            illegal_penalty: -0.01,
        };
        e.advance();
        e
    }

    pub fn reset(&mut self, seed: u64) {
        self.game = Game::new(seed, self.cfg.clone());
        self.advance();
    }

    pub fn done(&self) -> bool {
        self.game.is_over()
            || self.game.stopped_reason.is_some()
            || !self.game.players[self.agent].alive
    }

    pub fn observe(&self) -> Observation {
        observe(&self.game, self.agent)
    }

    pub fn legal_mask(&self) -> Vec<bool> {
        self.game.legal_mask(self.agent)
    }

    pub fn placement(&self) -> u8 {
        self.game.players[self.agent].placement
    }

    /// Let bots act until the agent must decide (or the episode ends).
    fn advance(&mut self) {
        let mut guard = 0;
        while !self.done() && !self.game.needs_action(self.agent) && guard < 10_000 {
            guard += 1;
            let mut progressed = false;
            for p in 0..self.game.players.len() {
                if p != self.agent && self.game.needs_action(p) {
                    progressed |= bot::play_turn(&mut self.game, p);
                }
            }
            if !progressed {
                if !self.done() {
                    self.game.stopped_reason = Some(StoppedReason::NoProgress);
                }
                return;
            }
        }
        if guard == 10_000 && !self.done() && !self.game.needs_action(self.agent) {
            self.game.stopped_reason = Some(StoppedReason::DriverGuard);
            return;
        }
        // Bots also act during the agent's turn so that their turns end before combat.
        for p in 0..self.game.players.len() {
            if p != self.agent
                && self.game.needs_action(p)
                && !self.done()
                && !bot::play_turn(&mut self.game, p)
                && self.game.needs_action(p)
                && !self.game.is_over()
            {
                self.game.stopped_reason = Some(StoppedReason::NoProgress);
                return;
            }
        }
    }

    pub fn step(&mut self, action: usize) -> StepResult {
        if self.game.stopped_reason.is_some() {
            return StepResult {
                reward: 0.0,
                done: true,
                illegal: Some("simulator stopped"),
            };
        }
        if self.done() {
            return StepResult {
                reward: 0.0,
                done: true,
                illegal: Some("episode is over"),
            };
        }
        let legal = self.game.legal_mask(self.agent);
        // No legal action although the episode is not over: a seat (the agent's or a bot's)
        // needs an action and none is accepted. The episode ends here instead of handing
        // out an all-false mask for ever.
        if !legal.iter().any(|x| *x) {
            self.game.stopped_reason = Some(StoppedReason::NoProgress);
            return StepResult {
                reward: 0.0,
                done: true,
                illegal: Some("stalled: no legal action"),
            };
        }
        if action >= legal.len() || !legal[action] {
            return StepResult {
                reward: self.illegal_penalty,
                done: false,
                illegal: Some("illegal action"),
            };
        }
        if let Err(e) = self.game.apply_index(self.agent, action) {
            return StepResult {
                reward: self.illegal_penalty,
                done: false,
                illegal: Some(e),
            };
        }
        self.advance();
        if self.game.stopped_reason.is_some() {
            return StepResult {
                reward: 0.0,
                done: true,
                illegal: Some("simulator stopped"),
            };
        }
        let done = self.done();
        let reward = if done {
            placement_reward(self.placement(), self.game.players.len())
        } else {
            0.0
        };
        StepResult {
            reward,
            done,
            illegal: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_reward_range() {
        assert_eq!(placement_reward(1, 8), 1.0);
        assert_eq!(placement_reward(8, 8), -1.0);
    }
}
