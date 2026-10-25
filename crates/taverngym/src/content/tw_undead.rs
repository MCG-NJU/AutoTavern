//! Timewarp cards (undead): the Timewarped Tavern's own pool (docs/TIMEWARP.md).

use super::*;
use crate::script::{Hook, minion_has};

const WARGHOUL: &str = "BG34_Giant_331";
/// Marker the engine's Reborn reads for "This is Reborn with full Health and enchantments":
/// the Ultra-Capacitor enchantment has that exact text (and no hook of its own on a minion).
const FULL_REBORN: &str = "BG31_HERO_801ptj";

/// "Your Undead have +a Attack this game (wherever they are)".
fn undead_atk(c: &mut Ctx, side: Side, a: i32) {
    c.bump_player_stat(side, |s| s.undead_atk = s.undead_atk.saturating_add(a));
}

/// Where a Deathrattle summons: right of the minion if it is alive (triggered), else where
/// it died.
fn dr_pos(c: &Ctx, me: Me) -> usize {
    if c.on_board(me.eid) {
        c.pos_right_of(me.eid)
    } else {
        me.pos
    }
}

/// Give every friendly minion +atk/+hp permanently.
fn buff_all_perm(c: &mut Ctx, side: Side, atk: i32, hp: i32) {
    for e in c.friendly(side) {
        c.buff_perm(side, e, atk, hp);
    }
}

/// The minions adjacent to a Deathrattle's owner: its neighbours if it is alive, else the
/// minions around the place where it died.
fn dr_neighbors(c: &Ctx, me: Me) -> Vec<EntityId> {
    if c.on_board(me.eid) {
        return c.neighbors(me.eid);
    }
    let b = &c.boards[me.side];
    let mut v = vec![];
    if me.pos > 0
        && let Some(m) = b.get(me.pos - 1)
    {
        v.push(m.eid);
    }
    if let Some(m) = b.get(me.pos) {
        v.push(m.eid);
    }
    v
}

fn warghoul(c: &mut Ctx, me: Me) {
    let own = cid(WARGHOUL);
    let mut cands: Vec<EntityId> = dr_neighbors(c, me)
        .into_iter()
        .filter(|e| {
            c.minion(*e).is_some_and(|m| {
                m.alive()
                    && !m.is_card_id(own)
                    && (minion_has(m, Hook::Deathrattle) || !m.gained_dr.is_empty())
            })
        })
        .collect();
    if !me.golden {
        // "An adjacent minion's Deathrattle": a random one of those that have a Deathrattle.
        cands = c.g.rng.pick(&cands).copied().into_iter().collect();
    }
    for e in cands {
        // Not `trigger_deathrattles`: deaths are resolved by the caller, not in the middle
        // of this Deathrattle.
        if let Some((s, i)) = c.locate(e) {
            let m = c.boards[s][i].clone();
            c.run_deathrattles(s, &m, i + 1);
        }
    }
}

fn radio_star(c: &mut Ctx, me: Me) {
    let killer = me.data as EntityId;
    if killer == 0 {
        return;
    }
    let enemy = Ctx::other(me.side);
    let recorded_dead = !c.boards[enemy].iter().any(|m| m.eid == killer);
    let src = c.boards[enemy]
        .iter()
        .find(|m| m.eid == killer)
        .or_else(|| c.cs[enemy].dead_copies.get(&killer))
        .cloned();
    if let Some(src) = src {
        for _ in 0..me.g() {
            let mut x = c.full_copy(&src);
            x.destroyed = false;
            x.killed_by = 0;
            x.avenge = 0;
            x.attacks = 0;
            // The owner's "this game" bonuses replace the enemy's.
            x.atk = (x.atk - x.pbonus_atk).max(0);
            x.max_hp = (x.max_hp - x.pbonus_hp).max(1);
            x.hp = x.max_hp;
            x.pbonus_atk = 0;
            x.pbonus_hp = 0;
            if let Some(p) = c.owner[me.side] {
                crate::engine::apply_bonuses(&c.g.cfg, &c.g.players[p], &mut x);
            }
            // The copy has at least the dead minion's recorded stats (how the enemy's "this game"
            // bonuses carry over is not known exactly); the death snapshot keeps its enchantments
            // too.
            if recorded_dead {
                x.set_stats(src.atk.max(x.atk), src.max_hp.max(x.max_hp));
            }
            c.add_to_hand(me.side, Item::Minion(x));
        }
    }
    // Friendly killers and missing entities are not enemy minions to copy.
}

