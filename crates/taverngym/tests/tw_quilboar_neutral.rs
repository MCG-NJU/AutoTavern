//! Tests for the Timewarp cards of `content/tw_quilboar_neutral.rs`.

mod common;
#[allow(unused_imports)]
use common::*;

use taverngym::cards::CardKind;
use taverngym::types::Mech;

const NOMI: &str = "BG34_Giant_032";
const ELISE: &str = "BG34_Giant_038";
const WINNER: &str = "BG34_Giant_039";
const PIPER: &str = "BG34_Giant_069";
const BONKER: &str = "BG34_Giant_102";
const NALAA: &str = "BG34_Giant_205";
const GEOMANCER: &str = "BG34_Giant_305";
const JAZZER: &str = "BG34_Giant_306";
const TIDE_RAZOR: &str = "BG34_Giant_328";
const HENCHMAN: &str = "BG34_Giant_593";
const BOTANI: &str = "BG34_Giant_594";
const TIPPER: &str = "BG34_Giant_604";
const TRAVELER: &str = "BG34_Giant_605";
const LIL_QUILBOAR: &str = "BG34_Giant_608";
const GEMSPLITTER: &str = "BG34_Giant_644";
const ZERUS: &str = "BG34_Giant_671";
const LUCKY_EGG: &str = "BG34_Giant_683";
const BLOODBINDER: &str = "BG34_PreMadeChamp_076";
const BANDIT: &str = "BG34_PreMadeChamp_078";
const GEM: &str = "BG20_GEM";
const TRIPLE_REWARD: &str = "TB_BaconShop_Triples_01";

fn eid(g: &Game, p: PlayerId, i: usize) -> u32 {
    g.players[p].board[i].eid
}

fn hand_cards(g: &Game, p: PlayerId) -> Vec<CardId> {
    g.players[p].hand.iter().map(|h| h.item.card()).collect()
}

#[test]
fn nomi_buffs_the_tavern_this_game() {
    let mut g = game(1);
    set_board(&mut g, 0, &[NOMI, "BG34_Giant_032_G"]);
    // Any pool Elemental without a Battlecry.
    let elemental = db()
        .iter()
        .find(|d| {
            d.kind == CardKind::Minion
                && d.pools.minion
                && d.normal.is_none()
                && d.races.intersects(Races::ELEMENTAL)
                && !d.mech.has(Mech::BATTLECRY)
        })
        .unwrap();
    let before: Vec<(i32, i32)> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion().map(|m| (m.atk, m.hp)))
        .collect();
    assert!(!before.is_empty());
    give(&mut g, 0, &elemental.id);
    play_last(&mut g, 0);
    // Normal +4/+3, golden +8/+6.
    let after: Vec<(i32, i32)> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion().map(|m| (m.atk, m.hp)))
        .collect();
    for (b, a) in before.iter().zip(&after) {
        assert_eq!((a.0 - b.0, a.1 - b.1), (12, 9));
    }
    assert_eq!(g.players[0].stats.tavern_atk, 12);
    assert_eq!(g.players[0].stats.tavern_hp, 9);
    // A non-Elemental does nothing.
    give(&mut g, 0, JAZZER);
    play_last(&mut g, 0);
    assert_eq!(g.players[0].stats.tavern_atk, 12);
}

#[test]
fn elise_gilds_the_tavern_after_five_refreshes() {
    let goldens = |g: &Game| {
        g.players[0]
            .shop
            .iter()
            .filter(|s| s.item.as_minion().is_some_and(|m| m.golden))
            .count()
    };
    let mut g = game(2);
    set_board(&mut g, 0, &[ELISE]);
    g.players[0].gold = 20;
    for _ in 0..4 {
        g.apply(0, Action::Refresh).unwrap();
    }
    assert_eq!(goldens(&g), 0);
    assert_eq!(g.players[0].board[0].counter, 4);
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(goldens(&g), 1);
    assert_eq!(g.players[0].board[0].counter, 0);

    // Golden: the two highest-Tier minions.
    let mut g = game(2);
    set_board(&mut g, 0, &["BG34_Giant_038_G"]);
    g.players[0].gold = 20;
    for _ in 0..5 {
        g.apply(0, Action::Refresh).unwrap();
    }
    assert_eq!(goldens(&g), 2);
}

