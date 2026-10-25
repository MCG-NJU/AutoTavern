//! Flat discrete action space for RL, with legality masks.
//!
//! Layout (N_ACTIONS = 245):
//! ```text
//!   0 EndTurn   1 Refresh   2 Freeze   3 Upgrade   4..6 HeroPower(slot 0..2)
//!   6 DarkGift  7 Cancel
//!   8..16   Buy(shop slot 0..8)
//!  16..23   Sell(board 0..7)
//!  23..30   Activate(board 0..7)
//!  30..46   Choose(option 0..16)          (only legal while a choice is pending)
//!  46..126  Play(hand 0..10, pos 0..8)    (spells: pos 0 only)
//! 126..196  Magnetize(hand 0..10, board 0..7)
//! 196..245  Move(from 0..7, to 0..7)
//! ```
//! Choices (hero, Discover, Choose One, target, trinket offer) block other actions.
//! `Cancel` backs out of spell / hero power / Activate targeting (nothing is paid until
//! the target is chosen).

use serde::{Deserialize, Serialize};

use crate::cards::db;
use crate::engine::{Game, Phase};
use crate::recruit::ActResult;
use crate::script::{TargetSpec, flag, script};
use crate::state::{Item, Minion};
use crate::types::{EntityId, PlayerId};

pub const N_SHOP: usize = 8;
pub const N_BOARD: usize = 7;
pub const N_HAND: usize = 10;
pub const N_CHOICE: usize = 16;
pub const N_POS: usize = 8;

pub const A_END: usize = 0;
pub const A_REFRESH: usize = 1;
pub const A_FREEZE: usize = 2;
pub const A_UPGRADE: usize = 3;
/// Hero Power slots: the hero's own and one extra (Genn, Reinvigorating Light...).
pub const N_POWERS: usize = 2;
pub const A_HERO_POWER: usize = 4;
pub const A_DARK_GIFT: usize = A_HERO_POWER + N_POWERS;
pub const A_CANCEL: usize = A_DARK_GIFT + 1;
pub const A_BUY: usize = A_CANCEL + 1;
pub const A_SELL: usize = A_BUY + N_SHOP;
pub const A_ACTIVATE: usize = A_SELL + N_BOARD;
pub const A_CHOOSE: usize = A_ACTIVATE + N_BOARD;
pub const A_PLAY: usize = A_CHOOSE + N_CHOICE;
pub const A_MAGNETIZE: usize = A_PLAY + N_HAND * N_POS;
pub const A_MOVE: usize = A_MAGNETIZE + N_HAND * N_BOARD;
pub const N_ACTIONS: usize = A_MOVE + N_BOARD * N_BOARD;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    EndTurn,
    Refresh,
    Freeze,
    Upgrade,
    HeroPower(usize),
    DarkGift,
    Cancel,
    Buy(usize),
    Sell(usize),
    Activate(usize),
    Choose(usize),
    Play { hand: usize, pos: usize },
    Magnetize { hand: usize, board: usize },
    Move { from: usize, to: usize },
}

impl Action {
    pub fn encode(self) -> usize {
        match self {
            Action::EndTurn => A_END,
            Action::Refresh => A_REFRESH,
            Action::Freeze => A_FREEZE,
            Action::Upgrade => A_UPGRADE,
            Action::HeroPower(slot) => A_HERO_POWER + slot,
            Action::DarkGift => A_DARK_GIFT,
            Action::Cancel => A_CANCEL,
            Action::Buy(i) => A_BUY + i,
            Action::Sell(i) => A_SELL + i,
            Action::Activate(i) => A_ACTIVATE + i,
            Action::Choose(i) => A_CHOOSE + i,
            Action::Play { hand, pos } => A_PLAY + hand * N_POS + pos,
            Action::Magnetize { hand, board } => A_MAGNETIZE + hand * N_BOARD + board,
            Action::Move { from, to } => A_MOVE + from * N_BOARD + to,
        }
    }

