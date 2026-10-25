//! Batched environment for reinforcement-learning frameworks: many independent games
//! stepped together.
//!
//! Every game is a full eight-player lobby. The caller controls a fixed set of seats
//! (`controlled`: e.g. `[0]` for one learner against seven scripted bots, or all eight for
//! self-play); the other seats are played by [`crate::bot`]. A *slot* is one controlled
//! seat of one game: slot `i` is seat `controlled[i % k]` of game `i / k`, with
//! `k = controlled.len()`.
//!
//! One round of the protocol:
//! 1. [`BatchEnv::observe`] writes, for every slot, the player's observation
//!    ([`crate::obs::observe`]), its legal-action mask and whether it must act now
//!    (`ready`). Several seats of one game can be ready at once: the recruit phase is
//!    simultaneous.
//! 2. [`BatchEnv::step`] takes one action per slot (ignored for slots that are not ready),
//!    applies them seat by seat, lets the bots act, and runs every game on until one of its
//!    controlled seats must act again. Only the slots that were ready when the step began
//!    act; a seat that comes to need an action meanwhile acts after the next `observe`. It reports a [`SlotResult`] per slot: the reward
//!    (the placement reward when the seat's episode ends), whether it ended, the final
//!    placement, and whether the action was rejected.
//! 3. A game whose controlled seats have all finished starts again with a new seed (its
//!    episode index goes up). Seeds follow from the environment's seed, the game's index
//!    and the episode index, so a run is reproducible.
//!
//! Games run on `threads` worker threads; results do not depend on the number of threads.
//! [`BatchEnv::save`] and [`BatchEnv::load`] copy a game and its episode state exactly
//! (JSON, random generator included).

use serde::{Deserialize, Serialize};

use crate::actions::N_ACTIONS;
use crate::bot;
use crate::config::RulesConfig;
use crate::engine::{Game, StoppedReason};
use crate::env::placement_reward;
use crate::obs::{OBS_CARDS, OBS_FLOATS, observe};
use crate::types::PlayerId;

/// One game of a [`BatchEnv`] and where its episode stands.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Episode {
    pub game: Game,
    /// The number of earlier episodes of this game index.
    pub index: u64,
    /// Per controlled seat: its episode has ended (eliminated, or the game is over).
    pub finished: Vec<bool>,
}

/// What [`BatchEnv::step`] reports for one slot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SlotResult {
    /// The placement reward when the seat's episode ends ([`placement_reward`]: 1st = 1,
    /// last = -1), plus [`BatchEnv::illegal_penalty`] when the action was rejected.
    pub reward: f32,
    /// The seat's episode ended in this step.
    pub done: bool,
    /// The final placement (1 = first) when `done`; 0 when a simulator guard stopped the
    /// game before it ended.
    pub placement: u8,
    /// The slot was ready and its action was illegal or out of range: it was ignored.
    pub rejected: bool,
}

/// Many games stepped together; see the module documentation.
pub struct BatchEnv {
    cfg: RulesConfig,
    seed: u64,
    controlled: Vec<PlayerId>,
    episodes: Vec<Episode>,
    threads: usize,
    /// Reward for a rejected action (the action is ignored).
    pub illegal_penalty: f32,
}

/// One slot's observation outputs: features, card indices, legal mask, ready flag.
type ObsRow<'a> = (&'a mut [f32], &'a mut [i32], &'a mut [u8], &'a mut u8);

fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn over(g: &Game) -> bool {
    g.is_over() || g.stopped_reason.is_some()
}

/// Let the bot seats act until a controlled seat must act, or the game is over.
fn advance(g: &mut Game, controlled: &[PlayerId]) {
    for _ in 0..10_000 {
        if over(g) {
            return;
        }
        // Bots act whenever they need to, so that their turns end before combat.
        let mut progressed = false;
        for p in 0..g.players.len() {
            if !controlled.contains(&p) && g.needs_action(p) {
                progressed |= bot::play_turn(g, p);
            }
        }
        if controlled.iter().any(|&p| g.needs_action(p)) {
            return;
        }
        if !progressed {
            if !over(g) {
                g.stopped_reason = Some(StoppedReason::NoProgress);
            }
            return;
        }
    }
    if !over(g) && !controlled.iter().any(|&p| g.needs_action(p)) {
        g.stopped_reason = Some(StoppedReason::DriverGuard);
    }
}

