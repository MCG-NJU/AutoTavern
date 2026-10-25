//! Beast minions (Fishbait: recruit-phase attacks in the Tavern).

use super::*;

pub const FISHBAIT: &str = "BG36_205";

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Sprightly Scarab: Choose One - give a Beast +1/+1 (+2/+2) and Reborn; or +4 (+8)
        // Attack and Windfury.
        (
            "BG27_084",
            Script {
                choose_one: true,
                battlecry: Some(|c, me, t| {
                    if t == 0 {
                        return;
                    }
                    if me.opt_a() {
                        c.buff(t, me.g(), me.g());
                        c.give_kw(t, Kw::REBORN);
                    }
                    if me.opt_b() {
                        c.buff(t, 4 * me.g(), 0);
                        c.give_kw(t, Kw::WINDFURY);
                    }
                }),
                battlecry_target: TargetSpec::FriendlyRaceOther(Races::BEAST),
                ..E
            },
        ),
        // Treasure Parrot: once this deals 35 damage, get 1 (2) Golden Touch. Fires once; a
        // golden made from Parrots can fire again, starting from the unfired copy closest
        // to 35.
        (
            "BG36_763",
            Script {
                on_merge: Some(|g, parts| {
                    g.counter = parts
                        .iter()
                        .map(|m| m.counter)
                        .filter(|c| *c < 35)
                        .max()
                        .unwrap_or(0);
                }),
                on_event: Some(|c, me, e| {
                    let Event::Damaged { amount, source, .. } = e else {
                        return;
                    };
                    if *source != me.eid || *amount <= 0 {
                        return;
                    }
                    let before = c.minion(me.eid).map(|m| m.counter).unwrap_or(0);
                    let after = c.counter_perm(me.side, me.eid, *amount);
                    if before < 35 && after >= 35 {
                        give_n(c, me.side, "BG28_830", me.g() as usize);
                    }
                }),
                listens: ev::DAMAGED,
                // "Once this deals 35 damage, get a Golden Touch. (35 left!)"
                shown: Some(|_, sh| vec![35, (35 - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Banana Slamma: after you summon a Beast in combat, double (triple) its Attack.
        (
            "BG26_802",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Summoned { side, eid } = e else {
                        return;
                    };
                    if !c.combat || *side != me.side || *eid == me.eid {
                        return;
                    }
                    if let Some(a) = c.minion(*eid).filter(|m| m.is(Races::BEAST)).map(|m| m.atk) {
                        c.buff(*eid, a.saturating_mul(me.g()), 0);
                    }
                }),
                listens: ev::SUMMONED,
                ..E
            },
        ),
        // Headhunter Gryphon: Rally: get 1 (2) random Beasts.
        (
            "BG36_204",
            Script {
                rally: Some(|c, me, _| {
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        if let Some(b) =
                            c.random_minion(|d| d.races.intersects(Races::BEAST) && d.tier <= tier)
                        {
                            c.give_card(me.side, b);
                        }
                    }
                }),
                ..E
            },
        ),
        // Hoarding Hyena: Rally: summon a (Golden) Tasty Lobster.
        (
            "BG36_210",
            Script {
                rally: Some(|c, me, _| summon_n(c, me, "BG36_202", 1, me.golden)),
                ..E
            },
        ),
        // Lurking Leviathan: whenever you summon a Beast, give it +3 (+6) Attack and improve
        // this permanently: each improvement adds the base amount again (+3, +6, +9...).
        (
            "BG35_602",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Summoned { side, eid } = e else {
                        return;
                    };
                    if *side != me.side
                        || *eid == me.eid
                        || !c.minion(*eid).is_some_and(|m| m.is(Races::BEAST))
                    {
                        return;
                    }
                    let k = c.minion(me.eid).map(|m| m.counter).unwrap_or(0);
                    c.buff(*eid, 3 * (me.g() + k), 0);
                    c.counter_perm(me.side, me.eid, me.g());
                }),
                listens: ev::SUMMONED,
                flags: flag::IMPROVES,
                // "Whenever you summon a Beast, give it +3 Attack and improve this
                // permanently."
                shown: Some(|_, sh| vec![3 * (sh.g().saturating_add(sh.counter()))]),
                ..E
            },
        ),
        // Sewer Lord: Deathrattle: summon two (Golden) Sewer Rats.
        (
            "BG35_604",
            Script {
                deathrattle: Some(|c, me| summon_n(c, me, "BG19_010", 2, me.golden)),
                ..E
            },
        ),
        // Sewer Rat: Deathrattle: summon a 2/3 (4/6) Half-Shell with Taunt.
        (
            "BG19_010",
            Script {
                deathrattle: Some(|c, me| summon_n(c, me, "BG19_010t", 1, me.golden)),
                ..E
            },
        ),
        // Ravaging Scorpid: after a friendly minion attacks, your Beetles have +4/+4 (+8/+8)
        // this game. Deathrattle: summon 1 (2) 2/2 Beetles.
        (
            "BG36_209",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::AttackDone { side, .. } if *side == me.side) {
                        let k = 4 * me.g();
                        c.bump_player_stat(me.side, |s| {
                            s.beetle_atk = s.beetle_atk.saturating_add(k);
                            s.beetle_hp = s.beetle_hp.saturating_add(k);
                        });
                    }
                }),
                listens: ev::ATTACK_DONE,
                deathrattle: Some(|c, me| summon_n(c, me, BEETLE, me.g() as usize, false)),
                ..E
            },
        ),
        // Fishbait: can't gain stats; Deathrattle: give its killer +5/+5 (golden +10/+10).
        (
            FISHBAIT,
            Script {
                flags: flag::NO_STAT_GAIN,
                deathrattle: Some(|c, me| {
                    if me.data != 0 {
                        c.buff(me.data as EntityId, 5 * me.g(), 5 * me.g());
                    }
                }),
                ..E
            },
        ),
        // Lurking Lionfish: Activate (2): replace a Tavern card with a (golden) Fishbait for your
        // left-most Beast to attack.
        (
            "BG36_201",
            Script {
                activate: Some((2, TargetSpec::TavernAny, |c, me, t| {
                    if let Some(sid) = place_fishbait(c, me.side, me.golden, Some(t)) {
                        leftmost_beast_attacks(c, me.side, sid);
                    }
                })),
                ..E
            },
        ),
        // Snarky Shark: when sold, Refresh the Tavern with a (golden) Fishbait; your left-most
        // Beast attacks it.
        (
            "BG36_206",
            Script {
                on_sell: Some(|c, me| {
                    c.refresh_shop(me.side);
                    let first = c.tavern_minions(me.side).first().copied();
                    if let Some(sid) = place_fishbait(c, me.side, me.golden, first) {
                        leftmost_beast_attacks(c, me.side, sid);
                    }
                }),
                ..E
            },
        ),
        // Patient Scout: when sold, Discover a minion of Tier 1 + turns on the board (golden: two).
        // It doesn't improve in the hand: a card only acts there when its text says "in your hand".
        (
            "BG24_715",
            Script {
                start_of_turn: Some(|c, me| {
                    if let Some(m) = c.find_any_mut(me.eid) {
                        m.counter = m.counter.saturating_add(1);
                    }
                }),
                on_sell: Some(|c, me| {
                    let t = me.data.saturating_add(1).clamp(1, 6) as u8;
                    for _ in 0..me.g() {
                        c.discover_minion(me.side, me.card, 3, |d| d.tier == t);
                    }
                }),
                // A Golden made from Scouts keeps the Tier the furthest of them had reached
                // (a triple keeps a card's growth; the Tiers are not added up).
                on_merge: Some(|g, parts| {
                    g.counter = parts.iter().map(|m| m.counter).max().unwrap_or(0);
                }),
                // "When you sell this, Discover a Tier 1 minion. (Improves each turn!)"
                shown: Some(|_, sh| vec![sh.counter().saturating_add(1).clamp(1, 6)]),
                ..E
            },
        ),
    ]
}

/// Put a Fishbait into the Tavern (replacing `replace` if given, which returns to the pool).
pub fn place_fishbait(
    c: &mut Ctx,
    side: Side,
    golden: bool,
    replace: Option<EntityId>,
) -> Option<EntityId> {
    let p = c.owner[side]?;
    let card = if golden {
        db().golden_of(cid(FISHBAIT))
    } else {
        cid(FISHBAIT)
    };
    let bait = c.new_minion(side, card);
    let sid = bait.eid;
    let slot = crate::state::ShopSlot::new(sid, Item::Minion(bait));
    let shop = &mut c.g.players[p].shop;
    match replace.and_then(|r| shop.iter().position(|s| s.sid == r)) {
        Some(i) => {
            let old = std::mem::replace(&mut shop[i], slot);
            if let Item::Minion(m) = old.item {
                c.g.pool.return_minion(&m);
            }
        }
        None => shop.push(slot),
    }
    Some(sid)
}

pub fn leftmost_beast_attacks(c: &mut Ctx, side: Side, sid: EntityId) {
    if let Some(b) = c.boards[side]
        .iter()
        .find(|m| m.alive() && m.is(Races::BEAST))
        .map(|m| m.eid)
    {
        c.tavern_attack(side, b, sid);
    }
}
