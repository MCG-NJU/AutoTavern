//! `Script::shown` of the trinkets of `trinkets_p5..p8`, the Quests and the Quest Rewards:
//! the text follows the state the scripts keep on the player.

mod common;
use common::*;

use taverngym::debug::{DebugCmd, live_text, run_debug, view};
use taverngym::event::Event;
use taverngym::script::Shown;

const PRISONGUARD: &str = "BG36_345"; // 3/3, Tier 1, no type
const SATELLITE: &str = "BG31_171t";

fn trinket(g: &mut Game, id: &str) -> CardId {
    let card = id.to_string();
    run_debug(g, DebugCmd::GiveTrinket { p: 0, card }).unwrap();
    g.players[0].pending.clear();
    cid(id)
}

fn reward(g: &mut Game, id: &str) -> CardId {
    let card = id.to_string();
    run_debug(g, DebugCmd::GiveReward { p: 0, card }).unwrap();
    g.players[0].pending.clear();
    cid(id)
}

/// English live text of the player-level card `card` (None = as printed).
fn shown_in(c: &Ctx, card: CardId) -> Option<String> {
    let sh = Shown {
        side: 0,
        card,
        golden: false,
        minion: None,
        data: 0,
    };
    live_text(c, &sh).map(|t| t.0)
}

fn shown(g: &mut Game, card: CardId) -> Option<String> {
    g.with_recruit(0, |c| shown_in(c, card))
}

fn start_of_turn(g: &mut Game) {
    g.with_recruit(0, |c| {
        c.run_player_hook(0, |s| s.start_of_turn, "start of turn")
    });
}

#[test]
fn war_drum_shows_its_battlecry_used() {
    let mut g = game(1);
    let drum = trinket(&mut g, "BG32_MagicItem_416");
    set_board(&mut g, 0, &["BG20_100"]); // Razorfen Geomancer: get 2 Blood Gems
    assert_eq!(shown(&mut g, drum), None);
    let e = g.players[0].board[0].eid;
    g.with_recruit(0, |c| c.trigger_battlecry(0, e, 0, 0));
    assert_eq!(g.players[0].hand.len(), 6, "three Battlecries");
    assert_eq!(
        shown(&mut g, drum).as_deref(),
        Some("One Battlecry each turn triggers two extra times. (0 left!)")
    );
}

#[test]
fn trinket_upgrades_count_down() {
    let mut g = game(2);
    set_board(&mut g, 0, &[PRISONGUARD]);
    let e = g.players[0].board[0].eid;
    let staff = trinket(&mut g, "BG36_MagicItem_407");
    let hook = trinket(&mut g, "BG36_MagicItem_408");
    assert_eq!(shown(&mut g, staff), None);
    g.with_recruit(0, |c| {
        for _ in 0..5 {
            c.emit(Event::Attacks {
                side: 0,
                attacker: e,
                defender: 0,
            });
        }
        c.emit(Event::GoldSpent {
            side: 0,
            amount: 20,
        });
    });
    assert_eq!(
        shown(&mut g, staff).as_deref(),
        Some(
            "After 13 friendly minions attack, replace this with a random Greater Dragon \
             Trinket. (8 left!)"
        )
    );
    assert_eq!(
        shown(&mut g, hook).as_deref(),
        Some(
            "After you spend 50 Gold, replace this with a random Greater Pirate Trinket. \
             (30 Gold left!)"
        )
    );
}

#[test]
fn trip_vouchers_show_the_turns_left() {
    let mut g = game(3);
    let trip = trinket(&mut g, "BG30_MagicItem_891");
    assert_eq!(shown(&mut g, trip), None);
    start_of_turn(&mut g);
    assert_eq!(
        shown(&mut g, trip).as_deref(),
        Some("In 2 turns, choose a Greater Trinket to buy and replace this. (1 turn left!)")
    );
}

#[test]
fn copper_coil_shows_its_improved_buff() {
    let mut g = game(4);
    let coil = trinket(&mut g, "BG35_MagicItem_300t");
    set_board(&mut g, 0, &[PRISONGUARD]);
    let host = g.players[0].board[0].eid;
    g.with_recruit(0, |c| {
        for _ in 0..2 {
            let card = cid(SATELLITE);
            c.emit(Event::Magnetized {
                side: 0,
                host,
                card,
            });
        }
    });
    assert_eq!(stats(&g, 0), vec![(3 + 2 + 4, 3 + 2 + 4)]);
    assert_eq!(
        shown(&mut g, coil).as_deref(),
        Some("Whenever you Magnetize a minion, give it +6/+6 and improve this.")
    );
}

#[test]
fn gyroblade_shows_the_satellite_it_will_give() {
    let mut g = game(5);
    let blade = trinket(&mut g, "BG36_MagicItem_810");
    let big = trinket(&mut g, "BG36_MagicItem_810t");
    assert_eq!(shown(&mut g, blade), None);
    g.players[0].stats.tavern_spells_cast_turn = 2;
    assert_eq!(
        shown(&mut g, blade).as_deref(),
        Some(
            "At the end of your turn, get a 12/12 Magnetic Satellite. (Improved by each Tavern \
             spell you've cast this turn!)"
        )
    );
    assert_eq!(
        shown(&mut g, big).as_deref(),
        Some(
            "At the end of your turn, get a 36/36 Magnetic Satellite. (Improved by each Tavern \
             spell you've cast this turn!)"
        )
    );
    // What the trinkets then give.
    g.with_recruit(0, |c| {
        c.run_player_hook(0, |s| s.end_of_turn, "end of turn")
    });
    let got: Vec<(i32, i32)> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion().map(|m| (m.atk, m.max_hp)))
        .collect();
    assert_eq!(got, vec![(12, 12), (36, 36)]);
}

