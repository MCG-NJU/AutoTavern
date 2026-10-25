//! The Timewarped Tavern (34.2 system; official 34.2 notes and the Developer Insight of
//! 2025-12-01, forum topic 154893).
//!
//! A visit is a `Pending::Timewarp`: five cards bought with Chronum instead of Gold. It is
//! opened at the start of a turn, before the normal recruit phase: by the global system on
//! turns 6 (Minor) and 9 (Major) when `RulesConfig::timewarp_enabled`, and by Morchie
//! (turn 5, Minor) and Murozond (turn 8, Major) through their hero powers.
//!
//! Offer rules implemented: 3 minions + 2 non-minions; three cards costing 1 and two costing 2 or
//! more; the Minor Timewarp includes a second Hero Power while the player has only one; typed cards
//! (`CardDef::tw_types`) need their type in the lobby and the player "in" it (2 minions for Minor,
//! 3 for Major); two of the three minions are tied to the most common type of the warband and hand;
//! at most one card of each other type; the card conditions listed in the Developer Insight.
//! Unspent Chronum is lost; only Timewarped Investment (offered in the Minor Timewarp of the global
//! system) carries one Chronum over to the Major Timewarp.

use super::*;
use crate::cards::{CardDef, CardKind};
use crate::state::Pending;

/// Timewarped Investment ("gain 1 extra Chronum at your next Timewarp").
const INVESTMENT: &str = "BG34_Treasure_300";

/// Timewarp "Power of X" spells (a second Hero Power; Minor Timewarp only).
fn is_power(d: &CardDef) -> bool {
    d.id.starts_with("BG34_HeroPowerSpell_")
}

/// Card-specific offer conditions (Developer Insight).
/// Not implemented on purpose: "Jungle King / Tamuzo only if you summoned a minion in the previous
/// combat" -- they follow the Beast type rule alone.
fn card_ok(c: &Ctx, side: Side, d: &CardDef) -> bool {
    let Some(pl) = c.player_ref(side) else {
        return false;
    };
    let board = &c.boards[side];
    let hero = pl.hero.map(|h| db().get(h).id.as_str()).unwrap_or("");
    let has = |f: fn(&Minion) -> bool| board.iter().any(f);
    match d.id.as_str() {
        // Timewarped Skipper: not for heroes whose minions sell or cost differently.
        "BG34_Giant_072" => ![
            "TB_BaconShop_HERO_10",
            "TB_BaconShop_HERO_49",
            "TB_BaconShop_HERO_27",
            "TB_BaconShop_HERO_55",
            "BG26_HERO_101",
            "TB_BaconShop_HERO_60",
        ]
        .contains(&hero),
        // Timewarped Lei: the hero needs a Buddy.
        "BG34_Giant_602" => c.owner[side]
            .and_then(|p| super::buddies_c::power_buddy(c, p))
            .is_some(),
        // Timewarped Saurolisk: not for N'Zoth.
        "BG34_Giant_202" => hero != "TB_BaconShop_HERO_93",
        // Timewarped Whirl-O-Tron: needs a Deathrattle minion.
        "BG34_Giant_599" => has(|m| crate::script::minion_has(m, crate::script::Hook::Deathrattle)),
        // Timewarped Murk-Eye: needs a Battlecry minion.
        "BG34_Giant_318" => has(|m| crate::script::minion_has(m, crate::script::Hook::Battlecry)),
        // Cards about "a friendly minion of each type" / "minions of different types": the
        // player must be "in" menagerie, three minions of different types (Developer
        // Insight; the cards carry no tag for it, so they are listed here).
        "BG34_Giant_679" | "BG34_Giant_205" | "BG34_Giant_684" => {
            let races: Vec<Races> = board
                .iter()
                .map(|m| m.races)
                .chain(c.hand_minions(side).into_iter().map(|x| x.2))
                .collect();
            distinct_types_of(&races) >= 3
        }
        _ => true,
    }
}

