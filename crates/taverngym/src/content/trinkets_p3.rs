//! Trinkets: economy, buying and Refreshing. Per-card buy prices are in `Game::slot_price` (rules
//! keyed by trinket); the hooks it needs live here.

use super::trinkets::{shown_left, shown_turn_left};
use super::*;
use crate::script::Shown;
use crate::state::Player;

const SARGERAS: &str = "BG30_MagicItem_701";
const TAPESTRY: &str = "BG35_MagicItem_152";
const WHISTLE: &str = "BG35_MagicItem_930"; // Warband Whistle
const CANDLE: &str = "BG32_MagicItem_366";

fn counter_of(p: &Player, id: &str) -> i32 {
    p.counters
        .iter()
        .find(|x| db().get(x.0).id == id)
        .map_or(0, |x| x.1)
}

/// The Eye of Sargeras: the next card you buy is the fourth (costs Health).
pub fn sargeras_ready(p: &Player) -> bool {
    p.has_trinket(SARGERAS) && counter_of(p, SARGERAS) == 3
}

/// Demonic Tapestry: 4 Refreshes done since its last use.
pub fn tapestry_armed(p: &Player) -> bool {
    counter_of(p, TAPESTRY) >= 4
}

/// A per-card price rule was used for a purchase.
pub fn price_rule_used(c: &mut Ctx, side: Side, rule: CardId) {
    if db().get(rule).id == TAPESTRY {
        c.set_card_data(side, rule, 0);
    } else {
        c.add_turn_data(side, rule, 1);
    }
}

/// The trinkets of card `id` `side`'s player owns (a Souvenir Stand copy is a second one).
fn copies(c: &Ctx, side: Side, id: CardId) -> Vec<CardId> {
    c.player_ref(side).map_or(Vec::new(), |p| {
        p.trinkets
            .iter()
            .copied()
            .filter(|t| t.base() == id)
            .collect()
    })
}

/// The Refresh action: Guiding Candle's Tier 6 Refreshes, else a normal one; then Knockoff
/// Wisdomball's helpful replacement (before "after Refresh" effects).
pub fn player_refresh(c: &mut Ctx, side: Side) {
    // Warband Whistle: the next Refresh the player presses (not a Refresh made by a spell or
    // another effect) contains plain copies of the warband. Each Whistle (a Souvenir Stand copy
    // too) arms its own press.
    let whistle = copies(c, side, cid(WHISTLE))
        .into_iter()
        .find(|t| c.card_data(side, *t) == 1);
    let armed = whistle.is_some();
    if let Some(w) = whistle {
        c.set_card_data(side, w, 0);
        c.shop_prefill = Some(whistle_copies);
    }
    // The copies come first: the Whistle's Refresh is not one of Guiding Candle's Tier 6 Refreshes,
    // and a helpful Refresh (Knockoff Wisdomball...) waits for the next press instead of clearing
    // the copies.
    c.shop_fixup = (!armed).then_some(super::helpful::fixup as fn(&mut Ctx, Side));
    // Guiding Candle: a Souvenir Stand copy acts on the same first two Refreshes, so every
    // copy counts each of them (whether two Candles make four is not known).
    let candles = copies(c, side, cid(CANDLE));
    if !armed && candles.iter().any(|t| c.turn_data(side, *t) < 2) {
        for t in candles {
            c.add_turn_data(side, t, 1);
        }
        tier6_refresh(c, side);
    } else {
        c.player_refresh_shop(side);
    }
    c.shop_fixup = None;
    c.shop_prefill = None;
    after_you_refresh(c, side);
}

