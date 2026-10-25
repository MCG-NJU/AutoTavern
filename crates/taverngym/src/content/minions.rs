//! Minion scripts (first content batch, chosen to exercise every mechanism family).
//! Numbers follow each card's normal and golden text in the bundle.

use super::*;

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // ------------------------------------------------------------------ tier 1
        (
            "BG25_013",
            Script {
                on_event: Some(rot_hide_gnoll),
                listens: ev::DIED,
                ..E
            },
        ),
        (
            "BG26_135",
            Script {
                battlecry: Some(|c, me, _| c.gold_next_turn(me.side, me.g())),
                ..E
            },
        ),
        (
            "BG26_146",
            Script {
                end_of_turn: Some(|c, me| c.buff(me.eid, 0, me.g())),
                ..E
            },
        ),
        (
            "BG28_300",
            Script {
                deathrattle: Some(|c, me| summon_n(c, me, SKELETON, 2 * me.g() as usize, false)),
                ..E
            },
        ),
        (
            "BG29_611",
            Script {
                deathrattle: Some(|c, me| summon_n(c, me, MICROBOT, 1, me.golden)),
                ..E
            },
        ),
        (
            "BG29_888",
            Script {
                rally: Some(|c, me, _| c.buff(me.eid, 2 * me.g(), 0)),
                ..E
            },
        ),
        (
            "BG31_149",
            Script {
                battlecry: Some(bubble_gunner),
                ..E
            },
        ),
        (
            "BG31_803",
            Script {
                deathrattle: Some(|c, me| summon_n(c, me, BEETLE, me.g() as usize, false)),
                ..E
            },
        ),
        (
            "BG31_815",
            Script {
                battlecry: Some(dune_dweller),
                ..E
            },
        ),
        (
            "BG36_098",
            Script {
                on_sell: Some(|c, me| give_n(c, me.side, TENTACLE, me.g() as usize)),
                ..E
            },
        ),
        (
            "BG36_110",
            Script {
                // Joyous: Deity +2/+1 (hotfix 36.6.2).
                battlecry: Some(|c, me, _| buff_deity(c, me.side, 2 * me.g(), me.g())),
                ..E
            },
        ),
        (
            "BG36_200",
            Script {
                rally: Some(flittering_bat),
                ..E
            },
        ),
        (
            "BG36_345",
            Script {
                activate: Some((1, TargetSpec::FriendlyOther, |c, me, t| {
                    c.buff(t, 3 * me.g(), 3 * me.g())
                })),
                ..E
            },
        ),
        (
            "BGS_004",
            Script {
                on_event: Some(wrath_weaver),
                listens: ev::PLAYED,
                ..E
            },
        ),
        (
            "BG20_100",
            Script {
                battlecry: Some(|c, me, _| give_n(c, me.side, BLOOD_GEM, 2 * me.g() as usize)),
                ..E
            },
        ),
        // ------------------------------------------------------------------ tier 2
        (
            "BG22_202",
            Script {
                on_sell: Some(tad),
                ..E
            },
        ),
        ("BG25_008", E), // Eternal Knight: bonus computed in engine::apply_bonuses
        (
            "BG25_011",
            Script {
                battlecry: Some(|c, me, _| {
                    let g = me.g();
                    c.bump_player_stat(me.side, |st| st.undead_atk += g)
                }),
                ..E
            },
        ),
        (
            "BG25_022",
            Script {
                deathrattle: Some(scarlet_skull),
                ..E
            },
        ),
        (
            "BG26_805",
            Script {
                start_of_combat: Some(humming_bird),
                ..E
            },
        ),
        (
            "BG26_963",
            Script {
                battlecry: Some(|c, me, _| synthesizer(c, me)),
                start_of_combat: Some(synthesizer),
                ..E
            },
        ),
        (
            "BG29_300",
            Script {
                on_event: Some(winterfinner),
                listens: ev::DAMAGED,
                ..E
            },
        ),
        (
            "BG31_801",
            Script {
                battlecry: Some(|c, me, _| {
                    let g = me.g();
                    c.bump_player_stat(me.side, |st| {
                        st.beetle_atk = st.beetle_atk.saturating_add(2 * g);
                        st.beetle_hp = st.beetle_hp.saturating_add(g);
                    })
                }),
                deathrattle: Some(|c, me| summon_n(c, me, BEETLE, me.g() as usize, false)),
                ..E
            },
        ),
        (
            "BG31_816",
            Script {
                on_sell: Some(|c, me| baller(c, me, true)),
                // "When you sell this, give your minions +1 Attack. Improve your future
                // Ballers."
                shown: Some(shown_baller),
                ..E
            },
        ),
        (
            "BG31_818",
            Script {
                on_sell: Some(|c, me| baller(c, me, false)),
                // "When you sell this, give your minions +1 Health. Improve your future
                // Ballers."
                shown: Some(shown_baller),
                ..E
            },
        ),
        (
            "BG32_235",
            Script {
                end_of_turn: Some(surfing_sylvar),
                ..E
            },
        ),
        (
            "BG36_116",
            Script {
                deathrattle: Some(underrot_spawn),
                ..E
            },
        ),
        (
            "BGS_115",
            Script {
                on_sell: Some(|c, me| give_n(c, me.side, WATER_DROPLET, me.g() as usize)),
                ..E
            },
        ),
        (
            "BG20_101",
            Script {
                rally: Some(|c, me, _| give_n(c, me.side, BLOOD_GEM, me.g() as usize)),
                ..E
            },
        ),
        (
            "BG36_360t3",
            Script {
                on_event: Some(paradox_played),
                listens: ev::CARD_PLAYED,
                ..E
            },
        ),
        // ------------------------------------------------------------------ tier 3
        (
            "BG24_500",
            Script {
                start_of_combat: Some(amber_guardian),
                ..E
            },
        ),
        (
            "BG25_010",
            Script {
                deathrattle: Some(|c, me| summon_n(c, me, HELPING_HAND, me.g() as usize, false)),
                ..E
            },
        ),
        (
            "BG30_125",
            Script {
                deathrattle: Some(|c, me| summon_n(c, me, SKELETON, 3 * me.g() as usize, false)),
                ..E
            },
        ),
        (
            "BG34_405",
            Script {
                flags: flag::TAUNT_WHILE_SHIELDED,
                avenge: Some((3, 3, |c, me| c.give_kw(me.eid, Kw::DIVINE_SHIELD))),
                ..E
            },
        ),
        (
            "BG27_005",
            Script {
                on_event: Some(hooktail),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        (
            "BG36_202",
            Script {
                deathrattle: Some(tasty_lobster),
                // "Deathrattle: Give a random friendly Beast +2/+1. Improve your future Tasty
                // Lobsters."
                shown: Some(|c, sh| {
                    let bonus = c.player_ref(sh.side).map_or(0, |p| p.stats.lobster_bonus);
                    let k = (1 + bonus) * sh.g();
                    vec![2 * k, k]
                }),
                ..E
            },
        ),
        (
            "BG36_207",
            Script {
                rally: Some(|c, me, _| {
                    for e in c.friendly(me.side) {
                        if e != me.eid {
                            c.buff(e, 4 * me.g(), me.g());
                        }
                    }
                }),
                ..E
            },
        ),
        (
            "BG28_309",
            Script {
                deathrattle: Some(mummifier),
                ..E
            },
        ),
        (
            "BG36_113",
            Script {
                deathrattle: Some(|c, me| buff_deity(c, me.side, 2 * me.g(), me.g())),
                ..E
            },
        ),
        ("BG36_360t6", E),  // Dark Paradox (aura): engine::apply_bonuses
        ("BG_DEEP_015", E), // Prosthetic Hand: magnetize rule in recruit::can_magnetize
        (
            "BG26_147",
            Script {
                start_of_turn: Some(|c, me| c.gain_gold(me.side, me.g())),
                ..E
            },
        ),
        // ------------------------------------------------------------------ tier 4
        (
            "BG26_817",
            Script {
                innate: Kw::CLEAVE,
                ..E
            },
        ),
        (
            "BGS_116",
            Script {
                battlecry: Some(|c, me, _| {
                    if let Some(p) = c.player(me.side) {
                        p.free_refreshes += 2 * me.g();
                    }
                }),
                ..E
            },
        ),
        (
            "BGS_123",
            Script {
                battlecry: Some(tavern_tempest),
                ..E
            },
        ),
        (
            "BG36_620",
            Script {
                start_of_combat: Some(boom_in_a_box),
                ..E
            },
        ),
        (
            "BG36_372",
            E, // Conditional +30/+30 is an idempotent owned-minion aura in apply_bonuses.
        ),
        (
            "BG34_690",
            Script {
                deathrattle: Some(plaguerunner),
                ..E
            },
        ),
        (
            "BG36_360t5",
            Script {
                rally: Some(paradox_rally),
                ..E
            },
        ),
        (
            "BG36_211",
            Script {
                on_event: Some(cage_gnawer),
                listens: ev::ATTACKS,
                ..E
            },
        ),
        (
            "BG36_106",
            Script {
                on_event: Some(cutthroat_kthir),
                listens: ev::DISCARDED,
                ..E
            },
        ),
        (
            "BG24_018",
            Script {
                on_sell: Some(|c, me| {
                    if c.player_ref(me.side).is_some_and(|p| p.stats.lost_last) {
                        // Sells for 5 (golden 10) instead of the base 1.
                        let base = c.cfg().sell_value;
                        c.gain_gold(me.side, 5 * me.g() - base);
                    }
                }),
                ..E
            },
        ),
        // ------------------------------------------------------------------ tier 5
        (
            "BG25_354",
            Script {
                flags: flag::EXTRA_DEATHRATTLE,
                ..E
            },
        ),
        (
            "BG_LOE_077",
            Script {
                flags: flag::EXTRA_BATTLECRY,
                ..E
            },
        ),
        // The minions Quest Rewards hand out (not in the pool).
        // Moira Bronzebeard (Secret Culprit): your Battlecries and Deathrattles trigger
        // twice (golden: three times).
        (
            "BG27_518",
            Script {
                flags: flag::EXTRA_BATTLECRY | flag::EXTRA_DEATHRATTLE,
                ..E
            },
        ),
        // Lightfang Enforcer (Ghastly Mask): at the end of your turn, give a friendly minion
        // of each type +4/+4 (+8/+8).
        (
            "BGS_009",
            Script {
                end_of_turn: Some(|c, me| {
                    for (t, _) in one_minion_per_type(c, me.side, |_| true) {
                        c.buff(t, 4 * me.g(), 4 * me.g());
                    }
                }),
                ..E
            },
        ),
        // Murozond (Gilnean War Horn): Battlecry: get a plain copy of a minion from your last
        // opponent's warband (golden: twice).
        (
            "BGS_043",
            Script {
                battlecry: Some(|c, me, _| {
                    let Some(p) = c.owner[me.side] else { return };
                    let Some(o) = c.g.last_fought(p) else {
                        return;
                    };
                    for _ in 0..me.g() {
                        let cards: Vec<CardId> =
                            c.g.fought_board(o)
                                .iter()
                                .map(|m| db().normal_of(m.card))
                                .collect();
                        if let Some(&x) = c.g.rng.pick(&cards) {
                            c.give_card(me.side, x);
                        }
                    }
                }),
                ..E
            },
        ),
        (
            "BG26_ICC_901",
            Script {
                flags: flag::EXTRA_END_OF_TURN,
                ..E
            },
        ),
        (
            "BG23_318",
            Script {
                deathrattle: Some(|c, me| {
                    if me.data != 0 {
                        c.destroy(me.data as EntityId);
                    }
                }),
                ..E
            },
        ),
        (
            "BGS_018",
            Script {
                deathrattle: Some(goldrinn),
                ..E
            },
        ),
        (
            "BGS_008",
            Script {
                deathrattle: Some(ghastcoiler),
                ..E
            },
        ),
        (
            "BG36_360t4",
            Script {
                on_create: Some(paradox_shield),
                ..E
            },
        ),
        (
            "BG36_097",
            Script {
                on_event: Some(ghursha),
                listens: ev::DISCARDED,
                ..E
            },
        ),
        (
            "BG31_809",
            Script {
                deathrattle: Some(turquoise_skitterer),
                ..E
            },
        ),
        (
            "BG36_514",
            Script {
                on_event: Some(barrier_banshee),
                listens: ev::REBORN,
                ..E
            },
        ),
        (
            "BGS_041",
            Script {
                on_event: Some(kalecgos),
                listens: ev::BATTLECRY,
                ..E
            },
        ),
        // ------------------------------------------------------------------ tier 6
        // Deathly Striker: Avenge (4) get random Undead (golden 2); Deathrattle summons them
        // from hand for this combat only.
        // Waveling: Deathrattle: after the Tavern is Refreshed this game, give a random
        // minion in it +4/+4 (golden: twice) -> persistent player effect.
        (
            "BG34_856",
            Script {
                deathrattle: Some(|c, me| c.add_player_effect(me.side, me.card, me.g())),
                ..refresh_effect(|c, me, e| refresh_random_tavern_minion(c, me, e, 4, 4))
            },
        ),
        // En-Djinn Blazer: Battlecry: same, +10/+10 (golden: twice).
        (
            "BG34_865",
            Script {
                battlecry: Some(|c, me, _| c.add_player_effect(me.side, me.card, me.g())),
                ..refresh_effect(|c, me, e| refresh_random_tavern_minion(c, me, e, 10, 10))
            },
        ),
        // Kelp Keeper: Activate (1): trigger a friendly minion's Battlecry (golden: twice).
        (
            "BG36_701",
            Script {
                activate: Some((
                    1,
                    TargetSpec::FriendlyHas(crate::script::Hook::Battlecry),
                    |c, me, t| {
                        for _ in 0..me.g() {
                            c.trigger_battlecry_ext(me.side, t);
                        }
                    },
                )),
                ..E
            },
        ),
        // Sky-hatch Runaway: Activate (1): trigger a friendly minion's Rally (golden: twice).
        (
            "BG36_243",
            Script {
                activate: Some((1, TargetSpec::FriendlyRallyTriggerable, |c, me, t| {
                    for _ in 0..me.g() {
                        c.trigger_rally(me.side, t, 0);
                    }
                })),
                ..E
            },
        ),
        // Deathstrider: after a friendly Rally minion attacks, trigger your left-most
        // Deathrattle (golden: twice).
        (
            "BG36_208",
            Script {
                on_event: Some(deathstrider),
                listens: ev::ATTACK_DONE,
                ..E
            },
        ),
        (
            "BG31_835",
            Script {
                avenge: Some((4, 4, deathly_striker_avenge)),
                deathrattle: Some(|c, me| {
                    let refs = c.cs[me.side]
                        .striker_refs
                        .remove(&me.eid)
                        .unwrap_or_default();
                    for (i, hid) in refs.into_iter().enumerate() {
                        c.summon_from_hand(me.side, hid, me.pos + i);
                    }
                }),
                ..E
            },
        ),
        (
            "BGS_040",
            Script {
                deathrattle: Some(nadina),
                ..E
            },
        ),
        (
            "BG25_009",
            Script {
                deathrattle: Some(|c, me| summon_n(c, me, ETERNAL_KNIGHT, 1, me.golden)),
                ..E
            },
        ),
        (
            "BG22_403",
            Script {
                end_of_turn: Some(young_murk_eye),
                ..E
            },
        ),
        (
            "BG36_849",
            Script {
                rally: Some(|c, me, _| c.give_kw(me.eid, Kw::DIVINE_SHIELD)),
                start_of_combat: Some(|c, me| {
                    for _ in 0..me.g() {
                        crate::combat::attack_now(c, me.side, me.eid);
                    }
                }),
                ..E
            },
        ),
        (
            "BG36_360t9",
            Script {
                deathrattle: Some(paradox_golem),
                ..E
            },
        ),
        (
            "BG36_356",
            Script {
                activate: Some((1, TargetSpec::FriendlyOther, |c, me, t| {
                    let v = 50 * me.g();
                    c.set_stats(t, v, v)
                })),
                ..E
            },
        ),
        // ------------------------------------------------------------------ discard / linked pairs
        ("BG36_099", DISCARD_ACTIVATE), // Brain Rotter
        ("BG36_311", DISCARD_ACTIVATE), // Abyssal Envoy
        ("BG28_582", DISCARD_ACTIVATE), // Mangled Bandit
        ("BG36_300", DISCARD_ACTIVATE), // N'raqi Frostcaller
        ("BG36_312", DISCARD_ACTIVATE), // Mindbending Recruiter
        (
            "BG36_100",
            Script {
                on_sell: Some(|c, me| linked_pairs(c, me, false)),
                ..E
            },
        ), // Wandering Willbreaker
        (
            "BG36_308",
            Script {
                on_sell: Some(|c, me| linked_pairs(c, me, true)),
                ..E
            },
        ), // Faceless Operative
        // ------------------------------------------------------------------ deities
        ("BGFYM_000", E), // C'Thun: content::deity_awakened
        (
            "BGFYM_011",
            Script {
                deathrattle: Some(yshaarj),
                ..E
            },
        ),
    ]
}

