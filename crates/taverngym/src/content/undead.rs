//! Undead (phase 1).

use super::*;

/// "Your Undead have +a Attack this game (wherever they are)".
fn undead_atk(c: &mut Ctx, side: Side, a: i32) {
    c.bump_player_stat(side, |s| s.undead_atk = s.undead_atk.saturating_add(a));
}

fn discover_undead(c: &mut Ctx, me: Me) {
    let tier = c.tier(me.side);
    for _ in 0..me.g() {
        c.discover_minion(me.side, me.card, 3, |d| {
            d.races.intersects(Races::UNDEAD) && d.tier <= tier
        });
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Maw Caster: Battlecry: destroy a friendly Undead to Discover an Undead (2).
        (
            "BG32_340",
            Script {
                battlecry: Some(|c, me, t| {
                    if !c
                        .minion(t)
                        .is_some_and(|m| m.alive() && m.is(Races::UNDEAD))
                    {
                        return;
                    }
                    c.destroy(t);
                    c.resolve_deaths();
                    discover_undead(c, me);
                }),
                battlecry_target: TargetSpec::FriendlyRaceOther(Races::UNDEAD),
                ..E
            },
        ),
        // Friendly Geist: Deathrattle: Tavern spells give an extra +1 (+2) Attack this game.
        (
            "BG32_880",
            Script {
                deathrattle: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.stats.spell_atk = p.stats.spell_atk.saturating_add(me.g());
                    }
                }),
                ..E
            },
        ),
        // Dead Bellringer: Activate (1): give a different friendly Undead Reborn, then destroy
        // it to gain +4/+4 (+8/+8).
        (
            "BG36_511",
            Script {
                activate: Some((
                    1,
                    TargetSpec::FriendlyRaceOther(Races::UNDEAD),
                    |c, me, t| {
                        if t == 0 {
                            return;
                        }
                        c.give_kw(t, Kw::REBORN);
                        c.destroy(t);
                        c.resolve_deaths();
                        c.buff(me.eid, 4 * me.g(), 4 * me.g());
                    },
                )),
                ..E
            },
        ),
        // Drustfallen Butcher: Avenge (4): get 1 (2) Butcherings.
        (
            "BG32_324",
            Script {
                avenge: Some((4, 4, |c, me| {
                    give_n(c, me.side, "BG28_604", me.g() as usize)
                })),
                ..E
            },
        ),
        // Butchering: destroy a friendly Undead; your Undead have +8 Attack this game. A Tavern
        // spell with a number: the Tavern-spell bonus adds to it, both parts - the spell's missing
        // number is a 0, so a bonus with Health gives your Undead that Health this game as well.
        (
            "BG28_604",
            Script {
                spell: Some(|c, me, t| {
                    if !c
                        .minion(t)
                        .is_some_and(|m| m.alive() && m.is(Races::UNDEAD))
                    {
                        return;
                    }
                    c.destroy(t);
                    c.resolve_deaths();
                    let (ea, eh) = c.spell_extra(me.side);
                    c.bump_player_stat(me.side, |s| {
                        s.undead_atk = s.undead_atk.saturating_add(ea.saturating_add(8));
                        s.undead_hp = s.undead_hp.saturating_add(eh);
                    });
                }),
                spell_target: TargetSpec::FriendlyRace(Races::UNDEAD),
                // "Destroy a friendly Undead. Your Undead have +8 Attack this game (wherever
                // they are)."
                shown: Some(|c, sh| vec![c.spell_extra(sh.side).0.saturating_add(8)]),
                ..E
            },
        ),
        // Eternal Tycoon (back in 36.6.3): Avenge (5): summon an Eternal Knight (golden: a
        // Golden one); it attacks immediately. Queued like the other "summon and attack"
        // effects: it waits for board space.
        (
            "BG34_403",
            Script {
                avenge: Some((5, 5, |c, me| {
                    let knight = cid(ETERNAL_KNIGHT);
                    let card = if me.golden {
                        db().golden_of(knight)
                    } else {
                        knight
                    };
                    let m = c.new_minion(me.side, card);
                    c.when_space(me.side, crate::engine::SpaceReq::AttackNow(m));
                })),
                ..E
            },
        ),
        // Lichling Hoarder: Avenge (3): get 1 (2) plain copies of a minion that started in your
        // warband this combat.
        (
            "BG36_848",
            Script {
                avenge: Some((3, 3, |c, me| {
                    let Some(p) = c.owner[me.side] else { return };
                    let start: Vec<CardId> = c.g.players[p].board.iter().map(|m| m.card).collect();
                    let Some(&card) = c.g.rng.pick(&start) else {
                        return;
                    };
                    for _ in 0..me.g() {
                        c.give_card(me.side, db().normal_of(card));
                    }
                })),
                ..E
            },
        ),
        // Forsaken Weaver: after you cast a Tavern spell, your Undead have +3 (+6) Attack this
        // game.
        (
            "BG34_692",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, tavern: true, .. } if *side == me.side)
                    {
                        undead_atk(c, me.side, 3 * me.g());
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Snazzy Phantom: after a friendly minion is Reborn, give stats equal to its Attack
        // (double) to your right-most Undead.
        (
            "BG36_515",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Reborn { side, eid } = e else {
                        return;
                    };
                    if *side != me.side {
                        return;
                    }
                    let Some(a) = c.minion(*eid).map(|m| m.atk) else {
                        return;
                    };
                    let t = c.boards[me.side]
                        .iter()
                        .rev()
                        .find(|m| m.alive() && m.is(Races::UNDEAD))
                        .map(|m| m.eid);
                    if let Some(t) = t {
                        c.buff(t, a.saturating_mul(me.g()), a.saturating_mul(me.g()));
                    }
                }),
                listens: ev::REBORN,
                ..E
            },
        ),
    ]
}
