//! Timewarp cards (spells): the Timewarped Tavern's own pool (docs/TIMEWARP.md).

use super::*;

const ANY: TargetSpec = TargetSpec::FriendlyOrTavern;

fn sp(f: crate::script::FxT) -> Script {
    Script {
        spell: Some(f),
        ..E
    }
}

fn tsp(spec: TargetSpec, f: crate::script::FxT) -> Script {
    Script {
        spell: Some(f),
        spell_target: spec,
        ..E
    }
}

/// "Get an X. Repeat at the start of each turn": the cast gives one and adds a "this game"
/// effect whose `start_of_turn` gives one per stack.
fn each_turn(f: crate::script::Fx) -> Script {
    Script {
        start_of_turn: Some(f),
        ..E
    }
}

fn cast_each_turn(c: &mut Ctx, me: Me, card: &str) {
    c.give_card(me.side, cid(card));
    c.add_player_effect(me.side, me.card, 1);
}

/// Discover a card of Chronum cost `cost` from the Minor (Tier 3) / Major (Tier 5) Timewarp,
/// among the cards a visit would offer this player (`timewarp::discover_cards`, as Timeworn
/// Candelabra).
fn discover_timewarp(c: &mut Ctx, me: Me, tier: u8, cost: i32, minions_only: bool) {
    let cands = super::timewarp::discover_cards(c, me.side, tier == 5, cost, 3, minions_only);
    let options = cands
        .into_iter()
        .map(|x| {
            if db().get(x).is_minion() {
                Item::Minion(c.new_minion(me.side, x))
            } else {
                Item::spell(x)
            }
        })
        .collect();
    c.offer_discover(me.side, me.card, options);
}

/// Discover among pool minions matching `f` (by copies left), each option adjusted by `make`.
fn discover_made(
    c: &mut Ctx,
    me: Me,
    f: impl Fn(&crate::cards::CardDef) -> bool,
    make: impl Fn(&mut Ctx, CardId) -> Minion,
) {
    let cands: Vec<(CardId, u64)> =
        c.g.universe
            .iter()
            .copied()
            .filter(|x| f(db().get(*x)))
            .map(|x| (x, c.g.pool.get(x) as u64))
            .collect();
    let options = c
        .weighted_distinct(cands, 3)
        .into_iter()
        .map(|x| Item::Minion(make(c, x)))
        .collect();
    c.offer_discover(me.side, me.card, options);
}

/// Summon an exact copy of `t` to its right.
fn summon_copy(c: &mut Ctx, me: Me, t: EntityId) {
    let Some(orig) = c.minion(t).cloned() else {
        return;
    };
    let m = c.full_copy(&orig);
    let pos = c.pos_right_of(t);
    c.summon(me.side, pos, m);
}

/// Timewarped New Recruit: top the Tavern up to its 7 cards with minions.
fn top_up_tavern(c: &mut Ctx, side: Side) {
    let max = c.g.cfg.shop_max_cards;
    let tier = c.tier(side);
    while c.player_ref(side).is_some_and(|p| p.shop.len() < max) {
        if c.add_shop_minion(side, 1, tier, Races::NONE).is_none() {
            break;
        }
    }
}

const BIG_WINNER_TURNS: u32 = 3;