/// "Every N Refreshes" counters: the Refresh button only.
fn after_you_refresh(c: &mut Ctx, side: Side) {
    let Some(p) = c.player_ref(side) else { return };
    if p.trinkets.is_empty() {
        return;
    }
    let has = |id: &str| p.has_trinket(id);
    let tapestry = has(TAPESTRY);
    // Souvenir copies each keep their own refresh counter and improvement level.
    let wheels: Vec<CardId> = p
        .trinkets
        .iter()
        .copied()
        .filter(|t| t.base() == cid("BG30_MagicItem_879t"))
        .collect();
    for wheel in wheels {
        if c.tick_counter(side, wheel, 4) {
            tavern_aura(c, side, 2, 2);
            c.improve_card(side, wheel);
        }
    }
    // Demonic Tapestry: armed after 4 Refreshes.
    if tapestry {
        let t = cid(TAPESTRY);
        let n = c.card_data(side, t);
        c.set_card_data(side, t, (n + 1).min(4));
    }
}

/// A Refresh that contains only Tier 6 cards.
fn tier6_refresh(c: &mut Ctx, side: Side) {
    let Some(p) = c.owner[side] else { return };
    c.clear_shop(side);
    for _ in 0..c.shop_minion_slots(side) {
        let Some(m) = c.draw_pool_minion(side, 6, 6, Races::NONE) else {
            break;
        };
        let sid = m.eid;
        c.g.players[p]
            .shop
            .push(crate::state::ShopSlot::new(sid, Item::Minion(m)));
    }
    for _ in 0..c.g.cfg.shop_spell_slots {
        if let Some(s) = c.draw_pool_spell(|d| d.tier == 6) {
            let sid = c.g.alloc_eid();
            c.g.players[p]
                .shop
                .push(crate::state::ShopSlot::new(sid, Item::spell(s)));
        }
    }
    if let Some(f) = c.shop_fixup.take() {
        f(c, side);
    }
    if let Some(pl) = c.player(side) {
        pl.stats.refreshes = pl.stats.refreshes.saturating_add(1);
    }
    c.emit(Event::Refreshed { side, auto: false });
}

/// "Minions in the Tavern have +a/+h" (a Tavern aura added now; improvements add more).
fn tavern_aura(c: &mut Ctx, side: Side, a: i32, h: i32) {
    if let Some(p) = c.player(side) {
        p.stats.tavern_atk = p.stats.tavern_atk.saturating_add(a);
        p.stats.tavern_hp = p.stats.tavern_hp.saturating_add(h);
    }
    c.buff_tavern(side, a, h, Races::NONE);
}

fn tavern_elementals(c: &mut Ctx, side: Side, a: i32, h: i32) {
    if let Some(p) = c.player(side) {
        p.stats.tavern_elemental_atk = p.stats.tavern_elemental_atk.saturating_add(a);
        p.stats.tavern_elemental_hp = p.stats.tavern_elemental_hp.saturating_add(h);
    }
    c.buff_tavern(side, a, h, Races::ELEMENTAL);
}

fn reduce_upgrade(c: &mut Ctx, side: Side, n: i32) {
    if let Some(p) = c.player(side) {
        p.upgrade_cost = (p.upgrade_cost - n).max(0);
    }
}

fn free_refreshes(c: &mut Ctx, side: Side, n: i32) {
    if let Some(p) = c.player(side) {
        p.free_refreshes += n;
    }
}

fn bought_minion(me: Me, e: &Event) -> Option<(CardId, EntityId)> {
    match e {
        Event::Bought {
            side, card, eid, ..
        } if *side == me.side && db().get(*card).kind == crate::cards::CardKind::Minion => {
            Some((*card, *eid))
        }
        _ => None,
    }
}

fn played_elemental(_c: &Ctx, me: Me, e: &Event) -> bool {
    matches!(e, Event::Played { side, played, .. } if *side == me.side
        && played.races.intersects(Races::ELEMENTAL))
}

/// Upstart Embers: double the stats of the highest-Health Tavern minion.
fn double_highest(c: &mut Ctx, side: Side) {
    if let Some(sid) = c.highest_health_tavern(side)
        && let Some((a, h)) = c.find_any(sid).map(|m| (m.atk, m.hp))
    {
        c.buff(sid, a, h);
    }
}

