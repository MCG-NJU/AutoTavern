//! Heroes: targeted and other active Hero Powers.

use super::*;

fn active(spec: TargetSpec, f: crate::script::FxT) -> Script {
    Script {
        hero_power: Some(f),
        hero_power_target: spec,
        ..E
    }
}

fn slot_in_shop(c: &Ctx, side: Side, sid: EntityId) -> Option<usize> {
    let p = c.owner[side]?;
    c.g.players[p].shop.iter().position(|s| s.sid == sid)
}

/// Put `m` into the Tavern at `pos` (clamped).
fn shop_insert(c: &mut Ctx, side: Side, pos: usize, m: Minion) {
    let Some(p) = c.owner[side] else { return };
    let sid = m.eid;
    let shop = &mut c.g.players[p].shop;
    let pos = pos.min(shop.len());
    shop.insert(pos, crate::state::ShopSlot::new(sid, Item::Minion(m)));
}

fn inge_form(c: &mut Ctx, me: Me) {
    let card = cid(if c.g.turn % 2 == 1 {
        "BG26_HERO_102p"
    } else {
        "BG26_HERO_102p2"
    });
    if let Some(hp) = c.power_mut(me.side, me.card) {
        hp.card = card;
    }
}

fn inge() -> Script {
    Script {
        on_attach: Some(inge_form),
        start_of_turn: Some(inge_form),
        hero_power: Some(|c, me, t| {
            let k = c.tier(me.side) as i32;
            if c.g.turn % 2 == 1 {
                c.buff(t, k, 0);
            } else {
                c.buff(t, 0, k);
            }
        }),
        hero_power_target: TargetSpec::Friendly,
        hero_power_uses: 2,
        ..E
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Xyrella: choose a minion in the Tavern; set its stats to 2 and add it to your hand.
        (
            "BG20_HERO_101p",
            active(TargetSpec::Tavern, |c, me, t| {
                if let Some(mut m) = c.take_from_shop(me.side, t) {
                    m.set_base_stats(2, 2);
                    c.add_to_hand(me.side, Item::Minion(m));
                }
            }),
        ),
        // Vol'jin: choose 2 minions; they gain each other's Attack until next turn (28.2 text:
        // each gets the other's Attack on top of its own, both read before either changes).
        (
            "BG20_HERO_201p",
            Script {
                hero_power: Some(|c, me, t| {
                    c.ask_followup(me.side, me.card, t, 0, TargetSpec::FriendlyOrTavern)
                }),
                hero_power_target: TargetSpec::FriendlyOrTavern,
                followup: Some(|c, me, t| {
                    let (a, b) = (me.eid, t);
                    let (Some(x), Some(y)) =
                        (c.find_any(a).map(|m| m.atk), c.find_any(b).map(|m| m.atk))
                    else {
                        return;
                    };
                    c.buff_temp(a, y, 0);
                    c.buff_temp(b, x, 0);
                }),
                ..E
            },
        ),
        // Mutanus: sell a friendly minion; spit its stats onto another. The devoured minion is
        // chosen, the one spat on is random: Nightmare Ectoplasm's extra recipients too, all
        // different.
        (
            "BG20_HERO_301p",
            Script {
                hero_power: Some(|c, me, t| {
                    let Some((_, i)) = c.locate(t) else { return };
                    let (a, h) = (c.boards[me.side][i].atk, c.boards[me.side][i].max_hp);
                    // Nightmare Ectoplasm: spit onto 1 (golden: 2) extra minions.
                    let extra = {
                        let m = &c.boards[me.side][i];
                        if m.is_card("BG20_HERO_301_Buddy") {
                            if m.golden { 2 } else { 1 }
                        } else {
                            0
                        }
                    };
                    c.sell_minion(me.side, i);
                    let mut others = c.friendly(me.side);
                    c.g.rng.shuffle(&mut others);
                    for t in others.into_iter().take(1 + extra) {
                        c.buff(t, a, h);
                    }
                }),
                hero_power_target: TargetSpec::Friendly,
                ..E
            },
        ),
        // Inge: twice per turn, give a minion Attack equal to your Tier (Health on alternate
        // turns, starting with Attack).
        ("BG26_HERO_102p", inge()),
        ("BG26_HERO_102p2", inge()),
        // Zerek: once per game, summon an exact copy of a friendly minion.
        (
            "BG31_HERO_005p",
            active(TargetSpec::Friendly, |c, me, t| {
                let Some(orig) = c.minion(t).cloned() else {
                    return;
                };
                // An exact copy: new id, one pool copy.
                let m = c.full_copy(&orig);
                let pos = c.pos_right_of(t);
                c.summon(me.side, pos, m);
            }),
        ),
        // Galakrond: choose a Tavern minion; Discover a higher-Tier minion to replace it
        // (`after_discover` puts the pick back in the Tavern at the same slot).
        (
            "TB_BaconShop_HP_011",
            active(TargetSpec::Tavern, |c, me, t| {
                let Some(pos) = slot_in_shop(c, me.side, t) else {
                    return;
                };
                let Some(m) = c.take_from_shop(me.side, t) else {
                    return;
                };
                let tier = (m.tier + 1).min(6);
                c.g.pool.return_minion(&m);
                if let Some(h) = c.power_mut(me.side, me.card) {
                    h.counter = pos as i32;
                }
                c.discover_minion(me.side, me.card, 3, |d| d.tier == tier);
            }),
        ),
        // Millificent Manastorm: Discover a Magnetic Mech (unlocks at Tier 4).
        (
            "TB_BaconShop_HP_015",
            active(TargetSpec::None, |c, me, _| {
                let tier = c.tier(me.side);
                c.discover_minion(me.side, me.card, 3, |d| {
                    d.races.intersects(Races::MECH) && d.kw.has(Kw::MAGNETIC) && d.tier <= tier
                });
            }),
        ),
        // Alexstrasza: Discover a Dragon (unlocks at Tier 4).
        (
            "TB_BaconShop_HP_064",
            active(TargetSpec::None, |c, me, _| {
                let tier = c.tier(me.side);
                c.discover_minion(me.side, me.card, 3, |d| {
                    d.races.intersects(Races::DRAGON) && d.tier <= tier
                });
            }),
        ),
        // The Jailer: destroy a friendly Undead to get a random Undead (unlocks at Tier 2).
        (
            "TB_BaconShop_HP_702t",
            active(TargetSpec::FriendlyRace(Races::UNDEAD), |c, me, t| {
                c.destroy(t);
                c.resolve_deaths();
                let tier = c.tier(me.side);
                if let Some(u) =
                    c.random_minion(|d| d.races.intersects(Races::UNDEAD) && d.tier <= tier)
                {
                    c.give_card(me.side, u);
                }
            }),
        ),
        // Jandice Barov: swap a friendly non-Golden minion with a random one in the Tavern.
        (
            "TB_BaconShop_HP_084",
            Script {
                hero_power: Some(|c, me, t| {
                    let tav = c.tavern_minions(me.side);
                    let Some(&sid) = c.g.rng.pick(&tav) else {
                        return;
                    };
                    let Some((s, i)) = c.locate(t) else { return };
                    let Some(pos) = slot_in_shop(c, me.side, sid) else {
                        return;
                    };
                    let Some(mut incoming) = c.take_from_shop(me.side, sid) else {
                        return;
                    };
                    let outgoing = std::mem::replace(&mut c.boards[s][i], {
                        incoming.attacked = false;
                        incoming
                    });
                    shop_insert(c, me.side, pos, outgoing);
                }),
                hero_power_target: TargetSpec::FriendlyNonGoldenMaxTier(7),
                ..E
            },
        ),
        // Malygos: replace a minion (friendly or in the Tavern) with a random one of the same
        // Tier (twice per turn).
        (
            "TB_BaconShop_HP_052",
            Script {
                hero_power: Some(|c, me, t| {
                    // A Tavern spell becomes a random spell of the same Tier.
                    if let Some(p) = c.owner[me.side]
                        && let Some(i) = c.g.players[p].shop.iter().position(|s| s.sid == t)
                        && let Item::Spell { card, .. } = c.g.players[p].shop[i].item
                    {
                        let tier = db().get(card).tier;
                        if let Some(new) = c.random_spell(|d| d.tier == tier && d.cid != card) {
                            c.g.players[p].shop[i].item = Item::spell(new);
                            c.g.players[p].shop[i].frozen = false;
                        }
                        return;
                    }
                    let Some((tier, card, pool)) =
                        c.find_any(t).map(|m| (m.tier, m.card, m.pool_entries()))
                    else {
                        return;
                    };
                    // Nexus Lord (Buddy): one (golden: two) Tiers higher.
                    let up = c.boards[me.side]
                        .iter()
                        .filter(|m| m.is_card("TB_BaconShop_HERO_58_Buddy"))
                        .map(|m| if m.golden { 2 } else { 1 })
                        .max()
                        .unwrap_or(0);
                    let tier = (tier + up).min(6);
                    let Some(new) =
                        c.random_minion(|d| d.tier == tier && d.cid != db().normal_of(card))
                    else {
                        return;
                    };
                    for (card, copies) in pool {
                        c.g.pool.put(card, copies);
                    }
                    let mut m = c.new_minion(me.side, new);
                    c.take_pool_copy(&mut m);
                    if c.g.players[c.owner[me.side].unwrap_or(0)]
                        .shop
                        .iter()
                        .any(|s| s.sid == t)
                    {
                        c.apply_tavern_buffs(me.side, &mut m);
                    }
                    if let Some(slot) = c.find_any_mut(t) {
                        m.eid = slot.eid;
                        *slot = m;
                    }
                    c.unfreeze_shop(t); // a transformed Tavern card isn't frozen
                }),
                hero_power_target: TargetSpec::FriendlyOrTavernAny,
                hero_power_uses: 2,
                ..E
            },
        ),
        // Maiev Shadowsong: take a Tavern card into your hand, locked for 2 turns.
        (
            "TB_BaconShop_HP_068",
            // Any Tavern card, spells included.
            active(TargetSpec::TavernAny, |c, me, t| {
                let Some(mut item) = c.take_card_from_shop(me.side, t) else {
                    return;
                };
                // Shadow Warden (Buddy): the next Hero Power(s) make the target Golden.
                let warden = cid("TB_BaconShop_HERO_62_Buddy");
                let left = c.card_data(me.side, warden);
                if left > 0
                    && let Item::Minion(m) = &mut item
                {
                    c.set_card_data(me.side, warden, left - 1);
                    make_golden(m);
                }
                // The card is locked the moment it arrives: it cannot complete a triple on
                // the way in (a locked card takes no part in triples, cannot be played or
                // discarded, but still gets buffs given to minions in hand).
                let prev = std::mem::replace(&mut c.no_triples, true);
                let hid = c.add_to_hand(me.side, item);
                c.no_triples = prev;
                if let Some(hid) = hid
                    && let Some(h) = c
                        .player(me.side)
                        .and_then(|p| p.hand.iter_mut().find(|h| h.hid == hid))
                {
                    h.locked = 2;
                }
            }),
        ),
        // Captain Hooktusk: remove a friendly minion; Discover one from a Tier lower.
        (
            "TB_BaconShop_HP_075",
            active(TargetSpec::Friendly, |c, me, t| {
                let Some((s, i)) = c.locate(t) else { return };
                let m = c.boards[s].remove(i);
                c.g.pool.return_minion(&m);
                // Raging Contender (Buddy): gain Gold equal to its Tier (golden: twice).
                let k: i32 = c.boards[me.side]
                    .iter()
                    .filter(|x| x.is_card("TB_BaconShop_HERO_67_Buddy"))
                    .map(|x| if x.golden { 2 } else { 1 })
                    .sum();
                if k > 0 {
                    c.gain_gold(me.side, k * m.tier as i32);
                }
                let tier = m.tier.saturating_sub(1).max(1);
                c.discover_minion(me.side, me.card, 3, |d| d.tier == tier);
            }),
        ),
        // The Rat King: Discover a minion of a specific type; the type changes each turn.
        (
            "TB_BaconShop_HP_041",
            active(TargetSpec::None, |c, me, _| {
                let Some(r) = c.g.rat_king_type() else { return };
                let tier = c.tier(me.side);
                c.discover_minion(me.side, me.card, 3, |d| {
                    d.races.intersects(r) && d.tier <= tier
                });
            }),
        ),
        // Sylvanas Windrunner (Reclaimed Souls): Discover a plain copy of a minion that died last
        // combat (unlocks on Turn 3: parsed from the text). The text doesn't say "friendly":
        // minions of both sides count. `memory` = the distinct minions that died in the last
        // combat.
        (
            "BG23_HERO_306p",
            Script {
                hero_power: Some(|c, me, _| {
                    let mut cards = c
                        .power_mut(me.side, me.card)
                        .map(|h| h.memory.clone())
                        .unwrap_or_default();
                    c.g.rng.shuffle(&mut cards);
                    let options: Vec<Item> = cards
                        .into_iter()
                        .take(3)
                        .map(|card| Item::Minion(c.new_minion(me.side, card)))
                        .collect();
                    c.offer_discover(me.side, me.card, options);
                }),
                start_of_combat: Some(|c, me| {
                    if let Some(h) = c.power_mut(me.side, me.card) {
                        h.memory.clear();
                    }
                }),
                on_event: Some(|c, me, e| {
                    let Event::Died { card, .. } = e else { return };
                    if !c.combat {
                        return;
                    }
                    let plain = db().normal_of(*card);
                    if let Some(h) = c.power_mut(me.side, me.card)
                        && !h.memory.contains(&plain)
                    {
                        h.memory.push(plain);
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Tess Greymane: Refresh the Tavern with plain copies of your last opponent's warband.
        (
            "TB_BaconShop_HP_077",
            active(TargetSpec::None, |c, me, _| {
                let Some(p) = c.owner[me.side] else { return };
                let Some(o) = c.g.last_fought(p) else {
                    return;
                };
                let cards: Vec<CardId> =
                    c.g.fought_board(o)
                        .iter()
                        .map(|m| db().normal_of(m.card))
                        .collect();
                if cards.is_empty() {
                    return;
                }
                c.clear_shop(me.side);
                for card in cards {
                    let mut m = c.new_minion(me.side, card);
                    c.take_pool_copy(&mut m);
                    c.apply_tavern_buffs(me.side, &mut m);
                    let pos = c.g.players[p].shop.len();
                    shop_insert(c, me.side, pos, m);
                }
                // The opponent's warband is the whole Refresh: no other minion or spell is added.
                c.finish_refresh(me.side, false);
            }),
        ),
        // Infinite Toki: Refresh the Tavern, including two minions from a Tier higher.
        (
            "TB_BaconShop_HP_028",
            active(TargetSpec::None, |c, me, _| {
                c.refresh_shop(me.side);
                // Tier 7 only in an unlocked Tier 7 Tavern.
                let cap = if c.player_ref(me.side).is_some_and(|p| p.tier7) {
                    7
                } else {
                    6
                };
                let tier = (c.tier(me.side) + 1).min(cap);
                // Two more minions on top of the Refresh; in a full Tavern they take the place of
                // default cards like every extra offer.
                for _ in 0..2 {
                    c.add_shop_minion(me.side, tier, tier, Races::NONE);
                }
            }),
        ),
        // Time Twister Chromie: Refresh the Tavern with Tavern spells.
        (
            "BG34_HERO_001p",
            active(TargetSpec::None, |c, me, _| {
                c.refresh_shop_with(me.side, Races::NONE, true)
            }),
        ),
    ]
}