    pub fn decode(a: usize) -> Option<Action> {
        Some(match a {
            A_END => Action::EndTurn,
            A_REFRESH => Action::Refresh,
            A_FREEZE => Action::Freeze,
            A_UPGRADE => Action::Upgrade,
            _ if (A_HERO_POWER..A_DARK_GIFT).contains(&a) => Action::HeroPower(a - A_HERO_POWER),
            A_DARK_GIFT => Action::DarkGift,
            A_CANCEL => Action::Cancel,
            _ if a < A_SELL => Action::Buy(a - A_BUY),
            _ if a < A_ACTIVATE => Action::Sell(a - A_SELL),
            _ if a < A_CHOOSE => Action::Activate(a - A_ACTIVATE),
            _ if a < A_PLAY => Action::Choose(a - A_CHOOSE),
            _ if a < A_MAGNETIZE => {
                let k = a - A_PLAY;
                Action::Play {
                    hand: k / N_POS,
                    pos: k % N_POS,
                }
            }
            _ if a < A_MOVE => {
                let k = a - A_MAGNETIZE;
                Action::Magnetize {
                    hand: k / N_BOARD,
                    board: k % N_BOARD,
                }
            }
            _ if a < N_ACTIONS => {
                let k = a - A_MOVE;
                Action::Move {
                    from: k / N_BOARD,
                    to: k % N_BOARD,
                }
            }
            _ => return None,
        })
    }
}

impl Game {
    /// Apply an action for player `p`.
    pub fn apply(&mut self, p: PlayerId, a: Action) -> ActResult {
        if self.phase == Phase::GameOver {
            return Err("game over");
        }
        self.players[p].actions_this_turn += 1;
        let r = self.apply_inner(p, a);
        if r.is_err() {
            // A refused action is not an action: it does not use up the turn's budget
            // (a refusal never moves the game on, so this is still the same turn).
            let n = &mut self.players[p].actions_this_turn;
            *n = n.saturating_sub(1);
        }
        self.recruit_deaths();
        if self.cfg.experimental_mode {
            self.players[p].normalize_timewarp_hand_limit(self.cfg.hand_size);
        }
        if self.players[p].alive && !self.players[p].hand_queue.is_empty() {
            self.flush_hand_queue(p);
        }
        r
    }

    /// The most actions seat `p` may take this turn: the turn's entry of
    /// `RulesConfig::turn_action_caps`.
    pub fn action_cap_of(&self, _p: PlayerId) -> u32 {
        self.cfg.action_cap(self.turn)
    }

    fn apply_inner(&mut self, p: PlayerId, a: Action) -> ActResult {
        match a {
            Action::Choose(i) => self.choose(p, i),
            Action::Cancel => self.cancel(p),
            _ if self.phase == Phase::HeroSelect => Err("choose a hero first"),
            Action::EndTurn => self.end_turn(p),
            _ if !self.players[p].uncapped
                && self.players[p].actions_this_turn > self.action_cap_of(p) =>
            {
                Err("action limit for this turn reached")
            }
            // A skipped turn (Faelin / A. F. Kay) only allows Freeze and End Turn.
            Action::Refresh
            | Action::Upgrade
            | Action::HeroPower(_)
            | Action::DarkGift
            | Action::Buy(_)
            | Action::Sell(_)
            | Action::Activate(_)
            | Action::Play { .. }
            | Action::Magnetize { .. }
            | Action::Move { .. }
                if self.players[p].skipping =>
            {
                Err("this turn is skipped")
            }
            Action::Refresh => self.refresh(p),
            Action::Freeze => self.freeze(p),
            Action::Upgrade => self.upgrade(p),
            Action::HeroPower(slot) => self.hero_power(p, slot),
            Action::DarkGift => self.dark_gift(p),
            Action::Buy(i) => self.buy(p, i),
            Action::Sell(i) => self.sell(p, i),
            Action::Activate(i) => self.activate(p, i),
            Action::Play { hand, pos } => self.play(p, hand, pos),
            Action::Magnetize { hand, board } => self.magnetize(p, hand, board),
            Action::Move { from, to } => self.move_minion(p, from, to),
        }
    }

    pub fn apply_index(&mut self, p: PlayerId, a: usize) -> ActResult {
        let act = Action::decode(a).ok_or("bad action index")?;
        self.apply(p, act)
    }

    /// Whether `p` must act now (alive, turn not ended, or a pending choice).
    pub fn needs_action(&self, p: PlayerId) -> bool {
        let pl = &self.players[p];
        if self.phase == Phase::GameOver || !pl.alive {
            return false;
        }
        !pl.pending.is_empty() || (self.phase == Phase::Recruit && !pl.ended_turn)
    }

