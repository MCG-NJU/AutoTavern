//! Hero Power framework users: swapping / adding Hero Powers, discounts and cooldowns.

use super::*;

/// Slot of Hero Power `card` for `side` (0 if not found).
fn slot_of(c: &Ctx, side: Side, card: CardId) -> usize {
    c.player_ref(side)
        .and_then(|p| p.powers.iter().position(|h| h.card == card))
        .unwrap_or(0)
}

/// "Make 'X' your second Hero Power" (Power of X spells, Casts When Bought).
fn power_of() -> Script {
    Script {
        spell: Some(|c, me, _| {
            let text = &db().get(me.card).text;
            let name = text.split('\'').nth(1).unwrap_or_default();
            if let Some(hp) = hero_power_named(name) {
                storm_replaced(c, me.side, 1);
                c.set_hero_power(me.side, 1, hp);
            }
        }),
        flags: flag::CAST_ON_BUY,
        ..E
    }
}

/// "Each turn, your next Hero Power costs (1) less" (the discount ends when it is used): the cost
/// drops as each recruit phase ends (official 24.0 developer note), so the first discount shows on
/// turn 2.
fn discount_each_turn(c: &mut Ctx, me: Me) {
    if c.g.turn > 1
        && let Some(h) = c.power_mut(me.side, me.card)
    {
        h.cost_mod -= 1;
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    let mut v = vec![
        // Sir Finley: at the start of the game, Discover a Hero Power (replaces this).
        (
            "TB_BaconShop_HP_057",
            Script {
                on_attach: Some(|c, me| {
                    let slot = slot_of(c, me.side, me.card);
                    c.offer_hero_powers(me.side, 3, slot, &[]);
                }),
                flags: flag::PASSIVE,
                ..E
            },
        ),
        // Power of the Storm: choose from 2 new Hero Powers every turn (`hero_start_of_turn`). The
        // effect belongs to whoever gets this power, in the slot it was put in - Master Nguyen, or
        // another hero through Sir Finley, a power swap or Reinvigorating Light.
        (
            "BG20_HERO_202p",
            Script {
                on_attach: Some(|c, me| {
                    // (Also when it comes back after a swap had ended an earlier one.)
                    let slot = slot_of(c, me.side, me.card);
                    c.set_card_data(me.side, me.card, slot as i32 + 1);
                    // Got on Turn 1 after the turn had started (Finley's Discover): the
                    // first choice is not lost. Later in the game it starts next turn.
                    if c.g.turn == 1 && !c.combat {
                        super::power_of_the_storm(c, me.side);
                    }
                }),
                flags: flag::PASSIVE,
                ..E
            },
        ),
        // Genn Greymane: on Turn 4, Discover two Hero Powers to replace this.
        (
            "BG35_HERO_001p",
            Script {
                start_of_turn: Some(|c, me| {
                    if c.g.turn == 4 {
                        let open = |c: &Ctx| c.player_ref(me.side).map_or(0, |p| p.pending.len());
                        let before = open(c);
                        let slot = slot_of(c, me.side, me.card);
                        let first = c.offer_hero_powers(me.side, 3, slot, &[]);
                        c.offer_hero_powers(me.side, 3, crate::actions::N_POWERS - 1, &first);
                        // Each pick of these acts on this turn's start (`Game::choose`).
                        let n = open(c).saturating_sub(before);
                        if n > 0 {
                            c.add_turn_data(me.side, me.card, n as i32);
                        }
                    }
                }),
                // "On Turn 4, Discover two Hero Powers to replace this. (3 turns left!)"
                shown: Some(|c, _| vec![4, super::heroes::shown_turns_left(c, 4)]),
                flags: flag::PASSIVE,
                ..E
            },
        ),
        // Unmasked Identity / Training Session: Discover a new Hero Power.
        (
            "EBG_Spell_037",
            Script {
                spell: Some(|c, me, _| {
                    c.offer_hero_powers(me.side, 3, 0, &[]);
                }),
                ..E
            },
        ),
        (
            "BGS_Treasures_011",
            Script {
                spell: Some(|c, me, _| {
                    c.offer_hero_powers(me.side, 3, 0, &[]);
                }),
                ..E
            },
        ),
        // Reinvigorating Light: Discover a second Hero Power. Gain 3 Gold.
        (
            "BG36_MagicItem_411",
            Script {
                on_attach: Some(|c, me| {
                    c.offer_hero_powers(me.side, 3, 1, &[]);
                    c.gain_gold(me.side, 3);
                }),
                ..E
            },
        ),
        // Snake Eyes: roll a die, gain that much Gold; can't be used for that many turns. The turn
        // of the roll counts as the first: a 1 is ready again next turn.
        (
            "BG28_HERO_400p",
            Script {
                hero_power: Some(|c, me, _| {
                    let roll = 1 + c.g.rng.below(6) as i32;
                    c.gain_gold(me.side, roll);
                    let who = c.who(me.side);
                    c.g.log(|| format!("{who} rolls {roll}"));
                    if let Some(h) = c.power_mut(me.side, me.card) {
                        h.cooldown = roll as u8;
                    }
                }),
                ..E
            },
        ),
        // Patches: get a Pirate; after you buy a Pirate, your next Hero Power costs (1) less.
        (
            "TB_BaconShop_HP_072",
            Script {
                hero_power: Some(|c, me, _| {
                    let tier = c.tier(me.side);
                    if let Some(card) =
                        c.random_minion(|d| d.races.intersects(Races::PIRATE) && d.tier <= tier)
                    {
                        c.give_card(me.side, card);
                    }
                }),
                on_event: Some(|c, me, e| {
                    if let Event::Bought { side, card, .. } = e
                        && *side == me.side
                        && db().get(*card).races.intersects(Races::PIRATE)
                        && let Some(h) = c.power_mut(me.side, me.card)
                    {
                        h.cost_mod -= 1;
                    }
                }),
                listens: ev::BOUGHT,
                flags: flag::HP_DISCOUNT_RESETS,
                ..E
            },
        ),
        // Elise Starseeker: Discover a minion from your Tier; costs (1) more after each use.
        (
            "TB_BaconShop_HP_047",
            Script {
                hero_power: Some(|c, me, _| {
                    let tier = c.tier(me.side);
                    c.discover_minion(me.side, me.card, 3, |d| d.tier == tier);
                    if let Some(h) = c.power_mut(me.side, me.card) {
                        h.cost_mod += 1;
                    }
                }),
                ..E
            },
        ),
        // Heistbaron Togwaggle: steal all cards from the Tavern; each turn, your next Hero
        // Power costs (1) less.
        (
            "BG23_HERO_305p",
            Script {
                hero_power: Some(|c, me, _| {
                    let Some(p) = c.owner[me.side] else { return };
                    let sids: Vec<EntityId> = c.g.players[p].shop.iter().map(|s| s.sid).collect();
                    for sid in sids {
                        let size = c.g.cfg.hand_size;
                        if c.player_ref(me.side).is_none_or(|pl| pl.hand.len() >= size) {
                            break;
                        }
                        let Some(i) = c.g.players[p].shop.iter().position(|s| s.sid == sid) else {
                            continue;
                        };
                        let item = c.g.players[p].shop.remove(i).item;
                        c.add_to_hand(me.side, item);
                    }
                }),
                start_of_turn: Some(discount_each_turn),
                flags: flag::HP_DISCOUNT_RESETS,
                ..E
            },
        ),
        // Farseer Nobundo: get a copy of the last Tavern spell you cast; each turn, your next
        // Hero Power costs (1) less.
        (
            "BG31_HERO_003p",
            Script {
                hero_power: Some(|c, me, _| {
                    give_last_tavern_spell(c, me.side, 1);
                }),
                start_of_turn: Some(discount_each_turn),
                flags: flag::HP_DISCOUNT_RESETS,
                ..E
            },
        ),
    ];
    for id in [
        "BG34_HeroPowerSpell_003",
        "BG34_HeroPowerSpell_005",
        "BG34_HeroPowerSpell_006",
        "BG34_HeroPowerSpell_008",
        "BG34_HeroPowerSpell_009",
        "BG34_HeroPowerSpell_010",
        "BG34_HeroPowerSpell_012",
        "BG34_HeroPowerSpell_015",
        "BG34_HeroPowerSpell_016",
        "BG34_HeroPowerSpell_017",
        "BG34_HeroPowerSpell_018",
        "BG34_HeroPowerSpell_022",
    ] {
        v.push((id, power_of()));
    }
    v
}
