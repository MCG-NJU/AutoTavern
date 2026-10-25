//! Heroes with their own mechanics: Thorim, Galewing, Cariel, Putricide, Tras'tath, Xavius.

use super::*;

const FLIGHTPATHS: [&str; 3] = [
    "BG20_HERO_283p_t1",
    "BG20_HERO_283p_t2",
    "BG20_HERO_283p_t3",
];
/// Cariel's three fixed upgrade options: United Stance, +1 Attack, +1 Health (the +2 cards belong
/// to her buddy, Captain Fairmount).
const CARIEL_UPGRADES: [&str; 3] = [
    "BG21_HERO_000p_t1",
    "BG21_HERO_000_Buddyt",
    "BG21_HERO_000_Buddyt2",
];

/// Discovers whose picks carry a Dark Gift (their gift's on-gain effect runs).
pub fn gift_discover_source(source: CardId) -> bool {
    matches!(
        db().get(source).id.as_str(),
        "BG36_HERO_101p" | "BG36_HERO_105p"
    )
}

/// Discover a minion (tiers `min..=max`, from the pool) with a fitting Dark Gift. Tras'tath's
/// start-of-game Discover uses its own gift list and ignores the turn windows.
pub fn discover_with_gift(c: &mut Ctx, side: Side, source: CardId, min: u8, max: u8) {
    let d = db();
    let trastath = d.get(source).id == "BG36_HERO_101p";
    // Tras'tath's list ignores the turn windows and the state-dependent offer rules.
    let turn = if trastath { 99 } else { c.g.turn };
    let gifts: Vec<CardId> = if trastath {
        super::gifts::TRASTATH_GIFTS
            .iter()
            .map(|x| cid(x))
            .filter(|g| c.g.gift_universe.contains(g))
            .collect()
    } else {
        window_gifts(c)
    };
    let cands: Vec<CardId> =
        c.g.universe
            .iter()
            .copied()
            .filter(|x| {
                let def = d.get(*x);
                def.tier >= min
                    && def.tier <= max
                    && c.g.pool.get(*x) > 0
                    && crate::recruit::gift_discoverable(def, turn)
            })
            .collect();
    c.gift_start = trastath;
    let options = gifted_options(c, side, &cands, 3, &gifts, Races::NONE, false);
    c.gift_start = false;
    c.offer_discover(side, source, options);
}

