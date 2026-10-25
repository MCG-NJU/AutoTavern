//! Trinkets: derived systems -- Blood Gems, Spellcraft trinkets (their spell is given each turn
//! like a Spellcraft minion's), Timewarp Discovers and discard triggers.

use super::trinkets_p6::shown_left;
use super::*;

const GOLEM: &str = "BG30_MagicItem_442t";

/// "Spellcraft: X" trinket: its temporary spell on purchase and at each turn start.
macro_rules! spellcraft_trinket {
    ($spell:literal) => {
        Script {
            on_attach: Some(|c, me| {
                c.give_temp_spell(me.side, cid($spell), 0);
            }),
            start_of_turn: Some(|c, me| {
                c.give_temp_spell(me.side, cid($spell), 0);
            }),
            ..E
        }
    };
}

/// Get `n` random minions of type `r` (tier <= yours).
fn get_random_of(c: &mut Ctx, side: Side, r: Races, n: usize) {
    let tier = c.tier(side);
    for _ in 0..n {
        if let Some(x) = c.random_minion(|d| d.tier <= tier && d.races.intersects(r)) {
            c.give_card(side, x);
        }
    }
}

/// Jailer Sticker: destroy the friendly Undead, then get `n` random Undead.
fn jailer(c: &mut Ctx, side: Side, t: EntityId, n: usize) {
    c.destroy(t);
    c.resolve_deaths();
    get_random_of(c, side, Races::UNDEAD, n);
}

/// Transform a board or Tavern minion into `card` in place (the old minion's copies return; the new
/// one takes a pool copy when one is left).
pub fn transform_any(c: &mut Ctx, side: Side, eid: EntityId, card: CardId) {
    // The target may be gone by now (consumed or sold while the choice was open): nothing
    // transforms and no pool copy is taken.
    let in_tavern = c
        .player_ref(side)
        .is_some_and(|p| p.shop.iter().any(|s| s.sid == eid));
    if c.locate(eid).is_none() && !in_tavern {
        return;
    }
    let mut m = c.new_minion(side, card);
    m.eid = eid;
    if !c.combat {
        c.take_pool_copy(&mut m);
    }
    if let Some((s, i)) = c.locate(eid) {
        let old = std::mem::replace(&mut c.boards[s][i], m);
        // Combat boards are clones: their copies are not the player's to return.
        if !c.combat {
            c.g.pool.return_minion(&old);
        }
        return;
    }
    let Some(p) = c.owner[side] else { return };
    c.apply_tavern_buffs(side, &mut m);
    if let Some(slot) = c.g.players[p].shop.iter_mut().find(|s| s.sid == eid) {
        let old = std::mem::replace(&mut slot.item, Item::Minion(m));
        slot.frozen = false;
        if let Item::Minion(o) = old {
            c.g.pool.return_minion(&o);
        }
    }
}

/// Chillmere Mosaic: Refresh the Tavern with Battlecry minions (tier <= yours) costing (1).
fn battlecry_refresh(c: &mut Ctx, side: Side) {
    let Some(p) = c.owner[side] else { return };
    let tier = c.tier(side);
    // Bob-blehead: the Tavern offers no Tier 1-2 cards (same rule as `fill_shop`).
    let min_tier = if c
        .player_ref(side)
        .is_some_and(|p| p.has_trinket("BG30_MagicItem_998"))
    {
        3.min(tier)
    } else {
        1
    };
    c.clear_shop(side);
    for _ in 0..c.shop_minion_slots(side) {
        let d = db();
        let cands: Vec<(CardId, u16)> =
            c.g.universe
                .iter()
                .copied()
                .filter(|x| {
                    d.get(*x).tier <= tier
                        && d.get(*x).tier >= min_tier
                        && d.get(*x).mech.has(Mech::BATTLECRY)
                })
                .map(|x| (x, c.g.pool.get(x)))
                .filter(|x| x.1 > 0)
                .collect();
        let total: u64 = cands.iter().map(|x| x.1 as u64).sum();
        if total == 0 {
            break;
        }
        let mut r = c.g.rng.next_u64() % total;
        let mut pick = cands[0].0;
        for (x, n) in cands {
            if r < n as u64 {
                pick = x;
                break;
            }
            r -= n as u64;
        }
        let mut m = c.new_minion(side, pick);
        m.pool_copies = u8::from(c.g.pool.take(pick));
        c.apply_tavern_buffs(side, &mut m);
        let mut slot = crate::state::ShopSlot::new(m.eid, Item::Minion(m));
        slot.price = Some(1);
        c.g.players[p].shop.push(slot);
    }
    for _ in 0..c.g.cfg.shop_spell_slots {
        if let Some(s) = c.random_spell(|d| d.tier <= tier) {
            let sid = c.g.alloc_eid();
            c.g.players[p]
                .shop
                .push(crate::state::ShopSlot::new(sid, Item::spell(s)));
        }
    }
    c.finish_refresh(side, false);
}

