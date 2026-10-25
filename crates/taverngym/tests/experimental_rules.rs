mod common;
use common::*;
use taverngym::experimental::{BuddyButton, ExtraAction};

fn experiment(hero: &str) -> Game {
    let cfg = RulesConfig {
        experimental_mode: true,
        num_players: 2,
        hero_pool: vec![hero.into()],
        hero_choices: 1,
        guaranteed_heroes: vec![],
        ..RulesConfig::default()
    };
    cfg.validate().unwrap();
    let mut g = Game::new(77, cfg);
    for p in 0..2 {
        g.apply(p, Action::Choose(0)).unwrap();
    }
    g
}
const PATCH: &str = "TB_BaconShop_HERO_34";

#[test]
fn experimental_hero_pool_has_only_usable_buddies_and_keeps_bans() {
    for seed in 0..30 {
        let g = Game::new(
            seed,
            RulesConfig {
                experimental_mode: true,
                ..RulesConfig::default()
            },
        );
        for pl in &g.players {
            let Pending::HeroSelect { options } = pl.pending.last().unwrap() else {
                panic!()
            };
            assert!(
                options
                    .iter()
                    .all(|h| taverngym::experimental::hero_allowed(db().get(*h)))
            );
        }
    }
    for h in ["BG34_HERO_000", "BG36_HERO_105", "TB_BaconShop_HERO_56"] {
        assert!(
            RulesConfig {
                experimental_mode: true,
                hero_pool: vec![h.into()],
                ..RulesConfig::default()
            }
            .validate()
            .is_err()
        );
    }
    assert!(taverngym::script::script(cid("BG23_HERO_306_Buddy")).is_some());
    let g = experiment(PATCH);
    assert!(g.cfg.timewarp_enabled);
}

#[test]
fn fourth_turn_offers_three_distinct_quests_to_everyone_once() {
    let mut g = experiment(PATCH);
    g.turn = 3;
    g.start_turn();
    for p in 0..2 {
        let Pending::Quest { options, .. } = g.players[p].pending.last().unwrap() else {
            panic!()
        };
        assert_eq!(options.len(), 3);
        let mut qs = options.iter().map(|x| x.0).collect::<Vec<_>>();
        qs.sort();
        qs.dedup();
        assert_eq!(qs.len(), 3);
        assert!(g.apply(p, Action::Refresh).is_err());
        g.apply(p, Action::Choose(0)).unwrap();
        assert_eq!(g.players[p].quests.len(), 1);
    }
    g.start_turn();
    assert!(g.players.iter().all(|p| p.pending.is_empty()));
}

#[test]
fn denathrius_keeps_opening_quest_and_gets_an_additional_one() {
    let mut g = experiment("BG24_HERO_100");
    for p in 0..2 {
        let Pending::Quest { options, .. } = g.players[p].pending.last().unwrap() else {
            panic!()
        };
        assert_eq!(options.len(), 2);
        g.apply(p, Action::Choose(0)).unwrap();
    }
    let old = g.players[0].quests[0].clone();
    g.turn = 3;
    g.start_turn();
    let Pending::Quest { options, .. } = g.players[0].pending.last().unwrap() else {
        panic!()
    };
    assert_eq!(options.len(), 3);
    assert!(options.iter().all(|x| x.0 != old.card && x.1 != old.reward));
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].quests.len(), 2);
    assert_eq!(g.players[0].quests[0], old);
}

#[test]
fn buddy_prices_discount_once_and_reset_after_each_claim() {
    let mut g = experiment(PATCH);
    let card = g.players[0].buddy_button.as_ref().unwrap().card;
    let tier = db().get(card).tier as i32;
    assert_eq!(
        g.players[0].buddy_button.as_ref().unwrap().price(),
        9 + 2 * tier
    );
    g.discount_buddy(0, false);
    g.discount_buddy(0, false);
    assert_eq!(g.players[0].buddy_button.as_ref().unwrap().discount, 3);
    g.turn += 1;
    g.discount_buddy(0, true);
    assert_eq!(g.players[0].buddy_button.as_ref().unwrap().discount, 5);
    g.players[0].gold = 99;
    g.claim_buddy(0, 0).unwrap();
    assert_eq!(g.players[0].hand.len(), 1);
    assert_eq!(
        g.players[0].buddy_button.as_ref().unwrap().price(),
        11 + 2 * tier
    );
    let gold = g.players[0].gold;
    assert!(g.claim_buddy(0, 0).is_err());
    assert_eq!(g.players[0].gold, gold);
    // A second claim is allowed in the same turn, with no artificial cooldown.
    g.claim_buddy(0, 1).unwrap();
    assert_eq!(g.players[0].buddy_button.as_ref().unwrap().claims, 2);
    assert!(g.claim_buddy(0, 2).is_err());
    assert!(
        g.players[0]
            .hand
            .iter()
            .any(|h| h.item.as_minion().is_some_and(|m| m.golden))
    );
}