/// Run `f` on consecutive chunks of `items` (and the matching chunks of `outs`, `per`
/// entries per item) on up to `threads` worker threads.
fn par_chunks<T: Send, O: Send>(
    threads: usize,
    items: &mut [T],
    outs: &mut [O],
    per: usize,
    f: impl Fn(usize, &mut [T], &mut [O]) + Sync,
) {
    let n = items.len();
    if n == 0 {
        return;
    }
    let chunk = n.div_ceil(threads.clamp(1, n));
    if chunk >= n {
        f(0, items, outs);
        return;
    }
    std::thread::scope(|s| {
        let jobs: Vec<_> = items
            .chunks_mut(chunk)
            .zip(outs.chunks_mut(chunk * per))
            .enumerate()
            .map(|(i, (it, out))| {
                let f = &f;
                crate::workers::spawn(s, move || f(i * chunk, it, out))
            })
            .collect();
        for j in jobs {
            j.join().expect("game worker panicked");
        }
    });
}

impl BatchEnv {
    /// `n_games` games of `cfg`, the seats in `controlled` played by the caller.
    pub fn new(
        n_games: usize,
        seed: u64,
        cfg: RulesConfig,
        controlled: Vec<PlayerId>,
        threads: usize,
    ) -> Result<BatchEnv, String> {
        cfg.validate()?;
        if n_games == 0 {
            return Err("n_games must be at least 1".into());
        }
        if controlled.is_empty() {
            return Err("at least one seat must be controlled".into());
        }
        let mut seen = vec![false; cfg.num_players];
        for &p in &controlled {
            if p >= cfg.num_players {
                return Err(format!("seat {p} out of range (0..{})", cfg.num_players));
            }
            if std::mem::replace(&mut seen[p], true) {
                return Err(format!("seat {p} is listed twice"));
            }
        }
        let mut env = BatchEnv {
            cfg,
            seed,
            controlled,
            episodes: Vec::new(),
            threads: threads.max(1),
            illegal_penalty: -0.01,
        };
        env.episodes = (0..n_games).map(|i| env.fresh(i, 0)).collect();
        Ok(env)
    }

    fn game_seed(&self, game: usize, index: u64) -> u64 {
        splitmix64(splitmix64(self.seed ^ splitmix64(game as u64)) ^ index)
    }

    fn fresh(&self, game: usize, index: u64) -> Episode {
        let mut g = Game::new(self.game_seed(game, index), self.cfg.clone());
        advance(&mut g, &self.controlled);
        Episode {
            game: g,
            index,
            finished: vec![false; self.controlled.len()],
        }
    }

    pub fn n_games(&self) -> usize {
        self.episodes.len()
    }

    /// The number of slots: games x controlled seats.
    pub fn n_slots(&self) -> usize {
        self.episodes.len() * self.controlled.len()
    }

    pub fn controlled(&self) -> &[PlayerId] {
        &self.controlled
    }

    pub fn config(&self) -> &RulesConfig {
        &self.cfg
    }

    /// Game `i` (for inspection: its state, `debug::view`, ...).
    pub fn game(&self, i: usize) -> &Game {
        &self.episodes[i].game
    }

    /// Episode `i`: the game and where its controlled seats stand.
    pub fn episode(&self, i: usize) -> &Episode {
        &self.episodes[i]
    }

    /// Write every slot's observation features (`n_slots x OBS_FLOATS`), card indices
    /// (`n_slots x OBS_CARDS`), legal mask (`n_slots x N_ACTIONS`, 1 = legal) and whether
    /// it must act now (`n_slots`, 1 = ready). A slot that is not ready has an all-zero
    /// mask; its observation is still that of its seat.
    pub fn observe(
        &self,
        floats: &mut [f32],
        cards: &mut [i32],
        masks: &mut [u8],
        ready: &mut [u8],
    ) -> Result<(), String> {
        let n = self.n_slots();
        for (name, len, want) in [
            ("floats", floats.len(), n * OBS_FLOATS),
            ("cards", cards.len(), n * OBS_CARDS),
            ("masks", masks.len(), n * N_ACTIONS),
            ("ready", ready.len(), n),
        ] {
            if len != want {
                return Err(format!("{name}: {len} entries, expected {want}"));
            }
        }
        let k = self.controlled.len();
        let controlled = &self.controlled;
        // One output row per slot, packed for the chunked workers.
        let mut rows: Vec<ObsRow> = floats
            .chunks_mut(OBS_FLOATS)
            .zip(cards.chunks_mut(OBS_CARDS))
            .zip(masks.chunks_mut(N_ACTIONS))
            .zip(ready.iter_mut())
            .map(|(((f, c), m), r)| (f, c, m, r))
            .collect();
        let mut eps: Vec<&Episode> = self.episodes.iter().collect();
        par_chunks(self.threads, &mut eps, &mut rows, k, |_, eps, rows| {
            for (e, rows) in eps.iter().zip(rows.chunks_mut(k)) {
                for (j, (f, c, m, r)) in rows.iter_mut().enumerate() {
                    let p = controlled[j];
                    let o = observe(&e.game, p);
                    f.copy_from_slice(&o.floats);
                    c.copy_from_slice(&o.cards);
                    let act = !e.finished[j] && !over(&e.game) && e.game.needs_action(p);
                    **r = act as u8;
                    if act {
                        for (dst, legal) in m.iter_mut().zip(e.game.legal_mask(p)) {
                            *dst = legal as u8;
                        }
                    } else {
                        m.fill(0);
                    }
                }
            }
        });
        Ok(())
    }

