//! Fixed-size observation encoding for RL (from one player's point of view).
//!
//! Output: `floats` (f32 features) and `cards` (i32 card indices for embedding tables, 0 = empty
//! slot, otherwise `CardId + 1`). Only information visible to the player is encoded: opponents
//! contribute public summaries and the warband this player saw in its last combat against them
//! (`Game::known_board`), never hands or boards it did not fight.

use crate::actions::{N_BOARD, N_CHOICE, N_HAND, N_SHOP};
use crate::cards::{CardId, db};
use crate::engine::{Game, Phase};
use crate::state::{Item, Minion, Pending};
use crate::types::{Kw, PlayerId, Races};

pub const MINION_F: usize = 8 + Kw::NAMES.len() + Races::COUNT;
/// Item header: present, is-minion, cost, locked, temporary, Choose-One-combined, frozen
/// (Tavern: frozen; hand: dies if played this turn, Tomb Turning), kept from last turn
/// (Tavern), marked (Tavern: Darkmoon Ticket; hand: linked group).
pub const ITEM_F: usize = 9 + MINION_F;
pub const GLOBAL_F: usize = 42 + Races::COUNT;
/// Active Quests shown (present, progress / goal each).
pub const N_QUESTS: usize = 3;
pub const QUEST_F: usize = 2;
/// Secrets in play shown: a player holds at most 5 (the Hearthstone limit; Akazamzarak's
/// four plus one from a trinket).
pub const N_SECRETS: usize = crate::state::MAX_SECRETS;
pub const OPP_F: usize = 9;
pub const N_OPP: usize = 7;
/// Opponent warband memory (last seen boards, 7 minions each).
pub const OPP_BOARD_F: usize = N_OPP * N_BOARD * MINION_F;
pub const PENDING_KINDS: usize = 9;

pub const OBS_FLOATS: usize = GLOBAL_F
    + N_QUESTS * QUEST_F
    + PENDING_KINDS
    + N_BOARD * MINION_F
    + N_HAND * ITEM_F
    + N_SHOP * ITEM_F
    + N_CHOICE * ITEM_F
    + N_OPP * OPP_F
    + OPP_BOARD_F;
/// hero, hero powers, Deity, source of the pending choice, board, hand, shop, choices, owned
/// trinkets (8), Quests with their Rewards, owned Rewards, Secrets, opponent heroes, opponent
/// last boards.
pub const OBS_CARDS: usize = 1
    + crate::actions::N_POWERS
    + 2
    + 3 * N_QUESTS
    + N_SECRETS
    + N_BOARD
    + N_HAND
    + N_SHOP
    + N_CHOICE
    + 8
    + N_OPP
    + N_OPP * N_BOARD;

fn card_index(c: CardId) -> i32 {
    c.idx() as i32 + 1
}

fn push_minion(f: &mut Vec<f32>, m: Option<&Minion>) {
    let Some(m) = m else {
        f.extend(std::iter::repeat_n(0.0, MINION_F));
        return;
    };
    f.push(1.0);
    f.push(m.atk as f32 / 10.0);
    f.push(m.hp as f32 / 10.0);
    f.push(m.max_hp as f32 / 10.0);
    f.push(m.tier as f32 / 6.0);
    f.push(m.golden as u8 as f32);
    f.push(m.attached.len() as f32 / 3.0);
    f.push(m.shield_hits as f32 / 3.0);
    for i in 0..Kw::NAMES.len() {
        f.push((m.kw.0 >> i & 1) as f32);
    }
    for i in 0..Races::COUNT {
        f.push((m.races.0 >> i & 1) as f32);
    }
}

fn push_item(f: &mut Vec<f32>, it: Option<&Item>, cost: i32, locked: u8, flags: [bool; 5]) {
    let [temp, comb, frozen, kept, marked] = flags.map(|b| b as u8 as f32);
    let head = |minion: f32| {
        [
            1.0,
            minion,
            cost as f32 / 5.0,
            locked as f32,
            temp,
            comb,
            frozen,
            kept,
            marked,
        ]
    };
    match it {
        None => f.extend(std::iter::repeat_n(0.0, ITEM_F)),
        Some(Item::Minion(m)) => {
            f.extend(head(1.0));
            push_minion(f, Some(m));
        }
        Some(Item::Spell { card, data }) => {
            f.extend(head(0.0));
            f.push(1.0);
            f.push(*data as f32 / 6.0);
            f.push(0.0);
            f.push(0.0);
            f.push(db().get(*card).tier as f32 / 6.0);
            f.extend(std::iter::repeat_n(0.0, MINION_F - 5));
        }
    }
}

