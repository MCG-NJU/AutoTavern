//! Buddies (part b).

use super::*;

/// Each helpful card independently gives one of its three outcomes.
fn helpful(c: &mut Ctx, side: Side, n: i32) {
    let tier = c.tier(side);
    for _ in 0..n {
        let card = match c.g.rng.below(3) {
            0 => Some(cid("BG32_MagicItem_205t")),
            1 => {
                let race = c.most_common_race(side);
                c.random_minion(|d| d.tier == tier && race.is_none_or(|r| d.races.intersects(r)))
            }
            _ => {
                let prizes: Vec<CardId> = db()
                    .iter()
                    .filter(|d| d.prize_tier == 3 && d.normal.is_none())
                    .map(|d| d.cid)
                    .collect();
                c.g.rng.pick(&prizes).copied()
            }
        };
        if let Some(card) = card {
            c.give_card(side, card);
        }
    }
}

/// Per-turn uses kept on the minion counter: 1 (golden: 2).
fn per_turn(s: Script) -> Script {
    Script {
        on_create: Some(|m| m.counter = if m.golden { 2 } else { 1 }),
        start_of_turn: Some(|c, me| {
            if let Some(m) = c.minion_mut(me.eid) {
                m.counter = me.g();
            }
        }),
        ..s
    }
}

/// Take one per-turn use; false if none left.
fn use_one(c: &mut Ctx, eid: EntityId) -> bool {
    match c.minion_mut(eid) {
        Some(m) if m.counter > 0 => {
            m.counter -= 1;
            true
        }
        _ => false,
    }
}

/// Spin the Wheel of Yogg-Saron (`content::yogg`).
pub fn yogg_spin(c: &mut Ctx, side: Side) {
    super::yogg::wheel_of_yogg(c, side, false);
}

/// Battlecruiser Upgrades (Jim Raynor) up to your Tier.
fn battlecruiser_upgrade(c: &mut Ctx, side: Side) {
    let tier = c.tier(side);
    let ups: Vec<CardId> = db()
        .iter()
        .filter(|d| {
            d.id.starts_with("BG31_HERO_801pt") && d.kind == crate::cards::CardKind::TavernSpell
        })
        .filter(|d| d.tier <= tier && super::starcraft::upgrade_available(c, side, d.cid))
        .map(|d| d.cid)
        .collect();
    if let Some(&u) = c.g.rng.pick(&ups) {
        c.give_card(side, u);
    }
}

const ZERG: [&str; 9] = [
    "BG31_HERO_811t2",
    "BG31_HERO_811t3",
    "BG31_HERO_811t4",
    "BG31_HERO_811t5",
    "BG31_HERO_811t6",
    "BG31_HERO_811t7",
    "BG31_HERO_811t8",
    "BG31_HERO_811t9",
    "BG31_HERO_811t10",
];

