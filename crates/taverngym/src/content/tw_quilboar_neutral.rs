//! Timewarp cards (quilboar / neutral): the Timewarped Tavern's own pool (docs/TIMEWARP.md).
//!
//! Not scripted here: Timewarped Whelp Smuggler (BG34_Giant_064) -- "whenever a friendly
//! minion gains Attack" needs a stat-gain hook for other minions, which only the engine has
//! (`Ctx::stat_watchers` / `content::on_stat_gain`).

use super::*;
use crate::cards::CardDef;
use crate::state::PlayerEffect;

/// Minion counter value while a "choose a minion to transform into" choice is open.
const WAIT: i32 = 100;
/// Data of a Blood Gem that "also counts as a Tavern spell" (Timewarped Bloodbinder). Values
/// 1..3 are the Jewelry Box keyword gems.
pub const TAVERN_GEM: i32 = 4;

/// "Give minions in the Tavern +a/+h this game."
fn tavern_this_game(c: &mut Ctx, side: Side, a: i32, h: i32) {
    if let Some(p) = c.player(side) {
        p.stats.tavern_atk = p.stats.tavern_atk.saturating_add(a);
        p.stats.tavern_hp = p.stats.tavern_hp.saturating_add(h);
    }
    c.buff_tavern(side, a, h, Races::NONE);
}

/// Cards of the Minor Timewarp (Tier 3 of the Timewarp pool) that can exist in this lobby:
/// scripted, their bound type in the lobby, no second Hero Powers. Player-specific offer rules
/// (being "in" a type, warband size) are not applied to random gets.
fn minor_timewarp(c: &Ctx, f: impl Fn(&CardDef) -> bool) -> Vec<CardId> {
    let all = c.g.cfg.content == crate::config::ContentMode::AllVanilla;
    let lobby = c.g.lobby_races;
    db().iter()
        .filter(|d| d.pools.timewarp && d.normal.is_none() && d.tier == 3)
        .filter(|d| !d.id.starts_with("BG34_HeroPowerSpell_"))
        .filter(|d| d.tw_types.is_none() || d.tw_types.intersects(lobby))
        .filter(|d| all || script(d.cid).is_some())
        .filter(|d| f(d))
        .map(|d| d.cid)
        .collect()
}

/// Replace board minion `eid` with `m` in place (same entity id; the old minion's pool
/// copies return). `keep`: the new minion has the old one's stats.
fn transform_into(c: &mut Ctx, side: Side, eid: EntityId, mut m: Minion, keep: bool) {
    let Some((s, i)) = c.locate(eid) else {
        c.g.pool.return_minion(&m);
        return;
    };
    let old = &c.boards[s][i];
    m.eid = eid;
    if keep {
        m.atk = old.atk;
        m.hp = old.hp;
        m.max_hp = old.max_hp;
        m.peak_atk = old.atk;
        m.peak_hp = old.hp;
        m.temp_atk = old.temp_atk;
        m.temp_hp = old.temp_hp;
    }
    let old = std::mem::replace(&mut c.boards[s][i], m);
    if !c.combat {
        c.g.pool.return_minion(&old);
        c.check_triples(side);
    }
}

/// "Choose from N minions to transform into": a Discover whose pick replaces this minion
/// (`form_pick`, through `Script::discover_pick`). Returns false when there is no option.
fn offer_forms(c: &mut Ctx, me: Me, cards: Vec<CardId>) -> bool {
    if cards.is_empty() {
        return false;
    }
    if let Some(m) = c.minion_mut(me.eid) {
        m.counter = WAIT;
    }
    let options = cards
        .into_iter()
        .map(|x| Item::Minion(c.new_minion(me.side, x)))
        .collect();
    c.offer_discover(me.side, me.card, options);
    true
}

/// The pick of `offer_forms`: the left-most waiting minion of the source card becomes it
/// (the pick takes a pool copy when one is left).
fn form_pick(c: &mut Ctx, side: Side, source: CardId, item: Item, keep: bool) -> Option<Item> {
    let mine = db().normal_of(source);
    let waiting = c.friendly_where(side, |m| {
        m.counter == WAIT && db().normal_of(m.card) == mine
    });
    let (Some(&eid), Item::Minion(mut m)) = (waiting.first(), item) else {
        return None;
    };
    c.take_pool_copy(&mut m);
    transform_into(c, side, eid, m, keep);
    None
}

