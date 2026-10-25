//! Timewarp cards (beasts): the Timewarped Tavern's own pool (docs/TIMEWARP.md).

use super::*;
use crate::cards::CardKind;
use crate::script::{Hook, minion_has};

const TABBYCAT: &str = "BG_CFM_315t";
const BOAR: &str = "BG34_Giant_201";
/// Timewarped Nine Frogs: Tavern spells one copy gives in total ("(9 left!)").
const FROG_SPELLS: i32 = 9;

// Timewarped Nellie's Ship keeps its crew (1 Pirate, golden 2) in the minion counter: two
// 15-bit slots holding card id + 1 (0 = empty).
const CREW_BITS: u32 = 15;
const CREW_MASK: i32 = (1 << CREW_BITS) - 1;

fn crew(counter: i32) -> Vec<CardId> {
    [0, CREW_BITS]
        .into_iter()
        .map(|s| (counter >> s) & CREW_MASK)
        .filter(|v| *v != 0)
        .map(|v| CardId((v - 1) as u16))
        .collect()
}

/// Put `card` in the first free crew slot (a full crew is left as it is).
fn crew_add(counter: i32, card: CardId) -> i32 {
    let v = card.0 as i32 + 1;
    if counter & CREW_MASK == 0 {
        counter | v
    } else if (counter >> CREW_BITS) & CREW_MASK == 0 {
        counter | (v << CREW_BITS)
    } else {
        counter
    }
}

/// The mark a Ship puts on its Discover options (`Minion::gift_data`; they carry no Dark
/// Gift) to recognise its pick when it reaches the hand.
fn crew_mark(ship: EntityId) -> i32 {
    -(ship as i32)
}

/// Timewarped Boar: the death count is a player effect (a dying Boar is no longer a
/// listener); every Boar entering the warband makes sure it exists (one stack).
fn arm_boar(c: &mut Ctx, side: Side) {
    let boar = crate::cardid!("BG34_Giant_201");
    if c.player_ref(side)
        .is_some_and(|p| !p.effects.iter().any(|x| x.card == boar))
    {
        c.add_player_effect(side, boar, 1);
    }
}