// ---------------------------------------------------------------------- tier 1

fn rot_hide_gnoll(c: &mut Ctx, me: Me, e: &Event) {
    if let Event::Died { side, .. } = e
        && *side == me.side
        && c.combat
    {
        c.buff(me.eid, me.g(), 0);
    }
}

fn bubble_gunner(c: &mut Ctx, me: Me, _t: EntityId) {
    for _ in 0..me.g() {
        let Some(m) = c.minion(me.eid) else { return };
        let missing: Vec<Kw> = Kw::BONUS.iter().copied().filter(|k| !m.has(*k)).collect();
        if let Some(&k) = c.g.rng.pick(&missing) {
            c.give_kw(me.eid, k);
        }
    }
}

fn dune_dweller(c: &mut Ctx, me: Me, _t: EntityId) {
    let g = me.g();
    if let Some(p) = c.player(me.side) {
        p.stats.tavern_elemental_atk = p.stats.tavern_elemental_atk.saturating_add(g);
        p.stats.tavern_elemental_hp = p.stats.tavern_elemental_hp.saturating_add(g);
    }
    c.buff_tavern(me.side, g, g, Races::ELEMENTAL);
}

fn flittering_bat(c: &mut Ctx, me: Me, _t: EntityId) {
    let pos = c.pos_right_of(me.eid);
    c.summon_card(me.side, pos, cid(FORAGING_BAT), me.g() as usize);
}

