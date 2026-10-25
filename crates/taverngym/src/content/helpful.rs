//! "Helpful Refresh" (no official definition exists). One of 11 outcomes replaces or adjusts the
//! Tavern of a Refresh you make with the Refresh button:
//!
//!  1. every minion is of your most common type            (half weight)
//!  2. every minion is a random Tier 6 minion (any Tavern Tier)
//!  3. a page of 7: Boom-in-a-Box, Sin'dorei Straight Shot, Brann Bronzebeard, Titus
//!     Rivendare, Drakkari Enchanter, Leeroy the Reckless, Balinda Stonehearth
//!  4. every Tavern minion gets Divine Shield and +5/+5    (half weight)
//!  5. a random Tavern minion becomes Golden
//!  6. the minions are plain copies of your warband
//!  7. a page of 7 copies of one card                      (half weight)
//!  8. one or two copies of a minion you hold, enough to complete its triple
//!  9. an extra Tier 7 minion (not before Turn 7: official 33.6.2 note)
//! 10. a page of 7 buddies (up to your Tier)
//! 11. a page of 7 Tavern spells                           (half weight)
//!
//! Sources, checked in this order, one per Refresh: Knockoff Wisdomball (its next 2
//! Refreshes), Chromie (1, golden 2, per turn), Wondrous Wisdomball (the first Refresh after
//! getting it for sure, then 50% each).

use super::*;
use crate::state::ShopSlot;

const KNOCKOFF: &str = "BG30_802";
const WONDROUS: &str = "BG24_Reward_313";
const CHROMIE: &str = "TB_BaconShop_HERO_57_Buddy";
const PAGE: usize = 7;
const UTILITY_PAGE: [&str; 7] = [
    "BG36_620",     // Boom-in-a-Box
    "BG25_016",     // Sin'dorei Straight Shot
    "BG_LOE_077",   // Brann Bronzebeard
    "BG25_354",     // Titus Rivendare
    "BG26_ICC_901", // Drakkari Enchanter
    "BG23_318",     // Leeroy the Reckless
    "BG35_883",     // Balinda Stonehearth
];
/// (outcome, weight): 1, 4, 7 and 11 are half as likely as the others.
const OUTCOMES: [(u8, u64); 11] = [
    (1, 1),
    (2, 2),
    (3, 2),
    (4, 1),
    (5, 2),
    (6, 2),
    (7, 1),
    (8, 2),
    (9, 2),
    (10, 2),
    (11, 1),
];

/// Whether this Refresh is helpful; uses up the source that made it so.
fn take_source(c: &mut Ctx, side: Side) -> bool {
    let knockoff = cid(KNOCKOFF);
    let left = c.card_data(side, knockoff);
    if left > 0 {
        c.set_card_data(side, knockoff, left - 1);
        return true;
    }
    let chromie = cid(CHROMIE);
    if let Some(m) = c.boards[side]
        .iter_mut()
        .find(|m| db().normal_of(m.card) == chromie && m.counter > 0)
    {
        m.counter -= 1;
        return true;
    }
    if c.player_ref(side).is_some_and(|p| p.has_reward(WONDROUS)) {
        let wondrous = cid(WONDROUS);
        if c.card_data(side, wondrous) == 0 {
            c.set_card_data(side, wondrous, 1);
            return true;
        }
        return c.g.rng.below(2) == 0;
    }
    false
}

/// Remove the Tavern's minions (or every card), returning pool copies.
fn clear(c: &mut Ctx, side: Side, spells_too: bool) {
    let Some(p) = c.owner[side] else { return };
    let old = std::mem::take(&mut c.g.players[p].shop);
    for s in old {
        match s.item {
            Item::Minion(m) => c.g.pool.return_minion(&m),
            item if !spells_too => c.g.players[p].shop.push(ShopSlot::new(s.sid, item)),
            _ => {}
        }
    }
}

fn push_minion(c: &mut Ctx, side: Side, m: Minion) {
    if let Some(p) = c.owner[side] {
        let sid = m.eid;
        c.g.players[p]
            .shop
            .push(ShopSlot::new(sid, Item::Minion(m)));
    }
}

/// Put a copy of `card` into the Tavern (a pool copy when one is left).
fn add_card(c: &mut Ctx, side: Side, card: CardId) {
    let mut m = c.new_minion(side, card);
    c.take_pool_copy(&mut m);
    c.apply_tavern_buffs(side, &mut m);
    push_minion(c, side, m);
}

/// Non-golden minions you hold (board + hand) that could still triple: (card, copies held).
fn triple_candidates(c: &Ctx, side: Side) -> Vec<(CardId, usize)> {
    let mut held: Vec<CardId> = c.boards[side].iter().map(|m| m.card).collect();
    if let Some(p) = c.player_ref(side) {
        held.extend(
            p.hand
                .iter()
                .filter_map(|h| h.item.as_minion().map(|m| m.card)),
        );
    }
    let mut out: Vec<(CardId, usize)> = vec![];
    for card in held {
        let d = db().get(card);
        if d.is_golden() || d.golden.is_none() {
            continue;
        }
        match out.iter_mut().find(|x| x.0 == card) {
            Some(x) => x.1 += 1,
            None => out.push((card, 1)),
        }
    }
    out.retain(|x| x.1 < 3);
    out
}

