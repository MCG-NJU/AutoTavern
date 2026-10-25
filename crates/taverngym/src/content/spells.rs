//! Tavern spells and special spells (Blood Gem, Triple Reward, discard-linked spells).

use super::*;

const ANY: TargetSpec = TargetSpec::FriendlyOrTavern;

/// The size of Blood Gem Barrage's gem: +2/+2, the owner's Blood Gem bonus ("your Blood
/// Gems give an extra") and the Tavern-spell bonus.
fn barrage_gem(c: &Ctx, side: Side) -> (i32, i32) {
    let st = c
        .player_ref(side)
        .map(|p| (p.stats.gem_atk, p.stats.gem_hp));
    let (ga, gh) = st.unwrap_or((0, 0));
    let (ea, eh) = c.spell_extra(side);
    (
        ga.saturating_add(ea).saturating_add(2),
        gh.saturating_add(eh).saturating_add(2),
    )
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        ("BG27_Anomaly_574t2", sp(|c, me, _| c.gain_gold(me.side, 3))),
        ("BG_TB_006", tsp(ANY, |c, me, t| spell_buff(c, me, t, 2, 2))),
        // ------------------------------------------------------------------ economy
        // Easterly Winds: after the Tavern is Refreshed this game, a random minion +9/+9.
        (
            "BG34_444",
            Script {
                spell: Some(|c, me, _| c.add_player_effect(me.side, me.card, 1)),
                // "After the Tavern is Refreshed this game, give a random minion in it +9/+9."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 9, 9)),
                ..refresh_effect(|c, me, e| refresh_random_tavern_minion(c, me, e, 9, 9))
            },
        ),
        // Blood Gem Barrage: after the Tavern is Refreshed this game, give its minions +2/+2 worth
        // of Blood Gems: each Tavern minion gets ONE Blood Gem whose size is the card's +2/+2 plus
        // your Blood Gem bonus plus the Tavern-spell bonus.
        (
            "BG34_689",
            Script {
                spell: Some(|c, me, _| c.add_player_effect(me.side, me.card, 1)),
                // "... give its minions +2/+2 worth of Blood Gems."
                shown: Some(|c, sh| {
                    let (a, h) = barrage_gem(c, sh.side);
                    vec![a, h]
                }),
                ..refresh_effect(|c, me, e| {
                    if matches!(e, Event::Refreshed { side, .. } if *side == me.side) {
                        let (a, h) = barrage_gem(c, me.side);
                        for t in c.tavern_minions(me.side) {
                            c.play_gem_sized(me.side, t, a, h, false);
                        }
                    }
                })
            },
        ),
        ("BG28_810", sp(|c, me, _| c.gain_gold(me.side, 1))), // Tavern Coin
        ("BG28_800", sp(|c, me, _| c.gold_next_turn(me.side, 2))), // Careful Investment
        ("BG28_805", sp(|c, me, _| c.add_max_gold(me.side, 1))), // Strike Oil
        ("BG33_815", sp(|c, me, _| c.gain_gold(me.side, 2))), // Wealthy Bounty
        (
            "BG28_827", // Leaf Through the Pages
            sp(|c, me, _| {
                if let Some(p) = c.player(me.side) {
                    p.free_refreshes += 2;
                }
            }),
        ),
        (
            "BG28_500", // Armor Stash: "Set your Armor to 5" (also down from more).
            sp(|c, me, _| {
                if let Some(p) = c.player(me.side) {
                    p.armor = 5;
                }
            }),
        ),
        (
            "BG28_884", // Overconfidence
            Script {
                spell: Some(overconfidence),
                scheduled: Some(overconfidence_payout),
                ..E
            },
        ),
        // ------------------------------------------------------------------ targeted buffs
        (
            "BG28_897", // Tavern Dish Banana
            Script {
                shown: Some(|c, sh| shown_spell_stats(c, sh, 2, 2)),
                ..tsp(ANY, |c, me, t| spell_buff(c, me, t, 2, 2))
            },
        ),
        (
            "BG28_503", // Fortify
            Script {
                // "Give a minion +3 Health and Taunt."
                shown: Some(|c, sh| vec![c.spell_extra(sh.side).1.saturating_add(3)]),
                ..tsp(ANY, |c, me, t| {
                    spell_buff(c, me, t, 0, 3);
                    c.give_kw(t, Kw::TAUNT);
                })
            },
        ),
        (
            "BG28_825", // Defender's Rites
            Script {
                // "Give a minion +7/+7 and Taunt."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 7, 7)),
                ..tsp(ANY, |c, me, t| {
                    spell_buff(c, me, t, 7, 7);
                    c.give_kw(t, Kw::TAUNT);
                })
            },
        ),
        (
            "BG28_838", // Perfect Vision
            Script {
                // "Set a minion's stats to 20/20." The numbers of a Tavern spell take the
                // Tavern-spell bonus, a "set to" as well.
                shown: Some(|c, sh| shown_spell_stats(c, sh, 20, 20)),
                ..tsp(ANY, |c, me, t| {
                    let (ea, eh) = c.spell_extra(me.side);
                    c.set_stats(t, ea.saturating_add(20), eh.saturating_add(20))
                })
            },
        ),
        (
            "BG36_624", // Repair Job
            Script {
                // "Give a minion +4/+8."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 4, 8)),
                ..tsp(ANY, |c, me, t| spell_buff(c, me, t, 4, 8))
            },
        ),
        (
            "BG31_880", // Alliance Flag (Choose One)
            Script {
                spell: Some(|c, me, t| {
                    if me.opt_a() {
                        spell_buff(c, me, t, 3, 1);
                    }
                    if me.opt_b() {
                        spell_buff(c, me, t, 1, 3);
                    }
                }),
                spell_target: ANY,
                choose_one: true,
                // "Choose One - Give a minion +3/+1; or +1/+3."
                shown: Some(|c, sh| shown_spell_pairs(c, sh, &[(3, 1), (1, 3)])),
                ..E
            },
        ),
        (
            "BG36_883", // Winner's Bread
            Script {
                spell: Some(winners_bread),
                spell_target: TargetSpec::Friendly,
                // The delayed part gets the Tavern-spell extra too.
                scheduled: Some(|c, me, s| {
                    let (ea, eh) = c.spell_extra(me.side);
                    c.buff(me.eid, s.a.saturating_add(ea), s.b.saturating_add(eh))
                }),
                // "Give a minion +2/+3. If you win your next combat, give it another +4/+6 at
                // the start of your next turn."
                shown: Some(|c, sh| shown_spell_pairs(c, sh, &[(2, 3), (4, 6)])),
                ..E
            },
        ),
        (
            "BG20_GEM",
            Script {
                // "Give a minion +1/+1.": the current size of your Blood Gems (`gem_stats`).
                // The card is played from the hand, so Surveyor Portrait's extra counts.
                shown: Some(|c, sh| {
                    let (a, h) = c.gem_stats(sh.side);
                    let hand = !c.cast_from_hand && has_trinket(c, sh.side, "BG30_MagicItem_943");
                    let x = if hand { 6 } else { 0 };
                    vec![a + x, h + x]
                }),
                ..tsp(TargetSpec::Friendly, |c, me, t| {
                    c.play_gem(me.side, t);
                    // Jewelry Box gems (data 1..3): a Quilboar also gets Taunt / Divine Shield
                    // / Reborn.
                    let kw = match me.data {
                        1 => Kw::TAUNT,
                        2 => Kw::DIVINE_SHIELD,
                        3 => Kw::REBORN,
                        _ => return,
                    };
                    if c.find_any(t).is_some_and(|m| m.is(Races::QUILBOAR)) {
                        c.give_kw(t, kw);
                    }
                })
            },
        ), // Blood Gem
        // ------------------------------------------------------------------ board-wide
        (
            "BG28_168", // Shiny Ring
            Script {
                // "Give your minions +1/+1."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 1, 1)),
                ..sp(|c, me, _| all_friendly(c, me, 1, 1, 1))
            },
        ),
        (
            "BG28_169", // Azerite Empowerment
            Script {
                // "Give your minions +2/+2 twice."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 2, 2)),
                ..sp(|c, me, _| all_friendly(c, me, 2, 2, 2))
            },
        ),
        (
            "BG36_301t", // Sludge Corrosion
            Script {
                // "Give your minions +1/+1. If you discard this, cast it twice."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 1, 1)),
                ..sp(|c, me, _| all_friendly(c, me, 1, 1, 1))
            },
        ),
        (
            "BG33_812", // Hostile Bounty
            Script {
                // "Give four friendly minions +4 Attack."
                shown: Some(|c, sh| vec![c.spell_extra(sh.side).0.saturating_add(4)]),
                ..sp(|c, me, _| random_friendly_n(c, me, 4, 4, 0))
            },
        ),
        (
            "BG33_811", // Healthy Bounty
            Script {
                // "Give four friendly minions +4 Health."
                shown: Some(|c, sh| vec![c.spell_extra(sh.side).1.saturating_add(4)]),
                ..sp(|c, me, _| random_friendly_n(c, me, 4, 0, 4))
            },
        ),
        (
            "BG35_951", // Might of Stormwind
            Script {
                // "Give four random friendly minions +1/+2."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 1, 2)),
                ..sp(|c, me, _| random_friendly_n(c, me, 4, 1, 2))
            },
        ),
        (
            "BG33_813", // Selfish Bounty
            Script {
                // "Give your left-most minion +6/+6."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 6, 6)),
                ..sp(|c, me, _| {
                    if let Some(t) = c.leftmost(me.side) {
                        spell_buff(c, me, t, 6, 6);
                    }
                })
            },
        ),
        (
            "BG34_990", // Wave of Gold
            Script {
                // "Give your minions +3/+2. Give Golden ones another +3/+2."
                shown: Some(|c, sh| shown_spell_pairs(c, sh, &[(3, 2), (3, 2)])),
                ..sp(|c, me, _| {
                    for e in c.friendly(me.side) {
                        let golden = c.minion(e).is_some_and(|m| m.golden);
                        spell_buff(c, me, e, 3, 2);
                        if golden {
                            spell_buff(c, me, e, 3, 2);
                        }
                    }
                })
            },
        ),
        (
            "BG31_881", // Time Management (Choose One)
            Script {
                spell: Some(|c, me, _| {
                    if me.opt_a() {
                        all_friendly(c, me, 2, 2, 1);
                    }
                    if me.opt_b() {
                        schedule(
                            c,
                            me.side,
                            Scheduled {
                                turns_left: 1,
                                script: me.card,
                                target: 0,
                                a: 0,
                                b: 0,
                                if_won: false,
                            },
                        );
                    }
                }),
                choose_one: true,
                scheduled: Some(|c, me, _| all_friendly(c, me, 2, 2, 2)),
                // "Choose One - Give your minions +2/+2; or At the start of your next turn,
                // give your minions +2/+2 twice."
                shown: Some(|c, sh| shown_spell_pairs(c, sh, &[(2, 2), (2, 2)])),
                ..E
            },
        ),
        (
            "BG36_371", // Energizing Chamber
            Script {
                // "Give your Deity +7/+7. If you discard this, cast it twice." With the
                // Tavern-spell bonus like every numbered Tavern spell.
                shown: Some(|c, sh| shown_spell_stats(c, sh, 7, 7)),
                ..sp(|c, me, _| {
                    let (ea, eh) = c.spell_extra(me.side);
                    buff_deity(c, me.side, ea.saturating_add(7), eh.saturating_add(7))
                })
            },
        ),
        ("BG36_303", sp(|c, me, _| c.gain_gold(me.side, 2))), // Corrupted Coin
        // ------------------------------------------------------------------ tavern
        (
            "BG28_886", // Staff of Enrichment (with the Tavern-spell extra)
            Script {
                // "Give minions in the Tavern +2/+2 this game."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 2, 2)),
                ..sp(|c, me, _| {
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
                })
            },
        ),
        (
            "BG28_966",
            Script {
                // "Give minions in the Tavern +1/+2."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 1, 2)),
                ..sp(|c, me, _| {
                    let (ea, eh) = c.spell_extra(me.side);
                    c.buff_tavern(
                        me.side,
                        ea.saturating_add(1),
                        eh.saturating_add(2),
                        Races::NONE,
                    )
                })
            },
        ), // Them Apples (with the Tavern-spell extra)
        ("BG28_512", sp(enchanted_lasso)),
        ("BG28_830", sp(golden_touch)),
        // ------------------------------------------------------------------ generation
        (
            "BG28_504", // Recruit a Trainee
            sp(|c, me, _| {
                if let Some(card) = c.random_minion(|d| d.tier == 1) {
                    c.give_card(me.side, card);
                }
            }),
        ),
        (
            "BG33_101",
            sp(|c, me, _| c.discover_minion(me.side, me.card, 3, |d| d.tier == 1)),
        ), // A New Sprout
        (
            // Silas's Prize: "Discover a minion from Tier 0." - the Tier it was given at, in its
            // data (TB_BaconShop_HP_101).
            "TB_BaconShop_HP_101t2",
            Script {
                shown: Some(|_, sh| vec![if sh.data == 0 { 0 } else { sh.data.clamp(1, 7) }]),
                ..sp(|c, me, _| {
                    let t = me.data.clamp(1, 7) as u8;
                    let options = c.minion_options(me.side, 3, |d| d.tier == t);
                    if !options.is_empty() {
                        c.offer_discover(me.side, me.card, options);
                    }
                })
            },
        ),
        (
            "TB_BaconShop_Triples_01", // Triple Reward: Discover a minion of tier `data`
            Script {
                // "Discover a minion from Tier 0.": the Tier this reward was made for.
                shown: Some(|_, sh| vec![if sh.data == 0 { 0 } else { sh.data.clamp(1, 7) }]),
                ..sp(|c, me, _| {
                    // Tier 7 only comes from a Tier 7 Tavern.
                    let t = me.data.clamp(1, 7) as u8;
                    let options = c.minion_options(me.side, 3, |d| d.tier == t);
                    if options.is_empty() {
                        c.give_card(me.side, cid("BG27_Anomaly_574t2"));
                    } else {
                        c.offer_discover(me.side, me.card, options);
                    }
                })
            },
        ),
    ]
}

