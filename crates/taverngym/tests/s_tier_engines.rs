//! Regression tests for the engine cards of the six comps that are S tier this season
//! (36.6.3). Every assertion is what the card's text says (Golden numbers from the Golden
//! card's own text); a test marked `MISMATCH?` keeps the text's reading where the script seems
//! to do something else.

mod common;
use common::*;
use taverngym::state::ShopSlot;

const COIN: &str = "BG28_810"; // Tavern Coin: "Gain 1 Gold." (a Tavern spell without a target)
const BANANA: &str = "BG28_897"; // Tavern Dish Banana: "Give a minion +2/+2."
const TEA_SET: &str = "BG28_888"; // Misplaced Tea Set
const ABSORPTION: &str = "BG35_911"; // Arcane Absorption
const GUARD: &str = "BG36_345"; // Suspicious Prisonguard: 3/3, no type
const SKELETON: &str = "BG_ICC_026t"; // 1/1 Undead
const HAND: &str = "BG25_010t"; // Helping Hand: 2/1 Undead with Reborn
const CYCLONE: &str = "BGS_119"; // Crackling Cyclone: 2/1 Elemental
const WRATH_WEAVER: &str = "BGS_004"; // 1/3 Demon: a played Demon deals 1 damage to your hero
const BAT: &str = "BG36_200"; // Flittering Bat: 1/4 Beast
const CAMPER: &str = "BG33_886"; // Tusked Camper: 2/3 Quilboar
const RESCUE_BOT: &str = "BG36_854"; // 2/1 Mech
const SATELLITE: &str = "BG34_Giant_610t"; // 4/4 Magnetic Mech
const SURFACER: &str = "BG35_881"; // Leyline Surfacer
const TEA_GUEST: &str = "BG32_111"; // Nightmare Par-tea Guest
const AMALGAM: &str = "BG36_640"; // Gatekeeper Amalgam: 6/6, all types
const VOLUMIZERS: [&str; 3] = ["BG34_170t", "BG34_170t2", "BG34_170t3"];
const BOUNTIES: [&str; 5] = ["BG33_811", "BG33_812", "BG33_813", "BG33_814", "BG33_815"];

/// Replace player 0's Tavern with fresh minions (as in `complex_review.rs`).
fn shop(g: &mut Game, ids: &[&str]) {
    g.players[0].shop.clear();
    for id in ids {
        let m = minion(g, 0, id);
        g.players[0]
            .shop
            .push(ShopSlot::new(m.eid, Item::Minion(m)));
    }
}

/// Add Tavern spell `id` to player 0's Tavern; returns its slot.
fn shop_spell(g: &mut Game, id: &str) -> usize {
    let sid = 900_000 + g.players[0].shop.len() as u32;
    g.players[0]
        .shop
        .push(ShopSlot::new(sid, Item::spell(cid(id))));
    g.players[0].shop.len() - 1
}

/// (attack, health) of the minions in player 0's Tavern, left to right.
fn shop_stats(g: &Game) -> Vec<(i32, i32)> {
    g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| (m.atk, m.hp))
        .collect()
}

/// (attack, health) of the minions in player 0's hand, left to right.
fn hand_stats(g: &Game) -> Vec<(i32, i32)> {
    g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .map(|m| (m.atk, m.hp))
        .collect()
}

/// Copies of card `id` in player 0's hand.
fn in_hand(g: &Game, id: &str) -> usize {
    let c = cid(id);
    g.players[0]
        .hand
        .iter()
        .filter(|h| h.item.card() == c)
        .count()
}

/// Bounties in player 0's hand.
fn bounties_in_hand(g: &Game) -> usize {
    g.players[0]
        .hand
        .iter()
        .filter(|h| BOUNTIES.contains(&db().get(h.item.card()).id.as_str()))
        .count()
}

/// (Health, Armor) of player 0's hero.
fn life(g: &Game) -> (i32, i32) {
    (g.players[0].health, g.players[0].armor)
}

/// Give `id` to player 0 and cast it (no target).
fn cast(g: &mut Game, id: &str) {
    give(g, 0, id);
    play_last(g, 0);
}

/// Give `id` to player 0 and cast it on the friendly minion at `idx`.
fn cast_at(g: &mut Game, id: &str, idx: usize) {
    give(g, 0, id);
    cast_on(g, 0, idx);
}

