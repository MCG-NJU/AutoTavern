//! Dragons: Chromadrakes and the cards that make them.

use super::*;

fn chroma(c: &mut Ctx, me: Me) {
    give_random_of(c, me.side, &CHROMADRAKES, me.g());
}

fn other_dragons(c: &mut Ctx, me: Me, a: i32, h: i32) {
    for d in c.friendly_where(me.side, |m| m.is(Races::DRAGON) && m.eid != me.eid) {
        c.buff(d, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Blue Chromadrake: Battlecry: get a random 2-Cost Tavern spell (2).
        (
            "BG34_634t",
            Script {
                battlecry: Some(|c, me, _| {
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        if let Some(s) = c.random_spell(|d| d.cost == 2 && d.tier <= tier) {
                            c.give_card(me.side, s);
                        }
                    }
                }),
                ..E
            },
        ),
        // Black Chromadrake: Battlecry: Tavern spells give an extra +1 (+2) Health this game.
        (
            "BG34_635t",
            Script {
                battlecry: Some(|c, me, _| {
                    if let Some(p) = c.player(me.side) {
                        p.stats.spell_hp = p.stats.spell_hp.saturating_add(me.g());
                    }
                }),
                ..E
            },
        ),
        // Green Chromadrake: Battlecry: your other Dragons +1/+3 (+2/+6).
        (
            "BG34_636t",
            Script {
                battlecry: Some(|c, me, _| other_dragons(c, me, 1, 3)),
                ..E
            },
        ),
        // Bronze Chromadrake: Battlecry: your other Dragons +3/+1 (+6/+2).
        (
            "BG34_637t",
            Script {
                battlecry: Some(|c, me, _| other_dragons(c, me, 3, 1)),
                ..E
            },
        ),
        // Red Chromadrake: Battlecry: Tavern spells give an extra +1 (+2) Attack this game.
        (
            "BG34_638t",
            Script {
                battlecry: Some(|c, me, _| {
                    if let Some(p) = c.player(me.side) {
                        p.stats.spell_atk = p.stats.spell_atk.saturating_add(me.g());
                    }
                }),
                ..E
            },
        ),
        // Hired Mount: Activate (2): get a random Chromadrake (2).
        (
            "BG36_240",
            Script {
                activate: Some((2, TargetSpec::None, |c, me, _| chroma(c, me))),
                ..E
            },
        ),
        // Incubation Researcher: Avenge (3): get a random Chromadrake (2).
        (
            "BG34_632",
            Script {
                avenge: Some((3, 3, chroma)),
                ..E
            },
        ),
        // Bronze Timewalker: Rally: get a random Chromadrake (2).
        (
            "BG36_242",
            Script {
                rally: Some(|c, me, _| chroma(c, me)),
                ..E
            },
        ),
        // Draconic Warden: Battlecry and Deathrattle: get a random Chromadrake (2).
        (
            "BG34_633",
            Script {
                battlecry: Some(|c, me, _| chroma(c, me)),
                deathrattle: Some(chroma),
                ..E
            },
        ),
        // Chromatic Tear: get a random Chromadrake; repeat at the start of each turn.
        (
            "BG35_MagicItem_840",
            Script {
                on_attach: Some(|c, me| give_random_of(c, me.side, &CHROMADRAKES, 1)),
                start_of_turn: Some(|c, me| give_random_of(c, me.side, &CHROMADRAKES, 1)),
                ..E
            },
        ),
        // Chromatic Tear (upgraded): get 2; after you play 7 Battlecry minions, repeat.
        (
            "BG35_MagicItem_840t",
            Script {
                on_attach: Some(|c, me| give_random_of(c, me.side, &CHROMADRAKES, 2)),
                on_event: Some(|c, me, e| {
                    if let Event::Played { side, played, .. } = e
                        && *side == me.side
                        && played.battlecry
                        && c.tick_counter(me.side, me.card, 7)
                    {
                        give_random_of(c, me.side, &CHROMADRAKES, 2);
                    }
                }),
                listens: ev::PLAYED,
                // "Get 2 random Chromadrakes. After you play 7 Battlecry minions, repeat this.
                // (7 left!)"
                shown: Some(|c, sh| vec![2, 7, (7 - c.card_data(sh.side, sh.card)).max(0)]),
                ..E
            },
        ),
    ]
}