/// The card that opened the pending choice (the Discover's source, the spell or minion
/// waiting for a target, the Choose One card); None for offers that have no source card.
fn pending_source(g: &Game, p: PlayerId) -> Option<CardId> {
    use crate::state::Continuation as C;
    let pl = &g.players[p];
    let minion = |eid: u32| pl.board.iter().find(|m| m.eid == eid).map(|m| m.card);
    match pl.pending.last()? {
        Pending::Discover { source, .. } => Some(*source),
        Pending::ChooseOne { card, .. } => Some(*card),
        Pending::Target { cont, .. } => match cont {
            C::Battlecry { eid, .. } | C::Activate { eid } => minion(*eid),
            C::Spell { card, .. } | C::Followup { card, .. } => Some(*card),
            C::AttachedBattlecry { card, .. } => Some(*card),
            C::HandSpell { hid, .. } => pl
                .hand
                .iter()
                .find(|h| h.hid == *hid)
                .map(|h| h.item.card()),
            C::HeroPower { slot, .. } => pl.powers.get(*slot as usize).map(|h| h.card),
        },
        _ => None,
    }
}

pub struct Observation {
    pub floats: Vec<f32>,
    pub cards: Vec<i32>,
}

pub fn observe(g: &Game, p: PlayerId) -> Observation {
    observe_view(g, p, g.players[p].sees_all_boards)
}