/// Electrode Attractor: an extra Magnetic Mech (tier <= yours) from the pool.
fn extra_magnetic_mech(c: &mut Ctx, side: Side) {
    let Some(p) = c.owner[side] else { return };
    if !c.shop_make_room(side) {
        return;
    }
    let tier = c.tier(side);
    let floor = c.tavern_tier_floor(side);
    let cands =
        c.g.universe
            .iter()
            .copied()
            .filter(|x| {
                let d = db().get(*x);
                d.tier >= floor
                    && d.tier <= tier
                    && d.races.intersects(Races::MECH)
                    && d.mech.has(Mech::MAGNETIC)
            })
            .map(|x| (x, c.g.pool.get(x) as u64))
            .collect();
    let Some(&card) = c.weighted_distinct(cands, 1).first() else {
        return;
    };
    let mut m = c.new_minion(side, card);
    c.take_pool_copy(&mut m);
    c.apply_tavern_buffs(side, &mut m);
    let sid = m.eid;
    c.g.players[p]
        .shop
        .push(crate::state::ShopSlot::new(sid, Item::Minion(m)));
}

/// Warband Whistle: a free Refresh whose minion slots are plain copies of warband minions
/// (from the pool when copies are left; remaining slots fill normally). The copies are in
/// place before "after Refresh" effects.
/// Warband Whistle's Refresh: a plain copy of every minion of the warband comes first (not limited
/// to the Tavern's usual minion count; the copies have the highest priority). The default fill then
/// adds its minions if the copies are fewer and its spell while the Tavern has room: seven copies
/// leave no spell.
fn whistle_copies(c: &mut Ctx, side: Side) {
    let Some(p) = c.owner[side] else { return };
    let mut cards: Vec<CardId> = c.boards[side]
        .iter()
        .map(|m| db().normal_of(m.card))
        .collect();
    c.g.rng.shuffle(&mut cards);
    cards.truncate(c.g.cfg.shop_max_cards);
    for card in cards {
        let mut m = c.new_minion(side, card);
        m.pool_copies = u8::from(c.g.pool.take(card));
        c.apply_tavern_buffs(side, &mut m);
        c.shop_priority.push(m.eid);
        c.g.players[p]
            .shop
            .push(crate::state::ShopSlot::new(m.eid, Item::Minion(m)));
    }
    c.shop_fill_cap = Some(c.g.cfg.shop_max_cards);
}