/// Give `id` to player 0 and cast it on the Tavern minion in `slot`.
fn cast_at_tavern(g: &mut Game, id: &str, slot: usize) {
    give(g, 0, id);
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Play { hand, pos: 0 }).unwrap();
    let sid = g.players[0].shop[slot].sid;
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
        panic!("expected a target choice")
    };
    let i = options.iter().position(|e| *e == sid).unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
}

/// Kill player 0's minion at `idx` in a combat against player 1's warband.
fn kill(g: &mut Game, idx: usize) {
    let mut c = combat_ctx(g);
    let e = c.boards[0][idx].eid;
    c.destroy(e);
    c.resolve_deaths();
}

/// What each minion in player 0's Tavern has on top of a fresh copy of its own card:
/// (its types, extra attack, extra health).
fn tavern_bonuses(g: &mut Game) -> Vec<(Races, i32, i32)> {
    let offered: Vec<Minion> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion().cloned())
        .collect();
    offered
        .iter()
        .map(|m| {
            let card = db().normal_of(m.card);
            let fresh = g.with_recruit(0, |c| c.new_minion(0, card));
            (m.races, m.atk - fresh.atk, m.hp - fresh.hp)
        })
        .collect()
}

/// The fixture game of the first seed from `from` on whose lobby has minion type `race`.
fn game_with(race: Races, from: u64) -> Game {
    (from..from + 64)
        .map(game)
        .find(|g| g.lobby_races.intersects(race))
        .expect("no lobby with that minion type")
}

// ---------------------------------------------------------------------------- 1. Elementals

#[test]
fn living_azerite_grows_the_elementals_in_the_tavern() {
    // "Whenever you cast a Tavern spell, give Elementals in the Tavern +4/+3 this game."
    // Golden: "... +4/+3 this game twice."
    for (azerite, times) in [("BG28_707", 1), ("BG28_707_G", 2)] {
        let mut g = game(36_101);
        set_board(&mut g, 0, &[azerite]);
        shop(&mut g, &[CYCLONE, GUARD]);
        let own = stats(&g, 0);
        cast(&mut g, COIN);
        // The Elemental grows, the minion without a type does not.
        assert_eq!(
            shop_stats(&g),
            vec![(2 + 4 * times, 1 + 3 * times), (3, 3)],
            "{azerite}"
        );
        // "In the Tavern": not the Elemental on the board (the Azerite itself).
        assert_eq!(stats(&g, 0), own, "{azerite}");
        // Every Tavern spell adds it again.
        cast(&mut g, COIN);
        assert_eq!(
            shop_stats(&g),
            vec![(2 + 8 * times, 1 + 6 * times), (3, 3)],
            "{azerite}"
        );
    }
}

#[test]
fn living_azerite_bonus_is_on_elementals_that_come_later() {
    // "... this game": an Elemental that enters the Tavern after the cast has the +4/+3 too.
    let mut g = game_with(Races::ELEMENTAL, 36_110);
    g.players[0].gold = 30;
    set_board(&mut g, 0, &["BG28_707"]);
    cast(&mut g, COIN);
    let st = &g.players[0].stats;
    assert_eq!((st.tavern_elemental_atk, st.tavern_elemental_hp), (4, 3));
    let mut seen = false;
    for _ in 0..20 {
        g.apply(0, Action::Refresh).unwrap();
        for (races, a, h) in tavern_bonuses(&mut g) {
            let elemental = races.intersects(Races::ELEMENTAL);
            let want = if elemental { (4, 3) } else { (0, 0) };
            assert_eq!((a, h), want);
            seen |= elemental;
        }
        if seen {
            break;
        }
    }
    assert!(seen, "no Elemental was offered in 20 Refreshes");
}

#[test]
fn leyline_surfacer_battlecry_and_deathrattle_give_arcane_absorptions() {
    // "Battlecry and Deathrattle: Get an Arcane Absorption." Golden: "Get 2 Arcane
    // Absorptions."
    for (surfacer, n) in [(SURFACER, 1), ("BG35_881_G", 2)] {
        let mut g = game(36_120);
        give(&mut g, 0, surfacer);
        play_last(&mut g, 0);
        assert_eq!(in_hand(&g, ABSORPTION), n, "{surfacer}: Battlecry");
        let mut g = game(36_121);
        set_board(&mut g, 0, &[surfacer]);
        kill(&mut g, 0);
        assert_eq!(in_hand(&g, ABSORPTION), n, "{surfacer}: Deathrattle");
    }
}