#[test]
fn winner_gets_a_triple_reward_when_it_survived() {
    let rewards = |g: &Game| {
        hand_cards(g, 0)
            .iter()
            .filter(|c| **c == cid(TRIPLE_REWARD))
            .count()
    };
    let mut g = game(3);
    give(&mut g, 0, WINNER);
    play_last(&mut g, 0);
    assert!(g.players[0].board[0].kw.has(Kw::STEALTH));
    // It fights an empty warband and survives.
    end_turn_all(&mut g);
    assert_eq!(rewards(&g), 1);
    // It dies in a combat: no reward at the next turn start.
    let w = eid(&g, 0, 0);
    {
        let mut c = combat_ctx(&mut g);
        c.destroy(w);
        c.resolve_deaths();
    }
    assert_eq!(g.players[0].board[0].counter, 1);
    end_turn_all(&mut g);
    assert_eq!(rewards(&g), 1);
    assert_eq!(g.players[0].board[0].counter, 0);
    // The golden gets two.
    let w = eid(&g, 0, 0);
    g.players[0].board[0] = minion(&mut g, 0, "BG34_Giant_039_G");
    g.players[0].board[0].eid = w;
    end_turn_all(&mut g);
    assert_eq!(rewards(&g), 3);
}

#[test]
fn piper_and_gemsplitter_improve_blood_gem_attack() {
    let mut g = game(4);
    set_board(&mut g, 0, &[PIPER, GEMSPLITTER, "BG34_Giant_644_G"]);
    let (piper, splitter) = (eid(&g, 0, 0), eid(&g, 0, 1));
    {
        let mut c = combat_ctx(&mut g);
        // Piper: 3 times per combat.
        for _ in 0..5 {
            c.damage(0, piper, 1);
        }
        assert_eq!(c.minion(piper).unwrap().hp, 3);
    }
    assert_eq!(g.players[0].stats.gem_atk, 3);
    assert_eq!(g.players[0].stats.gem_hp, 0);
    {
        let mut c = combat_ctx(&mut g);
        // A lost Divine Shield: +1 from the normal Gemsplitter, +2 from the golden one.
        c.damage(0, splitter, 1);
        assert!(!c.minion(splitter).unwrap().kw.has(Kw::DIVINE_SHIELD));
    }
    assert_eq!(g.players[0].stats.gem_atk, 6);
    // The bonus shows on the next Blood Gem.
    give(&mut g, 0, GEM);
    cast_on(&mut g, 0, 0);
    assert_eq!(stats(&g, 0)[0], (2 + 7, 8 + 1));
}

#[test]
fn bonker_rally_plays_permanent_gems_on_the_others() {
    let mut g = game(5);
    set_board(&mut g, 0, &[BONKER, JAZZER]);
    set_board(&mut g, 1, &[TIPPER]);
    let (bonker, target) = (eid(&g, 0, 0), eid(&g, 1, 0));
    {
        let mut c = combat_ctx(&mut g);
        c.trigger_rally(0, bonker, target);
        assert_eq!(c.minion(bonker).unwrap().atk, 7);
    }
    // Two permanent Blood Gems on the other minion, none on Bonker.
    assert_eq!(stats(&g, 0), vec![(7, 14), (7, 5)]);
    assert_eq!(g.players[0].board[1].gems, 2);
}

#[test]
fn nalaa_buffs_one_minion_of_each_type_on_a_spell() {
    let mut g = game(6);
    set_board(&mut g, 0, &[NALAA, PIPER, BONKER]);
    give(&mut g, 0, GEM);
    cast_on(&mut g, 0, 0);
    let s = stats(&g, 0);
    // Nalaa has no type: only the Blood Gem's +1/+1.
    assert_eq!(s[0], (13, 13));
    // Exactly one of the two Quilboar gets +4/+3.
    let piper = s[1] == (6, 11);
    let bonker = s[2] == (11, 17);
    assert!(piper != bonker, "{s:?}");
    assert!(piper || s[1] == (2, 8));
    assert!(bonker || s[2] == (7, 14));
}

#[test]
fn geomancer_and_jazzer_improve_blood_gems() {
    let mut g = game(7);
    set_board(
        &mut g,
        0,
        &[
            GEOMANCER,
            JAZZER,
            JAZZER,
            JAZZER,
            JAZZER,
            "BG34_Giant_306_G",
        ],
    );
    let jazzers: Vec<u32> = (1..6).map(|i| eid(&g, 0, i)).collect();
    {
        let mut c = combat_ctx(&mut g);
        for (i, j) in jazzers.iter().enumerate() {
            c.destroy(*j);
            c.resolve_deaths();
            // Avenge (5): only on the fifth death.
            let hand = c.g.players[0].hand.len();
            assert_eq!(hand, usize::from(i == 4));
        }
    }
    assert_eq!(hand_cards(&g, 0), vec![cid(GEM)]);
    // Jazzers: 4 x +2 Health and +4 from the golden; Geomancer: +1/+1.
    assert_eq!(g.players[0].stats.gem_atk, 1);
    assert_eq!(g.players[0].stats.gem_hp, 12 + 1);
}

