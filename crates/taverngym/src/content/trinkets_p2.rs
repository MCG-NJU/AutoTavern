//! Trinkets: cards given or Discovered right away, "again each turn", and "after N things happen,
//! get a card". "Get a random <type>" cards are tier <= yours (simulator assumption, see the
//! assumptions table).

use super::trinkets::{shown_left, shown_turn_left};
use super::*;
use crate::cards::CardDef;

const KNOCKOFF_WISDOMBALL: &str = "BG30_802";
const MECHA_DEMONS: [&str; 4] = ["BG25_807t", "BG25_807t2", "BG25_807t3", "BG25_807t4"];

/// Get a random minion (tier <= yours) matching `f`.
fn get_random(c: &mut Ctx, side: Side, f: impl Fn(&CardDef) -> bool) {
    let tier = c.tier(side);
    if let Some(x) = c.random_minion(|d| d.tier <= tier && f(d)) {
        c.give_card(side, x);
    }
}

/// The minion type of a Compass trinket (its card data): chosen once, the player's most
/// common type, else a random type of the lobby.
pub(super) fn compass_type(c: &mut Ctx, side: Side, card: CardId) -> Races {
    let stored = c.card_data(side, card);
    if stored != 0 {
        return Races(stored as u16);
    }
    let lobby: Vec<Races> = c.g.lobby_races.iter().collect();
    let r = c
        .most_common_race_ex(side, true)
        .or_else(|| c.g.rng.pick(&lobby).copied())
        .unwrap_or(Races::NONE);
    c.set_card_data(side, card, r.0 as i32);
    r
}

fn get_random_spell(c: &mut Ctx, side: Side) {
    let tier = c.tier(side);
    if let Some(s) = c.random_spell(|d| d.tier <= tier) {
        c.give_card(side, s);
    }
}

fn discover_spells(c: &mut Ctx, me: Me, n: usize) {
    let tier = c.tier(me.side);
    for _ in 0..n {
        c.discover_spell(me.side, me.card, 3, |d| d.tier <= tier);
    }
}

/// Glass of Perspective: "get a random Choose One card" draws by copies left, as other random cards
/// do: a minion by its pool copies, a Tavern spell by its copies not in a Tavern or hand; uniform
/// among them when none has a copy left.
fn get_choose_one(c: &mut Ctx, side: Side, n: usize) {
    for _ in 0..n {
        let cards = choose_one_cards(c, side);
        let weighted: Vec<(CardId, u64)> = cards
            .iter()
            .map(|&x| {
                let left = if db().get(x).is_minion() {
                    u64::from(c.g.pool.get(x))
                } else {
                    c.g.spell_copies_left(x)
                };
                (x, left)
            })
            .collect();
        let pick = match c.weighted_distinct(weighted, 1).first() {
            Some(&x) => Some(x),
            None => c.g.rng.pick(&cards).copied(),
        };
        if let Some(x) = pick {
            c.give_card(side, x);
        }
    }
}

/// Discover (3 options) minions from `cands`, each with a fitting Dark Gift (`gifted_options`);
/// `golden` makes them Golden without a Triple Reward. Dark Paradox is only an option for
/// Maldraxxus Dagger, which copies a friendly minion (`recruit::gift_discoverable`).
fn discover_gifted(c: &mut Ctx, side: Side, source: CardId, cands: Vec<CardId>, golden: bool) {
    let turn = c.g.turn;
    let dagger = db().get(source).id == "BG36_MagicItem_370";
    let cands: Vec<CardId> = cands
        .into_iter()
        .filter(|x| {
            let def = db().get(*x);
            if dagger {
                crate::recruit::gift_compatible(def, turn)
            } else {
                crate::recruit::gift_discoverable(def, turn)
            }
        })
        .collect();
    let gifts = window_gifts(c);
    let options = gifted_options(c, side, &cands, 3, &gifts, Races::NONE, golden);
    c.offer_discover(side, source, options);
}

