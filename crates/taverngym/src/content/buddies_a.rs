//! Buddies (part a).

use super::*;

/// Get a plain copy of the left-most card in `side`'s hand (Akali). A Triple Reward's Tier is
/// the card itself, not an enchantment, so its copy Discovers from the same Tier (as for
/// Voone's Hero Power).
fn copy_left_most_hand_card(c: &mut Ctx, side: Side) {
    let first = c
        .player_ref(side)
        .and_then(|p| p.hand.first())
        .map(|h| h.item.clone());
    match first {
        Some(Item::Spell { card, data }) if card == cid(crate::recruit::TRIPLE_REWARD) => {
            c.add_to_hand(side, Item::Spell { card, data });
        }
        Some(item) => {
            c.give_card(side, db().normal_of(item.card()));
        }
        None => {}
    }
}

/// Put fresh (plain, not from the pool) minions of `cards` into a cleared Tavern; counts as a
/// Refresh.
fn refresh_with_cards(c: &mut Ctx, side: Side, cards: Vec<CardId>) {
    let Some(p) = c.owner[side] else { return };
    c.clear_shop(side);
    for card in cards {
        let m = c.new_minion(side, card);
        let sid = m.eid;
        c.g.players[p]
            .shop
            .push(crate::state::ShopSlot::new(sid, Item::Minion(m)));
    }
    c.fill_shop(side);
    c.finish_refresh(side, false);
}

/// Transform a board minion into a random minion `up` Tiers higher (max 7); fresh stats.
fn transform_up(c: &mut Ctx, side: Side, t: EntityId, up: u8) {
    let Some((tier, pool)) = c.minion(t).map(|m| (m.tier, m.pool_entries())) else {
        return;
    };
    // Evolving never reaches Tier 7 -- except an effect that names it: Veranus says "up to Tier
    // 7!".
    let target = (tier + up).min(7);
    let pick = c.random_minion(|d| d.tier == target);
    let Some(new) = pick else { return };
    if !c.combat {
        for (card, copies) in pool {
            c.g.pool.put(card, copies);
        }
    }
    let mut m = c.new_minion(side, new);
    if !c.combat {
        c.take_pool_copy(&mut m);
    }
    if let Some(slot) = c.minion_mut(t) {
        m.eid = slot.eid;
        *slot = m;
    }
}

/// Is `card` one of the Blood Gem spells?
fn gem(card: CardId) -> bool {
    is_blood_gem(card)
}