fn wrath_weaver(c: &mut Ctx, me: Me, e: &Event) {
    let Event::Played { side, played, .. } = e else {
        return;
    };
    if *side != me.side || played.source == me.eid || !played.races.intersects(Races::DEMON) {
        return;
    }
    for _ in 0..me.g() {
        c.hero_damage(me.side, 1);
        c.buff(me.eid, 2, 2);
    }
}

// ---------------------------------------------------------------------- tier 2

fn tad(c: &mut Ctx, me: Me) {
    let tier = c.tier(me.side);
    for _ in 0..me.g() {
        if let Some(card) = c.random_minion(|d| d.races.intersects(Races::MURLOC) && d.tier <= tier)
        {
            c.give_card(me.side, card);
        }
    }
}

fn scarlet_skull(c: &mut Ctx, me: Me) {
    if let Some(t) = c.random_friendly(me.side, |m| m.is(Races::UNDEAD)) {
        c.buff(t, me.g(), 2 * me.g());
    }
}

/// Humming Bird: a combat-wide aura (`SideState::beast_atk_aura`), so Beasts summoned later
/// get it too, once, even after the Bird dies.
fn humming_bird(c: &mut Ctx, me: Me) {
    c.cs[me.side].beast_atk_aura += me.g();
    for e in c.friendly_where(me.side, |m| m.is(Races::BEAST)) {
        c.buff(e, me.g(), 0);
    }
}

