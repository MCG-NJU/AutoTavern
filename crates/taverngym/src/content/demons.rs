//! Demons that consume tavern / friendly minions, and Demon Fodder.

use super::*;

/// Each friendly Demon (except `except`) consumes a tavern minion.
fn demons_consume(c: &mut Ctx, side: Side, except: EntityId, mult: i32) {
    for d in c.friendly_where(side, |m| m.is(Races::DEMON) && m.eid != except) {
        c.consume_random(side, d, mult);
    }
}

fn consume_n(c: &mut Ctx, side: Side, eater: EntityId, n: usize, mult: i32) {
    for _ in 0..n {
        c.consume_random(side, eater, mult);
    }
}

/// "Add a Fodder (golden: two) to your next `refreshes` Refreshes."
fn fodder(c: &mut Ctx, me: Me, refreshes: usize) {
    c.add_fodder(me.side, me.g() as u8, refreshes);
}

fn played_demon(me: Me, e: &Event, _c: &Ctx) -> Option<EntityId> {
    match e {
        Event::Played { side, eid, played }
            if *side == me.side
                && played.source != me.eid
                && played.races.intersects(Races::DEMON) =>
        {
            Some(*eid)
        }
        _ => None,
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Demon Fodder: feeding is handled by the engine (`Ctx::feed_fodder`).
        (FODDER, E),
        // Picky Eater: Battlecry: consume a random tavern minion.
        (
            "BG24_009",
            Script {
                battlecry: Some(|c, me, _| {
                    c.consume_random(me.side, me.eid, me.g());
                }),
                ..E
            },
        ),
        // Mind Muck: Battlecry: a friendly Demon consumes a tavern minion.
        (
            "BG23_357",
            Script {
                battlecry: Some(|c, me, t| {
                    // No friendly Demon to choose: nothing is consumed.
                    if t == 0 {
                        return;
                    }
                    c.consume_random(me.side, t, me.g());
                }),
                battlecry_target: TargetSpec::FriendlyRaceOther(Races::DEMON),
                ..E
            },
        ),
        // Laboratory Assistant: Battlecry: add a Fodder (2) to your next 3 Refreshes.
        (
            "BG35_150",
            Script {
                battlecry: Some(|c, me, _| fodder(c, me, 3)),
                ..E
            },
        ),
        // False Implicator: end of turn, consume a tavern minion.
        (
            "BG29_140",
            Script {
                end_of_turn: Some(|c, me| {
                    c.consume_random(me.side, me.eid, me.g());
                }),
                ..E
            },
        ),
        // Timewarped Devourer: start of turn, consume the Demon to the right: stats + 3 (6) Gold.
        (
            "BG34_Giant_583",
            Script {
                start_of_turn: Some(|c, me| {
                    let Some((s, i)) = c.locate(me.eid) else {
                        return;
                    };
                    let Some(r) = c.boards[s]
                        .get(i + 1)
                        .filter(|m| m.is(Races::DEMON))
                        .map(|m| m.eid)
                    else {
                        return;
                    };
                    if c.consume_friendly(me.side, me.eid, r, me.g()).is_some() {
                        c.gain_gold(me.side, 3 * me.g());
                    }
                }),
                ..E
            },
        ),
        // Trapped Clapper: Deathrattle: add a Fodder (2) to your next 3 Refreshes.
        (
            "BG36_730",
            Script {
                deathrattle: Some(|c, me| fodder(c, me, 3)),
                ..E
            },
        ),
        // Soul Devourer: Battlecry: consume a friendly Demon: its stats and 3 (6) Gold.
        (
            "BGS_059",
            Script {
                battlecry: Some(|c, me, t| {
                    if c.consume_friendly(me.side, me.eid, t, me.g()).is_some() {
                        c.gain_gold(me.side, 3 * me.g());
                    }
                }),
                battlecry_target: TargetSpec::FriendlyRaceOther(Races::DEMON),
                ..E
            },
        ),
        // Peckish Feldrake: Battlecry: consume 3 tavern minions.
        (
            "BG27_009",
            Script {
                battlecry: Some(|c, me, _| consume_n(c, me.side, me.eid, 3, me.g())),
                ..E
            },
        ),
        // Flaming Enforcer: end of turn, consume the highest-Health tavern minion.
        (
            "BG34_500",
            Script {
                end_of_turn: Some(|c, me| {
                    let Some(sid) = c.highest_health_tavern(me.side) else {
                        return;
                    };
                    // The consumed minion's stats, read before the consume: other effects that
                    // fire on it (Consuming Claw...) are not part of what neighbors share.
                    let gain = c.find_any(sid).map(|m| {
                        (
                            m.atk.saturating_mul(me.g()),
                            m.max_hp.saturating_mul(me.g()),
                        )
                    });
                    c.consume(me.side, me.eid, sid, me.g());
                    // Flaming Portrait: the neighbors get the same stats.
                    if let Some((a, h)) = gain
                        && has_trinket(c, me.side, "BG35_MagicItem_156")
                    {
                        for n in c.neighbors(me.eid) {
                            c.buff(n, a, h);
                        }
                    }
                }),
                ..E
            },
        ),
        // Woodland Defiler: end of turn, add a Fodder (2) to your next 3 Refreshes.
        (
            "BG35_151",
            Script {
                end_of_turn: Some(|c, me| fodder(c, me, 3)),
                ..E
            },
        ),
        // Soulkeeping Jailer: Activate (2): your Demons each consume a random tavern minion.
        (
            "BG36_503",
            Script {
                activate: Some((2, TargetSpec::None, |c, me, _| {
                    demons_consume(c, me.side, 0, me.g())
                })),
                ..E
            },
        ),
        // Methodical Madness: a friendly Demon consumes 2 random tavern minions, with their
        // Bonus Keywords.
        (
            "BG36_880",
            Script {
                spell: Some(|c, me, t| {
                    for _ in 0..2 {
                        if let Some(kw) = c.consume_random(me.side, t, 1) {
                            c.give_kw(t, bonus_keywords(kw));
                        }
                    }
                }),
                spell_target: TargetSpec::FriendlyRace(Races::DEMON),
                ..E
            },
        ),
        // Insatiable Ur'zul: after you play a Demon, consume a random tavern minion.
        (
            "BG21_004",
            Script {
                on_event: Some(|c, me, e| {
                    if played_demon(me, e, c).is_some() {
                        c.consume_random(me.side, me.eid, me.g());
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Famished Felbat: end of turn, your Demons each consume a tavern minion.
        (
            "BG21_005",
            Script {
                end_of_turn: Some(|c, me| demons_consume(c, me.side, 0, me.g())),
                ..E
            },
        ),
        // Corrupted Cupcakes: a friendly Demon consumes 3 random tavern minions.
        (
            "BG28_607",
            Script {
                spell: Some(|c, me, t| consume_n(c, me.side, t, 3, 1)),
                spell_target: TargetSpec::FriendlyRace(Races::DEMON),
                ..E
            },
        ),
        // Felboar: after you cast 3 spells, consume a tavern minion. (Counter on the minion.)
        (
            "BG28_633",
            Script {
                on_event: Some(|c, me, e| {
                    if !matches!(e, Event::SpellCast { side, .. } if *side == me.side) {
                        return;
                    }
                    let Some(m) = c.minion_mut(me.eid) else {
                        return;
                    };
                    m.counter += 1;
                    if m.counter >= 3 {
                        m.counter = 0;
                        c.consume_random(me.side, me.eid, me.g());
                    }
                }),
                listens: ev::SPELL_CAST,
                // "After you cast 3 spells, consume a minion in the Tavern to gain its stats.
                // (3 left!)"
                shown: Some(|_, sh| vec![3, (3 - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Furious Driver: Battlecry: your other Demons each consume a tavern minion.
        (
            "BG31_874",
            Script {
                battlecry: Some(|c, me, _| demons_consume(c, me.side, me.eid, me.g())),
                ..E
            },
        ),
        // Timewarped Shivarra: whenever a minion is consumed, gain its stats.
        (
            "BG34_Giant_311",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Consumed { side, atk, hp, .. } = e
                        && *side == me.side
                    {
                        c.buff(
                            me.eid,
                            atk.saturating_mul(me.g()),
                            hp.saturating_mul(me.g()),
                        );
                    }
                }),
                listens: ev::CONSUMED,
                ..E
            },
        ),
        // Nether Construct: Choose One - tavern minions +4/+4 (+8/+8) this game; or consume
        // all tavern minions (Nether Greed; golden text still says "their stats").
        (
            "BG31_842",
            Script {
                choose_one: true,
                battlecry: Some(|c, me, _| {
                    if me.opt_a() {
                        let k = 4 * me.g();
                        if let Some(p) = c.player(me.side) {
                            p.stats.tavern_atk = p.stats.tavern_atk.saturating_add(k);
                            p.stats.tavern_hp = p.stats.tavern_hp.saturating_add(k);
                        }
                        c.buff_tavern(me.side, k, k, Races::NONE);
                    }
                    if me.opt_b() {
                        for sid in c.tavern_minions(me.side) {
                            // "gain their stats": the golden text doesn't double this.
                            c.consume(me.side, me.eid, sid, 1);
                        }
                    }
                }),
                ..E
            },
        ),
        // Batty Terrorguard: after you cast a Tavern spell, another friendly Demon consumes a
        // tavern minion.
        (
            "BG31_871",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, tavern: true, .. } if *side == me.side)
                        && let Some(d) = c.random_friendly_other(me, |m| m.is(Races::DEMON))
                    {
                        c.consume_random(me.side, d, me.g());
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Consummate Conqueror: whenever a minion is consumed, tavern minions +1/+1 (+2/+2)
        // this turn.
        (
            "BG35_153",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Consumed { side, .. } if *side == me.side) {
                        c.buff_tavern_temp(me.side, me.g(), me.g());
                    }
                }),
                listens: ev::CONSUMED,
                ..E
            },
        ),
        // Twisted Wrathguard: after you sell a minion, add a Fodder (2) to your next Refresh.
        (
            "BG35_155",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Sold { side, eid, .. } if *side == me.side && *eid != me.eid)
                    {
                        fodder(c, me, 1);
                    }
                }),
                listens: ev::SOLD,
                ..E
            },
        ),
        // ------------------------------------------------------------------ trinkets
        // Implicator Portrait: get 2 False Implicators; Demons consume the highest-Health
        // tavern minion (`Ctx::consume_pick`).
        (
            "BG32_MagicItem_824",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG29_140", 2)),
                ..E
            },
        ),
        // Statue of Hir'eek: after 2 tavern minions are consumed, get a random Tavern spell.
        (
            "BG32_MagicItem_902",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Consumed { side, tavern: true, .. } if *side == me.side)
                        && c.tick_counter(me.side, me.card, 2)
                    {
                        let tier = c.tier(me.side);
                        if let Some(s) = c.random_spell(|d| d.tier <= tier) {
                            c.give_card(me.side, s);
                        }
                    }
                }),
                listens: ev::CONSUMED,
                ..E
            },
        ),
        // Ur'zul Sticker: after you play a Demon, another friendly Demon consumes a tavern
        // minion.
        (
            "BG35_MagicItem_154",
            Script {
                on_event: Some(|c, me, e| {
                    if let Some(played) = played_demon(me, e, c)
                        && let Some(d) =
                            c.random_friendly(me.side, |m| m.is(Races::DEMON) && m.eid != played)
                    {
                        c.consume_random(me.side, d, 1);
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Consuming Claw: whenever a friendly Demon consumes a minion, it also gains its Bonus
        // Keywords and +5/+5.
        (
            "BG36_MagicItem_801",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Consumed {
                        side, eater, kw, ..
                    } = e
                        && *side == me.side
                        && c.minion(*eater).is_some_and(|m| m.is(Races::DEMON))
                    {
                        c.give_kw(*eater, bonus_keywords(*kw));
                        c.buff(*eater, 5, 5);
                    }
                }),
                listens: ev::CONSUMED,
                ..E
            },
        ),
        // Felsteel Cleaver: after you cast a spell on a tavern minion, a random friendly
        // minion consumes it.
        (
            "BG36_MagicItem_831",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::SpellCast { side, target, .. } = e
                        && *side == me.side
                        && c.tavern_minions(me.side).contains(target)
                        && let Some(f) = c.random_friendly(me.side, |_| true)
                    {
                        c.consume(me.side, f, *target, 1);
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Defiler Portrait: get a Woodland Defiler; Fodders get +4/+4 (greater: +15/+15;
        // `Ctx::spawn_fodder`). The greater one gives the Defiler too (its script was empty).
        (
            "BG35_MagicItem_151",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG35_151", 1)),
                ..E
            },
        ),
        (
            "BG35_MagicItem_151t",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG35_151", 1)),
                ..E
            },
        ),
        // Bloodfury Shield: whenever you cast a Tavern spell, add a Fodder to your next Refresh.
        (
            "BG36_MagicItem_830",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, tavern: true, .. } if *side == me.side)
                    {
                        c.add_fodder(me.side, 1, 1);
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
    ]
}

/// Keywords that transfer as "Bonus Keywords".
pub fn bonus_keywords(kw: Kw) -> Kw {
    let mut out = Kw::NONE;
    for k in [
        Kw::TAUNT,
        Kw::DIVINE_SHIELD,
        Kw::WINDFURY,
        Kw::REBORN,
        Kw::VENOMOUS,
        Kw::STEALTH,
    ] {
        if kw.has(k) {
            out.set(k);
        }
    }
    out
}
