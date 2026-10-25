//! Shared fixtures for the per-group content tests (`mod common; use common::*;`).
#![allow(dead_code, unused_imports)]

pub use taverngym::actions::Action;
pub use taverngym::cards::{CardId, cid, db};
pub use taverngym::engine::Ctx;
pub use taverngym::state::{Item, Minion, Pending};
pub use taverngym::types::{Kw, PlayerId, Races};
pub use taverngym::{Game, RulesConfig};

/// A game past hero selection, turn 1 recruit phase, with Aberrations in the lobby (so every
/// player has a Deity). Every seat is pinned to Patchwerk (a passive hero) with the picked
/// heroes' start effects undone; triples are off (`triple_copies = 99`) so that tests can
/// use several copies of a minion.
pub fn game(seed: u64) -> Game {
    let cfg = RulesConfig {
        aberration_forced: true,
        ..RulesConfig::default()
    };
    let mut g = Game::new(seed, cfg);
    for p in 0..8 {
        g.apply(p, Action::Choose(0)).unwrap();
    }
    for p in 0..8 {
        g.players[p].hero = Some(cid("TB_BaconShop_HERO_34"));
        g.players[p].pending.clear();
        g.players[p].hand.clear();
        g.players[p].stats = Default::default();
        g.players[p].effects.clear();
        g.players[p].quests.clear();
        g.players[p].counters.clear();
        g.players[p].max_gold_bonus = 0;
        g.players[p].skipping = false;
        g.players[p].board.clear();
        g.players[p].powers.clear();
        g.players[p].set_hero_power(0, cid("TB_BaconShop_HP_035"));
    }
    g.cfg.triple_copies = 99;
    g
}

/// A fresh minion of card `id` owned by player `p` (not placed anywhere).
pub fn minion(g: &mut Game, p: PlayerId, id: &str) -> Minion {
    g.with_recruit(p, |c| c.new_minion(0, cid(id)))
}

/// Replace player `p`'s warband with fresh minions (no Battlecries, no events).
pub fn set_board(g: &mut Game, p: PlayerId, ids: &[&str]) {
    let b: Vec<Minion> = ids.iter().map(|id| minion(g, p, id)).collect();
    g.players[p].board = b;
}

/// (attack, health) of player `p`'s warband, left to right.
pub fn stats(g: &Game, p: PlayerId) -> Vec<(i32, i32)> {
    g.players[p].board.iter().map(|m| (m.atk, m.hp)).collect()
}

/// Put card `id` into player `p`'s hand; returns its hand id.
pub fn give(g: &mut Game, p: PlayerId, id: &str) -> u32 {
    g.with_recruit(p, |c| c.give_card(0, cid(id)).unwrap())
}

/// Play the last hand card at the right end of the warband (a spell: just cast it).
pub fn play_last(g: &mut Game, p: PlayerId) {
    let hand = g.players[p].hand.len() - 1;
    let pos = g.players[p].board.len();
    g.apply(p, Action::Play { hand, pos }).unwrap();
}

/// Cast the last hand card on the friendly minion at `board_idx`.
pub fn cast_on(g: &mut Game, p: PlayerId, board_idx: usize) {
    let hand = g.players[p].hand.len() - 1;
    g.apply(p, Action::Play { hand, pos: 0 }).unwrap();
    let eid = g.players[p].board[board_idx].eid;
    let Some(Pending::Target { options, .. }) = g.players[p].pending.last() else {
        panic!("expected a target choice")
    };
    let i = options.iter().position(|e| *e == eid).unwrap();
    g.apply(p, Action::Choose(i)).unwrap();
}

/// Everyone answers pending choices with option 0 and ends the turn (combat follows and
/// the next turn starts).
pub fn end_turn_all(g: &mut Game) {
    for p in 0..8 {
        while g.players[p].pending.last().is_some() {
            g.apply(p, Action::Choose(0)).unwrap();
        }
        if g.needs_action(p) {
            g.apply(p, Action::EndTurn).unwrap();
        }
    }
}

/// A combat context between players 0 and 1 on copies of their warbands: drive it by hand
/// (`c.destroy(eid); c.resolve_deaths();`, `taverngym::combat::attack_now`...). Persistent
/// effects ("permanently", cards gained) reach the players when it is dropped.
pub fn combat_ctx(g: &mut Game) -> Ctx<'_> {
    let boards = [g.players[0].board.clone(), g.players[1].board.clone()];
    Ctx::new(g, boards, [Some(0), Some(1)], true)
}

/// Run a whole combat between players 0 and 1 (warbands are restored afterwards).
pub fn fight(g: &mut Game) -> taverngym::combat::CombatOutcome {
    g.simulate_combat(0, Some(1), None)
}
