//! Trinkets: stat growth and auras. Static auras are computed elsewhere: Tavern spell extras in
//! `Ctx::spell_extra` (Heart of the Forest, Honeycomb Ring, Multilayered Shield) and "your minions
//! have" in `engine::apply_bonuses` (Hammer of Twilight, Artisanal Urn). "Improve this" adds the
//! base amount again.

use super::trinkets::shown_left;
use super::*;
use crate::script::Shown;

fn friendly_of(c: &Ctx, side: Side, r: Races) -> Vec<EntityId> {
    c.friendly_where(side, |m| m.is(r))
}

fn tavern_spell_cast(me: Me, e: &Event) -> bool {
    matches!(e, Event::SpellCast { side, tavern: true, .. } if *side == me.side)
}

/// Left-most minion in hand (hand id).
fn leftmost_in_hand(c: &Ctx, side: Side) -> Option<EntityId> {
    c.hand_minions(side).first().map(|x| x.0)
}

/// Give up to `n` random friendly minions of type `r` +a/+h.
fn buff_random_of(c: &mut Ctx, side: Side, r: Races, n: usize, a: i32, h: i32) {
    let mut v = friendly_of(c, side, r);
    c.g.rng.shuffle(&mut v);
    for e in v.into_iter().take(n) {
        c.buff(e, a, h);
    }
}

fn leftmost_and_rightmost(c: &Ctx, side: Side) -> Vec<EntityId> {
    let mut v: Vec<EntityId> = [c.leftmost(side), c.rightmost(side)]
        .into_iter()
        .flatten()
        .collect();
    v.dedup();
    v
}

/// Re-sync "wherever they are" bonuses after the aura set changed.
fn resync(c: &mut Ctx, side: Side) {
    c.bump_player_stat(side, |_| {});
}

fn undead_attack(c: &mut Ctx, side: Side, n: i32) {
    c.bump_player_stat(side, |st| st.undead_atk = st.undead_atk.saturating_add(n));
}

fn spell_extra_this_game(c: &mut Ctx, side: Side, a: i32, h: i32) {
    if let Some(p) = c.player(side) {
        p.stats.spell_atk = p.stats.spell_atk.saturating_add(a);
        p.stats.spell_hp = p.stats.spell_hp.saturating_add(h);
    }
}

fn gold_mallet(c: &mut Ctx, me: Me, a: i32, h: i32) {
    let k = 1 + c.player_ref(me.side).map_or(0, |p| p.stats.golden_played);
    for e in c.friendly(me.side) {
        c.buff(e, a.saturating_mul(k), h.saturating_mul(k));
    }
}

/// Gold Mallet's text: "+a/+h" times one plus the Golden minions played this game.
fn shown_gold_mallet(c: &Ctx, sh: &Shown, a: i32, h: i32) -> Vec<i32> {
    let k = 1 + c.player_ref(sh.side).map_or(0, |p| p.stats.golden_played);
    vec![a.saturating_mul(k), h.saturating_mul(k)]
}