/// `Script::shown` of a Tavern spell whose text has several "+atk/+hp" pairs that all go
/// through `Ctx::spell_buff` / `Ctx::spell_extra`: each pair with the Tavern-spell extra.
pub fn shown_spell_pairs(c: &Ctx, sh: &crate::script::Shown, pairs: &[(i32, i32)]) -> Vec<i32> {
    let (a, h) = c.spell_extra(sh.side);
    pairs
        .iter()
        .flat_map(|p| [p.0.saturating_add(a), p.1.saturating_add(h)])
        .collect()
}

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

fn all_friendly(c: &mut Ctx, me: Me, a: i32, h: i32, times: u32) {
    for _ in 0..times {
        for e in c.friendly(me.side) {
            spell_buff(c, me, e, a, h);
        }
    }
}

fn random_friendly_n(c: &mut Ctx, me: Me, n: usize, a: i32, h: i32) {
    let mut f = c.friendly(me.side);
    c.g.rng.shuffle(&mut f);
    for e in f.into_iter().take(n) {
        spell_buff(c, me, e, a, h);
    }
}

fn overconfidence(c: &mut Ctx, me: Me, _t: EntityId) {
    schedule(
        c,
        me.side,
        Scheduled {
            turns_left: 1,
            script: me.card,
            target: 0,
            a: 0,
            b: 0,
            if_won: false,
        },
    );
}