#[test]
fn arcane_absorption_gives_half_the_stats_of_the_healthiest_tavern_minion() {
    // "Give a friendly Elemental half the stats of the highest-Health minion in the Tavern."
    let mut g = game(36_122);
    set_board(&mut g, 0, &[CYCLONE, GUARD]);
    shop(&mut g, &["BG36_851", GUARD]); // Spark Snapper 6/8 (the highest Health), a 3/3
    cast_at(&mut g, ABSORPTION, 0);
    assert_eq!(stats(&g, 0), vec![(2 + 3, 1 + 4), (3, 3)]);
    // Nothing is taken from the Tavern.
    assert_eq!(shop_stats(&g), vec![(6, 8), (3, 3)]);
}

// ---------------------------------------------------------------------------- 2-3, 7. Demons

#[test]
fn devilish_distractor_spell_on_it_buffs_the_tavern_for_the_game() {
    // "Whenever you cast a spell on this, give minions in the Tavern +1/+2 this game."
    // Golden: "... +2/+4 this game."
    for (distractor, a, h) in [("BG36_762", 1, 2), ("BG36_762_G", 2, 4)] {
        let mut g = game(36_201);
        g.players[0].gold = 10;
        set_board(&mut g, 0, &[distractor, GUARD]);
        shop(&mut g, &[CYCLONE, GUARD]);
        // A spell cast on another minion does nothing for the Tavern.
        cast_at(&mut g, BANANA, 1);
        assert_eq!(shop_stats(&g), vec![(2, 1), (3, 3)], "{distractor}");
        // Cast on the Distractor: every minion in the Tavern.
        cast_at(&mut g, BANANA, 0);
        assert_eq!(
            shop_stats(&g),
            vec![(2 + a, 1 + h), (3 + a, 3 + h)],
            "{distractor}"
        );
        let st = &g.players[0].stats;
        assert_eq!((st.tavern_atk, st.tavern_hp), (a, h), "{distractor}");
        // "This game": the minions of the next Tavern have it too.
        g.apply(0, Action::Refresh).unwrap();
        let later = tavern_bonuses(&mut g);
        assert!(!later.is_empty());
        for (_, da, dh) in later {
            assert_eq!((da, dh), (a, h), "{distractor}");
        }
    }
}

#[test]
fn ashen_corruptor_rewinds_hero_damage_and_buffs_the_tavern() {
    // "After your hero takes damage, rewind it and give minions in the Tavern +2/+2 this
    // turn." Golden: "... +4/+4 this turn."
    for (corruptor, k) in [("BG32_873", 2), ("BG32_873_G", 4)] {
        let mut g = game(36_210);
        set_board(&mut g, 0, &[corruptor, WRATH_WEAVER]);
        shop(&mut g, &[CYCLONE, GUARD]);
        let before = life(&g);
        // A Demon is played: the Wrath Weaver deals 1 damage to the hero (and gains +2/+2,
        // which shows that the damage was dealt).
        give(&mut g, 0, WRATH_WEAVER);
        play_last(&mut g, 0);
        assert_eq!(stats(&g, 0)[1], (1 + 2, 3 + 2), "{corruptor}");
        assert_eq!(life(&g), before, "{corruptor}: the damage is rewound");
        assert_eq!(
            shop_stats(&g),
            vec![(2 + k, 1 + k), (3 + k, 3 + k)],
            "{corruptor}"
        );
    }
}

#[test]
fn ashen_corruptor_tavern_buff_is_gone_next_turn() {
    // "... +2/+2 this turn": a minion kept in the Tavern by a Freeze has lost it next turn.
    let mut g = game(36_211);
    set_board(&mut g, 0, &["BG32_873", WRATH_WEAVER]);
    shop(&mut g, &[CYCLONE, GUARD]);
    give(&mut g, 0, WRATH_WEAVER);
    play_last(&mut g, 0);
    assert_eq!(shop_stats(&g), vec![(2 + 2, 1 + 2), (3 + 2, 3 + 2)]);
    let kept: Vec<_> = g.players[0].shop.iter().map(|s| s.sid).collect();
    g.apply(0, Action::Freeze).unwrap();
    end_turn_all(&mut g);
    assert_eq!(g.turn, 2);
    let now: Vec<(i32, i32)> = kept
        .iter()
        .map(|sid| {
            let slot = g.players[0].shop.iter().find(|s| s.sid == *sid);
            let m = slot
                .and_then(|s| s.item.as_minion())
                .expect("a frozen minion stays in the Tavern");
            (m.atk, m.hp)
        })
        .collect();
    assert_eq!(now, vec![(2, 1), (3, 3)]);
}