/// Hammer of Twilight's multiplier: one plus the cards discarded this game (`apply_bonuses`).
fn shown_hammer(c: &Ctx, sh: &Shown) -> i32 {
    1 + c.player_ref(sh.side).map_or(0, |p| p.stats.cards_discarded)
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Comfy Coffin: whenever you cast a Tavern spell, your Undead have +1 (+2) Attack this
        // game (wherever they are).
        (
            "BG30_MagicItem_547",
            Script {
                on_event: Some(|c, me, e| {
                    if tavern_spell_cast(me, e) {
                        undead_attack(c, me.side, 1);
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        (
            "BG30_MagicItem_547t",
            Script {
                on_event: Some(|c, me, e| {
                    if tavern_spell_cast(me, e) {
                        undead_attack(c, me.side, 2);
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Dragonwing Glider: whenever you play a card, give a friendly Dragon +4/+4.
        (
            "BG30_MagicItem_900",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::CardPlayed { side, .. } if *side == me.side) {
                        buff_random_of(c, me.side, Races::DRAGON, 1, 4, 4);
                    }
                }),
                listens: ev::CARD_PLAYED,
                ..E
            },
        ),
        // Nerglish Phrasebook: after you play a minion, the left-most minion in your hand gets
        // +3/+3 (+6/+6).
        (
            "BG30_MagicItem_914",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Played { side, .. } if *side == me.side)
                        && let Some(h) = leftmost_in_hand(c, me.side)
                    {
                        c.buff(h, 3, 3);
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        (
            "BG30_MagicItem_914t",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Played { side, .. } if *side == me.side)
                        && let Some(h) = leftmost_in_hand(c, me.side)
                    {
                        c.buff(h, 6, 6);
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Booty Bay Brew: whenever you spend Gold, give two friendly Pirates +3/+3 (+6/+6).
        (
            "BG30_MagicItem_924",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::GoldSpent { side, amount } if *side == me.side && *amount > 0)
                    {
                        buff_random_of(c, me.side, Races::PIRATE, 2, 3, 3);
                    }
                }),
                listens: ev::GOLD_SPENT,
                ..E
            },
        ),
        (
            "BG30_MagicItem_924t",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::GoldSpent { side, amount } if *side == me.side && *amount > 0)
                    {
                        buff_random_of(c, me.side, Races::PIRATE, 2, 6, 6);
                    }
                }),
                listens: ev::GOLD_SPENT,
                ..E
            },
        ),
        // Training Certificate: SoC double the stats of your two lowest-Attack minions (ties:
        // random).
        (
            "BG30_MagicItem_962",
            Script {
                start_of_combat: Some(|c, me| {
                    let mut v: Vec<(i32, usize, EntityId)> = c.boards[me.side]
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| m.alive())
                        .map(|(i, m)| (m.atk, i, m.eid))
                        .collect();
                    c.g.rng.shuffle(&mut v);
                    v.sort_by_key(|x| x.0);
                    for (_, _, e) in v.into_iter().take(2) {
                        if let Some((a, h)) = c.minion(e).map(|m| (m.atk, m.hp)) {
                            c.buff(e, a, h);
                        }
                    }
                }),
                ..E
            },
        ),
        // Artisanal Urn: your Undead have +20 Attack (`apply_bonuses`).
        (
            "BG30_MagicItem_989t",
            Script {
                on_attach: Some(|c, me| resync(c, me.side)),
                ..E
            },
        ),
        // Bronze Timepiece: SoC give each minion Health equal to half its Attack, rounded up (in
        // i64, as `a + 1` wraps at the largest Attack).
        (
            "BG30_MagicItem_995",
            Script {
                start_of_combat: Some(|c, me| {
                    for e in c.friendly(me.side) {
                        let a = c.minion(e).map_or(0, |m| i64::from(m.atk));
                        c.buff(e, 0, ((a + 1) / 2) as i32);
                    }
                }),
                ..E
            },
        ),
        // All-Purpose Kibble: whenever a friendly Beast attacks, give it +3 Attack and
        // permanently improve this.
        (
            "BG32_MagicItem_200",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Attacks { side, attacker, .. } = e
                        && *side == me.side
                        && c.minion(*attacker).is_some_and(|m| m.is(Races::BEAST))
                    {
                        let k = 3 * (1 + c.card_level(me.side, me.card));
                        c.buff(*attacker, k, 0);
                        c.improve_card(me.side, me.card);
                    }
                }),
                listens: ev::ATTACKS,
                // "Whenever a friendly Beast attacks, give it +3 Attack and permanently improve
                // this."
                shown: Some(|c, sh| vec![3 * (1 + c.card_level(sh.side, sh.card))]),
                ..E
            },
        ),
        // Shark Cannon: after you spend 7 Gold, give your Pirates +1/+1 and improve this.
        (
            "BG32_MagicItem_232",
            Script {
                on_event: Some(|c, me, e| {
                    gold_counter(c, me, e, 7, |c, me| {
                        let k = 1 + c.card_level(me.side, me.card);
                        for p in friendly_of(c, me.side, Races::PIRATE) {
                            c.buff(p, k, k);
                        }
                        c.improve_card(me.side, me.card);
                    })
                }),
                listens: ev::GOLD_SPENT,
                // "After you spend 7 Gold, give your Pirates +1/+1 and improve this. (7 Gold
                // left!)"
                shown: Some(|c, sh| {
                    let k = 1 + c.card_level(sh.side, sh.card);
                    vec![7, k, k, shown_left(c, sh, 7)]
                }),
                ..E
            },
        ),
        // Wicked Tome: Avenge (4): Tavern spells give an extra +1/+1 this game.
        (
            "BG32_MagicItem_270t",
            Script {
                avenge: Some((4, 4, |c, me| spell_extra_this_game(c, me.side, 1, 1))),
                ..E
            },
        ),
        // Enigmatic Headstone: at the end of each turn, your Undead have +2 Attack this game.
        (
            "BG32_MagicItem_276",
            Script {
                end_of_turn: Some(|c, me| undead_attack(c, me.side, 2)),
                ..E
            },
        ),
        // Heart of the Forest: Tavern spells +1/+1 (`spell_extra`); improves after every 6
        // Tavern spells cast from hand.
        (
            "BG32_MagicItem_801t",
            Script {
                on_event: Some(|c, me, e| {
                    if tavern_spell_cast(me, e)
                        && c.cast_from_hand
                        && c.tick_counter(me.side, me.card, 6)
                    {
                        c.improve_card(me.side, me.card);
                    }
                }),
                listens: ev::SPELL_CAST,
                // "Your Tavern spells give an extra +1/+1. After you cast 6 Tavern spells from
                // hand, improve this. (6 left!)"
                shown: Some(|c, sh| {
                    let k = 1 + c.card_level(sh.side, sh.card);
                    vec![k, k, 6, shown_left(c, sh, 6)]
                }),
                ..E
            },
        ),
        // Bluegill Flippers: whenever you cast a Tavern spell, the left-most minion in your
        // hand and in your warband get +3/+3.
        (
            "BG32_MagicItem_893",
            Script {
                on_event: Some(|c, me, e| {
                    if !tavern_spell_cast(me, e) {
                        return;
                    }
                    let targets = [leftmost_in_hand(c, me.side), c.leftmost(me.side)];
                    for t in targets.into_iter().flatten() {
                        c.buff(t, 3, 3);
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Fang Anklet: SoC your Beasts have +1/+1 this combat; after you summon a Beast,
        // improve this permanently.
        (
            "BG35_MagicItem_701",
            Script {
                start_of_combat: Some(|c, me| {
                    let k = 1 + c.card_level(me.side, me.card);
                    for b in friendly_of(c, me.side, Races::BEAST) {
                        c.buff(b, k, k);
                    }
                }),
                on_event: Some(|c, me, e| {
                    if let Event::Summoned { side, eid } = e
                        && *side == me.side
                        && c.minion(*eid).is_some_and(|m| m.is(Races::BEAST))
                    {
                        c.improve_card(me.side, me.card);
                    }
                }),
                listens: ev::SUMMONED,
                // "Start of Combat: Your Beasts have +1/+1 this combat. After you summon a Beast,
                // improve this permanently."
                shown: Some(|c, sh| {
                    let k = 1 + c.card_level(sh.side, sh.card);
                    vec![k, k]
                }),
                ..E
            },
        ),
        // Miniature Ship: after you cast a Tavern spell, give your Pirates +3/+3.
        (
            "BG35_MagicItem_710",
            Script {
                on_event: Some(|c, me, e| {
                    if tavern_spell_cast(me, e) {
                        for p in friendly_of(c, me.side, Races::PIRATE) {
                            c.buff(p, 3, 3);
                        }
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
        // Trusty Crowbar: whenever you get a Pirate, give your left-most minion +16/+16.
        (
            "BG35_MagicItem_713",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::CardAdded { side, hid } = e
                        && *side == me.side
                        && c.find_any(*hid).is_some_and(|m| m.is(Races::PIRATE))
                        && let Some(l) = c.leftmost(me.side)
                    {
                        c.buff(l, 16, 16);
                    }
                }),
                listens: ev::CARD_ADDED,
                ..E
            },
        ),
        // Mama Bear Sticker: whenever you summon a Beast, give it +6/+6.
        (
            "BG35_MagicItem_871",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Summoned { side, eid } = e
                        && *side == me.side
                        && c.minion(*eid).is_some_and(|m| m.is(Races::BEAST))
                    {
                        c.buff(*eid, 6, 6);
                    }
                }),
                listens: ev::SUMMONED,
                ..E
            },
        ),
        // Murky Sticker: at the end of each turn, your two left-most minions get +1/+2,
        // improved by each Battlecry you triggered this game.
        (
            "BG35_MagicItem_753",
            Script {
                end_of_turn: Some(|c, me| {
                    let k = 1 + c
                        .player_ref(me.side)
                        .map_or(0, |p| p.stats.battlecries_triggered);
                    for e in c.friendly(me.side).into_iter().take(2) {
                        c.buff(e, k, 2 * k);
                    }
                }),
                // "At the end of each turn, give your two left-most minions +1/+2. (Improved by
                // your Battlecries triggered this game!)"
                shown: Some(|c, sh| {
                    let k = 1 + c
                        .player_ref(sh.side)
                        .map_or(0, |p| p.stats.battlecries_triggered);
                    vec![k, 2 * k]
                }),
                ..E
            },
        ),
        // Dramaloc Sticker: SoC your Murlocs gain the Attack of the highest-Attack minion in
        // your hand.
        (
            "BG35_MagicItem_754",
            Script {
                start_of_combat: Some(|c, me| {
                    let a = c
                        .hand_minions(me.side)
                        .iter()
                        .map(|x| x.1)
                        .max()
                        .unwrap_or(0);
                    if a > 0 {
                        for m in friendly_of(c, me.side, Races::MURLOC) {
                            c.buff(m, a, 0);
                        }
                    }
                }),
                ..E
            },
        ),
        // Dragon Skull: whenever you trigger a Battlecry, your left- and right-most minions
        // get +5/+5.
        (
            "BG36_MagicItem_203",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::BattlecryTriggered { side, .. } if *side == me.side) {
                        for t in leftmost_and_rightmost(c, me.side) {
                            c.buff(t, 5, 5);
                        }
                    }
                }),
                listens: ev::BATTLECRY,
                ..E
            },
        ),
        // Gold Mallet: at the end of your turn, your minions get +1/+1 (+4/+2), improved by
        // each Golden minion you played this game.
        (
            "BG36_MagicItem_302",
            Script {
                end_of_turn: Some(|c, me| gold_mallet(c, me, 1, 1)),
                // "At the end of your turn, give your minions +1/+1. (Improved by Golden minions
                // you've played this game!)"
                shown: Some(|c, sh| shown_gold_mallet(c, sh, 1, 1)),
                ..E
            },
        ),
        (
            "BG36_MagicItem_302t",
            Script {
                end_of_turn: Some(|c, me| gold_mallet(c, me, 4, 2)),
                // "At the end of your turn, give your minions +4/+2. (Improved by Golden minions
                // you've played this game!)"
                shown: Some(|c, sh| shown_gold_mallet(c, sh, 4, 2)),
                ..E
            },
        ),
        // Honeycomb Ring: +1/+1 Tavern spell extra, improved for this turn by each spell you
        // cast on a minion (`spell_extra`).
        (
            "BG36_MagicItem_371",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::SpellCast { side, target, .. } if *side == me.side && *target != 0)
                    {
                        c.add_turn_data(me.side, me.card, 1);
                    }
                }),
                listens: ev::SPELL_CAST,
                // "Your Tavern spells give an extra +1/+1. After you cast a spell on a minion,
                // improve for this turn only."
                shown: Some(|c, sh| {
                    let k = 1 + c.turn_data(sh.side, sh.card);
                    vec![k, k]
                }),
                ..E
            },
        ),
        // Multilayered Shield: `spell_extra`.
        (
            "BG36_MagicItem_373",
            Script {
                // "Your Tavern spells give an extra +1/+2. (Improved by each different friendly
                // minion type!)"
                shown: Some(|c, sh| {
                    let races: Vec<Races> = c.boards[sh.side]
                        .iter()
                        .filter(|m| m.alive())
                        .map(|m| m.races)
                        .collect();
                    let k = 1 + distinct_types_of(&races);
                    vec![k, 2 * k]
                }),
                ..E
            },
        ),
        // Hammer of Twilight: `apply_bonuses`.
        (
            "BG36_MagicItem_403",
            Script {
                on_attach: Some(|c, me| resync(c, me.side)),
                // "Your minions have +1 Attack. (Improved by each card you've discarded this
                // game!)"
                shown: Some(|c, sh| vec![shown_hammer(c, sh)]),
                ..E
            },
        ),
        (
            "BG36_MagicItem_403t",
            Script {
                on_attach: Some(|c, me| resync(c, me.side)),
                // "Your minions have +2/+1. (Improved by each card you've discarded this game!)"
                shown: Some(|c, sh| {
                    let k = shown_hammer(c, sh);
                    vec![2 * k, k]
                }),
                ..E
            },
        ),
        // Weighted Gauntlet: at the end of your turn, your minions get +X/+X; X starts at 1 and
        // doubles at the start of each turn (card data = X), up to the stat limit i32::MAX.
        (
            "BG36_MagicItem_414",
            Script {
                on_attach: Some(|c, me| c.set_card_data(me.side, me.card, 1)),
                start_of_turn: Some(|c, me| {
                    let x = c.card_data(me.side, me.card).max(1);
                    c.set_card_data(me.side, me.card, x.saturating_mul(2));
                }),
                end_of_turn: Some(|c, me| {
                    let x = c.card_data(me.side, me.card).max(1);
                    for e in c.friendly(me.side) {
                        c.buff(e, x, x);
                    }
                }),
                // "At the end of your turn, give your minions +1/+1. (Doubles at the start of each
                // turn!)"
                shown: Some(|c, sh| {
                    let x = c.card_data(sh.side, sh.card).max(1);
                    vec![x, x]
                }),
                ..E
            },
        ),
        // Tome of the Ancients: after you discard 3 cards, Tavern spells give an extra +1/+1
        // this game.
        (
            "BG36_MagicItem_418",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Discarded { side, .. } if *side == me.side)
                        && c.tick_counter(me.side, me.card, 3)
                    {
                        spell_extra_this_game(c, me.side, 1, 1);
                    }
                }),
                listens: ev::DISCARDED,
                // "After you discard 3 cards, your Tavern spells give an extra +1/+1 this game. (3
                // left!)"
                shown: Some(|c, sh| vec![3, 1, 1, shown_left(c, sh, 3)]),
                ..E
            },
        ),
    ]
}