fn overconfidence_payout(c: &mut Ctx, me: Me, _s: &Scheduled) {
    let Some(p) = c.player_ref(me.side) else {
        return;
    };
    let n = if p.stats.won_last {
        3
    } else if p.stats.tied_last {
        1
    } else {
        0
    };
    c.gain_gold(me.side, n);
}

fn winners_bread(c: &mut Ctx, me: Me, t: EntityId) {
    spell_buff(c, me, t, 2, 3);
    schedule(
        c,
        me.side,
        Scheduled {
            turns_left: 1,
            script: me.card,
            target: t,
            a: 4,
            b: 6,
            if_won: true,
        },
    );
}

fn enchanted_lasso(c: &mut Ctx, me: Me, _t: EntityId) {
    let cands = c.tavern_minions(me.side);
    let Some(&sid) = c.g.rng.pick(&cands) else {
        return;
    };
    if let Some(m) = c.take_from_shop(me.side, sid) {
        c.add_to_hand(me.side, Item::Minion(m));
    }
}

fn golden_touch(c: &mut Ctx, me: Me, _t: EntityId) {
    let cands: Vec<EntityId> = c
        .tavern_minions(me.side)
        .into_iter()
        .filter(|sid| {
            c.player_ref(me.side)
                .and_then(|p| p.shop.iter().find(|s| s.sid == *sid))
                .and_then(|s| s.item.as_minion())
                .is_some_and(|m| !m.golden && db().get(m.card).golden.is_some())
        })
        .collect();
    let Some(&sid) = c.g.rng.pick(&cands) else {
        return;
    };
    if let Some(m) = c.find_any_mut(sid) {
        make_golden(m);
    }
}