    /// Legality mask over the flat action space.
    pub fn legal_mask(&self, p: PlayerId) -> Vec<bool> {
        let mut m = vec![false; N_ACTIONS];
        if !self.needs_action(p) {
            return m;
        }
        let pl = &self.players[p];
        if let Some(top) = pl.pending.last() {
            for i in 0..top.len().min(N_CHOICE) {
                m[A_CHOOSE + i] = self.option_legal(p, i);
            }
            m[A_CANCEL] = top.cancellable();
            return m;
        }
        let cfg = &self.cfg;
        m[A_END] = true;
        m[A_FREEZE] = self.can_freeze(p);
        // Past the per-turn action cap only End Turn stays legal (`apply` agrees), in a
        // skipped turn too.
        if !pl.uncapped && pl.actions_this_turn >= self.action_cap_of(p) {
            m[A_FREEZE] = false;
            return m;
        }
        if pl.skipping {
            return m;
        }
        m[A_REFRESH] = pl.gold >= self.refresh_price(p) || self.malchezaar(p).is_some();
        m[A_FREEZE] = self.can_freeze(p);
        m[A_UPGRADE] = self.can_upgrade(p);
        for slot in 0..N_POWERS {
            m[A_HERO_POWER + slot] = self.hero_power_usable(p, slot);
        }
        m[A_DARK_GIFT] = self.can_dark_gift(p);
        let hand_full = pl.hand.len() >= cfg.hand_size;
        for i in 0..pl.shop.len().min(N_SHOP) {
            if let Some((price, health, _)) = self.slot_price(p, i) {
                let cast =
                    script(pl.shop[i].item.card()).is_some_and(|x| x.has_flag(flag::CAST_ON_BUY));
                m[A_BUY + i] = (!hand_full || cast) && (pl.gold >= price || health);
            }
        }
        for i in 0..pl.board.len().min(N_BOARD) {
            m[A_SELL + i] = true;
            m[A_ACTIVATE + i] = self.can_activate(p, i);
        }
        let board_full = pl.board.len() >= cfg.board_size;
        for (h, card) in pl.hand.iter().enumerate().take(N_HAND) {
            if card.locked > 0 {
                continue;
            }
            match &card.item {
                Item::Minion(_) => {
                    if !board_full {
                        for pos in 0..=pl.board.len().min(N_POS - 1) {
                            m[A_PLAY + h * N_POS + pos] = true;
                        }
                    }
                    for b in 0..pl.board.len().min(N_BOARD) {
                        m[A_MAGNETIZE + h * N_BOARD + b] = self.can_magnetize(p, h, b);
                    }
                }
                Item::Spell { card: c, .. } => {
                    m[A_PLAY + h * N_POS] = self.spell_playable(p, *c);
                }
            }
        }
        let n = pl.board.len().min(N_BOARD);
        for f in 0..n {
            for t in 0..n {
                if f != t {
                    m[A_MOVE + f * N_BOARD + t] = true;
                }
            }
        }
        m
    }

    pub fn legal_actions(&self, p: PlayerId) -> Vec<usize> {
        self.legal_mask(p)
            .iter()
            .enumerate()
            .filter(|(_, ok)| **ok)
            .map(|(i, _)| i)
            .collect()
    }

    fn spell_playable(&self, p: PlayerId, card: crate::cards::CardId) -> bool {
        let Some(s) = script(card) else { return true };
        if s.has_flag(flag::UNPLAYABLE) {
            return false;
        }
        if s.has_flag(flag::NEXT_COMBAT | flag::ONE_PENDING)
            && self.players[p].effects.iter().any(|x| x.card == card)
        {
            return false;
        }
        if s.spell_target == TargetSpec::None {
            return true;
        }
        // A Choose One spell whose every option needs a target is playable like any targeted spell
        // (Forest's Bounty's second option has none).
        if s.choose_one && crate::cards::db().get(card).id == "BG31_886" {
            return true;
        }
        self.has_targets(p, s.spell_target)
    }

    /// Whether the Hero Power in `slot` can be pressed now: usable (`can_hero_power`) and,
    /// if it takes a target, with one: what the game offers, and the observation's
    /// "usable" flag (Drest'agath with an empty hand cannot use Incubate).
    pub fn hero_power_usable(&self, p: PlayerId, slot: usize) -> bool {
        self.can_hero_power(p, slot) && self.hero_power_has_target(p, slot)
    }

