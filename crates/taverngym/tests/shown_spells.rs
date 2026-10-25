//! Live text (`Script::shown`) of the spells: Tavern spells with the "your Tavern spells give
//! an extra +X/+Y" bonus, the Blood Gem, Triple Reward, counters. Each test changes the state
//! the script reads and, where it can, casts the spell to check that the text told the truth.

use taverngym::actions::Action;
use taverngym::cards::{cid, db};
use taverngym::debug::{DebugCmd, live_text, run_debug, view};
use taverngym::script::{Me, Shown, script};
use taverngym::state::{Minion, Pending};
use taverngym::types::EntityId;
use taverngym::{Game, RulesConfig};

/// A game past hero selection with every seat on Patchwerk (passive) and a clean slate.
fn game(seed: u64) -> Game {
    let mut g = Game::new(seed, RulesConfig::default());
    for p in 0..g.players.len() {
        if matches!(
            g.players[p].pending.last(),
            Some(Pending::HeroSelect { .. })
        ) {
            g.apply(p, Action::Choose(0)).unwrap();
        }
    }
    for p in 0..g.players.len() {
        let pl = &mut g.players[p];
        pl.hero = Some(cid("TB_BaconShop_HERO_34"));
        pl.pending.clear();
        pl.hand.clear();
        pl.stats = Default::default();
        pl.effects.clear();
        pl.quests.clear();
        pl.counters.clear();
        pl.board.clear();
        pl.powers.clear();
        pl.set_hero_power(0, cid("TB_BaconShop_HP_035"));
    }
    g
}

/// The live English text of card `id` (spell data `data`) for player 0; None = as printed.
fn text(g: &mut Game, id: &str, data: i32) -> Option<String> {
    let card = cid(id);
    g.with_recruit(0, |c| {
        let sh = Shown {
            side: 0,
            card,
            golden: false,
            minion: None,
            data,
        };
        live_text(c, &sh).map(|t| t.0)
    })
}

fn set_board(g: &mut Game, ids: &[&str]) {
    let b: Vec<Minion> = ids
        .iter()
        .map(|id| g.with_recruit(0, |c| c.new_minion(0, cid(id))))
        .collect();
    g.players[0].board = b;
}

/// Give player 0 the spell `id` and cast it from the hand, on `target` if it asks for one.
fn cast(g: &mut Game, id: &str, target: EntityId) {
    let card = cid(id);
    g.with_recruit(0, |c| c.give_card(0, card)).unwrap();
    let hand = g.players[0]
        .hand
        .iter()
        .position(|h| h.item.card() == card)
        .unwrap();
    g.apply(0, Action::Play { hand, pos: 0 }).unwrap();
    if let Some(Pending::Target { options, .. }) = g.players[0].pending.last() {
        let i = options.iter().position(|e| *e == target).unwrap();
        g.apply(0, Action::Choose(i)).unwrap();
    }
}

fn stats(g: &Game, i: usize) -> (i32, i32) {
    (g.players[0].board[i].atk, g.players[0].board[i].hp)
}

#[test]
fn a_one_stat_spell_shows_that_stat_with_its_bonus() {
    let mut g = game(1);
    assert_eq!(
        text(&mut g, "BG28_503", 0),
        None,
        "as printed without a bonus"
    );
    g.players[0].stats.spell_atk = 2;
    g.players[0].stats.spell_hp = 5;
    assert_eq!(
        text(&mut g, "BG28_503", 0).as_deref(), // Fortify
        Some("Give a minion +8 Health and Taunt.")
    );
    assert_eq!(
        text(&mut g, "EBG_Spell_014", 0).as_deref(), // Pointy Arrow
        Some("Give a minion +6 Attack.")
    );
    assert_eq!(
        text(&mut g, "BG33_812", 0).as_deref(), // Hostile Bounty
        Some("Give four friendly minions +6 Attack.")
    );
    assert_eq!(
        text(&mut g, "BG33_811", 0).as_deref(), // Healthy Bounty
        Some("Give four friendly minions +9 Health.")
    );
    // The cast gives what the text says.
    set_board(&mut g, &["BG36_345"]);
    let (eid, before) = (g.players[0].board[0].eid, stats(&g, 0));
    cast(&mut g, "BG28_503", eid);
    assert_eq!(stats(&g, 0).1 - before.1, 8);
}