fn maldraxxus_dagger(c: &mut Ctx, me: Me) {
    let mut cands: Vec<CardId> = c.boards[me.side]
        .iter()
        .map(|m| db().normal_of(m.card))
        .collect();
    cands.sort_by_key(|x| x.0);
    cands.dedup();
    discover_gifted(c, me.side, me.card, cands, false);
}

/// Pool minions (in the lobby, copies left) matching `f`.
fn pool_where(c: &Ctx, f: impl Fn(&CardDef) -> bool) -> Vec<CardId> {
    c.g.universe
        .iter()
        .copied()
        .filter(|x| c.g.pool.get(*x) > 0 && f(db().get(*x)))
        .collect()
}

fn most_common(c: &mut Ctx, side: Side) -> Races {
    // Trinkets also look at the minions in your hand.
    c.most_common_race_ex(side, true).unwrap_or(Races::NONE)
}

/// "After N friendly minions die, get a random X."
fn deaths_then(c: &mut Ctx, me: Me, e: &Event, n: i32, race: Races) {
    if matches!(e, Event::Died { side, .. } if *side == me.side)
        && c.tick_counter(me.side, me.card, n)
    {
        get_random(c, me.side, |d| d.races.intersects(race));
    }
}

/// Steal the highest-Tier card from the Tavern (random among ties).
fn steal_highest(c: &mut Ctx, side: Side) {
    let Some(p) = c.owner[side] else { return };
    let tier = |s: &crate::state::ShopSlot| match &s.item {
        Item::Minion(m) => m.tier,
        Item::Spell { card, .. } => db().get(*card).tier,
    };
    let shop = &c.g.players[p].shop;
    let Some(top) = shop.iter().map(tier).max() else {
        return;
    };
    let ids: Vec<EntityId> = shop
        .iter()
        .filter(|s| tier(s) == top)
        .map(|s| s.sid)
        .collect();
    let Some(&sid) = c.g.rng.pick(&ids) else {
        return;
    };
    let shop = &mut c.g.players[p].shop;
    let Some(i) = shop.iter().position(|s| s.sid == sid) else {
        return;
    };
    let slot = shop.remove(i);
    c.add_to_hand(side, slot.item);
}

/// Splinter of Aurum: once per game, when you have 15 Gold.
fn aurum_check(c: &mut Ctx, me: Me) {
    if c.card_data(me.side, me.card) != 0 || c.player_ref(me.side).is_none_or(|p| p.gold < 15) {
        return;
    }
    c.set_card_data(me.side, me.card, 1);
    if let Some(x) = c.random_minion(|d| d.tier == 5) {
        c.give_card(me.side, db().golden_of(x));
    }
}

/// Splinter of Aurum after Gold was gained (`Ctx::gain_gold`): each copy checks, with its
/// hand protection.
pub fn aurum_gold_check(c: &mut Ctx, side: Side) {
    let splinter = crate::cardid!("BG32_MagicItem_350");
    let Some(p) = c.player_ref(side) else { return };
    let copies: Vec<CardId> = p
        .trinkets
        .iter()
        .copied()
        .filter(|t| t.base() == splinter)
        .collect();
    for card in copies {
        let me = Me {
            side,
            eid: 0,
            card,
            golden: false,
            opt: 0,
            pos: 0,
            data: 0,
        };
        let prev = c.src.replace(card);
        aurum_check(c, me);
        c.src = prev;
    }
}