fn feasible(c: &mut Ctx, side: Side, outcome: u8) -> bool {
    match outcome {
        1 => c.most_common_race_ex(side, true).is_some(),
        5 => c.tavern_minions(side).iter().any(|e| {
            c.find_any(*e)
                .is_some_and(|m| !m.golden && db().get(m.card).golden.is_some())
        }),
        4 => !c.tavern_minions(side).is_empty(),
        6 => !c.boards[side].is_empty(),
        8 => !triple_candidates(c, side).is_empty(),
        9 => c.g.turn >= 7,
        _ => true,
    }
}

pub fn apply(c: &mut Ctx, side: Side, outcome: u8) {
    let tier = c.tier(side);
    let slots = c.shop_minion_slots(side);
    match outcome {
        1 => {
            let Some(r) = c.most_common_race_ex(side, true) else {
                return;
            };
            clear(c, side, false);
            for _ in 0..slots {
                if let Some(m) = c.draw_pool_minion(side, 1, tier, r) {
                    push_minion(c, side, m);
                }
            }
        }
        2 => {
            clear(c, side, false);
            for _ in 0..slots {
                if let Some(m) = c.draw_pool_minion(side, 6, 6, Races::NONE) {
                    push_minion(c, side, m);
                }
            }
        }
        3 => {
            clear(c, side, true);
            for id in UTILITY_PAGE {
                add_card(c, side, cid(id));
            }
        }
        4 => {
            for e in c.tavern_minions(side) {
                c.give_kw(e, Kw::DIVINE_SHIELD);
                c.buff(e, 5, 5);
            }
        }
        5 => {
            let cands: Vec<EntityId> = c
                .tavern_minions(side)
                .into_iter()
                .filter(|e| {
                    c.find_any(*e)
                        .is_some_and(|m| !m.golden && db().get(m.card).golden.is_some())
                })
                .collect();
            if let Some(&e) = c.g.rng.pick(&cands)
                && let Some(m) = c.find_any_mut(e)
            {
                super::spells::make_golden(m);
            }
        }
        6 => {
            let cards: Vec<CardId> = c.boards[side]
                .iter()
                .map(|m| db().normal_of(m.card))
                .collect();
            clear(c, side, false);
            for card in cards {
                add_card(c, side, card);
            }
        }
        7 => {
            if let Some(card) = c.random_minion(|d| d.tier <= tier) {
                clear(c, side, true);
                for _ in 0..PAGE {
                    add_card(c, side, card);
                }
            }
        }
        8 => {
            let cands = triple_candidates(c, side);
            if let Some(&(card, held)) = c.g.rng.pick(&cands) {
                // Make room in the minion slots, then add what completes the triple.
                let need = 3 - held;
                let Some(p) = c.owner[side] else { return };
                for _ in 0..need {
                    if let Some(i) = c.g.players[p]
                        .shop
                        .iter()
                        .position(|s| s.item.as_minion().is_some_and(|m| m.card != card))
                        && let Item::Minion(m) = c.g.players[p].shop.remove(i).item
                    {
                        c.g.pool.return_minion(&m);
                    }
                }
                for _ in 0..need {
                    add_card(c, side, card);
                }
            }
        }
        9 => {
            if let Some(m) = c.draw_pool_minion(side, 7, 7, Races::NONE) {
                push_minion(c, side, m);
            }
        }
        10 => {
            let buddies = super::quests::buddies_up_to_tier(c, side);
            clear(c, side, true);
            for i in c.g.rng.sample_indices(buddies.len(), PAGE) {
                let mut m = c.new_minion(side, buddies[i]);
                c.apply_tavern_buffs(side, &mut m);
                push_minion(c, side, m);
            }
        }
        11 => {
            clear(c, side, true);
            let Some(p) = c.owner[side] else { return };
            for _ in 0..PAGE {
                if let Some(s) = c.draw_pool_spell(|d| d.tier <= tier) {
                    let sid = c.g.alloc_eid();
                    c.g.players[p].shop.push(ShopSlot::new(sid, Item::spell(s)));
                }
            }
        }
        _ => {}
    }
}

/// `Ctx::shop_fixup` of the Refresh button: runs after the normal refill and before "after
/// Refresh" effects see the Tavern.
pub fn fixup(c: &mut Ctx, side: Side) {
    if !take_source(c, side) {
        return;
    }
    let mut cands: Vec<(u8, u64)> = vec![];
    for (o, w) in OUTCOMES {
        if feasible(c, side, o) {
            cands.push((o, w));
        }
    }
    let total: u64 = cands.iter().map(|x| x.1).sum();
    let mut r = c.g.rng.next_u64() % total.max(1);
    let mut outcome = 0;
    for (o, w) in cands {
        if r < w {
            outcome = o;
            break;
        }
        r -= w;
    }
    let who = c.who(side);
    c.g.log(|| format!("{who}: helpful Refresh (outcome {outcome})"));
    apply(c, side, outcome);
}