    fn hero_power_has_target(&self, p: PlayerId, slot: usize) -> bool {
        let Some(hp) = self.players[p].powers.get(slot).map(|h| h.card) else {
            return false;
        };
        let spec = script(hp)
            .map(|s| s.hero_power_target)
            .unwrap_or(TargetSpec::None);
        spec == TargetSpec::None || self.has_targets(p, spec)
    }

    /// Whether a targeted effect has a legal target, without building a context: the same
    /// answer as `Ctx::legal_targets(.., spec, source)` being non-empty, variant by variant.
    /// `source` is the entity whose effect it is (an Activate's minion) and that "another" /
    /// "a different" exclude; 0 for a spell or a Hero Power.
    pub fn has_targets_from(&self, p: PlayerId, spec: TargetSpec, source: EntityId) -> bool {
        use crate::script::{Hook, gildable_buddy, minion_has, rally_untriggerable};
        let pl = &self.players[p];
        let board = |f: &dyn Fn(&Minion) -> bool| pl.board.iter().any(f);
        let tavern =
            |f: &dyn Fn(&Minion) -> bool| pl.shop.iter().any(|s| s.item.as_minion().is_some_and(f));
        let any = |f: &dyn Fn(&Minion) -> bool| board(f) || tavern(f);
        let other = |m: &Minion| m.eid != source;
        let gildable = |m: &Minion| !m.golden && db().get(m.card).golden.is_some();
        match spec {
            TargetSpec::None => true,
            TargetSpec::Friendly => !pl.board.is_empty(),
            TargetSpec::FriendlyOther => board(&other),
            TargetSpec::FriendlyRace(r) => board(&|m| m.is(r)),
            TargetSpec::FriendlyRaceOther(r) => board(&|m| m.is(r) && other(m)),
            TargetSpec::FriendlyNonGoldenMaxTier(t) => board(&|m| m.tier <= t && gildable(m)),
            TargetSpec::FriendlyEligiblePirate => board(&|m| {
                other(m) && m.is(crate::types::Races::PIRATE) && m.tier <= 4 && gildable(m)
            }),
            TargetSpec::Tavern => tavern(&|_| true),
            TargetSpec::TavernCard => pl
                .shop
                .iter()
                .any(|s| s.item.as_minion().is_none_or(|m| !m.golden)),
            TargetSpec::TavernAny => !pl.shop.is_empty(),
            TargetSpec::FriendlyOrTavernAny => !pl.shop.is_empty() || !pl.board.is_empty(),
            TargetSpec::FriendlyOrTavern => any(&|_| true),
            TargetSpec::AnyNonGolden => any(&gildable),
            TargetSpec::AnyMaxTier(t) => any(&|m| m.tier <= t),
            TargetSpec::AnyOther => any(&other),
            TargetSpec::AnyRace(r) => any(&|m| m.is(r)),
            TargetSpec::AnyRaceOther(r) => any(&|m| m.is(r) && other(m)),
            TargetSpec::AnyTyped => any(&|m| !m.races.is_none()),
            TargetSpec::AnyNoDivineShield => any(&|m| !m.has(crate::types::Kw::DIVINE_SHIELD)),
            TargetSpec::BuddyNonGolden => any(&|m| other(m) && gildable_buddy(m)),
            TargetSpec::Hand => pl.hand.iter().any(crate::recruit::discardable),
            TargetSpec::FriendlyHas(h) => board(&|m| other(m) && minion_has(m, h)),
            TargetSpec::FriendlyRallyTriggerable => {
                board(&|m| other(m) && minion_has(m, Hook::Rally) && !rally_untriggerable(m.card))
            }
        }
    }

    /// `has_targets_from` for an effect without a source of its own.
    fn has_targets(&self, p: PlayerId, spec: TargetSpec) -> bool {
        self.has_targets_from(p, spec, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        for a in 0..N_ACTIONS {
            let act = Action::decode(a).unwrap();
            assert_eq!(act.encode(), a, "{act:?}");
        }
        assert!(Action::decode(N_ACTIONS).is_none());
        assert_eq!(N_ACTIONS, 245);
    }
}