#[test]
fn buddy_claim_is_blocked_by_choices_and_full_hand_delivery_is_protected() {
    let mut g = experiment(PATCH);
    g.players[0].gold = 99;
    for _ in 0..10 {
        give(&mut g, 0, "BG28_810");
    }
    g.players[0].pending.push(Pending::Quest {
        options: vec![],
        types: vec![],
    });
    assert!(g.claim_buddy(0, 0).is_err());
    g.players[0].pending.clear();
    g.claim_buddy(0, 0).unwrap();
    g.claim_buddy(0, 1).unwrap();
    assert_eq!(g.players[0].hand.len(), 10);
    assert_eq!(g.players[0].hand_queue.len(), 3);
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert_eq!(g.players[0].hand_queue.len(), 2);
}

#[test]
fn final_combat_settles_both_seats_and_ghost_but_simulation_does_not() {
    let mut g = experiment(PATCH);
    let out = g.simulate_combat(0, Some(1), None);
    assert_eq!(g.players[0].buddy_button.as_ref().unwrap().discount, 0);
    g.apply_combat_result(0, Some(1), out);
    g.apply_combat_result(0, Some(1), out);
    assert_eq!(g.players[0].buddy_button.as_ref().unwrap().discount, 3);
    assert_eq!(g.players[1].buddy_button.as_ref().unwrap().discount, 3);
    g.turn += 1;
    let out = g.simulate_combat(0, None, Some((vec![], 1)));
    g.apply_combat_result(0, None, out);
    assert_eq!(g.players[0].buddy_button.as_ref().unwrap().discount, 6);
}

#[test]
fn timewarp_balance_survives_exit_and_investment_pays_next_visit() {
    let mut g = experiment(PATCH);
    g.with_recruit(0, |c| taverngym::content::timewarp::open(c, 0, false));
    let n = g.players[0].pending.last().unwrap().len() - 1;
    g.apply(0, Action::Choose(n)).unwrap();
    assert_eq!(g.players[0].chronum_balance, 2);
    g.players[0].chronum_saved = 1;
    g.with_recruit(0, |c| taverngym::content::timewarp::open(c, 0, true));
    let Pending::Timewarp { chronum, .. } = g.players[0].pending.last().unwrap() else {
        panic!()
    };
    assert_eq!(*chronum, 3);
    assert_eq!(g.players[0].chronum_saved, 0);
    let n = g.players[0].pending.last().unwrap().len() - 1;
    g.apply(0, Action::Choose(n)).unwrap();
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(g.players[0].chronum_balance, 0);
}

#[test]
fn purchases_expand_actual_hand_and_extra_actions_can_play_eleventh_card() {
    let mut g = experiment(PATCH);
    for _ in 0..10 {
        give(&mut g, 0, "BG28_810");
    }
    let item = Item::Minion(minion(&mut g, 0, "BG34_Giant_006"));
    g.players[0].pending.push(Pending::Timewarp {
        options: vec![(item, 1)],
        chronum: 2,
        major: false,
    });
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].hand.len(), 11);
    assert!(g.players[0].hand_queue.is_empty());
    let hid = g.players[0].hand[10].hid;
    let view = taverngym::debug::view(&g, 0);
    assert_eq!(view["me"]["hand"][10]["extra_play_positions"][0], 0);
    assert!(taverngym::invariants::check(&g).is_empty());
    g.extra_action(0, ExtraAction::Play { hid, pos: 0 })
        .unwrap();
    assert_eq!(g.players[0].board.len(), 1);
    assert_eq!(g.players[0].hand.len(), 10);
    assert_eq!(g.players[0].timewarp_hand_limit, 0);
    assert!(
        g.extra_action(0, ExtraAction::Play { hid, pos: 0 })
            .is_err()
    );
}