#[test]
fn every_pair_of_a_spell_gets_the_bonus_of_the_board() {
    let mut g = game(2);
    // Humon'gozz: "Your Tavern spells give an extra +1/+2", on top of +2/+0 this game.
    set_board(&mut g, &["BG36_345", "BG32_341"]);
    g.players[0].stats.spell_atk = 2;
    assert_eq!(
        text(&mut g, "BG36_883", 0).as_deref(), // Winner's Bread
        Some(
            "Give a minion +5/+5. If you win your next combat, give it another +7/+8 at the \
             start of your next turn."
        )
    );
    assert_eq!(
        text(&mut g, "BG31_880", 0).as_deref(), // Alliance Flag
        Some("Choose One - Give a minion +6/+3; or +4/+5.")
    );
    assert_eq!(
        text(&mut g, "BG34_990", 0).as_deref(), // Wave of Gold
        Some("Give your minions +6/+4. Give Golden ones another +6/+4.")
    );
    assert_eq!(
        text(&mut g, "BG31_886", 0).as_deref(), // Forest's Bounty
        Some("Choose One - Give a minion +9/+8 twice; or Give your minions +5/+4.")
    );
    assert_eq!(
        text(&mut g, "BG28_966", 0).as_deref(), // Them Apples
        Some("Give minions in the Tavern +4/+4.")
    );
    // Winner's Bread's first part, cast on the Prisonguard.
    let (eid, before) = (g.players[0].board[0].eid, stats(&g, 0));
    cast(&mut g, "BG36_883", eid);
    let after = stats(&g, 0);
    assert_eq!((after.0 - before.0, after.1 - before.1), (5, 5));
}

#[test]
fn blood_gem_shows_its_size_and_the_portrait_extra() {
    let mut g = game(3);
    assert_eq!(text(&mut g, "BG20_GEM", 0), None);
    g.players[0].stats.gem_atk = 2;
    g.players[0].stats.gem_hp = 3;
    assert_eq!(
        text(&mut g, "BG20_GEM", 0).as_deref(),
        Some("Give a minion +3/+4.")
    );
    // Surveyor Portrait: "Blood Gems played from your hand give an extra +6/+6."
    run_debug(
        &mut g,
        DebugCmd::GiveTrinket {
            p: 0,
            card: "BG30_MagicItem_943".into(),
        },
    )
    .unwrap();
    g.players[0].pending.clear();
    assert_eq!(
        text(&mut g, "BG20_GEM", 0).as_deref(),
        Some("Give a minion +9/+10.")
    );
    set_board(&mut g, &["BG36_345"]);
    let (eid, before) = (g.players[0].board[0].eid, stats(&g, 0));
    cast(&mut g, "BG20_GEM", eid);
    let after = stats(&g, 0);
    assert_eq!((after.0 - before.0, after.1 - before.1), (9, 10));
}

#[test]
fn blood_gem_barrage_shows_one_gem_with_both_bonuses() {
    let mut g = game(4);
    g.players[0].stats.gem_atk = 1;
    g.players[0].stats.spell_atk = 1;
    g.players[0].stats.spell_hp = 1;
    // One Blood Gem: the card's +2/+2, +1/+0 from the gem bonus, +1/+1 from the Tavern-spell bonus.
    assert_eq!(
        text(&mut g, "BG34_689", 0).as_deref(),
        Some(
            "After the Tavern is Refreshed this game, give its minions +4/+3 worth of Blood \
             Gems."
        )
    );
    // Cast it and Refresh: every Tavern minion has exactly that over its base stats.
    cast(&mut g, "BG34_689", 0);
    g.players[0].gold = 10;
    g.apply(0, Action::Refresh).unwrap();
    let minions: Vec<&Minion> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .collect();
    assert!(!minions.is_empty());
    for m in minions {
        let d = db().get(m.card);
        assert_eq!(
            (m.gems, m.atk - d.atk, m.hp - d.hp),
            (1, 4, 3),
            "{}",
            d.name
        );
    }
}

#[test]
fn triple_reward_shows_its_tier() {
    let mut g = game(5);
    let reward = "TB_BaconShop_Triples_01";
    assert_eq!(text(&mut g, reward, 0), None);
    assert_eq!(
        text(&mut g, reward, 4).as_deref(),
        Some("Discover a minion from Tier 4.")
    );
    // The script never looks above Tier 7 (a Tier 7 Tavern's Triple Reward).
    assert_eq!(
        text(&mut g, reward, 9).as_deref(),
        Some("Discover a minion from Tier 7.")
    );
    // The hand card of the view carries its data.
    run_debug(
        &mut g,
        DebugCmd::AddSpell {
            p: 0,
            zone: "hand".into(),
            card: reward.into(),
            data: 5,
        },
    )
    .unwrap();
    let v = view(&g, 0);
    assert_eq!(v["me"]["hand"][0]["text"], "Discover a minion from Tier 5.");
    assert!(
        v["me"]["hand"][0]["text_zh"]
            .as_str()
            .unwrap()
            .contains("等级5")
    );
}

