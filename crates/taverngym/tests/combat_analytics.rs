use serde_json::{Value, json};
use taverngym::cards::cid;
use taverngym::debug::{DebugCmd, run_debug};
use taverngym::state::Minion;
use taverngym::types::Kw;
use taverngym::{Game, RulesConfig};

fn prepared() -> Game {
    let mut game = Game::new(43, RulesConfig::default());
    game.turn = 8;
    game.turn_alive = 8;
    for p in &mut game.players {
        p.powers.clear();
        p.pending.clear();
        p.hand.clear();
        p.board.clear();
        p.quests.clear();
        p.rewards.clear();
        p.trinkets.clear();
        p.effects.clear();
        p.secrets.clear();
        p.deity = None;
        p.health = 30;
        p.armor = 0;
        p.tier = 6;
    }
    game.players[0].board = (0..7)
        .map(|i| {
            let mut m = Minion::new(100 + i, cid("BG36_345"));
            m.set_stats(50, 50);
            m.tier = 6;
            m
        })
        .collect();
    game
}

fn preview(game: &mut Game, ghost: bool) -> Value {
    let cmd: DebugCmd = serde_json::from_value(json!({
        "cmd": "combat_preview", "a": 0, "b": 1, "n": 3,
        "details": true, "ghost_b": ghost,
    }))
    .unwrap();
    run_debug(game, cmd).unwrap()
}

#[test]
fn analytics_applies_cap_and_armor_without_mutating_game_or_recording_frames() {
    let mut game = prepared();
    game.players[1].health = 10;
    game.players[1].armor = 6;
    let before = serde_json::to_string(&game).unwrap();
    let result = preview(&mut game, false);
    assert_eq!(serde_json::to_string(&game).unwrap(), before);
    assert_eq!(result["sample"], Value::Null);
    for trial in result["trials"].as_array().unwrap() {
        assert_eq!(trial["damage_dealt"], 15);
        assert_eq!(trial["damage_taken"], 0);
        assert_eq!(trial["lethal_win"], false);
        assert_eq!(trial["truncated"], false);
    }
    game.players[1].armor = 5;
    assert_eq!(preview(&mut game, false)["trials"][0]["lethal_win"], true);
}

#[test]
fn analytics_uses_uncapped_top_four_damage_and_honors_ice_block() {
    let mut game = prepared();
    game.turn_alive = 4;
    assert_eq!(preview(&mut game, false)["trials"][0]["damage_dealt"], 48);
    assert_eq!(preview(&mut game, false)["trials"][0]["lethal_win"], true);
    game.players[1].secrets.push(cid("TB_Bacon_Secrets_12"));
    let trial = &preview(&mut game, false)["trials"][0];
    assert_eq!(trial["damage_dealt"], 0);
    assert_eq!(trial["lethal_win"], false);
}

#[test]
fn analytics_never_reports_damage_or_lethal_against_a_ghost() {
    let mut game = prepared();
    game.players[1].health = 0;
    game.players[1].alive = false;
    let trial = &preview(&mut game, true)["trials"][0];
    assert_eq!(trial["winner"], 0);
    assert_eq!(trial["damage_dealt"], 0);
    assert_eq!(trial["lethal_win"], false);
}

#[test]
fn truncated_trials_never_apply_invented_hero_damage() {
    let mut game = prepared();
    game.cfg.max_combat_attacks = 0;
    let mut enemy = Minion::new(900, cid("BG36_345"));
    enemy.set_stats(50, 50);
    game.players[1].board.push(enemy);
    let trial = &preview(&mut game, false)["trials"][0];
    assert_eq!(trial["truncated"], true);
    assert_eq!(trial["damage_dealt"], 0);
    assert_eq!(trial["damage_taken"], 0);
}

#[test]
fn legacy_preview_keeps_replay_frames_and_its_uncapped_damage_semantics() {
    let mut game = prepared();
    let cmd: DebugCmd = serde_json::from_value(json!({
        "cmd": "combat_preview", "a": 0, "b": 1, "n": 2,
    }))
    .unwrap();
    let result = run_debug(&mut game, cmd).unwrap();
    assert!(result.get("trials").is_none());
    assert!(result["sample"]["frames"].as_array().unwrap().len() >= 2);
    assert_eq!(result["sample"]["damage"], 48);
}

#[test]
fn continuation_never_reapplies_start_of_combat_buffs() {
    let mut game = prepared();
    let mut bird = Minion::new(300, cid("BG26_805"));
    bird.set_stats(7, 7); // already includes the observed opening buff
    game.players[0].board = vec![bird];
    let command = |after_start| {
        serde_json::from_value::<DebugCmd>(json!({
        "cmd": "combat_preview", "a": 0, "b": 1, "n": 1,
        "after_start": after_start, "first_attacker": if after_start { json!(0) } else { Value::Null },
    })).unwrap()
    };
    let resumed = run_debug(&mut game, command(true)).unwrap();
    let initialized = run_debug(&mut game, command(false)).unwrap();
    assert_eq!(resumed["sample"]["frames"][1]["boards"][0][0]["atk"], 7);
    assert_eq!(initialized["sample"]["frames"][1]["boards"][0][0]["atk"], 8);
}

#[test]
fn continuation_honors_observed_first_side_and_complete_windfury_turn() {
    for (keyword, expected) in [(Kw::WINDFURY, 2), (Kw::MEGA_WINDFURY, 4)] {
        let mut game = prepared();
        game.cfg.max_combat_attacks = 1;
        let mut attacker = Minion::new(400, cid("BG20_100"));
        attacker.set_stats(1, 100);
        attacker.give(keyword);
        game.players[0].board = vec![attacker];
        game.players[1].board = (0..2)
            .map(|i| {
                let mut target = Minion::new(500 + i, cid("BG20_100"));
                target.set_stats(1, 100);
                target
            })
            .collect();
        // With two bodies the enemy normally attacks first. The observed side wins.
        let cmd: DebugCmd = serde_json::from_value(json!({
            "cmd":"combat_preview", "a":0, "b":1, "n":1,
            "after_start":true, "first_attacker":0,
        }))
        .unwrap();
        let result = run_debug(&mut game, cmd).unwrap();
        let attacks = result["sample"]["frames"][2]["attacks"].as_array().unwrap();
        assert_eq!(attacks.len(), expected);
        assert!(attacks.iter().all(|pair| pair[0] == 400));
    }
}

#[test]
fn continuation_requires_an_explicit_valid_first_side_and_acknowledges_mode() {
    let mut game = prepared();
    for first in [Value::Null, json!(2)] {
        let cmd: DebugCmd = serde_json::from_value(json!({
            "cmd":"combat_preview", "a":0, "b":1, "n":1,
            "after_start":true, "first_attacker":first,
        }))
        .unwrap();
        assert!(run_debug(&mut game, cmd).is_err());
    }
    let cmd: DebugCmd = serde_json::from_value(json!({
        "cmd":"combat_preview", "a":0, "b":1, "n":1, "details":true,
        "after_start":true, "first_attacker":0,
    }))
    .unwrap();
    assert_eq!(run_debug(&mut game, cmd).unwrap()["after_start"], true);
}