#[test]
fn ashen_corruptor_rewinds_a_refresh_paid_with_health() {
    // Malchezaar: "Two Refreshes each turn cost Health instead of Gold. (2 left!)" -- with the
    // Corruptor the Health comes back. (The +2/+2 lasts the turn in the Tavern, so the new
    // Tavern has it: `ashen_corruptor_bonus_stays_in_the_tavern_for_the_turn` in mechanics.rs.)
    let mut g = game(36_212);
    g.players[0].gold = 0;
    g.players[0].free_refreshes = 0;
    set_board(&mut g, 0, &["BG26_524", "BG32_873"]);
    let before = life(&g);
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(g.players[0].board[0].counter, 1, "one Health Refresh used");
    assert_eq!(g.players[0].gold, 0);
    assert_eq!(life(&g), before, "the damage is rewound");
}

#[test]
fn felboar_consumes_a_tavern_minion_after_three_spells() {
    // "After you cast 3 spells, consume a minion in the Tavern to gain its stats. (3 left!)"
    // Golden: "... to gain double its stats."
    for (felboar, mult) in [("BG28_633", 1), ("BG28_633_G", 2)] {
        let mut g = game(36_220);
        set_board(&mut g, 0, &[felboar]);
        let mut want = stats(&g, 0)[0];
        // Twice, to see the count start again: (the minion in the Tavern, its stats).
        for (food, a, h) in [(CAMPER, 2, 3), (GUARD, 3, 3)] {
            shop(&mut g, &[food]);
            for cast_so_far in [1, 2] {
                cast(&mut g, COIN);
                assert_eq!(stats(&g, 0)[0], want, "{felboar}");
                assert_eq!(shop_stats(&g), vec![(a, h)], "{felboar}");
                assert_eq!(g.players[0].board[0].counter, cast_so_far, "{felboar}");
            }
            cast(&mut g, COIN);
            want = (want.0 + a * mult, want.1 + h * mult);
            assert_eq!(stats(&g, 0)[0], want, "{felboar}");
            assert!(
                shop_stats(&g).is_empty(),
                "{felboar}: the minion is consumed"
            );
            assert_eq!(g.players[0].board[0].counter, 0, "{felboar}: 3 left again");
        }
    }
}

// ---------------------------------------------------------------------------- 4, 12. Undead

#[test]
fn forsaken_weaver_gives_your_undead_attack_for_the_game() {
    // "After you cast a Tavern spell, your Undead have +3 Attack this game (wherever they are)."
    // Golden: "... +6 Attack..."
    for (weaver, k) in [("BG34_692", 3), ("BG34_692_G", 6)] {
        let mut g = game(36_301);
        set_board(&mut g, 0, &[weaver, SKELETON, GUARD]);
        give(&mut g, 0, SKELETON);
        give(&mut g, 0, GUARD);
        let before = stats(&g, 0);
        cast(&mut g, COIN);
        // On the board: the Undead (the Weaver is one), Attack only.
        assert_eq!(
            stats(&g, 0),
            vec![(before[0].0 + k, before[0].1), (1 + k, 1), (3, 3)],
            "{weaver}"
        );
        // In the hand.
        assert_eq!(hand_stats(&g), vec![(1 + k, 1), (3, 3)], "{weaver}");
        // "This game": the next cast adds to it, and an Undead that comes later has all of it.
        cast(&mut g, COIN);
        give(&mut g, 0, SKELETON);
        assert_eq!(
            hand_stats(&g),
            vec![(1 + 2 * k, 1), (3, 3), (1 + 2 * k, 1)],
            "{weaver}"
        );
        assert_eq!(g.players[0].stats.undead_atk, 2 * k, "{weaver}");
    }
}

#[test]
fn forsaken_weaver_bonus_is_on_an_undead_bought_afterwards() {
    let mut g = game(36_302);
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG34_692"]);
    shop(&mut g, &[SKELETON, GUARD]);
    cast(&mut g, COIN);
    g.apply(0, Action::Buy(0)).unwrap();
    g.apply(0, Action::Buy(0)).unwrap();
    assert_eq!(hand_stats(&g), vec![(1 + 3, 1), (3, 3)]);
}

