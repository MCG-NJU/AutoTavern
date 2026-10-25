//! `Script::shown` of Beasts, Demons, Dragons, Elementals, Mechs and Aberrations: the live
//! text after the state changed (`tests/shown.rs` covers the fresh state of every hook).

use taverngym::actions::Action;
use taverngym::cards::{cid, db};
use taverngym::debug::{DebugCmd, live_text, run_debug};
use taverngym::event::Event;
use taverngym::script::Shown;
use taverngym::state::{Item, Minion, Pending};
use taverngym::types::PlayerId;
use taverngym::{Game, RulesConfig};

/// A game past hero selection, turn 1 recruit phase, every seat pinned to Patchwerk (a
/// passive hero) with its start-of-game state cleared.
fn game(seed: u64) -> Game {
    let mut g = Game::new(seed, RulesConfig::default());
    for p in 0..8 {
        g.apply(p, Action::Choose(0)).unwrap();
    }
    for p in 0..8 {
        g.players[p].hero = Some(cid("TB_BaconShop_HERO_34"));
        g.players[p].pending.clear();
        g.players[p].hand.clear();
        g.players[p].stats = Default::default();
        g.players[p].effects.clear();
        g.players[p].quests.clear();
        g.players[p].counters.clear();
        g.players[p].board.clear();
        g.players[p].powers.clear();
        g.players[p].set_hero_power(0, cid("TB_BaconShop_HP_035"));
    }
    g.cfg.triple_copies = 99; // no triples: the fixtures keep their copies
    g
}

fn minion(g: &mut Game, p: PlayerId, id: &str) -> Minion {
    g.with_recruit(p, |c| c.new_minion(0, cid(id)))
}

fn set_board(g: &mut Game, ids: &[&str]) {
    let b: Vec<Minion> = ids.iter().map(|id| minion(g, 0, id)).collect();
    g.players[0].board = b;
}

fn give(g: &mut Game, id: &str) {
    g.with_recruit(0, |c| c.give_card(0, cid(id)).unwrap());
}

/// Play the last hand card to the right end of the board.
fn play_last(g: &mut Game) {
    let hand = g.players[0].hand.len() - 1;
    let pos = g.players[0].board.len();
    g.apply(0, Action::Play { hand, pos }).unwrap();
}

/// Cast the last hand card (a targeted spell) on the board minion `board_idx`.
fn cast_on(g: &mut Game, board_idx: usize) {
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Play { hand, pos: 0 }).unwrap();
    let eid = g.players[0].board[board_idx].eid;
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
        panic!("expected a target choice")
    };
    let i = options.iter().position(|e| *e == eid).unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
}

/// Live English text of the board minion `i` (None = as printed).
fn board_text(g: &mut Game, i: usize) -> Option<String> {
    g.with_recruit(0, |c| {
        let m = c.boards[0][i].clone();
        let sh = Shown {
            side: 0,
            card: m.card,
            golden: m.golden,
            minion: Some(&m),
            data: 0,
        };
        live_text(c, &sh).map(|t| t.0)
    })
}

/// Live English text of the hand card `i` (None = as printed).
fn hand_text(g: &mut Game, i: usize) -> Option<String> {
    let item = g.players[0].hand[i].item.clone();
    let card = item.card();
    g.with_recruit(0, |c| {
        let sh = Shown {
            side: 0,
            card,
            golden: db().get(card).normal.is_some(),
            minion: item.as_minion(),
            data: 0,
        };
        live_text(c, &sh).map(|t| t.0)
    })
}

/// Live English text of a card without an instance (a spell, a trinket).
fn card_text(g: &mut Game, id: &str) -> Option<String> {
    g.with_recruit(0, |c| {
        let sh = Shown {
            side: 0,
            card: cid(id),
            golden: false,
            minion: None,
            data: 0,
        };
        live_text(c, &sh).map(|t| t.0)
    })
}

fn txt(s: &str) -> Option<String> {
    Some(s.to_string())
}

