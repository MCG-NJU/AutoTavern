//! Live card texts (`Script::shown`) of `content/{minions, murlocs_p1, naga, pirates,
//! pirates_p1}.rs` once their state has changed (`tests/shown.rs` covers the fresh state).

mod common;
#[allow(unused_imports)]
use common::*;

use taverngym::debug::{DebugCmd, live_text, run_debug, view};
use taverngym::script::Shown;

const FIRE_BALLER: &str = "BG31_816";
const SNOW_BALLER: &str = "BG31_818";
const LOBSTER: &str = "BG36_202";
const HACKERFIN: &str = "BG31_148";
const STEW: &str = "BG32_337";
const MYCOLOGIST: &str = "BG33_891";
const APPRENTICE: &str = "BG33_890t";
const TRAILBLAZER: &str = "BG31_327";
const COURIER: &str = "BG26_810";
const ESCAPEE: &str = "BG36_523";
const ROGERS: &str = "BG33_823";
const BALLADIST: &str = "BG26_814";
const LOCKBOX: &str = "BG36_520t";
const HOOKTUSK: &str = "BG36_344";

const GOLEM: &str = "BG29_864t"; // 5/5, no text
const HUMONGOZZ: &str = "BG32_341"; // 5/5 Divine Shield: Tavern spells give an extra +1/+2
const CRATER_MINER: &str = "BG31_320"; // Choose One - 2 Blood Gems; or a Gem Day
const BANANA: &str = "BG28_897"; // Tavern spell, costs 1

fn minion_text(c: &Ctx, m: &Minion) -> Option<String> {
    let sh = Shown {
        side: 0,
        card: m.card,
        golden: m.golden,
        minion: Some(m),
        data: 0,
    };
    live_text(c, &sh).map(|t| t.0)
}

/// Live English text of the warband minion at `idx` (None = as printed).
fn board_text(g: &mut Game, idx: usize) -> Option<String> {
    g.with_recruit(0, |c| {
        let m = c.boards[0][idx].clone();
        minion_text(c, &m)
    })
}

/// Live English text of a fresh copy of minion `id`, as the Tavern would show it.
fn fresh_text(g: &mut Game, id: &str) -> Option<String> {
    g.with_recruit(0, |c| {
        let m = c.new_minion(0, cid(id));
        minion_text(c, &m)
    })
}

/// Live English text of the hand card at `idx`.
fn hand_text(g: &mut Game, idx: usize) -> Option<String> {
    let item = g.players[0].hand[idx].item.clone();
    g.with_recruit(0, |c| match &item {
        Item::Minion(m) => minion_text(c, m),
        Item::Spell { card, data } => {
            let sh = Shown {
                side: 0,
                card: *card,
                golden: false,
                minion: None,
                data: *data,
            };
            live_text(c, &sh).map(|t| t.0)
        }
    })
}

fn spend(g: &mut Game, gold: i32) {
    g.players[0].gold = 10;
    g.with_recruit(0, |c| c.spend_gold(0, gold));
}

#[test]
fn ballers_show_the_improved_amount() {
    let mut g = game(1);
    assert_eq!(fresh_text(&mut g, FIRE_BALLER), None, "as printed");
    set_board(&mut g, 0, &[FIRE_BALLER, SNOW_BALLER, GOLEM]);
    g.apply(0, Action::Sell(0)).unwrap(); // +1 Attack; future Ballers improve
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some("When you sell this, give your minions +2 Health. Improve your future Ballers.")
    );
    // The view carries it.
    let v = view(&g, 0);
    assert_eq!(
        v["me"]["board"][0]["text"],
        "When you sell this, give your minions +2 Health. Improve your future Ballers."
    );
    // The text is what the sale then gives.
    g.apply(0, Action::Sell(0)).unwrap();
    assert_eq!(stats(&g, 0), vec![(6, 7)]);
    assert_eq!(
        fresh_text(&mut g, "BG31_816_G").as_deref(),
        Some("When you sell this, give your minions +6 Attack. Improve your future Ballers.")
    );
}

#[test]
fn tasty_lobster_shows_the_improved_stats() {
    let mut g = game(2);
    set_board(&mut g, 0, &[LOBSTER]);
    let eid = g.players[0].board[0].eid;
    g.with_recruit(0, |c| {
        c.destroy(eid);
        c.resolve_deaths();
    });
    assert_eq!(g.players[0].stats.lobster_bonus, 1);
    assert_eq!(
        fresh_text(&mut g, LOBSTER).as_deref(),
        Some(
            "Deathrattle: Give a random friendly Beast +4/+2. Improve your future Tasty Lobsters."
        )
    );
    assert_eq!(
        fresh_text(&mut g, "BG36_202_G").as_deref(),
        Some(
            "Deathrattle: Give a random friendly Beast +8/+4. Improve your future Tasty Lobsters."
        )
    );
}