/// Marker on a Zerg minion that doesn't morph (Broken Horn): `counter` value.
pub const NO_MORPH: i32 = -1;

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        (
            "BG32_MagicItem_205t",
            Script {
                spell: Some(|c, me, _| c.gain_gold(me.side, 3)),
                ..E
            },
        ),
        // Reliquary Attendant: once per turn, after you cast a Tavern spell, get a new copy of
        // it (golden: 2).
        (
            "BG28_HERO_800_Buddy",
            per_turn(Script {
                on_event: Some(|c, me, e| {
                    let Event::SpellCast {
                        side,
                        card,
                        tavern: true,
                        ..
                    } = e
                    else {
                        return;
                    };
                    if *side != me.side || !use_one(c, me.eid) {
                        return;
                    }
                    // Once per turn even when golden (golden gives 2 copies, not 2 uses).
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = 0;
                    }
                    for _ in 0..me.g() {
                        c.give_card(me.side, *card);
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            }),
        ),
        // The Nine Frogs: after you buy a minion, get 1 (2) random Tavern spells from the same
        // Tier (9 uses; counter = uses made).
        (
            "BG28_HERO_801_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Bought { side, card, .. } = e else {
                        return;
                    };
                    if *side != me.side || !db().get(*card).is_minion() {
                        return;
                    }
                    let Some(m) = c.minion_mut(me.eid).filter(|m| m.counter < 9) else {
                        return;
                    };
                    m.counter += 1;
                    let tier = db().get(*card).tier;
                    for _ in 0..me.g() {
                        if let Some(s) = c.random_spell(|d| d.tier == tier) {
                            c.give_card(me.side, s);
                        }
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Fantastic Bellhop: end of turn, get 1 (2) helpful cards.
        (
            "BG30_HERO_304_Buddy",
            Script {
                end_of_turn: Some(|c, me| helpful(c, me.side, me.g())),
                ..E
            },
        ),
        // Outland Sunbeam: Battlecry: Discover 1 (2) Tavern spells costing (3)+ from any Tier.
        (
            "BG31_HERO_003_Buddy",
            Script {
                battlecry: Some(|c, me, _| {
                    // "from any Tier": not limited to your Tavern Tier, but Tier 7 only once the
                    // Tier 7 Tavern is open.
                    let top = if c.player_ref(me.side).is_some_and(|p| p.tier7) {
                        7
                    } else {
                        6
                    };
                    for _ in 0..me.g() {
                        c.discover_spell(me.side, me.card, 3, |d| d.cost >= 3 && d.tier <= top);
                    }
                }),
                ..E
            },
        ),
        // Mini-Zerek: Battlecry: choose a Tavern minion; transform into a (Golden) copy of it.
        (
            "BG31_HERO_005_Buddy",
            Script {
                // Becomes a full copy of the Tavern minion; golden: a Golden copy. Its own copy
                // returns to the pool; the copy takes one when left.
                battlecry: Some(|c, me, t| {
                    let Some(src) = c.find_any(t).cloned() else {
                        return;
                    };
                    let mut m = c.full_copy(&src);
                    if me.golden && !m.golden {
                        super::spells::make_golden(&mut m);
                    }
                    m.eid = me.eid;
                    let old = c.minion_mut(me.eid).map(|slot| std::mem::replace(slot, m));
                    if let Some(old) = old {
                        c.g.pool.return_minion(&old);
                    }
                }),
                battlecry_target: TargetSpec::Tavern,
                ..E
            },
        ),
        // The Celestial Archive: whenever you buy a Tavern spell that costs (0), get 1 (2)
        // copies of it.
        (
            "BG31_HERO_006_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Bought {
                        side, card, price, ..
                    } = e
                    else {
                        return;
                    };
                    // "Costs (0)" = the price paid: no pool Tavern spell prints (0).
                    if *side == me.side && is_tavern_spell(*card) && *price == 0 {
                        for _ in 0..me.g() {
                            c.give_card(me.side, *card);
                        }
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Tychus Findlay: after you cast two Tavern spells, get 1 (2) random Battlecruiser
        // Upgrades.
        (
            "BG31_HERO_801_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    if !matches!(e, Event::SpellCast { side, tavern: true, .. } if *side == me.side)
                    {
                        return;
                    }
                    let Some(m) = c.minion_mut(me.eid) else {
                        return;
                    };
                    m.counter += 1;
                    if m.counter < 2 {
                        return;
                    }
                    m.counter = 0;
                    for _ in 0..me.g() {
                        battlecruiser_upgrade(c, me.side);
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Probius: Magnetic; after you Magnetize this to a Mech, make it Golden.
        (
            "BG31_HERO_802_Buddy",
            Script {
                // `on_attach` runs when Magnetized; `me.eid` is the host.
                on_attach: Some(|c, me| {
                    if let Some(m) = c.minion_mut(me.eid)
                        && m.is(Races::MECH)
                        && !m.golden
                    {
                        make_golden(m);
                    }
                }),
                ..E
            },
        ),
        // Broken Horn: when sold, Discover 1 (2) Zerg minions set to 6/6 that don't morph.
        (
            "BG31_HERO_811_Buddy",
            Script {
                on_sell: Some(|c, me| {
                    for _ in 0..me.g() {
                        let picks = c.g.rng.sample_indices(ZERG.len(), 3);
                        let options = picks
                            .into_iter()
                            .map(|i| {
                                let mut m = c.new_minion(me.side, cid(ZERG[i]));
                                m.set_base_stats(6, 6);
                                m.counter = NO_MORPH;
                                Item::Minion(m)
                            })
                            .collect();
                        c.offer_discover(me.side, me.card, options);
                    }
                }),
                ..E
            },
        ),
        // Malorne: +1/+1 (+2/+2) for every 3 Gold spent this game (`apply_bonuses`).
        ("BG32_HERO_001_Buddy", E),
        // Zippers: Deathrattle: get 1 (2) helpful cards.
        (
            "BG32_HERO_002_Buddy",
            Script {
                deathrattle: Some(|c, me| helpful(c, me.side, me.g())),
                ..E
            },
        ),
        // Stoneshell Guardian: the first (two) Triple Reward(s) you play each turn Discover from
        // Golden minions (the Discover it opens is upgraded to Golden options).
        (
            "BG33_HERO_000_Buddy",
            per_turn(Script {
                on_event: Some(|c, me, e| {
                    let Event::SpellCast { side, card, .. } = e else {
                        return;
                    };
                    if *side != me.side
                        || *card != cid(crate::recruit::TRIPLE_REWARD)
                        || !c.use_physical_spell_allowance(me.eid)
                    {
                        return;
                    }
                    if let Some(p) = c.player(me.side)
                        && let Some(crate::state::Pending::Discover { options, .. }) =
                            p.pending.last_mut()
                    {
                        for o in options.iter_mut() {
                            if let Item::Minion(m) = o {
                                make_golden(m);
                            }
                        }
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            }),
        ),
        // SI:7 Scout: after you buy a card, gain +2/+2 (+4/+4).
        (
            "TB_BaconShop_HERO_01_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Bought { side, .. } if *side == me.side) {
                        c.buff(me.eid, 2 * me.g(), 2 * me.g());
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Apostle of Galakrond: Battlecry: replace the Tavern's cards with ones from 1 (2)
        // Tiers higher.
        (
            "TB_BaconShop_HERO_02_Buddy",
            Script {
                battlecry: Some(|c, me, _| {
                    let Some(p) = c.owner[me.side] else { return };
                    let up = me.g() as u8;
                    let slots: Vec<(EntityId, u8, bool)> = c.g.players[p]
                        .shop
                        .iter()
                        .map(|s| match &s.item {
                            Item::Minion(m) => (s.sid, m.tier, true),
                            Item::Spell { card, .. } => (s.sid, db().get(*card).tier, false),
                        })
                        .collect();
                    for (sid, tier, minion) in slots {
                        let t = (tier + up).min(6);
                        let Some(i) = c.g.players[p].shop.iter().position(|s| s.sid == sid) else {
                            continue;
                        };
                        let item = if minion {
                            match c.draw_pool_minion(me.side, t, t, Races::NONE) {
                                Some(m) => Item::Minion(m),
                                None => continue,
                            }
                        } else {
                            match c.random_spell(|d| d.tier == t) {
                                Some(s) => Item::spell(s),
                                None => continue,
                            }
                        };
                        let new_sid = match &item {
                            Item::Minion(m) => m.eid,
                            _ => c.g.alloc_eid(),
                        };
                        let old = std::mem::replace(
                            &mut c.g.players[p].shop[i],
                            crate::state::ShopSlot::new(new_sid, item),
                        );
                        if let Item::Minion(m) = old.item {
                            c.g.pool.return_minion(&m);
                        }
                    }
                }),
                ..E
            },
        ),
        // Eclipsion Illidari: your first (two) attacking minion(s) are Immune while Attacking
        // for that attack only (counter on the combat copy = grants used).
        (
            "TB_BaconShop_HERO_08_Buddy",
            Script {
                on_event: Some(|c, me, e| match e {
                    Event::Attacks { side, attacker, .. } if *side == me.side && c.combat => {
                        let Some(m) = c.minion_mut(me.eid).filter(|m| m.counter < me.g()) else {
                            return;
                        };
                        m.counter += 1;
                        if let Some(a) = c.minion_mut(*attacker)
                            && !a.has(Kw::IMMUNE_ATTACKING)
                        {
                            a.give(Kw::IMMUNE_ATTACKING);
                            a.temp_kw.set(Kw::IMMUNE_ATTACKING);
                        }
                    }
                    Event::AttackDone { side, attacker } if *side == me.side => {
                        if let Some(a) = c.minion_mut(*attacker)
                            && a.temp_kw.has(Kw::IMMUNE_ATTACKING)
                        {
                            a.remove(Kw::IMMUNE_ATTACKING);
                            a.temp_kw.clear(Kw::IMMUNE_ATTACKING);
                        }
                    }
                    _ => {}
                }),
                listens: ev::ATTACKS | ev::ATTACK_DONE,
                ..E
            },
        ),
        // Bilgewater Mogul: end of turn, +1 (+2) maximum Gold.
        (
            "TB_BaconShop_HERO_10_Buddy",
            Script {
                end_of_turn: Some(|c, me| c.add_max_gold(me.side, me.g())),
                ..E
            },
        ),
        // Lucifron: end of turn effects trigger 1 (2) extra times (`end_of_turn_mult`).
        ("TB_BaconShop_HERO_11_Buddy", E),
        // Pigeon Lord: free Refreshes while the Tavern lacks your Hero Power's type
        // (`Game::refresh_price`).
        ("TB_BaconShop_HERO_12_Buddy", E),
        // Elder Taggawag: Start of Combat: with 4 minions of different types, gain your minions'
        // highest Attack and Health (double).
        (
            "TB_BaconShop_HERO_14_Buddy",
            Script {
                start_of_combat: Some(|c, me| {
                    if distinct_type_count(c, me.side) < 4 {
                        return;
                    }
                    let alive = c.boards[me.side].iter().filter(|m| m.alive());
                    let (a, h) = alive.fold((0, 0), |(a, h), m| (a.max(m.atk), h.max(m.hp)));
                    c.buff(me.eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                }),
                ..E
            },
        ),
        // Karl the Lost: after Hero Power use, Divine Shield minions +2 (+4) Attack (engine).
        ("TB_BaconShop_HERO_15_Buddy", E),
        // Snack Vendor: end of turn, give this minion's stats to a Tier 3 minion (golden: twice).
        (
            "TB_BaconShop_HERO_16_Buddy",
            Script {
                end_of_turn: Some(|c, me| {
                    for _ in 0..me.g() {
                        let Some((a, h)) = c.minion(me.eid).map(|m| (m.atk, m.hp)) else {
                            return;
                        };
                        let Some(t) =
                            c.random_friendly(me.side, |m| m.tier == 3 && m.eid != me.eid)
                        else {
                            return;
                        };
                        c.buff(t, a, h);
                    }
                }),
                ..E
            },
        ),
        // Elementium Squirrel Bomb: Deathrattle: 4 (8) damage to a random enemy minion for each
        // friendly Mech that died this combat.
        (
            "TB_BaconShop_HERO_17_Buddy",
            Script {
                deathrattle: Some(|c, me| {
                    let mechs = c.cs[me.side]
                        .dead
                        .iter()
                        // By the minion's types when it died.
                        .filter(|d| d.races.intersects(Races::MECH))
                        .count();
                    for _ in 0..mechs {
                        let foes: Vec<EntityId> = c.boards[1 - me.side]
                            .iter()
                            .filter(|m| m.alive())
                            .map(|m| m.eid)
                            .collect();
                        let Some(&t) = c.g.rng.pick(&foes) else {
                            return;
                        };
                        c.damage(me.eid, t, 4 * me.g());
                    }
                }),
                ..E
            },
        ),
        // Tuskarr Raider: Battlecry, Deathrattle and Rally: get 1 (2) random Bounties.
        (
            "TB_BaconShop_HERO_18_Buddy",
            Script {
                battlecry: Some(|c, me, _| give_random_of(c, me.side, &BOUNTIES, me.g())),
                deathrattle: Some(|c, me| give_random_of(c, me.side, &BOUNTIES, me.g())),
                rally: Some(|c, me, _| give_random_of(c, me.side, &BOUNTIES, me.g())),
                ..E
            },
        ),
        // Street Magician: 'Prestidigitation' chooses from Better Secrets (read by the Secret
        // system).
        ("TB_BaconShop_HERO_21_Buddy", E), // handled by Akazamzarak's power
        // Arfus: after a friendly minion is Reborn, give it this minion's Attack (twice).
        (
            "TB_BaconShop_HERO_22_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Reborn { side, eid } = e else {
                        return;
                    };
                    if *side != me.side {
                        return;
                    }
                    if let Some(a) = c.minion(me.eid).map(|m| m.atk) {
                        c.buff(*eid, a.saturating_mul(me.g()), 0);
                    }
                }),
                listens: ev::REBORN,
                ..E
            },
        ),
        // Muckslinger: Battlecry: get 1 (2) random Battlecry minions.
        (
            "TB_BaconShop_HERO_23_Buddy",
            Script {
                battlecry: Some(|c, me, _| {
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        if let Some(m) =
                            c.random_minion(|d| d.mech.has(Mech::BATTLECRY) && d.tier <= tier)
                        {
                            c.give_card(me.side, m);
                        }
                    }
                }),
                ..E
            },
        ),
        // Unearthed Underling: whenever your hero takes damage, rewind it and gain stats equal to
        // that amount (double).
        (
            "TB_BaconShop_HERO_25_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::HeroDamaged { side, amount, .. } = e else {
                        return;
                    };
                    if *side != me.side || *amount <= 0 {
                        return;
                    }
                    c.rewind_hero_damage(me.side);
                    c.buff(
                        me.eid,
                        amount.saturating_mul(me.g()),
                        amount.saturating_mul(me.g()),
                    );
                }),
                listens: ev::HERO_DAMAGED,
                ..E
            },
        ),
        // Thawed Champion: end of turn, make 1 (2) random Frozen Tavern minions Golden.
        (
            "TB_BaconShop_HERO_27_Buddy",
            Script {
                end_of_turn: Some(|c, me| {
                    let Some(p) = c.owner[me.side] else { return };
                    let mut cands: Vec<EntityId> = c.g.players[p]
                        .shop
                        .iter()
                        .filter(|s| s.frozen)
                        .filter_map(|s| {
                            s.item
                                .as_minion()
                                .filter(|m| !m.golden && db().get(m.card).golden.is_some())
                                .map(|_| s.sid)
                        })
                        .collect();
                    c.g.rng.shuffle(&mut cands);
                    for sid in cands.into_iter().take(me.g() as usize) {
                        if let Some(m) = c.find_any_mut(sid) {
                            make_golden(m);
                        }
                    }
                }),
                ..E
            },
        ),
        // Clockwork Assistant: Battlecry: Discover 1 (2) minions from a Tier higher than yours.
        (
            "TB_BaconShop_HERO_28_Buddy",
            Script {
                battlecry: Some(|c, me, _| {
                    let t = (c.tier(me.side) + 1).min(6);
                    for _ in 0..me.g() {
                        c.discover_minion(me.side, me.card, 3, |d| d.tier == t);
                    }
                }),
                ..E
            },
        ),
        // Tentacle of C'Thun / Mishmash: stat watchers in the engine (`stat_watchers`).
        ("TB_BaconShop_HERO_29_Buddy", E),
        ("TB_BaconShop_HERO_33_Buddy", E),
        // Weebomination: end of turn, the minion to the left (golden: adjacent minions) +1
        // Health, improved by each Health your hero is missing.
        (
            "TB_BaconShop_HERO_34_Buddy",
            Script {
                end_of_turn: Some(|c, me| {
                    // Missing from the hero's full Health: 60 for Patchwerk, whose Buddy
                    // this is.
                    let missing = c.player_ref(me.side).map_or(0, |p| {
                        let patchwerk = p.hero == Some(crate::cardid!("TB_BaconShop_HERO_34"));
                        (if patchwerk { 60 } else { 30 } - p.health).max(0)
                    });
                    let k = 1 + missing;
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
                        c.buff(t, 0, k);
                    }
                }),
                ..E
            },
        ),
        // Acolyte of Yogg-Saron: at the start of your turn, spin the Wheel of Yogg-Saron
        // (golden: its effect twice).
        (
            "TB_BaconShop_HERO_35_Buddy",
            Script {
                // Golden: one spin, effect doubled.
                start_of_turn: Some(|c, me| super::yogg::wheel_of_yogg(c, me.side, me.golden)),
                ..E
            },
        ),
        // Asher the Haberdasher: after you sell a minion, gain 2 (4) +1/+1 hats that pass to
        // a friendly minion when sold. The hats are on the wearer (`Player::hats`): each one
        // goes to a random friendly minion when its wearer is sold, Asher or a later wearer
        // (`Ctx::pass_hats`, as Dancin' Deryl's hats do), and a triple keeps them.
        (
            "TB_BaconShop_HERO_36_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Sold { side, eid, .. } = e
                        && *side == me.side
                        && *eid != me.eid
                    {
                        let k = 2 * me.g();
                        c.buff(me.eid, k, k);
                        if let Some(p) = c.player(me.side) {
                            p.hats.extend(std::iter::repeat_n(me.eid, k as usize));
                        }
                    }
                }),
                listens: ev::SOLD,
                ..E
            },
        ),
        // Kil'rek: Taunt. Deathrattle: get 1 (2) random Demons.
        (
            "TB_BaconShop_HERO_37_Buddy",
            Script {
                deathrattle: Some(|c, me| {
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        if let Some(d) =
                            c.random_minion(|d| d.races.intersects(Races::DEMON) && d.tier <= tier)
                        {
                            c.give_card(me.side, d);
                        }
                    }
                }),
                ..E
            },
        ),
        // Crazy Monkey: when sold, feed 1 (2) Bananas (+1/+1 each) to your minions; improves (by
        // the base) after every 2 spells cast. The counter is twice the improvements, in steps of
        // the plain card, plus 1 while one spell of the next pair is cast (`flag::IMPROVES`: a
        // triple keeps the growth).
        (
            "TB_BaconShop_HERO_38_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, .. } if *side == me.side)
                        && let Some(m) = c.minion_mut(me.eid)
                    {
                        m.counter += if m.counter % 2 == 0 {
                            1
                        } else {
                            2 * me.g() - 1
                        };
                    }
                }),
                listens: ev::SPELL_CAST,
                on_sell: Some(|c, me| {
                    let k = me.g() + me.data / 2;
                    for t in c.friendly_where(me.side, |m| m.eid != me.eid) {
                        c.buff(t, k, k);
                    }
                }),
                flags: flag::IMPROVES,
                ..E
            },
        ),
        // Bananas: give a minion +1/+1.
        (
            "BG_EX1_014t",
            Script {
                spell: Some(|c, _, t| c.buff(t, 1, 1)),
                spell_target: TargetSpec::FriendlyOrTavern,
                ..E
            },
        ),
    ]
}
