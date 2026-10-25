//! Timewarp cards (elementals): the Timewarped Tavern's own pool (docs/TIMEWARP.md).

use super::*;

const TAVERN_TEMPEST: &str = "BGS_123";
const SELLEMENTAL: &str = "BGS_115";
const TEMPERATURE_SHIFT: &str = "BG31_819";
const STONE_DRAKE: &str = "BG34_Giant_675";

/// The minion another friendly Played event is about (not the listener itself).
fn played_other(me: Me, e: &Event) -> Option<EntityId> {
    match e {
        Event::Played { side, eid, played } if *side == me.side && played.source != me.eid => {
            Some(*eid)
        }
        _ => None,
    }
}

/// Add one extra Tavern spell (up to the player's Tier) to the Tavern; in a full Tavern it
/// takes the place of a card of the default fill (`shop_make_room`).
fn add_shop_spell(c: &mut Ctx, side: Side) {
    let Some(p) = c.owner[side] else { return };
    let tier = c.tier(side);
    if !c.shop_make_room(side) {
        return;
    }
    if let Some(s) = c.draw_pool_spell(|d| d.tier <= tier) {
        let sid = c.g.alloc_eid();
        c.g.players[p]
            .shop
            .push(crate::state::ShopSlot::new(sid, Item::spell(s)));
    }
}

/// `n` random Elementals of the player's Tier or lower to hand.
fn give_random_elementals(c: &mut Ctx, side: Side, n: i32) {
    let tier = c.tier(side);
    for _ in 0..n {
        if let Some(card) =
            c.random_minion(|d| d.races.intersects(Races::ELEMENTAL) && d.tier <= tier)
        {
            c.give_card(side, card);
        }
    }
}

