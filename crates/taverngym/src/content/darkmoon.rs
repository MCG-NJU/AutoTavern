//! Darkmoon Prizes: tiered prizes (BATTLEGROUNDS_DARKMOON_PRIZE_TURN 1-4) Discovered through
//! Tickatus and other channels.

use super::*;

/// Darkmoon Prize tier for the current turn (Tickatus: every 4 turns, tier 1..4).
pub fn prize_tier_for_turn(turn: u32) -> u8 {
    (turn / 4).clamp(1, 4) as u8
}

/// Discover a Darkmoon Prize of `tier`.
pub fn discover_prize(c: &mut Ctx, side: Side, source: CardId, tier: u8) {
    let cands: Vec<CardId> = db()
        .iter()
        .filter(|d| d.prize_tier == tier && d.normal.is_none())
        .map(|d| d.cid)
        .collect();
    let picks = c.g.rng.sample_indices(cands.len(), 3);
    let options = picks
        .into_iter()
        .map(|i| {
            let x = cands[i];
            if db().get(x).is_minion() {
                Item::Minion(c.new_minion(side, x))
            } else {
                Item::spell(x)
            }
        })
        .collect();
    c.offer_discover(side, source, options);
}

fn sp(f: crate::script::FxT) -> Script {
    Script {
        spell: Some(f),
        ..E
    }
}

fn friendly_spell(f: crate::script::FxT) -> Script {
    Script {
        spell: Some(f),
        spell_target: TargetSpec::Friendly,
        ..E
    }
}

/// A persistent "this game" effect (runs its hooks once per stack).
fn effect(s: Script) -> Script {
    Script {
        spell: Some(|c, me, _| c.add_player_effect(me.side, me.card, 1)),
        ..s
    }
}

/// New Recruit's extra Tavern minion: any type up to your Tier, with +2/+2.
fn new_recruit(c: &mut Ctx, side: Side) {
    let tier = c.tier(side);
    if let Some(e) = c.add_shop_minion(side, 1, tier, Races::NONE) {
        c.buff(e, 2, 2);
    }
}