#[test]
fn lil_quilboar_gems_your_quilboar() {
    let mut g = game(8);
    set_board(&mut g, 0, &[LIL_QUILBOAR, PIPER, NALAA]);
    let (lil, piper, nalaa) = (eid(&g, 0, 0), eid(&g, 0, 1), eid(&g, 0, 2));
    let mut c = combat_ctx(&mut g);
    c.destroy(lil);
    c.resolve_deaths();
    let m = c.minion(piper).unwrap();
    assert_eq!((m.atk, m.hp, m.gems), (5, 11, 3));
    let m = c.minion(nalaa).unwrap();
    assert_eq!((m.atk, m.hp), (12, 12));
    // Reborn comes back after the Deathrattle: no gems on it.
    assert_eq!(c.boards[0].len(), 3);
    assert_eq!((c.boards[0][0].atk, c.boards[0][0].hp), (5, 1));
}

#[test]
fn tide_razor_summons_and_gets_pirates() {
    // A lobby with Pirates.
    let mut g = (1..200)
        .map(game)
        .find(|g| g.lobby_races.intersects(Races::PIRATE))
        .unwrap();
    g.players[0].tier = 3;
    set_board(&mut g, 0, &[TIDE_RAZOR]);
    let razor = eid(&g, 0, 0);
    let summoned: Vec<CardId> = {
        let mut c = combat_ctx(&mut g);
        c.destroy(razor);
        c.resolve_deaths();
        c.boards[0].iter().map(|m| m.card).collect()
    };
    assert_eq!(summoned.len(), 4);
    for card in &summoned {
        let d = db().get(*card);
        assert!(
            d.races.intersects(Races::PIRATE) && d.tier <= 3,
            "{}",
            d.name
        );
    }
    // The same four Pirates are in hand.
    assert_eq!(hand_cards(&g, 0), summoned);
    // The warband itself is untouched by the combat.
    assert_eq!(g.players[0].board.len(), 1);
}

#[test]
fn henchman_copies_the_second_kill() {
    let mut g = game(9);
    set_board(&mut g, 0, &[HENCHMAN, "BG34_Giant_593_G"]);
    set_board(&mut g, 1, &[JAZZER, TIPPER, BOTANI]);
    let killer = eid(&g, 0, 0);
    let victims: Vec<u32> = (0..3).map(|i| eid(&g, 1, i)).collect();
    {
        let mut c = combat_ctx(&mut g);
        for v in victims {
            c.damage(killer, v, 100);
            c.resolve_deaths();
        }
    }
    // A plain copy of the second victim from the normal Henchman, two from the golden.
    assert_eq!(hand_cards(&g, 0), vec![cid(TIPPER); 3]);
    assert!(hand_cards(&g, 1).is_empty());
}

#[test]
fn botani_and_tipper_at_end_of_turn() {
    let mut g = game(10);
    set_board(&mut g, 0, &[BOTANI, TIPPER]);
    // Turn 1: 3 unspent Gold.
    end_turn_all(&mut g);
    assert_eq!(g.players[0].max_gold_bonus, 1);
    let hand = hand_cards(&g, 0);
    assert_eq!(hand.len(), 1);
    let d = db().get(hand[0]);
    assert!(d.kind == CardKind::Minion && d.tier == 1, "{}", d.name);
    // No unspent Gold: no extra maximum Gold; the golden Botani gives two minions.
    g.players[0].gold = 0;
    g.players[0].hand.clear();
    g.players[0].board[0] = minion(&mut g, 0, "BG34_Giant_594_G");
    end_turn_all(&mut g);
    assert_eq!(g.players[0].max_gold_bonus, 1);
    assert_eq!(g.players[0].hand.len(), 2);
}

#[test]
fn traveler_gets_a_minor_timewarp_card() {
    let mut g = game(11);
    set_board(&mut g, 0, &[TRAVELER, JAZZER, JAZZER, JAZZER, JAZZER]);
    let jazzers: Vec<u32> = (1..5).map(|i| eid(&g, 0, i)).collect();
    {
        let mut c = combat_ctx(&mut g);
        for (i, j) in jazzers.iter().enumerate() {
            c.destroy(*j);
            c.resolve_deaths();
            assert_eq!(c.g.players[0].hand.len(), usize::from(i == 3));
        }
    }
    let hand = hand_cards(&g, 0);
    assert_eq!(hand.len(), 1);
    let d = db().get(hand[0]);
    assert!(d.pools.timewarp && d.tier == 3 && d.cost == 1, "{}", d.name);
}

