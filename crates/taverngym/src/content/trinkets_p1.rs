//! Trinkets: Portraits. Most give a card on purchase (`on_attach`) and some change how that minion
//! works; those changes live in the minion's own script, gated on the trinket (`has_trinket`), or
//! in `trinket_on_create` for new instances.

use super::trinkets::shown_left;
use super::*;

/// Give each id once.
fn give_all(c: &mut Ctx, side: Side, ids: &[&str]) {
    for id in ids {
        c.give_card(side, cid(id));
    }
}

/// Start of Combat: give friendly `card` minions keyword `kw`.
fn soc_kw(c: &mut Ctx, side: Side, card: &str, kw: Kw) {
    for e in c.friendly_where(side, |m| m.is_card(card)) {
        c.give_kw(e, kw);
    }
}

fn died(me: Me, e: &Event) -> Option<(CardId, Races)> {
    match e {
        Event::Died {
            side, card, races, ..
        } if *side == me.side => Some((db().normal_of(*card), *races)),
        _ => None,
    }
}

/// "Get a X. At the start of each turn, get another."
macro_rules! every_turn {
    ($id:literal) => {
        Script {
            on_attach: Some(|c, me| give_n(c, me.side, $id, 1)),
            start_of_turn: Some(|c, me| give_n(c, me.side, $id, 1)),
            ..E
        }
    };
}

/// "Get a X." only.
macro_rules! get_one {
    ($($id:literal),+) => {
        Script {
            on_attach: Some(|c, me| give_all(c, me.side, &[$($id),+])),
            ..E
        }
    };
}

/// Adjust a minion for its owner's Portraits (Bronzebeard Portrait): a freshly created one,
/// and one that becomes theirs from the Tavern (`Ctx::add_to_hand`).
pub fn trinket_on_create(p: &crate::state::Player, m: &mut Minion) {
    if p.trinkets.is_empty() {
        return;
    }
    if m.is_card_id(crate::cardid!("BG_LOE_077"))
        && p.has_trinket_id(crate::cardid!("BG30_MagicItem_418"))
    {
        m.races = Races(m.races.0 | Races::MURLOC.0 | Races::DRAGON.0);
    }
}

/// Surprise Portrait: extra Elementals of Surprise appear -- the real rule is hidden; the
/// simulator uses 10% on buying an Elemental to get an extra one; on each Refresh 10% for a
/// whole page of them and 10% for the Tavern's Elementals to become them (one roll, so the
/// two never coincide). All are extra cards, not taken from the minion pool.
const SURPRISE_BUY_PCT: usize = 10;
const SURPRISE_PAGE_PCT: usize = 10;
const SURPRISE_SWAP_PCT: usize = 10;