/// Timeworn Candelabra: Discover a 2-Cost Timewarp minion of `tier` (Minor = 3, Major = 5),
/// from the cards a visit would offer this player (`timewarp::discover_minions`).
fn discover_timewarp(c: &mut Ctx, me: Me, tier: u8) {
    let cands = super::timewarp::discover_minions(c, me.side, tier == 5, 2, 3);
    let options = cands
        .into_iter()
        .map(|x| Item::Minion(c.new_minion(me.side, x)))
        .collect();
    c.offer_discover(me.side, me.card, options);
}

fn gem_all(c: &mut Ctx, side: Side, n: i32) {
    for e in c.friendly(side) {
        gems_on(c, side, e, n);
    }
}

fn earrings(c: &mut Ctx, me: Me, e: &Event, every: i32, gems: i32) {
    if matches!(e, Event::SpellCast { side, .. } if *side == me.side)
        && c.tick_counter(me.side, me.card, every)
    {
        gem_all(c, me.side, gems);
    }
}

fn jewelry_gem(c: &mut Ctx, me: Me) {
    let kw = 1 + c.g.rng.below(3) as i32;
    c.add_to_hand(
        me.side,
        Item::Spell {
            card: cid(BLOOD_GEM),
            data: kw,
        },
    );
}

/// Writhing Tentacles: after you discard your first minion each turn, get a copy of it with double
/// stats (unbanned with 36.6.3).
pub fn writhing_tentacles(c: &mut Ctx, side: Side, m: &Minion) {
    let t = cid("BG36_MagicItem_606");
    if !c.player_ref(side).is_some_and(|p| p.has_trinket_id(t)) || c.turn_data(side, t) > 0 {
        return;
    }
    let mut copy = c.full_copy(m);
    copy.set_stats(m.atk.saturating_mul(2), m.max_hp.saturating_mul(2));
    if c.add_to_hand(side, Item::Minion(copy)).is_some() {
        c.add_turn_data(side, t, 1);
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Quilligraphy Set: Avenge (3): Blood Gems +1 Health (greater: Avenge (4): +1/+1).
        (
            "BG30_MagicItem_410",
            Script {
                avenge: Some((3, 3, |c, me| gem_bonus(c, me.side, 0, 1))),
                ..E
            },
        ),
        (
            "BG30_MagicItem_410t2",
            Script {
                avenge: Some((4, 4, |c, me| gem_bonus(c, me.side, 1, 1))),
                ..E
            },
        ),
        // Hoggy Bank: SoC your Quilboar get "Deathrattle: get 2 Blood Gems" (attached).
        (
            "BG30_MagicItem_411",
            Script {
                start_of_combat: Some(|c, me| {
                    if me.eid != 0 {
                        return;
                    }
                    for e in c.friendly_where(me.side, |m| m.is(Races::QUILBOAR)) {
                        if let Some(m) = c.minion_mut(e) {
                            m.attached.push(me.card);
                        }
                    }
                }),
                deathrattle: Some(|c, me| give_n(c, me.side, BLOOD_GEM, 2)),
                ..E
            },
        ),
        // Token of the Old Gods: Spellcraft: choose a minion, then a minion from a Tier higher
        // to transform it into (`after_discover`).
        (
            "BG30_MagicItem_416",
            spellcraft_trinket!("BG30_MagicItem_416t"),
        ),
        (
            "BG30_MagicItem_416t",
            Script {
                spell: Some(|c, me, t| {
                    let Some(tier) = c.find_any(t).map(|m| m.tier) else {
                        return;
                    };
                    let next = (tier + 1).min(6);
                    offer_targeted_transform(c, me.side, me.card, t, 3, |d| d.tier == next);
                }),
                spell_target: TargetSpec::FriendlyOrTavern,
                ..E
            },
        ),
        // Blood Golem Sticker: whenever a friendly Quilboar dies, summon a Golem with its Blood
        // Gem stats (only if it had any).
        (
            "BG30_MagicItem_442",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Died {
                        side, eid, races, ..
                    } = e
                    else {
                        return;
                    };
                    if *side != me.side || !races.intersects(Races::QUILBOAR) {
                        return;
                    }
                    let Some((a, h, pos)) = c
                        .dead_record(me.side, *eid)
                        .map(|d| (d.gem_atk, d.gem_hp, d.pos))
                    else {
                        return;
                    };
                    if a <= 0 && h <= 0 {
                        return;
                    }
                    let mut g = c.new_minion(me.side, cid(GOLEM));
                    g.set_stats(a.max(0), h.max(1));
                    // Where the Quilboar died, like a Deathrattle.
                    let pos = pos.min(c.boards[me.side].len());
                    c.summon(me.side, pos, g);
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Jar o' Gems: after every 2 friendly attacks in a combat, a Blood Gem on each Quilboar.
        (
            "BG30_MagicItem_546",
            Script {
                on_event: Some(|c, me, e| {
                    if !c.combat || !matches!(e, Event::Attacks { side, .. } if *side == me.side) {
                        return;
                    }
                    if c.add_combat_data(me.side, me.card, 1) % 2 == 0 {
                        for q in c.friendly_where(me.side, |m| m.is(Races::QUILBOAR)) {
                            c.play_gem(me.side, q);
                        }
                    }
                }),
                listens: ev::ATTACKS,
                // "After 2 friendly minions attack each combat, this plays a Blood Gem on all
                // your Quilboar. (2 left!)"
                shown: Some(|c, sh| vec![2, 2 - c.combat_data(sh.side, sh.card) % 2]),
                ..E
            },
        ),
        // Great Boar Sticker: get 3 (5) Blood Gems; your Blood Gems give an extra +2/+1 (+3/+3).
        (
            "BG30_MagicItem_988",
            Script {
                on_attach: Some(|c, me| {
                    give_n(c, me.side, BLOOD_GEM, 3);
                    gem_bonus(c, me.side, 2, 1);
                }),
                ..E
            },
        ),
        (
            "BG30_MagicItem_988t",
            Script {
                on_attach: Some(|c, me| {
                    give_n(c, me.side, BLOOD_GEM, 5);
                    gem_bonus(c, me.side, 3, 3);
                }),
                ..E
            },
        ),
        // Aggem Sticker: at the end of your turn, 7 Blood Gems on a friendly minion of each
        // type.
        (
            "BG32_MagicItem_284",
            Script {
                end_of_turn: Some(|c, me| {
                    for (e, _) in one_minion_per_type(c, me.side, |_| true) {
                        gems_on(c, me.side, e, 7);
                    }
                }),
                ..E
            },
        ),
        // Bloodbound Earrings: after 4 spells, a Blood Gem on all your minions (greater: after 5,
        // two each).
        (
            "BG32_MagicItem_808",
            Script {
                on_event: Some(|c, me, e| earrings(c, me, e, 4, 1)),
                listens: ev::SPELL_CAST,
                // "After you cast 4 spells, this plays a Blood Gem on all your minions.
                // (4 left!)"
                shown: Some(|c, sh| shown_left(c, sh, 4)),
                ..E
            },
        ),
        (
            "BG32_MagicItem_808t",
            Script {
                on_event: Some(|c, me, e| earrings(c, me, e, 5, 2)),
                listens: ev::SPELL_CAST,
                // "After you cast 5 spells, this plays two Blood Gems on all your minions.
                // (5 left!)"
                shown: Some(|c, sh| shown_left(c, sh, 5)),
                ..E
            },
        ),
        // Vibrant Bubble: Spellcraft: give a Murloc a random Bonus Keyword.
        (
            "BG32_MagicItem_892",
            spellcraft_trinket!("BG32_MagicItem_892t"),
        ),
        (
            "BG32_MagicItem_892t",
            Script {
                spell: Some(|c, _, t| give_random_bonus_keyword(c, t)),
                spell_target: TargetSpec::FriendlyRace(Races::MURLOC),
                ..E
            },
        ),
        // Jailer Sticker: Spellcraft: destroy a friendly Undead to get 1 (greater: 2) random
        // Undead.
        (
            "BG35_MagicItem_306",
            spellcraft_trinket!("BG35_MagicItem_306t"),
        ),
        (
            "BG35_MagicItem_306t",
            Script {
                spell: Some(|c, me, t| jailer(c, me.side, t, 1)),
                spell_target: TargetSpec::FriendlyRace(Races::UNDEAD),
                ..E
            },
        ),
        (
            "BG35_MagicItem_733",
            spellcraft_trinket!("BG35_MagicItem_733t"),
        ),
        (
            "BG35_MagicItem_733t",
            Script {
                spell: Some(|c, me, t| jailer(c, me.side, t, 2)),
                spell_target: TargetSpec::FriendlyRace(Races::UNDEAD),
                ..E
            },
        ),
        // Jewelry Box: a Blood Gem that also gives a Quilboar Taunt, Divine Shield or Reborn
        // (gem data 1..3); again each turn.
        (
            "BG35_MagicItem_434",
            Script {
                on_attach: Some(jewelry_gem),
                start_of_turn: Some(jewelry_gem),
                ..E
            },
        ),
        // Chillmere Mosaic: Spellcraft: Refresh the Tavern with Battlecry minions costing (1).
        (
            "BG35_MagicItem_755",
            spellcraft_trinket!("BG35_MagicItem_755t"),
        ),
        (
            "BG35_MagicItem_755t",
            Script {
                spell: Some(|c, me, _| battlecry_refresh(c, me.side)),
                ..E
            },
        ),
        // Timeworn Candelabra: Discover a 2-Cost minion from the Minor (Tier 3) / Major (Tier 5)
        // Timewarp.
        (
            "BG35_MagicItem_823",
            Script {
                on_attach: Some(|c, me| discover_timewarp(c, me, 3)),
                ..E
            },
        ),
        (
            "BG35_MagicItem_823t",
            Script {
                on_attach: Some(|c, me| discover_timewarp(c, me, 5)),
                ..E
            },
        ),
        // Ophidian Staff: Spellcraft: give a Beast +2/+2 and Reborn.
        (
            "BG35_MagicItem_872",
            spellcraft_trinket!("BG35_MagicItem_872t"),
        ),
        (
            "BG35_MagicItem_872t",
            Script {
                spell: Some(|c, _, t| {
                    c.buff(t, 2, 2);
                    c.give_kw(t, Kw::REBORN);
                }),
                spell_target: TargetSpec::FriendlyRace(Races::BEAST),
                ..E
            },
        ),
        // Floating Candle Set: Spellcraft: trigger a friendly minion's Deathrattle.
        (
            "BG36_MagicItem_208",
            spellcraft_trinket!("BG36_MagicItem_208t"),
        ),
        (
            "BG36_MagicItem_208t",
            Script {
                spell: Some(|c, me, t| c.trigger_deathrattles(me.side, t)),
                spell_target: TargetSpec::FriendlyHas(crate::script::Hook::Deathrattle),
                ..E
            },
        ),
        // Shath'Yar Shrine: after you discard a spell, get a random Aberration.
        (
            "BG36_MagicItem_406",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Discarded { side, card } = e
                        && *side == me.side
                        && db().get(*card).kind != crate::cards::CardKind::Minion
                    {
                        get_random_of(c, me.side, Races::ABERRATION, 1);
                    }
                }),
                listens: ev::DISCARDED,
                ..E
            },
        ),
        // Writhing Tentacles: `writhing_tentacles` (called from the discard itself: the copy
        // needs the discarded minion, not only its card).
        (
            "BG36_MagicItem_606",
            Script {
                // "After you discard your first minion each turn, get a copy of it with double
                // stats. (1 left!)"
                shown: Some(|c, sh| vec![(1 - c.turn_data(sh.side, sh.card)).max(0)]),
                ..E
            },
        ),
        // Makeshift Master: Spellcraft: choose a minion; its recruit-phase stat gains this
        // turn also go to your Deity (`on_stat_gain`; turn data = eid + 1). Every cast is its
        // own effect: two spells (a Souvenir Stand copy's) on two minions both act, two on
        // one minion give the Deity its gains twice - one turn-data entry per cast.
        (
            "BG36_MagicItem_417",
            spellcraft_trinket!("BG36_MagicItem_417t"),
        ),
        (
            "BG36_MagicItem_417t",
            Script {
                spell: Some(|c, me, t| {
                    if let Some(p) = c.player(me.side) {
                        p.turn_data.push((me.card.base(), t as i32 + 1));
                    }
                }),
                spell_target: TargetSpec::FriendlyOrTavern,
                ..E
            },
        ),
    ]
}
