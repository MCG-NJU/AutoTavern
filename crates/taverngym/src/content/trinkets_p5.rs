//! Trinkets: trigger multipliers and "trigger X" effects. War Drum and Dragon's Eye change the
//! Battlecry count in `Ctx::trigger_battlecry`; Deathly Phylactery is in `Ctx::run_deathrattles`.

use super::*;
use crate::script::{Hook, minion_has};

const REPAIR_JOB: &str = "BG36_624";
/// `SideState::stored` key for Soul Fermenter's destroyed minions.
const FERMENTER_KEY: EntityId = EntityId::MAX;

fn with_hook(c: &Ctx, side: Side, h: Hook) -> Vec<EntityId> {
    c.friendly_where(side, |m| minion_has(m, h))
}

fn cast_repair_job(c: &mut Ctx, side: Side, target: Option<EntityId>) {
    if let Some(t) = target {
        c.you_cast(side, cid(REPAIR_JOB), 0, t, 0);
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // War Drum / Dragon's Eye: `trigger_battlecry`.
        (
            "BG32_MagicItem_416",
            Script {
                // "One Battlecry each turn triggers two extra times. (1 left!)"
                shown: Some(|c, sh| vec![(1 - c.turn_data(sh.side, sh.card)).max(0)]),
                ..E
            },
        ),
        ("BG36_MagicItem_215", E),
        // Young Murk-Eye Sticker: at the end of your turn, trigger your left- and right-most
        // minions' Battlecries.
        (
            "BG35_MagicItem_752",
            Script {
                end_of_turn: Some(|c, me| {
                    let mut v: Vec<EntityId> = [c.leftmost(me.side), c.rightmost(me.side)]
                        .into_iter()
                        .flatten()
                        .collect();
                    v.dedup();
                    for e in v {
                        if c.minion(e).is_some_and(|m| minion_has(m, Hook::Battlecry)) {
                            c.trigger_battlecry_ext(me.side, e);
                        }
                    }
                }),
                ..E
            },
        ),
        // Lovely Locket: after you cast a spell on a friendly minion, this casts it on another
        // (random legal) friendly minion. Its own casts don't repeat (card data = busy).
        (
            "BG36_MagicItem_211",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::SpellCast {
                        side,
                        card,
                        target,
                        data,
                        opt,
                        ..
                    } = e
                    else {
                        return;
                    };
                    // The target was a friendly minion when the spell was cast, even if the
                    // spell destroyed it (Butchering): `Ctx::cast_on_friendly`.
                    if *side != me.side
                        || *target == 0
                        || !c.cast_on_friendly
                        || c.card_data(me.side, me.card) != 0
                    {
                        return;
                    }
                    let spec = effect_target_spec(*card).unwrap_or(TargetSpec::Friendly);
                    let others: Vec<EntityId> = c
                        .legal_targets(me.side, spec, 0)
                        .into_iter()
                        .filter(|x| x != target && c.friendly(me.side).contains(x))
                        .collect();
                    let Some(&t) = c.g.rng.pick(&others) else {
                        return;
                    };
                    let (card, data, opt) = (*card, *data, *opt);
                    c.set_card_data(me.side, me.card, 1);
                    c.cast_spell(me.side, card, data, t, opt);
                    c.set_card_data(me.side, me.card, 0);
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Wolfhead Flail: at the end of your turn, trigger your two left-most Deathrattles.
        (
            "BG36_MagicItem_212",
            Script {
                end_of_turn: Some(|c, me| {
                    for e in with_hook(c, me.side, Hook::Deathrattle).into_iter().take(2) {
                        c.trigger_deathrattles(me.side, e);
                    }
                }),
                ..E
            },
        ),
        // Insurrectionist's Blade: at the end of your turn, trigger all friendly Rallies.
        (
            "BG36_MagicItem_214",
            Script {
                end_of_turn: Some(|c, me| {
                    for e in with_hook(c, me.side, Hook::Rally) {
                        if c.on_board(e) {
                            c.trigger_rally(me.side, e, 0);
                        }
                    }
                }),
                ..E
            },
        ),
        // Funeral Wreath: after a friendly minion is Reborn, get a plain copy of it (3 times
        // per combat).
        (
            "BG36_MagicItem_217",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Reborn { side, eid } = e
                        && *side == me.side
                        && c.combat_data(me.side, me.card) < 3
                        && let Some(card) = c.minion(*eid).map(|m| db().normal_of(m.card))
                    {
                        c.add_combat_data(me.side, me.card, 1);
                        c.give_card(me.side, card);
                    }
                }),
                listens: ev::REBORN,
                ..E
            },
        ),
        // Soul Fermenter: SoC destroy your 3 left-most minions; after your last minion dies,
        // resummon them (as they were at Start of Combat).
        (
            "BG35_MagicItem_732",
            Script {
                start_of_combat: Some(|c, me| {
                    let ids: Vec<EntityId> = c.friendly(me.side).into_iter().take(3).collect();
                    let copies: Vec<Minion> =
                        ids.iter().filter_map(|e| c.minion(*e).cloned()).collect();
                    c.cs[me.side]
                        .stored
                        .insert(FERMENTER_KEY - u32::from(me.card.0), copies);
                    for e in ids {
                        c.destroy(e);
                    }
                    c.resolve_deaths();
                }),
                on_event: Some(|c, me, e| {
                    if !c.combat || !matches!(e, Event::Died { side, .. } if *side == me.side) {
                        return;
                    }
                    if !c.friendly(me.side).is_empty() {
                        return;
                    }
                    let Some(saved) = c.cs[me.side]
                        .stored
                        .remove(&(FERMENTER_KEY - u32::from(me.card.0)))
                    else {
                        return;
                    };
                    for m in saved {
                        let mut m = m;
                        m.eid = c.g.alloc_eid();
                        m.combat_only = true;
                        m.pool_copies = 0;
                        m.attacked = false;
                        let pos = c.boards[me.side].len();
                        c.summon(me.side, pos, m);
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Sphere of Memory: at the end of your turn, get 3 copies of the last Tavern spell you
        // cast.
        (
            "BG36_MagicItem_372",
            Script {
                end_of_turn: Some(|c, me| {
                    give_last_tavern_spell(c, me.side, 3);
                }),
                ..E
            },
        ),
        // Emergency Gearblade: at the end of your turn, cast Repair Job on your left-most Mech.
        (
            "BG36_MagicItem_812",
            Script {
                end_of_turn: Some(|c, me| {
                    let t = c
                        .friendly_where(me.side, |m| m.is(Races::MECH))
                        .first()
                        .copied();
                    cast_repair_job(c, me.side, t);
                }),
                ..E
            },
        ),
        // Spell-powered Wrench: after you play a Magnetic minion (incl. Magnetizing it), cast
        // Repair Job on a random friendly Mech.
        (
            "BG32_MagicItem_170",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::CardPlayed { side, card, .. } = e else {
                        return;
                    };
                    let d = db().get(*card);
                    if *side != me.side
                        || d.kind != crate::cards::CardKind::Minion
                        || !d.mech.has(Mech::MAGNETIC)
                    {
                        return;
                    }
                    let mechs = c.friendly_where(me.side, |m| m.is(Races::MECH));
                    let t = c.g.rng.pick(&mechs).copied();
                    cast_repair_job(c, me.side, t);
                }),
                listens: ev::CARD_PLAYED,
                ..E
            },
        ),
    ]
}