#[test]
fn hackerfin_counts_the_bonus_keywords_of_the_warband() {
    let mut g = game(3);
    set_board(&mut g, 0, &[GOLEM, GOLEM]);
    g.players[0].board[0].give(Kw::TAUNT);
    g.players[0].board[1].give(Kw::DIVINE_SHIELD);
    give(&mut g, 0, HACKERFIN);
    assert_eq!(
        hand_text(&mut g, 0).as_deref(),
        Some(
            "Battlecry: Give your other minions +9/+6. (Improved by each different Bonus \
             Keyword in your warband!)"
        )
    );
    assert_eq!(
        fresh_text(&mut g, "BG31_148_G").as_deref(),
        Some(
            "Battlecry: Give your other minions +18/+12. (Improved by each different Bonus \
             Keyword in your warband!)"
        )
    );
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0)[..2], [(14, 11), (14, 11)]);
}

#[test]
fn seafood_stew_shows_the_tavern_spell_bonus() {
    let mut g = game(4);
    give(&mut g, 0, STEW);
    assert_eq!(hand_text(&mut g, 0), None, "as printed");
    set_board(&mut g, 0, &[HUMONGOZZ]);
    assert_eq!(
        hand_text(&mut g, 0).as_deref(),
        Some("Give a minion +2/+3. Repeat for each different Bonus Keyword in your warband.")
    );
    // One Bonus Keyword (Divine Shield): +2/+3 twice.
    cast_on(&mut g, 0, 0);
    assert_eq!(stats(&g, 0), vec![(9, 11)]);
}

#[test]
fn mycologist_shows_the_uses_left_this_turn() {
    let mut g = game(5);
    set_board(&mut g, 0, &[MYCOLOGIST, "BG33_891_G"]);
    assert_eq!(board_text(&mut g, 0), None, "as printed");
    assert_eq!(board_text(&mut g, 1), None, "as printed");
    let buy_banana = |g: &mut Game| {
        run_debug(
            g,
            DebugCmd::AddSpell {
                p: 0,
                zone: "shop".into(),
                card: BANANA.into(),
                data: 0,
            },
        )
        .unwrap();
        g.players[0].gold = 10;
        let slot = g.players[0].shop.len() - 1;
        g.apply(0, Action::Buy(slot)).unwrap();
    };
    buy_banana(&mut g);
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some(
            "Once per turn, after you buy a Tavern spell, get a 1/1 Murloc and teach it that \
             spell. (0 left!)"
        )
    );
    assert_eq!(
        board_text(&mut g, 1).as_deref(),
        Some(
            "Twice per turn, after you buy a Tavern spell, get a 1/1 Murloc and teach it that \
             spell. (1 left!)"
        )
    );
    buy_banana(&mut g);
    assert_eq!(
        board_text(&mut g, 1).as_deref(),
        Some(
            "Twice per turn, after you buy a Tavern spell, get a 1/1 Murloc and teach it that \
             spell. (0 left!)"
        )
    );
    // Three uses in all: three Apprentices.
    let apprentice = cid(APPRENTICE);
    let n = g.players[0]
        .hand
        .iter()
        .filter(|h| h.item.card() == apprentice)
        .count();
    assert_eq!(n, 3);
}

#[test]
fn thorned_trailblazer_shows_the_uses_left_this_turn() {
    let mut g = game(6);
    set_board(&mut g, 0, &["BG31_327_G"]);
    assert_eq!(board_text(&mut g, 0), None, "as printed");
    give(&mut g, 0, CRATER_MINER);
    play_last(&mut g, 0);
    assert!(g.players[0].pending.is_empty(), "both effects combined");
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some("Two Choose One cards each turn have both effects combined. (1 left!)")
    );
    // The normal one has a single use.
    let mut g = game(6);
    set_board(&mut g, 0, &[TRAILBLAZER]);
    give(&mut g, 0, CRATER_MINER);
    play_last(&mut g, 0);
    assert!(g.players[0].pending.is_empty(), "both effects combined");
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some("One Choose One card each turn has both effects combined. (0 left!)")
    );
}