#[test]
fn earrings_count_the_spells_cast() {
    let mut g = game(6);
    let ear = trinket(&mut g, "BG32_MagicItem_808");
    set_board(&mut g, 0, &[PRISONGUARD]);
    for _ in 0..3 {
        give(&mut g, 0, "BG28_897"); // Tavern Dish Banana
        cast_on(&mut g, 0, 0);
    }
    let text = "After you cast 4 spells, this plays a Blood Gem on all your minions. (1 left!)";
    assert_eq!(shown(&mut g, ear).as_deref(), Some(text));
    // The debug view carries it, in both languages.
    let v = view(&g, 0);
    assert_eq!(v["me"]["trinkets"][0]["text"], text);
    assert!(
        v["me"]["trinkets"][0]["text_zh"]
            .as_str()
            .unwrap()
            .contains('1')
    );
    // The fourth spell plays the gem and starts over.
    give(&mut g, 0, "BG28_897");
    cast_on(&mut g, 0, 0);
    assert_eq!(shown(&mut g, ear), None);
}

#[test]
fn per_turn_and_per_combat_counters() {
    let mut g = game(7);
    let jar = trinket(&mut g, "BG30_MagicItem_546");
    let tentacles = trinket(&mut g, "BG36_MagicItem_606");
    set_board(&mut g, 0, &[PRISONGUARD]);
    let e = g.players[0].board[0].eid;
    g.with_recruit(0, |c| {
        assert_eq!(shown_in(c, jar), None);
        assert_eq!(shown_in(c, tentacles), None);
        // Jar o' Gems counts attacks in combat (per-combat data of the context).
        c.combat = true;
        c.emit(Event::Attacks {
            side: 0,
            attacker: e,
            defender: 0,
        });
        c.combat = false;
        assert_eq!(
            shown_in(c, jar).as_deref(),
            Some(
                "After 2 friendly minions attack each combat, this plays a Blood Gem on all \
                 your Quilboar. (1 left!)"
            )
        );
        // Writhing Tentacles: the first minion discarded this turn.
        let m = c.new_minion(0, cid(PRISONGUARD));
        taverngym::content::writhing_tentacles(c, 0, &m);
        assert_eq!(
            shown_in(c, tentacles).as_deref(),
            Some(
                "After you discard your first minion each turn, get a copy of it with double \
                 stats. (0 left!)"
            )
        );
    });
    assert_eq!(g.players[0].hand.len(), 1);
}

#[test]
fn an_active_quest_shows_its_goal() {
    let mut g = game(8);
    let quest = cid("BG24_Quest_114");
    assert_eq!(shown(&mut g, quest), None, "as printed while not active");
    run_debug(
        &mut g,
        DebugCmd::GiveQuest {
            p: 0,
            quest: "BG24_Quest_114".into(),
            reward: Some("BG24_Reward_306".into()),
        },
    )
    .unwrap();
    let goal = g.players[0].quests[0].goal;
    assert!(goal > 0);
    assert_eq!(
        shown(&mut g, quest),
        Some(format!("Quest: Summon {goal} minions."))
    );
    let v = view(&g, 0);
    assert_eq!(
        v["me"]["quests"][0]["quest"]["text"],
        format!("Quest: Summon {goal} minions.")
    );
}

#[test]
fn rewards_show_their_improvements() {
    let mut g = game(9);
    let book = reward(&mut g, "BG24_Reward_306");
    let vault = reward(&mut g, "BG24_Reward_361");
    let kobold = reward(&mut g, "BG28_Reward_508");
    let chamber = reward(&mut g, "BG28_Reward_509");
    for r in [book, vault, kobold, chamber] {
        assert_eq!(shown(&mut g, r), None);
    }
    set_board(&mut g, 0, &[PRISONGUARD]);
    let e = g.players[0].board[0].eid;

    // Cooked Book: two minions bought.
    g.with_recruit(0, |c| {
        for _ in 0..2 {
            c.emit(Event::Bought {
                side: 0,
                card: cid(PRISONGUARD),
                eid: e,
                sid: e,
                price: 3,
            });
        }
    });
    assert_eq!(stats(&g, 0), vec![(3 + 2 + 4, 3 + 2 + 4)]);
    assert_eq!(
        shown(&mut g, book).as_deref(),
        Some("After you buy a minion, give it +6/+6 and improve this.")
    );

    // Gift of the Golden Kobold: two Refreshes.
    g.with_recruit(0, |c| {
        for _ in 0..2 {
            c.emit(Event::Refreshed {
                side: 0,
                auto: false,
            });
        }
    });
    assert_eq!(
        shown(&mut g, kobold).as_deref(),
        Some(
            "After you Refresh 5 times, make the highest-Tier minion in the Tavern Golden. \
             (3 left!)"
        )
    );

    // Hidden Treasure Vault (1 Gold, then 2) and Smelting Chamber (the Tier 1 minion).
    let gold = g.players[0].gold;
    start_of_turn(&mut g);
    start_of_turn(&mut g);
    assert_eq!(g.players[0].gold, gold + 1 + 2);
    assert_eq!(
        shown(&mut g, vault).as_deref(),
        Some("At the start of each turn, gain 3 Gold. (Improves each turn!)")
    );
    assert!(g.players[0].board[0].golden);
    assert_eq!(
        shown(&mut g, chamber).as_deref(),
        // Two turns: it improves every turn, with or without a minion of the Tier.
        Some("At the start of each turn, make a friendly Tier 3 minion Golden and improve this.")
    );
}