/// Turn a minion instance golden in place, keeping enchantments and enabling its reward.
/// Effects that explicitly deny a Triple Reward clear the flag at their own call site.
///
/// The Golden minion is a new card as far as "this turn" goes: it can be Activated again, and its
/// per-turn allowance starts over at the Golden amount (Malchezaar: 4 Health Refreshes left,
/// whatever the plain one had used). Its own progress follows the card's triple rule as a triple of
/// one (`Script::on_merge`): Treasure Parrot's fired count starts again at 0 of 35, a count under
/// way is kept.
pub fn make_golden(m: &mut Minion) {
    let d = db();
    let Some(g) = d.get(m.card).golden else {
        return;
    };
    let part = script(g).and_then(|s| s.on_merge).map(|f| (f, m.clone()));
    let (base, gold) = (d.get(m.card), d.get(g));
    m.card = g;
    m.golden = true;
    m.add_stats(gold.atk - base.atk, gold.hp - base.hp);
    m.kw = m.kw.union(gold.kw);
    if m.has(Kw::DIVINE_SHIELD) && m.shield_hits == 0 {
        m.shield_hits = m.shield_max.max(1);
    }
    m.triple_reward = true;
    m.activations = 0;
    if let Some(s) = script(g) {
        // Uses counted up this turn start again; uses counted down are set by `on_create`.
        if s.has_flag(flag::TURN_USES) {
            m.counter = 0;
        }
        if let Some(f) = s.on_create {
            f(m);
        }
    }
    if let Some((f, part)) = part {
        m.counter = 0;
        f(m, std::slice::from_ref(&part));
    }
}