#[test]
fn gold_counters_show_the_gold_left() {
    let mut g = game(7);
    set_board(&mut g, 0, &[COURIER, "BG36_523_G", ROGERS]);
    // The Courier counts 5; its printed "(6 Gold left!)" is the old 6.
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some("Whenever you spend 5 Gold, give your Pirates +3/+1. (5 Gold left!)")
    );
    for i in 1..3 {
        assert_eq!(board_text(&mut g, i), None, "as printed");
    }
    spend(&mut g, 4);
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some("Whenever you spend 5 Gold, give your Pirates +3/+1. (1 Gold left!)")
    );
    assert_eq!(
        board_text(&mut g, 1).as_deref(),
        Some(
            "After you spend 6 Gold, get a Lockbox. If you already have one, it opens 2 turns \
             sooner instead. (2 Gold left!)"
        )
    );
    assert_eq!(
        board_text(&mut g, 2).as_deref(),
        Some("After you spend 9 Gold, get a random Bounty. (5 Gold left!)")
    );
    // 7 Gold spent: the 6-Gold Escapee fired and starts again from 1, the 5-Gold Courier
    // from 2.
    spend(&mut g, 3);
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some("Whenever you spend 5 Gold, give your Pirates +3/+1. (3 Gold left!)")
    );
    assert_eq!(
        board_text(&mut g, 1).as_deref(),
        Some(
            "After you spend 6 Gold, get a Lockbox. If you already have one, it opens 2 turns \
             sooner instead. (5 Gold left!)"
        )
    );
    assert_eq!(
        board_text(&mut g, 2).as_deref(),
        Some("After you spend 9 Gold, get a random Bounty. (2 Gold left!)")
    );
    // The normal Escapee prints "1 turns sooner".
    set_board(&mut g, 0, &[ESCAPEE]);
    spend(&mut g, 2);
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some(
            "After you spend 6 Gold, get a Lockbox. If you already have one, it opens 1 turns \
             sooner instead. (4 Gold left!)"
        )
    );
}

#[test]
fn lockbox_shows_the_turns_left() {
    let mut g = game(8);
    set_board(&mut g, 0, &["BG36_523_G"]);
    spend(&mut g, 6);
    let lockbox = cid(LOCKBOX);
    assert_eq!(g.players[0].hand[0].item.card(), lockbox);
    assert_eq!(hand_text(&mut g, 0), None, "as printed: 5 turns left");
    // The golden Escapee opens it 2 turns sooner.
    spend(&mut g, 6);
    assert_eq!(
        hand_text(&mut g, 0).as_deref(),
        Some(
            "Unplayable. In 5 turns, break this open and get a random Golden minion with a \
             type! (3 turns left!)"
        )
    );
    let v = view(&g, 0);
    assert_eq!(
        v["me"]["hand"][0]["text"],
        "Unplayable. In 5 turns, break this open and get a random Golden minion with a type! \
         (3 turns left!)"
    );
}

#[test]
fn balladist_shows_the_gold_spent_this_turn() {
    let mut g = game(9);
    spend(&mut g, 3);
    assert_eq!(
        fresh_text(&mut g, BALLADIST).as_deref(),
        Some("Battlecry: Give a Pirate +8 Health. (Improved by each Gold you spent this turn!)")
    );
    assert_eq!(
        fresh_text(&mut g, "BG26_814_G").as_deref(),
        Some(
            "Battlecry: Give a Pirate +8 Health twice. (Improved by each Gold you spent this \
             turn!)"
        )
    );
    // The Battlecry gives what the text says.
    set_board(&mut g, 0, &[ROGERS]);
    let hp = g.players[0].board[0].hp;
    give(&mut g, 0, BALLADIST);
    play_last(&mut g, 0);
    if let Some(Pending::Target { .. }) = g.players[0].pending.last() {
        g.apply(0, Action::Choose(0)).unwrap();
    }
    assert_eq!(g.players[0].board[0].hp, hp + 8);
}

#[test]
fn hooktusk_shows_the_golden_minions_played() {
    let mut g = game(10);
    set_board(&mut g, 0, &[HOOKTUSK, "BG36_344_G"]);
    assert_eq!(board_text(&mut g, 0), None, "as printed");
    for _ in 0..2 {
        give(&mut g, 0, "BG29_864_Gt"); // a Golden minion
        play_last(&mut g, 0);
    }
    assert_eq!(g.players[0].stats.golden_played, 2);
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some(
            "After you Discover a card, give your other Pirates +3/+3. (Improved by Golden \
             minions you played this game!)"
        )
    );
    assert_eq!(
        board_text(&mut g, 1).as_deref(),
        Some(
            "After you Discover a card, give your other Pirates +6/+6. (Improved by Golden \
             minions you played this game!)"
        )
    );
}
