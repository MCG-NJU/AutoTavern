//! Tests for the Timewarp cards of `content/tw_spells.rs`.

mod common;
#[allow(unused_imports)]
use common::*;

use taverngym::cards::CardDef;
use taverngym::types::Mech;

/// A lobby minion (normal form) matching `f`.
fn lobby_minion(g: &Game, f: impl Fn(&CardDef) -> bool) -> &'static str {
    let c = g.universe.iter().find(|c| f(db().get(**c))).unwrap();
    db().get(*c).id.as_str()
}

/// Give `id` to player 0 and cast it (untargeted).
fn cast(g: &mut Game, id: &str) {
    give(g, 0, id);
    play_last(g, 0);
}

/// Give `id` to player 0 and cast it on the friendly minion at `idx`.
fn cast_at(g: &mut Game, id: &str, idx: usize) {
    give(g, 0, id);
    cast_on(g, 0, idx);
}

/// The Discover on top of player 0's choices: (source, options).
fn discover(g: &Game) -> (CardId, Vec<Item>) {
    match g.players[0].pending.last() {
        Some(Pending::Discover { source, options }) => (*source, options.clone()),
        x => panic!("expected a Discover, got {x:?}"),
    }
}

fn hand_count(g: &Game, id: &str) -> usize {
    let c = cid(id);
    g.players[0]
        .hand
        .iter()
        .filter(|h| h.item.card() == c)
        .count()
}

fn has_effect(g: &Game, id: &str) -> bool {
    g.players[0].effects.iter().any(|x| x.card == cid(id))
}

#[test]
fn conch_and_cloning_device_summon_exact_copies() {
    let mut g = game(1);
    let murloc = db()
        .iter()
        .find(|d| d.is_minion() && d.pools.minion && d.races == Races::MURLOC && d.tier == 1)
        .unwrap()
        .id
        .as_str();
    let other = db()
        .iter()
        .find(|d| d.is_minion() && d.pools.minion && d.races == Races::BEAST && d.tier == 1)
        .unwrap()
        .id
        .as_str();
    set_board(&mut g, 0, &[murloc, other]);
    g.players[0].board[0].add_stats(3, 4);
    g.players[0].board[0].give(Kw::TAUNT);
    let orig = g.players[0].board[0].clone();
    // Conch: only the Murloc can be chosen.
    give(&mut g, 0, "BG34_Treasure_951");
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
        panic!("expected a target choice")
    };
    assert_eq!(options, &vec![orig.eid]);
    g.apply(0, Action::Choose(0)).unwrap();
    let b = &g.players[0].board;
    assert_eq!(b.len(), 3);
    assert_eq!(
        (b[1].card, b[1].atk, b[1].hp),
        (orig.card, orig.atk, orig.hp)
    );
    assert!(b[1].has(Kw::TAUNT) && b[1].eid != orig.eid);
    // Cloning Device: any friendly minion; the copy stands to its right.
    cast_at(&mut g, "BG34_Treasure_302", 2);
    let b = &g.players[0].board;
    assert_eq!(b.len(), 4);
    assert_eq!(b[3].card, b[2].card);
    assert_eq!(b[3].card, cid(other));
}

#[test]
fn ring_lasso_and_evolving_tavern_repeat_each_turn() {
    let mut g = game(2);
    for id in [
        "BG34_Treasure_608",
        "BG34_Treasure_609",
        "BG34_Treasure_900",
    ] {
        cast(&mut g, id);
    }
    let gets = ["BG28_168", "BG28_512", "BGS_Treasures_006"];
    for id in gets {
        assert_eq!(hand_count(&g, id), 1);
    }
    end_turn_all(&mut g);
    for id in gets {
        assert_eq!(hand_count(&g, id), 2);
    }
    end_turn_all(&mut g);
    for id in gets {
        assert_eq!(hand_count(&g, id), 3);
    }
}

#[test]
fn big_winner_repeats_every_three_turns() {
    let mut g = game(3);
    let bw = cid("BG34_Treasure_606");
    cast(&mut g, "BG34_Treasure_606");
    let (source, options) = discover(&g);
    assert_eq!(source, bw);
    assert!(!options.is_empty());
    assert!(options.iter().all(|o| db().get(o.card()).prize_tier == 3));
    g.apply(0, Action::Choose(0)).unwrap();
    let offered = |g: &Game| {
        g.players[0]
            .pending
            .iter()
            .any(|x| matches!(x, Pending::Discover { source, .. } if *source == bw))
    };
    // Cast on turn 1: again on turns 4 and 7.
    for turn in 2..=7 {
        end_turn_all(&mut g);
        assert_eq!(g.turn, turn);
        assert_eq!(offered(&g), turn == 4 || turn == 7, "turn {turn}");
    }
}

