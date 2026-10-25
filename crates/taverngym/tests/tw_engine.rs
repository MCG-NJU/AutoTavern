//! Timewarp cards whose effect lives in the engine (stat watchers, spell bonus, "your
//! Elementals give an extra", Blood Gems that count as Tavern spells).

mod common;
use common::*;

#[test]
fn lubber_adds_to_the_tavern_spell_bonus() {
    let mut g = game(1);
    assert_eq!(g.with_recruit(0, |c| c.spell_extra(0)), (0, 0));
    set_board(&mut g, 0, &["BG34_Giant_066"]);
    assert_eq!(g.with_recruit(0, |c| c.spell_extra(0)), (1, 1));
    set_board(&mut g, 0, &["BG34_Giant_066_G", "BG34_Giant_066"]);
    assert_eq!(g.with_recruit(0, |c| c.spell_extra(0)), (3, 3));
}

#[test]
fn whelp_smuggler_gives_health_for_attack_gains() {
    let mut g = game(2);
    set_board(&mut g, 0, &["BG34_Giant_064", "BG36_345"]);
    let (smuggler, other) = (g.players[0].board[0].eid, g.players[0].board[1].eid);
    g.with_recruit(0, |c| c.buff(other, 2, 0));
    assert_eq!(stats(&g, 0)[1], (3 + 2, 3 + 3));
    // Health alone doesn't trigger it; its own Attack gains do.
    g.with_recruit(0, |c| c.buff(other, 0, 5));
    assert_eq!(stats(&g, 0)[1], (5, 11));
    let before = stats(&g, 0)[0];
    g.with_recruit(0, |c| c.buff(smuggler, 1, 0));
    assert_eq!(stats(&g, 0)[0], (before.0 + 1, before.1 + 3));
    // Golden: +6 Health.
    set_board(&mut g, 0, &["BG34_Giant_064_G", "BG36_345"]);
    let other = g.players[0].board[1].eid;
    g.with_recruit(0, |c| c.buff(other, 1, 1));
    assert_eq!(stats(&g, 0)[1], (4, 4 + 6));
}

#[test]
fn astrogill_grows_in_hand_when_murlocs_gain_stats() {
    let mut g = game(3);
    set_board(&mut g, 0, &["BG32_330", "BG36_345"]); // a Murloc, a non-Murloc
    give(&mut g, 0, "BG34_Giant_801");
    let hand = |g: &Game| {
        let m = g.players[0].hand[0].item.as_minion().unwrap();
        (m.atk, m.hp)
    };
    let base = hand(&g);
    let (murloc, other) = (g.players[0].board[0].eid, g.players[0].board[1].eid);
    g.with_recruit(0, |c| c.buff(other, 2, 2));
    assert_eq!(hand(&g), base, "not a Murloc");
    g.with_recruit(0, |c| c.buff(murloc, 1, 1));
    assert_eq!(hand(&g), (base.0 + 3, base.1 + 2));
    // On the board it does nothing.
    play_last(&mut g, 0);
    let before = stats(&g, 0)[2];
    g.with_recruit(0, |c| c.buff(murloc, 1, 1));
    assert_eq!(stats(&g, 0)[2], before);
}

#[test]
fn swirler_makes_elementals_give_more() {
    let mut g = game(4);
    set_board(&mut g, 0, &["BG36_345"]);
    let target = g.players[0].board[0].eid;
    let elemental = cid("BG34_Giant_067"); // Timewarped Sellemental (an Elemental)
    let give_from = |g: &mut Game, src: CardId, eid: u32| {
        g.with_recruit(0, |c| {
            let prev = c.src.replace(src);
            c.buff(eid, 1, 1);
            c.src = prev;
        })
    };
    give_from(&mut g, elemental, target);
    assert_eq!(stats(&g, 0)[0], (4, 4), "no Swirler yet");
    give(&mut g, 0, "BG34_Giant_686");
    play_last(&mut g, 0);
    assert_eq!(g.players[0].stats.elem_give_atk, 3);
    give_from(&mut g, elemental, target);
    assert_eq!(stats(&g, 0)[0], (4 + 1 + 3, 4 + 1 + 3));
    // A non-Elemental source gets nothing extra; "this game" survives the Swirler leaving.
    give_from(&mut g, cid("BG36_345"), target);
    assert_eq!(stats(&g, 0)[0], (9, 9));
    g.players[0].board.truncate(1);
    give_from(&mut g, elemental, target);
    assert_eq!(stats(&g, 0)[0], (13, 13));
}