#[test]
fn forsaken_weaver_reaches_undead_in_the_tavern() {
    // "(wherever they are)": an Undead that is in the Tavern when the spell is cast has the +3
    // Attack there, not only once it is bought (`Ctx::bump_player_stat` re-syncs the Tavern too).
    let mut g = game(36_303);
    set_board(&mut g, 0, &["BG34_692"]);
    shop(&mut g, &[SKELETON, GUARD]);
    cast(&mut g, COIN);
    assert_eq!(shop_stats(&g), vec![(1 + 3, 1), (3, 3)]);
}

#[test]
fn handless_forsaken_leaves_hands_with_reborn() {
    // "Deathrattle: Summon a 2/1 Hand with Reborn." Golden: "Summon two 2/1 Hands with
    // Reborn."
    for (forsaken, hands) in [("BG25_010", 1), ("BG25_010_G", 2)] {
        let mut g = game(36_310);
        set_board(&mut g, 0, &[forsaken, GUARD]);
        let mut c = combat_ctx(&mut g);
        let e = c.boards[0][0].eid;
        c.destroy(e);
        c.resolve_deaths();
        assert_eq!(c.boards[0].len(), hands + 1, "{forsaken}");
        for m in &c.boards[0][..hands] {
            assert_eq!((m.card, m.atk, m.hp), (cid(HAND), 2, 1), "{forsaken}");
            assert!(m.has(Kw::REBORN), "{forsaken}");
        }
        // A Hand dies: it comes back once, as a 2/1 without Reborn.
        let e = c.boards[0][0].eid;
        c.destroy(e);
        c.resolve_deaths();
        assert_eq!(c.boards[0].len(), hands + 1, "{forsaken}");
        let back = &c.boards[0][0];
        assert_eq!(
            (back.card, back.atk, back.hp),
            (cid(HAND), 2, 1),
            "{forsaken}"
        );
        assert!(!back.has(Kw::REBORN), "{forsaken}");
    }
}

#[test]
fn snazzy_phantom_gives_the_reborn_minions_attack_to_the_rightmost_undead() {
    // "After a friendly minion is Reborn, give stats equal to its Attack to your right-most
    // Undead." Golden: "... equal to double its Attack..."
    for (phantom, mult) in [("BG36_515", 1), ("BG36_515_G", 2)] {
        // (the minion that is Reborn, its Attack): an Undead and a minion without a type.
        for (reborn, atk) in [(HAND, 2), (GUARD, 3)] {
            let mut g = game(36_311);
            set_board(&mut g, 0, &[reborn, SKELETON, phantom, GUARD]);
            g.players[0].board[0].give(Kw::REBORN);
            let before = stats(&g, 0);
            let mut c = combat_ctx(&mut g);
            let e = c.boards[0][0].eid;
            c.destroy(e);
            c.resolve_deaths();
            let now: Vec<(i32, i32)> = c.boards[0].iter().map(|m| (m.atk, m.hp)).collect();
            assert_eq!(now.len(), 4, "{phantom} {reborn}");
            assert_eq!(now[0], (atk, 1), "{phantom} {reborn}: Reborn with 1 Health");
            // An Undead, but not the right-most one.
            assert_eq!(now[1], before[1], "{phantom} {reborn}");
            assert_eq!(
                now[2],
                (before[2].0 + atk * mult, before[2].1 + atk * mult),
                "{phantom} {reborn}"
            );
            // The right-most minion is not an Undead.
            assert_eq!(now[3], before[3], "{phantom} {reborn}");
        }
    }
}

// ---------------------------------------------------------------------------- 5-6. Tea Sets

#[test]
fn par_tea_guest_battlecry_and_deathrattle_give_tea_sets() {
    // "Battlecry and Deathrattle: Get a Misplaced Tea Set." Golden: "Get 2 Misplaced Tea
    // Sets."
    for (guest, n) in [(TEA_GUEST, 1), ("BG32_111_G", 2)] {
        let mut g = game(36_401);
        give(&mut g, 0, guest);
        play_last(&mut g, 0);
        assert_eq!(in_hand(&g, TEA_SET), n, "{guest}: Battlecry");
        let mut g = game(36_402);
        set_board(&mut g, 0, &[guest]);
        kill(&mut g, 0);
        assert_eq!(in_hand(&g, TEA_SET), n, "{guest}: Deathrattle");
    }
}