fn big_winner(c: &mut Ctx, me: Me) {
    darkmoon::discover_prize(c, me.side, me.card, 3);
    schedule(
        c,
        me.side,
        Scheduled {
            turns_left: BIG_WINNER_TURNS,
            script: me.card,
            target: 0,
            a: 0,
            b: 0,
            if_won: false,
        },
    );
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Timewarped Beanstalk: Discover a 1-Cost card from the Major Timewarp. Lock it in
        // your hand for 1 turn.
        (
            "BG34_Treasure_301",
            // (The lock is applied in `after_discover`, like Search Through Time.)
            sp(|c, me, _| discover_timewarp(c, me, 5, 1, false)),
        ),
        // Timewarped Cloning Device: choose a friendly minion; summon an exact copy of it.
        ("BG34_Treasure_302", tsp(TargetSpec::Friendly, summon_copy)),
        // Timewarped Big Winner!: Discover a Tier 3 Darkmoon Prize. Repeat at the start of
        // every three turns (each cast runs its own 3-turn cycle from the turn it was cast).
        (
            "BG34_Treasure_606",
            Script {
                spell: Some(|c, me, _| big_winner(c, me)),
                scheduled: Some(|c, me, _| big_winner(c, me)),
                ..E
            },
        ),
        // Timewarped Trainee: get a random minion each from Tiers 1, 2 and 3.
        (
            "BG34_Treasure_607",
            sp(|c, me, _| {
                for t in 1..=3 {
                    if let Some(x) = c.random_minion(|d| d.tier == t) {
                        c.give_card(me.side, x);
                    }
                }
            }),
        ),
        // Timewarped Ring: get a Shiny Ring. Repeat at the start of each turn.
        (
            "BG34_Treasure_608",
            Script {
                spell: Some(|c, me, _| cast_each_turn(c, me, "BG28_168")),
                ..each_turn(|c, me| give_n(c, me.side, "BG28_168", 1))
            },
        ),
        // Timewarped Lasso: get an Enchanted Lasso. Repeat at the start of each turn.
        (
            "BG34_Treasure_609",
            Script {
                spell: Some(|c, me, _| cast_each_turn(c, me, "BG28_512")),
                ..each_turn(|c, me| give_n(c, me.side, "BG28_512", 1))
            },
        ),
        // Timewarped Apples: after the Tavern is Refreshed this game, cast Them Apples (your cast:
        // the text has no "this casts").
        (
            "BG34_Treasure_620",
            Script {
                spell: Some(|c, me, _| c.add_player_effect(me.side, me.card, 1)),
                ..refresh_effect(|c, me, e| {
                    if matches!(e, Event::Refreshed { side, .. } if *side == me.side) {
                        c.you_cast(me.side, cid("BG28_966"), 0, 0, 0);
                    }
                })
            },
        ),
        // Timewarped Secrecy: Discover a Golden Tier 7 minion.
        (
            "BG34_Treasure_625",
            sp(|c, me, _| {
                discover_made(
                    c,
                    me,
                    |d| d.pools.tier7 && d.golden.is_some(),
                    |c, x| c.new_minion(me.side, db().golden_of(x)),
                );
            }),
        ),
        // Timewarped Evolving Tavern: get an Evolving Tavern. Repeat at the start of each
        // turn.
        (
            "BG34_Treasure_900",
            Script {
                spell: Some(|c, me, _| cast_each_turn(c, me, "BGS_Treasures_006")),
                ..each_turn(|c, me| give_n(c, me.side, "BGS_Treasures_006", 1))
            },
        ),
        // Timewarped On the House: Discover 3 minions from your Tier (three Discovers).
        (
            "BG34_Treasure_903",
            sp(|c, me, _| {
                let tier = c.tier(me.side);
                for _ in 0..3 {
                    c.discover_minion(me.side, me.card, 3, |d| d.tier == tier);
                }
            }),
        ),
        // Timewarped Rat in a Cage: give a minion +2/+2, then double its stats (the +2/+2
        // gets the Tavern-spell extra; the doubling doesn't).
        (
            "BG34_Treasure_905",
            Script {
                // "Give a minion +2/+2, then double its stats."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 2, 2)),
                ..tsp(ANY, |c, me, t| {
                    spell_buff(c, me, t, 2, 2);
                    if let Some((a, h)) = c.find_any(t).map(|m| (m.atk, m.hp)) {
                        c.buff(t, a, h);
                    }
                })
            },
        ),
        // Timewarped B.A.N.A.N.A.S.: fill your hand with Tavern Dish Bananas. Your Tavern
        // spells give an extra +3/+3 this game.
        (
            "BG34_Treasure_912",
            sp(|c, me, _| {
                let size = c.g.cfg.hand_size;
                while c.player_ref(me.side).is_some_and(|p| p.hand.len() < size) {
                    if c.give_card(me.side, cid("BG28_897")).is_none() {
                        break;
                    }
                }
                if let Some(p) = c.player(me.side) {
                    p.stats.spell_atk = p.stats.spell_atk.saturating_add(3);
                    p.stats.spell_hp = p.stats.spell_hp.saturating_add(3);
                }
            }),
        ),
        // Timewarped New Recruit: give minions in the Tavern +2/+2 this game (with the Tavern-spell
        // extra). The Tavern always has 7 cards this game: topped up with minions on cast and after
        // every Refresh, not after a purchase.
        (
            "BG34_Treasure_917",
            Script {
                spell: Some(|c, me, _| {
                    let (ea, eh) = c.spell_extra(me.side);
                    if let Some(p) = c.player(me.side) {
                        p.stats.tavern_atk =
                            p.stats.tavern_atk.saturating_add(ea.saturating_add(2));
                        p.stats.tavern_hp = p.stats.tavern_hp.saturating_add(eh.saturating_add(2));
                    }
                    c.buff_tavern(
                        me.side,
                        ea.saturating_add(2),
                        eh.saturating_add(2),
                        Races::NONE,
                    );
                    c.add_player_effect(me.side, me.card, 1);
                    top_up_tavern(c, me.side);
                }),
                on_fill_shop: Some(|c, me| top_up_tavern(c, me.side)),
                // "Give minions in the Tavern +2/+2 this game. The Tavern always has 7 cards
                // this game."
                shown: Some(|c, sh| {
                    let (ea, eh) = c.spell_extra(sh.side);
                    vec![
                        ea.saturating_add(2),
                        eh.saturating_add(2),
                        c.g.cfg.shop_max_cards as i32,
                    ]
                }),
                ..E
            },
        ),
        // Timewarped Ritual: Discover two Tier 7 minions (two Discovers).
        (
            "BG34_Treasure_919",
            sp(|c, me, _| {
                for _ in 0..2 {
                    c.discover_tier7(me.side, me.card, 3);
                }
            }),
        ),
        // Timewarped Strike Oil: increase your maximum Gold by 4. Gain 4 Gold.
        (
            "BG34_Treasure_932",
            sp(|c, me, _| {
                c.add_max_gold(me.side, 4);
                c.gain_gold(me.side, 4);
            }),
        ),
        // Timewarped Evolution: choose a minion. Discover a Tier 6 minion to transform it
        // into. Set its stats to 30/30. (Data = the chosen minion + 1.)
        (
            "BG34_Treasure_933",
            Script {
                spell: Some(|c, me, t| {
                    offer_targeted_transform(c, me.side, me.card, t, 3, |d| d.tier == 6);
                }),
                spell_target: ANY,
                // The pick replaces the chosen minion; it never enters the hand.
                discover_pick: Some(|c, side, source, item| {
                    if let Some(t) = take_transform_target(c, side, source) {
                        trinkets_p8::transform_any(c, side, t, db().normal_of(item.card()));
                        let (a, h) = c.spell_extra(side);
                        c.set_stats(t, 30i32.saturating_add(a), 30i32.saturating_add(h));
                    }
                    None
                }),
                ..E
            },
        ),
        // Timewarped Armor Stash: Casts When Bought. Gain 10 Armor.
        (
            "BG34_Treasure_934",
            Script {
                spell: Some(|c, me, _| {
                    if let Some(p) = c.player(me.side) {
                        p.armor += 10;
                    }
                }),
                flags: flag::CAST_ON_BUY,
                ..E
            },
        ),
        // Timewarped Corpse: Discover a Deathrattle minion from Tier 5 or higher. Give it Reborn.
        // (Tier 7 only in a Tier 7 Tavern)
        (
            "BG34_Treasure_937",
            sp(|c, me, _| {
                let tier = c.tier(me.side);
                discover_made(
                    c,
                    me,
                    |d| d.mech.has(Mech::DEATHRATTLE) && d.tier >= 5 && (d.tier <= 6 || tier >= 7),
                    |c, x| {
                        let mut m = c.new_minion(me.side, x);
                        m.give(Kw::REBORN);
                        m
                    },
                );
            }),
        ),
        // Timewarped Chef's Choice: choose a minion. Get a random minion of the same type from
        // Tiers 4, 5 and 6 (one each; unlike Chef's Choice it may be the same minion).
        (
            "BG34_Treasure_940",
            tsp(ANY, |c, me, t| {
                let Some(r) = c.find_any(t).map(|m| m.races) else {
                    return;
                };
                let r = target_type(c, r);
                if r.is_none() {
                    return;
                }
                for tier in 4..=6 {
                    if let Some(x) = c.random_minion(|d| d.races.intersects(r) && d.tier == tier) {
                        c.give_card(me.side, x);
                    }
                }
            }),
        ),
        // Timewarped Conch: choose a friendly Murloc; summon an exact copy of it.
        (
            "BG34_Treasure_951",
            tsp(TargetSpec::FriendlyRace(Races::MURLOC), summon_copy),
        ),
        // Timewarped Revelation: Discover a 1-Cost and a 2-Cost minion from the Minor Timewarp
        // (two Discovers; the 1-Cost one is answered first).
        (
            "BG34_Treasure_953",
            sp(|c, me, _| {
                for cost in [2, 1] {
                    discover_timewarp(c, me, 3, cost, true);
                }
            }),
        ),
        // Timewarped Goldenizer: choose a minion; make it Golden. Only minions that can become
        // Golden are legal targets.
        (
            "BG34_Treasure_955",
            tsp(TargetSpec::AnyNonGolden, |c, _, t| {
                if let Some(m) = c.find_any_mut(t) {
                    make_golden(m);
                }
            }),
        ),
        // Timewarped Investment: Casts When Bought. Gain 1 extra Chronum at your next Timewarp.
        (
            "BG34_Treasure_300",
            Script {
                spell: Some(|c, me, _| {
                    if let Some(p) = c.player(me.side) {
                        p.chronum_saved += 1;
                    }
                }),
                flags: flag::CAST_ON_BUY,
                ..E
            },
        ),
    ]
}