#[test]
fn bloodbinder_gems_count_as_tavern_spells() {
    let mut g = game(5);
    set_board(&mut g, 0, &["BG36_345"]);
    for data in [0, 4] {
        g.with_recruit(0, |c| {
            c.add_to_hand(
                0,
                Item::Spell {
                    card: cid("BG20_GEM"),
                    data,
                },
            );
        });
    }
    let cast = |g: &mut Game| {
        let before = g.players[0].stats.tavern_spells_cast;
        cast_on(g, 0, 0);
        g.players[0].stats.tavern_spells_cast - before
    };
    assert_eq!(
        cast(&mut g),
        1,
        "the Bloodbinder gem (data 4, last in hand)"
    );
    assert_eq!(cast(&mut g), 0, "a plain Blood Gem");
}

#[test]
fn sea_glass_doubles_twice_per_combat_and_scout_pays_tier_seven() {
    // Timewarped Sea Glass: Rally doubles its stats, twice per combat (golden: triples).
    for (id, mult) in [("BG34_Giant_110", 2), ("BG34_Giant_110_G", 3)] {
        let mut g = game(6);
        set_board(&mut g, 0, &[id]);
        let mut wall = minion(&mut g, 1, "BG36_345");
        wall.set_stats(0, 100_000);
        g.players[1].board = vec![wall];
        let (a, h) = stats(&g, 0)[0];
        let mut c = combat_ctx(&mut g);
        let e = c.boards[0][0].eid;
        for _ in 0..3 {
            taverngym::combat::attack_now(&mut c, 0, e);
        }
        let m = &c.boards[0][0];
        assert_eq!((m.atk, m.hp), (a * mult * mult, h * mult * mult), "{id}");
        drop(c);
        assert_eq!(stats(&g, 0)[0], (a, h), "combat only");
    }
    // Timewarped Scout: sold after two turns, 3 random Tier 7 minions (golden: 6).
    let mut g = game(7);
    set_board(&mut g, 0, &["BG34_Giant_333"]);
    g.players[0].board[0].counter = 2;
    g.players[0].hand.clear();
    g.apply(0, Action::Sell(0)).unwrap();
    assert_eq!(g.players[0].hand.len(), 3);
    for h in &g.players[0].hand {
        assert_eq!(db().get(h.item.card()).tier, 7);
    }
}

#[test]
fn a_killer_that_dies_in_the_exchange_still_gets_its_kill() {
    // Timewarped Pagle attacks, kills and dies: the Triple Reward is still given.
    let mut g = game(8);
    set_board(&mut g, 0, &["BG34_Giant_208"]);
    g.players[0].board[0].set_stats(5, 1);
    let mut enemy = minion(&mut g, 1, "BG36_345");
    enemy.set_stats(5, 1);
    g.players[1].board = vec![enemy];
    g.players[0].hand.clear();
    let mut c = combat_ctx(&mut g);
    let e = c.boards[0][0].eid;
    taverngym::combat::attack_now(&mut c, 0, e);
    assert!(
        c.boards[0].is_empty() && c.boards[1].is_empty(),
        "both died"
    );
    drop(c);
    let hand: Vec<_> = g.players[0].hand.iter().map(|h| h.item.card()).collect();
    assert_eq!(hand, vec![cid("TB_BaconShop_Triples_01")]);
}

#[test]
fn goldenizer_targets_only_minions_that_can_become_golden() {
    let mut g = game(9);
    set_board(&mut g, 0, &["BG36_345_G", "BG36_345"]);
    g.players[0].shop.clear();
    give(&mut g, 0, "BG34_Treasure_955");
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Play { hand, pos: 0 }).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
        panic!("expected a target choice")
    };
    assert_eq!(
        options,
        &vec![g.players[0].board[1].eid],
        "not the Golden one"
    );
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].board[1].golden);
}

#[test]
fn selling_the_last_winner_removes_its_watcher() {
    let mut g = game(10);
    give(&mut g, 0, "BG34_Giant_039");
    play_last(&mut g, 0);
    let winner = cid("BG34_Giant_039");
    assert!(g.players[0].effects.iter().any(|x| x.card == winner));
    g.apply(0, Action::Sell(0)).unwrap();
    assert!(!g.players[0].effects.iter().any(|x| x.card == winner));
}
