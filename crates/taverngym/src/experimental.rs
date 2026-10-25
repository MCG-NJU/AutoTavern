//! The experimental mode (`RulesConfig::experimental_mode`): opt-in rules for a global
//! turn-four Quest, Buddies and Timewarp, and the extra actions they need outside the fixed
//! action space (`Game::extra_action`).
use crate::actions::Action;
use crate::cards::{CardDef, CardId, db};
use crate::engine::{Game, Phase};
use crate::recruit::ActResult;
use crate::script::script;
use crate::state::Item;
use crate::types::{EntityId, PlayerId};
use serde::{Deserialize, Serialize};

pub fn buddy_allowed(card: CardId) -> bool {
    db().get(card).id != "TB_BaconShop_HERO_56_Buddy" && script(card).is_some()
}
pub fn hero_allowed(hero: &CardDef) -> bool {
    hero.buddy.is_some_and(buddy_allowed)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BuddyButton {
    pub card: CardId,
    pub claims: u8,
    pub discount: i32,
    pub last_combat_turn: u32,
}
impl BuddyButton {
    pub fn new(card: CardId) -> Self {
        Self {
            card,
            claims: 0,
            discount: 0,
            last_combat_turn: 0,
        }
    }
    pub fn price(&self) -> i32 {
        let tier = db().get(self.card).tier.clamp(1, 6) as i32;
        (9 + 2 * tier + 2 * self.claims as i32 - self.discount).max(0)
    }
}

/// Entity ids, rather than hand indices, protect expanded-hand operations from stale UI.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExtraAction {
    ClaimBuddy { expected_claims: u8 },
    Play { hid: EntityId, pos: usize },
    Magnetize { hid: EntityId, board: EntityId },
}
impl Game {
    pub fn buddy_available(&self, p: PlayerId) -> bool {
        let pl = &self.players[p];
        self.cfg.experimental_mode
            && self.phase == Phase::Recruit
            && pl.alive
            && !pl.ended_turn
            && !pl.skipping
            && pl.pending.is_empty()
            && (pl.uncapped || pl.actions_this_turn < self.cfg.action_cap(self.turn))
            && pl
                .buddy_button
                .as_ref()
                .is_some_and(|b| b.claims < 2 && pl.gold >= b.price())
    }
    pub fn claim_buddy(&mut self, p: PlayerId, expected_claims: u8) -> ActResult {
        if !self.buddy_available(p) {
            return Err("Buddy button unavailable");
        }
        let b = self.players[p].buddy_button.as_ref().unwrap();
        if b.claims != expected_claims {
            return Err("stale Buddy claim");
        }
        let (card, price, count) = (b.card, b.price(), if b.claims == 0 { 1 } else { 2 });
        let pl = &mut self.players[p];
        pl.actions_this_turn += 1;
        let b = pl.buddy_button.as_mut().unwrap();
        b.claims += 1;
        b.discount = 0;
        self.log(|| format!("p{p} claims {count} Buddies for {price} Gold"));
        // Protected delivery, including a triple as each queued copy enters the hand.
        self.with_recruit(p, |c| {
            c.spend_gold(0, price);
            for _ in 0..count {
                let item = Item::Minion(c.new_minion(0, card));
                if c.g.players[p].hand.len() >= c.g.cfg.hand_size {
                    c.g.players[p].hand_queue.push(item);
                } else {
                    c.add_to_hand(0, item);
                }
            }
        });
        self.flush_hand_queue(p);
        self.recruit_deaths();
        self.players[p].normalize_timewarp_hand_limit(self.cfg.hand_size);
        Ok(())
    }
    pub fn discount_buddy(&mut self, p: PlayerId, lost: bool) {
        if let Some(b) = self.players[p].buddy_button.as_mut()
            && b.claims < 2
            && b.last_combat_turn != self.turn
        {
            b.discount += if lost { 2 } else { 3 };
            b.last_combat_turn = self.turn;
        }
    }
    pub fn auto_claim_buddy(&mut self, p: PlayerId) -> bool {
        if !self.buddy_available(p) {
            return false;
        }
        let b = self.players[p].buddy_button.as_ref().unwrap();
        if b.price() > 2 || self.players[p].hand.len() >= self.cfg.hand_size {
            return false;
        }
        self.claim_buddy(p, b.claims).is_ok()
    }
    pub fn extra_action(&mut self, p: PlayerId, action: ExtraAction) -> ActResult {
        if !self.cfg.experimental_mode {
            return Err("experimental actions are disabled");
        }
        match action {
            ExtraAction::ClaimBuddy { expected_claims } => self.claim_buddy(p, expected_claims),
            ExtraAction::Play { hid, pos } => {
                let hand = self.players[p]
                    .hand
                    .iter()
                    .position(|h| h.hid == hid)
                    .ok_or("card no longer in hand")?;
                if pos >= crate::actions::N_POS {
                    return Err("bad board position");
                }
                let mut probe = self.clone();
                probe.players[p].hand.swap(0, hand);
                if !probe.legal_mask(p)[crate::actions::A_PLAY + pos] {
                    return Err("card cannot be played now");
                }
                self.apply(p, Action::Play { hand, pos })
            }
            ExtraAction::Magnetize { hid, board } => {
                let hand = self.players[p]
                    .hand
                    .iter()
                    .position(|h| h.hid == hid)
                    .ok_or("card no longer in hand")?;
                let board = self.players[p]
                    .board
                    .iter()
                    .position(|m| m.eid == board)
                    .ok_or("host no longer on board")?;
                let mut probe = self.clone();
                probe.players[p].hand.swap(0, hand);
                if !probe.legal_mask(p)[crate::actions::A_MAGNETIZE + board] {
                    return Err("card cannot magnetize now");
                }
                self.apply(p, Action::Magnetize { hand, board })
            }
        }
    }
}
