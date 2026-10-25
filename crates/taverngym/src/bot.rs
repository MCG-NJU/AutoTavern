//! Simple heuristic bot used for the 7 non-agent seats and for smoke tests.
//! It is deliberately cheap and deterministic given the game RNG; it is not meant to be
//! strong, only to produce sensible, varied games.

use crate::actions::{Action, N_BOARD};
use crate::cards::db;
use crate::engine::{Game, Phase};
use crate::state::{Item, Minion, Pending};
use crate::types::{Kw, PlayerId};

/// Rough value of a minion instance.
pub fn minion_value(m: &Minion) -> i32 {
    let mut v = m.atk + m.hp + 3 * m.tier as i32;
    if m.golden {
        v += 6;
    }
    for k in [
        Kw::DIVINE_SHIELD,
        Kw::TAUNT,
        Kw::REBORN,
        Kw::WINDFURY,
        Kw::VENOMOUS,
    ] {
        if m.has(k) {
            v += 3;
        }
    }
    v
}

fn item_value(i: &Item) -> i32 {
    match i {
        Item::Minion(m) => minion_value(m),
        Item::Spell { card, .. } => 2 + db().get(*card).tier as i32,
    }
}

/// Choose one action for `p` (always legal when `game.needs_action(p)`).
pub fn choose_action(g: &mut Game, p: PlayerId) -> Action {
    if let Some(top) = g.players[p].pending.last().cloned() {
        return Action::Choose(pick_option(g, p, &top));
    }
    let pl = &g.players[p];
    if g.phase != Phase::Recruit {
        return Action::EndTurn;
    }
    let mask = g.legal_mask(p);
    let legal = |a: Action| mask[a.encode()];
    // Hard stop against pathological loops.
    if pl.actions_this_turn > 60 {
        return Action::EndTurn;
    }

    // 2. Play cards from hand.
    for (h, card) in pl.hand.iter().enumerate().take(crate::actions::N_HAND) {
        match &card.item {
            Item::Minion(m) => {
                if m.has(Kw::MAGNETIC)
                    && let Some(b) = (0..pl.board.len().min(N_BOARD))
                        .find(|b| legal(Action::Magnetize { hand: h, board: *b }))
                {
                    return Action::Magnetize { hand: h, board: b };
                }
                let pos = pl.board.len();
                if pos < 8 && legal(Action::Play { hand: h, pos }) {
                    return Action::Play { hand: h, pos };
                }
            }
            Item::Spell { .. } => {
                if legal(Action::Play { hand: h, pos: 0 }) {
                    return Action::Play { hand: h, pos: 0 };
                }
            }
        }
    }
    // 3. Upgrade on a loose curve.
    let price = g.upgrade_price(p);
    if legal(Action::Upgrade)
        && (price <= 3 || (g.turn as i32 > 2 * pl.tier as i32 && pl.gold >= price))
    {
        return Action::Upgrade;
    }
    // 4. Sell the weakest minion if the board is full and hand/shop have better.
    let board_full = pl.board.len() >= g.cfg.board_size;
    let best_shop = pl
        .shop
        .iter()
        .enumerate()
        .filter(|(i, _)| legal(Action::Buy(*i)))
        .max_by_key(|(_, s)| item_value(&s.item) + if pair_bonus(g, p, &s.item) { 15 } else { 0 });
    if board_full {
        let weakest = pl
            .board
            .iter()
            .enumerate()
            .min_by_key(|(_, m)| minion_value(m));
        if let (Some((wi, wm)), Some((_, s))) = (weakest, best_shop)
            && item_value(&s.item) > minion_value(wm) + 4
            && pl.gold >= g.minion_price(p)
        {
            return Action::Sell(wi);
        }
    }
    // 5. Buy the best affordable card (minions only if there will be room).
    let minions_owned = pl.board.len()
        + pl.hand
            .iter()
            .filter(|h| matches!(h.item, Item::Minion(_)))
            .count();
    if let Some((i, s)) = best_shop {
        let is_minion = matches!(s.item, Item::Minion(_));
        if !is_minion || minions_owned < g.cfg.board_size || pair_bonus(g, p, &s.item) {
            return Action::Buy(i);
        }
    }
    // 6. Hero power, Dark Gift.
    for slot in 0..crate::actions::N_POWERS {
        if legal(Action::HeroPower(slot)) && pl.gold >= 1 {
            return Action::HeroPower(slot);
        }
    }
    if legal(Action::DarkGift) && minions_owned < g.cfg.board_size {
        return Action::DarkGift;
    }
    // 7. Activate minions.
    for i in 0..pl.board.len().min(N_BOARD) {
        if legal(Action::Activate(i)) {
            return Action::Activate(i);
        }
    }
    // 8. Refresh with spare gold (bounded).
    if legal(Action::Refresh)
        && pl.gold >= 3
        && pl.stats.refreshes < 30 * g.turn as i32
        && pl.actions_this_turn < 40
    {
        return Action::Refresh;
    }
    Action::EndTurn
}