#[test]
fn tavern_visits_block_actions_and_suspend_ordinary_stat_watchers() {
    let mut g = experiment(PATCH);
    set_board(&mut g, 0, &["BG34_Giant_064", "BG36_345"]);
    let e = g.players[0].board[1].eid;
    let old = g.players[0].board[1].hp;
    g.with_recruit(0, |c| taverngym::content::timewarp::open(c, 0, false));
    for a in [
        Action::Refresh,
        Action::Freeze,
        Action::Sell(0),
        Action::HeroPower(0),
        Action::EndTurn,
    ] {
        assert!(g.apply(0, a).is_err());
    }
    g.with_recruit(0, |c| c.buff(e, 1, 0));
    assert_eq!(g.players[0].board[1].hp, old);
    let n = g.players[0].pending.last().unwrap().len() - 1;
    g.apply(0, Action::Choose(n)).unwrap();
    g.with_recruit(0, |c| c.buff(e, 1, 0));
    assert_eq!(g.players[0].board[1].hp, old + 3);
}

#[test]
fn timewarp_takes_precedence_over_retained_turn_six_trinket() {
    let mut g = experiment(PATCH);
    g.turn = 5;
    g.start_turn();
    assert!(matches!(
        g.players[0].pending.last(),
        Some(Pending::Timewarp { major: false, .. })
    ));
    assert!(
        g.players[0]
            .pending
            .iter()
            .any(|x| matches!(x, Pending::Trinket { .. }))
    );
    let n = g.players[0].pending.last().unwrap().len() - 1;
    g.apply(0, Action::Choose(n)).unwrap();
    assert!(matches!(
        g.players[0].pending.last(),
        Some(Pending::Trinket { .. })
    ));
}

#[test]
fn normal_and_timewarped_versions_do_not_form_a_mixed_triple() {
    let mut g = experiment(PATCH);
    for id in ["BGS_127", "BGS_127", "BG34_Giant_006"] {
        give(&mut g, 0, id);
    }
    assert_eq!(g.players[0].hand.len(), 3);
    assert!(
        g.players[0]
            .hand
            .iter()
            .all(|h| h.item.as_minion().is_some_and(|m| !m.golden))
    );
}

#[test]
fn serialization_restores_claim_currency_and_overflow_and_defaults_old_states() {
    let mut g = experiment(PATCH);
    g.players[0].chronum_balance = 1;
    g.players[0].timewarp_hand_limit = 12;
    g.players[0].buddy_button.as_mut().unwrap().claims = 1;
    let raw = serde_json::to_string(&g).unwrap();
    let restored: Game = serde_json::from_str(&raw).unwrap();
    assert_eq!(restored.players, g.players);
    let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    for p in value["players"].as_array_mut().unwrap() {
        for k in [
            "buddy_button",
            "chronum_balance",
            "chronum_granted_turn",
            "timewarp_hand_limit",
        ] {
            p.as_object_mut().unwrap().remove(k);
        }
    }
    let old: Game = serde_json::from_value(value).unwrap();
    assert!(old.players[0].buddy_button.is_none());
    assert_eq!(old.players[0].chronum_balance, 0);
}

#[test]
fn golden_nathanos_sells_then_splits_double_stats_to_adjacent_minions() {
    for (id, mult) in [("BG23_HERO_306_Buddy", 1), ("BG23_HERO_306_Buddy_G", 2)] {
        let mut g = experiment(PATCH);
        g.cfg.triple_copies = 99;
        set_board(&mut g, 0, &["BG36_345", "BG36_345", "BG36_345"]);
        g.players[0].board[1].set_stats(5, 7);
        let target = g.players[0].board[1].eid;
        let gold = g.players[0].gold;
        give(&mut g, 0, id);
        play_last(&mut g, 0);
        let Pending::Target { options, .. } = g.players[0].pending.last().unwrap() else {
            panic!()
        };
        let i = options.iter().position(|e| *e == target).unwrap();
        g.apply(0, Action::Choose(i)).unwrap();
        assert_eq!(g.players[0].gold, gold + 1);
        // Each split rounds down. The normal 5/7
        // target gives each neighbor +2/+3; golden doubles first, giving +5/+7.
        assert_eq!(g.players[0].board.len(), 3);
        assert!(g.players[0].board.iter().all(|m| m.eid != target));
        for m in &g.players[0].board[..2] {
            assert_eq!((m.atk, m.hp), (3 + (5 * mult) / 2, 3 + (7 * mult) / 2));
        }
    }
}