#[test]
fn prize_wall_counts_the_turns_down() {
    let mut g = game(6);
    let wall = cid("TB_BaconShop_HP_106");
    g.set_hero_power(0, 0, wall);
    g.players[0].pending.clear();
    assert_eq!(text(&mut g, "TB_BaconShop_HP_106", 0), None);
    // The Hero Power's own start-of-turn hook.
    let tick = |g: &mut Game| {
        let f = script(wall).unwrap().start_of_turn.unwrap();
        g.with_recruit(0, |c| {
            f(
                c,
                Me {
                    side: 0,
                    eid: 0,
                    card: wall,
                    golden: false,
                    opt: 0,
                    pos: 0,
                    data: 0,
                },
            )
        });
    };
    // On the game's Turns 4, 8...: the turns are the game's.
    g.turn = 1;
    tick(&mut g);
    assert_eq!(
        text(&mut g, "TB_BaconShop_HP_106", 0).as_deref(),
        Some("Every 4 turns, Discover a Darkmoon Prize. (3 turns left!)")
    );
    g.turn = 2;
    tick(&mut g);
    g.turn = 3;
    tick(&mut g);
    assert_eq!(
        text(&mut g, "TB_BaconShop_HP_106", 0).as_deref(),
        Some("Every 4 turns, Discover a Darkmoon Prize. (1 turn left!)")
    );
    assert_eq!(
        view(&g, 0)["me"]["hero_power"]["text"],
        "Every 4 turns, Discover a Darkmoon Prize. (1 turn left!)"
    );
    // Turn 4 offers the prize and starts over.
    g.turn = 4;
    tick(&mut g);
    assert!(matches!(
        g.players[0].pending.last(),
        Some(Pending::Discover { .. })
    ));
    assert_eq!(text(&mut g, "TB_BaconShop_HP_106", 0), None);
}

#[test]
fn boon_of_beetles_shows_the_beetles_as_they_are_now() {
    let mut g = game(7);
    assert_eq!(text(&mut g, "BG28_603", 0), None);
    g.players[0].stats.beetle_atk = 4;
    g.players[0].stats.beetle_hp = 6;
    assert_eq!(
        text(&mut g, "BG28_603", 0).as_deref(),
        Some(
            "When you have space in combat, summon two 6/8 Beetles and give them Taunt. (2 left!)"
        )
    );
    // The Beetle the spell would summon.
    let b = g.with_recruit(0, |c| c.new_minion(0, cid("BG28_603t")));
    assert_eq!((b.atk, b.hp), (6, 8));
}

#[test]
fn casting_crystallization_changes_the_spells_in_hand_and_in_the_tavern() {
    let mut g = game(8);
    g.players[0].shop.clear();
    for (zone, card) in [("hand", "BG28_897"), ("shop", "BG28_168")] {
        run_debug(
            &mut g,
            DebugCmd::AddSpell {
                p: 0,
                zone: zone.into(),
                card: card.into(),
                data: 0,
            },
        )
        .unwrap();
    }
    let v = view(&g, 0);
    assert_eq!(v["me"]["hand"][0]["text"], "Give a minion +2/+2.");
    assert_eq!(v["me"]["hand"][0]["live_text"], serde_json::Value::Null);
    // Crystallization: "Your Tavern spells give an extra +1/+1 this game."
    cast(&mut g, "BGS_Treasures_110", 0);
    cast(&mut g, "BGS_Treasures_110", 0);
    let v = view(&g, 0);
    assert_eq!(v["me"]["hand"][0]["text"], "Give a minion +4/+4."); // Tavern Dish Banana
    assert_eq!(v["me"]["hand"][0]["live_text"], true);
    assert_eq!(v["me"]["shop"][0]["text"], "Give your minions +3/+3."); // Shiny Ring
    assert!(
        v["me"]["shop"][0]["text_zh"]
            .as_str()
            .unwrap()
            .contains("+3/+3")
    );
    // A prize is not a Tavern spell: it shows no bonus (and `spell_buff` gives it none).
    assert_eq!(text(&mut g, "BGS_Treasures_015", 0), None);
}