/// The player's first Stone Drake (warband left to right, then hand): the one that keeps
/// the "sold this turn" tally, so that several Drakes don't count a sale more than once.
fn first_stone_drake(c: &Ctx, side: Side) -> Option<EntityId> {
    let drake = cid(STONE_DRAKE);
    let is = |m: &Minion| db().normal_of(m.card) == drake;
    c.boards[side]
        .iter()
        .find(|m| is(m))
        .map(|m| m.eid)
        .or_else(|| {
            c.player_ref(side)?
                .hand
                .iter()
                .filter_map(|h| h.item.as_minion())
                .find(|m| is(m))
                .map(|m| m.eid)
        })
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Timewarped Molten Rock: after you play an Elemental, gain +1/+1 (+2/+2) and improve
        // this (by the base amount; counter = improvements).
        (
            "BG34_Giant_006",
            Script {
                on_event: Some(|c, me, e| {
                    if !matches!(e, Event::Played { side, played, .. }
                        if *side == me.side && played.source != me.eid && played.races.intersects(Races::ELEMENTAL))
                    {
                        return;
                    }
                    let Some(n) = c.minion(me.eid).map(|m| m.counter) else {
                        return;
                    };
                    let k = me.g().saturating_add(n);
                    c.buff(me.eid, k, k);
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = m.counter.saturating_add(me.g());
                    }
                }),
                listens: ev::PLAYED,
                // "After you play an Elemental, gain +1/+1 and improve this."
                shown: Some(|_, sh| {
                    let k = sh.g().saturating_add(sh.counter());
                    vec![k, k]
                }),
                flags: flag::IMPROVES,
                ..E
            },
        ),
        // Timewarped Cyclone: Divine Shield, Windfury, Reborn (keywords only).
        ("BG34_Giant_012", E),
        // Timewarped Ichoron: whenever you play a minion, give it Divine Shield (permanent:
        // no "until next turn" in the text; same for the golden).
        (
            "BG34_Giant_040",
            Script {
                on_event: Some(|c, me, e| {
                    if let Some(t) = played_other(me, e) {
                        c.give_kw(t, Kw::DIVINE_SHIELD);
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Timewarped Lubber: the Tavern always offers 1 (2) extra Tavern spell(s).
        // "Always" = whenever the Tavern is Refreshed, like Land Lubber.
        // "Your Tavern spells give an extra +1/+1 (+2/+2)" is a board aura: `Ctx::spell_extra`.
        (
            "BG34_Giant_066",
            Script {
                on_fill_shop: Some(|c, me| {
                    for _ in 0..me.g() {
                        add_shop_spell(c, me.side);
                    }
                }),
                ..E
            },
        ),
        // Timewarped Swirler: "Your Elementals give an extra +3/+3 (+6/+6) this game." Applied when
        // it is played (the text has no trigger word); the bonus itself is `Ctx::elemental_extra`.
        (
            "BG34_Giant_686",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Played { side, played, .. } if *side == me.side && played.source == me.eid)
                    {
                        let k = 3 * me.g();
                        c.bump_player_stat(me.side, |st| {
                            st.elem_give_atk += k;
                            st.elem_give_hp += k;
                        });
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Timewarped Sea Glass: Divine Shield. Rally: double (golden: triple) this minion's
        // stats, 2 times per combat (counted on the combat copy).
        (
            "BG34_Giant_110",
            Script {
                rally: Some(|c, me, _| {
                    let Some(m) = c.minion_mut(me.eid) else {
                        return;
                    };
                    if m.counter >= 2 {
                        return;
                    }
                    m.counter += 1;
                    let (a, h) = (m.atk, m.hp);
                    c.buff(me.eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                }),
                ..E
            },
        ),
        // Timewarped Sellemental: at the end of your turn, get 1 (2) Sellemental(s).
        (
            "BG34_Giant_067",
            Script {
                end_of_turn: Some(|c, me| give_n(c, me.side, SELLEMENTAL, me.g() as usize)),
                ..E
            },
        ),
        // Timewarped Ragnaros: Start of Combat: deal this minion's Attack (double) to the
        // highest-Health enemy minion.
        (
            "BG34_Giant_580",
            Script {
                start_of_combat: Some(|c, me| {
                    let Some(a) = c.minion(me.eid).map(|m| m.atk) else {
                        return;
                    };
                    let keyed: Vec<(EntityId, i64)> = c.boards[1 - me.side]
                        .iter()
                        .filter(|m| m.alive())
                        .map(|m| (m.eid, m.hp as i64))
                        .collect();
                    if let Some(t) = c.pick_max(keyed) {
                        c.damage(me.eid, t, a.saturating_mul(me.g()));
                        c.resolve_deaths();
                    }
                }),
                ..E
            },
        ),
        // Timewarped Snow Elemental: the Tavern offers 1 (2) extra Frozen Elemental(s)
        // whenever it is Refreshed (same as the Buddy, buddies_c.rs).
        (
            "BG34_Giant_586",
            Script {
                on_fill_shop: Some(|c, me| {
                    let t = c.tier(me.side);
                    for _ in 0..me.g() {
                        if let Some(sid) = c.add_shop_minion(me.side, 1, t, Races::ELEMENTAL)
                            && let Some(p) = c.owner[me.side]
                            && let Some(s) = c.g.players[p].shop.iter_mut().find(|s| s.sid == sid)
                        {
                            s.frozen = true;
                        }
                    }
                }),
                ..E
            },
        ),
        // Timewarped Stone Drake: Start of Combat: gain the stats (double) of all the minions
        // you sold this turn. The tally is per player (`turn_data`: Attack under the normal
        // card id, Health under the golden one) and only counts sales made while a Drake is
        // in the warband or in hand.
        (
            "BG34_Giant_675",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Sold { side, atk, hp, .. } = e else {
                        return;
                    };
                    if *side != me.side || first_stone_drake(c, me.side) != Some(me.eid) {
                        return;
                    }
                    let n = cid(STONE_DRAKE);
                    c.add_turn_data(me.side, n, (*atk).max(0));
                    c.add_turn_data(me.side, db().golden_of(n), (*hp).max(0));
                }),
                listens: ev::SOLD,
                listens_in_hand: true,
                start_of_combat: Some(|c, me| {
                    let n = cid(STONE_DRAKE);
                    let a = c.turn_data(me.side, n);
                    let h = c.turn_data(me.side, db().golden_of(n));
                    if a > 0 || h > 0 {
                        c.buff(me.eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                    }
                }),
                // "Start of Combat: Gain the stats of all the minions you sold this turn. (0/0)"
                // (golden: "Gain double the stats..."): the tally itself.
                shown: Some(|c, sh| {
                    let n = cid(STONE_DRAKE);
                    vec![
                        c.turn_data(sh.side, n),
                        c.turn_data(sh.side, db().golden_of(n)),
                    ]
                }),
                ..E
            },
        ),
        // Timewarped Trumpeter: after you sell 5 Elementals, get 1 (2) random Elemental(s)
        // (counter = Elementals sold; the sold minion's card types decide).
        (
            "BG34_Giant_676",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Sold { side, card, .. } = e else {
                        return;
                    };
                    if *side != me.side || !db().get(*card).races.intersects(Races::ELEMENTAL) {
                        return;
                    }
                    let Some(m) = c.minion_mut(me.eid) else {
                        return;
                    };
                    m.counter += 1;
                    if m.counter < 5 {
                        return;
                    }
                    m.counter = 0;
                    give_random_elementals(c, me.side, me.g());
                }),
                listens: ev::SOLD,
                // "After you sell 5 Elementals, get a random Elemental. (5 left!)"
                shown: Some(|_, sh| vec![5, (5 - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Timewarped Behemoth: Taunt. After you buy an Elemental, gain its stats (double).
        (
            "BG34_Giant_777",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Bought { side, eid, .. } = e else {
                        return;
                    };
                    if *side != me.side {
                        return;
                    }
                    // The bought card is in hand now (`eid` = its hand id).
                    let Some((a, h)) = c
                        .player_ref(me.side)
                        .and_then(|p| p.hand.iter().find(|h| h.hid == *eid))
                        .and_then(|h| h.item.as_minion())
                        .filter(|m| m.is(Races::ELEMENTAL))
                        .map(|m| (m.atk, m.hp))
                    else {
                        return;
                    };
                    c.buff(me.eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Timewarped Stormcloud: Deathrattle and Avenge (3): get 1 (2) Tavern Tempest(s).
        (
            "BG34_PreMadeChamp_031",
            Script {
                deathrattle: Some(|c, me| give_n(c, me.side, TAVERN_TEMPEST, me.g() as usize)),
                avenge: Some((3, 3, |c, me| {
                    give_n(c, me.side, TAVERN_TEMPEST, me.g() as usize)
                })),
                ..E
            },
        ),
        // Timewarped Substrate: Divine Shield. At the end of your turn, get 1 (2)
        // Temperature Shift(s).
        (
            "BG34_PreMadeChamp_032",
            Script {
                end_of_turn: Some(|c, me| give_n(c, me.side, TEMPERATURE_SHIFT, me.g() as usize)),
                ..E
            },
        ),
    ]
}