/// The observation with the opponents' warbands in a given view (`Game::board_shown`).
pub fn observe_view(g: &Game, p: PlayerId, all: bool) -> Observation {
    let pl = &g.players[p];
    let mut f: Vec<f32> = Vec::with_capacity(OBS_FLOATS);
    let mut c: Vec<i32> = Vec::with_capacity(OBS_CARDS);

    // ---- global
    let alive = g.alive_count();
    f.extend([
        g.turn as f32 / 20.0,
        (g.phase == Phase::HeroSelect) as u8 as f32,
        (g.phase == Phase::GameOver) as u8 as f32,
        alive as f32 / 8.0,
        pl.alive as u8 as f32,
        pl.health as f32 / 40.0,
        pl.armor as f32 / 20.0,
        pl.gold as f32 / 10.0,
        pl.max_gold_bonus as f32 / 5.0,
        pl.gold_next_turn as f32 / 5.0,
        pl.tier as f32 / 6.0,
        g.upgrade_price(p) as f32 / 10.0,
        g.refresh_price(p) as f32 / 2.0,
        g.minion_price(p) as f32 / 3.0,
        pl.free_refreshes as f32 / 3.0,
        pl.is_frozen() as u8 as f32,
        g.hero_power_usable(p, 0) as u8 as f32,
        g.hero_power_price(p, 0).unwrap_or(0) as f32 / 5.0,
        pl.powers.first().map_or(0, |h| h.uses_game) as f32 / 5.0,
        g.hero_power_usable(p, 1) as u8 as f32,
        g.hero_power_price(p, 1).unwrap_or(0) as f32 / 5.0,
        pl.powers.get(1).map_or(0, |h| h.uses_game) as f32 / 5.0,
        g.can_dark_gift(p) as u8 as f32,
        pl.dark_gift_uses as f32 / 3.0,
        pl.trinkets.len() as f32 / 2.0,
        pl.deity.as_ref().map(|d| d.atk).unwrap_or(0) as f32 / 10.0,
        pl.deity.as_ref().map(|d| d.hp).unwrap_or(0) as f32 / 10.0,
        pl.stats.won_last as u8 as f32,
        pl.stats.lost_last as u8 as f32,
        pl.stats.spell_atk as f32 / 5.0,
        pl.stats.spell_hp as f32 / 5.0,
        pl.stats.gem_atk as f32 / 5.0,
        pl.stats.gem_hp as f32 / 5.0,
        pl.board.len() as f32 / 7.0,
        pl.hand.len() as f32 / 10.0,
        pl.deity.is_some() as u8 as f32,
        pl.hand_queue.len() as f32 / 5.0,
        pl.next_is_ghost as u8 as f32,
        pl.stats.tavern_atk as f32 / 10.0,
        pl.stats.tavern_hp as f32 / 10.0,
        pl.effects.len() as f32 / 5.0,
        // Chronum to spend in an open Timewarp visit, else the Chronum saved for the next.
        match pl.pending.last() {
            Some(Pending::Timewarp { chronum, .. }) => *chronum,
            _ => pl.chronum_saved,
        } as f32
            / 4.0,
    ]);
    for i in 0..Races::COUNT {
        f.push((g.lobby_races.0 >> i & 1) as f32);
    }

    // ---- active Quests
    for i in 0..N_QUESTS {
        match pl.quests.get(i) {
            Some(q) => f.extend([1.0, q.progress as f32 / q.goal.max(1) as f32]),
            None => f.extend([0.0; QUEST_F]),
        }
    }
    // ---- pending choice
    let kind = match pl.pending.last() {
        None => 0,
        Some(Pending::HeroSelect { .. }) => 1,
        Some(Pending::Discover { .. }) => 2,
        Some(Pending::ChooseOne { .. }) => 3,
        Some(Pending::Target { .. }) => 4,
        Some(Pending::HeroPowerChoice { .. }) => 5,
        Some(Pending::Trinket { .. }) => 6,
        Some(Pending::Quest { .. }) => 7,
        Some(Pending::Timewarp { .. }) => 8,
    };
    for i in 0..PENDING_KINDS {
        f.push((i == kind) as u8 as f32);
    }

    c.push(pl.hero.map(card_index).unwrap_or(0));
    for slot in 0..crate::actions::N_POWERS {
        c.push(pl.powers.get(slot).map(|h| card_index(h.card)).unwrap_or(0));
    }
    c.push(pl.deity.as_ref().map(|d| card_index(d.card)).unwrap_or(0));
    c.push(pending_source(g, p).map(card_index).unwrap_or(0));
    for i in 0..N_QUESTS {
        let q = pl.quests.get(i);
        c.push(q.map(|q| card_index(q.card)).unwrap_or(0));
        c.push(q.map(|q| card_index(q.reward)).unwrap_or(0));
        c.push(pl.rewards.get(i).map(|r| card_index(*r)).unwrap_or(0));
    }
    for i in 0..N_SECRETS {
        c.push(pl.secrets.get(i).map(|x| card_index(*x)).unwrap_or(0));
    }

    // ---- board
    for i in 0..N_BOARD {
        let m = pl.board.get(i);
        push_minion(&mut f, m);
        c.push(m.map(|m| card_index(m.card)).unwrap_or(0));
    }
    // ---- hand
    // A hand card is never frozen: the slot says it dies if played this turn (Tomb Turning's
    // Undead, engine `Minion::doomed_turn`; client enchantment BG34_888e).
    let doomed = |h: &crate::state::HandCard| {
        h.item
            .as_minion()
            .is_some_and(|m| m.doomed_turn != 0 && m.doomed_turn == g.turn)
    };
    for i in 0..N_HAND {
        let h = pl.hand.get(i);
        // Playing a hand card costs nothing, and the client reports 0 for a card in hand: the cost
        // float of a hand item is 0 (which the client never fed).
        push_item(
            &mut f,
            h.map(|h| &h.item),
            0,
            h.map(|h| h.locked).unwrap_or(0),
            h.map(|h| [h.temporary, h.combined, doomed(h), false, h.group != 0])
                .unwrap_or_default(),
        );
        c.push(h.map(|h| card_index(h.item.card())).unwrap_or(0));
    }
    // ---- shop (Silas Darkmoon's tickets are kept on his power)
    let silas = crate::cardid!("TB_BaconShop_HP_101");
    let tickets: &[i32] = pl.power_of(silas).map_or(&[], |h| &h.values);
    for i in 0..N_SHOP {
        let s = pl.shop.get(i);
        // The price this card would cost now (per-card overrides and discounts included).
        let cost = g.slot_price(p, i).map_or(0, |x| x.0);
        let flags = s.map_or([false; 5], |s| {
            [
                false,
                false,
                s.frozen,
                s.kept,
                tickets.contains(&(s.sid as i32)),
            ]
        });
        push_item(&mut f, s.map(|s| &s.item), cost, 0, flags);
        c.push(s.map(|s| card_index(s.item.card())).unwrap_or(0));
    }
    // ---- pending options
    let mut opts: Vec<(Option<Item>, i32, i32)> = vec![];
    match pl.pending.last() {
        Some(Pending::HeroSelect { options }) | Some(Pending::HeroPowerChoice { options, .. }) => {
            for h in options {
                opts.push((None, card_index(*h), 0));
            }
        }
        Some(Pending::Discover { options, .. }) => {
            for it in options {
                opts.push((Some(it.clone()), card_index(it.card()), 0));
            }
        }
        Some(Pending::ChooseOne { card, .. }) => {
            opts.push((None, card_index(*card), 0));
            opts.push((None, card_index(*card), 0));
        }
        Some(Pending::Target { options, .. }) => {
            for e in options {
                let found = pl
                    .board
                    .iter()
                    .find(|m| m.eid == *e)
                    .map(|m| Item::Minion(m.clone()))
                    .or_else(|| pl.shop.iter().find(|s| s.sid == *e).map(|s| s.item.clone()))
                    .or_else(|| pl.hand.iter().find(|h| h.hid == *e).map(|h| h.item.clone()));
                let ci = found.as_ref().map(|i| card_index(i.card())).unwrap_or(0);
                opts.push((found, ci, 0));
            }
        }
        Some(Pending::Trinket { options, .. }) => {
            for t in options {
                opts.push((
                    Some(Item::spell(*t)),
                    card_index(*t),
                    g.trinket_price(p, *t),
                ));
            }
        }
        Some(Pending::Quest { options, .. }) => {
            // Card slot = the reward; the quest's goal stands in for the cost.
            for (q, r) in options {
                opts.push((
                    Some(Item::spell(*r)),
                    card_index(*r),
                    crate::content::quest_goal(g.lobby_races, *q, *r),
                ));
            }
        }
        Some(Pending::Timewarp { options, .. }) => {
            // The cards with their Chronum cost, then the "leave" option (present, no card).
            for (it, cost) in options {
                opts.push((Some(it.clone()), card_index(it.card()), *cost));
            }
            opts.push((None, 0, 0));
        }
        None => {}
    }
    for i in 0..N_CHOICE {
        let o = opts.get(i);
        let item = o.and_then(|(it, _, _)| it.as_ref());
        if o.is_some() && item.is_none() {
            let mut v = vec![1.0];
            v.extend(std::iter::repeat_n(0.0, ITEM_F - 1));
            f.extend(v);
        } else {
            push_item(&mut f, item, o.map(|x| x.2).unwrap_or(0), 0, [false; 5]);
        }
        c.push(o.map(|x| x.1).unwrap_or(0));
    }
    // ---- owned trinkets
    for i in 0..8 {
        c.push(pl.trinkets.get(i).map(|t| card_index(*t)).unwrap_or(0));
    }
    // ---- opponents (public info)
    let others: Vec<PlayerId> = (1..g.players.len())
        .map(|k| (p + k) % g.players.len())
        .take(N_OPP)
        .collect();
    for i in 0..N_OPP {
        match others.get(i).map(|o| &g.players[*o]) {
            Some(o) if g.phase == Phase::HeroSelect && !all => {
                f.extend([
                    1.0,
                    o.alive as u8 as f32,
                    1.0,
                    0.0,
                    1.0 / 6.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                ]);
                c.push(0);
            }
            Some(o) => {
                f.extend([
                    1.0,
                    o.alive as u8 as f32,
                    o.health as f32 / 40.0,
                    o.armor as f32 / 20.0,
                    o.tier as f32 / 6.0,
                    g.board_shown(p, o.id, all).0.len() as f32 / 7.0,
                    o.placement as f32 / 8.0,
                    (pl.last_opponent == Some(o.id)) as u8 as f32,
                    (pl.next_opponent == Some(o.id)) as u8 as f32,
                ]);
                c.push(o.hero.map(card_index).unwrap_or(0));
            }
            None => {
                f.extend(std::iter::repeat_n(0.0, OPP_F));
                c.push(0);
            }
        }
    }
    for i in 0..N_OPP {
        // Only what this player saw in its last combat against the opponent.
        let board = others.get(i).map(|o| g.board_shown(p, *o, all).0);
        for j in 0..N_BOARD {
            let m = board.and_then(|b| b.get(j));
            push_minion(&mut f, m);
            c.push(m.map(|m| card_index(m.card)).unwrap_or(0));
        }
    }
    debug_assert_eq!(f.len(), OBS_FLOATS);
    debug_assert_eq!(c.len(), OBS_CARDS);
    Observation {
        floats: f,
        cards: c,
    }
}