/// Timewarped Nest Swarmer: your Beetles have +2/+2 (+4/+4) this game; summon 1 (2) Beetles.
fn nest_swarmer(c: &mut Ctx, me: Me) {
    let v = 2 * me.g();
    c.bump_player_stat(me.side, |st| {
        st.beetle_atk = st.beetle_atk.saturating_add(v);
        st.beetle_hp = st.beetle_hp.saturating_add(v);
    });
    summon_n(c, me, BEETLE, me.g() as usize, false);
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        (
            "BG34_Giant_202",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::DeathrattleTriggered { side, .. } if *side == me.side) {
                        c.buff_perm(me.side, me.eid, 3 * me.g(), 2 * me.g());
                    }
                }),
                listens: ev::DEATHRATTLE,
                ..E
            },
        ),
        // Timewarped Alleycat: at the end of your turn, summon 1 (2) Tabbycats with this
        // minion's stats (its current Attack and Health; the Tabbycats are not golden).
        (
            "BG34_Giant_009",
            Script {
                end_of_turn: Some(|c, me| {
                    let Some((a, h)) = c.minion(me.eid).map(|m| (m.atk, m.hp)) else {
                        return;
                    };
                    for i in 0..me.g() as usize {
                        let mut t = c.new_minion(me.side, cid(TABBYCAT));
                        t.set_stats(a, h);
                        let pos = c.pos_right_of(me.eid) + i;
                        c.summon(me.side, pos, t);
                    }
                }),
                ..E
            },
        ),
        // Timewarped Chameleon: Start of Combat: transform into a (Golden) copy of the minion to
        // the left of this. A full copy (stats, enchantments), for this combat only; the copy's own
        // Start of Combat does not run this combat.
        (
            "BG34_Giant_042",
            Script {
                start_of_combat: Some(|c, me| {
                    let Some((s, i)) = c.locate(me.eid) else {
                        return;
                    };
                    if i == 0 {
                        return;
                    }
                    let mut m = c.boards[s][i - 1].clone();
                    if me.golden && !m.golden {
                        make_golden(&mut m);
                    }
                    m.eid = me.eid;
                    m.pool_copies = 0;
                    m.triple_reward = false;
                    m.attacked = false;
                    c.boards[s][i] = m;
                }),
                ..E
            },
        ),
        // Timewarped Nellie's Ship: at the start of each turn, Discover 1 (2) Pirates to crew
        // the ship. Deathrattle: summon and get those Pirates.
        // Assumptions: each turn's Discover replaces the crew ("that Pirate"); 3 options of
        // tier <= yours; the crew are normal copies; the Ship keeps its crew after the
        // Deathrattle. The options are tagged with their ship (`crew_mark`).
        (
            "BG34_Giant_074t",
            Script {
                start_of_turn: Some(|c, me| {
                    let tier = c.tier(me.side);
                    let d = db();
                    let mut offered = false;
                    for _ in 0..me.g() {
                        let cands: Vec<(CardId, u64)> =
                            c.g.universe
                                .iter()
                                .copied()
                                .filter(|x| {
                                    let x = d.get(*x);
                                    x.tier <= tier && x.races.intersects(Races::PIRATE)
                                })
                                .map(|x| (x, c.g.pool.get(x) as u64))
                                .collect();
                        let options: Vec<Item> = c
                            .weighted_distinct(cands, 3)
                            .into_iter()
                            .map(|card| {
                                let mut m = c.new_minion(me.side, card);
                                m.gift_data = crew_mark(me.eid);
                                Item::Minion(m)
                            })
                            .collect();
                        offered |= !options.is_empty();
                        c.offer_discover(me.side, me.card, options);
                    }
                    if offered && let Some(m) = c.minion_mut(me.eid) {
                        m.counter = 0;
                    }
                }),
                // The picked Pirate boards the ship (it never enters the hand).
                discover_pick: Some(|c, _side, _source, item| {
                    if let Item::Minion(pirate) = &item
                        && pirate.gift_data < 0
                        && let Some(ship) = c.minion_mut((-pirate.gift_data) as EntityId)
                    {
                        ship.counter = crew_add(ship.counter, pirate.card);
                    }
                    None
                }),
                deathrattle: Some(|c, me| {
                    let counter = c
                        .minion(me.eid)
                        .map(|m| m.counter)
                        .or_else(|| c.dead_record(me.side, me.eid).map(|d| d.counter))
                        .unwrap_or(0);
                    for (i, card) in crew(counter).into_iter().enumerate() {
                        c.summon_card(me.side, me.pos + i, card, 1);
                        c.give_card(me.side, card);
                    }
                }),
                on_merge: Some(|g, parts| {
                    for card in parts.iter().flat_map(|m| crew(m.counter)) {
                        g.counter = crew_add(g.counter, card);
                    }
                }),
                ..E
            },
        ),
        // Timewarped Boar: whenever every third friendly Timewarped Boar dies, get 1 (2)
        // random Golden Beasts. One count per player, kept across turns and combats; the
        // third Boar to die decides the amount (golden: 2); Beasts of tier <= yours.
        (
            BOAR,
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Summoned { side, eid } = e
                        && *side == me.side
                        && *eid == me.eid
                    {
                        arm_boar(c, me.side);
                    }
                }),
                listens: ev::SUMMONED,
                start_of_turn: Some(|c, me| arm_boar(c, me.side)),
                persistent_event: Some(|c, me, e| {
                    let Event::Died { side, card, .. } = e else {
                        return;
                    };
                    if *side != me.side
                        || db().normal_of(*card) != me.card
                        || !c.tick_counter(me.side, me.card, 3)
                    {
                        return;
                    }
                    let n = if db().get(*card).is_golden() { 2 } else { 1 };
                    let tier = c.tier(me.side);
                    for _ in 0..n {
                        if let Some(x) =
                            c.random_minion(|d| d.tier <= tier && d.races.intersects(Races::BEAST))
                        {
                            c.give_card(me.side, db().golden_of(x));
                        }
                    }
                }),
                persistent_listens: ev::DIED,
                // "Whenever every third friendly Timewarped Boar dies, get a random Golden
                // Beast. (0 left!)" (golden: "get 2 random Golden Beasts"). The printed 0
                // stands until the player's death count exists (a Boar entered the warband).
                shown: Some(|c, sh| {
                    let boar = crate::cardid!("BG34_Giant_201");
                    let armed = c
                        .player_ref(sh.side)
                        .is_some_and(|p| p.effects.iter().any(|x| x.card == boar));
                    let left = if armed {
                        (3 - c.card_data(sh.side, boar)).max(0)
                    } else {
                        0
                    };
                    if sh.golden { vec![2, left] } else { vec![left] }
                }),
                ..E
            },
        ),
        // Timewarped Nine Frogs: after you buy a minion, get 1 (2) random Tavern spells from
        // the same Tier. (9 left!) = 9 spells per copy, golden too; then it does nothing.
        (
            "BG34_Giant_309",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Bought { side, card, .. } = e else {
                        return;
                    };
                    let d = db().get(*card);
                    if *side != me.side || d.kind != CardKind::Minion {
                        return;
                    }
                    let tier = d.tier;
                    for _ in 0..me.g() {
                        if c.minion(me.eid).is_none_or(|m| m.counter >= FROG_SPELLS) {
                            return;
                        }
                        let Some(s) = c.random_spell(|d| d.tier == tier) else {
                            return;
                        };
                        c.give_card(me.side, s);
                        c.counter_perm(me.side, me.eid, 1);
                    }
                }),
                listens: ev::BOUGHT,
                // "After you buy a minion, get a random Tavern spell from the same Tier.
                // (9 left!)" (golden: "get 2 random Tavern spells").
                shown: Some(|_, sh| {
                    let left = (FROG_SPELLS - sh.counter()).max(0);
                    if sh.golden { vec![2, left] } else { vec![left] }
                }),
                ..E
            },
        ),
        // Timewarped Hawkstrider: Start of Combat: trigger all friendly Deathrattles (golden:
        // twice = two passes over the minions that had one at Start of Combat).
        (
            "BG34_Giant_370",
            Script {
                start_of_combat: Some(|c, me| {
                    let ids = c.friendly_where(me.side, |m| {
                        minion_has(m, Hook::Deathrattle) || !m.gained_dr.is_empty()
                    });
                    for _ in 0..me.g() {
                        for &e in &ids {
                            c.trigger_deathrattles(me.side, e);
                        }
                    }
                }),
                ..E
            },
        ),
        // Timewarped Hyena: whenever a friendly Beast dies, gain +2/+2 (+4/+4) permanently.
        (
            "BG34_Giant_581",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Died { side, races, .. } = e
                        && *side == me.side
                        && races.intersects(Races::BEAST)
                    {
                        c.buff_perm(me.side, me.eid, 2 * me.g(), 2 * me.g());
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Timewarped Sporebat: Taunt. Deathrattle: get 1 (2) random Tavern spells that cost
        // (2) or more (tier <= yours).
        (
            "BG34_Giant_582",
            Script {
                deathrattle: Some(|c, me| {
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        if let Some(s) = c.random_spell(|d| d.cost >= 2 && d.tier <= tier) {
                            c.give_card(me.side, s);
                        }
                    }
                }),
                ..E
            },
        ),
        // Timewarped Tamuzo: after you summon a minion in combat, double (triple) its stats.
        (
            "BG34_Giant_595",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Summoned { side, eid } = e else {
                        return;
                    };
                    if !c.combat || *side != me.side || *eid == me.eid {
                        return;
                    }
                    if let Some((a, h)) = c.minion(*eid).map(|m| (m.atk, m.hp)) {
                        c.buff(*eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                    }
                }),
                listens: ev::SUMMONED,
                ..E
            },
        ),
        // Timewarped Nest Swarmer: Battlecry, Deathrattle and Rally: your Beetles have +2/+2
        // (+4/+4) this game; summon 1 (2) 2/2 Beetles.
        (
            "BG34_Giant_687",
            Script {
                battlecry: Some(|c, me, _| nest_swarmer(c, me)),
                deathrattle: Some(nest_swarmer),
                rally: Some(|c, me, _| nest_swarmer(c, me)),
                // "Your Beetles have +2/+2 this game. Summon a 2/2 Beetle." (golden: "+4/+4",
                // "two 2/2 Beetles"): the Beetle as it is summoned now, with this game's bonus.
                shown: Some(|c, sh| {
                    let (a, h) = c
                        .player_ref(sh.side)
                        .map_or((0, 0), |p| (p.stats.beetle_atk, p.stats.beetle_hp));
                    let beetle = db().get(cid(BEETLE));
                    vec![2 * sh.g(), 2 * sh.g(), beetle.atk + a, beetle.hp + h]
                }),
                ..E
            },
        ),
        // Timewarped Jungle King: Stealth. After you summon a Beast, give it +4/+3 (+8/+6);
        // improves (by the base amount) after you cast a spell. Playing a Beast counts as
        // summoning it; the King doesn't buff itself; any spell cast from hand improves it
        // while it is on the board.
        (
            "BG34_PreMadeChamp_004",
            Script {
                on_event: Some(|c, me, e| match e {
                    Event::Summoned { side, eid } if *side == me.side && *eid != me.eid => {
                        if !c.minion(*eid).is_some_and(|m| m.is(Races::BEAST)) {
                            return;
                        }
                        let k = me
                            .g()
                            .saturating_add(c.minion(me.eid).map_or(0, |m| m.counter));
                        c.buff(*eid, k.saturating_mul(4), k.saturating_mul(3));
                    }
                    Event::SpellCast { side, .. } if *side == me.side => {
                        c.counter_perm(me.side, me.eid, me.g());
                    }
                    _ => {}
                }),
                listens: ev::SUMMONED | ev::SPELL_CAST,
                // "After you summon a Beast, give it +4/+3. Improves after you cast a spell."
                shown: Some(|_, sh| {
                    let k = sh.g().saturating_add(sh.counter());
                    vec![k.saturating_mul(4), k.saturating_mul(3)]
                }),
                flags: flag::IMPROVES,
                ..E
            },
        ),
    ]
}
