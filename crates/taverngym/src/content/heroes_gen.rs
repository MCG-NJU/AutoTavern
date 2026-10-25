//! Heroes: counters, generation, copies and opponent information.

use super::heroes::shown_left;
use super::*;

fn passive(s: Script) -> Script {
    Script {
        flags: s.flags | flag::PASSIVE,
        hero_power_uses: 0,
        ..s
    }
}

fn active(spec: TargetSpec, f: crate::script::FxT) -> Script {
    Script {
        hero_power: Some(f),
        hero_power_target: spec,
        ..E
    }
}

fn tick(c: &mut Ctx, me: Me, n: i32, every: i32) -> i32 {
    let Some(h) = c.power_mut(me.side, me.card) else {
        return 0;
    };
    h.counter = h.counter.saturating_add(n);
    let k = h.counter / every;
    h.counter %= every;
    k
}

/// Silas Darkmoon: the Tavern cards carrying a Darkmoon Ticket are kept in the power's `values`
/// (Tavern ids), not on the cards. After each Refresh every new card (spells too) gets one with
/// `silas_ticket_pct` (30%), and one Refresh in twenty puts a ticket on every new card instead.
/// Cards kept by a Freeze are not rolled again: the ones with a ticket keep it, the ones without
/// stay without.
const SILAS_FULL_PAGE_PCT: usize = 5;

fn mark_tickets(c: &mut Ctx, me: Me) {
    let Some(p) = c.owner[me.side] else { return };
    let shop: Vec<(i32, bool)> = c.g.players[p]
        .shop
        .iter()
        .map(|s| (s.sid as i32, s.kept))
        .collect();
    let pct = c.g.cfg.silas_ticket_pct as usize;
    let Some(h) = c.power_mut(me.side, me.card) else {
        return;
    };
    h.values.retain(|x| shop.iter().any(|s| s.0 == *x));
    let old = h.values.clone();
    let full = pct > 0 && c.g.rng.below(100) < SILAS_FULL_PAGE_PCT;
    let mut new = vec![];
    for (sid, kept) in shop {
        if !kept && !old.contains(&sid) && (full || c.g.rng.below(100) < pct) {
            new.push(sid);
        }
    }
    if let Some(h) = c.power_mut(me.side, me.card) {
        h.values.extend(new);
    }
}

