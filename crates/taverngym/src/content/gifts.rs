//! Dark Gifts. A gift is a card attached to its minion (`Minion::attached`); its hooks run with
//! `me.eid` = the host minion. Static stat/keyword parts are applied in `on_attach` when the gifted
//! minion is gained. Counter-based "Has +X/+X for each..." gifts are computed in
//! `engine::apply_bonuses`.

use super::*;

/// The owner's "this game" counters, for the texts of the counter auras ("... (0)").
fn stats_of<'a>(c: &'a Ctx, sh: &crate::script::Shown) -> Option<&'a crate::state::PlayerStats> {
    c.player_ref(sh.side).map(|p| &p.stats)
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        ("BG36_MidGameEffect_000t73", STAT),    // Fortitude
        ("BG36_MidGameEffect_000t72", STAT),    // Titanic Strength
        ("BG36_MidGameEffect_000t13", KEYWORD), // Harpy's Talons
        ("BG36_MidGameEffect_000t69", KEYWORD), // Toxicity
        ("BG36_MidGameEffect_000t79", KEYWORD), // Furtiveness
        ("BG36_MidGameEffect_000t60", KEYWORD), // Invulnerability
        ("BG36_MidGameEffect_000t12", KEYWORD), // Persisting Horror (full-stat reborn in engine)
        (
            "BG36_MidGameEffect_000t15", // Toreth's Blessing: shield takes 3 hits
            Script {
                on_attach: Some(|c, me| {
                    if let Some(m) = c.find_any_mut(me.eid) {
                        m.shield_max = 3;
                        m.give(Kw::DIVINE_SHIELD);
                        m.shield_hits = 3;
                    }
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t22", // Amalgamation: has all minion types
            Script {
                on_attach: Some(|c, me| {
                    if let Some(m) = c.find_any_mut(me.eid) {
                        m.races = Races::ALL;
                    }
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t14", // Gilding: Golden, no Triple Reward
            Script {
                on_attach: Some(|c, me| {
                    if let Some(m) = c.find_any_mut(me.eid) {
                        super::spells::make_golden(m);
                        m.triple_reward = false;
                    }
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t11", // Double Vision: get an extra copy of this
            Script {
                // A full copy, taken once the gifted card is in hand.
                on_attach: Some(|c, me| {
                    c.give_copy_of_hand(me.side, me.eid);
                }),
                ..E
            },
        ),
        // Offensive Sacrifice: +10 Attack. Deathrattle: give this minion's Attack to another.
        (
            "BG36_MidGameEffect_000t",
            Script {
                on_attach: Some(|c, me| c.buff(me.eid, 10, 0)),
                deathrattle: Some(|c, me| sacrifice(c, me, true)),
                ..E
            },
        ),
        // Defensive Sacrifice: +10 Health. Deathrattle: give max Health to another.
        (
            "BG36_MidGameEffect_000t2",
            Script {
                on_attach: Some(|c, me| c.buff(me.eid, 0, 10)),
                deathrattle: Some(|c, me| sacrifice(c, me, false)),
                ..E
            },
        ),
        ("BG36_MidGameEffect_000t74", ON_PLAY), // Sharpened Sword
        ("BG36_MidGameEffect_000t75", ON_PLAY), // Toughened Shield
        ("BG36_MidGameEffect_000t64", ON_PLAY), // Dexterity
        ("BG36_MidGameEffect_000t64t", ON_PLAY), // Dexterity (late)
        (
            "BG36_MidGameEffect_000t71", // Hostility: SoC double Attack
            soc(|c, me| {
                // The server sets the Attack to double and its player-level Attack bonus (the
                // Undead aura) then counts on top, as `Ctx::set_stats` does (2853 with a 273 Undead
                // bonus became 2 x 2853 + 273 = 5979).
                let (a, aura) = c.minion(me.eid).map_or((0, 0), |m| (m.atk, m.pbonus_atk));
                c.buff(me.eid, a.saturating_add(aura), 0);
            }),
        ),
        (
            "BG36_MidGameEffect_000t7", // Resistance: SoC double Health
            soc(|c, me| {
                let h = c.minion(me.eid).map(|m| m.hp).unwrap_or(0);
                c.buff(me.eid, 0, h);
            }),
        ),
        (
            "BG36_MidGameEffect_000t81", // Transcendence: SoC triple stats
            soc(|c, me| {
                let (a, h) = c.minion(me.eid).map(|m| (m.atk, m.hp)).unwrap_or((0, 0));
                c.buff(me.eid, a.saturating_mul(2), h.saturating_mul(2));
            }),
        ),
        (
            "BG36_MidGameEffect_000t9", // Admiration: SoC gain the Attack of the minion to the left
            soc(|c, me| {
                let Some((s, i)) = c.locate(me.eid) else {
                    return;
                };
                if i > 0 {
                    let a = c.boards[s][i - 1].atk;
                    c.buff(me.eid, a, 0);
                }
            }),
        ),
        (
            "BG36_MidGameEffect_000t89", // Worship: SoC gain the stats of your Deity
            soc(|c, me| {
                let (a, h) = c
                    .player_ref(me.side)
                    .and_then(|p| p.deity.as_ref())
                    .map(|d| (d.atk, d.hp))
                    .unwrap_or((0, 0));
                c.buff(me.eid, a, h);
            }),
        ),
        (
            "BG36_MidGameEffect_000t16", // Jaws of Death: SoC trigger this minion's Deathrattles
            soc(|c, me| {
                let Some((s, i)) = c.locate(me.eid) else {
                    return;
                };
                let m = c.boards[s][i].clone();
                // Triggering is not dying: skip the gift's own hook to avoid self-recursion.
                let mut host = m.clone();
                host.attached
                    .retain(|a| db().get(*a).id != "BG36_MidGameEffect_000t16");
                c.run_deathrattles(s, &host, i + 1);
                c.resolve_deaths();
            }),
        ),
        (
            "BG36_MidGameEffect_000t90", // Doppelganger: SoC summon a copy of this
            soc(|c, me| {
                let Some(m) = c.minion(me.eid).cloned() else {
                    return;
                };
                let pos = c.pos_right_of(me.eid);
                let mut copy = m.clone();
                copy.eid = c.g.alloc_eid();
                copy.combat_only = true;
                copy.pool_copies = 0;
                copy.attached
                    .retain(|a| db().get(*a).id != "BG36_MidGameEffect_000t90");
                c.summon(me.side, pos, copy);
            }),
        ),
        (
            "BG36_MidGameEffect_000t61", // Golemancy: DR summon a Golem with this minion's stats
            Script {
                deathrattle: Some(|c, me| {
                    let (a, h) = c.deathrattle_stats(me.side, me.eid).unwrap_or((1, 1));
                    let mut m = c.new_minion(me.side, cid(GOLEM));
                    m.set_stats(a, h.max(1));
                    c.summon(me.side, me.pos, m);
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t5", // Mystic Essence: DR get a random Tavern spell
            Script {
                deathrattle: Some(|c, me| {
                    let tier = c.tier(me.side);
                    if let Some(s) = c.random_spell(|d| d.tier <= tier) {
                        c.give_card(me.side, s);
                    }
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t52", // Fresh Perspective: DR gain 2 free Refreshes
            Script {
                deathrattle: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.free_refreshes += 2;
                    }
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t66", // Demonology: Rally add a Fodder to your next 3 Refreshes
            Script {
                rally: Some(|c, me, _| c.add_fodder(me.side, 1, 3)),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t80", // Consanguinity: Rally get 2 Blood Gems
            Script {
                rally: Some(|c, me, _| give_n(c, me.side, BLOOD_GEM, 2)),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t3", // Charisma: Rally get a random minion of your most common type
            Script {
                rally: Some(|c, me, _| {
                    let Some(r) = c.most_common_race(me.side) else {
                        return;
                    };
                    let tier = c.tier(me.side);
                    if let Some(card) = c.random_minion(|d| d.races.intersects(r) && d.tier <= tier)
                    {
                        c.give_card(me.side, card);
                    }
                }),
                ..E
            },
        ),
        (
            // Steady Growth: EOT gain +2/+2, +2/+3 or +3/+4 by the turn it was offered (3, 4, 5+).
            // Each attached gift stores its own offering stage (`gift_init`).
            "BG36_MidGameEffect_000t51",
            Script {
                end_of_turn: Some(|c, me| {
                    let stage = me.data;
                    let (a, h) = STEADY_GROWTH[stage.clamp(0, 2) as usize];
                    c.buff(me.eid, a, h);
                }),
                // "At the end of your turn, gain +0/+0.": the amount of its stage, once the
                // gift is on a minion.
                shown: Some(|_, sh| match sh.minion {
                    Some(_) => {
                        let (a, h) = STEADY_GROWTH[sh.data.clamp(0, 2) as usize];
                        vec![a, h]
                    }
                    None => vec![0, 0],
                }),
                ..E
            },
        ),
        (
            // Time Turning: this minion's end of turn effects also trigger at start of turn.
            "BG36_MidGameEffect_000t21",
            Script {
                start_of_turn: Some(|c, me| {
                    let Some(m) = c.minion(me.eid) else { return };
                    let cards: Vec<(CardId, i32)> = std::iter::once((m.card, 0))
                        .chain(
                            m.attached
                                .iter()
                                .enumerate()
                                .map(|(i, card)| (*card, m.attachment_data_at(i))),
                        )
                        .filter(|(card, _)| *card != me.card)
                        .collect();
                    let mult = c.end_of_turn_mult(me.side);
                    for (card, data) in cards {
                        let Some(fx) = script(card).and_then(|s| s.end_of_turn) else {
                            continue;
                        };
                        for _ in 0..mult {
                            if !c.on_board(me.eid) {
                                return;
                            }
                            // Attached Magnetic minions keep their own normal/golden form.
                            let g = db().get(card).is_golden();
                            fx(
                                c,
                                Me {
                                    card,
                                    golden: g,
                                    data,
                                    ..me
                                },
                            );
                        }
                    }
                }),
                ..E
            },
        ),
        // Tarecgosa's Blessing: keeps Bonus Keywords and double stats gained in combat
        // (`Ctx::keep_combat_gain`).
        ("BG36_MidGameEffect_000t50", E),
        (
            // Polarization: at the end of your turn, Magnetize a random Mech (tier <= yours,
            // not from the pool) to this.
            "BG36_MidGameEffect_000t65",
            Script {
                end_of_turn: Some(|c, me| {
                    let tier = c.tier(me.side);
                    let Some(x) = c.random_minion(|d| {
                        d.tier <= tier
                            && d.races.intersects(Races::MECH)
                            && d.mech.has(Mech::MAGNETIC)
                    }) else {
                        return;
                    };
                    let m = c.new_minion(me.side, x);
                    crate::recruit::magnetize_onto(c, me.side, me.eid, m);
                }),
                ..E
            },
        ),
        (
            // Affinity: at the end of every 2 turns, get a random minion of this type (tier <=
            // yours).
            "BG36_MidGameEffect_000t82",
            Script {
                end_of_turn: Some(|c, me| {
                    if !c.minion_turn_cycle(me.eid, 2) {
                        return;
                    }
                    let Some(m) = c.minion_mut(me.eid) else {
                        return;
                    };
                    let r = m.races;
                    let r = target_type(c, r);
                    let tier = c.tier(me.side);
                    if let Some(x) = c.random_minion(|d| d.tier <= tier && d.races.intersects(r)) {
                        c.give_card(me.side, x);
                    }
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t10", // Echoing Voice: EOT trigger this minion's Battlecries
            Script {
                end_of_turn: Some(|c, me| {
                    let has_bc = c
                        .minion(me.eid)
                        .is_some_and(|m| script(m.card).is_some_and(|s| s.battlecry.is_some()));
                    if has_bc {
                        c.trigger_battlecry_ext(me.side, me.eid);
                    }
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t4", // Incubation: +4/+4; in 2 turns double stats
            Script {
                on_attach: Some(|c, me| {
                    c.buff(me.eid, 4, 4);
                    schedule(
                        c,
                        me.side,
                        Scheduled {
                            turns_left: 2,
                            script: me.card,
                            target: me.eid,
                            a: 0,
                            b: 0,
                            if_won: false,
                        },
                    );
                }),
                scheduled: Some(|c, me, _s| {
                    let stats = c.find_any_mut(me.eid).map(|m| (m.atk, m.max_hp));
                    if let Some((a, h)) = stats {
                        c.buff(me.eid, a, h);
                    }
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t18", // Replication: every 2 turns get a plain copy of this
            Script {
                end_of_turn: Some(|c, me| {
                    if c.minion_turn_cycle(me.eid, 2)
                        && let Some(card) = c.minion(me.eid).map(|m| db().normal_of(m.card))
                    {
                        c.give_card(me.side, card);
                    }
                }),
                ..E
            },
        ),
        ("BG36_MidGameEffect_000t78", E), // Efficiency: Activate twice (recruit.rs)
        // Counter auras computed in engine::apply_bonuses:
        // Counter auras: the stats are computed in engine::apply_bonuses; the scripts only
        // show the count so far ("Has +3/+3 for each Battlecry you've triggered this game.
        // (0)").
        (
            "BG36_MidGameEffect_000t28",
            Script {
                shown: Some(|c, sh| {
                    vec![3, 3, stats_of(c, sh).map_or(0, |s| s.battlecries_triggered)]
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t28t",
            Script {
                shown: Some(|c, sh| {
                    vec![2, 2, stats_of(c, sh).map_or(0, |s| s.battlecries_triggered)]
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t29",
            Script {
                shown: Some(|c, sh| {
                    vec![
                        2,
                        2,
                        stats_of(c, sh).map_or(0, |s| s.deathrattles_triggered),
                    ]
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t29t",
            Script {
                shown: Some(|c, sh| {
                    vec![
                        1,
                        1,
                        stats_of(c, sh).map_or(0, |s| s.deathrattles_triggered),
                    ]
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t30",
            Script {
                shown: Some(|c, sh| {
                    vec![3, 3, stats_of(c, sh).map_or(0, |s| s.tavern_spells_cast)]
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t30t",
            Script {
                shown: Some(|c, sh| {
                    vec![2, 2, stats_of(c, sh).map_or(0, |s| s.tavern_spells_cast)]
                }),
                ..E
            },
        ),
        (
            "BG36_MidGameEffect_000t88",
            Script {
                shown: Some(|c, sh| vec![3, 3, stats_of(c, sh).map_or(0, |s| s.cards_discarded)]),
                ..E
            },
        ),
    ]
}

/// Steady Growth stages: offered on turn 3, 4, 5+.
const STEADY_GROWTH: [(i32, i32); 3] = [(2, 2), (2, 3), (3, 4)];

/// Gifts Tras'tath's start-of-game Discover can offer (`transath_source_initial_list` of the card
/// data, without Toxicity). They ignore the normal turn windows.
/// Tras'tath's gifts: the official list of 20 (Toxicity removed in 36.2.2); the early variants of
/// Battle Scars / Death's Embrace / Spell Siphon are not in it (hsdata tag 4874).
pub const TRASTATH_GIFTS: [&str; 20] = [
    "BG36_MidGameEffect_000t",
    "BG36_MidGameEffect_000t10",
    "BG36_MidGameEffect_000t12",
    "BG36_MidGameEffect_000t13",
    "BG36_MidGameEffect_000t15",
    "BG36_MidGameEffect_000t16",
    "BG36_MidGameEffect_000t2",
    "BG36_MidGameEffect_000t22",
    "BG36_MidGameEffect_000t28",
    "BG36_MidGameEffect_000t29",
    "BG36_MidGameEffect_000t3",
    "BG36_MidGameEffect_000t30",
    "BG36_MidGameEffect_000t50",
    "BG36_MidGameEffect_000t60",
    "BG36_MidGameEffect_000t61",
    "BG36_MidGameEffect_000t65",
    "BG36_MidGameEffect_000t7",
    "BG36_MidGameEffect_000t71",
    "BG36_MidGameEffect_000t72",
    "BG36_MidGameEffect_000t81",
];

/// Battlecry / Choose One minions only take these gifts: the four of the general rules and the
/// ones the client puts on such minions (Hostility on Battlecry and Choose One minions from turn 5, Jaws of Death on
/// Battlecry + Deathrattle ones, Violent Toxicity on Hackerfin). Not every gift: the full condition
/// is not known.
const BATTLECRY_GIFTS: [&str; 7] = [
    "BG36_MidGameEffect_000t11", // Double Vision
    "BG36_MidGameEffect_000t18", // Replication
    "BG36_MidGameEffect_000t14", // Gilding
    "BG36_MidGameEffect_000t10", // Echoing Voice
    "BG36_MidGameEffect_000t71", // Hostility
    "BG36_MidGameEffect_000t16", // Jaws of Death
    "BG36_MidGameEffect_000t69", // Violent Toxicity
];

/// Turns the client offers a gift on the Dark Gift button: a window never shuts out a turn seen
/// there (Hostility on turn 5, where its data window starts at 7). The
/// data windows stay where they are wider.
const SEEN_TURNS: [(&str, u8, u8); 39] = [
    ("BG36_MidGameEffect_000t13", 3, 16),
    ("BG36_MidGameEffect_000t51", 3, 5),
    ("BG36_MidGameEffect_000t52", 3, 5),
    ("BG36_MidGameEffect_000t73", 3, 3),
    ("BG36_MidGameEffect_000t74", 3, 6),
    ("BG36_MidGameEffect_000t75", 3, 6),
    ("BG36_MidGameEffect_000t80", 3, 5),
    ("BG36_MidGameEffect_000t82", 3, 4),
    ("BG36_MidGameEffect_000t14", 4, 7),
    ("BG36_MidGameEffect_000t18", 4, 7),
    ("BG36_MidGameEffect_000t28t", 4, 5),
    ("BG36_MidGameEffect_000t29t", 4, 6),
    ("BG36_MidGameEffect_000t79", 4, 5),
    ("BG36_MidGameEffect_000t11", 5, 16),
    ("BG36_MidGameEffect_000t16", 5, 13),
    ("BG36_MidGameEffect_000t21", 5, 5),
    ("BG36_MidGameEffect_000t30t", 5, 6),
    ("BG36_MidGameEffect_000t5", 5, 8),
    ("BG36_MidGameEffect_000t65", 5, 7),
    ("BG36_MidGameEffect_000t66", 5, 5),
    ("BG36_MidGameEffect_000t71", 5, 9),
    ("BG36_MidGameEffect_000t78", 5, 11),
    ("BG36_MidGameEffect_000t88", 5, 5),
    ("BG36_MidGameEffect_000t", 6, 9),
    ("BG36_MidGameEffect_000t10", 6, 9),
    ("BG36_MidGameEffect_000t15", 6, 7),
    ("BG36_MidGameEffect_000t2", 6, 9),
    ("BG36_MidGameEffect_000t4", 6, 8),
    ("BG36_MidGameEffect_000t64", 7, 7),
    ("BG36_MidGameEffect_000t90", 7, 8),
    ("BG36_MidGameEffect_000t3", 8, 10),
    ("BG36_MidGameEffect_000t81", 8, 13),
    ("BG36_MidGameEffect_000t9", 8, 14),
    ("BG36_MidGameEffect_000t22", 9, 16),
    ("BG36_MidGameEffect_000t29", 9, 9),
    ("BG36_MidGameEffect_000t12", 10, 14),
    ("BG36_MidGameEffect_000t30", 10, 10),
    ("BG36_MidGameEffect_000t61", 10, 10),
    ("BG36_MidGameEffect_000t69", 11, 14),
];

/// Stat-granting gifts; Deathrattle minions don't get them (Sharpened Sword and Death's Embrace are
/// the named exceptions and are not listed).
const STAT_GIFTS: [&str; 18] = [
    "BG36_MidGameEffect_000t",
    "BG36_MidGameEffect_000t2",
    "BG36_MidGameEffect_000t4",
    "BG36_MidGameEffect_000t7",
    "BG36_MidGameEffect_000t9",
    "BG36_MidGameEffect_000t28",
    "BG36_MidGameEffect_000t28t",
    "BG36_MidGameEffect_000t30",
    "BG36_MidGameEffect_000t30t",
    "BG36_MidGameEffect_000t51",
    "BG36_MidGameEffect_000t64",
    "BG36_MidGameEffect_000t64t",
    "BG36_MidGameEffect_000t71",
    "BG36_MidGameEffect_000t72",
    "BG36_MidGameEffect_000t73",
    "BG36_MidGameEffect_000t75",
    "BG36_MidGameEffect_000t81",
    "BG36_MidGameEffect_000t89",
];

/// Charisma's named minion exclusions.
const CHARISMA_EXCLUDED: [&str; 10] = [
    "BG25_354", "BG29_816", "BG36_211", "BG26_802", "BG35_602", "BG36_515", "BG36_514", "BG36_209",
    "BG35_883", "BG36_208",
];

/// Whether Dark Gift `gift` may be offered on minion `def` to `side`'s player
/// (`rules/dark_gift_offering.json`). `lowest` = the lowest Tier among the offer's candidate
/// minions (Gilding goes only on those).
/// Whether `n` is the highest of your Battlecry / Deathrattle / Tavern-spell counts (and > 0).
fn most_triggered(s: &crate::state::PlayerStats, n: i32) -> bool {
    n > 0
        && n >= s
            .battlecries_triggered
            .max(s.deathrattles_triggered)
            .max(s.tavern_spells_cast)
}

/// Activate minions Efficiency is not offered on (hsdata 253932 tag 4953, normal forms): the
/// "discard a card to..." ones, Lurking Lionfish, Tyrael, Soulkeeping Jailer, Dead Bellringer.
const EFFICIENCY_EXCLUDED: [&str; 9] = [
    "BG28_582", // Mangled Bandit
    "BG36_099", // Brain Rotter
    "BG36_201", // Lurking Lionfish
    "BG36_300", // N'raqi Frostcaller
    "BG36_311", // Abyssal Envoy
    "BG36_312", // Mindbending Recruiter
    "BG36_356", // Tyrael
    "BG36_503", // Soulkeeping Jailer
    "BG36_511", // Dead Bellringer
];

pub fn gift_fits(
    c: &Ctx,
    side: Side,
    gift: CardId,
    def: &crate::cards::CardDef,
    lowest: u8,
) -> bool {
    // Hot path (every candidate x gift of each offer): the sets are resolved to CardIds once.
    struct Sets {
        battlecry: Vec<CardId>,
        stat: Vec<CardId>,
        charisma: Vec<CardId>,
    }
    static SETS: std::sync::OnceLock<Sets> = std::sync::OnceLock::new();
    let sets = SETS.get_or_init(|| Sets {
        battlecry: BATTLECRY_GIFTS.iter().map(|x| cid(x)).collect(),
        stat: STAT_GIFTS.iter().map(|x| cid(x)).collect(),
        charisma: CHARISMA_EXCLUDED.iter().map(|x| cid(x)).collect(),
    });
    let id = db().get(gift).id.as_str();
    let typed = !def.races.is_none();
    let has = |m: Mech| def.mech.has(m);
    let battlecry = has(Mech::BATTLECRY) || has(Mech::CHOOSE_ONE);
    if battlecry && !sets.battlecry.contains(&gift) {
        return false;
    }
    if has(Mech::DEATHRATTLE) && sets.stat.contains(&gift) {
        return false;
    }
    let st = c.player_ref(side).map(|p| &p.stats);
    let lobby = c.g.lobby_races;
    let taunt = def.kw.has(Kw::TAUNT);
    let avenge = has(Mech::AVENGE);
    match id {
        "BG36_MidGameEffect_000t"
        | "BG36_MidGameEffect_000t2"
        | "BG36_MidGameEffect_000t12"
        | "BG36_MidGameEffect_000t4"
        | "BG36_MidGameEffect_000t64"
        | "BG36_MidGameEffect_000t64t"
        | "BG36_MidGameEffect_000t82" => typed,
        // Golemancy: not on Stitched Salvager (36.2.1).
        "BG36_MidGameEffect_000t61" => typed && !def.id.starts_with("BG31_999"),
        // Echoing Voice: not on Maw Caster (36.6.3).
        "BG36_MidGameEffect_000t10" => has(Mech::BATTLECRY) && def.id != "BG32_340",
        "BG36_MidGameEffect_000t14" => def.tier == lowest && !has(Mech::ACTIVATE),
        "BG36_MidGameEffect_000t15" => def.kw.has(Kw::DIVINE_SHIELD),
        "BG36_MidGameEffect_000t16" => has(Mech::DEATHRATTLE),
        "BG36_MidGameEffect_000t21" => has(Mech::END_OF_TURN),
        "BG36_MidGameEffect_000t22" | "BG36_MidGameEffect_000t81" => !typed,
        // Battle Scars / Death's Embrace / Spell Siphon: only the one whose event you have
        // triggered most (36.2.1 hotfix; ties offer each).
        "BG36_MidGameEffect_000t28" | "BG36_MidGameEffect_000t28t" => {
            c.gift_start || st.is_some_and(|s| most_triggered(s, s.battlecries_triggered))
        }
        "BG36_MidGameEffect_000t29" | "BG36_MidGameEffect_000t29t" => {
            c.gift_start || st.is_some_and(|s| most_triggered(s, s.deathrattles_triggered))
        }
        "BG36_MidGameEffect_000t30" | "BG36_MidGameEffect_000t30t" => {
            c.gift_start || st.is_some_and(|s| most_triggered(s, s.tavern_spells_cast))
        }
        // Charisma: not on Tier 7 minions either (36.2.1).
        "BG36_MidGameEffect_000t3" => {
            !taunt && !avenge && !sets.charisma.contains(&def.cid) && def.tier < 7
        }
        "BG36_MidGameEffect_000t50" => def.races.intersects(Races::DRAGON),
        "BG36_MidGameEffect_000t60" => !taunt && typed,
        "BG36_MidGameEffect_000t62" => def.spellcraft.is_some(),
        // Polarization: Mechs, and not offered below Tavern Tier 3.
        "BG36_MidGameEffect_000t65" => {
            def.races.intersects(Races::MECH) && (c.gift_start || c.tier(side) >= 3)
        }
        "BG36_MidGameEffect_000t66" => def.races.intersects(Races::DEMON),
        "BG36_MidGameEffect_000t69" => {
            def.races.intersects(Races::MURLOC)
                && !def.kw.has(Kw::VENOMOUS)
                && !def.kw.has(Kw::POISONOUS)
        }
        "BG36_MidGameEffect_000t7" => {
            typed
                && !def
                    .races
                    .intersects(Races(Races::BEAST.0 | Races::UNDEAD.0))
        }
        // Hostility: not on Venomous / Poisonous minions (36.2.1).
        "BG36_MidGameEffect_000t71" => {
            typed && !avenge && !def.kw.has(Kw::VENOMOUS) && !def.kw.has(Kw::POISONOUS)
        }
        "BG36_MidGameEffect_000t72" => !def.races.intersects(Races::DRAGON),
        "BG36_MidGameEffect_000t74" => !avenge,
        "BG36_MidGameEffect_000t75" => !lobby.intersects(Races(Races::QUILBOAR.0 | Races::NAGA.0)),
        // Efficiency: Activate minions, except those whose effect would not usefully run
        // twice (36.6.2 fix; hsdata 253932 tag 4953).
        "BG36_MidGameEffect_000t78" => {
            has(Mech::ACTIVATE) && !EFFICIENCY_EXCLUDED.contains(&def.id.as_str())
        }
        "BG36_MidGameEffect_000t79" => avenge,
        // Consanguinity: only on Quilboar (official Developer Insight wording).
        "BG36_MidGameEffect_000t80" => def.races.intersects(Races::QUILBOAR),
        // Desecration (discards) and Worship (the Deity) belong to Aberration lobbies.
        "BG36_MidGameEffect_000t88" => lobby.intersects(Races::ABERRATION),
        "BG36_MidGameEffect_000t89" => c.player_ref(side).is_some_and(|p| p.deity.is_some()),
        _ => true,
    }
}

/// Set up a gifted minion's gift state (Steady Growth's stage by the offer turn).
pub fn gift_init(m: &mut Minion, gift: CardId, turn: u32) {
    if db().get(gift).id == "BG36_MidGameEffect_000t51" {
        let stage = (turn.saturating_sub(3)).min(2) as i32;
        let data: Vec<i32> = (0..m.attached.len())
            .map(|i| m.attachment_data_at(i))
            .collect();
        m.attachment_data = data;
        m.gift_data = stage; // Compatibility with saves predating attachment_data.
        if let Some(index) = m.attached.iter().rposition(|card| *card == gift) {
            m.attachment_data[index] = stage;
        }
    }
}

/// Dark Gifts in their turn window this turn (the normal offer pool): the data window, or a
/// turn the client offered the gift on (`SEEN_TURNS`).
pub fn window_gifts(c: &Ctx) -> Vec<CardId> {
    let turn = c.g.turn;
    c.g.gift_universe
        .iter()
        .copied()
        .filter(|g| {
            let card = db().get(*g);
            let data = match card.window {
                Some((lo, hi)) => turn >= lo as u32 && hi.is_none_or(|h| turn <= h as u32),
                None => true,
            };
            data || SEEN_TURNS
                .iter()
                .any(|(id, lo, hi)| *id == card.id && (*lo as u32..=*hi as u32).contains(&turn))
        })
        .collect()
}

/// Up to `n` Discover options: distinct minions from `cands`, each with a different Dark Gift from
/// `gifts` that fits it (`gift_fits`). The first option is of type `first_race` when possible (the
/// most-common-type guarantee). `golden`: Golden, without a Triple Reward. Offer weight of `gift`
/// on `def`: hsdata 253216 tag 4856 where present (50 for Persisting Horror, Invulnerability,
/// Titanic Strength and Worship; 200 for eleven others), 100 otherwise. Gilding is officially rare
/// but untagged: `dark_gift_rare_pct`. Harpy's Talons is `harpy_rally_mult` times as likely on
/// Rally minions.
fn gift_weight(c: &Ctx, gift: CardId, def: &crate::cards::CardDef) -> u64 {
    const TAGGED: [(&str, u64); 15] = [
        ("BG36_MidGameEffect_000t12", 50),
        ("BG36_MidGameEffect_000t60", 50),
        ("BG36_MidGameEffect_000t72", 50),
        ("BG36_MidGameEffect_000t89", 50),
        ("BG36_MidGameEffect_000t15", 200),
        ("BG36_MidGameEffect_000t16", 200),
        ("BG36_MidGameEffect_000t21", 200),
        ("BG36_MidGameEffect_000t22", 200),
        ("BG36_MidGameEffect_000t50", 200),
        ("BG36_MidGameEffect_000t62", 200),
        ("BG36_MidGameEffect_000t65", 200),
        ("BG36_MidGameEffect_000t66", 200),
        ("BG36_MidGameEffect_000t78", 200),
        ("BG36_MidGameEffect_000t80", 200),
        ("BG36_MidGameEffect_000t81", 200),
    ];
    let id = db().get(gift).id.as_str();
    if let Some(x) = TAGGED.iter().find(|x| x.0 == id) {
        x.1
    } else if id == "BG36_MidGameEffect_000t14" {
        c.g.cfg.dark_gift_rare_pct
    } else if id == "BG36_MidGameEffect_000t13" && def.mech.has(Mech::RALLY) {
        100 * c.g.cfg.harpy_rally_mult
    } else {
        100
    }
}

/// The Dark Gift button's options by place (`RulesConfig::dark_gift_slots`): option k is a minion
/// of Tier `slots[k].0` (and of type `slots[k].1` unless that is NONE), each minion and each gift
/// different, every gift fitting its minion (`gift_fits`, Gilding only at the lowest Tier of the
/// places). An option no candidate fills as asked is filled at its Tier without the type, then at
/// any of the turn's Tiers: an assumption, the client's fallback is not known. The places keep
/// their order.
pub fn slotted_gift_options(
    c: &mut Ctx,
    side: Side,
    cands: &[CardId],
    slots: &[(u8, Races)],
    gifts: &[CardId],
) -> Vec<Item> {
    let d = db();
    let turn = c.g.turn;
    let lowest = slots.iter().map(|s| s.0).min().unwrap_or(0);
    let mut cands: Vec<CardId> = cands.to_vec();
    let mut gifts: Vec<CardId> = gifts.to_vec();
    c.g.rng.shuffle(&mut cands);
    c.g.rng.shuffle(&mut gifts);
    let mut used_m: Vec<CardId> = vec![];
    let mut used_g: Vec<CardId> = vec![];
    let mut out = vec![];
    for &(tier, race) in slots {
        let mut found = None;
        'stages: for stage in 0..3 {
            for &x in &cands {
                let def = d.get(x);
                let wanted = match stage {
                    0 => def.tier == tier && (race.is_none() || super::goes_with_type(def, race)),
                    1 => def.tier == tier,
                    _ => true,
                };
                if used_m.contains(&x) || !wanted {
                    continue;
                }
                let fit: Vec<(CardId, u64)> = gifts
                    .iter()
                    .filter(|g| !used_g.contains(g) && gift_fits(c, side, **g, def, lowest))
                    .map(|g| (*g, gift_weight(c, *g, def)))
                    .collect();
                if let Some(&g) = c.weighted_distinct(fit, 1).first() {
                    found = Some((x, g));
                    break 'stages;
                }
            }
        }
        let Some((x, g)) = found else { break };
        used_m.push(x);
        used_g.push(g);
        let mut m = c.new_minion(side, x);
        m.attached.push(g);
        gift_init(&mut m, g, turn);
        out.push(Item::Minion(m));
    }
    out
}

pub fn gifted_options(
    c: &mut Ctx,
    side: Side,
    cands: &[CardId],
    n: usize,
    gifts: &[CardId],
    first_race: Races,
    golden: bool,
) -> Vec<Item> {
    gifted_options_with(c, side, cands, n, gifts, golden, &|k, def, _| {
        k != 0 || first_race.is_none() || def.races.intersects(first_race)
    })
}

/// `n` different minions of `cands`, each with a different Dark Gift that fits it. `want(k,
/// card, picked)` is what option `k` should be (a type, a Tier), given the minions picked
/// for the options before it; when no candidate that fits a gift satisfies it, the option
/// is filled without the wish.
pub fn gifted_options_with(
    c: &mut Ctx,
    side: Side,
    cands: &[CardId],
    n: usize,
    gifts: &[CardId],
    golden: bool,
    want: &dyn Fn(usize, &crate::cards::CardDef, &[CardId]) -> bool,
) -> Vec<Item> {
    let d = db();
    let turn = c.g.turn;
    let lowest = cands.iter().map(|x| d.get(*x).tier).min().unwrap_or(0);
    let mut cands: Vec<CardId> = cands.to_vec();
    let mut gifts: Vec<CardId> = gifts.to_vec();
    c.g.rng.shuffle(&mut cands);
    c.g.rng.shuffle(&mut gifts);
    let mut used_m: Vec<CardId> = vec![];
    let mut used_g: Vec<CardId> = vec![];
    let mut out = vec![];
    for k in 0..n {
        let mut found = None;
        for wish in [true, false] {
            for &x in &cands {
                let def = d.get(x);
                if used_m.contains(&x) || (wish && !want(k, def, &used_m)) {
                    continue;
                }
                let fit: Vec<(CardId, u64)> = gifts
                    .iter()
                    .filter(|g| !used_g.contains(g) && gift_fits(c, side, **g, def, lowest))
                    .map(|g| (*g, gift_weight(c, *g, def)))
                    .collect();
                if let Some(&g) = c.weighted_distinct(fit, 1).first() {
                    found = Some((x, g));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        let Some((x, g)) = found else { break };
        used_m.push(x);
        used_g.push(g);
        let card = if golden { d.golden_of(x) } else { x };
        let mut m = c.new_minion(side, card);
        if golden {
            m.triple_reward = false;
        }
        m.attached.push(g);
        gift_init(&mut m, g, turn);
        out.push(Item::Minion(m));
    }
    out
}

/// Static stat gifts (values per card text).
const STAT: Script = Script {
    on_attach: Some(stat_attach),
    ..E
};
/// Keyword gifts.
const KEYWORD: Script = Script {
    on_attach: Some(kw_attach),
    ..E
};
/// "Whenever you play a card, gain..." gifts.
const ON_PLAY: Script = Script {
    on_event: Some(on_play_fx),
    listens: ev::CARD_PLAYED,
    ..E
};

fn stat_attach(c: &mut Ctx, me: Me) {
    let (a, h) = match db().get(me.card).id.as_str() {
        "BG36_MidGameEffect_000t73" => (5, 5),
        "BG36_MidGameEffect_000t72" => (1000, 0),
        _ => (0, 0),
    };
    c.buff(me.eid, a, h);
}

fn kw_attach(c: &mut Ctx, me: Me) {
    let k = match db().get(me.card).id.as_str() {
        "BG36_MidGameEffect_000t13" => Kw::DIVINE_SHIELD.union(Kw::WINDFURY),
        "BG36_MidGameEffect_000t69" => Kw::VENOMOUS,
        "BG36_MidGameEffect_000t79" => Kw::STEALTH,
        "BG36_MidGameEffect_000t60" => Kw::IMMUNE_ATTACKING,
        "BG36_MidGameEffect_000t12" => Kw::REBORN,
        _ => Kw::NONE,
    };
    c.give_kw(me.eid, k);
}

fn on_play_fx(c: &mut Ctx, me: Me, e: &Event) {
    let Event::CardPlayed { side, eid, .. } = e else {
        return;
    };
    if *side != me.side || *eid == me.eid {
        return;
    }
    let (a, h) = match db().get(me.card).id.as_str() {
        "BG36_MidGameEffect_000t74" => (3, 0),
        "BG36_MidGameEffect_000t75" => (0, 3),
        "BG36_MidGameEffect_000t64" => (2, 2),
        "BG36_MidGameEffect_000t64t" => (4, 4),
        _ => (0, 0),
    };
    c.buff(me.eid, a, h);
}

fn soc(f: crate::script::Fx) -> Script {
    Script {
        start_of_combat: Some(f),
        ..E
    }
}

fn sacrifice(c: &mut Ctx, me: Me, attack: bool) {
    let (a, h) = c.deathrattle_stats(me.side, me.eid).unwrap_or((0, 0));
    if let Some(t) = c.random_friendly(me.side, |_| true) {
        if attack {
            c.buff(t, a, 0);
        } else {
            c.buff(t, 0, h);
        }
    }
}