#[test]
fn trainee_strike_oil_and_armor_stash() {
    let mut g = game(4);
    // Trainee: one minion each from Tiers 1, 2 and 3.
    cast(&mut g, "BG34_Treasure_607");
    let tiers: Vec<u8> = g.players[0]
        .hand
        .iter()
        .map(|h| h.item.as_minion().unwrap().tier)
        .collect();
    assert_eq!(tiers, vec![1, 2, 3]);
    g.players[0].hand.clear();
    // Strike Oil: +4 maximum Gold and 4 Gold now.
    let gold = g.players[0].gold;
    cast(&mut g, "BG34_Treasure_932");
    assert_eq!(g.players[0].gold, gold + 4);
    assert_eq!(g.players[0].max_gold_bonus, 4);
    // Armor Stash is cast when bought in the Timewarped Tavern.
    let armor = g.players[0].armor;
    g.players[0].pending.push(Pending::Timewarp {
        options: vec![(Item::spell(cid("BG34_Treasure_934")), 1)],
        chronum: 2,
        major: true,
    });
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].armor, armor + 10);
    assert!(g.players[0].hand.is_empty());
    assert!(g.players[0].pending.is_empty());
}

#[test]
fn rat_in_a_cage_and_bananas() {
    let mut g = game(5);
    let id = lobby_minion(&g, |d| d.tier == 1);
    set_board(&mut g, 0, &[id]);
    let (a, h) = stats(&g, 0)[0];
    // +2/+2, then doubled.
    cast_at(&mut g, "BG34_Treasure_905", 0);
    let (a, h) = ((a + 2) * 2, (h + 2) * 2);
    assert_eq!(stats(&g, 0)[0], (a, h));
    // B.A.N.A.N.A.S.: a hand of Bananas, and Tavern spells give +3/+3 more.
    cast(&mut g, "BG34_Treasure_912");
    assert_eq!(hand_count(&g, "BG28_897"), g.cfg.hand_size);
    assert_eq!(
        (g.players[0].stats.spell_atk, g.players[0].stats.spell_hp),
        (3, 3)
    );
    cast_on(&mut g, 0, 0);
    assert_eq!(stats(&g, 0)[0], (a + 5, h + 5));
    // The doubling itself gets no extra: (+2 +3), then doubled.
    g.players[0].hand.clear();
    cast_at(&mut g, "BG34_Treasure_905", 0);
    assert_eq!(stats(&g, 0)[0], ((a + 10) * 2, (h + 10) * 2));
}

/// (attack, health) each Tavern minion has above its card.
fn tavern_gains(g: &Game) -> Vec<(i32, i32)> {
    g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| (m.atk - m.def().atk, m.hp - m.def().hp))
        .collect()
}

#[test]
fn apples_cast_them_apples_after_each_refresh() {
    let mut g = game(6);
    cast(&mut g, "BG34_Treasure_620");
    assert!(tavern_gains(&g).iter().all(|x| *x == (0, 0)));
    g.players[0].gold = 10;
    g.apply(0, Action::Refresh).unwrap();
    let gains = tavern_gains(&g);
    assert!(!gains.is_empty());
    assert!(gains.iter().all(|x| *x == (1, 2)), "{gains:?}");
    // "Cast Them Apples" has no "this casts": it is a spell you cast, like Apples itself.
    assert_eq!(g.players[0].stats.spells_cast, 2);
}

#[test]
fn new_recruit_buffs_the_tavern_and_keeps_seven_cards() {
    let mut g = game(7);
    assert!(g.players[0].shop.len() < 7);
    cast(&mut g, "BG34_Treasure_917");
    assert_eq!(g.players[0].shop.len(), 7);
    assert!(tavern_gains(&g).iter().all(|x| *x == (2, 2)));
    g.players[0].gold = 10;
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(g.players[0].shop.len(), 7);
    assert!(tavern_gains(&g).iter().all(|x| *x == (2, 2)));
    end_turn_all(&mut g);
    assert_eq!(g.players[0].shop.len(), 7);
    assert!(tavern_gains(&g).iter().all(|x| *x == (2, 2)));
}

#[test]
fn tier7_discovers() {
    let mut g = game(8);
    // Secrecy: a Golden Tier 7 minion.
    cast(&mut g, "BG34_Treasure_625");
    let (_, options) = discover(&g);
    assert_eq!(options.len(), 3);
    for o in &options {
        let m = o.as_minion().unwrap();
        assert!(m.golden && db().get(db().normal_of(m.card)).pools.tier7);
    }
    g.apply(0, Action::Choose(0)).unwrap();
    let m = g.players[0].hand[0].item.as_minion().unwrap();
    assert!(m.golden && m.triple_reward);
    g.players[0].hand.clear();
    // Ritual: two Tier 7 Discovers.
    cast(&mut g, "BG34_Treasure_919");
    assert_eq!(g.players[0].pending.len(), 2);
    for _ in 0..2 {
        let (_, options) = discover(&g);
        assert!(options.iter().all(|o| db().get(o.card()).pools.tier7));
        g.apply(0, Action::Choose(0)).unwrap();
    }
    assert_eq!(g.players[0].hand.len(), 2);
}