/// The cards a visit could offer this player, and the player's most common minion type
/// (warband and hand; ties random; `Races::NONE` without typed minions).
fn eligible(c: &mut Ctx, side: Side, major: bool) -> (Vec<&'static CardDef>, Races) {
    let Some(p) = c.owner[side] else {
        return (vec![], Races::NONE);
    };
    let tier = if major { 5 } else { 3 };
    let need = c.g.cfg.timewarp_type_min[usize::from(major)];
    let lobby = c.g.lobby_races;
    let all = c.g.cfg.content == crate::config::ContentMode::AllVanilla;
    // Minions of each type on board and in hand (all-type minions count for every type).
    let races: Vec<Races> = c.boards[side]
        .iter()
        .map(|m| m.races)
        .chain(c.hand_minions(side).into_iter().map(|x| x.2))
        .collect();
    let counts: Vec<(Races, usize)> = Races::ALL
        .iter()
        .map(|t| (t, races.iter().filter(|r| r.intersects(t)).count()))
        .collect();
    let mut in_types = counts
        .iter()
        .filter(|x| x.1 >= need)
        .fold(Races::NONE, |a, x| a.union(x.0));
    // Heroes tied to a type can always be offered its cards.
    if let Some(h) = c.g.players[p].hero {
        in_types = in_types.union(db().get(h).assoc);
    }
    // The most common type in the warband and hand (ties random), whatever its count: two of the
    // three minions are tied to it.
    // (Only types of this lobby: an all-type minion counts for every type.)
    let in_lobby = |x: &&(Races, usize)| x.0.intersects(lobby);
    let top = counts
        .iter()
        .filter(in_lobby)
        .map(|x| x.1)
        .filter(|n| *n > 0)
        .max();
    let tied: Vec<Races> = counts
        .iter()
        .filter(in_lobby)
        .filter(|x| Some(x.1) == top)
        .map(|x| x.0)
        .collect();
    let most = c.g.rng.pick(&tied).copied().unwrap_or(Races::NONE);
    in_types = in_types.union(most);
    let warband = c.boards[side].len();
    let powers = c.g.players[p].powers.len();
    let owned: Vec<CardId> = c.g.players[p].power_cards().collect();

    let cands: Vec<&CardDef> = db()
        .iter()
        .filter(|d| d.pools.timewarp && d.normal.is_none() && d.tier == tier)
        .filter(|d| all || script(d.cid).is_some())
        .filter(|d| {
            // One and the same type must be in the lobby and one the player is in.
            d.tw_types.is_none() || d.tw_types.intersects(Races(lobby.0 & in_types.0))
        })
        .filter(|d| warband >= d.tw_min_minions as usize)
        .filter(|d| {
            if !is_power(d) {
                return true;
            }
            // A second Hero Power: only while the player has one, and not their own.
            let name = d.text.split('\'').nth(1).unwrap_or_default();
            let in_lobby = hero_power_pool(c.g);
            powers < 2
                && hero_power_named(name)
                    .is_some_and(|hp| !owned.contains(&hp) && in_lobby.contains(&hp))
        })
        // Timewarped Investment saves a Chronum for the Major Timewarp: only with the global
        // system, in the Minor one.
        .filter(|d| d.id != INVESTMENT || (c.g.cfg.timewarp_enabled && !major))
        .filter(|d| card_ok(c, side, d))
        .collect();
    (cands, most)
}

/// Minions for a Discover from the Timewarp ("Discover a 2-Cost minion from the Minor Timewarp",
/// Timeworn Candelabra): `n` different minions costing `cost` Chronum, chosen by the visit's own
/// rules -- only cards this player could be offered there, two of them tied to the most common type
/// when there are such cards, at most one of each other type. No source describes this Discover
/// itself; the simulator follows the visit's rules.
pub fn discover_minions(c: &mut Ctx, side: Side, major: bool, cost: i32, n: usize) -> Vec<CardId> {
    discover_cards(c, side, major, cost, n, true)
}

