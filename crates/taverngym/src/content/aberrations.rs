//! Aberrations and all-type minions (phase 1).

use super::*;

const SLUDGE: &str = "BG36_301t"; // Sludge Corrosion
const CHAMBER: &str = "BG36_371"; // Energizing Chamber
const TEA_SET: &str = "BG28_888"; // Misplaced Tea Set

fn one_cost_spells(c: &mut Ctx, side: Side, n: i32) {
    let tier = c.tier(side);
    for _ in 0..n {
        if let Some(s) = c.random_spell(|d| d.cost == 1 && d.tier <= tier) {
            c.give_card(side, s);
        }
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Unwilling Slacker: Deathrattle: get 1 (2) random 1-Cost Tavern spells.
        (
            "BG36_101",
            Script {
                deathrattle: Some(|c, me| one_cost_spells(c, me.side, me.g())),
                ..E
            },
        ),
        // Vicious Mindslasher: whenever you cast a Tavern spell, give this and your Deity +1/+2
        // (+2/+4; hotfix 36.6.2).
        (
            "BG36_108",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, tavern: true, .. } if *side == me.side)
                    {
                        c.buff(me.eid, me.g(), 2 * me.g());
                        buff_deity(c, me.side, me.g(), 2 * me.g());
                        // Mindslasher Portrait: adjacent minions get the stats too.
                        if has_trinket(c, me.side, "BG36_MagicItem_402") {
                            for n in c.neighbors(me.eid) {
                                c.buff(n, me.g(), 2 * me.g());
                            }
                        }
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Fetid Corroder: Battlecry: get 1 (2) Sludge Corrosions.
        (
            "BG36_112",
            Script {
                battlecry: Some(|c, me, _| give_n(c, me.side, SLUDGE, me.g() as usize)),
                ..E
            },
        ),
        // Nightmare Corroder: at the end of your turn, get 1 (2) Sludge Corrosions (36.6.3; a
        // Deathrattle before).
        (
            "BG36_115",
            Script {
                end_of_turn: Some(|c, me| give_n(c, me.side, SLUDGE, me.g() as usize)),
                ..E
            },
        ),
        // Parasitic Fleshling: end of turn, left-most minion +2/+2 (+4/+4), improved (by the
        // base) for each card discarded this game.
        (
            "BG36_114",
            Script {
                end_of_turn: Some(|c, me| {
                    let n = c.player_ref(me.side).map_or(0, |p| p.stats.cards_discarded);
                    let k = 2 * me.g() * (1 + n);
                    if let Some(t) = c.leftmost(me.side) {
                        c.buff(t, k, k);
                    }
                }),
                // "At the end of your turn, give your left-most minion +2/+2. (Improved by
                // each card you've discarded this game!)"
                shown: Some(|c, sh| {
                    let n = c.player_ref(sh.side).map_or(0, |p| p.stats.cards_discarded);
                    let k = 2 * sh.g() * (1 + n);
                    vec![k, k]
                }),
                ..E
            },
        ),
        // De-volition-ist: after this attacks, deal damage equal to its Attack (double) to the
        // highest-Health enemy minion.
        (
            "BG36_102",
            Script {
                after_attack: Some(|c, me| {
                    let Some(a) = c.minion(me.eid).map(|m| m.atk) else {
                        return;
                    };
                    let keyed: Vec<(EntityId, i64)> = c.boards[1 - me.side]
                        .iter()
                        .filter(|m| m.alive())
                        .map(|m| (m.eid, m.hp as i64))
                        .collect();
                    if let Some(t) = c.pick_max(keyed) {
                        c.damage(me.eid, t, a.saturating_mul(me.g()));
                    }
                }),
                ..E
            },
        ),
        // N'raqi Sapper: Battlecry and Deathrattle: get 1 (2) Energizing Chambers.
        (
            "BG36_103",
            Script {
                battlecry: Some(|c, me, _| give_n(c, me.side, CHAMBER, me.g() as usize)),
                deathrattle: Some(|c, me| give_n(c, me.side, CHAMBER, me.g() as usize)),
                ..E
            },
        ),
        // Faceless Converter: Deathrattle: Deity +2/+1 (+4/+2; hotfix 36.6.2), improved (by the
        // base) by each Tavern spell cast this game.
        (
            "BG36_318",
            Script {
                deathrattle: Some(|c, me| {
                    let n = c
                        .player_ref(me.side)
                        .map_or(0, |p| p.stats.tavern_spells_cast);
                    let k = me.g() * (1 + n);
                    buff_deity(c, me.side, 2 * k, k);
                }),
                // "Deathrattle: Give your Deity +2/+1. (Improved by each Tavern spell you've
                // cast this game!)"
                shown: Some(|c, sh| {
                    let n = c
                        .player_ref(sh.side)
                        .map_or(0, |p| p.stats.tavern_spells_cast);
                    let k = sh.g() * (1 + n);
                    vec![2 * k, k]
                }),
                ..E
            },
        ),
        // Mysterious K'Thir: end of turn, discard your 3 (6) left-most Tavern spells; +7/+7
        // for each (36.6.3: the golden one discards more instead of gaining double).
        (
            "BG36_320",
            Script {
                end_of_turn: Some(|c, me| {
                    let spells: Vec<EntityId> = c
                        .player_ref(me.side)
                        .map(|p| {
                            p.hand
                                .iter()
                                .filter(|h| {
                                    crate::recruit::discardable(h)
                                        && !h.held
                                        && matches!(h.item,
                                        Item::Spell { card, data }
                                            if is_tavern_spell_instance(card, data))
                                })
                                .take(3 * me.g() as usize)
                                .map(|h| h.hid)
                                .collect()
                        })
                        .unwrap_or_default();
                    for h in spells {
                        if crate::recruit::discard_card(c, me.side, h).is_some() {
                            c.buff(me.eid, 7, 7);
                        }
                    }
                }),
                ..E
            },
        ),
        // Dark Puppeteer: Deathrattle: Tavern spells give an extra +4 (+8) Health this game.
        (
            "BG36_104",
            Script {
                deathrattle: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.stats.spell_hp = p.stats.spell_hp.saturating_add(4 * me.g());
                    }
                }),
                ..E
            },
        ),
        // The Shadow of Doubt: whenever a card is added to your hand, Deity +4/+5 (+8/+10;
        // hotfix 36.6.2).
        (
            "BG36_109",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::CardAdded { side, .. } if *side == me.side) {
                        buff_deity(c, me.side, 4 * me.g(), 5 * me.g());
                    }
                }),
                listens: ev::CARD_ADDED,
                ..E
            },
        ),
        // Harbinger Aph'lass: whenever you discard a card, give your Deity +2/+1 (+4/+2) and
        // improve this (each improvement adds the base again).
        (
            "BGFYM_005",
            Script {
                on_event: Some(|c, me, e| {
                    if !matches!(e, Event::Discarded { side, .. } if *side == me.side) {
                        return;
                    }
                    let k = me
                        .g()
                        .saturating_add(c.minion(me.eid).map_or(0, |m| m.counter));
                    buff_deity(c, me.side, 2 * k, k);
                    c.counter_perm(me.side, me.eid, me.g());
                }),
                listens: ev::DISCARDED,
                flags: flag::IMPROVES,
                // "Whenever you discard a card, give your Deity +2/+1 and improve this."
                shown: Some(|_, sh| {
                    let k = sh.g().saturating_add(sh.counter());
                    vec![2 * k, k]
                }),
                ..E
            },
        ),
        // Nightmare Par-tea Guest: Battlecry and Deathrattle: get 1 (2) Misplaced Tea Sets.
        (
            "BG32_111",
            Script {
                battlecry: Some(|c, me, _| give_n(c, me.side, TEA_SET, me.g() as usize)),
                deathrattle: Some(|c, me| give_n(c, me.side, TEA_SET, me.g() as usize)),
                ..E
            },
        ),
        // Misplaced Tea Set: give a friendly minion of each type +4/+4 (picked per type).
        (
            TEA_SET,
            Script {
                spell: Some(|c, me, _| {
                    for (t, _) in one_minion_per_type(c, me.side, |_| true) {
                        spell_buff(c, me, t, 4, 4);
                    }
                }),
                // "Give a friendly minion of each type +4/+4."
                shown: Some(|c, sh| shown_spell_stats(c, sh, 4, 4)),
                ..E
            },
        ),
        // Gatekeeper Amalgam: whenever you cast a spell on this, it casts Misplaced Tea Set
        // (twice).
        (
            "BG36_640",
            Script {
                on_event: Some(|c, me, e| {
                    if aimed_at_me(c, me, e)
                        && matches!(e, Event::SpellCast { card, .. } if *card != cid(TEA_SET))
                    {
                        for _ in 0..me.g() {
                            c.cast_spell(me.side, cid(TEA_SET), 0, 0, 0);
                        }
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Faceless Manipulator (only from Manipulator Portrait): Battlecry: choose a minion and
        // become a copy of it -- a full copy of another friendly or Tavern minion; the golden text
        // is the same. Its own pool copy returns; the copy takes one when one is left.
        (
            "BG_EX1_564",
            Script {
                battlecry: Some(|c, me, t| {
                    if c.minion(me.eid).is_none() {
                        return;
                    }
                    let Some(src) = c.find_any(t).cloned() else {
                        return;
                    };
                    let mut m = c.full_copy(&src);
                    m.eid = me.eid;
                    if c.combat {
                        // Triggered in combat: only the combat copy changes, so no pool copy moves.
                        c.g.pool.return_minion(&m);
                        m.pool_copies = 0;
                    }
                    let old = c.minion_mut(me.eid).map(|slot| std::mem::replace(slot, m));
                    if let Some(old) = old
                        && !c.combat
                    {
                        c.g.pool.return_minion(&old);
                    }
                }),
                battlecry_target: TargetSpec::FriendlyOther,
                ..E
            },
        ),
    ]
}