#[test]
fn zerus_transforms_and_keeps_its_stats() {
    let mut g = game(12);
    set_board(&mut g, 0, &[ZERUS]);
    g.players[0].board[0].atk = 9;
    let zerus = eid(&g, 0, 0);
    g.apply(0, Action::Activate(0)).unwrap();
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!("expected a choice")
    };
    let options: Vec<CardId> = options.iter().map(|o| o.card()).collect();
    assert_eq!(options.len(), 2);
    assert_ne!(options[0], options[1]);
    for o in &options {
        let d = db().get(*o);
        assert!(d.pools.timewarp && d.tier == 3 && d.kind == CardKind::Minion);
        assert!(!d.is_golden() && *o != cid(ZERUS));
    }
    g.apply(0, Action::Choose(1)).unwrap();
    assert!(g.players[0].pending.is_empty());
    assert!(g.players[0].hand.is_empty());
    let m = &g.players[0].board[0];
    assert_eq!((m.card, m.eid), (options[1], zerus));
    assert_eq!((m.atk, m.hp, m.max_hp), (9, 6, 6));

    // Golden Zerus with a full hand: the choice still opens (the pick never enters the hand).
    let mut g = game(12);
    set_board(&mut g, 0, &["BG34_Giant_671_G"]);
    for _ in 0..10 {
        give(&mut g, 0, GEM);
    }
    g.apply(0, Action::Activate(0)).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].pending.is_empty());
    assert_eq!(g.players[0].hand.len(), 10);
    let m = &g.players[0].board[0];
    let d = db().get(m.card);
    assert!(d.is_golden() && d.pools.timewarp && d.name != "Timewarped Zerus");
    assert_eq!((m.atk, m.hp), (12, 12));
}

#[test]
fn zerus_activates_once_per_turn() {
    let mut g = game(13);
    set_board(&mut g, 0, &[ZERUS, ZERUS]);
    assert!(g.can_activate(0, 0));
    g.apply(0, Action::Activate(0)).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    // The first one is another minion now; the second can still be used.
    assert_ne!(g.players[0].board[0].card, cid(ZERUS));
    assert_eq!(g.players[0].board[1].card, cid(ZERUS));
    assert!(g.can_activate(0, 1));
}

#[test]
fn lucky_egg_hatches_in_two_turns() {
    let mut g = game(14);
    set_board(&mut g, 0, &[LUCKY_EGG, LUCKY_EGG]);
    let eggs = [eid(&g, 0, 0), eid(&g, 0, 1)];
    end_turn_all(&mut g);
    assert!(g.players[0].pending.is_empty());
    assert_eq!(g.players[0].board[0].counter, 1);
    end_turn_all(&mut g);
    // Both Eggs offer three Golden Tier 7 minions.
    assert_eq!(g.players[0].pending.len(), 2);
    for (i, egg) in eggs.iter().enumerate() {
        let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
            panic!("expected a choice")
        };
        assert_eq!(options.len(), 3);
        for o in options {
            let d = db().get(o.card());
            assert!(d.is_golden() && d.tier == 7, "{}", d.name);
        }
        let pick = options[2].card();
        g.apply(0, Action::Choose(2)).unwrap();
        // The Eggs transform left to right.
        let m = &g.players[0].board[i];
        assert_eq!((m.card, m.eid), (pick, *egg));
        assert_eq!((m.atk, m.hp), (db().get(pick).atk, db().get(pick).hp));
        assert!(g.players[0].hand.is_empty());
    }
    assert!(g.players[0].pending.is_empty());
    assert!(
        g.players[0]
            .board
            .iter()
            .all(|m| m.golden && m.card != cid(LUCKY_EGG))
    );
}

#[test]
fn bloodbinder_gives_gems_and_bandit_discards_one() {
    let mut g = game(15);
    set_board(&mut g, 0, &[BLOODBINDER, BANDIT]);
    end_turn_all(&mut g);
    // Bloodbinder (left) gives 5 Blood Gems first; Bandit discards one of them and plays 4
    // Blood Gems on each minion.
    let hand = &g.players[0].hand;
    assert_eq!(hand.len(), 4);
    for h in hand {
        assert!(matches!(h.item, Item::Spell { card, data: 4 } if card == cid(GEM)));
    }
    assert_eq!(g.players[0].stats.cards_discarded, 1);
    assert_eq!(stats(&g, 0), vec![(16, 12), (11, 17)]);
    // The marked gems are still Blood Gems when played.
    cast_on(&mut g, 0, 0);
    assert_eq!(stats(&g, 0)[0], (17, 13));

    // No spell in hand: the Bandit does nothing.
    let mut g = game(15);
    set_board(&mut g, 0, &[BANDIT]);
    end_turn_all(&mut g);
    assert_eq!(stats(&g, 0), vec![(7, 13)]);
}