fn synthesizer(c: &mut Ctx, me: Me) {
    for e in c.friendly_where(me.side, |m| m.is(Races::DRAGON) && m.eid != me.eid) {
        c.buff(e, me.g(), me.g());
    }
}

fn winterfinner(c: &mut Ctx, me: Me, e: &Event) {
    let Event::Damaged { eid, .. } = e else {
        return;
    };
    if *eid != me.eid {
        return;
    }
    let Some(p) = c.player_ref(me.side) else {
        return;
    };
    let targets: Vec<EntityId> = p
        .hand
        .iter()
        .filter(|h| matches!(h.item, Item::Minion(_)))
        .map(|h| h.hid)
        .collect();
    if let Some(&t) = c.g.rng.pick(&targets) {
        c.buff(t, 2 * me.g(), me.g());
    }
}

fn baller(c: &mut Ctx, me: Me, attack: bool) {
    let bonus = c
        .player_ref(me.side)
        .map(|p| p.stats.baller_bonus)
        .unwrap_or(0);
    let v = (1 + bonus) * me.g();
    for e in c.friendly(me.side) {
        if attack {
            c.buff(e, v, 0);
        } else {
            c.buff(e, 0, v);
        }
    }
    if let Some(p) = c.player(me.side) {
        p.stats.baller_bonus = p.stats.baller_bonus.saturating_add(1);
    }
}