/// As `discover_minions`, for "Discover a 1-Cost card from the Major Timewarp" (Timewarped
/// Beanstalk): spells too unless `minions_only`. Second Hero Powers are never Discovered.
pub fn discover_cards(
    c: &mut Ctx,
    side: Side,
    major: bool,
    cost: i32,
    n: usize,
    minions_only: bool,
) -> Vec<CardId> {
    let (cands, most) = eligible(c, side, major);
    let cands: Vec<&CardDef> = cands
        .into_iter()
        .filter(|d| d.cost == cost && !is_power(d))
        .filter(|d| d.kind == CardKind::Minion || !minions_only)
        .collect();
    let mut picked: Vec<&CardDef> = vec![];
    let mut used = Races::NONE;
    while picked.len() < n {
        let lead = picked.len() < 2 && !most.is_none();
        let pool = |strict: bool| -> Vec<(CardId, u64)> {
            cands
                .iter()
                .filter(|d| !picked.iter().any(|x| x.cid == d.cid))
                .filter(|d| {
                    if strict {
                        d.tw_types.intersects(most)
                    } else {
                        d.tw_types.intersects(most) || !d.tw_types.intersects(used)
                    }
                })
                .map(|d| (d.cid, d.tw_weight as u64))
                .collect()
        };
        let mut ws = if lead { pool(true) } else { vec![] };
        if ws.is_empty() {
            ws = pool(false);
        }
        let Some(&cid) = c.weighted_distinct(ws, 1).first() else {
            break;
        };
        let d = db().get(cid);
        if !d.tw_types.intersects(most) {
            used = used.union(d.tw_types);
        }
        picked.push(d);
    }
    picked.into_iter().map(|d| d.cid).collect()
}

/// Build the offer: (card, Chronum cost), arranged by cost.
pub fn offer(c: &mut Ctx, side: Side, major: bool) -> Vec<(Item, i32)> {
    if c.owner[side].is_none() {
        return vec![];
    }
    let (cands, most) = eligible(c, side, major);
    let experimental = c.g.cfg.experimental_mode;

    // Buckets: (minion?, costs 1?).
    let bucket = |d: &CardDef| (d.kind == CardKind::Minion, d.cost <= 1);
    let weight = |d: &CardDef| -> u64 { d.tw_weight as u64 };
    let draw = |c: &mut Ctx, pool: Vec<&'static CardDef>| -> Option<&'static CardDef> {
        let ws: Vec<(CardId, u64)> = pool.iter().map(|d| (d.cid, weight(d))).collect();
        c.weighted_distinct(ws, 1).first().map(|x| db().get(*x))
    };
    // Two minions of the most common type come first (as many as there are): the shape of
    // the offer is then chosen around their costs.
    let mut lead: Vec<&'static CardDef> = vec![];
    for _ in 0..2 {
        if most.is_none() {
            break;
        }
        let pool: Vec<&CardDef> = cands
            .iter()
            .copied()
            .filter(|d| d.kind == CardKind::Minion && d.tw_types.intersects(most))
            .filter(|d| !lead.iter().any(|x| x.cid == d.cid))
            .filter(|d| !experimental || !is_starcraft(d) || !lead.iter().any(|x| is_starcraft(x)))
            .collect();
        let Some(d) = draw(c, pool) else { break };
        lead.push(d);
    }
    let cheap_lead = lead.iter().filter(|d| bucket(d).1).count();
    // (cheap minions, cheap non-minions): 3 minions + 2 non-minions, 3 cheap + 2 dear.
    let mut shapes = [(3usize, 0usize), (2, 1), (1, 2)];
    c.g.rng.shuffle(&mut shapes);
    let mut best: Vec<&CardDef> = vec![];
    let needs_power = experimental && !major && cands.iter().any(|d| is_power(d));
    for (m1, s1) in shapes {
        if needs_power && s1 == 0 {
            continue;
        }
        if cheap_lead > m1 || lead.len() - cheap_lead > 3 - m1 {
            continue; // the leading minions don't fit this shape
        }
        let mut room = [[2 - s1, s1], [3 - m1, m1]]; // [minion?][cheap?]
        let mut picked: Vec<&CardDef> = vec![];
        let mut used = Races::NONE;
        let take = |d: &'static CardDef,
                    room: &mut [[usize; 2]; 2],
                    picked: &mut Vec<&CardDef>,
                    used: &mut Races| {
            let (m, cheap) = bucket(d);
            room[usize::from(m)][usize::from(cheap)] -= 1;
            if !d.tw_types.intersects(most) {
                *used = used.union(d.tw_types);
            }
            picked.push(d);
        };
        let fits = |d: &CardDef, room: &[[usize; 2]; 2], picked: &[&CardDef], used: Races| {
            let (m, cheap) = bucket(d);
            room[usize::from(m)][usize::from(cheap)] > 0
                && !picked.iter().any(|x| x.cid == d.cid)
                && (!experimental || !is_starcraft(d) || !picked.iter().any(|x| is_starcraft(x)))
                // At most one card of a type that isn't the most common one.
                && (d.tw_types.intersects(most) || !d.tw_types.intersects(used))
        };
        // 1. The two minions of the most common type.
        for d in lead.iter().copied() {
            take(d, &mut room, &mut picked, &mut used);
        }
        // 2. The second Hero Power of the Minor Timewarp.
        if !major {
            let pool: Vec<&CardDef> = cands
                .iter()
                .copied()
                .filter(|d| is_power(d) && fits(d, &room, &picked, used))
                .collect();
            if let Some(d) = draw(c, pool) {
                take(d, &mut room, &mut picked, &mut used);
            }
        }
        // 3. The rest (no further Hero Power).
        loop {
            let pool: Vec<&CardDef> = cands
                .iter()
                .copied()
                .filter(|d| !is_power(d) && fits(d, &room, &picked, used))
                .collect();
            let Some(d) = draw(c, pool) else { break };
            take(d, &mut room, &mut picked, &mut used);
        }
        if picked.len() > best.len() {
            best = picked;
        }
        if best.len() == 5 {
            break;
        }
    }
    // (When no shape can be filled completely, the fullest one is offered.)
    best.sort_by_key(|d| (d.cost, d.cid));
    best.into_iter()
        .map(|d| {
            let item = if d.kind == CardKind::Minion {
                Item::Minion(c.new_minion(side, d.cid))
            } else {
                Item::spell(d.cid)
            };
            (item, d.cost)
        })
        .collect()
}