/// Phase 1 Dragons.
pub fn scripts_p1() -> Vec<(&'static str, Script)> {
    vec![
        // Scarlet Survivor: once this reaches 6 Attack, gain Divine Shield (counter = done).
        (
            "BG35_814",
            Script {
                on_stats: Some(|c, me| {
                    if let Some(m) = c.minion_mut(me.eid)
                        && m.counter == 0
                        && m.atk >= 6
                    {
                        m.counter = 1;
                        c.give_kw(me.eid, Kw::DIVINE_SHIELD);
                    }
                }),
                on_create: Some(|m| {
                    if m.atk >= 6 {
                        m.counter = 1;
                        m.give(Kw::DIVINE_SHIELD);
                    }
                }),
                ..E
            },
        ),
        // Tarecgosa / Persistent Poet: keep combat gains (`Ctx::keep_combat_gain`).
        ("BG21_015", E),
        ("BG29_813", E),
        // Roaring Recruiter: whenever another friendly Dragon attacks, give it +3/+1 (+6/+2).
        (
            "BG29_816",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Attacks { side, attacker, .. } = e
                        && *side == me.side
                        && *attacker != me.eid
                        && c.minion(*attacker).is_some_and(|m| m.is(Races::DRAGON))
                    {
                        c.buff(*attacker, 3 * me.g(), me.g());
                    }
                }),
                listens: ev::ATTACKS,
                ..E
            },
        ),
        // Blue Whelp: Rally: Tavern spells give an extra +1 (+2) Health this game.
        (
            "BG33_924",
            Script {
                rally: Some(|c, me, _| {
                    if let Some(p) = c.player(me.side) {
                        p.stats.spell_hp = p.stats.spell_hp.saturating_add(me.g());
                    }
                }),
                ..E
            },
        ),
        // Runic Arcanist: Start of Combat: cast Shiny Ring twice (4 times).
        (
            "BG36_245",
            Script {
                start_of_combat: Some(|c, me| {
                    for _ in 0..2 * me.g() {
                        c.you_cast(me.side, cid("BG28_168"), 0, 0, 0);
                    }
                }),
                ..E
            },
        ),
        // Firescale Hoarder: Battlecry and Deathrattle: get 1 (2) Shiny Rings.
        (
            "BG32_820",
            Script {
                battlecry: Some(|c, me, _| give_n(c, me.side, "BG28_168", me.g() as usize)),
                deathrattle: Some(|c, me| give_n(c, me.side, "BG28_168", me.g() as usize)),
                ..E
            },
        ),
        // Felfire Conjurer: end of turn, Tavern spells give an extra +1/+1 (+2/+2) this game.
        (
            "BG32_821",
            Script {
                end_of_turn: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.stats.spell_atk = p.stats.spell_atk.saturating_add(me.g());
                        p.stats.spell_hp = p.stats.spell_hp.saturating_add(me.g());
                    }
                }),
                ..E
            },
        ),
        // Hopebringer: Start of Combat: give your minions +4/+3 (+8/+6); permanently improves
        // (by the base amount) after a friendly minion loses Divine Shield.
        (
            "BG36_364",
            Script {
                start_of_combat: Some(|c, me| {
                    let k = me
                        .g()
                        .saturating_add(c.minion(me.eid).map_or(0, |m| m.counter));
                    for t in c.friendly(me.side) {
                        c.buff(t, k.saturating_mul(4), k.saturating_mul(3));
                    }
                }),
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::ShieldLost { side, .. } if *side == me.side) {
                        c.counter_perm(me.side, me.eid, me.g());
                    }
                }),
                listens: ev::SHIELD_LOST,
                flags: flag::IMPROVES,
                // "Start of Combat: Give your minions +4/+3. (Permanently improves after a
                // friendly minion loses Divine Shield!)"
                shown: Some(|_, sh| {
                    let k = sh.g().saturating_add(sh.counter());
                    vec![k.saturating_mul(4), k.saturating_mul(3)]
                }),
                ..E
            },
        ),
        // Crimson Vindicator: Rally: cast Mighty Dragonbreath (twice).
        (
            "BG36_241",
            Script {
                rally: Some(|c, me, _| {
                    for _ in 0..me.g() {
                        c.you_cast(me.side, cid("BG36_246"), 0, 0, 0);
                    }
                }),
                ..E
            },
        ),
        // Mighty Dragonbreath: give your minions +3/+2; repeat for your Dragons; repeat for your
        // minions with Divine Shield.
        (
            "BG36_246",
            Script {
                spell: Some(|c, me, _| {
                    for t in c.friendly(me.side) {
                        spell_buff(c, me, t, 3, 2);
                    }
                    for t in c.friendly_where(me.side, |m| m.is(Races::DRAGON)) {
                        spell_buff(c, me, t, 3, 2);
                    }
                    for t in c.friendly_where(me.side, |m| m.has(Kw::DIVINE_SHIELD)) {
                        spell_buff(c, me, t, 3, 2);
                    }
                }),
                // "Give your minions +3/+2. Repeat for your Dragons. Repeat for your minions
                // with Divine Shield."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 3, 2)),
                ..E
            },
        ),
    ]
}
