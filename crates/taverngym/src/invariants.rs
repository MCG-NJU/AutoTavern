//! State invariants for fuzzing and tests. Not used on the RL hot path.

use crate::cards::db;
use crate::engine::Game;
use crate::state::Item;

/// Structural checks that must hold between any two actions. Returns the violations.
pub fn check(g: &Game) -> Vec<String> {
    let mut bad = vec![];
    let mut eids = std::collections::HashSet::new();
    for p in &g.players {
        let who = p.id;
        if p.board.len() > g.cfg.board_size {
            bad.push(format!("p{who}: {} minions on the board", p.board.len()));
        }
        let hand_limit = if g.cfg.experimental_mode {
            g.cfg.hand_size.max(p.timewarp_hand_limit)
        } else {
            g.cfg.hand_size
        };
        if p.hand.len() > hand_limit {
            bad.push(format!("p{who}: {} cards in hand", p.hand.len()));
        }
        // Everything the player can act on fits the action space and the observation.
        use crate::actions::{N_CHOICE, N_POWERS, N_SHOP};
        use crate::obs::{N_QUESTS, N_SECRETS};
        let options = p.pending.last().map_or(0, |x| x.len());
        for (what, n, max) in [
            (
                "Tavern cards",
                p.shop.len(),
                g.cfg.shop_max_cards.min(N_SHOP),
            ),
            ("options", options, N_CHOICE),
            ("hero powers", p.powers.len(), N_POWERS),
            (
                "quests",
                p.quests.len(),
                if g.cfg.experimental_mode {
                    usize::MAX
                } else {
                    N_QUESTS
                },
            ),
            ("secrets", p.secrets.len(), N_SECRETS),
            ("trinkets", p.trinkets.len(), 8),
        ] {
            if n > max {
                bad.push(format!("p{who}: {n} {what} (the interface shows {max})"));
            }
        }
        if !(1..=7).contains(&p.tier) {
            bad.push(format!("p{who}: tier {}", p.tier));
        }
        if p.gold < 0 {
            bad.push(format!("p{who}: gold {}", p.gold));
        }
        if p.alive && p.health <= 0 {
            bad.push(format!("p{who}: alive with {} Health", p.health));
        }
        let hand = p.hand.iter().filter_map(|h| h.item.as_minion());
        let shop = p.shop.iter().filter_map(|s| s.item.as_minion());
        for (zone, m) in p
            .board
            .iter()
            .map(|m| ("board", m))
            .chain(hand.map(|m| ("hand", m)))
            .chain(shop.map(|m| ("tavern", m)))
        {
            if !eids.insert(m.eid) {
                bad.push(format!("p{who}: duplicate eid {} ({})", m.eid, m.name()));
            }
            if m.atk < 0 || m.max_hp <= 0 || m.hp > m.max_hp {
                bad.push(format!(
                    "p{who} {zone}: {} is {}/{} (max {})",
                    m.name(),
                    m.atk,
                    m.hp,
                    m.max_hp
                ));
            }
            if zone == "board" && m.hp <= 0 {
                bad.push(format!("p{who}: dead {} on the board", m.name()));
            }
        }
    }
    let mut places: Vec<u8> = g
        .players
        .iter()
        .filter(|p| !p.alive)
        .map(|p| p.placement)
        .collect();
    places.sort_unstable();
    if places.windows(2).any(|w| w[0] == w[1]) {
        bad.push(format!("duplicate placements {places:?}"));
    }
    bad
}

/// Pool copies: (copies in the pool and in every living player's zones, copies the lobby
/// started with). The two are equal whenever no choice is pending (options of an open
/// Discover hold copies outside these zones).
pub fn pool_balance(g: &Game) -> (u64, u64) {
    let initial: u64 = g
        .universe
        .iter()
        .map(|c| g.cfg.pool_copies_by_tier[(db().get(*c).tier as usize - 1).min(6)] as u64)
        .sum();
    let mut held: u64 = g.pool.counts.iter().map(|n| *n as u64).sum();
    let copies = |i: &Item| i.as_minion().map_or(0, |m| m.pool_copies as u64);
    for p in g.players.iter().filter(|p| p.alive) {
        held += p.board.iter().map(|m| m.pool_copies as u64).sum::<u64>();
        held += p.hand.iter().map(|h| copies(&h.item)).sum::<u64>();
        held += p.shop.iter().map(|s| copies(&s.item)).sum::<u64>();
        held += p.hand_queue.iter().map(copies).sum::<u64>();
    }
    (held, initial)
}

/// Per-species conservation catches mixed triples returning copies to the wrong pool,
/// which the total-only check cannot detect. Call between choices, as for pool_balance.
pub fn pool_species_violations(g: &Game) -> Vec<String> {
    let mut held: Vec<u64> = g.pool.counts.iter().map(|n| *n as u64).collect();
    for p in g.players.iter().filter(|p| p.alive) {
        for m in p
            .board
            .iter()
            .chain(p.hand.iter().filter_map(|h| h.item.as_minion()))
            .chain(p.shop.iter().filter_map(|s| s.item.as_minion()))
            .chain(p.hand_queue.iter().filter_map(Item::as_minion))
        {
            m.visit_pool_parts(|c, n| held[c.idx()] += n as u64);
        }
    }
    g.universe
        .iter()
        .filter_map(|c| {
            let initial = g.cfg.pool_copies_by_tier[(db().get(*c).tier as usize - 1).min(6)] as u64;
            (held[c.idx()] != initial)
                .then(|| format!("{}: pool {} != {initial}", db().get(*c).id, held[c.idx()]))
        })
        .collect()
}