    /// Apply one action per slot (see the module documentation); `out` gets one result per
    /// slot.
    pub fn step(&mut self, actions: &[i64], out: &mut [SlotResult]) -> Result<(), String> {
        let n = self.n_slots();
        if actions.len() != n || out.len() != n {
            return Err(format!(
                "{} actions and {} results for {n} slots",
                actions.len(),
                out.len()
            ));
        }
        let k = self.controlled.len();
        let mut work: Vec<(&i64, &mut SlotResult)> = actions.iter().zip(out.iter_mut()).collect();
        let mut eps = std::mem::take(&mut self.episodes);
        {
            let this = &*self;
            par_chunks(self.threads, &mut eps, &mut work, k, |first, eps, work| {
                for (gi, (e, slots)) in eps.iter_mut().zip(work.chunks_mut(k)).enumerate() {
                    this.step_game(first + gi, e, slots);
                }
            });
        }
        self.episodes = eps;
        Ok(())
    }

    fn step_game(&self, gi: usize, e: &mut Episode, slots: &mut [(&i64, &mut SlotResult)]) {
        let n = e.game.players.len();
        // The seats that must act now, as the caller observed them. A seat that comes to
        // need an action while the others' actions are applied (a new turn's offer after
        // the last seat ended its turn) waits for the next `observe`.
        let ready: Vec<bool> = (0..self.controlled.len())
            .map(|j| !e.finished[j] && !over(&e.game) && e.game.needs_action(self.controlled[j]))
            .collect();
        for (j, (a, r)) in slots.iter_mut().enumerate() {
            **r = SlotResult::default();
            let p = self.controlled[j];
            if !ready[j] || over(&e.game) || !e.game.needs_action(p) {
                continue;
            }
            let legal = e.game.legal_mask(p);
            let ok = usize::try_from(**a)
                .ok()
                .filter(|&a| a < legal.len() && legal[a]);
            match ok.map(|a| e.game.apply_index(p, a)) {
                Some(Ok(())) => {}
                _ => {
                    r.rejected = true;
                    r.reward += self.illegal_penalty;
                }
            }
        }
        advance(&mut e.game, &self.controlled);
        for (j, (_, r)) in slots.iter_mut().enumerate() {
            let p = self.controlled[j];
            if !e.finished[j] && (over(&e.game) || !e.game.players[p].alive) {
                e.finished[j] = true;
                r.done = true;
                r.placement = e.game.players[p].placement;
                r.reward += placement_reward(r.placement, n);
            }
        }
        if e.finished.iter().all(|x| *x) {
            *e = self.fresh(gi, e.index + 1);
        }
    }

    /// Game `i` and its episode state as JSON.
    pub fn save(&self, i: usize) -> Result<String, String> {
        let e = self
            .episodes
            .get(i)
            .ok_or_else(|| format!("game {i} out of range"))?;
        serde_json::to_string(e).map_err(|e| e.to_string())
    }