/// `Script::shown` of Fire Baller and Snow Baller: the amount `baller` gives now.
fn shown_baller(c: &Ctx, sh: &crate::script::Shown) -> Vec<i32> {
    let bonus = c.player_ref(sh.side).map_or(0, |p| p.stats.baller_bonus);
    vec![(1 + bonus) * sh.g()]
}

fn surfing_sylvar(c: &mut Ctx, me: Me) {
    let goldens = c.boards[me.side].iter().filter(|m| m.golden).count() as i32;
    for _ in 0..(1 + goldens) {
        for n in c.neighbors(me.eid) {
            c.buff(n, me.g(), 0);
        }
    }
}

fn underrot_spawn(c: &mut Ctx, me: Me) {
    summon_n(c, me, TENTACLE, me.g() as usize, false);
    for e in c.friendly(me.side) {
        c.buff(e, me.g(), 0);
    }
}

fn paradox_played(c: &mut Ctx, me: Me, e: &Event) {
    if let Event::CardPlayed { side, eid, .. } = e
        && *side == me.side
        && (*eid != me.eid || c.magnet_play)
    {
        c.buff(me.eid, 3 * me.g(), 0);
    }
}

// ---------------------------------------------------------------------- tier 3

fn amber_guardian(c: &mut Ctx, me: Me) {
    let mut cands = c.friendly_where(me.side, |m| m.is(Races::DRAGON) && m.eid != me.eid);
    c.g.rng.shuffle(&mut cands);
    for e in cands.into_iter().take(me.g() as usize) {
        c.buff(e, 2, 2);
        c.give_kw(e, Kw::DIVINE_SHIELD);
    }
}

