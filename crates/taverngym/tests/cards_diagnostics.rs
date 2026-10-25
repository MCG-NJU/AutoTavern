//! Driver stops and effect cutoffs must remain observable without inventing a result.
use taverngym::RulesConfig;
use taverngym::actions::A_END;
use taverngym::bot;
use taverngym::cards::{CardKind, db};
use taverngym::engine::{Ctx, Game, Phase, StoppedReason};
use taverngym::env::SoloEnv;
use taverngym::event::Event;

// A supported counterfactual config whose lobby has no eligible heroes. This exercises
// the actual public constructor/driver failure path, without rewriting a pending menu.
fn empty_hero_lobby() -> RulesConfig {
    RulesConfig {
        num_players: 2,
        guaranteed_heroes: vec![],
        hero_bans: db()
            .iter()
            .filter(|c| c.kind == CardKind::Hero)
            .map(|c| c.id.clone())
            .collect(),
        record_log: false,
        ..RulesConfig::default()
    }
}

#[test]
fn bot_stall_is_persistent_and_does_not_assign_final_placements() {
    let cfg = empty_hero_lobby();
    cfg.validate().unwrap();
    let g = bot::run_bot_game(4600, cfg);
    assert_eq!(g.stopped_reason, Some(StoppedReason::NoProgress));
    assert_eq!(g.phase, Phase::HeroSelect);
    assert!(!g.is_over());
    assert!(g.players.iter().all(|p| p.placement == 0));
    assert!(g.log.is_empty());
    let json = serde_json::to_value(&g).unwrap();
    assert_eq!(json["stopped_reason"], "no_progress");
    let restored: Game = serde_json::from_value(json).unwrap();
    assert_eq!(restored.stopped_reason, g.stopped_reason);
    assert!(!restored.is_over());
}

#[test]
fn no_action_step_and_environment_done_agree_without_terminal_reward() {
    let cfg = empty_hero_lobby();
    // Construct immediately before the driver's advance, to reach the agent's own
    // all-false mask branch independently of the bot-seat stall branch below.
    let mut e = SoloEnv {
        game: Game::new(4601, cfg.clone()),
        agent: 0,
        cfg,
        illegal_penalty: -0.01,
    };
    assert!(!e.done());
    assert!(!e.legal_mask().iter().any(|v| *v));
    let result = e.step(A_END);
    assert!(result.done && e.done());
    assert_eq!(result.reward, 0.0);
    assert_eq!(e.game.stopped_reason, Some(StoppedReason::NoProgress));
    assert!(!e.game.is_over());
    assert_eq!(e.placement(), 0);
    let again = e.step(A_END);
    assert!(again.done && e.done());
    assert_eq!(again.reward, 0.0);
    assert!(again.illegal.is_some());
}

#[test]
fn solo_constructor_records_a_bot_seat_stall_and_reset_recomputes_it() {
    let mut e = SoloEnv::new(4602, empty_hero_lobby(), 0);
    assert!(e.done());
    assert_eq!(e.game.stopped_reason, Some(StoppedReason::NoProgress));
    assert!(!e.game.is_over());
    e.reset(4603);
    assert_eq!(e.game.seed, 4603);
    assert!(e.done());
    assert_eq!(e.game.stopped_reason, Some(StoppedReason::NoProgress));
}

#[test]
fn recruit_cutoff_counts_contexts_instead_of_every_rejected_emit() {
    let mut g = Game::new(
        4604,
        RulesConfig {
            num_players: 2,
            max_effect_depth: 1,
            ..RulesConfig::default()
        },
    );
    // Explicit guard fixture: the first event arrives at the configured depth cap.
    // Later rejected events in the same context must not inflate its diagnostic.
    for expected in [1, 2] {
        let mut c = Ctx::new(&mut g, [vec![], vec![]], [Some(0), None], false);
        c.depth = 1;
        for _ in 0..3 {
            c.emit(Event::CardAdded { side: 0, hid: 0 });
        }
        assert!(c.truncated);
        assert_eq!(c.g.truncated_recruit_contexts, expected);
    }
    assert!(g.stopped_reason.is_none());
    assert_eq!(g.truncated_combats, 0);
    let restored: Game = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
    assert_eq!(restored.truncated_recruit_contexts, 2);
}

#[test]
fn combat_cutoff_is_not_counted_as_a_recruit_context() {
    let mut g = Game::new(
        4605,
        RulesConfig {
            num_players: 2,
            max_effect_depth: 1,
            ..RulesConfig::default()
        },
    );
    let mut c = Ctx::new(&mut g, [vec![], vec![]], [Some(0), Some(1)], true);
    c.depth = 1;
    c.emit(Event::CardAdded { side: 0, hid: 0 });
    assert!(c.truncated);
    assert_eq!(c.g.truncated_recruit_contexts, 0);
}

#[test]
fn normal_state_omits_diagnostics_and_old_snapshots_default_them() {
    let g = Game::new(
        4606,
        RulesConfig {
            num_players: 2,
            ..RulesConfig::default()
        },
    );
    let json = serde_json::to_value(&g).unwrap();
    assert!(json.get("stopped_reason").is_none());
    assert!(json.get("truncated_recruit_contexts").is_none());
    let restored: Game = serde_json::from_value(json).unwrap();
    assert_eq!(restored.stopped_reason, None);
    assert_eq!(restored.truncated_recruit_contexts, 0);
}