fn fill_hand(c: &mut Ctx, side: Side, mut next: impl FnMut(&mut Ctx) -> Option<CardId>) {
    let size = c.g.cfg.hand_size;
    while c.player_ref(side).is_some_and(|p| p.hand.len() < size) {
        let Some(x) = next(c) else { break };
        if c.give_card(side, x).is_none() {
            break;
        }
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Tickatus: every 4 turns, Discover a Darkmoon Prize (tier by turn).
        (
            "TB_BaconShop_HP_106",
            Script {
                // On the game's Turns 4, 8... (`period_turn`), also when gained later.
                start_of_turn: Some(|c, me| {
                    if c.period_turn(me.side, me.card, 4) {
                        let tier = prize_tier_for_turn(c.g.turn);
                        discover_prize(c, me.side, me.card, tier);
                    }
                }),

                flags: flag::PASSIVE,
                // "Every 4 turns, Discover a Darkmoon Prize. (4 turns left!)"
                shown: Some(|c, sh| vec![4, (4 - c.card_data(sh.side, sh.card)).max(0)]),
                ..E
            },
        ),
        // All-Seeing Elder: Start of Combat: gain the Attack of your left-most minion and the
        // Health of your right-most minion.
        (
            "BG33_300",
            Script {
                // Golden: twice; the second pass reads the values after the first.
                start_of_combat: Some(|c, me| {
                    for _ in 0..me.g() {
                        let a = c
                            .leftmost(me.side)
                            .and_then(|t| c.minion(t))
                            .map_or(0, |m| m.atk);
                        let h = c
                            .rightmost(me.side)
                            .and_then(|t| c.minion(t))
                            .map_or(0, |m| m.hp);
                        c.buff(me.eid, a, h);
                    }
                }),
                ..E
            },
        ),
        (
            "BGS_Treasures_004",
            sp(|c, me, _| c.discover_minion(me.side, me.card, 3, |d| d.tier == 1)),
        ),
        // Evolving Tavern: replace all cards in the Tavern (minions and spells) with ones of a Tier
        // higher (at most Tier 6).
        (
            "BGS_Treasures_006",
            sp(|c, me, _| {
                let Some(p) = c.owner[me.side] else { return };
                let slots: Vec<(EntityId, u8, bool)> = c.g.players[p]
                    .shop
                    .iter()
                    .map(|s| match &s.item {
                        Item::Minion(m) => (s.sid, m.tier, true),
                        Item::Spell { card, .. } => (s.sid, db().get(*card).tier, false),
                    })
                    .collect();
                for (sid, tier, minion) in slots {
                    let t = (tier + 1).min(6);
                    let Some(i) = c.g.players[p].shop.iter().position(|s| s.sid == sid) else {
                        continue;
                    };
                    let item = if minion {
                        match c.draw_pool_minion(me.side, t, t, Races::NONE) {
                            Some(m) => Item::Minion(m),
                            None => continue,
                        }
                    } else {
                        match c.draw_pool_spell(|d| d.tier == t) {
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
        ),
        // Gruul Rules: give a friendly minion "At the end of your turn, gain +4/+4" (the prize
        // is attached as the enchantment).
        (
            "BGS_Treasures_009",
            Script {
                spell: Some(|c, me, t| {
                    if let Some(m) = c.minion_mut(t) {
                        m.attached.push(me.card);
                    }
                }),
                spell_target: TargetSpec::Friendly,
                end_of_turn: Some(|c, me| c.buff(me.eid, 4, 4)),
                ..E
            },
        ),
        // Time Thief: Discover a (plain) minion from your last opponent's warband.
        (
            "BGS_Treasures_010",
            sp(|c, me, _| {
                let Some(p) = c.owner[me.side] else { return };
                let Some(o) = c.g.last_fought(p) else {
                    return;
                };
                // Copies of the minions as they were (not "plain").
                let board = c.g.fought_board(o).clone();
                let picks = c.g.rng.sample_indices(board.len(), 3);
                let options = picks
                    .into_iter()
                    .map(|i| {
                        let mut m = board[i].clone();
                        m.eid = c.g.alloc_eid();
                        m.pool_copies = 0;
                        m.attacked = false;
                        m.combat_only = false;
                        Item::Minion(m)
                    })
                    .collect();
                c.offer_discover(me.side, me.card, options);
            }),
        ),
        (
            "BGS_Treasures_012",
            sp(|c, me, _| {
                let tier = c.tier(me.side);
                c.discover_minion(me.side, me.card, 3, |d| d.tier == tier);
            }),
        ),
        // The Good Stuff: minions in the Tavern +1/+1 this game.
        (
            "BGS_Treasures_013",
            sp(|c, me, _| {
                if let Some(p) = c.player(me.side) {
                    p.stats.tavern_atk = p.stats.tavern_atk.saturating_add(1);
                    p.stats.tavern_hp = p.stats.tavern_hp.saturating_add(1);
                }
                c.buff_tavern(me.side, 1, 1, Races::NONE);
            }),
        ),
        // The Unlimited Coin: gain 1 Gold; at the end of this turn it returns to your hand (an
        // end of turn effect: Drakkari / Lucifron multiply it). One effect stack per cast.
        (
            "BGS_Treasures_014",
            Script {
                spell: Some(|c, me, _| {
                    c.gain_gold(me.side, 1);
                    c.add_player_effect(me.side, me.card, 1);
                }),
                end_of_turn: Some(|c, me| {
                    // The end-of-turn multiplier repeats this hook (`run_player_hook_times`).
                    c.give_card(me.side, me.card);
                    if let Some(p) = c.player(me.side) {
                        if let Some(x) = p.effects.iter_mut().find(|x| x.card == me.card) {
                            x.stacks -= 1;
                        }
                        p.effects.retain(|x| x.stacks > 0 || x.card != me.card);
                    }
                }),
                ..E
            },
        ),
        (
            "BGS_Treasures_015",
            friendly_spell(|c, _, t| {
                c.buff(t, 10, 0);
                c.give_kw(t, Kw::DIVINE_SHIELD);
            }),
        ),
        // Raise the Stakes: make a friendly minion Golden and return it to your hand.
        (
            "BGS_Treasures_016",
            friendly_spell(|c, me, t| {
                let Some((s, i)) = c.locate(t) else { return };
                let mut m = c.boards[s].remove(i);
                make_golden(&mut m);
                c.add_to_hand(me.side, Item::Minion(m));
            }),
        ),
        // I'm Still Just a Rat in a Cage: +2 Attack, then double its Attack.
        (
            "BGS_Treasures_018",
            Script {
                spell: Some(|c, _, t| {
                    c.buff(t, 2, 0);
                    if let Some(a) = c.find_any(t).map(|m| m.atk) {
                        c.buff(t, a, 0);
                    }
                }),
                spell_target: TargetSpec::FriendlyOrTavern,
                ..E
            },
        ),
        (
            "BGS_Treasures_019",
            sp(|c, me, _| fill_hand(c, me.side, |_| Some(cid("BG28_897")))),
        ),
        // Top Shelf: Discover a minion from a Tier higher (up to Tier 7).
        (
            "BGS_Treasures_020",
            sp(|c, me, _| {
                let tier = c.tier(me.side) + 1;
                if tier >= 7 {
                    c.discover_tier7(me.side, me.card, 3);
                } else {
                    c.discover_minion(me.side, me.card, 3, |d| d.tier == tier);
                }
            }),
        ),
        // Friends and Family Discount: Tavern minions cost (2) this game (`minion_price`).
        ("BGS_Treasures_022", effect(E)),
        // Open Bar: gain 5 free Refreshes now and at the start of each turn.
        (
            "BGS_Treasures_023",
            Script {
                spell: Some(|c, me, _| {
                    c.add_player_effect(me.side, me.card, 1);
                    if let Some(p) = c.player(me.side) {
                        p.free_refreshes += 5;
                    }
                }),
                start_of_turn: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.free_refreshes += 5;
                    }
                }),
                ..E
            },
        ),
        ("BGS_Treasures_025", sp(|c, me, _| c.gain_gold(me.side, 12))),
        (
            "BGS_Treasures_026",
            friendly_spell(|c, _, t| {
                c.give_kw(t, Kw::TAUNT);
                if let Some(h) = c.minion(t).map(|m| m.hp) {
                    c.buff(t, 0, h);
                }
            }),
        ),
        (
            "BGS_Treasures_028",
            friendly_spell(|c, _, t| {
                c.give_kw(t, Kw::DIVINE_SHIELD);
                c.give_kw(t, Kw::WINDFURY);
                c.buff(t, 15, 15);
            }),
        ),
        // Rocking and Rolling: a free Refresh at the start of each turn.
        (
            "BGS_Treasures_029",
            effect(Script {
                start_of_turn: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.free_refreshes += 1;
                    }
                }),
                ..E
            }),
        ),
        // Big Brann Play: your Battlecries trigger an extra time this turn (`battlecry_mult`).
        (
            "BGS_Treasures_030",
            // Copies this turn stack (`turn_data`).
            sp(|c, me, _| {
                c.add_turn_data(me.side, me.card, 1);
            }),
        ),
        // Big Winner!: Discover a Darkmoon Prize from each previous Prize tier.
        (
            "BGS_Treasures_032",
            sp(|c, me, _| {
                for t in [3u8, 2, 1] {
                    discover_prize(c, me.side, me.card, t);
                }
            }),
        ),
        // New Recruit: the Tavern offers an extra +2/+2 minion this game - one at once when cast,
        // beside the cards already there (a sixth minion, a 5/3 Southsea Busker, right after the
        // cast), then one with every Refresh.
        (
            "BGS_Treasures_033",
            Script {
                spell: Some(|c, me, _| {
                    c.add_player_effect(me.side, me.card, 1);
                    new_recruit(c, me.side);
                }),
                on_fill_shop: Some(|c, me| new_recruit(c, me.side)),
                ..E
            },
        ),
        // Repeat Customer: return a friendly non-Golden minion to your hand; give it +6/+6.
        (
            "BGS_Treasures_034",
            Script {
                spell: Some(|c, me, t| {
                    let Some((s, i)) = c.locate(t) else { return };
                    let mut m = c.boards[s].remove(i);
                    m.add_stats(6, 6);
                    c.add_to_hand(me.side, Item::Minion(m));
                }),
                spell_target: TargetSpec::FriendlyNonGoldenMaxTier(7),
                ..E
            },
        ),
        // All That Glitters: make a random minion in the Tavern Golden.
        (
            "BGS_Treasures_037",
            sp(|c, me, _| {
                let t: Vec<EntityId> = c
                    .tavern_minions(me.side)
                    .into_iter()
                    .filter(|s| {
                        c.find_any(*s)
                            .is_some_and(|m| !m.golden && db().get(m.card).golden.is_some())
                    })
                    .collect();
                if let Some(&sid) = c.g.rng.pick(&t)
                    && let Some(m) = c.find_any_mut(sid)
                {
                    make_golden(m);
                }
            }),
        ),
        // Mindflayer Goggles: steal all cards from the Tavern, then Refresh it.
        (
            "BGS_Treasures_039",
            sp(|c, me, _| {
                let Some(p) = c.owner[me.side] else { return };
                let size = c.g.cfg.hand_size;
                while c.g.players[p].hand.len() < size && !c.g.players[p].shop.is_empty() {
                    let item = c.g.players[p].shop.remove(0).item;
                    c.add_to_hand(me.side, item);
                }
                c.refresh_shop(me.side);
            }),
        ),
        (
            "BGS_Treasures_040",
            sp(|c, me, _| give_n(c, me.side, "BG28_897", 2)),
        ),
        (
            "BGS_Treasures_100",
            sp(|c, me, _| {
                let tier = c.tier(me.side);
                if let Some(s) = c.random_spell(|d| d.cost >= 2 && d.tier <= tier) {
                    c.give_card(me.side, s);
                }
            }),
        ),
        (
            "BGS_Treasures_101",
            sp(|c, me, _| {
                let tier = c.tier(me.side);
                c.discover_spell(me.side, me.card, 3, |d| d.tier == tier);
            }),
        ),
        // Reserve Prices: Tavern spells cost (1) less this turn (`spell_price`).
        (
            "BGS_Treasures_104",
            sp(|c, me, _| {
                c.add_turn_data(me.side, me.card, 1);
            }),
        ),
        (
            "BGS_Treasures_106",
            sp(|c, me, _| {
                let tier = c.tier(me.side);
                fill_hand(c, me.side, |c| c.random_spell(|d| d.tier <= tier));
            }),
        ),
        (
            "BGS_Treasures_110",
            sp(|c, me, _| {
                if let Some(p) = c.player(me.side) {
                    p.stats.spell_atk = p.stats.spell_atk.saturating_add(1);
                    p.stats.spell_hp = p.stats.spell_hp.saturating_add(1);
                }
            }),
        ),
    ]
}
