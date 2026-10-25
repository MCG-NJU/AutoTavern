//! Timewarp cards (murlocs / mechs): the Timewarped Tavern's own pool (docs/TIMEWARP.md).

use super::*;

/// The Protoss minions (`starcraft.rs`).
const PROTOSS: [&str; 5] = [
    "BG31_HERO_802pt",
    "BG31_HERO_802pt1",
    "BG31_HERO_802pt4",
    "BG31_HERO_802pt5",
    "BG31_HERO_802pt7",
];

/// Minions in the side owner's hand as (hand id, attack, health), left to right, skipping
/// cards already summoned from hand this combat (they are "not in hand any more").
fn hand_stats(c: &Ctx, side: Side) -> Vec<(EntityId, i32, i32)> {
    let Some(p) = c.player_ref(side) else {
        return vec![];
    };
    let used = &c.cs[side].from_hand;
    p.hand
        .iter()
        .filter(|x| !used.contains(&x.hid))
        .filter_map(|x| x.item.as_minion().map(|m| (x.hid, m.atk, m.max_hp)))
        .collect()
}

/// Give the minions adjacent to `me` matching `f` +a/+h.
fn buff_adjacent(c: &mut Ctx, me: Me, f: impl Fn(&Minion) -> bool, a: i32, h: i32) {
    for n in c.neighbors(me.eid) {
        if c.minion(n).is_some_and(|m| m.alive() && f(m)) {
            c.buff(n, a, h);
        }
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        (
            "BG34_Giant_072",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Sold { side, card, .. } if *side == me.side && db().get(*card).tier == 2)
                    {
                        for _ in 0..me.g() {
                            if let Some(card) = c.random_minion(|d| d.tier == 1) {
                                c.give_card(me.side, card);
                            }
                        }
                    }
                }),
                listens: ev::SOLD,
                ..E
            },
        ),
        (
            "BG34_Giant_318",
            Script {
                end_of_turn: Some(|c, me| {
                    let targets = c.friendly_where(me.side, |m| {
                        crate::script::minion_has(m, crate::script::Hook::Battlecry)
                    });
                    for target in targets {
                        for _ in 0..me.g() {
                            c.trigger_battlecry_ext(me.side, target);
                        }
                    }
                }),
                ..E
            },
        ),
        (
            "BG34_Giant_599",
            Script {
                start_of_combat: Some(super::buddies_a::copy_leftmost_deathrattles),
                ..E
            },
        ),
        // Timewarped Annoy-o-Tron: Taunt, Divine Shield, Reborn (keywords only).
        ("BG34_Giant_007", E),
        // Timewarped Scourfin: Deathrattle: give a random minion in your hand +7/+7 (+14/+14)
        // and summon it for this combat only. The stats stay on the hand card. With a full
        // board the minion still gets the stats.
        (
            "BG34_Giant_017",
            Script {
                deathrattle: Some(|c, me| {
                    let hand = hand_stats(c, me.side);
                    if let Some(&(hid, _, _)) = c.g.rng.pick(&hand) {
                        c.buff(hid, 7 * me.g(), 7 * me.g());
                        c.summon_from_hand(me.side, hid, me.pos);
                    }
                }),
                ..E
            },
        ),
        // Timewarped Bassgill: Deathrattle: summon the highest-Health minion (golden: the 2
        // highest) from your hand and give it Divine Shield, for this combat only.
        (
            "BG34_Giant_071",
            Script {
                deathrattle: Some(|c, me| {
                    let mut hand = hand_stats(c, me.side);
                    // Highest Health first; ties in random order.
                    c.g.rng.shuffle(&mut hand);
                    hand.sort_by_key(|x| -x.2);
                    for (i, (hid, _, _)) in hand.into_iter().take(me.g() as usize).enumerate() {
                        if let Some(e) = c.summon_from_hand(me.side, hid, me.pos + i) {
                            c.give_kw(e, Kw::DIVINE_SHIELD);
                        }
                    }
                }),
                ..E
            },
        ),
        // Timewarped Interpreter: whenever you play or Magnetize a Mech, give your Mechs
        // +3/+3 (+6/+6). Playing this card itself doesn't count (as Mechagnome Interpreter).
        (
            "BG34_Giant_120",
            Script {
                on_event: Some(|c, me, e| {
                    let hit = match e {
                        Event::Magnetized { side, card, .. } => {
                            *side == me.side && db().get(*card).races.intersects(Races::MECH)
                        }
                        // "Play or Magnetize": one trigger per action.
                        Event::Played { side, played, .. } => {
                            !c.magnet_play
                                && *side == me.side
                                && played.source != me.eid
                                && played.races.intersects(Races::MECH)
                        }
                        _ => false,
                    };
                    if hit {
                        for t in c.friendly_where(me.side, |m| m.is(Races::MECH)) {
                            c.buff(t, 3 * me.g(), 3 * me.g());
                        }
                    }
                }),
                listens: ev::PLAYED | ev::MAGNETIZED,
                ..E
            },
        ),
        // Timewarped Murky: at the end of your turn, gain +2/+2 (+4/+4), improved by each
        // Battlecry you've triggered this game (base x (1 + Battlecries), as Murky Sticker).
        (
            "BG34_Giant_206",
            Script {
                end_of_turn: Some(|c, me| {
                    let k = 1 + c
                        .player_ref(me.side)
                        .map_or(0, |p| p.stats.battlecries_triggered);
                    c.buff(me.eid, 2 * me.g() * k, 2 * me.g() * k);
                }),
                // "At the end of your turn, gain +2/+2. (Improved by each Battlecry you've
                // triggered this game!)"
                shown: Some(|c, sh| {
                    let k = 1 + c
                        .player_ref(sh.side)
                        .map_or(0, |p| p.stats.battlecries_triggered);
                    vec![2 * sh.g() * k, 2 * sh.g() * k]
                }),
                ..E
            },
        ),
        // Timewarped Murcules: whenever this kills a minion, give the left-most minion in
        // your hand +4/+4 (+8/+8).
        (
            "BG34_Giant_207",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Killed { side, killer, .. } = e
                        && *side == me.side
                        && *killer == me.eid
                        && let Some(&(hid, _, _)) = hand_stats(c, me.side).first()
                    {
                        c.buff(hid, 4 * me.g(), 4 * me.g());
                    }
                }),
                listens: ev::KILLED,
                ..E
            },
        ),
        // Timewarped Sensei: at the end of your turn, give adjacent Mechs +3/+3 (+6/+6).
        (
            "BG34_Giant_209",
            Script {
                end_of_turn: Some(|c, me| {
                    buff_adjacent(c, me, |m| m.is(Races::MECH), 3 * me.g(), 3 * me.g());
                }),
                ..E
            },
        ),
        // Timewarped Copter: Avenge (3): get 1 (2) random Mechs (up to your Tier).
        (
            "BG34_Giant_302",
            Script {
                avenge: Some((3, 3, |c, me| {
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        if let Some(m) =
                            c.random_minion(|d| d.tier <= tier && d.races.intersects(Races::MECH))
                        {
                            c.give_card(me.side, m);
                        }
                    }
                })),
                ..E
            },
        ),
        // Timewarped Painter: at the end of your turn, give adjacent minions +3/+2 (+6/+4).
        // After you play a card from Tier 3 or below, improve this (counter = improvements;
        // base x (1 + improvements)). Cards without a Tier (0) don't count.
        (
            "BG34_Giant_319",
            Script {
                end_of_turn: Some(|c, me| {
                    let k = me
                        .g()
                        .saturating_add(c.minion(me.eid).map_or(0, |m| m.counter));
                    buff_adjacent(c, me, |_| true, k.saturating_mul(3), k.saturating_mul(2));
                }),
                on_event: Some(|c, me, e| {
                    if let Event::CardPlayed { side, card, eid } = e
                        && *side == me.side
                        && *eid != me.eid
                        && (1..=3).contains(&db().get(*card).tier)
                    {
                        c.counter_perm(me.side, me.eid, me.g());
                    }
                }),
                listens: ev::CARD_PLAYED,
                // "At the end of your turn, give adjacent minions +3/+2. After you play a card
                // from Tier 3 or below, improve this."
                shown: Some(|_, sh| {
                    let k = sh.g().saturating_add(sh.counter());
                    vec![k.saturating_mul(3), k.saturating_mul(2), 3]
                }),
                flags: flag::IMPROVES,
                ..E
            },
        ),
        // Timewarped Mystic: after you sell 3 Murlocs, your Tavern spells give an extra
        // +2/+1 (+4/+2) this game (counter = Murlocs sold; repeats every 3). The sold
        // minion's types are its card's.
        (
            "BG34_Giant_320",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Sold { side, card, .. } = e
                        && *side == me.side
                        && db().get(*card).races.intersects(Races::MURLOC)
                        && c.counter_perm(me.side, me.eid, 1) >= 3
                    {
                        c.counter_perm(me.side, me.eid, -3);
                        if let Some(p) = c.player(me.side) {
                            p.stats.spell_atk = p.stats.spell_atk.saturating_add(2 * me.g());
                            p.stats.spell_hp = p.stats.spell_hp.saturating_add(me.g());
                        }
                    }
                }),
                listens: ev::SOLD,
                // "After you sell 3 Murlocs, your Tavern spells give an extra +2/+1 this game.
                // (3 left!)"
                shown: Some(|_, sh| vec![3, 2 * sh.g(), sh.g(), (3 - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Timewarped Mrrrglr: Start of Combat: give adjacent Murlocs the stats of all the
        // minions in your hand (golden: twice).
        (
            "BG34_Giant_321",
            Script {
                start_of_combat: Some(|c, me| {
                    let (a, h) = hand_stats(c, me.side)
                        .into_iter()
                        .fold((0, 0), |(a, h), x| (a + x.1, h + x.2));
                    buff_adjacent(
                        c,
                        me,
                        |m| m.is(Races::MURLOC),
                        a.saturating_mul(me.g()),
                        h.saturating_mul(me.g()),
                    );
                }),
                ..E
            },
        ),
        // Timewarped Acolyte: at the start of your turn, spin the Wheel of Yogg-Saron (golden:
        // and double its effect).
        (
            "BG34_Giant_591",
            Script {
                start_of_turn: Some(|c, me| wheel_of_yogg(c, me.side, me.golden)),
                ..E
            },
        ),
        // Timewarped Immortal: Start of Combat: gain the stats (golden: double) of adjacent
        // minions.
        (
            "BG34_Giant_597",
            Script {
                start_of_combat: Some(|c, me| {
                    let (mut a, mut h) = (0, 0);
                    for n in c.neighbors(me.eid) {
                        if let Some(m) = c.minion(n) {
                            a += m.atk;
                            h += m.hp;
                        }
                    }
                    c.buff(me.eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                }),
                ..E
            },
        ),
        // Timewarped Mothership: Avenge (4): get 1 (2) random Protoss minions. Any of the 5 Protoss
        // minions: the regular Mothership is not this card.
        (
            "BG34_Giant_598",
            Script {
                avenge: Some((4, 4, |c, me| give_random_of(c, me.side, &PROTOSS, me.g()))),
                ..E
            },
        ),
        // Timewarped Wargear: Magnetic. After you Magnetize this, double (golden: triple) the
        // target's stats: the host's stats after the merge, current Health.
        (
            "BG34_Giant_677",
            Script {
                // `on_attach` runs when Magnetized; `me.eid` is the host.
                on_attach: Some(|c, me| {
                    if let Some(m) = c.minion(me.eid) {
                        let (a, h) = (m.atk, m.hp);
                        c.buff(me.eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                    }
                }),
                ..E
            },
        ),
        // Timewarped Squallfin: after you play a Murloc, give minions in your hand and board
        // +2/+2 (+4/+4). Playing this card itself doesn't count.
        (
            "BG34_PreMadeChamp_049",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Played { side, played, .. } = e
                        && !c.magnet_play
                        && *side == me.side
                        && played.source != me.eid
                        && played.races.intersects(Races::MURLOC)
                    {
                        let mut all = c.friendly(me.side);
                        all.extend(hand_stats(c, me.side).into_iter().map(|x| x.0));
                        for t in all {
                            c.buff(t, 2 * me.g(), 2 * me.g());
                        }
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Timewarped Astrogill: while in hand, after a different friendly Murloc gains
        // stats, gain +3/+2 (+6/+4): `Ctx::timewarp_stat_watchers`.
        ("BG34_Giant_801", E),
    ]
}