    /// Replace game `i` by a saved one (`save`). The saved game must have this build's card
    /// table and as many controlled seats as this environment.
    pub fn load(&mut self, i: usize, json: &str) -> Result<(), String> {
        if i >= self.episodes.len() {
            return Err(format!("game {i} out of range"));
        }
        let e: Episode = serde_json::from_str(json).map_err(|e| e.to_string())?;
        e.game.check_loaded()?;
        if e.finished.len() != self.controlled.len() {
            return Err(format!(
                "saved with {} controlled seats, this environment has {}",
                e.finished.len(),
                self.controlled.len()
            ));
        }
        self.episodes[i] = e;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_run(threads: usize, controlled: Vec<PlayerId>, steps: usize) -> Vec<SlotResult> {
        let mut env = BatchEnv::new(3, 11, RulesConfig::default(), controlled, threads).unwrap();
        let n = env.n_slots();
        let (mut f, mut c) = (vec![0.0; n * OBS_FLOATS], vec![0; n * OBS_CARDS]);
        let (mut m, mut r) = (vec![0u8; n * N_ACTIONS], vec![0u8; n]);
        let mut out = vec![SlotResult::default(); n];
        let mut all = vec![];
        let mut rng = crate::rng::Rng::new(5);
        for _ in 0..steps {
            env.observe(&mut f, &mut c, &mut m, &mut r).unwrap();
            let actions: Vec<i64> = (0..n)
                .map(|s| {
                    let legal: Vec<usize> = (0..N_ACTIONS)
                        .filter(|&a| m[s * N_ACTIONS + a] == 1)
                        .collect();
                    // Random legal actions, ending the turn now and then so that games end.
                    if legal.is_empty() {
                        0
                    } else if legal.contains(&crate::actions::A_END) && rng.below(10) < 3 {
                        crate::actions::A_END as i64
                    } else {
                        legal[rng.below(legal.len())] as i64
                    }
                })
                .collect();
            env.step(&actions, &mut out).unwrap();
            all.extend_from_slice(&out);
        }
        all
    }

    #[test]
    fn results_do_not_depend_on_the_number_of_threads() {
        assert_eq!(random_run(1, vec![0], 400), random_run(3, vec![0], 400));
    }

    #[test]
    fn episodes_end_with_a_placement_and_a_reward_in_range() {
        let all = random_run(2, vec![0, 3], 3_000);
        let done: Vec<&SlotResult> = all.iter().filter(|r| r.done).collect();
        assert!(!done.is_empty());
        for r in done {
            assert!((1..=8).contains(&r.placement), "{r:?}");
            assert!((-1.01..=1.0).contains(&r.reward), "{r:?}");
        }
        assert!(
            all.iter().all(|r| !r.rejected),
            "legal actions are never rejected"
        );
    }

    #[test]
    fn an_illegal_action_is_rejected_and_ignored() {
        let mut env = BatchEnv::new(1, 3, RulesConfig::default(), vec![0], 1).unwrap();
        let before = env.save(0).unwrap();
        let mut out = vec![SlotResult::default(); 1];
        env.step(&[N_ACTIONS as i64], &mut out).unwrap();
        assert!(out[0].rejected && out[0].reward < 0.0 && !out[0].done);
        assert_eq!(env.save(0).unwrap(), before);
    }

    #[test]
    fn a_saved_game_continues_exactly() {
        let mut a = BatchEnv::new(2, 9, RulesConfig::default(), vec![0], 1).unwrap();
        let mut out = vec![SlotResult::default(); 2];
        for _ in 0..30 {
            a.step(&[0, 0], &mut out).unwrap();
        }
        let mut b = BatchEnv::new(2, 1, RulesConfig::default(), vec![0], 1).unwrap();
        for i in 0..2 {
            b.load(i, &a.save(i).unwrap()).unwrap();
        }
        for _ in 0..50 {
            let mut oa = vec![SlotResult::default(); 2];
            let mut ob = vec![SlotResult::default(); 2];
            a.step(&[0, 0], &mut oa).unwrap();
            b.step(&[0, 0], &mut ob).unwrap();
            assert_eq!(oa, ob);
        }
        assert_eq!(a.save(1).unwrap(), b.save(1).unwrap());
    }

    #[test]
    fn bad_arguments_are_errors() {
        let cfg = RulesConfig::default;
        assert!(BatchEnv::new(0, 1, cfg(), vec![0], 1).is_err());
        assert!(BatchEnv::new(1, 1, cfg(), vec![], 1).is_err());
        assert!(BatchEnv::new(1, 1, cfg(), vec![8], 1).is_err());
        assert!(BatchEnv::new(1, 1, cfg(), vec![2, 2], 1).is_err());
        let mut env = BatchEnv::new(1, 1, cfg(), vec![0], 1).unwrap();
        let mut out = vec![SlotResult::default(); 2];
        assert!(env.step(&[0, 0], &mut out).is_err());
        assert!(env.load(0, "{}").is_err());
    }
}