fn hooktail(c: &mut Ctx, me: Me, e: &Event) {
    if let Event::SpellCast {
        side, tavern: true, ..
    } = e
        && *side == me.side
    {
        for e in c.friendly(me.side) {
            c.buff(e, me.g(), 0);
        }
    }
}

fn tasty_lobster(c: &mut Ctx, me: Me) {
    let bonus = c
        .player_ref(me.side)
        .map(|p| p.stats.lobster_bonus)
        .unwrap_or(0);
    let k = (1 + bonus) * me.g();
    if let Some(t) = c.random_friendly(me.side, |m| m.is(Races::BEAST)) {
        c.buff(t, 2 * k, k);
    }
    if let Some(p) = c.player(me.side) {
        p.stats.lobster_bonus = p.stats.lobster_bonus.saturating_add(1);
    }
}

fn mummifier(c: &mut Ctx, me: Me) {
    // Not itself or another Mummifier.
    let own = db().normal_of(me.card);
    let mut cands = c.friendly_where(me.side, |m| {
        m.is(Races::UNDEAD)
            && m.eid != me.eid
            && db().normal_of(m.card) != own
            && !m.has(Kw::REBORN)
    });
    c.g.rng.shuffle(&mut cands);
    for e in cands.into_iter().take(me.g() as usize) {
        c.give_kw(e, Kw::REBORN);
    }
}

// ---------------------------------------------------------------------- tier 4

fn tavern_tempest(c: &mut Ctx, me: Me, _t: EntityId) {
    let tier = c.tier(me.side);
    for _ in 0..me.g() {
        if let Some(card) =
            c.random_minion(|d| d.races.intersects(Races::ELEMENTAL) && d.tier <= tier)
        {
            c.give_card(me.side, card);
        }
    }
}

fn boom_in_a_box(c: &mut Ctx, me: Me) {
    for _ in 0..me.g() {
        let all: Vec<EntityId> = (0..2)
            .flat_map(|s| c.friendly(s))
            .filter(|e| *e != me.eid)
            .collect();
        for e in all {
            c.damage(me.eid, e, 3);
        }
        c.resolve_deaths();
    }
}

fn plaguerunner(c: &mut Ctx, me: Me) {
    let v = if c.combat { 2 } else { 4 } * me.g();
    c.bump_player_stat(me.side, |st| {
        st.undead_atk = st.undead_atk.saturating_add(v)
    });
}

fn paradox_rally(c: &mut Ctx, me: Me, _t: EntityId) {
    let Some(race) = c.most_common_race(me.side) else {
        return;
    };
    let tier = c.tier(me.side);
    for _ in 0..me.g() {
        if let Some(card) = c.random_minion(|d| d.races.intersects(race) && d.tier <= tier) {
            c.give_card(me.side, card);
        }
    }
}

fn cage_gnawer(c: &mut Ctx, me: Me, e: &Event) {
    let Event::Attacks { side, attacker, .. } = e else {
        return;
    };
    if *side != me.side || !c.minion(*attacker).is_some_and(|m| m.is(Races::BEAST)) {
        return;
    }
    for b in c.friendly_where(me.side, |m| m.is(Races::BEAST)) {
        c.buff(b, 2 * me.g(), me.g());
    }
}

fn cutthroat_kthir(c: &mut Ctx, me: Me, e: &Event) {
    if e.side() == me.side {
        c.buff(me.eid, 4 * me.g(), 4 * me.g());
        buff_deity(c, me.side, 4 * me.g(), 4 * me.g());
    }
}

// ---------------------------------------------------------------------- tier 5

/// Goldrinn: Beasts have +7/+7 until next turn -- in combat a combat-wide aura, so Beasts summoned
/// later get it too (33.2.2 wording "rest of this combat").
fn goldrinn(c: &mut Ctx, me: Me) {
    let k = 7 * me.g();
    if c.combat {
        c.cs[me.side].beast_atk_aura += k;
        c.cs[me.side].beast_hp_aura += k;
    }
    for e in c.friendly_where(me.side, |m| m.is(Races::BEAST)) {
        c.buff_temp(e, k, k);
    }
}