#[test]
fn misplaced_tea_set_buffs_one_minion_of_each_type() {
    // "Give a friendly minion of each type +4/+4."
    let mut g = game(36_403);
    set_board(&mut g, 0, &[BAT, BAT, WRATH_WEAVER, SKELETON, GUARD]);
    cast(&mut g, TEA_SET);
    let s = stats(&g, 0);
    // Two Beasts: one of them.
    let mut beasts = [s[0], s[1]];
    beasts.sort();
    assert_eq!(beasts, [(1, 4), (1 + 4, 4 + 4)]);
    assert_eq!(s[2], (1 + 4, 3 + 4), "the Demon");
    assert_eq!(s[3], (1 + 4, 1 + 4), "the Undead");
    assert_eq!(s[4], (3, 3), "no type");
}

#[test]
fn gatekeeper_amalgam_casts_a_tea_set_when_a_spell_is_cast_on_it() {
    // "Whenever you cast a spell on this, it casts Misplaced Tea Set." Golden: "... twice."
    for (amalgam, sets) in [(AMALGAM, 1), ("BG36_640_G", 2)] {
        let mut g = game(36_410);
        set_board(&mut g, 0, &[amalgam, WRATH_WEAVER, GUARD]);
        let before = stats(&g, 0);
        // A spell on another minion: its +2/+2 and nothing else.
        cast_at(&mut g, BANANA, 2);
        assert_eq!(
            stats(&g, 0),
            vec![before[0], before[1], (3 + 2, 3 + 2)],
            "{amalgam}"
        );
        // A spell on the Amalgam: its +2/+2, then the Tea Set(s): +4/+4 each for the Amalgam
        // (a minion of every type) and the Demon, nothing for the minion without a type.
        cast_at(&mut g, BANANA, 0);
        assert_eq!(
            stats(&g, 0),
            vec![
                (before[0].0 + 2 + 4 * sets, before[0].1 + 2 + 4 * sets),
                (1 + 4 * sets, 3 + 4 * sets),
                (3 + 2, 3 + 2),
            ],
            "{amalgam}"
        );
        // The minion casts it: no Tea Set card is involved.
        assert_eq!(in_hand(&g, TEA_SET), 0, "{amalgam}");
    }
}

#[test]
fn balinda_casts_spells_on_friendly_minions_twice() {
    // "Your spells that target friendly minions cast twice." Golden: "... cast three times."
    for (balinda, times) in [("BG35_883", 2), ("BG35_883_G", 3)] {
        let mut g = game(36_420);
        set_board(&mut g, 0, &[GUARD, balinda]);
        shop(&mut g, &[GUARD]);
        cast_at(&mut g, BANANA, 0);
        assert_eq!(stats(&g, 0)[0], (3 + 2 * times, 3 + 2 * times), "{balinda}");
        // A spell without a target is cast once.
        let gold = g.players[0].gold;
        cast(&mut g, COIN);
        assert_eq!(g.players[0].gold, gold + 1, "{balinda}");
        // A minion in the Tavern is not a friendly minion: once.
        cast_at_tavern(&mut g, BANANA, 0);
        assert_eq!(shop_stats(&g), vec![(3 + 2, 3 + 2)], "{balinda}");
    }
}

#[test]
fn balinda_makes_the_amalgam_cast_its_tea_set_for_every_cast() {
    // The spell is cast twice (Golden Balinda: three times) on the Amalgam, so the Amalgam
    // casts a Tea Set each time.
    for (balinda, times) in [("BG35_883", 2), ("BG35_883_G", 3)] {
        let mut g = game(36_421);
        set_board(&mut g, 0, &[AMALGAM, WRATH_WEAVER, balinda]);
        let before = stats(&g, 0);
        cast_at(&mut g, BANANA, 0);
        assert_eq!(
            stats(&g, 0),
            vec![
                (6 + (2 + 4) * times, 6 + (2 + 4) * times),
                (1 + 4 * times, 3 + 4 * times),
                before[2], // Balinda has no type
            ],
            "{balinda}"
        );
    }
}

#[test]
fn balinda_doubles_the_spell_cast_on_the_distractor() {
    // Two casts on the Devilish Distractor: the Tavern gets its +1/+2 twice.
    let mut g = game(36_422);
    set_board(&mut g, 0, &["BG36_762", "BG35_883"]);
    shop(&mut g, &[GUARD]);
    cast_at(&mut g, BANANA, 0);
    assert_eq!(stats(&g, 0)[0], (4 + 4, 7 + 4));
    assert_eq!(shop_stats(&g), vec![(3 + 2, 3 + 4)]);
}

// ---------------------------------------------------------------------------- 8-10. Mechs