#[test]
fn baseline_keeps_default_contract_and_experimental_actions_are_opt_in() {
    // This flag is omitted from the default JSON, but remains a supported public
    // configuration key.
    let cfg =
        RulesConfig::from_json(r#"{"experimental_mode":true,"timewarp_enabled":true}"#).unwrap();
    assert!(cfg.experimental_mode && cfg.timewarp_enabled);
    assert!(RulesConfig::from_json(r#"{"experimental_mdoe":true}"#).is_err());
    let mut g = game(2);
    assert!(!g.cfg.experimental_mode);
    assert!(!g.cfg.timewarp_enabled);
    assert!(
        serde_json::to_value(RulesConfig::default())
            .unwrap()
            .get("experimental_mode")
            .is_none()
    );
    assert!(
        g.extra_action(0, ExtraAction::ClaimBuddy { expected_claims: 0 })
            .is_err()
    );
    let b = BuddyButton::new(cid("BG23_HERO_306_Buddy"));
    assert_eq!(b.price(), 17);
}

#[test]
fn minor_visit_includes_second_power_and_offers_have_fixed_shape() {
    let mut g = experiment(PATCH);
    g.cfg.triple_copies = 99;
    set_board(
        &mut g,
        0,
        &[
            "BG34_Giant_007",
            "BG34_Giant_302",
            "BG34_Giant_597",
            "BG34_Giant_598",
        ],
    );
    g.lobby_races = g.lobby_races.union(Races::MECH);
    for major in [false, true] {
        for _ in 0..100 {
            let offer = g.with_recruit(0, |c| taverngym::content::timewarp::offer(c, 0, major));
            assert_eq!(offer.len(), 5);
            assert_eq!(
                offer
                    .iter()
                    .filter(|x| matches!(x.0, Item::Minion(_)))
                    .count(),
                3
            );
            assert_eq!(offer.iter().filter(|x| x.1 == 1).count(), 3);
            assert!(offer.windows(2).all(|x| x[0].1 <= x[1].1));
            assert_eq!(
                offer
                    .iter()
                    .filter(|x| db().get(x.0.card()).id.starts_with("BG34_HeroPowerSpell_"))
                    .count(),
                usize::from(!major)
            );
            assert!(
                offer
                    .iter()
                    .filter(|x| matches!(
                        db().get(x.0.card()).id.as_str(),
                        "BG34_Giant_597" | "BG34_Giant_598" | "BG34_Treasure_994"
                    ))
                    .count()
                    <= 1
            );
        }
    }
    g.players[0].set_hero_power(1, cid("TB_BaconShop_HP_001"));
    let offer = g.with_recruit(0, |c| taverngym::content::timewarp::offer(c, 0, false));
    assert!(
        offer
            .iter()
            .all(|x| !db().get(x.0.card()).id.starts_with("BG34_HeroPowerSpell_"))
    );
}

#[test]
fn buddy_purchase_counts_gold_spent_and_external_buddies_leave_button_unchanged() {
    let mut g = experiment(PATCH);
    let reward = db()
        .iter()
        .find(|d| {
            d.kind == taverngym::cards::CardKind::QuestReward
                && taverngym::content::quest_offerable(g.lobby_races, &d.id)
        })
        .unwrap()
        .cid;
    g.with_recruit(0, |c| c.start_quest(0, cid("BG24_Quest_126"), reward));
    g.players[0].quests[0].goal = 1000;
    g.players[0].gold = 99;
    let price = g.players[0].buddy_button.as_ref().unwrap().price();
    let card = g.players[0].buddy_button.as_ref().unwrap().card;
    g.with_recruit(0, |c| c.give_card(0, card));
    assert_eq!(g.players[0].buddy_button.as_ref().unwrap().claims, 0);
    g.claim_buddy(0, 0).unwrap();
    assert_eq!(g.players[0].quests[0].progress, price);
    assert_eq!(g.players[0].stats.gold_spent_total, price);
}

#[test]
fn all_bot_experiments_finish_without_structural_errors() {
    for seed in 0..12 {
        let mut g = Game::new(
            seed,
            RulesConfig {
                experimental_mode: true,
                max_turns: 20,
                ..RulesConfig::default()
            },
        );
        for _ in 0..200 {
            for p in 0..8 {
                taverngym::bot::play_turn(&mut g, p);
            }
            let violations = taverngym::invariants::check(&g);
            assert!(
                violations.is_empty(),
                "seed {seed} turn {}: {violations:?}",
                g.turn
            );
            if g.phase == taverngym::engine::Phase::GameOver {
                break;
            }
        }
        assert_eq!(g.phase, taverngym::engine::Phase::GameOver);
    }
}

#[test]
fn full_hand_direct_purchase_triples_from_board_without_losing_the_golden() {
    let mut g = experiment(PATCH);
    set_board(&mut g, 0, &["BG34_Giant_006", "BG34_Giant_006"]);
    for _ in 0..10 {
        give(&mut g, 0, "BG28_810");
    }
    let item = Item::Minion(minion(&mut g, 0, "BG34_Giant_006"));
    g.players[0].pending.push(Pending::Timewarp {
        options: vec![(item, 1)],
        chronum: 2,
        major: false,
    });
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].board.is_empty());
    assert_eq!(g.players[0].hand.len(), 11);
    assert!(g.players[0].hand[10].item.as_minion().unwrap().golden);
    assert!(
        g.with_recruit(0, |c| c.give_card(0, cid("BG28_810")))
            .is_none()
    );
    assert_eq!(g.players[0].hand.len(), 11);
}

#[test]
fn buddy_price_table_zero_floor_and_insufficient_gold() {
    for (tier, first, second) in [
        (1, 11, 13),
        (2, 13, 15),
        (3, 15, 17),
        (4, 17, 19),
        (5, 19, 21),
        (6, 21, 23),
    ] {
        let card = db()
            .iter()
            .find(|d| d.id.ends_with("_Buddy") && d.tier == tier)
            .unwrap()
            .cid;
        let mut b = BuddyButton::new(card);
        assert_eq!(b.price(), first);
        b.claims = 1;
        assert_eq!(b.price(), second);
        b.discount = 100;
        assert_eq!(b.price(), 0);
    }
    let mut g = experiment(PATCH);
    let old = g.players[0].clone();
    assert!(g.claim_buddy(0, 0).is_err());
    assert_eq!(g.players[0], old);
    g.players[0].buddy_button.as_mut().unwrap().discount = 100;
    g.players[0].gold = 0;
    g.claim_buddy(0, 0).unwrap();
    assert_eq!(g.players[0].gold, 0);
}

#[test]
fn changing_and_adding_powers_preserves_buddy_button_and_claim_count() {
    let mut g = experiment(PATCH);
    g.players[0].gold = 99;
    g.claim_buddy(0, 0).unwrap();
    let original = g.players[0].buddy_button.clone();
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_001"));
    g.set_hero_power(0, 1, cid("TB_BaconShop_HP_011"));
    assert_eq!(g.players[0].buddy_button, original);
}

#[test]
fn cast_on_buy_investment_avoids_hand_and_pays_next_actual_visit() {
    let mut g = experiment(PATCH);
    for _ in 0..10 {
        give(&mut g, 0, "BG28_810");
    }
    g.with_recruit(0, |c| taverngym::content::timewarp::open(c, 0, false));
    let Pending::Timewarp { options, .. } = g.players[0].pending.last_mut().unwrap() else {
        panic!()
    };
    *options = vec![(Item::spell(cid("BG34_Treasure_300")), 1)];
    let gold = g.players[0].gold;
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].hand.len(), 10);
    assert_eq!(g.players[0].gold, gold);
    assert_eq!(g.players[0].chronum_saved, 1);
    assert_eq!(g.players[0].chronum_balance, 1);
    g.with_recruit(0, |c| taverngym::content::timewarp::open(c, 0, true));
    let Pending::Timewarp { chronum, .. } = g.players[0].pending.last().unwrap() else {
        panic!()
    };
    assert_eq!(*chronum, 2);
    assert_eq!(g.players[0].chronum_saved, 0);
}

#[test]
fn purchased_turn_start_minion_does_not_replay_turn_start() {
    let mut g = experiment(PATCH);
    let item = Item::Minion(minion(&mut g, 0, "BG34_PreMadeChamp_076"));
    g.players[0].pending.push(Pending::Timewarp {
        options: vec![(item, 1)],
        chronum: 2,
        major: true,
    });
    g.apply(0, Action::Choose(0)).unwrap();
    play_last(&mut g, 0);
    assert!(g.players[0].hand.is_empty());
    g.start_turn();
    assert_eq!(g.players[0].hand.len(), 5);
}

#[test]
fn silent_turn_end_shrinks_capacity_after_temporary_cards_expire() {
    let mut g = experiment(PATCH);
    for _ in 0..10 {
        give(&mut g, 0, "BG28_810");
    }
    let item = Item::Minion(minion(&mut g, 0, "BG34_Giant_006"));
    g.with_recruit(0, |c| c.add_timewarp_purchase(0, item));
    g.players[0].hand[0].temporary = true;
    g.players[0].hand[1].temporary = true;
    g.end_turn_silent(0);
    assert_eq!(g.players[0].hand.len(), 9);
    assert_eq!(g.players[0].timewarp_hand_limit, 0);
}