/// Timewarped Winner: its "died in combat" watcher is a player effect, because a dead minion
/// gets no event about its own death.
fn winner_watch(c: &mut Ctx, me: Me) {
    let card = db().normal_of(me.card);
    if let Some(p) = c.player(me.side)
        && !p.effects.iter().any(|x| x.card == card)
    {
        p.effects.push(PlayerEffect { card, stacks: 1 });
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        (
            "BG34_Giant_602",
            Script {
                start_of_turn: Some(|c, me| {
                    let Some(owner) = c.owner[me.side] else {
                        return;
                    };
                    if let Some(buddy) = super::buddies_c::power_buddy(c, owner) {
                        for _ in 0..me.g() {
                            c.give_card(me.side, buddy);
                        }
                    }
                }),
                ..E
            },
        ),
        // Timewarped Nomi: after you play an Elemental, minions in the Tavern get +4/+3
        // (+8/+6) this game.
        (
            "BG34_Giant_032",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Played { side, played, .. } = e
                        && *side == me.side
                        && played.source != me.eid
                        && played.races.intersects(Races::ELEMENTAL)
                    {
                        tavern_this_game(c, me.side, 4 * me.g(), 3 * me.g());
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Timewarped Elise: after you Refresh 5 times, make the highest-Tier Tavern minion
        // (golden: the two highest) Golden. Progress is the minion counter; ties are random.
        (
            "BG34_Giant_038",
            Script {
                on_event: Some(|c, me, e| {
                    // The turn-start refill and effect Refreshes don't count.
                    if !matches!(e, Event::Refreshed { side, auto: false } if *side == me.side) {
                        return;
                    }
                    let Some(m) = c.minion_mut(me.eid) else {
                        return;
                    };
                    m.counter += 1;
                    if m.counter < 5 {
                        return;
                    }
                    m.counter = 0;
                    let Some(p) = c.owner[me.side] else { return };
                    for _ in 0..me.g() {
                        let keyed: Vec<(EntityId, i64)> = c.g.players[p]
                            .shop
                            .iter()
                            .filter_map(|s| s.item.as_minion().map(|m| (s.sid, m)))
                            .filter(|(_, m)| !m.golden && db().get(m.card).golden.is_some())
                            .map(|(sid, m)| (sid, m.tier as i64))
                            .collect();
                        if let Some(sid) = c.pick_max(keyed)
                            && let Some(m) = c.find_any_mut(sid)
                        {
                            make_golden(m);
                        }
                    }
                }),
                listens: ev::REFRESHED,
                // "After you Refresh 5 times, make the highest-Tier minion in the Tavern
                // Golden. (5 left!)"
                shown: Some(|_, sh| vec![5, (5 - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Timewarped Winner: Stealth. Start of turn: if this survived last combat, get 1 (2)
        // Triple Rewards. Counter 1 on the warband copy = it died in the last combat. A
        // Winner on the board at turn start is taken to have fought the last combat.
        (
            "BG34_Giant_039",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Played { eid, .. } | Event::Summoned { eid, .. }
                        if *eid == me.eid)
                    {
                        winner_watch(c, me);
                    }
                }),
                listens: ev::PLAYED | ev::SUMMONED,
                persistent_event: Some(|c, me, e| {
                    if let Event::Died {
                        side, eid, card, ..
                    } = e
                        && *side == me.side
                        && c.combat
                        && db().normal_of(*card) == me.card
                        && let Some(p) = c.owner[me.side]
                        && let Some(m) = c.g.players[p].board.iter_mut().find(|m| m.eid == *eid)
                    {
                        m.counter = 1;
                    }
                }),
                persistent_listens: ev::DIED,
                // The death watcher goes when the last Winner is sold.
                on_sell: Some(|c, me| {
                    let card = db().normal_of(me.card);
                    let other = c.boards[me.side]
                        .iter()
                        .any(|m| m.eid != me.eid && db().normal_of(m.card) == card);
                    if !other && let Some(p) = c.player(me.side) {
                        p.effects.retain(|x| x.card != card);
                    }
                }),
                start_of_turn: Some(|c, me| {
                    winner_watch(c, me);
                    let Some(m) = c.minion_mut(me.eid) else {
                        return;
                    };
                    let died = m.counter == 1;
                    m.counter = 0;
                    if !died {
                        for _ in 0..me.g() {
                            give_triple_reward(c, me.side);
                        }
                    }
                }),
                ..E
            },
        ),
        // Timewarped Piper: whenever this takes damage, your Blood Gems give an extra +1 (+2)
        // Attack this game; 3 times per combat (counter on the combat copy). Damage outside combat
        // is not limited.
        (
            "BG34_Giant_069",
            Script {
                on_event: Some(|c, me, e| {
                    if !matches!(e, Event::Damaged { eid, .. } if *eid == me.eid) {
                        return;
                    }
                    if c.combat {
                        let Some(m) = c.minion_mut(me.eid) else {
                            return;
                        };
                        if m.counter >= 3 {
                            return;
                        }
                        m.counter += 1;
                    }
                    gem_bonus(c, me.side, me.g(), 0);
                }),
                listens: ev::DAMAGED,
                ..E
            },
        ),
        // Timewarped Bonker: Windfury. Rally: play 2 (4) permanent Blood Gems on all your
        // other minions.
        (
            "BG34_Giant_102",
            Script {
                rally: Some(|c, me, _| {
                    for t in c.friendly(me.side) {
                        if t != me.eid {
                            for _ in 0..2 * me.g() {
                                c.play_gem_perm(me.side, t);
                            }
                        }
                    }
                }),
                ..E
            },
        ),
        // Timewarped Nalaa: whenever you cast a spell, give a friendly minion of each type
        // +4/+3 (+8/+6). Any spell cast from hand counts (Blood Gems too).
        (
            "BG34_Giant_205",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, .. } if *side == me.side) {
                        for (t, _) in one_minion_per_type(c, me.side, |_| true) {
                            c.buff(t, 4 * me.g(), 3 * me.g());
                        }
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Timewarped Geomancer: Avenge (5): get a Blood Gem; your Blood Gems give an extra
        // +1/+1 (+2/+2) this game.
        (
            "BG34_Giant_305",
            Script {
                avenge: Some((5, 5, |c, me| {
                    give_n(c, me.side, BLOOD_GEM, 1);
                    gem_bonus(c, me.side, me.g(), me.g());
                })),
                ..E
            },
        ),
        // Timewarped Jazzer: Deathrattle: your Blood Gems give an extra +2 (+4) Health this
        // game.
        (
            "BG34_Giant_306",
            Script {
                deathrattle: Some(|c, me| gem_bonus(c, me.side, 0, 2 * me.g())),
                ..E
            },
        ),
        // Timewarped Tide Razor: Deathrattle: summon and get 4 (8) random Pirates. Read as: each
        // random Pirate (tier <= yours) is summoned and a plain copy of it goes to your hand, also
        // when the board had no room for it.
        (
            "BG34_Giant_328",
            Script {
                deathrattle: Some(|c, me| {
                    let tier = c.tier(me.side);
                    let mut pos = me.pos;
                    for _ in 0..4 * me.g() {
                        let Some(card) = c
                            .random_minion(|d| d.races.intersects(Races::PIRATE) && d.tier <= tier)
                        else {
                            break;
                        };
                        let m = c.new_minion(me.side, card);
                        if c.summon(me.side, pos, m).is_some() {
                            pos += 1;
                        }
                        c.give_card(me.side, card);
                    }
                }),
                ..E
            },
        ),
        // Timewarped Henchman: after you kill a second minion each combat, get 1 (2) plain
        // copies of it (counter on the combat copy = kills this combat), like Loyal Henchman.
        (
            "BG34_Giant_593",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Killed { side, victim, .. } = e else {
                        return;
                    };
                    // A friendly minion killed it (it may have died too).
                    if !c.combat || *side != me.side {
                        return;
                    }
                    let Some(m) = c.minion_mut(me.eid) else {
                        return;
                    };
                    m.counter += 1;
                    if m.counter != 2 {
                        return;
                    }
                    let card = c
                        .dead_record(1 - me.side, *victim)
                        .map(|d| d.card)
                        .or_else(|| c.minion(*victim).map(|m| m.card));
                    if let Some(card) = card {
                        for _ in 0..me.g() {
                            c.give_card(me.side, db().normal_of(card));
                        }
                    }
                }),
                listens: ev::KILLED,
                ..E
            },
        ),
        // Timewarped Botani: end of turn, get 1 (2) random minions of your Tier.
        (
            "BG34_Giant_594",
            Script {
                end_of_turn: Some(|c, me| {
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        if let Some(m) = c.random_minion(|d| d.tier == tier) {
                            c.give_card(me.side, m);
                        }
                    }
                }),
                ..E
            },
        ),
        // Timewarped Tipper: end of turn with unspent Gold: +1 (+2) maximum Gold.
        (
            "BG34_Giant_604",
            Script {
                end_of_turn: Some(|c, me| {
                    if c.player_ref(me.side).is_some_and(|p| p.gold > 0) {
                        c.add_max_gold(me.side, me.g());
                    }
                }),
                ..E
            },
        ),
        // Timewarped Traveler: Avenge (4): get 1 (2) random 1-Cost cards (minions or spells)
        // from the Minor Timewarp; uniform, they are not pooled. Never another Traveler (a
        // minion's random minion is never itself).
        (
            "BG34_Giant_605",
            Script {
                avenge: Some((4, 4, |c, me| {
                    let own = db().normal_of(me.card);
                    let cands = minor_timewarp(c, |d| d.cost == 1 && d.cid != own);
                    for _ in 0..me.g() {
                        if let Some(&card) = c.g.rng.pick(&cands) {
                            c.give_card(me.side, card);
                        }
                    }
                })),
                ..E
            },
        ),
        // Timewarped Lil' Quilboar: Reborn. Deathrattle: play 3 (6) Blood Gems on all your
        // Quilboar.
        (
            "BG34_Giant_608",
            Script {
                deathrattle: Some(|c, me| {
                    for t in c.friendly_where(me.side, |m| m.is(Races::QUILBOAR)) {
                        gems_on(c, me.side, t, 3 * me.g());
                    }
                }),
                ..E
            },
        ),
        // Timewarped Gemsplitter: Divine Shield. After a friendly minion (this one too) loses
        // Divine Shield, your Blood Gems give an extra +1 (+2) Attack this game.
        (
            "BG34_Giant_644",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::ShieldLost { side, .. } if *side == me.side) {
                        gem_bonus(c, me.side, me.g(), 0);
                    }
                }),
                listens: ev::SHIELD_LOST,
                ..E
            },
        ),
        // Timewarped Zerus: once per turn, choose from 2 (Golden) Minor Timewarped minions to
        // transform into; keep this minion's stats. Modelled as a free Activate (the client has a
        // hidden "Timewarped Zerus Activation" spell). The options are any two other Minor Timewarp
        // minions, uniform.
        (
            "BG34_Giant_671",
            Script {
                activate: Some((0, TargetSpec::None, |c, me, _| {
                    // A choice already open (Efficiency's second run): nothing more.
                    if c.minion(me.eid).is_none_or(|m| m.counter == WAIT) {
                        return;
                    }
                    let own = db().normal_of(me.card);
                    let cands = minor_timewarp(c, |d| {
                        d.kind == crate::cards::CardKind::Minion && d.cid != own
                    });
                    let cards =
                        c.g.rng
                            .sample_indices(cands.len(), 2)
                            .into_iter()
                            .map(|i| {
                                if me.golden {
                                    db().golden_of(cands[i])
                                } else {
                                    cands[i]
                                }
                            })
                            .collect();
                    offer_forms(c, me, cards);
                })),
                discover_pick: Some(|c, side, source, item| form_pick(c, side, source, item, true)),
                // A choice that never resolved (its pick was merged into a triple).
                start_of_turn: Some(|c, me| {
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = 0;
                    }
                }),
                ..E
            },
        ),
        // Timewarped Lucky Egg: in two turns, choose from three Golden Tier 7 minions to
        // transform into (the golden Egg too). Counter = turn starts seen on the board; the
        // options come from the pool like a Tier 7 Discover and the pick takes a copy.
        (
            "BG34_Giant_683",
            Script {
                start_of_turn: Some(|c, me| {
                    let Some(m) = c.minion_mut(me.eid) else {
                        return;
                    };
                    if m.counter < 2 {
                        m.counter += 1;
                    }
                    if m.counter < 2 {
                        return;
                    }
                    m.counter = 2;
                    let cands: Vec<(CardId, u64)> =
                        c.g.universe
                            .iter()
                            .copied()
                            .filter(|x| db().get(*x).pools.tier7 && db().get(*x).golden.is_some())
                            .map(|x| (x, c.g.pool.get(x) as u64))
                            .collect();
                    let cards = c
                        .weighted_distinct(cands, 3)
                        .into_iter()
                        .map(|x| db().golden_of(x))
                        .collect();
                    // No Tier 7 minion left: it tries again next turn.
                    offer_forms(c, me, cards);
                }),
                discover_pick: Some(|c, side, source, item| {
                    form_pick(c, side, source, item, false)
                }),
                // "In two turns, choose from three Golden Tier 7 minions to transform into.
                // (2 turns left!)" (0 while its choice is open).
                shown: Some(|_, sh| vec![7, (2 - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Timewarped Bloodbinder: start of turn, get 5 (10) Blood Gems that also count as
        // Tavern spells. The gems carry `TAVERN_GEM` as data; `Ctx::cast_spell` treats such a
        // gem as a Tavern spell.
        (
            "BG34_PreMadeChamp_076",
            Script {
                start_of_turn: Some(|c, me| {
                    for _ in 0..5 * me.g() {
                        c.add_to_hand(
                            me.side,
                            Item::Spell {
                                card: cid(BLOOD_GEM),
                                data: TAVERN_GEM,
                            },
                        );
                    }
                }),
                ..E
            },
        ),
        // Timewarped Bandit: start of turn, discard a spell for this to play 4 (8) Blood Gems on
        // all your minions. The spell is a random unlocked one in hand; with none, nothing happens.
        (
            "BG34_PreMadeChamp_078",
            Script {
                start_of_turn: Some(|c, me| {
                    let spells: Vec<EntityId> = c
                        .player_ref(me.side)
                        .map(|p| {
                            p.hand
                                .iter()
                                .filter(|h| {
                                    crate::recruit::discardable(h) && h.item.as_minion().is_none()
                                })
                                .map(|h| h.hid)
                                .collect()
                        })
                        .unwrap_or_default();
                    let Some(&hid) = c.g.rng.pick(&spells) else {
                        return;
                    };
                    if crate::recruit::discard_card(c, me.side, hid).is_none() {
                        return;
                    }
                    for t in c.friendly(me.side) {
                        gems_on(c, me.side, t, 4 * me.g());
                    }
                }),
                ..E
            },
        ),
        // Timewarped Whelp Smuggler: whenever a friendly minion gains Attack, give it +3 (+6)
        // Health: `Ctx::timewarp_stat_watchers`.
        ("BG34_Giant_064", E),
        // Timewarped Scout: when you sell this, get 1 (2) random Tier 7 minions; improves
        // each turn on the board (base x (1 + turns), like every "improve"; not in the hand,
        // like Patient Scout).
        (
            "BG34_Giant_333",
            Script {
                start_of_turn: Some(|c, me| {
                    if let Some(m) = c.find_any_mut(me.eid) {
                        m.counter = m.counter.saturating_add(me.g());
                    }
                }),
                on_sell: Some(|c, me| {
                    for _ in 0..me.g().saturating_add(me.data.max(0)) {
                        if let Some(x) = c.random_minion(|d| d.pools.tier7) {
                            c.give_card(me.side, x);
                        }
                    }
                }),
                // "When you sell this, get 1 random minion from Tier 7. (Improves each turn!)"
                shown: Some(|_, sh| vec![sh.g().saturating_add(sh.counter().max(0)), 7]),
                flags: flag::IMPROVES,
                ..E
            },
        ),
    ]
}