fn ghastcoiler(c: &mut Ctx, me: Me) {
    let d = db();
    let own = d.normal_of(me.card);
    let mut pos = me.pos;
    // Of the owner's Tavern Tier or below, like every random summon.
    let tier = c.tier(me.side);
    for _ in 0..(2 * me.g()) {
        let Some(card) =
            c.random_minion(|x| x.mech.has(Mech::DEATHRATTLE) && x.cid != own && x.tier <= tier)
        else {
            break;
        };
        let m = c.new_minion(me.side, card);
        if c.summon(me.side, pos, m).is_some() {
            pos += 1;
        }
    }
}

fn paradox_shield(m: &mut Minion) {
    m.shield_max = if m.golden { 4 } else { 3 };
    m.shield_hits = m.shield_max;
}

fn ghursha(c: &mut Ctx, me: Me, e: &Event) {
    if e.side() != me.side {
        return;
    }
    for x in c.friendly(me.side) {
        if x != me.eid {
            c.buff(x, 4 * me.g(), 4 * me.g());
        }
    }
}

fn turquoise_skitterer(c: &mut Ctx, me: Me) {
    let v = 5 * me.g();
    c.bump_player_stat(me.side, |st| {
        st.beetle_atk = st.beetle_atk.saturating_add(v);
        st.beetle_hp = st.beetle_hp.saturating_add(v);
    });
    summon_n(c, me, BEETLE, me.g() as usize, false);
}

fn barrier_banshee(c: &mut Ctx, me: Me, e: &Event) {
    if e.side() == me.side {
        c.give_kw(me.eid, Kw::DIVINE_SHIELD);
        c.buff(me.eid, 8 * me.g(), 8 * me.g());
    }
}

fn kalecgos(c: &mut Ctx, me: Me, e: &Event) {
    if e.side() != me.side {
        return;
    }
    for x in c.friendly_where(me.side, |m| m.is(Races::DRAGON)) {
        c.buff(x, 2 * me.g(), 2 * me.g());
    }
}

// ---------------------------------------------------------------------- tier 6

fn nadina(c: &mut Ctx, me: Me) {
    let mut cands = c.friendly_where(me.side, |m| {
        m.is(Races::DRAGON) && !m.has(Kw::DIVINE_SHIELD)
    });
    c.g.rng.shuffle(&mut cands);
    for e in cands.into_iter().take(3 * me.g() as usize) {
        c.give_kw(e, Kw::DIVINE_SHIELD);
    }
}

fn young_murk_eye(c: &mut Ctx, me: Me) {
    let mut ns = c.neighbors(me.eid);
    ns.retain(|e| {
        c.minion(*e)
            .is_some_and(|m| crate::script::minion_has(m, crate::script::Hook::Battlecry))
    });
    // 36.6.3: both adjacent minions (golden: twice each).
    for e in ns {
        for _ in 0..me.g() {
            c.trigger_battlecry_ext(me.side, e);
        }
    }
}

fn paradox_golem(c: &mut Ctx, me: Me) {
    // The dead minion is gone from the board (its stats are in the death log); a triggered
    // Deathrattle reads the living minion.
    let (a, h) = c
        .deathrattle_stats(me.side, me.eid)
        .unwrap_or_else(|| (db().get(me.card).atk, db().get(me.card).hp));
    let mut m = c.new_minion(me.side, cid(GOLEM));
    m.set_stats(a.saturating_mul(me.g()), h.saturating_mul(me.g()));
    c.summon(me.side, me.pos, m);
}

// ---------------------------------------------------------------------- deities

fn yshaarj(c: &mut Ctx, me: Me) {
    let n = 2 * me.g() as usize;
    let d = db();
    // Which minions died as Aberrations is recorded at their death (their types then, e.g. from the
    // Amalgamation gift), so those are the ones that come back -- as the plain card with its
    // maximum stats, without keywords or enchantments added later.
    let dead: Vec<(CardId, i32, i32)> = c.cs[me.side]
        .dead
        .iter()
        .filter(|r| {
            let deity = crate::flow::DEITIES
                .iter()
                .any(|x| d.normal_of(r.card) == d.id(x));
            r.races.intersects(Races::ABERRATION) && !deity
        })
        .map(|r| (r.card, r.peak_atk, r.peak_hp))
        .take(n)
        .collect();
    let mut pos = me.pos;
    for (card, a, h) in dead {
        let mut m = c.new_minion(me.side, card);
        m.set_stats(a, h.max(1));
        if c.summon(me.side, pos, m).is_some() {
            pos += 1;
            let st = &mut c.cstats[me.side];
            st.returned_max = st
                .returned_max
                .max(a.saturating_add(h.max(1)).max(0) as u32);
        }
    }
}