#[test]
fn on_the_house_corpse_and_revelation_discovers() {
    let mut g = game(9);
    g.players[0].tier = 4;
    // On the House: three Discovers of your Tier.
    cast(&mut g, "BG34_Treasure_903");
    assert_eq!(g.players[0].pending.len(), 3);
    for _ in 0..3 {
        let (_, options) = discover(&g);
        assert!(options.iter().all(|o| o.as_minion().unwrap().tier == 4));
        g.apply(0, Action::Choose(0)).unwrap();
    }
    assert_eq!(g.players[0].hand.len(), 3);
    g.players[0].hand.clear();
    // Corpse: a Deathrattle minion of Tier 5-6 (the Tavern is not Tier 7), with Reborn.
    cast(&mut g, "BG34_Treasure_937");
    let (_, options) = discover(&g);
    assert!(!options.is_empty());
    for o in &options {
        let m = o.as_minion().unwrap();
        assert!(m.def().mech.has(Mech::DEATHRATTLE) && (5..=6).contains(&m.tier));
        assert!(m.has(Kw::REBORN));
    }
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(
        g.players[0].hand[0]
            .item
            .as_minion()
            .unwrap()
            .has(Kw::REBORN)
    );
    g.players[0].hand.clear();
    // Revelation: a 1-Cost, then a 2-Cost minion of the Minor Timewarp.
    cast(&mut g, "BG34_Treasure_953");
    for cost in [1, 2] {
        let (_, options) = discover(&g);
        assert!(!options.is_empty());
        for o in &options {
            let d = db().get(o.card());
            assert!(d.is_minion() && d.pools.timewarp && d.tier == 3 && d.cost == cost);
        }
        g.apply(0, Action::Choose(0)).unwrap();
    }
    assert_eq!(g.players[0].hand.len(), 2);
}

#[test]
fn beanstalk_locks_its_pick() {
    let mut g = game(10);
    cast(&mut g, "BG34_Treasure_301");
    let (_, options) = discover(&g);
    assert!(!options.is_empty());
    for o in &options {
        let d = db().get(o.card());
        assert!(d.pools.timewarp && d.tier == 5 && d.cost == 1);
    }
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].hand.len(), 1);
    assert_eq!(g.players[0].hand[0].locked, 1);
    // Later cards are not locked.
    give(&mut g, 0, "BG34_Treasure_934");
    assert_eq!(g.players[0].hand[1].locked, 0);
    // The lock lasts one turn.
    end_turn_all(&mut g);
    assert_eq!(g.players[0].hand[0].locked, 0);
}

#[test]
fn evolution_transforms_into_the_pick_at_30_30() {
    let mut g = game(11);
    let id = lobby_minion(&g, |d| d.tier == 1);
    set_board(&mut g, 0, &[id, id]);
    let eid = g.players[0].board[1].eid;
    cast_at(&mut g, "BG34_Treasure_933", 1);
    let (_, options) = discover(&g);
    assert_eq!(options.len(), 3);
    assert!(options.iter().all(|o| o.as_minion().unwrap().tier == 6));
    let pick = options[1].card();
    g.apply(0, Action::Choose(1)).unwrap();
    let b = &g.players[0].board;
    assert_eq!(b.len(), 2);
    assert_eq!((b[1].eid, b[1].card), (eid, pick));
    assert_eq!((b[1].atk, b[1].hp), (30, 30));
    assert!(b[0].is_card(id));
    // The pick only stood for the choice.
    assert!(g.players[0].hand.is_empty());
    assert!(!has_effect(&g, "BG34_Treasure_933"));
}

#[test]
fn chefs_choice_and_goldenizer() {
    let mut g = game(12);
    let id = lobby_minion(&g, |d| {
        d.tier == 1 && d.golden.is_some() && !d.races.is_none() && d.races != Races::ALL
    });
    let races = db().get(cid(id)).races;
    set_board(&mut g, 0, &[id]);
    // Chef's Choice: one minion of its type from each of Tiers 4, 5 and 6.
    cast_at(&mut g, "BG34_Treasure_940", 0);
    let got: Vec<&Minion> = g.players[0]
        .hand
        .iter()
        .map(|h| h.item.as_minion().unwrap())
        .collect();
    assert_eq!(got.iter().map(|m| m.tier).collect::<Vec<_>>(), [4, 5, 6]);
    assert!(got.iter().all(|m| m.def().races.intersects(races)));
    g.players[0].hand.clear();
    // Goldenizer.
    cast_at(&mut g, "BG34_Treasure_955", 0);
    let m = &g.players[0].board[0];
    assert!(m.golden && m.triple_reward);
    assert_eq!(m.card, db().golden_of(cid(id)));
    assert_eq!((m.atk, m.hp), (m.def().atk, m.def().hp));
}