fn embalmer(c: &mut Ctx, me: Me, e: &Event) {
    let (side, eid) = match e {
        Event::Summoned { side, eid } => (*side, *eid),
        Event::Played { side, eid, .. } if c.magnet_play => (*side, *eid),
        _ => return,
    };
    if side != me.side || eid == me.eid {
        return;
    }
    let used = c.minion(me.eid).map_or(0, |m| m.counter);
    // A minion that already has Reborn doesn't use a charge (as with Divine Shield).
    if used >= me.g() || !c.minion(eid).is_some_and(|m| !m.has(Kw::REBORN)) {
        return;
    }
    // A turn is the recruit phase and its combat: charges used in the Tavern are not available in
    // combat, and all come back at the start of the next turn.
    if let Some(m) = c.minion_mut(me.eid) {
        m.counter += 1;
    }
    c.give_kw(eid, Kw::REBORN);
}

fn hag(c: &mut Ctx, me: Me) {
    let Some((s, i)) = c.locate(me.eid) else {
        return;
    };
    let b = &c.boards[s];
    let mut idx = vec![i + 1];
    if me.golden && i > 0 {
        idx.insert(0, i - 1);
    }
    let targets: Vec<EntityId> = idx
        .into_iter()
        .filter_map(|j| b.get(j))
        .filter(|m| m.alive() && m.is(Races::UNDEAD))
        .map(|m| m.eid)
        .collect();
    let marker = cid(FULL_REBORN);
    for t in targets {
        c.give_kw(t, Kw::REBORN);
        if let Some(m) = c.minion_mut(t)
            && !m.attached.contains(&marker)
        {
            m.attached.push(marker);
        }
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Timewarped Geist: Deathrattle: Tavern spells give an extra +2/+2 (+4/+4) this game.
        (
            "BG34_Giant_034",
            Script {
                deathrattle: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.stats.spell_atk = p.stats.spell_atk.saturating_add(2 * me.g());
                        p.stats.spell_hp = p.stats.spell_hp.saturating_add(2 * me.g());
                    }
                }),
                ..E
            },
        ),
        // Timewarped Deathswarmer: whenever this takes damage, your Undead have +1 (+2) Attack
        // this game (wherever they are).
        (
            "BG34_Giant_081",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Damaged { eid, .. } if *eid == me.eid) {
                        undead_atk(c, me.side, me.g());
                    }
                }),
                listens: ev::DAMAGED,
                ..E
            },
        ),
        // Timewarped Recycler: Avenge (4): increase your maximum Gold by 1 (2).
        (
            "BG34_Giant_082",
            Script {
                avenge: Some((4, 4, |c, me| c.add_max_gold(me.side, me.g()))),
                ..E
            },
        ),
        // Timewarped Pillager: Taunt, Reborn. Deathrattle: get 1 (2) Tavern Coins.
        (
            "BG34_Giant_204",
            Script {
                deathrattle: Some(|c, me| give_n(c, me.side, TAVERN_COIN, me.g() as usize)),
                ..E
            },
        ),
        // Timewarped Radio Star: Deathrattle: get 1 (2) copies of the enemy minion that killed
        // this, with full Health and enchantments.
        (
            "BG34_Giant_330",
            Script {
                deathrattle: Some(radio_star),
                ..E
            },
        ),
        // Timewarped Warghoul: Taunt. Deathrattle: trigger an adjacent minion's Deathrattle
        // (golden: both adjacent minions'), except Timewarped Warghoul.
        (
            WARGHOUL,
            Script {
                deathrattle: Some(warghoul),
                ..E
            },
        ),
        // Timewarped Embalmer: one minion (two) you summon each turn gains Reborn. The uses
        // are counted on the minion and reset at the start of the turn. A played minion is
        // summoned too (engine convention for `Summoned`).
        (
            "BG34_Giant_332",
            Script {
                flags: flag::TURN_USES,
                on_event: Some(embalmer),
                listens: ev::SUMMONED | ev::PLAYED,
                start_of_turn: Some(|c, me| {
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = 0;
                    }
                }),
                // "One minion you summon each turn gains Reborn. (1 left!)" (golden: "Two
                // minions... (2 left!)")
                shown: Some(|_, sh| vec![(sh.g() - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Timewarped Hag: Start of Combat: give the Undead to the right (golden: adjacent
        // Undead) Reborn and "This is Reborn with full Health and enchantments".
        (
            "BG34_Giant_342",
            Script {
                start_of_combat: Some(hag),
                ..E
            },
        ),
        // Timewarped Festergut: Deathrattle: summon and get 1 (2) random Undead Creations
        // (Putricide's Creations, as Festergut). The hand copy is the same Creation.
        (
            "BG34_Giant_590",
            Script {
                deathrattle: Some(|c, me| {
                    for i in 0..me.g() as usize {
                        let Some(m) = super::putricide::random_creation(c, me.side) else {
                            return;
                        };
                        let mut copy = m.clone();
                        copy.eid = 0;
                        let pos = dr_pos(c, me).saturating_add(i);
                        c.summon(me.side, pos, m);
                        c.add_to_hand(me.side, Item::Minion(copy));
                    }
                }),
                ..E
            },
        ),
        // Timewarped Ghoul-acabra: after you trigger a Deathrattle, give your minions +3/+2
        // (+6/+4) permanently. Each trigger counts (extra triggers too).
        (
            "BG34_Giant_609",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::DeathrattleTriggered { side, .. } if *side == me.side) {
                        buff_all_perm(c, me.side, 3 * me.g(), 2 * me.g());
                    }
                }),
                listens: ev::DEATHRATTLE,
                ..E
            },
        ),
        // Timewarped Caretaker: Deathrattle: summon five (ten) 1/1 Skeletons. Any that don't
        // fit give your Undead +1 Attack this game (wherever they are).
        (
            "BG34_Giant_618",
            Script {
                deathrattle: Some(|c, me| {
                    let n = if me.golden { 10 } else { 5 };
                    let pos = dr_pos(c, me);
                    let fit = c.summon_card(me.side, pos, cid(SKELETON), n).len();
                    if fit < n {
                        undead_atk(c, me.side, (n - fit) as i32);
                    }
                }),
                ..E
            },
        ),
        // Timewarped Deadstomper: after you summon a minion, give your minions +4 (+8) Attack
        // permanently. A played minion is summoned too (engine convention for `Summoned`).
        (
            "BG34_Giant_654",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Summoned { side, eid }
                        if *side == me.side && *eid != me.eid)
                    {
                        buff_all_perm(c, me.side, 4 * me.g(), 0);
                    }
                }),
                listens: ev::SUMMONED,
                ..E
            },
        ),
        // Timewarped Anub'arak: after you play an Undead, your Undead have an extra +3 (+6)
        // Attack this game. Playing this one doesn't count.
        (
            "BG34_PreMadeChamp_083",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Played { side, played, .. } = e
                        && *side == me.side
                        && played.source != me.eid
                        && played.races.intersects(Races::UNDEAD)
                    {
                        undead_atk(c, me.side, 3 * me.g());
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
    ]
}
