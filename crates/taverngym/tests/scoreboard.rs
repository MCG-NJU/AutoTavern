mod common;
use common::*;
use taverngym::debug::public_view;

#[test]
fn scoreboard_uses_completed_combat_and_does_not_reveal_live_cards() {
    let mut g = game(4109);
    let mut beast = minion(&mut g, 1, "BG36_354");
    beast.races = Races::BEAST;
    let mut demon = beast.clone();
    demon.races = Races::DEMON;
    g.players[1].seen_board = vec![beast.clone(), beast.clone(), beast];
    g.players[1].board = vec![demon.clone(); 7];
    g.players[1].last_board = vec![demon; 7];
    g.players[1].stats.streak = 3;
    g.players[1].stats.last_damage = 15;
    let v = public_view(&g, 0);
    let o = &v["opponents"][0];
    assert_eq!(o["last_warband"]["race"], "BEAST");
    assert_eq!(o["last_warband"]["count"], 3);
    assert_eq!(o["last_warband"]["size"], 3);
    assert_eq!(o["win_streak"], 3);
    assert_eq!(o["last_damage"], 15);
    assert_eq!(o["board"].as_array().unwrap().len(), 0);
    assert_eq!(o["seen_board"].as_array().unwrap().len(), 0);
    assert_eq!(o["last_board"].as_array().unwrap().len(), 0);
    assert!(v["me"]["last_warband"].is_null());
    g.players[0].stats.tied_last = true;
    assert_eq!(public_view(&g, 0)["me"]["last_warband"]["size"], 0);
}

#[test]
fn scoreboard_counts_dual_all_and_tied_tribes() {
    let mut g = game(4110);
    g.lobby_races = Races(Races::BEAST.0 | Races::DEMON.0);
    let mut dual = minion(&mut g, 0, "BG36_354");
    dual.races = Races(Races::BEAST.0 | Races::DEMON.0);
    let mut all = dual.clone();
    all.races = Races::ALL;
    g.players[0].seen_board = vec![dual, all];
    g.players[0].stats.streak = -2;
    g.players[0].stats.last_damage = -15;
    let v = public_view(&g, 0);
    let me = &v["me"];
    assert!(me["last_warband"]["race"].is_null());
    assert_eq!(me["last_warband"]["mixed"], true);
    assert_eq!(me["last_warband"]["count"], 2);
    assert_eq!(me["last_warband"]["counts"].as_array().unwrap().len(), 2);
    assert_eq!(me["win_streak"], 0);
    assert_eq!(me["last_damage"], -15);
}