fn passive(s: Script) -> Script {
    Script {
        flags: s.flags | flag::PASSIVE,
        hero_power_uses: 0,
        ..s
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Thorim: at the start of the game, Discover a Tier 7 minion to get after you spend 60
        // Gold. It sits in the hand, locked, until then (`after_discover`, `release_held`);
        // memory = the card still held.
        (
            "BG27_HERO_801p2",
            passive(Script {
                on_attach: Some(|c, me| c.discover_tier7(me.side, me.card, 3)),
                on_event: Some(|c, me, e| {
                    let Event::GoldSpent { side, amount } = e else {
                        return;
                    };
                    if *side != me.side {
                        return;
                    }
                    let Some(h) = c.power_mut(me.side, me.card) else {
                        return;
                    };
                    // The count for "(60 left!)". The card in the hand unlocks by its own
                    // condition (`release_ready_held`); what is still in `memory` here is
                    // a pick that found no room in the hand and is given now.
                    h.counter = h.counter.saturating_add(*amount).min(60);
                    if h.counter >= 60 {
                        let got = std::mem::take(&mut h.memory);
                        for x in got {
                            super::release_held(c, me.side, x);
                        }
                    }
                }),
                listens: ev::GOLD_SPENT,
                // "Passive. At the start of the game, Discover a Tier 7 minion to get after you
                // spend 60 Gold. (60 left!)"
                shown: Some(|c, sh| vec![7, 60, super::heroes::shown_left(c, sh, 60)]),
                ..E
            }),
        ),
        // Tras'tath: at the start of the game, Discover a Tier 5 minion with a Dark Gift; it
        // unlocks on Turn 7 (locked in hand until then).
        (
            "BG36_HERO_101p",
            passive(Script {
                on_attach: Some(|c, me| discover_with_gift(c, me.side, me.card, 5, 5)),
                // "At the start of the game, Discover a Tier 5 minion with a Dark Gift. It
                // unlocks on Turn 7. (6 turns left!)"
                shown: Some(|c, _| vec![5, 7, super::heroes::shown_turns_left(c, 7)]),
                ..E
            }),
        ),
        // Xavius: every 4 turns, Discover a minion with a Dark Gift. It is a free press of the Dark
        // Gift button: the same Tiers, most-common-type and Tier 6 guarantees, but no Gold and no
        // use of the button.
        (
            "BG36_HERO_105p",
            passive(Script {
                start_of_turn: Some(|c, me| {
                    if c.period_turn(me.side, me.card, 4) {
                        let options = c.dark_gift_options(me.side);
                        c.offer_discover(me.side, me.card, options);
                    }
                }),
                // "Every 4 turns, Discover a minion with a Dark Gift. (4 turns left!)": on the
                // game's Turns 4, 8... (`period_turn`; the player's card data).
                shown: Some(|c, sh| vec![4, (4 - c.card_data(sh.side, sh.card)).max(0)]),
                ..E
            }),
        ),
        // Galewing: choose a flightpath (when none is in progress); complete it for its bonus.
        // memory = [current or last path], counter = turns left (0 = none in progress). The
        // path just completed is not offered again (only the other two).
        (
            "BG20_HERO_283p",
            Script {
                hero_power: Some(|c, me, _| {
                    let last = c
                        .power_mut(me.side, me.card)
                        .and_then(|h| h.memory.first().copied());
                    let options = FLIGHTPATHS
                        .iter()
                        .map(|x| cid(x))
                        .filter(|&x| Some(x) != last)
                        .map(Item::spell)
                        .collect();
                    c.offer_discover(me.side, me.card, options);
                }),
                start_of_turn: Some(|c, me| {
                    let Some(h) = c.power_mut(me.side, me.card) else {
                        return;
                    };
                    let Some(&path) = h.memory.first() else {
                        return;
                    };
                    if h.counter <= 0 {
                        return;
                    }
                    h.counter -= 1;
                    if h.counter > 0 {
                        return;
                    }
                    // Flight Trainer: flightpaths trigger twice (golden: three times).
                    let times = 1 + c.boards[me.side]
                        .iter()
                        .filter(|m| m.is_card("BG20_HERO_283_Buddy"))
                        .map(|m| if m.golden { 2 } else { 1 })
                        .max()
                        .unwrap_or(0);
                    for _ in 0..times {
                        match db().get(path).id.as_str() {
                            "BG20_HERO_283p_t1" => {
                                let tier = c.tier(me.side);
                                if let Some(s) = c.random_spell(|d| d.cost == 1 && d.tier <= tier) {
                                    c.give_card(me.side, s);
                                }
                            }
                            "BG20_HERO_283p_t2" => c.gain_gold(me.side, 2),
                            _ => {
                                let tier = c.tier(me.side);
                                c.discover_minion(me.side, me.card, 3, |d| d.tier == tier);
                            }
                        }
                    }
                }),
                ..E
            },
        ),
        // Cariel Roame: give 1 (+ United Stances) random friendly minions +1/+1 (+ Stances);
        // after each combat, choose an improvement (levels: values = [minions, atk, hp]).
        (
            "BG21_HERO_000p",
            Script {
                hero_power: Some(|c, me, _| {
                    let (n, a, h) = cariel(c, me);
                    let mut f = c.friendly(me.side);
                    c.g.rng.shuffle(&mut f);
                    for t in f.into_iter().take(n as usize) {
                        c.buff(t, a, h);
                    }
                }),
                start_of_turn: Some(|c, me| {
                    if c.g.turn > 1 {
                        let options = CARIEL_UPGRADES
                            .iter()
                            .map(|u| Item::spell(cid(u)))
                            .collect();
                        c.offer_discover(me.side, me.card, options);
                    }
                }),
                // "Give 1 random friendly minion +1/+1. Passive: After each combat, choose an
                // improvement."
                shown: Some(|c, sh| {
                    let me = Me {
                        side: sh.side,
                        eid: 0,
                        card: sh.card,
                        golden: false,
                        opt: 0,
                        pos: 0,
                        data: 0,
                    };
                    let (n, a, h) = cariel(c, me);
                    vec![n, a, h]
                }),
                ..E
            },
        ),
        // Professor Putricide: craft a custom Undead (3 Creations): a 1/1 Creation with stats
        // and a Bonus Keyword chosen for your Tier (approximation of the part picker).
        (
            "BG25_HERO_100p",
            Script {
                hero_power: Some(|c, me, _| putricide_craft(c, me.side)),
                // "Craft a custom Undead. (3 Creations left!)": the uses are counted in
                // `uses_game` (`hero_power_available`).
                shown: Some(|c, sh| {
                    let used = c
                        .player_ref(sh.side)
                        .and_then(|p| p.power_of(sh.card))
                        .map_or(0, |h| h.uses_game as i32);
                    vec![(3 - used).max(0)]
                }),
                ..E
            },
        ),
    ]
}

/// Craft a custom Undead (Putricide; approximation): a Tier x 2 Creation with a random Bonus
/// Keyword.
/// Build-An-Undead (two picks; `content::putricide`).
pub fn putricide_craft(c: &mut Ctx, side: Side) {
    super::putricide::craft(c, side);
}

/// Cariel's Conviction: (minions, +atk, +hp) after improvements.
pub fn cariel(c: &Ctx, me: Me) -> (i32, i32, i32) {
    let v = c
        .player_ref(me.side)
        .and_then(|p| p.power_of(me.card))
        .map(|h| h.values.clone())
        .unwrap_or_default();
    let get = |i: usize| v.get(i).copied().unwrap_or(0);
    (
        get(0).saturating_add(1),
        get(1).saturating_add(1),
        get(2).saturating_add(1),
    )
}

/// Apply a Cariel improvement card to her Conviction power of `side`.
pub fn improve_conviction(c: &mut Ctx, side: Side, card: CardId) {
    let conviction = cid("BG21_HERO_000p");
    let (i, k) = match db().get(card).id.as_str() {
        "BG21_HERO_000p_t1" => (0, 1),
        "BG21_HERO_000_Buddyt" => (1, 1),
        "BG21_HERO_000_Buddyt3" => (1, 2),
        "BG21_HERO_000_Buddy_Gt" => (1, 4),
        "BG21_HERO_000_Buddyt2" => (2, 1),
        "BG21_HERO_000_Buddyt4" => (2, 2),
        "BG21_HERO_000_Buddyt_Gt2" => (2, 4),
        _ => return,
    };
    if let Some(h) = c.power_mut(side, conviction) {
        if h.values.len() < 3 {
            h.values.resize(3, 0);
        }
        h.values[i] = h.values[i].saturating_add(k);
    }
}