pub(super) fn copy_leftmost_deathrattles(c: &mut Ctx, me: Me) {
    let whirl = cid("BG21_HERO_030_Buddy");
    let timewarped = cid("BG34_Giant_599");
    // Each minion's own, attached and gained Deathrattles.
    let has_dr = |x: &CardId| script(*x).is_some_and(|s| s.deathrattle.is_some());
    let drs: Vec<CardId> = c.boards[me.side]
        .iter()
        .filter(|m| {
            m.alive()
                && m.eid != me.eid
                && db().normal_of(m.card) != whirl
                && db().normal_of(m.card) != timewarped
        })
        .filter(|m| {
            crate::script::minion_has(m, crate::script::Hook::Deathrattle)
                || !m.gained_dr.is_empty()
        })
        .take(2)
        .flat_map(|m| {
            std::iter::once(m.card)
                .chain(m.attached.iter().copied())
                .filter(has_dr)
                .chain(m.gained_dr.iter().copied())
                .collect::<Vec<_>>()
        })
        .collect();
    if let Some(m) = c.minion_mut(me.eid) {
        for _ in 0..me.g() {
            m.gained_dr.extend(drs.iter().copied());
        }
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Icesnarl the Mighty: after a friendly minion kills an enemy, gain +3 (+6) Health
        // permanently.
        (
            "BG20_HERO_100_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Killed { side, .. } = e
                        && *side == me.side
                    {
                        c.buff_perm(me.side, me.eid, 0, 3 * me.g());
                    }
                }),
                listens: ev::KILLED,
                ..E
            },
        ),
        // Baby Elekk: whenever you play a minion with less Attack than this, give it +1/+1 (+2/+2)
        // and improve this (by the base amount; the counter is in steps of the plain card,
        // `flag::IMPROVES`: a triple keeps the growth).
        (
            "BG20_HERO_101_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Played { side, eid, played } = e else {
                        return;
                    };
                    if *side != me.side || played.source == me.eid {
                        return;
                    }
                    let Some(mine) = c.minion(me.eid).map(|m| (m.atk, m.counter)) else {
                        return;
                    };
                    if played.atk < mine.0 {
                        let k = me.g() + mine.1;
                        c.buff(*eid, k, k);
                        c.counter_perm(me.side, me.eid, me.g());
                    }
                }),
                listens: ev::PLAYED,
                flags: flag::IMPROVES,
                ..E
            },
        ),
        // Dranosh Saurfang: after you buy a minion, gain half (golden: all) its stats.
        (
            "BG20_HERO_102_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Bought {
                        side, eid, card, ..
                    } = e
                    else {
                        return;
                    };
                    if *side != me.side || !db().get(*card).is_minion() {
                        return;
                    }
                    if let Some((a, h)) = c.find_any(*eid).map(|m| (m.atk, m.hp)) {
                        let (a, h) = if me.golden { (a, h) } else { (a / 2, h / 2) };
                        c.buff(me.eid, a, h);
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Death's Head Sage: after you get a Blood Gem, get 1 (2) extra copies of it (the
        // copies don't trigger it again).
        (
            "BG20_HERO_103_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::CardAdded { side, hid } = e else {
                        return;
                    };
                    // One guard for normal and golden Sages: copies never trigger either.
                    let sage = db().normal_of(me.card);
                    if *side != me.side || c.card_data(me.side, sage) != 0 {
                        return;
                    }
                    let item = c
                        .player_ref(me.side)
                        .and_then(|p| p.hand.iter().find(|h| h.hid == *hid))
                        .map(|h| h.item.clone());
                    let Some(item) = item.filter(|x| gem(x.card())) else {
                        return;
                    };
                    c.set_card_data(me.side, sage, 1);
                    for _ in 0..me.g() {
                        // Spell data carries Jewelry Box's Taunt and other Gem variants.
                        c.add_to_hand(me.side, item.clone());
                    }
                    c.set_card_data(me.side, sage, 0);
                }),
                listens: ev::CARD_ADDED,
                ..E
            },
        ),
        // Master Gadrin: Start of Combat: the minion to the left (golden: both neighbours)
        // gains Health equal to its Attack.
        (
            "BG20_HERO_201_Buddy",
            Script {
                start_of_combat: Some(|c, me| {
                    let Some((s, i)) = c.locate(me.eid) else {
                        return;
                    };
                    let mut targets = vec![];
                    if i > 0 {
                        targets.push(c.boards[s][i - 1].eid);
                    }
                    if me.golden && i + 1 < c.boards[s].len() {
                        targets.push(c.boards[s][i + 1].eid);
                    }
                    for t in targets {
                        if let Some(a) = c.minion(t).map(|m| m.atk) {
                            c.buff(t, 0, a);
                        }
                    }
                }),
                ..E
            },
        ),
        // Lei Flamepaw: at the start of your turn, get 1 (2) Buddies of your Hero Power.
        (
            "BG20_HERO_202_Buddy",
            Script {
                start_of_turn: Some(|c, me| {
                    let Some(owner) = c.owner[me.side] else {
                        return;
                    };
                    let Some(b) = super::buddies_c::power_buddy(c, owner) else {
                        return;
                    };
                    for _ in 0..me.g() {
                        c.give_card(me.side, b);
                    }
                }),
                ..E
            },
        ),
        // Baby Kodo: Battlecry: Refresh the Tavern with a minion of each Tier (up to yours).
        (
            "BG20_HERO_242_Buddy",
            Script {
                battlecry: Some(|c, me, _| {
                    let Some(p) = c.owner[me.side] else { return };
                    c.clear_shop(me.side);
                    // One of each Tier 1-6 whatever your own Tier, Tier 7 included once unlocked
                    // (the official Kodo fix).
                    let top = if c.g.players[p].tier7 { 7 } else { 6 };
                    for t in 1..=top {
                        if let Some(m) = c.draw_pool_minion(me.side, t, t, Races::NONE) {
                            let sid = m.eid;
                            c.g.players[p]
                                .shop
                                .push(crate::state::ShopSlot::new(sid, Item::Minion(m)));
                        }
                    }
                    c.fill_shop(me.side);
                    c.finish_refresh(me.side, false);
                }),
                ..E
            },
        ),
        // Living Nightmare: after you buy a card, minions in the Tavern have +2/+2 (+4/+4) this
        // turn. Read as Ashen Corruptor's "this turn": the minions in the Tavern get it now and the
        // ones that enter it later in the turn get the total so far on arrival
        // (`apply_tavern_buffs`).
        (
            "BG20_HERO_280_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Bought { side, .. } if *side == me.side) {
                        let k = 2 * me.g();
                        c.add_turn_data(me.side, crate::cardid!("BG32_873"), k);
                        c.buff_tavern_temp(me.side, k, k);
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Monstrosity: after a friendly minion dies, gain its Attack (twice).
        (
            "BG20_HERO_282_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Died { side, eid, .. } = e else {
                        return;
                    };
                    if *side != me.side || *eid == me.eid {
                        return;
                    }
                    if let Some(a) = c.dead_record(me.side, *eid).map(|d| d.atk) {
                        c.buff(me.eid, a.saturating_mul(me.g()), 0);
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Flight Trainer: Galewing's flightpaths trigger twice (golden: three times) -- read by
        // Galewing's power (`heroes_special.rs`).
        ("BG20_HERO_283_Buddy", E), // handled by its hero's power
        // Nightmare Ectoplasm: Mutanus' 'Devour' spits its stats onto extra minions -- read by
        // the Devour hero power (`heroes_active.rs`).
        ("BG20_HERO_301_Buddy", E), // handled by its hero's power
        // Captain Fairmount: end of turn, randomly improve 'Conviction' (golden: twice).
        (
            "BG21_HERO_000_Buddy",
            Script {
                end_of_turn: Some(|c, me| {
                    const UPS: [&str; 5] = [
                        "BG21_HERO_000p_t1",
                        "BG21_HERO_000_Buddyt",
                        "BG21_HERO_000_Buddyt2",
                        "BG21_HERO_000_Buddyt3",
                        "BG21_HERO_000_Buddyt4",
                    ];
                    for _ in 0..me.g() {
                        let i = c.g.rng.below(UPS.len());
                        super::heroes_special::improve_conviction(c, me.side, cid(UPS[i]));
                    }
                }),
                ..E
            },
        ),
        // Warden Thelwater: at the start of your turn, get 1 (2) of your next opponent's
        // Buddy.
        (
            "BG21_HERO_010_Buddy",
            Script {
                start_of_turn: Some(|c, me| {
                    let Some(p) = c.owner[me.side] else { return };
                    let Some(o) = c.g.players[p].next_opponent else {
                        return;
                    };
                    let Some(b) = super::buddies_c::hero_buddy(c, o) else {
                        return;
                    };
                    for _ in 0..me.g() {
                        c.give_card(me.side, b);
                    }
                }),
                ..E
            },
        ),
        // Sous Chef: extra Hero Power uses each turn (`can_hero_power`).
        ("BG21_HERO_020_Buddy", E),
        // Piloted Whirl-O-Tron: Start of Combat: copy your two left-most Deathrattles (twice),
        // except other Whirl-O-Trons.
        (
            "BG21_HERO_030_Buddy",
            Script {
                start_of_combat: Some(copy_leftmost_deathrattles),
                ..E
            },
        ),
        // Crabby: Tavish's 'Lock and Load' copy count is handled by its hero's power.
        ("BG22_HERO_000_Buddy", E), // handled by its hero's power
        // Spirit Raptor: Deathrattle: call upon every Element it remembers (twice): the Elements
        // Bru'kan called upon while it was in the warband (`counter` = bit set, kept on the warband
        // minion).
        (
            "BG22_HERO_001_Buddy",
            Script {
                deathrattle: Some(|c, me| {
                    let bits = c
                        .dead_record(me.side, me.eid)
                        .map(|d| d.counter)
                        .or_else(|| c.minion(me.eid).map(|m| m.counter))
                        .unwrap_or(0);
                    let elems = [
                        "BG22_HERO_001p_t1_s",
                        "BG22_HERO_001p_t2_s",
                        "BG22_HERO_001p_t3_s",
                        "BG22_HERO_001p_t4_s",
                    ];
                    for (i, e) in elems.iter().enumerate() {
                        if bits & (1 << i) != 0 {
                            for _ in 0..me.g() {
                                c.cast_spell(me.side, cid(e), 0, 0, 0);
                            }
                        }
                    }
                }),
                ..E
            },
        ),
        // Frostwolf Lieutenant: end of turn, left-most minion +10 (+20) Attack.
        (
            "BG22_HERO_002_Buddy",
            Script {
                end_of_turn: Some(|c, me| {
                    if let Some(t) = c.leftmost(me.side) {
                        c.buff(t, 10 * me.g(), 0);
                    }
                }),
                ..E
            },
        ),
        // Stormpike Lieutenant: end of turn, right-most minion +10 (+20) Health.
        (
            "BG22_HERO_003_Buddy",
            Script {
                end_of_turn: Some(|c, me| {
                    if let Some(t) = c.rightmost(me.side) {
                        c.buff(t, 0, 10 * me.g());
                    }
                }),
                ..E
            },
        ),
        // Varden's Aquarrior: 'Twice as Nice' extra stats are handled by its hero's power.
        ("BG22_HERO_004_Buddy", E), // handled by its hero's power
        // Sub Scrubber: after you play a Mech, gain +3/+3 (+6/+6).
        (
            "BG22_HERO_200_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Played { side, played, .. } = e
                        && *side == me.side
                        && played.source != me.eid
                        && played.races.intersects(Races::MECH)
                    {
                        c.buff(me.eid, 3 * me.g(), 3 * me.g());
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Submersible Chef: Battlecry: get 1 (2) random minions each from Tiers 1, 3 and 5.
        (
            "BG22_HERO_201_Buddy",
            Script {
                battlecry: Some(|c, me, _| {
                    for t in [1u8, 3, 5] {
                        for _ in 0..me.g() {
                            if let Some(m) = c.random_minion(|d| d.tier == t) {
                                c.give_card(me.side, m);
                            }
                        }
                    }
                }),
                ..E
            },
        ),
        // Many Whelps: whenever you summon a Whelp, gain +2/+2 (+4/+4) permanently.
        (
            "BG22_HERO_305_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Summoned { side, eid } = e
                        && *side == me.side
                        && *eid != me.eid
                        && c.minion(*eid).is_some_and(|m| m.name().contains("Whelp"))
                    {
                        c.buff_perm(me.side, me.eid, 2 * me.g(), 2 * me.g());
                    }
                }),
                listens: ev::SUMMONED,
                ..E
            },
        ),
        // Tamuzo: after you summon a minion in combat, double (triple) its stats.
        (
            "BG23_HERO_201_Buddy",
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
        // Watfin: Murloc Holmes' correct guesses -- read by Holmes' guess (`after_discover`).
        ("BG23_HERO_303_Buddy", E), // handled by its hero's power
        // Waxadred: Battlecry: Refresh the Tavern with the highest-Tier minion from each
        // opponent's warband (plain copies).
        (
            "BG23_HERO_305_Buddy",
            Script {
                battlecry: Some(|c, me, _| {
                    let Some(p) = c.owner[me.side] else { return };
                    let mut cards = vec![];
                    for q in 0..c.g.players.len() {
                        // Eliminated opponents count too, with their last warband.
                        if q == p {
                            continue;
                        }
                        let board = c.g.fought_board(q);
                        let keyed: Vec<(CardId, i64)> = board
                            .iter()
                            .map(|m| (db().normal_of(m.card), m.tier as i64))
                            .collect();
                        if let Some(x) = c.pick_max(keyed) {
                            cards.push(x);
                        }
                    }
                    cards.truncate(7);
                    refresh_with_cards(c, me.side, cards);
                }),
                ..E
            },
        ),
        // Nathanos Blightcaller: Battlecry: sell a friendly minion and split its stats (golden:
        // double its stats) evenly among the minions that stood next to it. Halves round down.
        (
            "BG23_HERO_306_Buddy",
            Script {
                battlecry: Some(|c, me, t| {
                    // No other friendly minion to choose: nothing is sold.
                    let Some((side, i)) = c.locate(t) else { return };
                    if t == 0 || side != me.side {
                        return;
                    }
                    let near = c.neighbors(t);
                    let (a, h) = (c.boards[side][i].atk, c.boards[side][i].max_hp);
                    c.sell_minion(side, i);
                    let n = near.len().max(1) as i32;
                    for x in near {
                        c.buff(
                            x,
                            ((i64::from(a) * i64::from(me.g())) / i64::from(n))
                                .clamp(i32::MIN as i64, i32::MAX as i64)
                                as i32,
                            ((i64::from(h) * i64::from(me.g())) / i64::from(n))
                                .clamp(i32::MIN as i64, i32::MAX as i64)
                                as i32,
                        );
                    }
                }),
                battlecry_target: TargetSpec::FriendlyOther,
                ..E
            },
        ),
        // Shady Aristocrat: when sold, Discover a Quest; completing it gives an 8 (16) Gold
        // Coin Pouch.
        (
            "BG24_HERO_100_Buddy",
            Script {
                on_sell: Some(|c, me| {
                    let pouch = cid(if me.golden {
                        "BG24_HERO_100_Buddy_Gt"
                    } else {
                        "BG24_HERO_100_Buddyt"
                    });
                    let active: Vec<CardId> = c
                        .player_ref(me.side)
                        .map(|p| p.quests.iter().map(|q| q.card).collect())
                        .unwrap_or_default();
                    let lobby = c.g.lobby_races;
                    let quests: Vec<CardId> = db()
                        .iter()
                        .filter(|d| d.kind == crate::cards::CardKind::Quest && d.normal.is_none())
                        .filter(|d| !active.contains(&d.cid))
                        .filter(|d| quest_offerable(lobby, &d.id) && script(d.cid).is_some())
                        .map(|d| d.cid)
                        .collect();
                    let picks = c.g.rng.sample_indices(quests.len(), 3);
                    let options: Vec<(CardId, CardId)> =
                        picks.into_iter().map(|i| (quests[i], pouch)).collect();
                    if options.is_empty() {
                        return;
                    }
                    let choice = c.quest_choice(options);
                    if let Some(p) = c.player(me.side) {
                        p.pending.push(choice);
                    }
                }),
                ..E
            },
        ),
        // Coin Pouches: as a Quest reward, go to your hand; cast: gain 8 (16) Gold.
        ("BG24_HERO_100_Buddyt", pouch()),
        ("BG24_HERO_100_Buddy_Gt", pouch()),
        // Enhance-o Medico: after you buy a minion, gain +3/+3 (+6/+6) per Bonus Keyword on it.
        (
            "BG24_HERO_204_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Bought {
                        side, eid, card, ..
                    } = e
                    else {
                        return;
                    };
                    if *side != me.side || !db().get(*card).is_minion() {
                        return;
                    }
                    let n = c.find_any(*eid).map_or(0, |m| {
                        BONUS_KWS.iter().filter(|k| m.kw.has(**k)).count() as i32
                    });
                    if n > 0 {
                        c.buff(me.eid, 3 * me.g() * n, 3 * me.g() * n);
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Festergut: Deathrattle: summon and get 1 (2) random Putricide's Creations (a random base
        // + part). The hand copy is the same Creation.
        (
            "BG25_HERO_100_Buddy",
            Script {
                deathrattle: Some(|c, me| {
                    for _ in 0..me.g() {
                        let Some(m) = super::putricide::random_creation(c, me.side) else {
                            return;
                        };
                        let mut copy = m.clone();
                        copy.eid = 0;
                        let pos = if c.on_board(me.eid) {
                            c.pos_right_of(me.eid)
                        } else {
                            me.pos
                        };
                        c.summon(me.side, pos, m);
                        c.add_to_hand(me.side, Item::Minion(copy));
                    }
                }),
                ..E
            },
        ),
        // Shadowy Construct: whenever a friendly minion dies, gain its maximum stats (once;
        // golden twice per combat -- counter on the combat copy).
        (
            "BG25_HERO_103_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Died { side, eid, .. } = e else {
                        return;
                    };
                    if !c.combat || *side != me.side || *eid == me.eid {
                        return;
                    }
                    let used = c.minion(me.eid).map_or(i32::MAX, |m| m.counter);
                    if used >= me.g() {
                        return;
                    }
                    let Some((a, h)) = c
                        .dead_record(me.side, *eid)
                        .map(|d| (d.peak_atk, d.peak_hp))
                    else {
                        return;
                    };
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter += 1;
                    }
                    c.buff(me.eid, a, h);
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Talent Scout: Battlecry: make a Buddy (golden: 2 Buddies) Golden. A targeted Battlecry
        // (client TARGETING_ARROW_TEXT "Make a Buddy Golden." on both forms, as on Elite Navigator
        // and Captain Sanders): the player chooses the Buddy, in the warband or in the Tavern (no
        // "friendly" in the text); the golden's second Buddy is a random friendly one
        // (an assumption).
        (
            "BG25_HERO_105_Buddy",
            Script {
                battlecry: Some(|c, me, t| {
                    let eligible = |m: &Minion| m.eid != me.eid && crate::script::gildable_buddy(m);
                    let mut picks = vec![];
                    if t != 0 && c.find_any(t).is_some_and(eligible) {
                        picks.push(t);
                    }
                    let mut rest = c.friendly_where(me.side, |m| eligible(m) && m.eid != t);
                    c.g.rng.shuffle(&mut rest);
                    let n = me.g() as usize;
                    picks.extend(rest.into_iter().take(n.saturating_sub(picks.len())));
                    for t in picks {
                        if let Some(m) = c.find_any_mut(t) {
                            make_golden(m);
                        }
                    }
                }),
                battlecry_target: TargetSpec::BuddyNonGolden,
                ..E
            },
        ),
        // Shining Sailor: the Tavern offers 1 (2) extra Pirates whenever it is Refreshed.
        (
            "BG26_HERO_101_Buddy",
            Script {
                on_fill_shop: Some(|c, me| {
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        c.add_shop_minion(me.side, 1, tier, Races::PIRATE);
                    }
                }),
                ..E
            },
        ),
        // Solemn Serenader: target reaction is handled directly by its hero's power.
        ("BG26_HERO_102_Buddy", E), // handled by its hero's power
        // Akali, Rock Rhino: end of every 2 turns (golden: every turn), get a plain copy of the
        // left-most card in your hand.
        (
            "BG26_HERO_104_Buddy",
            Script {
                end_of_turn: Some(|c, me| {
                    if !c.minion_turn_cycle(me.eid, if me.golden { 1 } else { 2 }) {
                        return;
                    }
                    copy_left_most_hand_card(c, me.side);
                }),
                ..E
            },
        ),
        // Veranus: end of turn, transform the minion to the left into one from 1 (2) Tiers
        // higher (up to Tier 7).
        (
            "BG27_HERO_801_Buddy",
            Script {
                end_of_turn: Some(|c, me| {
                    let Some((s, i)) = c.locate(me.eid) else {
                        return;
                    };
                    if i == 0 {
                        return;
                    }
                    let t = c.boards[s][i - 1].eid;
                    transform_up(c, me.side, t, me.g() as u8);
                }),
                ..E
            },
        ),
        // Box Cars: at the start of your turn, roll 1 (2) dice; Discover a Tavern spell of each
        // rolled Tier.
        (
            "BG28_HERO_400_Buddy",
            Script {
                start_of_turn: Some(|c, me| {
                    for _ in 0..me.g() {
                        let t = 1 + c.g.rng.below(6) as u8;
                        c.discover_spell(me.side, me.card, 3, |d| d.tier == t);
                    }
                }),
                ..E
            },
        ),
    ]
}

/// Coin Pouch: earned as a Quest reward it goes to your hand (not a passive reward); cast it
/// to gain 8 (Hefty: 16) Gold.
fn pouch() -> Script {
    Script {
        on_attach: Some(|c, me| {
            if let Some(p) = c.player(me.side)
                && let Some(i) = p.rewards.iter().position(|r| *r == me.card)
            {
                p.rewards.remove(i);
            }
            c.give_card(me.side, me.card);
        }),
        spell: Some(|c, me, _| {
            let g = if db().get(me.card).id.ends_with("_Gt") {
                16
            } else {
                8
            };
            c.gain_gold(me.side, g);
        }),
        ..E
    }
}