#[test]
fn leviathan_shows_its_improved_attack() {
    let mut g = game(11);
    set_board(&mut g, &["BG35_602"]);
    assert_eq!(
        board_text(&mut g, 0),
        None,
        "as printed while nothing changed"
    );
    for _ in 0..2 {
        give(&mut g, "BG26_800"); // Manasaber, a Beast
        play_last(&mut g);
    }
    let want = "Whenever you summon a Beast, give it +9 Attack and improve this permanently.";
    assert_eq!(board_text(&mut g, 0), txt(want));
    // The debug view carries it, in both languages.
    let v = taverngym::debug::view(&g, 0);
    assert_eq!(v["me"]["board"][0]["text"], want);
    assert!(
        v["me"]["board"][0]["text_zh"]
            .as_str()
            .unwrap()
            .contains("+9")
    );
    assert_eq!(v["me"]["board"][1]["live_text"], serde_json::Value::Null);

    // Golden: +6, then +12 after one summon.
    set_board(&mut g, &["BG35_602_G"]);
    give(&mut g, "BG26_800");
    play_last(&mut g);
    assert_eq!(
        board_text(&mut g, 0),
        txt("Whenever you summon a Beast, give it +12 Attack and improve this permanently.")
    );
}

#[test]
fn patient_scout_shows_the_tier_it_reached() {
    let mut g = game(12);
    give(&mut g, "BG24_715");
    assert_eq!(hand_text(&mut g, 0), None);
    let set = |g: &mut Game, n: i32| {
        if let Item::Minion(m) = &mut g.players[0].hand[0].item {
            m.counter = n; // turns held (its start-of-turn hook)
        }
    };
    set(&mut g, 2);
    assert_eq!(
        hand_text(&mut g, 0),
        txt("When you sell this, Discover a Tier 3 minion. (Improves each turn!)")
    );
    set(&mut g, 9);
    assert_eq!(
        hand_text(&mut g, 0),
        txt("When you sell this, Discover a Tier 6 minion. (Improves each turn!)"),
        "no Tier above 6"
    );
}

#[test]
fn felboar_and_faceless_converter_count_spells() {
    let mut g = game(13);
    // Felboar, Faceless Converter, a golden Converter and a target.
    set_board(&mut g, &["BG28_633", "BG36_318", "BG36_318_G", "BG36_345"]);
    give(&mut g, "BG28_897"); // Tavern Dish Banana
    cast_on(&mut g, 3);
    assert_eq!(
        board_text(&mut g, 0),
        txt(
            "After you cast 3 spells, consume a minion in the Tavern to gain its stats. \
             (2 left!)"
        )
    );
    assert_eq!(
        board_text(&mut g, 1),
        txt(
            "Deathrattle: Give your Deity +4/+2. (Improved by each Tavern spell you've cast \
             this game!)"
        )
    );
    assert_eq!(
        board_text(&mut g, 2),
        txt(
            "Deathrattle: Give your Deity +8/+4. (Improved by each Tavern spell you've cast \
             this game!)"
        )
    );
    for _ in 0..2 {
        give(&mut g, "BG28_897");
        cast_on(&mut g, 3);
    }
    assert_eq!(board_text(&mut g, 0), None, "it fired: 3 left again");
    assert_eq!(
        board_text(&mut g, 1),
        txt(
            "Deathrattle: Give your Deity +8/+4. (Improved by each Tavern spell you've cast \
             this game!)"
        )
    );
}

#[test]
fn malchezaar_shows_the_health_refreshes_left() {
    let mut g = game(14);
    set_board(&mut g, &["BG26_524"]);
    assert_eq!(board_text(&mut g, 0), None);
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(
        board_text(&mut g, 0),
        txt("Two Refreshes each turn cost Health instead of Gold. (1 left!)")
    );
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(
        board_text(&mut g, 0),
        txt("Two Refreshes each turn cost Health instead of Gold. (0 left!)")
    );
    // Golden: four.
    set_board(&mut g, &["BG26_524_G"]);
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(
        board_text(&mut g, 0),
        txt("Four Refreshes each turn cost Health instead of Gold. (3 left!)")
    );
}