/// Replace Tavern minions (all, or only Elementals) by Elementals of Surprise. Cards kept
/// by a Freeze and Surprises already there are left alone.
fn surprise_tavern(c: &mut Ctx, side: Side, only_elementals: bool) {
    let Some(p) = c.owner[side] else { return };
    let card = cid("BG26_175");
    for i in 0..c.g.players[p].shop.len() {
        let slot = &c.g.players[p].shop[i];
        let Some(old) = slot.item.as_minion() else {
            continue;
        };
        if slot.frozen
            || slot.kept
            || old.card == card
            || (only_elementals && !old.races.intersects(Races::ELEMENTAL))
        {
            continue;
        }
        let back = old.pool_entries();
        for (card, copies) in back {
            c.g.pool.put(card, copies);
        }
        let mut m = c.new_minion(side, card);
        c.apply_tavern_buffs(side, &mut m);
        c.g.players[p].shop[i] = crate::state::ShopSlot::new(m.eid, Item::Minion(m));
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Eternal Portrait: get an Eternal Knight; SoC your Eternal Knights get Taunt and Reborn.
        (
            "BG30_MagicItem_301",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG25_008", 1)),
                start_of_combat: Some(|c, me| {
                    soc_kw(c, me.side, "BG25_008", Kw::TAUNT);
                    soc_kw(c, me.side, "BG25_008", Kw::REBORN);
                }),
                ..E
            },
        ),
        // Rivendare Portrait: get a Titus Rivendare; SoC double your Tituses' Health.
        (
            "BG30_MagicItem_310",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG25_354", 1)),
                start_of_combat: Some(|c, me| {
                    for e in c.friendly_where(me.side, |m| m.is_card("BG25_354")) {
                        let h = c.minion(e).map_or(0, |m| m.hp);
                        c.buff(e, 0, h);
                    }
                }),
                ..E
            },
        ),
        // Bronzebeard Portrait: get a Brann and a random Battlecry minion (tier <= yours);
        // your Branns are Murlocs and Dragons (`trinket_on_create` + existing ones here).
        (
            "BG30_MagicItem_418",
            Script {
                on_attach: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        let extra = Races(Races::MURLOC.0 | Races::DRAGON.0);
                        let hand = p.hand.iter_mut().filter_map(|h| h.item.as_minion_mut());
                        for m in p.board.iter_mut().chain(hand) {
                            if m.is_card("BG_LOE_077") {
                                m.races = Races(m.races.0 | extra.0);
                            }
                        }
                    }
                    for m in c.boards[me.side].iter_mut() {
                        if m.is_card("BG_LOE_077") {
                            m.races = Races(m.races.0 | Races::MURLOC.0 | Races::DRAGON.0);
                        }
                    }
                    give_n(c, me.side, "BG_LOE_077", 1);
                    let tier = c.tier(me.side);
                    if let Some(x) =
                        c.random_minion(|d| d.mech.has(Mech::BATTLECRY) && d.tier <= tier)
                    {
                        c.give_card(me.side, x);
                    }
                }),
                ..E
            },
        ),
        // Azerite Portrait: get a Living Azerite (its extra effect: `elementals.rs`).
        ("BG30_MagicItem_431", get_one!("BG28_707")),
        // Surprise Portrait: get an Elemental of Surprise; more appear in the Tavern.
        (
            "BG30_MagicItem_555",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG26_175", 1)),
                on_fill_shop: Some(|c, me| {
                    let roll = c.g.rng.below(100);
                    if roll < SURPRISE_PAGE_PCT {
                        surprise_tavern(c, me.side, false);
                    } else if roll < SURPRISE_PAGE_PCT + SURPRISE_SWAP_PCT {
                        surprise_tavern(c, me.side, true);
                    }
                }),
                // Buying or selling an Elemental (the sale of a Golden Elemental gave one; the
                // chance of a sale is the purchase's, a reading).
                on_event: Some(|c, me, e| {
                    let (Event::Bought { side, card, .. } | Event::Sold { side, card, .. }) = e
                    else {
                        return;
                    };
                    let d = db().get(*card);
                    if *side != me.side
                        || !d.is_minion()
                        || !d.races.intersects(Races::ELEMENTAL)
                        || c.g.rng.below(100) >= SURPRISE_BUY_PCT
                    {
                        return;
                    }
                    let m = c.new_minion(me.side, cid("BG26_175"));
                    c.add_to_hand(me.side, Item::Minion(m));
                }),
                listens: ev::BOUGHT | ev::SOLD,
                ..E
            },
        ),
        ("BG30_MagicItem_821", get_one!("TB_BaconShop_HP_105t")),
        // Rewinder Portrait: get a Soul Rewinder and a Wrath Weaver (Attack: `demons_p1.rs`).
        ("BG30_MagicItem_868", get_one!("BG26_174", "BGS_004")),
        ("BG30_MagicItem_876", get_one!("BG_EX1_564")),
        // Surveyor Portrait: get a Hot-Air Surveyor (+6/+6 gems: `Ctx::gem_stats`).
        ("BG30_MagicItem_943", get_one!("BG30_121")),
        ("BG32_MagicItem_172", every_turn!("BG31_176")),
        // Maw Caster Portrait: get a Maw Caster; whenever you destroy a minion outside combat,
        // gain 2 Gold.
        (
            "BG32_MagicItem_205",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG32_340", 1)),
                on_event: Some(|c, me, e| {
                    if !c.combat && died(me, e).is_some() {
                        c.gain_gold(me.side, 2);
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Czarina Portrait: get a Charging Czarina (Health: `mechs_p1.rs`).
        ("BG32_MagicItem_283", get_one!("BG28_741")),
        // Bassgill Portrait: get a Bassgill; after you summon a Murloc in combat, give it
        // Divine Shield.
        (
            "BG32_MagicItem_301",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG26_350", 1)),
                on_event: Some(|c, me, e| {
                    if let Event::Summoned { side, eid } = e
                        && *side == me.side
                        && c.combat
                        && c.minion(*eid).is_some_and(|m| m.is(Races::MURLOC))
                    {
                        c.give_kw(*eid, Kw::DIVINE_SHIELD);
                    }
                }),
                listens: ev::SUMMONED,
                ..E
            },
        ),
        ("BG32_MagicItem_364", get_one!("BG34_Giant_314")),
        ("BG32_MagicItem_831", every_turn!("BGS_115")),
        // Blessing Portrait: a Natural Blessing each turn (hand effect: `spells_p2.rs`).
        ("BG32_MagicItem_894", every_turn!("BG28_845")),
        // Hackerfin Portrait: get a Hackerfin; at the end of each turn, trigger your
        // Hackerfins' Battlecries.
        (
            "BG32_MagicItem_925",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG31_148", 1)),
                end_of_turn: Some(|c, me| {
                    for e in c.friendly_where(me.side, |m| m.is_card("BG31_148")) {
                        c.trigger_battlecry_ext(me.side, e);
                    }
                }),
                ..E
            },
        ),
        ("BG32_MagicItem_926", get_one!("BG27_513")),
        // Grifter Portrait: get a Doubloon Grifter; the first Pirate you buy each turn is free
        // (`Game::slot_price`).
        ("BG32_MagicItem_957", get_one!("BG31_826")),
        ("BG32_MagicItem_998", get_one!("BG31_360")),
        // Flaming Portrait: get a Flaming Enforcer (neighbors: `demons.rs`).
        ("BG35_MagicItem_156", get_one!("BG34_500")),
        // Conch Portrait: get a Cloning Conch; repeat at the start of every 2 turns.
        (
            "BG35_MagicItem_305",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG28_601", 1)),
                start_of_turn: Some(|c, me| {
                    if c.tick_counter(me.side, me.card, 2) {
                        give_n(c, me.side, "BG28_601", 1);
                    }
                }),
                // "Get a Cloning Conch. At the start of every 2 turns, repeat this. (2 turns
                // left!)"
                shown: Some(|c, sh| vec![2, shown_left(c, sh, 2)]),
                ..E
            },
        ),
        // Sky Golem Portrait: get a Falling Sky Golem; SoC give your minions "Deathrattle:
        // give your minions +2/+2 permanently" (the trinket is attached as that enchantment).
        (
            "BG35_MagicItem_740",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG35_342", 1)),
                start_of_combat: Some(|c, me| {
                    if me.eid != 0 {
                        return; // the attached copy
                    }
                    for e in c.friendly(me.side) {
                        if let Some(m) = c.minion_mut(e) {
                            m.attached.push(me.card);
                        }
                    }
                }),
                deathrattle: Some(|c, me| {
                    for e in c.friendly(me.side) {
                        c.buff_perm(me.side, e, 2, 2);
                    }
                }),
                ..E
            },
        ),
        ("BG35_MagicItem_741", get_one!("BG26_149")),
        // Accord-o-Tron Portrait: at the end of each turn, Magnetize an Accord-o-Tron to your
        // left- and right-most Mechs (once if they are the same).
        (
            "BG35_MagicItem_742",
            Script {
                end_of_turn: Some(|c, me| {
                    let mechs = c.friendly_where(me.side, |m| m.is(Races::MECH));
                    let mut hosts: Vec<EntityId> = mechs
                        .first()
                        .into_iter()
                        .chain(mechs.last())
                        .copied()
                        .collect();
                    hosts.dedup();
                    for h in hosts {
                        let m = c.new_minion(me.side, cid("BG26_147"));
                        crate::recruit::magnetize_onto(c, me.side, h, m);
                    }
                }),
                ..E
            },
        ),
        // Baller Portrait (greater): get a Temperature Shift; again after every 11 Elementals
        // you play.
        (
            "BG35_MagicItem_861",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG31_819", 1)),
                on_event: Some(|c, me, e| {
                    if let Event::Played { side, played, .. } = e
                        && *side == me.side
                        && played.races.intersects(Races::ELEMENTAL)
                        && c.tick_counter(me.side, me.card, 11)
                    {
                        give_n(c, me.side, "BG31_819", 1);
                    }
                }),
                listens: ev::PLAYED,
                // "Get a Temperature Shift. After you play 11 Elementals, repeat this. (11 left!)"
                shown: Some(|c, sh| vec![11, shown_left(c, sh, 11)]),
                ..E
            },
        ),
        // Lionfish Portrait: get a Lurking Lionfish; whenever a friendly Beast attacks, give it
        // +4/+2.
        (
            "BG36_MagicItem_201",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG36_201", 1)),
                on_event: Some(|c, me, e| {
                    if let Event::Attacks { side, attacker, .. } = e
                        && *side == me.side
                        && c.minion(*attacker).is_some_and(|m| m.is(Races::BEAST))
                    {
                        c.buff(*attacker, 4, 2);
                    }
                }),
                listens: ev::ATTACKS,
                ..E
            },
        ),
        // Plaguerunner Portrait: get a Plaguerunner; after you destroy one outside combat, get a
        // plain copy.
        (
            "BG36_MagicItem_204",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG34_690", 1)),
                on_event: Some(|c, me, e| {
                    if !c.combat && died(me, e).is_some_and(|d| d.0 == cid("BG34_690")) {
                        give_n(c, me.side, "BG34_690", 1);
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Eternal Portrait (greater): get a Golden Eternal Knight; after a friendly Undead dies,
        // your Eternal Knights have +4/+2 this game (`PlayerStats::knight_bonus`).
        (
            "BG36_MagicItem_216",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG25_008_G", 1)),
                on_event: Some(|c, me, e| {
                    if died(me, e).is_some_and(|d| d.1.intersects(Races::UNDEAD)) {
                        c.bump_player_stat(me.side, |s| s.knight_bonus += 1);
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Lockbox Portrait: get a Lockbox, or open yours 2 turns sooner; repeat each turn.
        (
            "BG36_MagicItem_301",
            Script {
                on_attach: Some(|c, me| lockbox_or_hasten(c, me.side, 2)),
                start_of_turn: Some(|c, me| lockbox_or_hasten(c, me.side, 2)),
                ..E
            },
        ),
        // Escapee Portrait: get a Golden Enterprising Escapee; your Lockboxes open 1 turn
        // sooner (new ones: `lockbox_or_hasten`; the ones you hold now: here).
        (
            ESCAPEE_PORTRAIT,
            Script {
                on_attach: Some(|c, me| {
                    give_n(c, me.side, "BG36_523_G", 1);
                    let lb = cid(LOCKBOX);
                    let mut open = 0;
                    if let Some(p) = c.player(me.side) {
                        for h in p.hand.iter_mut() {
                            if let Item::Spell { card, data } = &mut h.item
                                && *card == lb
                            {
                                *data -= 1;
                            }
                        }
                        let before = p.hand.len();
                        p.hand.retain(|h| {
                            !matches!(h.item, Item::Spell { card, data } if card == lb && data <= 0)
                        });
                        open = before - p.hand.len();
                    }
                    for _ in 0..open {
                        open_lockbox(c, me.side);
                    }
                }),
                ..E
            },
        ),
        // Baller Portrait (lesser): a Fire Baller or a Snow Baller each turn.
        (
            "BG36_MagicItem_390",
            Script {
                on_attach: Some(|c, me| give_random_of(c, me.side, &["BG31_816", "BG31_818"], 1)),
                start_of_turn: Some(|c, me| {
                    give_random_of(c, me.side, &["BG31_816", "BG31_818"], 1)
                }),
                ..E
            },
        ),
        (
            "BG36_MagicItem_400",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG36_303", 2)),
                ..E
            },
        ),
        // Mindslasher Portrait: get a Vicious Mindslasher (adjacent: `aberrations.rs`).
        ("BG36_MagicItem_402", get_one!("BG36_108")),
        // Sludge Portrait: get a Sludge Corrosion; after you discard a spell, get another
        // (36.6.3: spells only).
        (
            "BG36_MagicItem_430",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG36_301t", 1)),
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Discarded { side, card } if *side == me.side
                        && !db().get(*card).is_minion())
                    {
                        give_n(c, me.side, "BG36_301t", 1);
                    }
                }),
                listens: ev::DISCARDED,
                ..E
            },
        ),
        // Snarky Portrait: get 2 Snarky Sharks; whenever a Fishbait dies, give your Beasts
        // +4/+4 and improve this (by the base amount).
        (
            "BG36_MagicItem_450",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG36_206", 2)),
                on_event: Some(|c, me, e| {
                    let Event::Died { card, .. } = e else { return };
                    if db().normal_of(*card) != cid(super::beasts::FISHBAIT) {
                        return;
                    }
                    let k = 4 * (1 + c.card_level(me.side, me.card));
                    for b in c.friendly_where(me.side, |m| m.is(Races::BEAST)) {
                        c.buff(b, k, k);
                    }
                    c.improve_card(me.side, me.card);
                }),
                listens: ev::DIED,
                // "Get 2 Snarky Sharks. Whenever a Fishbait dies, give your Beasts +4/+4 and
                // improve this."
                shown: Some(|c, sh| {
                    let k = 4 * (1 + c.card_level(sh.side, sh.card));
                    vec![2, k, k]
                }),
                ..E
            },
        ),
        // Converter Portrait: get a Faceless Converter; SoC your Faceless Converters get Reborn.
        (
            "BG36_MagicItem_600",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG36_318", 1)),
                start_of_combat: Some(|c, me| soc_kw(c, me.side, "BG36_318", Kw::REBORN)),
                ..E
            },
        ),
        ("BG36_MagicItem_611", every_turn!("BG34_272")),
        // Flighty Portrait: get a Flighty Scout; after you cast a Tavern spell, give Flighty
        // Scouts in your hand and warband +3/+3.
        (
            "BG36_MagicItem_820",
            Script {
                on_attach: Some(|c, me| give_n(c, me.side, "BG32_330", 1)),
                on_event: Some(|c, me, e| {
                    if !matches!(e, Event::SpellCast { side, tavern: true, .. } if *side == me.side)
                    {
                        return;
                    }
                    let mut ids = c.friendly_where(me.side, |m| m.is_card("BG32_330"));
                    if let Some(p) = c.player_ref(me.side) {
                        ids.extend(p.hand.iter().filter_map(|h| {
                            h.item
                                .as_minion()
                                .filter(|m| m.is_card("BG32_330"))
                                .map(|_| h.hid)
                        }));
                    }
                    for x in ids {
                        c.buff(x, 3, 3);
                    }
                }),
                listens: ev::SPELL_CAST,
                ..E
            },
        ),
    ]
}