fn pair_bonus(g: &Game, p: PlayerId, item: &Item) -> bool {
    let Item::Minion(m) = item else { return false };
    if m.golden {
        return false;
    }
    let pl = &g.players[p];
    let n = pl.board.iter().filter(|x| x.card == m.card).count()
        + pl.hand.iter().filter(|h| h.item.card() == m.card).count();
    n >= 1
}

fn pick_option(g: &mut Game, p: PlayerId, top: &Pending) -> usize {
    match top {
        // First affordable trinket.
        Pending::Trinket { options, .. } => (0..options.len())
            .find(|i| g.option_legal(p, *i))
            .unwrap_or(0),
        Pending::HeroSelect { options } => g.rng.below(options.len().max(1)),
        Pending::Quest { options, .. } => g.rng.below(options.len().max(1)),
        // The dearest affordable card; leave when nothing is affordable.
        Pending::Timewarp {
            options, chronum, ..
        } => options
            .iter()
            .enumerate()
            .filter(|(_, o)| o.1 <= *chronum)
            .max_by_key(|(_, o)| o.1)
            .map_or(options.len(), |(i, _)| i),
        Pending::Discover { options, .. } => options
            .iter()
            .enumerate()
            .max_by_key(|(_, it)| item_value(it))
            .map(|(i, _)| i)
            .unwrap_or(0),
        Pending::ChooseOne { .. } => g.rng.below(2),
        Pending::Target { options, .. } => {
            // Prefer our own strongest minion.
            let pl = &g.players[p];
            options
                .iter()
                .enumerate()
                .max_by_key(|(_, e)| {
                    pl.board
                        .iter()
                        .find(|m| m.eid == **e)
                        .map(|m| 100 + minion_value(m))
                        .unwrap_or(0)
                })
                .map(|(i, _)| i)
                .unwrap_or(0)
        }
        Pending::HeroPowerChoice { options, .. } => g.rng.below(options.len().max(1)),
    }
}

/// Let a bot act until it no longer needs to (ends its turn or the game ends). Returns
/// whether it took any action: false means the seat needs an action and none is accepted.
pub fn play_turn(g: &mut Game, p: PlayerId) -> bool {
    let mut guard = 0;
    let mut acted = false;
    let t0 = g.turn;
    while g.needs_action(p) && guard < 200 {
        guard += 1;
        if g.auto_claim_buddy(p) {
            continue;
        }
        let a = choose_action(g, p);
        if g.apply(p, a).is_err() {
            // Fall back to ending the turn / first option.
            let fallback = if g.players[p].pending.is_empty() {
                Action::EndTurn
            } else {
                Action::Choose(0)
            };
            if g.apply(p, fallback).is_err() {
                break;
            }
        }
        acted = true;
        // Ending the turn may trigger combat and a new turn; stop after our turn ended.
        if g.turn != t0 || g.players[p].ended_turn || !g.players[p].alive {
            break;
        }
    }
    acted
}

/// Run bots until game over or an explicit simulator-driver failure. Placements are
/// final only for a game-rule/turn-cap terminal result, never a diagnostic stop.
pub fn run_bot_game(seed: u64, cfg: crate::config::RulesConfig) -> Game {
    let mut g = Game::new(seed, cfg);
    let mut rounds = 0;
    while !g.is_over() && g.stopped_reason.is_none() && rounds < 10_000 {
        rounds += 1;
        let mut progressed = false;
        for p in 0..g.players.len() {
            if g.needs_action(p) {
                progressed |= play_turn(&mut g, p);
            }
        }
        // Nobody could act (a seat that needs an action has no legal one): the game is
        // returned unfinished at once instead of after ten thousand empty rounds.
        if !progressed && !g.is_over() {
            g.stopped_reason = Some(crate::engine::StoppedReason::NoProgress);
            g.log(|| "stalled: a seat needs an action and none is legal".into());
            break;
        }
    }
    if !g.is_over() && g.stopped_reason.is_none() {
        g.stopped_reason = Some(crate::engine::StoppedReason::DriverGuard);
    }
    g
}