#[test]
fn auto_reveille_magnetizes_a_volumizer_after_three_purchases() {
    // "After you buy 3 cards, Magnetize a random Volumizer to this and get a copy of it.
    // (3 left!)" Golden: "... and get two copies of it."
    for (reveille, copies) in [("BG36_367", 1), ("BG36_367_G", 2)] {
        let mut g = game(36_501);
        g.players[0].gold = 30;
        set_board(&mut g, 0, &[reveille]);
        let base = stats(&g, 0)[0];
        shop(&mut g, &[GUARD, GUARD]);
        // "Cards": a Tavern spell counts like a minion.
        let coin = shop_spell(&mut g, COIN);
        g.apply(0, Action::Buy(coin)).unwrap();
        g.apply(0, Action::Buy(0)).unwrap();
        let m = &g.players[0].board[0];
        assert!(m.attached.is_empty(), "{reveille}: nothing after two");
        assert_eq!(m.counter, 2, "{reveille}");
        g.apply(0, Action::Buy(0)).unwrap();
        let m = &g.players[0].board[0];
        assert_eq!(m.attached.len(), 1, "{reveille}: one Magnetization");
        let vol = m.attached[0];
        assert!(VOLUMIZERS.iter().any(|v| cid(v) == vol), "{reveille}");
        // Magnetized: its stats are added to the Reveille.
        let d = db().get(vol);
        assert_eq!((m.atk, m.hp), (base.0 + d.atk, base.1 + d.hp), "{reveille}");
        assert_eq!(m.counter, 0, "{reveille}: 3 left again");
        // The copies of that Volumizer, next to the three cards bought.
        let hand = &g.players[0].hand;
        let got = hand.iter().filter(|h| h.item.card() == vol).count();
        assert_eq!((got, hand.len()), (copies, 3 + copies), "{reveille}");
    }
}

#[test]
fn spark_snapper_magnetizes_a_satellite_to_a_played_mech_and_improves() {
    // "Whenever you play a Mech, Magnetize a 2/3 Satellite to it and improve this." Golden: "... a
    // 4/6 Satellite..." (An improvement adds the printed amount again)
    for (snapper, k) in [("BG36_851", 1), ("BG36_851_G", 2)] {
        let mut g = game(36_510);
        set_board(&mut g, 0, &[snapper]);
        let own = stats(&g, 0)[0];
        // Not a Mech: nothing.
        give(&mut g, 0, GUARD);
        play_last(&mut g, 0);
        let m = &g.players[0].board[1];
        assert_eq!((m.atk, m.hp, m.attached.len()), (3, 3, 0), "{snapper}");
        // A Mech (2/1): the Satellite.
        give(&mut g, 0, RESCUE_BOT);
        play_last(&mut g, 0);
        let m = &g.players[0].board[2];
        assert_eq!((m.atk, m.hp), (2 + 2 * k, 1 + 3 * k), "{snapper}");
        assert_eq!(m.attached.len(), 1, "{snapper}");
        assert_eq!(db().get(m.attached[0]).name, "Satellite", "{snapper}");
        // Improved: the next Satellite is one printed amount larger.
        give(&mut g, 0, RESCUE_BOT);
        play_last(&mut g, 0);
        let m = &g.players[0].board[3];
        assert_eq!((m.atk, m.hp), (2 + 4 * k, 1 + 6 * k), "{snapper}");
        // The Snapper itself gets nothing.
        assert_eq!(stats(&g, 0)[0], own, "{snapper}");
    }
}

#[test]
fn utility_drone_buffs_per_magnetization_at_the_end_of_the_turn() {
    // "At the end of your turn, give your minions +4/+5 for each Magnetization they have." Golden:
    // "... +8/+10..."
    for (drone, a, h) in [("BG26_152", 4, 5), ("BG26_152_G", 8, 10)] {
        let mut g = game(36_520);
        set_board(&mut g, 0, &[drone, RESCUE_BOT, GUARD]);
        // Two Satellites (4/4) on the Rescue Bot, one on the Drone, none on the last minion.
        for board in [1, 1, 0] {
            give(&mut g, 0, SATELLITE);
            let hand = g.players[0].hand.len() - 1;
            g.apply(0, Action::Magnetize { hand, board }).unwrap();
        }
        let before = stats(&g, 0);
        assert_eq!(before[1], (2 + 8, 1 + 8), "{drone}");
        g.apply(0, Action::EndTurn).unwrap(); // the others are still in their turn
        let after = stats(&g, 0);
        assert_eq!(after[0], (before[0].0 + a, before[0].1 + h), "{drone}");
        assert_eq!(
            after[1],
            (before[1].0 + 2 * a, before[1].1 + 2 * h),
            "{drone}"
        );
        assert_eq!(after[2], before[2], "{drone}");
    }
}