macro_rules! every_turn {
    ($fx:expr) => {
        Script {
            on_attach: Some($fx),
            start_of_turn: Some($fx),
            ..E
        }
    };
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        (
            "BG30_MagicItem_406",
            every_turn!(|c, me| give_n(c, me.side, "BG28_604", 1)),
        ),
        // Book of Medivh: Discover a Tavern spell (tier <= yours); again each turn.
        (
            "BG30_MagicItem_420",
            every_turn!(|c, me| discover_spells(c, me, 1)),
        ),
        (
            "BG30_MagicItem_420t",
            every_turn!(|c, me| discover_spells(c, me, 2)),
        ),
        // Azeroth Model Globe: every two turns, gain 2 Gold and Discover a Tier 6 minion.
        (
            "BG30_MagicItem_425",
            Script {
                start_of_turn: Some(|c, me| {
                    if c.tick_counter(me.side, me.card, 2) {
                        c.gain_gold(me.side, 2);
                        c.discover_minion(me.side, me.card, 3, |d| d.tier == 6);
                    }
                }),
                // "At the start of every two turns, gain 2 Gold and Discover a Tier 6 minion. (2
                // turns left!)"
                shown: Some(|c, sh| vec![2, 6, shown_left(c, sh, 2)]),
                ..E
            },
        ),
        // Colorful Compass: "Get a random 92": the 92 is a placeholder the game fills with a minion
        // type. The type is fixed when the trinket is taken: the player's most common type, else a
        // random type of the lobby.
        (
            "BG30_MagicItem_426",
            every_turn!(|c, me| {
                let r = compass_type(c, me.side, me.card);
                get_random(c, me.side, |d| d.races.intersects(r));
            }),
        ),
        (
            "BG30_MagicItem_426t",
            every_turn!(|c, me| {
                let r = compass_type(c, me.side, me.card);
                for _ in 0..2 {
                    get_random(c, me.side, |d| d.races.intersects(r));
                }
            }),
        ),
        (
            "BG30_MagicItem_430",
            every_turn!(|c, me| get_random(c, me.side, |d| d.mech.has(Mech::BATTLECRY))),
        ),
        // Goldenizer Supply: at the end of every 3 turns, get a Goldenizer.
        (
            "BG30_MagicItem_435",
            Script {
                end_of_turn: Some(|c, me| {
                    if c.turn_cycle(me.side, me.card, |c| c.tick_counter(me.side, me.card, 3)) {
                        give_n(c, me.side, "BG26_813t", 1);
                    }
                }),
                // "At the end of every 3 turns, get a Goldenizer. (3 turns left!)"
                shown: Some(|c, sh| vec![3, shown_left(c, sh, 3)]),
                ..E
            },
        ),
        // The Goldenizer: make a friendly minion Golden.
        (
            "BG26_813t",
            Script {
                spell: Some(|c, _, t| {
                    if let Some(m) = c.find_any_mut(t) {
                        super::spells::make_golden(m);
                    }
                }),
                spell_target: TargetSpec::FriendlyNonGoldenMaxTier(7),
                ..E
            },
        ),
        // Reflective Pendant: a plain copy of a random friendly minion; again each turn.
        (
            "BG30_MagicItem_706",
            every_turn!(|c, me| {
                let f = c.friendly(me.side);
                if let Some(&e) = c.g.rng.pick(&f)
                    && let Some(card) = c.minion(e).map(|m| db().normal_of(m.card))
                {
                    c.give_card(me.side, card);
                }
            }),
        ),
        // Tickatus Sticker: Discover a Tier 3 Darkmoon Prize; again every three turns.
        (
            "BG30_MagicItem_707",
            Script {
                on_attach: Some(|c, me| super::darkmoon::discover_prize(c, me.side, me.card, 3)),
                start_of_turn: Some(|c, me| {
                    if c.tick_counter(me.side, me.card, 3) {
                        super::darkmoon::discover_prize(c, me.side, me.card, 3);
                    }
                }),
                // "Discover a Tier 3 Darkmoon Prize. At the start of every three turns, repeat
                // this. (3 turns left!)"
                shown: Some(|c, sh| vec![3, shown_left(c, sh, 3)]),
                ..E
            },
        ),
        (
            "BG30_MagicItem_713",
            Script {
                on_event: Some(|c, me, e| deaths_then(c, me, e, 8, Races::UNDEAD)),
                listens: ev::DIED,
                // "After 8 friendly minions die, get a random Undead. (8 left!)"
                shown: Some(|c, sh| vec![8, shown_left(c, sh, 8)]),
                ..E
            },
        ),
        (
            "BG30_MagicItem_931",
            Script {
                on_event: Some(|c, me, e| deaths_then(c, me, e, 7, Races::BEAST)),
                listens: ev::DIED,
                // "After 7 friendly minions die, get a random Beast. (7 left!)"
                shown: Some(|c, sh| vec![7, shown_left(c, sh, 7)]),
                ..E
            },
        ),
        // Mecha-Jaraxxus Sticker: 2 random Magnetic Mecha-Demons; 2 more each turn.
        (
            "BG30_MagicItem_942",
            every_turn!(|c, me| give_random_of(c, me.side, &MECHA_DEMONS, 2)),
        ),
        ("BG25_807t", E),
        ("BG25_807t2", E),
        ("BG25_807t3", E),
        ("BG25_807t4", E),
        // Lava Lamp: after you sell 6 minions, get a random Elemental.
        (
            "BG30_MagicItem_951",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Sold { side, .. } if *side == me.side)
                        && c.tick_counter(me.side, me.card, 6)
                    {
                        get_random(c, me.side, |d| d.races.intersects(Races::ELEMENTAL));
                    }
                }),
                listens: ev::SOLD,
                // "After you sell 6 minions, get a random Elemental. (6 left!)"
                shown: Some(|c, sh| vec![6, shown_left(c, sh, 6)]),
                ..E
            },
        ),
        (
            "BG30_MagicItem_993",
            every_turn!(|c, me| {
                if let Some(x) = c.random_minion(|d| d.tier == 7) {
                    c.give_card(me.side, x);
                }
            }),
        ),
        (
            "BG30_MagicItem_994",
            every_turn!(|c, me| super::buddies_b::yogg_spin(c, me.side)),
        ),
        (
            "BG31_MagicItem_903",
            every_turn!(|c, me| give_n(c, me.side, KNOCKOFF_WISDOMBALL, 1)),
        ),
        // Knockoff Wisdomball: your next 2 Refreshes are helpful (`content::helpful`).
        (
            KNOCKOFF_WISDOMBALL,
            Script {
                spell: Some(|c, me, _| {
                    let wb = cid(KNOCKOFF_WISDOMBALL);
                    let n = c.card_data(me.side, wb);
                    c.set_card_data(me.side, wb, n + 2);
                }),
                ..E
            },
        ),
        // Putricide Sticker: craft a custom Undead; again every 2 turns.
        (
            "BG32_MagicItem_300",
            Script {
                on_attach: Some(|c, me| super::heroes_special::putricide_craft(c, me.side)),
                start_of_turn: Some(|c, me| {
                    if c.tick_counter(me.side, me.card, 2) {
                        super::heroes_special::putricide_craft(c, me.side);
                    }
                }),
                // "Craft a custom Undead. At the start of every 2 turns, repeat this. (2 turns
                // left!)"
                shown: Some(|c, sh| vec![2, shown_left(c, sh, 2)]),
                ..E
            },
        ),
        // Splinter of Aurum: when you have 15 Gold, a random Golden Tier 5 minion (once).
        // Checked when you get it, at turn start, after spells, sales and plays, and whenever
        // Gold comes in (`aurum_gold_check`).
        (
            "BG32_MagicItem_350",
            Script {
                on_attach: Some(aurum_check),
                start_of_turn: Some(aurum_check),
                on_event: Some(|c, me, _| aurum_check(c, me)),
                listens: ev::SPELL_CAST | ev::SOLD | ev::CARD_PLAYED,
                ..E
            },
        ),
        // Portable Factory: Discover a Tier 4 (5) minion with a type; a copy of it each turn
        // (the pick is stored by `after_discover`).
        (
            "BG32_MagicItem_361",
            Script {
                on_attach: Some(|c, me| {
                    c.discover_minion(me.side, me.card, 3, |d| d.tier == 4 && !d.races.is_none())
                }),
                start_of_turn: Some(factory_copy),
                ..E
            },
        ),
        (
            "BG32_MagicItem_361t",
            Script {
                on_attach: Some(|c, me| {
                    c.discover_minion(me.side, me.card, 3, |d| d.tier == 5 && !d.races.is_none())
                }),
                start_of_turn: Some(factory_copy),
                ..E
            },
        ),
        // Curator Sticker: a Golden Mishmash and a 10/10 Venomous Amalgam.
        (
            "BG32_MagicItem_807",
            Script {
                on_attach: Some(|c, me| {
                    give_n(c, me.side, "TB_BaconShop_HERO_33_Buddy_G", 1);
                    let mut m = c.new_minion(me.side, cid("TB_BaconShop_HP_033t"));
                    m.set_base_stats(10, 10);
                    m.give(Kw::VENOMOUS);
                    c.add_to_hand(me.side, Item::Minion(m));
                }),
                ..E
            },
        ),
        // Rendle Sticker: steal the highest-Tier card from the Tavern; again each end of turn.
        (
            "BG32_MagicItem_817",
            Script {
                on_attach: Some(|c, me| steal_highest(c, me.side)),
                end_of_turn: Some(|c, me| steal_highest(c, me.side)),
                ..E
            },
        ),
        (
            "BG35_MagicItem_301",
            every_turn!(|c, me| {
                get_random(c, me.side, |d| {
                    d.races.intersects(Races::MECH) && d.mech.has(Mech::MAGNETIC)
                })
            }),
        ),
        (
            "BG35_MagicItem_302",
            Script {
                on_event: Some(|c, me, e| deaths_then(c, me, e, 6, Races::MECH)),
                listens: ev::DIED,
                // "After 6 friendly minions die, get a random Mech. (6 left!)"
                shown: Some(|c, sh| vec![6, shown_left(c, sh, 6)]),
                ..E
            },
        ),
        // Wildfeather Duster: after you summon 6 Beasts, get a random Beast.
        (
            "BG35_MagicItem_700",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Summoned { side, eid } = e
                        && *side == me.side
                        && c.minion(*eid).is_some_and(|m| m.is(Races::BEAST))
                        && c.tick_counter(me.side, me.card, 6)
                    {
                        get_random(c, me.side, |d| d.races.intersects(Races::BEAST));
                    }
                }),
                listens: ev::SUMMONED,
                // "After you summon 6 Beasts, get a random Beast. (6 left!)"
                shown: Some(|c, sh| vec![6, shown_left(c, sh, 6)]),
                ..E
            },
        ),
        // Lens Case: a Duplicating Lens; again every 2 turns.
        (
            "BG35_MagicItem_817",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG35_MagicItem_817t", 1)),
                start_of_turn: Some(|c, me| {
                    if c.tick_counter(me.side, me.card, 2) {
                        give_n(c, me.side, "BG35_MagicItem_817t", 1);
                    }
                }),
                // "Get a Duplicating Lens. At the start of every 2 turns, repeat this. (2 turns
                // left!)"
                shown: Some(|c, sh| vec![2, shown_left(c, sh, 2)]),
                ..E
            },
        ),
        // Duplicating Lens: choose a minion from Tier 3 or below; get a plain copy of it.
        (
            "BG35_MagicItem_817t",
            Script {
                spell: Some(|c, me, t| {
                    if let Some(card) = c.find_any(t).map(|m| db().normal_of(m.card)) {
                        c.give_card(me.side, card);
                    }
                }),
                spell_target: TargetSpec::AnyMaxTier(3),
                ..E
            },
        ),
        // Safety Patch: cast Ice Block (a Secret; its spell puts it into play) and gain 5 Gold.
        // "Cast" with no "this casts": your spell - it counts, "after you cast" hears it, and
        // extra-cast effects repeat it.
        (
            "BG35_MagicItem_820",
            Script {
                on_attach: Some(|c, me| {
                    c.you_cast(me.side, cid("TB_Bacon_Secrets_12"), 0, 0, 0);
                    c.gain_gold(me.side, 5);
                }),
                ..E
            },
        ),
        // Pocket Cyclone: cast Easterly Winds (greater: four times); again each turn (twice).
        (
            "BG35_MagicItem_850",
            every_turn!(|c, me| c.you_cast(me.side, cid("BG34_444"), 0, 0, 0)),
        ),
        (
            "BG35_MagicItem_850t",
            Script {
                on_attach: Some(|c, me| {
                    for _ in 0..4 {
                        c.you_cast(me.side, cid("BG34_444"), 0, 0, 0);
                    }
                }),
                start_of_turn: Some(|c, me| {
                    for _ in 0..2 {
                        c.you_cast(me.side, cid("BG34_444"), 0, 0, 0);
                    }
                }),
                ..E
            },
        ),
        // Water Wheel: after you play an Elemental, get a random Tavern spell (twice a turn).
        (
            "BG35_MagicItem_851",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Played { side, played, .. } = e
                        && *side == me.side
                        && played.races.intersects(Races::ELEMENTAL)
                        && c.turn_data(me.side, me.card) < 2
                    {
                        c.add_turn_data(me.side, me.card, 1);
                        get_random_spell(c, me.side);
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Ominous Stone: Discover a Tier 4 minion of your most common type with a Dark Gift.
        (
            "BG36_MagicItem_206",
            Script {
                on_attach: Some(|c, me| {
                    let r = most_common(c, me.side);
                    let cands =
                        pool_where(c, |d| d.tier == 4 && (r.is_none() || d.races.intersects(r)));
                    discover_gifted(c, me.side, me.card, cands, false);
                }),
                ..E
            },
        ),
        // Wax Lance: Discover a Tier 7 minion with a Dark Gift.
        (
            "BG36_MagicItem_309",
            Script {
                on_attach: Some(|c, me| {
                    let cands = pool_where(c, |d| d.tier == 7);
                    discover_gifted(c, me.side, me.card, cands, false);
                }),
                ..E
            },
        ),
        // Maldraxxus Dagger: Discover a plain copy of a friendly minion with a Dark Gift;
        // repeat at the start of every 2 turns (36.6.3; every turn before).
        (
            "BG36_MagicItem_370",
            Script {
                on_attach: Some(maldraxxus_dagger),
                start_of_turn: Some(|c, me| {
                    if c.tick_counter(me.side, me.card, 2) {
                        maldraxxus_dagger(c, me);
                    }
                }),
                // "Discover a plain copy of a friendly minion with a Dark Gift. Repeat at the start
                // of every 2 turns. (2 turns left!)"
                shown: Some(|c, sh| vec![2, shown_left(c, sh, 2)]),
                ..E
            },
        ),
        (
            "BG36_MagicItem_303",
            every_turn!(|c, me| get_choose_one(c, me.side, 1)),
        ),
        (
            "BG36_MagicItem_303t",
            every_turn!(|c, me| get_choose_one(c, me.side, 2)),
        ),
        // Willbreaker Sticker: 2 random Tavern spells as a linked pair; again each turn.
        (
            "BG36_MagicItem_852",
            every_turn!(|c, me| {
                let tier = c.tier(me.side);
                let a = c.random_spell(|d| d.tier <= tier);
                let b = c.random_spell(|d| d.tier <= tier);
                if let (Some(a), Some(b)) = (a, b) {
                    crate::recruit::give_linked_pair(c, me.side, a, b);
                }
            }),
        ),
        // Cookie's Stirring Rod: after you play 5 Murlocs, get two random Tavern spells.
        (
            "BG36_MagicItem_850",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Played { side, played, .. } = e
                        && *side == me.side
                        && played.races.intersects(Races::MURLOC)
                        && c.tick_counter(me.side, me.card, 5)
                    {
                        get_random_spell(c, me.side);
                        get_random_spell(c, me.side);
                    }
                }),
                listens: ev::PLAYED,
                // "After you play 5 Murlocs, get two random Tavern spells. (5 left!)"
                shown: Some(|c, sh| vec![5, shown_left(c, sh, 5)]),
                ..E
            },
        ),
        // Secret Schematic: after you buy a Mech, get a random Tavern spell.
        (
            "BG36_MagicItem_840",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Bought { side, card, .. } = e
                        && *side == me.side
                        && db().get(*card).races.intersects(Races::MECH)
                        && db().get(*card).kind == crate::cards::CardKind::Minion
                    {
                        get_random_spell(c, me.side);
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Fridge Magnet: Avenge (3): get a random Magnetic minion.
        (
            "BG30_MagicItem_545",
            Script {
                avenge: Some((3, 3, |c, me| {
                    get_random(c, me.side, |d| d.mech.has(Mech::MAGNETIC))
                })),
                ..E
            },
        ),
        // The Eye of Dalaran: whenever a friendly minion with no type dies, get a random
        // Tavern spell.
        (
            "BG30_MagicItem_981",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Died { side, races, .. } if *side == me.side && races.is_none())
                    {
                        get_random_spell(c, me.side);
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Sinister Invitation: Discover a Golden minion of your most common type with a Dark Gift
        // (no Triple Reward). 36.6.3 "offers higher-Tier minions more frequently": the minion's
        // Tier follows the Dark Gift tiers of the turn.
        (
            "BG36_MagicItem_610",
            Script {
                on_attach: Some(|c, me| {
                    let r = most_common(c, me.side);
                    let tiers = c.g.cfg.dark_gift_tiers(c.g.turn);
                    let cands = pool_where(c, |d| {
                        tiers.contains(&d.tier) && (r.is_none() || d.races.intersects(r))
                    });
                    discover_gifted(c, me.side, me.card, cands, true);
                }),
                ..E
            },
        ),
        // Sinstone Sticker: the first 2 Discovers each turn give an extra copy.
        (
            "BG30_MagicItem_801",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Discovered { side, hid, .. } = e
                        && *side == me.side
                        && c.turn_data(me.side, me.card) < 2
                    {
                        c.add_turn_data(me.side, me.card, 1);
                        // A full copy of the Discovered card; of one waiting for hand space too
                        // (the copy waits as well).
                        c.give_copy_of_hand(me.side, *hid);
                    }
                }),
                listens: ev::DISCOVERED,
                // "The first 2 times you Discover a card each turn, get an extra copy of it. (2
                // left!)"
                shown: Some(|c, sh| vec![2, shown_turn_left(c, sh, 2)]),
                ..E
            },
        ),
        // Deathly Phylactery: Discover a Deathrattle minion (tier <= yours); the first
        // Deathrattle each combat triggers an extra time (`run_deathrattles`).
        (
            "BG30_MagicItem_700",
            Script {
                on_attach: Some(|c, me| {
                    let tier = c.tier(me.side);
                    c.discover_minion(me.side, me.card, 3, |d| {
                        d.tier <= tier && d.mech.has(Mech::DEATHRATTLE)
                    });
                }),
                ..E
            },
        ),
    ]
}

/// Portable Factory: a copy of the stored pick (`card_data` = card id).
fn factory_copy(c: &mut Ctx, me: Me) {
    let v = c.card_data(me.side, me.card);
    if v > 0 {
        c.give_card(me.side, CardId(v as u16));
    }
}