/// `shown` of Dalaran Cheese Wheel / Nether Pendant: the +2/+2 with its improvements, the
/// period and what is left of it.
fn shown_tavern_aura(c: &Ctx, sh: &crate::script::Shown, every: i32) -> Vec<i32> {
    let k = 2 * (1 + c.card_level(sh.side, sh.card));
    vec![k, k, every, every - c.card_data(sh.side, sh.card)]
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Nether Pendant: Tavern minions have +2/+2; improves every 3 times your hero takes
        // damage outside combat.
        (
            "BG30_MagicItem_541",
            Script {
                on_attach: Some(|c, me| tavern_aura(c, me.side, 2, 2)),
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::HeroDamaged { side, in_combat: false, amount } if *side == me.side && *amount > 0)
                        && c.tick_counter(me.side, me.card, 3)
                    {
                        tavern_aura(c, me.side, 2, 2);
                        // The level is only kept for the card text (`shown`).
                        c.improve_card(me.side, me.card);
                    }
                }),
                listens: ev::HERO_DAMAGED,
                // "Minions in the Tavern have +2/+2. Improves every 3 times your hero takes
                // damage outside combat. (3 left!)"
                shown: Some(|c, sh| shown_tavern_aura(c, sh, 3)),
                ..E
            },
        ),
        // Nomi Sticker: after you play an Elemental, Tavern Elementals +3/+2 (+5/+5) this game.
        (
            "BG30_MagicItem_544",
            Script {
                on_event: Some(|c, me, e| {
                    if played_elemental(c, me, e) {
                        tavern_elementals(c, me.side, 3, 2);
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        (
            "BG30_MagicItem_544t",
            Script {
                on_event: Some(|c, me, e| {
                    if played_elemental(c, me, e) {
                        tavern_elementals(c, me.side, 5, 5);
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // The Eye of Sargeras: every fourth card you buy costs Health (`slot_price`); this
        // counts purchases.
        (
            SARGERAS,
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Bought { side, .. } if *side == me.side) {
                        c.tick_counter(me.side, me.card, 4);
                    }
                }),
                listens: ev::BOUGHT,
                // "Every fourth card you buy costs Health instead of Gold. (3 left!)"
                shown: Some(|c, sh| vec![shown_left(c, sh, 3)]),
                ..E
            },
        ),
        // Bartend-o-Tron's Oilcan: upgrading costs (3) less; again each turn.
        (
            "BG30_MagicItem_705",
            Script {
                on_attach: Some(|c, me| reduce_upgrade(c, me.side, 3)),
                start_of_turn: Some(|c, me| reduce_upgrade(c, me.side, 3)),
                ..E
            },
        ),
        // Dalaran Cheese Wheel: Tavern minions have +2/+2; improves every 4 Refreshes
        // (`after_you_refresh`).
        (
            "BG30_MagicItem_879t",
            Script {
                on_attach: Some(|c, me| tavern_aura(c, me.side, 2, 2)),
                // "Minions in the Tavern have +2/+2. Improves every 4 times you Refresh.
                // (4 left!)"
                shown: Some(|c, sh| shown_tavern_aura(c, sh, 4)),
                ..E
            },
        ),
        // Guiding Candle: `player_refresh`.
        (
            CANDLE,
            Script {
                // "Your first two Refreshes each turn contain only Tier 6 cards. (2 left!)"
                shown: Some(|c, sh| vec![6, shown_turn_left(c, sh, 2)]),
                ..E
            },
        ),
        // Pilgrimp / Bazaar Sticker: `slot_price`.
        (
            "BG32_MagicItem_821",
            Script {
                // "One Demon each turn costs Health instead of Gold to buy. (1 left!)"
                shown: Some(|c, sh| vec![shown_turn_left(c, sh, 1)]),
                ..E
            },
        ),
        (
            "BG32_MagicItem_822",
            Script {
                // "One Tavern spell each turn costs Health instead of Gold to buy. (1 left!)"
                shown: Some(|c, sh| vec![shown_turn_left(c, sh, 1)]),
                ..E
            },
        ),
        // Recycling Sticker: after you play an Elemental, gain a free Refresh.
        (
            "BG32_MagicItem_888",
            Script {
                on_event: Some(|c, me, e| {
                    if played_elemental(c, me, e) {
                        free_refreshes(c, me.side, 1);
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Gold-plated Compass: the next minion of its type (see Colorful Compass; the type is the
        // card data, cleared once used) you buy becomes Golden; gain 5 free Refreshes. Played from
        // hand, that Golden minion gives a Triple Reward: the cards whose Golden minions give none
        // say so in their text (Aureate Laureate: "always Golden, but won't get a Triple Reward"),
        // and this one does not. No source settles it.
        (
            "BG32_MagicItem_901",
            Script {
                on_attach: Some(|c, me| {
                    free_refreshes(c, me.side, 5);
                    super::trinkets_p2::compass_type(c, me.side, me.card);
                }),
                on_event: Some(|c, me, e| {
                    let Some((card, hid)) = bought_minion(me, e) else {
                        return;
                    };
                    let r = Races(c.card_data(me.side, me.card) as u16);
                    // `make_golden` gives the reward; a minion that already is Golden keeps
                    // what it had (Aureate Laureate: none).
                    if !r.is_none()
                        && db().get(card).races.intersects(r)
                        && let Some(m) = c.find_any_mut(hid)
                    {
                        super::spells::make_golden(m);
                        c.set_card_data(me.side, me.card, 0);
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Demonic Tapestry: armed after 4 Refreshes (`after_you_refresh`, `slot_price`).
        (
            TAPESTRY,
            Script {
                // "After you Refresh 4 times, the highest-Tier minion in the Tavern costs Health
                // instead of Gold to buy. (4 left!)"
                shown: Some(|c, sh| vec![4, shown_left(c, sh, 4)]),
                ..E
            },
        ),
        // Electrode Attractor: Magnetic Mechs cost (2) (`slot_price`); an extra one in each
        // Refresh.
        (
            "BG35_MagicItem_743",
            Script {
                on_fill_shop: Some(|c, me| extra_magnetic_mech(c, me.side)),
                ..E
            },
        ),
        // Magicfin Sticker: after you buy a Tavern spell, get a 1/1 Magicfin Apprentice taught
        // that spell (2 times per turn).
        (
            "BG35_MagicItem_750",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Bought { side, card, .. } = e else {
                        return;
                    };
                    if *side != me.side
                        || !is_tavern_spell(*card)
                        || c.turn_data(me.side, me.card) >= 2
                    {
                        return;
                    }
                    c.add_turn_data(me.side, me.card, 1);
                    let mut a = c.new_minion(me.side, cid("BG33_890t"));
                    a.counter = card.0 as i32 + 1; // the taught spell
                    c.add_to_hand(me.side, Item::Minion(a));
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Sous Chef Sticker: extra Hero Power use and 1 Gold after each (`recruit.rs`).
        ("BG35_MagicItem_801", E),
        // Upstart Embers: after you Refresh, double the highest-Health Tavern minion's stats.
        (
            "BG35_MagicItem_862",
            Script {
                on_event: Some(|c, me, e| {
                    // "After you Refresh": not the turn-start refill or an effect's Refresh.
                    if matches!(e, Event::Refreshed { side, auto: false } if *side == me.side) {
                        double_highest(c, me.side);
                    }
                }),
                listens: ev::REFRESHED,
                ..E
            },
        ),
        (
            WHISTLE,
            Script {
                // "Gain a free Refresh that contains plain copies of minions in your
                // warband": a free Refresh now; the copies come with the player's next
                // press of the Refresh button (`player_refresh`).
                on_attach: Some(|c, me| {
                    free_refreshes(c, me.side, 1);
                    c.set_card_data(me.side, me.card, 1);
                }),
                ..E
            },
        ),
        // Transcribing Typewriter: an extra copy of the next 2 (4) minions you buy.
        (
            "BG35_MagicItem_931",
            Script {
                on_attach: Some(|c, me| c.set_card_data(me.side, me.card, 2)),
                on_event: Some(typewriter),
                listens: ev::BOUGHT,
                // "Get an extra copy of the next 2 minions you buy. (2 left!)"
                shown: Some(|c, sh| vec![2, typewriter_left(c, sh, 2)]),
                ..E
            },
        ),
        (
            "BG35_MagicItem_931t",
            Script {
                on_attach: Some(|c, me| c.set_card_data(me.side, me.card, 4)),
                on_event: Some(typewriter),
                listens: ev::BOUGHT,
                // "Get an extra copy of the next 4 minions you buy. (4 left!)"
                shown: Some(|c, sh| vec![4, typewriter_left(c, sh, 4)]),
                ..E
            },
        ),
        // Herding Horn: whenever a friendly Rally minion attacks, gain a free Refresh.
        (
            "BG36_MagicItem_200",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Attacks { side, attacker, .. } = e
                        && *side == me.side
                        && c.minion(*attacker).is_some_and(|m| {
                            crate::script::minion_has(m, crate::script::Hook::Rally)
                        })
                    {
                        free_refreshes(c, me.side, 1);
                    }
                }),
                listens: ev::ATTACKS,
                ..E
            },
        ),
        // Warcry Totem: `slot_price`.
        (
            "BG36_MagicItem_202",
            Script {
                // "The first two Battlecry minions you buy each turn are free. (2 left!)"
                shown: Some(|c, sh| vec![shown_turn_left(c, sh, 2)]),
                ..E
            },
        ),
        // Glowing Crystal: at the start of your turn, 1 Gold per different friendly type.
        (
            "BG36_MagicItem_220",
            Script {
                start_of_turn: Some(|c, me| {
                    let n = distinct_type_count(c, me.side);
                    c.gain_gold(me.side, n);
                }),
                ..E
            },
        ),
        // Frigid Blossom: after you Refresh, upgrading costs (1) less.
        (
            "BG36_MagicItem_300",
            Script {
                on_event: Some(|c, me, e| {
                    // "After you Refresh": not the turn-start refill or an effect's Refresh.
                    if matches!(e, Event::Refreshed { side, auto: false } if *side == me.side) {
                        reduce_upgrade(c, me.side, 1);
                    }
                }),
                listens: ev::REFRESHED,
                ..E
            },
        ),
        // Wand of Divination: after you cast 3 spells on minions, gain 1 Gold.
        (
            "BG36_MagicItem_307",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, target, .. } if *side == me.side && *target != 0)
                        && c.tick_counter(me.side, me.card, 3)
                    {
                        c.gain_gold(me.side, 1);
                    }
                }),
                listens: ev::SPELL_CAST,
                // "After you cast 3 spells on minions, gain 1 Gold. (3 left!)"
                shown: Some(|c, sh| vec![3, 1, shown_left(c, sh, 3)]),
                ..E
            },
        ),
        // Rascal Sticker: 1 Gold per friendly Golden minion; again each turn.
        (
            "BG36_MagicItem_424",
            Script {
                on_attach: Some(rascal),
                start_of_turn: Some(rascal),
                ..E
            },
        ),
        // Trickster's Sleeve: after you buy a minion, gain 1 Gold, then swap to Tavern spells
        // (and back): card data 0 = minion, 1 = spell.
        (
            "BG36_MagicItem_609",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Bought { side, card, .. } = e else {
                        return;
                    };
                    if *side != me.side {
                        return;
                    }
                    let spell = db().get(*card).kind != crate::cards::CardKind::Minion;
                    let mode = c.card_data(me.side, me.card);
                    if (mode == 1) == spell {
                        c.gain_gold(me.side, 1);
                        c.set_card_data(me.side, me.card, 1 - mode);
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Demonic Bloodletter: whenever you cast a Tavern spell, Tavern minions +1/+1 this
        // game.
        (
            "BG36_MagicItem_800",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, tavern: true, .. } if *side == me.side)
                    {
                        tavern_aura(c, me.side, 1, 1);
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
    ]
}

fn typewriter(c: &mut Ctx, me: Me, e: &Event) {
    let Some((card, hid)) = bought_minion(me, e) else {
        return;
    };
    let left = c.card_data(me.side, me.card);
    if left > 0 {
        c.set_card_data(me.side, me.card, left - 1);
        // A full copy of the bought minion. A copy that waits for hand space is given all the same:
        // only a bought minion that is not in hand any more falls back to a plain card.
        if c.give_copy_of_hand(me.side, hid).is_none() && c.queued.is_none() {
            c.give_card(me.side, card);
        }
    }
}

/// Transcribing Typewriter's "(N left!)": the card data once the trinket is owned (its
/// `on_attach` sets it to `n`), the printed `n` before that.
fn typewriter_left(c: &Ctx, sh: &Shown, n: i32) -> i32 {
    if c.player_ref(sh.side)
        .is_some_and(|p| p.has_trinket_id(sh.card.base()))
    {
        c.card_data(sh.side, sh.card).max(0)
    } else {
        n
    }
}

fn rascal(c: &mut Ctx, me: Me) {
    let n = c.friendly_where(me.side, |m| m.golden).len() as i32;
    c.gain_gold(me.side, n);
}