#[test]
fn wrathguard_shows_its_improved_buff_and_keeps_the_activate_cost() {
    let mut g = game(15);
    g.players[0].gold = 10;
    set_board(&mut g, &["BG36_362", "BG36_362_G"]);
    for i in 0..2 {
        g.apply(0, Action::Activate(i)).unwrap();
    }
    assert_eq!(
        board_text(&mut g, 0),
        txt("Deathrattle: Give minions in the Tavern +4/+4 this game. Activate (1): Improve this.")
    );
    assert_eq!(
        board_text(&mut g, 1),
        txt("Deathrattle: Give minions in the Tavern +8/+8 this game. Activate (1): Improve this.")
    );
}

#[test]
fn escapist_and_revenant_show_what_is_left() {
    let mut g = game(16);
    set_board(&mut g, &["BG36_733", "BG34_858"]);
    g.with_recruit(0, |c| {
        c.hero_damage(0, 3);
        c.spend_gold(0, 3);
    });
    assert_eq!(
        board_text(&mut g, 0),
        txt("After your hero takes 4 damage, get a copy of Corrupted Cupcakes. (1 left!)")
    );
    assert_eq!(
        board_text(&mut g, 1),
        txt("After you spend 7 Gold, cast Easterly Winds. (4 left!)")
    );
    // Past the threshold the remainder carries over.
    g.with_recruit(0, |c| {
        c.hero_damage(0, 2);
        c.spend_gold(0, 10);
    });
    assert_eq!(
        board_text(&mut g, 0),
        txt("After your hero takes 4 damage, get a copy of Corrupted Cupcakes. (3 left!)")
    );
    assert_eq!(
        board_text(&mut g, 1),
        txt("After you spend 7 Gold, cast Easterly Winds. (1 left!)")
    );
}

#[test]
fn chromatic_tear_counts_battlecry_minions() {
    let mut g = game(17);
    let tear = "BG35_MagicItem_840t";
    run_debug(
        &mut g,
        DebugCmd::GiveTrinket {
            p: 0,
            card: tear.into(),
        },
    )
    .unwrap();
    g.players[0].hand.clear(); // the two Chromadrakes
    assert_eq!(card_text(&mut g, tear), None);
    for _ in 0..3 {
        give(&mut g, "BG36_112"); // Fetid Corroder (a Battlecry)
        play_last(&mut g);
    }
    assert_eq!(
        card_text(&mut g, tear),
        txt(
            "Get 2 random Chromadrakes. After you play 7 Battlecry minions, repeat this. \
             (4 left!)"
        )
    );
}

#[test]
fn elementals_show_their_improvements() {
    let mut g = game(18);
    // Ultraviolet Ascendant, Unbound Tempest.
    set_board(&mut g, &["BG31_810", "BG36_352"]);
    give(&mut g, "BG36_181"); // Air Baller, an Elemental
    assert_eq!(hand_text(&mut g, 0), None);
    play_last(&mut g);
    assert_eq!(
        board_text(&mut g, 0),
        txt(
            "Start of Combat: Give your other Elementals +6/+6. (Improves after you play an \
             Elemental!)"
        )
    );
    assert_eq!(
        board_text(&mut g, 1),
        txt(
            "After you play 4 Elementals, gain the stats of the highest- Health minion in the \
             Tavern. (3 left!)"
        )
    );
    // Conflagration: improved by the Elemental played this turn, plus the Tavern-spell bonus.
    let conflagration = "BG35_910";
    assert_eq!(
        card_text(&mut g, conflagration),
        txt("Give a minion +8/+8. (Improved by each Elemental you played this turn!)")
    );
    g.players[0].stats.spell_atk = 1;
    g.players[0].stats.spell_hp = 2;
    assert_eq!(
        card_text(&mut g, conflagration),
        txt("Give a minion +9/+10. (Improved by each Elemental you played this turn!)")
    );
    // A sold Air Baller improves the next ones, wherever they are.
    g.apply(0, Action::Sell(2)).unwrap();
    give(&mut g, "BG36_181");
    give(&mut g, "BG36_181_G");
    assert_eq!(
        hand_text(&mut g, 0),
        txt("When you sell this, give your minions +4/+4. Improve your future Ballers.")
    );
    assert_eq!(
        hand_text(&mut g, 1),
        txt("When you sell this, give your minions +8/+8. Improve your future Ballers.")
    );
}