// ---------------------------------------------------------------------- discard

/// "Activate (0): Discard a card to..." -- the target is the hand card to discard.
const DISCARD_ACTIVATE: Script = Script {
    activate: Some((0, TargetSpec::Hand, discard_then)),
    ..E
};

fn discard_then(c: &mut Ctx, me: Me, t: EntityId) {
    let g = me.g();
    let id = db().get(db().normal_of(me.card)).id.as_str();
    // N'raqi Frostcaller: "Discard a card for your Tavern spells to give an extra +1/+1": the
    // bonus is there when the discard resolves - a discarded Sludge Corrosion casts twice
    // with it. Only this card: the others act after the discard.
    if id == "BG36_300" {
        let locked = c
            .player_ref(me.side)
            .and_then(|p| p.hand.iter().find(|h| h.hid == t))
            .is_none_or(|h| !crate::recruit::discardable(h));
        if locked {
            return;
        }
        if let Some(p) = c.player(me.side) {
            p.stats.spell_atk = p.stats.spell_atk.saturating_add(g);
            p.stats.spell_hp = p.stats.spell_hp.saturating_add(g);
        }
        crate::recruit::discard_card(c, me.side, t);
        return;
    }
    if crate::recruit::discard_card(c, me.side, t).is_none() {
        return;
    }
    match id {
        "BG36_099" => buff_deity(c, me.side, 2 * g, 2 * g),
        "BG36_311" => {
            let tier = c.tier(me.side);
            for _ in 0..g {
                if let Some(s) = c.random_spell(|d| d.tier <= tier) {
                    c.give_card(me.side, s);
                }
            }
        }
        "BG28_582" => give_n(c, me.side, BLOOD_GEM, 3 * g as usize),
        // Mindbending Recruiter: get 1 (2) random Aberrations.
        "BG36_312" => {
            let tier = c.tier(me.side);
            for _ in 0..g {
                if let Some(a) =
                    c.random_minion(|d| d.races.intersects(Races::ABERRATION) && d.tier <= tier)
                {
                    c.give_card(me.side, a);
                }
            }
        }
        _ => {}
    }
}

/// Sell: get 2 linked random Tavern spells / Aberrations (play one, the other is discarded).
/// Golden: 4 in one group -- play two, the rest are discarded.
fn linked_pairs(c: &mut Ctx, me: Me, aberrations: bool) {
    let tier = c.tier(me.side);
    let pick = |c: &mut Ctx| {
        if aberrations {
            c.random_minion(|d| d.races.intersects(Races::ABERRATION) && d.tier <= tier)
        } else {
            c.random_spell(|d| d.tier <= tier)
        }
    };
    if me.g() == 1 {
        if let (Some(a), Some(b)) = (pick(c), pick(c)) {
            crate::recruit::give_linked_pair(c, me.side, a, b);
        }
    } else {
        let cards: Vec<CardId> = (0..4).filter_map(|_| pick(c)).collect();
        crate::recruit::give_linked_group(c, me.side, &cards, 2);
    }
}

fn deathly_striker_avenge(c: &mut Ctx, me: Me) {
    let tier = c.tier(me.side);
    for _ in 0..me.g() {
        let Some(card) = c.random_minion(|d| d.races.intersects(Races::UNDEAD) && d.tier <= tier)
        else {
            return;
        };
        if let Some(hid) = c.give_card(me.side, card) {
            c.cs[me.side]
                .striker_refs
                .entry(me.eid)
                .or_default()
                .push(hid);
        }
    }
}

fn deathstrider(c: &mut Ctx, me: Me, e: &Event) {
    let Event::AttackDone { side, attacker } = e else {
        return;
    };
    if *side != me.side
        || !c
            .minion(*attacker)
            .is_some_and(|m| crate::script::minion_has(m, crate::script::Hook::Rally))
    {
        return;
    }
    for _ in 0..me.g() {
        let left = c.boards[me.side]
            .iter()
            .find(|m| m.alive() && crate::script::minion_has(m, crate::script::Hook::Deathrattle))
            .map(|m| m.eid);
        if let Some(t) = left {
            c.trigger_deathrattles(me.side, t);
        }
    }
}