/// Visit the Timewarped Tavern: the choice opens on top of the player's pending stack.
pub fn open(c: &mut Ctx, side: Side, major: bool) {
    let Some(p) = c.owner[side] else { return };
    let options = offer(c, side, major);
    if options.is_empty() {
        return;
    }
    let saved = std::mem::take(&mut c.g.players[p].chronum_saved);
    let chronum = if c.g.cfg.experimental_mode {
        let pl = &mut c.g.players[p];
        if pl.chronum_granted_turn != c.g.turn {
            pl.chronum_balance += c.g.cfg.timewarp_chronum;
            pl.chronum_granted_turn = c.g.turn;
        }
        pl.chronum_balance += saved;
        pl.chronum_balance
    } else {
        c.g.cfg.timewarp_chronum + saved
    };
    let kind = if major { "Major" } else { "Minor" };
    c.g.log(|| format!("p{p} visits the {kind} Timewarp with {chronum} Chronum"));
    c.g.players[p].pending.push(Pending::Timewarp {
        options,
        chronum,
        major,
    });
}

/// A card bought in the Timewarped Tavern. "Casts When Bought" spells are cast; anything
/// else goes to the hand, waiting for space if it is full (the real hand limit grows
/// temporarily). The hand, board and Hero Power "do not function" there: no buy triggers.
pub fn bought(c: &mut Ctx, side: Side, item: Item) {
    // The purchase itself stays inside the visit even if the final choice popped its menu.
    let old = std::mem::replace(&mut c.timewarp_purchase[side], true);
    bought_inner(c, side, item);
    // Also consume direct stat writes before the final purchase resumes reactions.
    c.mishmash_sync(side);
    c.timewarp_purchase[side] = old;
}

fn bought_inner(c: &mut Ctx, side: Side, item: Item) {
    let Some(p) = c.owner[side] else { return };
    let card = item.card();
    let name = &db().get(card).name;
    c.g.log(|| format!("p{p} takes {name} from the Timewarp"));
    if !matches!(item, Item::Minion(_))
        && script(card).is_some_and(|s| s.has_flag(flag::CAST_ON_BUY))
    {
        c.cast_spell(side, card, 0, 0, 0);
        return;
    }
    if c.g.cfg.experimental_mode {
        c.add_timewarp_purchase(side, item);
    } else if c.g.players[p].hand.len() >= c.g.cfg.hand_size {
        c.g.players[p].hand_queue.push(item);
    } else {
        c.add_to_hand(side, item);
    }
}

/// The StarCraft-themed assets; Ultralisk is currently outside the enabled offer pool.
fn is_starcraft(d: &CardDef) -> bool {
    matches!(
        d.id.as_str(),
        "BG34_Giant_597" | "BG34_Giant_598" | "BG34_Treasure_994"
    )
}