/// Zephrys: the non-Golden minions you have two copies of (board and hand).
pub fn pairs_of(board: &[Minion], hand: &[crate::state::HandCard]) -> Vec<CardId> {
    let mut counts: Vec<(CardId, u32)> = vec![];
    let cards = board
        .iter()
        .chain(hand.iter().filter_map(|h| h.item.as_minion()))
        .filter(|m| !m.golden)
        .map(|m| m.card);
    for card in cards {
        match counts.iter_mut().find(|x| x.0 == card) {
            Some(x) => x.1 += 1,
            None => counts.push((card, 1)),
        }
    }
    counts
        .into_iter()
        .filter(|x| x.1 >= 2)
        .map(|x| x.0)
        .collect()
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Rock Master Voone: every 3 turns (end of turn), get a plain copy of the left-most
        // card in your hand.
        (
            "BG26_HERO_104p",
            passive(Script {
                end_of_turn: Some(|c, me| {
                    if !c.turn_cycle(me.side, me.card, |c| tick(c, me, 1, 3) > 0) {
                        return;
                    }
                    // A plain copy reads even a locked original, and arrives unlocked.
                    let first = c
                        .player_ref(me.side)
                        .and_then(|p| p.hand.first())
                        .map(|h| h.item.clone());
                    match first {
                        // A Triple Reward's Tier is the card itself, not an enchantment: its
                        // copy Discovers from the same Tier (a bare one would have no Tier).
                        Some(Item::Spell { card, data })
                            if card == cid(crate::recruit::TRIPLE_REWARD) =>
                        {
                            c.add_to_hand(me.side, Item::Spell { card, data });
                        }
                        Some(item) => {
                            c.give_card(me.side, db().normal_of(item.card()));
                        }
                        None => {}
                    }
                }),
                // "At the end of every 3 turns, get a plain copy of the left- most card in your
                // hand. (3 turns left!)"
                shown: Some(|c, sh| vec![3, shown_left(c, sh, 3)]),
                ..E
            }),
        ),
        // Lord Jaraxxus: after friendly minions deal 150 damage, the power becomes Nether Portal.
        // Every point a friendly minion deals counts: attacking or struck back when attacked, also
        // when it dies in the same exchange or deals the damage with its Deathrattle, and the
        // damage its effects deal to your own hero (Wrath Weaver, Imposing Percussionist, a Refresh
        // paid with Health through Malchezaar), rewound or not.
        (
            "TB_BaconShop_HP_036",
            passive(Script {
                on_event: Some(|c, me, e| {
                    let amount = match e {
                        Event::Damaged { source, amount, .. }
                            if c.locate(*source).is_some_and(|x| x.0 == me.side)
                                || c.dead_record(me.side, *source).is_some() =>
                        {
                            *amount
                        }
                        Event::HeroDamaged { side, amount, .. }
                            if *side == me.side
                                && c.hero_hit_by.is_some_and(|x| {
                                    db().get(x).kind == crate::cards::CardKind::Minion
                                }) =>
                        {
                            *amount
                        }
                        _ => return,
                    };
                    if amount <= 0 {
                        return;
                    }
                    if tick(c, me, amount, 150) > 0 {
                        let slot = c
                            .player_ref(me.side)
                            .and_then(|p| p.powers.iter().position(|h| h.card == me.card))
                            .unwrap_or(0);
                        c.set_hero_power(me.side, slot, cid("TB_BaconShop_HP_036t"));
                    }
                }),
                listens: ev::DAMAGED | ev::HERO_DAMAGED,
                // "After friendly minions deal 150 damage, open a portal to the Twisting
                // Nether! (150 left!)"
                shown: Some(|c, sh| vec![150, shown_left(c, sh, 150)]),
                ..E
            }),
        ),
        // Nether Portal: at the start of each turn, get 2 random Demons.
        (
            "TB_BaconShop_HP_036t",
            passive(Script {
                start_of_turn: Some(|c, me| {
                    let tier = c.tier(me.side);
                    for _ in 0..2 {
                        if let Some(d) =
                            c.random_minion(|d| d.races.intersects(Races::DEMON) && d.tier <= tier)
                        {
                            c.give_card(me.side, d);
                        }
                    }
                }),
                ..E
            }),
        ),
        // Dancin' Deryl: played minions get a +1/+1 hat that passes to a friendly minion when
        // sold (values = eids wearing hats, one entry per hat).
        (
            "TB_BaconShop_HP_042",
            passive(Script {
                on_event: Some(|c, me, e| match e {
                    Event::Played { side, eid, .. } if *side == me.side => {
                        c.buff(*eid, 1, 1);
                        if let Some(h) = c.power_mut(me.side, me.card) {
                            h.values.push(*eid as i32);
                        }
                    }
                    Event::Sold { side, eid, .. } if *side == me.side => {
                        let Some(h) = c.power_mut(me.side, me.card) else {
                            return;
                        };
                        let hats = h.values.iter().filter(|x| **x == *eid as i32).count();
                        h.values.retain(|x| *x != *eid as i32);
                        for _ in 0..hats {
                            if let Some(t) = c.random_friendly(me.side, |_| true) {
                                c.buff(t, 1, 1);
                                if let Some(h) = c.power_mut(me.side, me.card) {
                                    h.values.push(t as i32);
                                }
                            }
                        }
                    }
                    _ => {}
                }),
                listens: ev::PLAYED | ev::SOLD,
                ..E
            }),
        ),
        // Zephrys: if you have two copies of a minion, find the third (3 Wishes per game).
        (
            "TB_BaconShop_HP_102",
            Script {
                // "If you have two copies of a minion, find the third. (3 Wishes left!)": the
                // uses are counted in `uses_game` (`hero_power_available`).
                shown: Some(|c, sh| {
                    let used = c
                        .player_ref(sh.side)
                        .and_then(|p| p.power_of(sh.card))
                        .map_or(0, |h| h.uses_game as i32);
                    vec![(3 - used).max(0)]
                }),
                ..active(TargetSpec::None, |c, me, _| {
                    let Some(p) = c.owner[me.side] else { return };
                    // Several pairs: a random one.
                    let pairs = pairs_of(&c.boards[me.side], &c.g.players[p].hand);
                    if let Some(&card) = c.g.rng.pick(&pairs) {
                        c.give_card(me.side, card);
                    }
                })
            },
        ),
        // Silas Darkmoon: Darkmoon Tickets appear on Tavern cards after each Refresh; get 3
        // (buy the marked cards) to Discover a minion of your Tier.
        (
            "TB_BaconShop_HP_101",
            passive(Script {
                on_event: Some(|c, me, e| match e {
                    Event::Refreshed { side, .. } if *side == me.side => mark_tickets(c, me),
                    Event::Bought { side, sid, .. } if *side == me.side => {
                        let Some(h) = c.power_mut(me.side, me.card) else {
                            return;
                        };
                        let Some(i) = h.values.iter().position(|x| *x == *sid as i32) else {
                            return;
                        };
                        h.values.remove(i);
                        if tick(c, me, 1, 3) > 0 {
                            // Three tickets give a Prize card of your Tier (its data), played
                            // when you want: it Discovers then.
                            let tier = i32::from(c.tier(me.side));
                            let prize = Item::Spell {
                                card: cid("TB_BaconShop_HP_101t2"),
                                data: tier,
                            };
                            c.add_to_hand(me.side, prize);
                        }
                    }
                    _ => {}
                }),
                listens: ev::REFRESHED | ev::BOUGHT,
                ..E
            }),
        ),
        // Varden Dawngrasp: after the Tavern is Refreshed, copy its highest-Tier minion (random
        // among ties) and Freeze them both (only those two).
        (
            "BG22_HERO_004p",
            passive(Script {
                on_event: Some(|c, me, e| {
                    if !matches!(e, Event::Refreshed { side, .. } if *side == me.side) {
                        return;
                    }
                    let Some(p) = c.owner[me.side] else { return };
                    let shop = &c.g.players[p].shop;
                    let top = shop
                        .iter()
                        .filter_map(|s| s.item.as_minion())
                        .map(|m| m.tier)
                        .max();
                    let Some(top) = top else { return };
                    let cands: Vec<EntityId> = shop
                        .iter()
                        .filter(|s| s.item.as_minion().is_some_and(|m| m.tier == top))
                        .map(|s| s.sid)
                        .collect();
                    let Some(&orig) = c.g.rng.pick(&cands) else {
                        return;
                    };
                    let Some(orig_m) = c.find_any(orig).cloned() else {
                        return;
                    };
                    let copy = c.full_copy(&orig_m);
                    let sid = copy.eid;
                    let mut slot = crate::state::ShopSlot::new(sid, Item::Minion(copy));
                    slot.frozen = true;
                    let shop = &mut c.g.players[p].shop;
                    let at = shop
                        .iter()
                        .position(|s| s.sid == orig)
                        .map_or(shop.len(), |i| i + 1);
                    shop.insert(at, slot);
                    if let Some(s) = shop.iter_mut().find(|s| s.sid == orig) {
                        s.frozen = true;
                    }
                    // Varden's Aquarrior: both get stats equal to your Tier (golden: twice).
                    let k = buddy_power(c, me.side, "BG22_HERO_004_Buddy") * c.tier(me.side) as i32;
                    if k > 0 {
                        for t in [orig, sid] {
                            c.buff(t, k, k);
                        }
                    }
                }),
                listens: ev::REFRESHED,
                ..E
            }),
        ),
        // Scabbs Cutterbutter: Discover a plain copy of a minion from your next opponent's
        // warband.
        (
            "BG21_HERO_010p",
            active(TargetSpec::None, |c, me, _| {
                let Some(p) = c.owner[me.side] else { return };
                // The next opponent's warband as it last fought; a ghost (odd lobby) shows its
                // last board.
                let board = match c.g.players[p].next_opponent {
                    Some(o) => c.g.fought_board(o),
                    None => match c.g.eliminated.last() {
                        Some(&e) if c.g.players[p].next_is_ghost => c.g.fought_board(e),
                        _ => return,
                    },
                };
                let mut cards: Vec<CardId> = board.iter().map(|m| db().normal_of(m.card)).collect();
                // Distinct cards (not just adjacent duplicates), in board order.
                let mut seen = Vec::new();
                cards.retain(|x| {
                    let new = !seen.contains(x);
                    seen.push(*x);
                    new
                });
                let picks = c.g.rng.sample_indices(cards.len(), 3);
                let options = picks
                    .into_iter()
                    .map(|i| Item::Minion(c.new_minion(me.side, cards[i])))
                    .collect();
                c.offer_discover(me.side, me.card, options);
            }),
        ),
        // Cookie the Cook: throw a minion in your pot; with 3, Discover from their types.
        (
            "BG21_HERO_020p",
            Script {
                // "Throw a minion in your pot. When you've gathered 3, Discover from their
                // types. (3 left!)": the pot is `memory`.
                shown: Some(|c, sh| {
                    let n = c
                        .player_ref(sh.side)
                        .and_then(|p| p.power_of(sh.card))
                        .map_or(0, |h| h.memory.len() as i32);
                    vec![3, (3 - n).max(0)]
                }),
                ..active(TargetSpec::AnyTyped, |c, me, t| {
                    // A friendly or Tavern minion goes into the pot; it must have a type.
                    let m = match c.locate(t) {
                        Some((s, i)) => c.boards[s].remove(i),
                        None => match c.take_from_shop(me.side, t) {
                            Some(m) => m,
                            None => return,
                        },
                    };
                    c.g.pool.return_minion(&m);
                    let Some(h) = c.power_mut(me.side, me.card) else {
                        return;
                    };
                    h.memory.push(db().normal_of(m.card));
                    if h.memory.len() < 3 {
                        return;
                    }
                    let pot = std::mem::take(&mut h.memory);
                    let types = pot
                        .iter()
                        .fold(Races::NONE, |r, x| Races(r.0 | db().get(*x).races.0));
                    let tier = c.tier(me.side);
                    c.discover_minion(me.side, me.card, 3, |d| {
                        (types.is_none() || d.races.intersects(types)) && d.tier <= tier
                    });
                })
            },
        ),
        // Murloc Holmes: guess which of 2 minions your next opponent had last combat; correct = a
        // Tavern Coin (`after_discover`; memory = the right answer). The right one is a minion of
        // that warband as it was (a Golden one stays Golden). The other is a likely-looking minion
        // they did not have: Tier up to theirs, of their main type or typeless; Golden as often as
        // their warband's minions are (on the client the decoy is Golden about as often as the
        // right one).
        (
            "BG23_HERO_303p2",
            active(TargetSpec::None, |c, me, _| {
                let Some(p) = c.owner[me.side] else { return };
                let o = match c.g.players[p].next_opponent {
                    Some(o) => o,
                    None => match c.g.eliminated.last() {
                        Some(&e) if c.g.players[p].next_is_ghost => e,
                        _ => return,
                    },
                };
                let board = c.g.fought_board(o);
                let theirs: Vec<CardId> = board.iter().map(|m| m.card).collect();
                let plain: Vec<CardId> = theirs.iter().map(|x| db().normal_of(*x)).collect();
                let goldens = board.iter().filter(|m| m.golden).count() as u32;
                let main = main_type(board, c.g.lobby_races);
                let tier = c.g.players[o].tier;
                let right =
                    c.g.rng
                        .pick(&theirs)
                        .copied()
                        .unwrap_or(cid("BG23_HERO_303pt"));
                let fits = |d: &crate::cards::CardDef| d.tier <= tier && !plain.contains(&d.cid);
                let Some(wrong) = c
                    .random_minion(|d| {
                        fits(d) && (d.races.is_none() || main.is_none_or(|t| d.races.intersects(t)))
                    })
                    .or_else(|| c.random_minion(fits))
                else {
                    return;
                };
                let wrong = match db().get(wrong).golden {
                    Some(g) if c.g.rng.chance(goldens, theirs.len().max(1) as u32) => g,
                    _ => wrong,
                };
                if let Some(h) = c.power_mut(me.side, me.card) {
                    h.memory = vec![right];
                }
                let mut options = vec![right, wrong];
                c.g.rng.shuffle(&mut options);
                let items = options
                    .into_iter()
                    .map(|x| Item::Minion(c.new_minion(me.side, x)))
                    .collect();
                c.offer_discover(me.side, me.card, items);
            }),
        ),
        // Lord Barov: guess the winner of another combat; correct = 3 Tavern Coins, checked at
        // the start of the next turn (counter = guessed player + 1; -(p + 1) = the ghost beats
        // p). Your own combat is offered only when no other exists (two players left); a
        // player-vs-ghost combat can be picked (odd number alive).
        (
            "TB_BaconShop_HP_081",
            Script {
                hero_power: Some(|c, me, _| {
                    let Some(p) = c.owner[me.side] else { return };
                    let others: Vec<(usize, Option<usize>)> =
                        c.g.pairs
                            .iter()
                            .copied()
                            .filter(|(a, b)| *a != p && *b != Some(p))
                            .collect();
                    let pairs = if others.is_empty() {
                        c.g.pairs.clone()
                    } else {
                        others
                    };
                    let Some(&(a, b)) = c.g.rng.pick(&pairs) else {
                        return;
                    };
                    let ghost = c.g.eliminated.last().copied();
                    let Some(b) = b.or(ghost) else { return };
                    if let Some(h) = c.power_mut(me.side, me.card) {
                        h.values = vec![a as i32, b as i32];
                    }
                    let items = [a, b]
                        .iter()
                        .filter_map(|q| c.g.players[*q].hero)
                        .map(Item::spell)
                        .collect();
                    c.offer_discover(me.side, me.card, items);
                }),
                start_of_turn: Some(|c, me| {
                    let Some(h) = c.power_mut(me.side, me.card) else {
                        return;
                    };
                    let guess = std::mem::take(&mut h.counter);
                    if guess == 0 {
                        return;
                    }
                    let q = (guess.abs() - 1) as usize;
                    let st = c.g.players.get(q).map(|pl| &pl.stats);
                    let right = if guess > 0 {
                        st.is_some_and(|s| s.won_last)
                    } else {
                        st.is_some_and(|s| s.lost_last)
                    };
                    if right {
                        give_n(c, me.side, TAVERN_COIN, 3);
                    }
                }),
                ..E
            },
        ),
        // Mr. Bigglesworth: after another hero dies, Discover a minion from their warband
        // (keeping enchantments) -- `on_eliminated`.
        ("TB_BaconShop_HP_080", passive(Script { ..E })),
        // Consolation Coin: gain 1 Gold.
        (
            "TB_BaconShop_HP_080t",
            Script {
                spell: Some(|c, me, _| c.gain_gold(me.side, 1)),
                ..E
            },
        ),
        // Ambassador Faelin: skip your first turn (no Gold); Discover minions from Tiers 6, 4
        // and 2 to get when you reach those Tiers. The three sit in the hand, locked, until
        // then (`after_discover`, `release_held`); memory = the cards still held.
        (
            "BG22_HERO_201p",
            passive(Script {
                on_attach: Some(|c, me| {
                    for t in [2u8, 4, 6] {
                        c.discover_minion(me.side, me.card, 3, |d| d.tier == t);
                    }
                }),
                start_of_turn: Some(|c, me| {
                    if c.g.turn == 1
                        && let Some(p) = c.player(me.side)
                    {
                        p.gold = 0;
                        p.skipping = true;
                    }
                    let tier = c.tier(me.side);
                    let Some(h) = c.power_mut(me.side, me.card) else {
                        return;
                    };
                    let (ready, later): (Vec<CardId>, Vec<CardId>) =
                        h.memory.iter().partition(|x| db().get(**x).tier <= tier);
                    h.memory = later;
                    for x in ready {
                        super::release_held(c, me.side, x);
                    }
                }),
                on_event: Some(|c, me, e| {
                    let Event::Upgraded { side, tier } = e else {
                        return;
                    };
                    if *side != me.side {
                        return;
                    }
                    let Some(h) = c.power_mut(me.side, me.card) else {
                        return;
                    };
                    let (ready, later): (Vec<CardId>, Vec<CardId>) =
                        h.memory.iter().partition(|x| db().get(**x).tier <= *tier);
                    h.memory = later;
                    for x in ready {
                        super::release_held(c, me.side, x);
                    }
                }),
                listens: ev::UPGRADED,
                ..E
            }),
        ),
        // A. F. Kay: skip your first two turns (no Gold), then Discover a minion from Tier 3
        // and from Tier 4.
        (
            "TB_BaconShop_HP_044",
            passive(Script {
                start_of_turn: Some(|c, me| match c.g.turn {
                    1 | 2 => {
                        if let Some(p) = c.player(me.side) {
                            p.gold = 0;
                            p.skipping = true;
                        }
                    }
                    3 => {
                        c.discover_minion(me.side, me.card, 3, |d| d.tier == 4);
                        c.discover_minion(me.side, me.card, 3, |d| d.tier == 3);
                    }
                    _ => {}
                }),
                ..E
            }),
        ),
    ]
}

/// A warband's main type as the leaderboard shows it: the lobby type most of its minions have
/// (a minion of several types counts for each; a type only an Amalgam has does not show);
/// ties go to the first type. None for a warband without a typed minion.
fn main_type(board: &[Minion], lobby: Races) -> Option<Races> {
    let mut present = Races::NONE;
    for m in board.iter().filter(|m| m.races != Races::ALL) {
        present = Races(present.0 | (m.races.0 & lobby.0));
    }
    present
        .iter()
        .map(|t| (t, board.iter().filter(|m| m.races.intersects(t)).count()))
        .fold(None, |best: Option<(Races, usize)>, (t, n)| match best {
            Some((_, k)) if k >= n => best,
            _ => Some((t, n)),
        })
        .map(|(t, _)| t)
}
