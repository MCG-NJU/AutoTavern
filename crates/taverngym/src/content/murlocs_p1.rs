//! Murlocs (phase 1).

use super::*;

const APPRENTICE: &str = "BG33_890t"; // Magicfin Apprentice: Battlecry: cast its taught spell

/// The minion `me`'s side cast a spell on (a friendly or Tavern minion).
fn spell_on(c: &Ctx, me: Me, e: &Event) -> Option<EntityId> {
    match e {
        Event::SpellCast { side, target, .. }
            if *side == me.side && *target != 0 && c.find_any(*target).is_some() =>
        {
            Some(*target)
        }
        _ => None,
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Shoalfin Mystic: when sold, Tavern spells give an extra +1/+1 (+2/+2) this game.
        (
            "BG32_860",
            Script {
                on_sell: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.stats.spell_atk = p.stats.spell_atk.saturating_add(me.g());
                        p.stats.spell_hp = p.stats.spell_hp.saturating_add(me.g());
                    }
                }),
                ..E
            },
        ),
        // Gormling Gourmet: Battlecry and Deathrattle: get 1 (2) Seafood Stews.
        (
            "BG32_336",
            Script {
                battlecry: Some(|c, me, _| give_n(c, me.side, "BG32_337", me.g() as usize)),
                deathrattle: Some(|c, me| give_n(c, me.side, "BG32_337", me.g() as usize)),
                ..E
            },
        ),
        // Seafood Stew: give a minion +1/+1; repeat for each different Bonus Keyword in your
        // warband.
        (
            "BG32_337",
            Script {
                spell: Some(|c, me, t| {
                    for _ in 0..1 + distinct_bonus_keywords(c, me.side) {
                        spell_buff(c, me, t, 1, 1);
                    }
                }),
                spell_target: TargetSpec::Friendly,
                // "Give a minion +1/+1. Repeat for each different Bonus Keyword in your
                // warband."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 1, 1)),
                ..E
            },
        ),
        // Sewer Escapee: Activate (1): give another Murloc +7/+7 (+14/+14) and a random Bonus
        // Keyword. Any other Murloc, a Tavern one too: the text says no "friendly" (a Tavern Bile
        // Spitter 9/22 -> 16/29 with Windfury).
        (
            "BG36_700",
            Script {
                activate: Some((1, TargetSpec::AnyRaceOther(Races::MURLOC), |c, me, t| {
                    if t != 0 {
                        c.buff(t, 7 * me.g(), 7 * me.g());
                        give_random_bonus_keyword(c, t);
                    }
                })),
                ..E
            },
        ),
        // Twilight Tidehunter: whenever you cast a spell on this, give the left-most minion in
        // your hand +8/+8 (+16/+16).
        (
            "BG36_703",
            Script {
                on_event: Some(|c, me, e| {
                    if !aimed_at_me(c, me, e) {
                        return;
                    }
                    let first = c
                        .player_ref(me.side)
                        .and_then(|p| p.hand.iter().find(|h| h.item.as_minion().is_some()))
                        .map(|h| h.hid);
                    if let Some(h) = first {
                        c.buff(h, 8 * me.g(), 8 * me.g());
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Hackerfin: Battlecry: your other minions +3/+2 (+6/+4), improved (by the base) for
        // each different Bonus Keyword in your warband.
        (
            "BG31_148",
            Script {
                battlecry: Some(|c, me, _| {
                    let k = me.g() * (1 + distinct_bonus_keywords(c, me.side));
                    for t in c.friendly_where(me.side, |m| m.eid != me.eid) {
                        c.buff(t, 3 * k, 2 * k);
                    }
                }),
                // "Battlecry: Give your other minions +3/+2. (Improved by each different
                // Bonus Keyword in your warband!)"
                shown: Some(|c, sh| {
                    let k = sh.g() * (1 + distinct_bonus_keywords(c, sh.side));
                    vec![3 * k, 2 * k]
                }),
                ..E
            },
        ),
        // Bile Spitter: Rally: give 1 (2) other friendly Murlocs Venomous.
        (
            "BG33_318",
            Script {
                rally: Some(|c, me, _| {
                    let mut cands = c.friendly_where(me.side, |m| {
                        m.eid != me.eid && m.is(Races::MURLOC) && !m.has(Kw::VENOMOUS)
                    });
                    c.g.rng.shuffle(&mut cands);
                    for t in cands.into_iter().take(me.g() as usize) {
                        c.give_kw(t, Kw::VENOMOUS);
                    }
                }),
                ..E
            },
        ),
        // Shamanic Tidecaller: whenever you cast a spell on a Murloc, give Murlocs in your hand
        // and board +3/+3 (+6/+6).
        (
            "BG36_704",
            Script {
                on_event: Some(|c, me, e| {
                    let Some(t) = spell_on(c, me, e) else { return };
                    if !c.find_any(t).is_some_and(|m| m.is(Races::MURLOC)) {
                        return;
                    }
                    let k = 3 * me.g();
                    for m in c.friendly_where(me.side, |m| m.is(Races::MURLOC)) {
                        c.buff(m, k, k);
                    }
                    let hand: Vec<EntityId> = c
                        .player_ref(me.side)
                        .map(|p| {
                            p.hand
                                .iter()
                                .filter(|h| h.item.as_minion().is_some_and(|m| m.is(Races::MURLOC)))
                                .map(|h| h.hid)
                                .collect()
                        })
                        .unwrap_or_default();
                    for h in hand {
                        c.buff(h, k, k);
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Primalfin Lookout: Battlecry: if you control another Murloc, Discover 1 (2) Murlocs.
        (
            "BGS_020",
            Script {
                battlecry: Some(|c, me, _| {
                    if c.friendly_where(me.side, |m| m.eid != me.eid && m.is(Races::MURLOC))
                        .is_empty()
                    {
                        return;
                    }
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        c.discover_minion(me.side, me.card, 3, |d| {
                            d.races.intersects(Races::MURLOC) && d.tier <= tier
                        });
                    }
                }),
                ..E
            },
        ),
        // Magicfin Mycologist: once (twice) per turn, after you buy a Tavern spell, get a 1/1
        // Magicfin Apprentice taught that spell (counter = uses left this turn).
        (
            "BG33_891",
            Script {
                on_create: Some(|m| m.counter = if m.golden { 2 } else { 1 }),
                start_of_turn: Some(|c, me| {
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = me.g();
                    }
                }),
                on_event: Some(|c, me, e| {
                    let Event::Bought { side, card, .. } = e else {
                        return;
                    };
                    if *side != me.side || !is_tavern_spell(*card) {
                        return;
                    }
                    let Some(m) = c.minion_mut(me.eid).filter(|m| m.counter > 0) else {
                        return;
                    };
                    m.counter -= 1;
                    let mut a = c.new_minion(me.side, cid(APPRENTICE));
                    a.counter = card.0 as i32 + 1; // taught spell
                    c.add_to_hand(me.side, Item::Minion(a));
                }),
                listens: ev::BOUGHT,
                // "Once per turn, after you buy a Tavern spell, get a 1/1 Murloc and teach it that
                // spell. (1 left!)"; golden: "Twice per turn,... (2 left!)"
                shown: Some(|_, sh| vec![1, 1, sh.minion.map_or(sh.g(), |m| m.counter).max(0)]),
                ..E
            },
        ),
        // Magicfin Apprentice: Battlecry: cast the taught spell. Played: on the target the
        // player chooses, never itself. Triggered by an effect: on itself if it can be
        // targeted, else a random legal target.
        (
            APPRENTICE,
            Script {
                // The player chooses the target when the spell has one (asked for by
                // `Ctx::battlecry_spec`); a Choose One spell takes a random option. Triggered
                // by an effect (no chosen target): itself if it can be targeted, else random.
                battlecry: Some(|c, me, t| {
                    let Some(n) = c.minion(me.eid).map(|m| m.counter).filter(|n| *n > 0) else {
                        return;
                    };
                    let spell = crate::cards::CardId((n - 1) as u16);
                    let sc = script(spell);
                    let spec = effect_target_spec(spell).unwrap_or(TargetSpec::None);
                    let target = if spec == TargetSpec::None {
                        0
                    } else if t != 0 {
                        t
                    } else if c.triggered_battlecries == 0 {
                        // Played with nothing else to aim at: its own body is no target
                        // (`Ctx::resolve_or_ask`), the spell is not cast.
                        return;
                    } else {
                        let opts = c.legal_targets(me.side, spec, 0);
                        if opts.contains(&me.eid) {
                            me.eid
                        } else if let Some(&t) = c.g.rng.pick(&opts) {
                            t
                        } else {
                            return;
                        }
                    };
                    let opt = if sc.is_some_and(|s| s.choose_one) {
                        c.g.rng.below(2) as u8
                    } else {
                        0
                    };
                    c.cast_spell(me.side, spell, 0, target, opt);
                }),
                ..E
            },
        ),
        // Bassgill (only from Bassgill Portrait): Deathrattle: summon the highest-Health Murloc
        // (golden: the 2 highest) from your hand for this combat only. Ties in random order; a card
        // already summoned from hand this combat is skipped.
        (
            "BG26_350",
            Script {
                deathrattle: Some(|c, me| {
                    let used = c.cs[me.side].from_hand.clone();
                    let mut hand: Vec<(EntityId, i32)> = c
                        .player_ref(me.side)
                        .map(|p| {
                            p.hand
                                .iter()
                                .filter(|x| x.locked == 0 && !x.held && !used.contains(&x.hid))
                                .filter_map(|x| x.item.as_minion().map(|m| (x.hid, m)))
                                .filter(|(_, m)| m.is(Races::MURLOC))
                                .map(|(hid, m)| (hid, m.max_hp))
                                .collect()
                        })
                        .unwrap_or_default();
                    c.g.rng.shuffle(&mut hand);
                    hand.sort_by_key(|x| -x.1);
                    for (i, (hid, _)) in hand.into_iter().take(me.g() as usize).enumerate() {
                        c.summon_from_hand(me.side, hid, me.pos + i);
                    }
                }),
                ..E
            },
        ),
        // Tide Oracle Morgl (only from Morgl Portrait): Poisonous. When this attacks and kills a
        // minion, give that minion's maximum stats (golden: double) to a random minion in your
        // hand. "Attacks and kills": a kill made during its own attack, as Timewarped Pagle
        // (counter = its attack is under way); a Morgl that died in that attack still gives them.
        (
            "BG27_513",
            Script {
                on_event: Some(|c, me, e| match e {
                    Event::Attacks { attacker, .. } => {
                        if let Some(m) = c.minion_mut(me.eid) {
                            m.counter = i32::from(*attacker == me.eid);
                        }
                    }
                    Event::Killed {
                        side,
                        killer,
                        victim,
                    } if *side == me.side && *killer == me.eid => {
                        let attacking = match c.minion(me.eid) {
                            Some(m) => m.counter,
                            None => c.dead_record(me.side, me.eid).map_or(0, |d| d.counter),
                        };
                        if attacking == 0 {
                            return;
                        }
                        let Some((atk, hp)) = c
                            .dead_record(1 - me.side, *victim)
                            .map(|d| (d.atk, d.max_hp))
                        else {
                            return;
                        };
                        let hand: Vec<EntityId> = c
                            .player_ref(me.side)
                            .map(|p| {
                                p.hand
                                    .iter()
                                    .filter(|h| h.item.as_minion().is_some())
                                    .map(|h| h.hid)
                                    .collect()
                            })
                            .unwrap_or_default();
                        if let Some(&hid) = c.g.rng.pick(&hand) {
                            c.buff(hid, atk.max(0) * me.g(), hp.max(0) * me.g());
                        }
                    }
                    _ => {}
                }),
                listens: ev::ATTACKS | ev::KILLED,
                ..E
            },
        ),
    ]
}