#[test]
fn tavern_spells_show_the_spell_bonus() {
    let mut g = game(19);
    assert_eq!(card_text(&mut g, "BG36_246"), None);
    assert_eq!(card_text(&mut g, "BG28_888"), None);
    set_board(&mut g, &["BG35_341_G"]); // Enchanted Sentinel: Tavern spells +2/+2
    g.players[0].stats.spell_atk = 1;
    assert_eq!(
        card_text(&mut g, "BG36_246"),
        txt(
            "Give your minions +6/+4. Repeat for your Dragons. Repeat for your minions with \
             Divine Shield."
        )
    );
    assert_eq!(
        card_text(&mut g, "BG28_888"),
        txt("Give a friendly minion of each type +7/+6.")
    );
}

#[test]
fn mechs_show_their_progress() {
    let mut g = game(20);
    // Spark Snapper, a golden one, Auto Reveille.
    set_board(&mut g, &["BG36_851", "BG36_851_G", "BG36_367"]);
    give(&mut g, "BG36_854"); // Rescue Bot, a Mech
    play_last(&mut g);
    assert_eq!(
        board_text(&mut g, 0),
        txt("Whenever you play a Mech, Magnetize a 4/6 Satellite to it and improve this.")
    );
    assert_eq!(
        board_text(&mut g, 1),
        txt("Whenever you play a Mech, Magnetize a 8/12 Satellite to it and improve this.")
    );
    g.with_recruit(0, |c| {
        c.emit(Event::Bought {
            side: 0,
            card: cid("BG36_345"),
            eid: 0,
            sid: 0,
            price: 3,
        })
    });
    assert_eq!(
        board_text(&mut g, 2),
        txt(
            "After you buy 3 cards, Magnetize a random Volumizer to this and get a copy of it. \
             (2 left!)"
        )
    );
}

#[test]
fn counters_on_the_minion_and_on_the_player_scale_the_amounts() {
    let mut g = game(21);
    // Hopebringer, golden Hopebringer, Harbinger Aph'lass, Parasitic Fleshling (golden too).
    set_board(
        &mut g,
        &[
            "BG36_364",
            "BG36_364_G",
            "BGFYM_005",
            "BG36_114",
            "BG36_114_G",
        ],
    );
    // Two friendly Divine Shields lost, one card discarded with Aph'lass in play. The
    // count is in steps of the normal card: a Golden's improvement is two (`flag::IMPROVES`).
    g.players[0].board[0].counter = 2;
    g.players[0].board[1].counter = 4;
    g.players[0].board[2].counter = 1;
    g.players[0].stats.cards_discarded = 3;
    assert_eq!(
        board_text(&mut g, 0),
        txt(
            "Start of Combat: Give your minions +12/+9. (Permanently improves after a friendly \
             minion loses Divine Shield!)"
        )
    );
    assert_eq!(
        board_text(&mut g, 1),
        txt(
            "Start of Combat: Give your minions +24/+18. (Permanently improves after a \
             friendly minion loses Divine Shield!)"
        )
    );
    assert_eq!(
        board_text(&mut g, 2),
        txt("Whenever you discard a card, give your Deity +4/+2 and improve this.")
    );
    assert_eq!(
        board_text(&mut g, 3),
        txt(
            "At the end of your turn, give your left-most minion +8/+8. (Improved by each card \
             you've discarded this game!)"
        )
    );
    assert_eq!(
        board_text(&mut g, 4),
        txt(
            "At the end of your turn, give your left-most minion +16/+16. (Improved by each \
             card you've discarded this game!)"
        )
    );
}