// ---------------------------------------------------------------------------- 11. Pirates

#[test]
fn sky_admiral_rogers_gives_a_bounty_for_nine_gold_spent() {
    // "After you spend 9 Gold, get a random Bounty. (9 Gold left!)" Golden: "... get two
    // random Bounties."
    for (rogers, n) in [("BG33_823", 1), ("BG33_823_G", 2)] {
        let mut g = game(36_601);
        g.players[0].gold = 9;
        set_board(&mut g, 0, &[rogers]);
        shop(&mut g, &[GUARD, GUARD, GUARD]);
        for spent in [3, 6] {
            g.apply(0, Action::Buy(0)).unwrap();
            assert_eq!(g.players[0].gold, 9 - spent, "{rogers}");
            assert_eq!(bounties_in_hand(&g), 0, "{rogers}: {spent} Gold spent");
        }
        g.apply(0, Action::Buy(0)).unwrap();
        assert_eq!(g.players[0].gold, 0, "{rogers}");
        assert_eq!(bounties_in_hand(&g), n, "{rogers}");
        assert_eq!(g.players[0].hand.len(), 3 + n, "{rogers}");
        assert_eq!(g.players[0].board[0].counter, 0, "{rogers}: 9 left again");
    }
}

#[test]
fn proud_privateer_casts_bounties_twice() {
    // "Your Bounties cast twice." Golden: "Your Bounties cast three times."
    for (privateer, times) in [("BG33_825", 2), ("BG33_825_G", 3)] {
        let mut g = game(36_610);
        set_board(&mut g, 0, &[GUARD, privateer]);
        // Selfish Bounty: "Give your left-most minion +6/+6."
        cast(&mut g, "BG33_813");
        assert_eq!(
            stats(&g, 0)[0],
            (3 + 6 * times, 3 + 6 * times),
            "{privateer}"
        );
        // A Tavern spell that is not a Bounty is cast once.
        let gold = g.players[0].gold;
        cast(&mut g, COIN);
        assert_eq!(g.players[0].gold, gold + 1, "{privateer}");
    }
}

// ---------------------------------------------------------------------------- 13. Battlecries

#[test]
fn kelp_keeper_triggers_a_friendly_battlecry() {
    // "Activate (1): Trigger a friendly minion's Battlecry." Golden: "... Battlecry twice."
    for (keeper, times) in [("BG36_701", 1), ("BG36_701_G", 2)] {
        let mut g = game(36_701);
        g.players[0].gold = 10;
        set_board(&mut g, 0, &[keeper, SURFACER, GUARD]);
        g.apply(0, Action::Activate(0)).unwrap();
        let Some(Pending::Target { options, .. }) = g.players[0].pending.last().cloned() else {
            panic!("expected a target choice")
        };
        assert_eq!(
            options,
            vec![g.players[0].board[1].eid],
            "{keeper}: only the Battlecry minion"
        );
        g.apply(0, Action::Choose(0)).unwrap();
        assert_eq!(in_hand(&g, ABSORPTION), times, "{keeper}");
        assert_eq!(g.players[0].gold, 9, "{keeper}: Activate (1)");
    }
}

#[test]
fn brann_makes_battlecries_trigger_twice() {
    // "Your Battlecries trigger twice." Golden: "Your Battlecries trigger three times."
    for (brann, times) in [("BG_LOE_077", 2), ("TB_BaconUps_045", 3)] {
        // Played from the hand.
        let mut g = game(36_710);
        set_board(&mut g, 0, &[brann]);
        give(&mut g, 0, SURFACER);
        play_last(&mut g, 0);
        assert_eq!(in_hand(&g, ABSORPTION), times, "{brann}");
        give(&mut g, 0, TEA_GUEST);
        play_last(&mut g, 0);
        assert_eq!(in_hand(&g, TEA_SET), times, "{brann}");
        // Triggered by the Kelp Keeper.
        let mut g = game(36_711);
        g.players[0].gold = 10;
        set_board(&mut g, 0, &["BG36_701", TEA_GUEST, brann]);
        g.apply(0, Action::Activate(0)).unwrap();
        g.apply(0, Action::Choose(0)).unwrap();
        assert_eq!(in_hand(&g, TEA_SET), times, "{brann}: with the Kelp Keeper");
    }
}
