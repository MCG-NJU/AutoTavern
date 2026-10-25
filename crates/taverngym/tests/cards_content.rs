//! Card scripts, one case per behaviour.
use taverngym::actions::Action;
use taverngym::cards::{cid, db};
use taverngym::content;
use taverngym::script::Me;
use taverngym::state::{Item, Minion, Pending, ShopSlot};
use taverngym::types::Kw;
use taverngym::{Game, RulesConfig};

fn game(seed: u64) -> Game {
    let mut g = Game::new(
        seed,
        RulesConfig {
            experimental_mode: true,
            triple_copies: 99,
            ..Default::default()
        },
    );
    for p in 0..8 {
        g.apply(p, Action::Choose(0)).unwrap();
    }
    for p in &mut g.players {
        p.pending.clear();
        p.board.clear();
        p.hand.clear();
        p.trinkets.clear();
        p.rewards.clear();
        p.quests.clear();
        p.effects.clear();
        p.powers.clear();
        p.stats = Default::default();
        p.counters.clear();
        p.skipping = false;
        p.gold = 100;
        p.health = 100;
        p.set_hero_power(0, cid("TB_BaconShop_HP_035"));
    }
    g
}
fn add(g: &mut Game, id: &str) -> u32 {
    g.with_recruit(0, |c| {
        let m = c.new_minion(0, cid(id));
        let e = m.eid;
        c.boards[0].push(m);
        e
    })
}
fn give(g: &mut Game, id: &str) {
    g.with_recruit(0, |c| {
        c.give_card(0, cid(id));
    });
}
fn play_last(g: &mut Game) {
    let hand = g.players[0].hand.len() - 1;
    let pos = g.players[0].board.len();
    g.apply(0, Action::Play { hand, pos }).unwrap();
}
fn targeted_play(g: &mut Game, id: &str, target: u32) {
    give(g, id);
    play_last(g);
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
        panic!("target menu")
    };
    let i = options.iter().position(|e| *e == target).unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
}

#[test]
fn ruby_tusk_counts_the_hand_gem_not_nested_gems() {
    let mut g = game(61001);
    let target = add(&mut g, "BG31_327");
    g.with_recruit(0, |c| {
        content::grant_trinket(c, 0, cid("BG36_MagicItem_409"))
    });
    targeted_play(&mut g, "BG28_698", target);
    assert_eq!(
        g.with_recruit(0, |c| c.card_data(0, cid("BG36_MagicItem_409"))),
        0
    );
    targeted_play(&mut g, "BG20_GEM", target);
    assert_eq!(
        g.with_recruit(0, |c| c.card_data(0, cid("BG36_MagicItem_409"))),
        1
    );
}

#[test]
fn souvenir_shields_have_separate_combat_allowances() {
    for copies in [1, 2] {
        let mut g = game(61002);
        g.players[0].trinkets.push(cid("BG30_MagicItem_886"));
        if copies == 2 {
            g.players[0].trinkets.push(cid("BG30_MagicItem_886").copy());
        }
        g.with_recruit(0, |c| {
            c.combat = true;
            // Free board space between summons; the same combat allowance must persist.
            let mut shielded = 0;
            for _ in 0..12 {
                let m = c.new_minion(0, cid("BG26_146"));
                let e = m.eid;
                c.summon(0, 0, m);
                shielded += usize::from(c.minion(e).unwrap().has(Kw::DIVINE_SHIELD));
                c.boards[0].clear();
            }
            assert_eq!(shielded, 5 * copies);
        });
    }
}

#[test]
fn souvenir_fermenters_retain_both_starting_snapshots() {
    let mut g = game(61003);
    g.players[0].trinkets = vec![cid("BG35_MagicItem_732"), cid("BG35_MagicItem_732").copy()];
    for _ in 0..7 {
        add(&mut g, "BG26_146");
    }
    g.with_recruit(0, |c| {
        c.combat = true;
        c.run_player_hook(0, |s| s.start_of_combat, "test SoC");
        assert_eq!(c.boards[0].len(), 1);
        let stored: Vec<_> = c.cs[0].stored.values().collect();
        assert_eq!(stored.len(), 2);
        assert!(stored.iter().all(|v| v.len() == 3));
        assert_ne!(stored[0][0].eid, stored[1][0].eid);
    });
}

#[test]
fn each_cheese_wheel_improves_on_four_real_refreshes() {
    for copies in [1, 2] {
        let mut g = game(61004);
        g.with_recruit(0, |c| {
            content::grant_trinket(c, 0, cid("BG30_MagicItem_879t"));
            if copies == 2 {
                content::grant_trinket(c, 0, cid("BG30_MagicItem_879t").copy());
            }
        });
        assert_eq!(g.players[0].stats.tavern_atk, 2 * copies);
        for _ in 0..4 {
            g.apply(0, Action::Refresh).unwrap();
        }
        assert_eq!(
            (g.players[0].stats.tavern_atk, g.players[0].stats.tavern_hp),
            (4 * copies, 4 * copies)
        );
    }
}

#[test]
fn repeated_cube_sources_keep_their_own_replacements_after_save() {
    let mut g = game(61005);
    // Real grant/swap handlers; deterministic replacement identities isolate source state.
    g.with_recruit(0, |c| {
        content::grant_trinket(c, 0, cid("BG30_MagicItem_703"));
        c.player(0).unwrap().pending.clear();
        content::replace_trinket(c, 0, cid("BG30_MagicItem_703"), cid("BG30_MagicItem_879"));
        content::grant_trinket(c, 0, cid("BG30_MagicItem_703"));
        c.player(0).unwrap().pending.clear();
        content::replace_trinket(c, 0, cid("BG30_MagicItem_703"), cid("BG30_MagicItem_426"));
    });
    let encoded = serde_json::to_string(&g).unwrap();
    let mut g: Game = serde_json::from_str(&encoded).unwrap();
    g.with_recruit(0, |c| content::cube_turn_start(c, 0));
    let replacements: Vec<_> = g.players[0]
        .pending
        .iter()
        .filter_map(|p| match p {
            Pending::Trinket {
                free: true,
                replace: Some(old),
                ..
            } => Some(*old),
            _ => None,
        })
        .collect();
    assert_eq!(replacements.len(), 2);
    assert!(replacements.contains(&cid("BG30_MagicItem_879")));
    assert!(replacements.contains(&cid("BG30_MagicItem_426")));
}

#[test]
fn steady_gifts_keep_distinct_offering_stages_on_one_minion() {
    let mut g = game(61006);
    let eid = add(&mut g, "BG36_764");
    g.with_recruit(0, |c| {
        let m = c.minion_mut(eid).unwrap();
        m.attached.push(cid("BG36_MidGameEffect_000t51"));
        content::gift_init(m, cid("BG36_MidGameEffect_000t51"), 3);
        m.attached.push(cid("BG36_MidGameEffect_000t51"));
        content::gift_init(m, cid("BG36_MidGameEffect_000t51"), 4);
        assert_eq!((m.attachment_data_at(0), m.attachment_data_at(1)), (0, 1));
    });
    let view = taverngym::debug::view(&g, 0);
    for (i, numbers) in [(0, vec![2, 2]), (1, vec![2, 3])] {
        let gift = &view["me"]["board"][0]["attached"][i];
        assert_eq!(gift["live_text"], true);
        assert_eq!(
            taverngym::debug::text_numbers(gift["text"].as_str().unwrap()),
            numbers
        );
    }
    let before = (g.players[0].board[0].atk, g.players[0].board[0].max_hp);
    // The real turn dispatcher uses a board snapshot, rather than run_minion_hook.
    g.apply(0, Action::EndTurn).unwrap();
    let m = &g.players[0].board[0];
    assert_eq!((m.atk - before.0, m.max_hp - before.1), (4, 5));
}

#[test]
fn perfect_reborn_preserves_a_steady_gifts_offering_stage() {
    let mut g = game(61007);
    let eid = add(&mut g, "BG36_764");
    g.with_recruit(0, |c| {
        let m = c.minion_mut(eid).unwrap();
        m.attached.push(cid("BG36_MidGameEffect_000t51"));
        content::gift_init(m, cid("BG36_MidGameEffect_000t51"), 4);
        m.attach(cid("BG36_MidGameEffect_000t12"), 0);
        m.give(Kw::REBORN);
        c.destroy(eid);
        c.resolve_deaths();
        let m = &c.boards[0][0];
        let next = m.eid;
        let before = (m.atk, m.max_hp);
        assert_ne!(eid, next);
        assert_eq!(m.attachment_data_at(0), 1);
        c.run_board_hook(0, |s| s.end_of_turn, 1, "test EOT");
        let m = c.minion(next).unwrap();
        assert_eq!((m.atk - before.0, m.max_hp - before.1), (2, 3));
    });
}

#[test]
fn time_turning_uses_each_magnets_own_golden_form() {
    for (host, magnet, expected) in [("BG36_764", "BG26_146_G", 2), ("BG36_764_G", "BG26_146", 1)] {
        let mut g = game(61008);
        let eid = add(&mut g, host);
        g.with_recruit(0, |c| {
            let m = c.minion_mut(eid).unwrap();
            m.attach(cid(magnet), 0);
            m.attach(cid("BG36_MidGameEffect_000t21"), 0);
            let before = m.max_hp;
            c.run_minion_hook(0, eid, |s| s.end_of_turn, 1, "test EOT");
            let after_eot = c.minion(eid).unwrap().max_hp;
            c.run_minion_hook(0, eid, |s| s.start_of_turn, 1, "test SOT");
            assert_eq!(after_eot - before, expected);
            assert_eq!(c.minion(eid).unwrap().max_hp - after_eot, expected);
        });
    }
}

#[test]
fn upgrades_keep_purchase_history_and_free_use_without_raynor() {
    let mut g = game(61009);
    let bc = add(&mut g, "BG31_HERO_801pt");
    g.with_recruit(0, |c| {
        c.minion_mut(bc).unwrap().attach(cid("BG31_HERO_801ptf"), 0);
    });
    let mut spent = vec![];
    for id in ["BG31_HERO_801pth", "BG31_HERO_801pti"] {
        let card = cid(id);
        let sid = g.alloc_eid();
        g.players[0].shop = vec![ShopSlot::new(sid, Item::spell(card))];
        let before = g.players[0].gold;
        g.apply(0, Action::Buy(0)).unwrap();
        spent.push(before - g.players[0].gold);
        assert!(!g.with_recruit(0, |c| content::starcraft::upgrade_available(c, 0, card)));
    }
    assert_eq!(spent[0], 0);
    assert!(spent[1] > 0);
    g.players[0].powers.clear();
    g.players[0].board.clear();
    assert!(
        !g.with_recruit(0, |c| content::starcraft::upgrade_available(
            c,
            0,
            cid("BG31_HERO_801pth")
        ))
    );
    assert!(g.with_recruit(0, |c| content::starcraft::upgrade_available(
        c,
        0,
        cid("BG31_HERO_801pta")
    )));
}

#[test]
fn zerg_morph_updates_tier_and_only_keeps_granted_keywords() {
    for (source, target) in [
        ("BG31_HERO_811t3", "BG31_HERO_811t9"),
        ("BG31_HERO_811t3_G", "BG31_HERO_811t9_G"),
    ] {
        for granted in [false, true] {
            let mut g = game(61010);
            let eid = add(&mut g, source);
            g.with_recruit(0, |c| {
                assert!(c.minion(eid).unwrap().has(Kw::TAUNT));
                if granted {
                    c.give_kw(eid, Kw::TAUNT);
                }
                content::starcraft::morph(c, eid, cid(target));
                let m = c.minion(eid).unwrap();
                assert_eq!(m.tier, db().get(cid(target)).tier);
                assert_eq!(m.has(Kw::TAUNT), granted);
            });
        }
    }
}

#[test]
fn two_same_source_transform_discovers_retain_separate_targets_after_save() {
    for source in ["BG30_MagicItem_416t", "BG34_Treasure_933"] {
        let mut g = game(61011);
        let left = add(&mut g, "BG26_146");
        let right = add(&mut g, "BG26_147");
        g.with_recruit(0, |c| {
            c.cast_spell(0, cid(source), 0, left, 0);
            c.cast_spell(0, cid(source), 0, right, 0);
        });
        assert_eq!(g.players[0].discover_targets.len(), 2);
        let old = [g.players[0].board[0].card, g.players[0].board[1].card];
        let encoded = serde_json::to_string(&g).unwrap();
        let mut g: Game = serde_json::from_str(&encoded).unwrap();
        let first = match g.players[0].pending.last().unwrap() {
            Pending::Discover { options, .. } => options[0].card(),
            _ => panic!("discover"),
        };
        g.apply(0, Action::Choose(0)).unwrap();
        assert_eq!(g.players[0].board[0].card, old[0]);
        assert_eq!(g.players[0].board[1].card, first);
        let second = match g.players[0].pending.last().unwrap() {
            Pending::Discover { options, .. } => options[0].card(),
            _ => panic!("discover"),
        };
        g.apply(0, Action::Choose(0)).unwrap();
        assert_eq!(g.players[0].board[0].card, second);
        assert!(g.players[0].discover_targets.is_empty());
    }
}

#[test]
fn timewarped_evolution_takes_spell_bonus_as_well_as_target_context() {
    let mut g = game(61012);
    let eid = add(&mut g, "BG26_146");
    g.players[0].stats.spell_atk = 3;
    g.players[0].stats.spell_hp = 4;
    g.with_recruit(0, |c| c.cast_spell(0, cid("BG34_Treasure_933"), 0, eid, 0));
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(
        (g.players[0].board[0].atk, g.players[0].board[0].max_hp),
        (33, 34)
    );
}

#[test]
fn current_golden_growth_keeps_the_old_improvement_without_doubling_it() {
    for (id, counter) in [
        ("BG31_HERO_802pt", 1),
        ("BG31_HERO_802pt1", 1),
        ("BG34_Giant_088", 1),
        ("BG34_Giant_006", 2),
        ("BG34_Giant_091", 3),
    ] {
        let mut m = Minion::new(1, cid(id));
        m.counter = counter;
        content::make_golden(&mut m);
        assert_eq!(m.counter, counter, "{id}");
        assert!(
            taverngym::script::script(m.card)
                .unwrap()
                .has_flag(taverngym::script::flag::IMPROVES)
        );
    }
}

#[test]
fn large_golden_stats_saturate_before_becoming_negative_effects() {
    let mut g = game(61013);
    let slamma = add(&mut g, "BG26_802_G");
    g.with_recruit(0, |c| {
        c.combat = true;
        let mut m = c.new_minion(0, cid("BG19_010"));
        m.set_stats(1_200_000_000, 20);
        let eid = m.eid;
        c.summon(0, 1, m);
        assert!(c.minion(slamma).is_some());
        assert_eq!(c.minion(eid).unwrap().atk, i32::MAX);
    });
    let mut g = game(61014);
    let golden = db().golden_of(cid("BG36_360t9"));
    let eid = add(&mut g, &db().get(golden).id);
    g.with_recruit(0, |c| {
        c.minion_mut(eid)
            .unwrap()
            .set_stats(1_200_000_000, 1_200_000_000);
        c.trigger_deathrattles(0, eid);
        let golem = c.boards[0].iter().find(|m| m.eid != eid).unwrap();
        assert_eq!((golem.atk, golem.max_hp), (i32::MAX, i32::MAX));
    });
}

#[test]
fn golden_cthun_splits_in_wide_precision_before_per_target_saturation() {
    let mut g = game(61015);
    let eid = add(&mut g, "BGFYM_000_G");
    add(&mut g, "BG26_146");
    add(&mut g, "BG26_147");
    g.with_recruit(0, |c| {
        c.minion_mut(eid)
            .unwrap()
            .set_stats(1_200_000_000, 1_200_000_000);
        let before: Vec<_> = c.boards[0]
            .iter()
            .skip(1)
            .map(|m| (m.atk, m.max_hp))
            .collect();
        content::deity_awakened(
            c,
            Me {
                side: 0,
                eid,
                card: cid("BGFYM_000_G"),
                golden: true,
                opt: 0,
                pos: 0,
                data: 0,
            },
        );
        for (m, (a, h)) in c.boards[0].iter().skip(1).zip(before) {
            assert_eq!((m.atk - a, m.max_hp - h), (1_200_000_000, 1_200_000_000));
        }
    });
}

#[test]
fn volumizers_only_pay_the_lifetime_reward_once_after_real_bounce() {
    for (id, reward) in [
        ("BG34_170t", (3, 0)),
        ("BG34_170t2", (0, 3)),
        ("BG34_170t3", (1, 1)),
    ] {
        for magnetize in [false, true] {
            let mut g = game(61016);
            give(&mut g, id);
            play_last(&mut g);
            let eid = g.players[0].board[0].eid;
            let first = (
                g.players[0].stats.volumizer_atk,
                g.players[0].stats.volumizer_hp,
            );
            assert_eq!(first, reward);
            targeted_play(&mut g, "BGS_Treasures_034", eid);
            if magnetize {
                add(&mut g, "BG26_146");
                g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
            } else {
                play_last(&mut g);
            }
            assert_eq!(
                (
                    g.players[0].stats.volumizer_atk,
                    g.players[0].stats.volumizer_hp
                ),
                first
            );
        }
    }
}

#[test]
fn kthir_skips_locked_spells_before_taking_its_discard_quota() {
    for (id, quota) in [("BG36_320", 3), ("BG36_320_G", 6)] {
        let mut g = game(61017);
        let eid = add(&mut g, id);
        for _ in 0..=quota {
            give(&mut g, "BG28_503");
        }
        g.players[0].hand[0].locked = 1;
        let before = (g.players[0].board[0].atk, g.players[0].board[0].max_hp);
        g.apply(0, Action::EndTurn).unwrap();
        let m = g.players[0].board.iter().find(|m| m.eid == eid).unwrap();
        assert_eq!(
            (m.atk - before.0, m.max_hp - before.1),
            (7 * quota, 7 * quota)
        );
        assert_eq!(g.players[0].hand.len(), 1);
        assert_eq!(g.players[0].hand[0].locked, 1);
    }
}

#[test]
fn upgrades_ignore_tavern_spell_discounts_health_cost_and_extra_casts() {
    let mut g = game(61018);
    let bc = add(&mut g, "BG31_HERO_801pt");
    g.players[0].set_hero_power(0, cid("BG28_HERO_800p"));
    g.players[0].powers[0].counter = 2;
    g.players[0].rewards = vec![cid("BG28_Reward_500"), cid("BG28_Reward_501")];
    g.players[0].trinkets = vec![cid("BG32_MagicItem_822")];
    g.players[0].stats.spell_discount_next = 3;
    let card = cid("BG31_HERO_801pta");
    let price = db().get(card).cost;
    assert_eq!(g.spell_price(0, card), price);
    let sid = g.alloc_eid();
    g.players[0].shop = vec![ShopSlot::new(sid, Item::spell(card))];
    let gold = g.players[0].gold;
    let health = g.players[0].health;
    g.apply(0, Action::Buy(0)).unwrap();
    assert_eq!(g.players[0].gold, gold - price);
    assert_eq!(g.players[0].health, health);
    assert_eq!(g.players[0].stats.spell_discount_next, 3);
    assert_eq!(g.players[0].powers[0].counter, 2);
    let before = g.players[0].board.iter().find(|m| m.eid == bc).unwrap().atk;
    play_last(&mut g);
    assert_eq!(
        g.players[0].board.iter().find(|m| m.eid == bc).unwrap().atk - before,
        6
    );
}

#[test]
fn golden_improvements_use_new_base_plus_retained_growth_in_real_effects() {
    let mut g = game(61019);
    let eid = add(&mut g, "BG34_Giant_006");
    g.with_recruit(0, |c| {
        let m = c.minion_mut(eid).unwrap();
        m.counter = 1;
        content::make_golden(m);
    });
    let before = (g.players[0].board[0].atk, g.players[0].board[0].max_hp);
    give(&mut g, "BG36_180");
    play_last(&mut g);
    let m = &g.players[0].board[0];
    assert_eq!((m.atk - before.0, m.max_hp - before.1), (3, 3));
    assert_eq!(m.counter, 3);
    let mut g = game(61020);
    let eid = add(&mut g, "BG34_Giant_091");
    g.with_recruit(0, |c| {
        let m = c.minion_mut(eid).unwrap();
        m.counter = 3;
        content::make_golden(m);
        let mut enemy = c.new_minion(1, cid("BG26_146"));
        enemy.set_stats(0, 20);
        let target = enemy.eid;
        c.boards[1].push(enemy);
        c.combat = true;
        c.run_minion_hook(0, eid, |s| s.start_of_combat, 1, "test SoC");
        assert_eq!(
            c.minion(target).unwrap().hp,
            15,
            "own golden base2 + retained3 damage"
        );
    });
    let mut g = game(61021);
    let eid = add(&mut g, "BG31_HERO_802pt");
    g.with_recruit(0, |c| {
        let m = c.minion_mut(eid).unwrap();
        m.counter = 1;
        content::make_golden(m);
        for _ in 0..3 {
            let mut m = c.new_minion(1, cid("BG26_146"));
            m.set_stats(0, 20);
            c.boards[1].push(m);
        }
        let target = c.boards[1][1].eid;
        c.combat = true;
        c.trigger_rally(0, eid, target);
        assert_eq!(
            (c.boards[1][0].hp, c.boards[1][2].hp),
            (20, 20),
            "Colossus needs a real attack"
        );
        taverngym::combat::do_attack(c, 0, eid, target);
        assert_eq!((c.boards[1][0].hp, c.boards[1][2].hp), (15, 15));
    });
}

#[test]
fn bassgill_ranks_only_unlocked_and_not_previously_summoned_cards() {
    for (body, quota) in [("BG26_350", 1), ("BG26_350_G", 2)] {
        let mut g = game(61022);
        let bass = add(&mut g, body);
        for _ in 0..4 {
            give(&mut g, "BG36_764");
        }
        for (i, h) in g.players[0].hand.iter_mut().enumerate() {
            h.item.as_minion_mut().unwrap().set_stats(1, 100 - i as i32);
        }
        g.players[0].hand[0].locked = 1;
        let already = g.players[0].hand[1].hid;
        g.with_recruit(0, |c| {
            c.combat = true;
            c.cs[0].from_hand.push(already);
            c.trigger_deathrattles(0, bass);
            let got: Vec<_> = c.boards[0]
                .iter()
                .filter(|m| m.eid != bass)
                .map(|m| m.max_hp)
                .collect();
            assert_eq!(got, if quota == 1 { vec![98] } else { vec![98, 97] });
        });
    }
}

#[test]
fn the_last_timewarp_purchase_still_pauses_board_card_added_listeners() {
    for timewarp in [false, true] {
        let mut g = game(61023);
        add(&mut g, "BG34_Giant_327");
        add(&mut g, "BG26_135");
        let before: Vec<_> = g.players[0]
            .board
            .iter()
            .map(|m| (m.atk, m.max_hp))
            .collect();
        let item = g.with_recruit(0, |c| Item::Minion(c.new_minion(0, cid("BG36_764"))));
        if timewarp {
            // A controlled one-item final offer: selecting it leaves no pending visit.
            g.players[0].pending.push(Pending::Timewarp {
                options: vec![(item, 1)],
                chronum: 1,
                major: true,
            });
            g.apply(0, Action::Choose(0)).unwrap();
            assert!(
                !g.players[0]
                    .pending
                    .iter()
                    .any(|p| matches!(p, Pending::Timewarp { .. }))
            );
        } else {
            let sid = g.alloc_eid();
            g.players[0].shop = vec![ShopSlot::new(sid, item)];
            g.apply(0, Action::Buy(0)).unwrap();
        }
        let expected = if timewarp { 0 } else { 1 };
        for (m, (a, h)) in g.players[0].board.iter().zip(before) {
            assert_eq!((m.atk - a, m.max_hp - h), (expected, expected));
        }
    }
}

#[test]
fn numbered_spell_bonuses_reach_setters_deities_and_future_undead() {
    for (a, h) in [(3i32, 4i32), (i32::MAX, i32::MAX)] {
        let mut g = game(61024);
        let target = add(&mut g, "BG26_146");
        g.players[0].stats.spell_atk = a;
        g.players[0].stats.spell_hp = h;
        targeted_play(&mut g, "BG28_838", target);
        assert_eq!(
            (g.players[0].board[0].atk, g.players[0].board[0].max_hp),
            (a.saturating_add(20), h.saturating_add(20))
        );
        g.players[0].deity = Some(taverngym::state::DeityState {
            card: cid("BGFYM_000"),
            atk: 1,
            hp: 1,
            golden: false,
            reborn: false,
        });
        give(&mut g, "BG36_371");
        play_last(&mut g);
        let god = g.players[0].deity.as_ref().unwrap();
        assert_eq!(
            (god.atk, god.hp),
            (a.saturating_add(8), h.saturating_add(8))
        );
        let undead = add(&mut g, "BG25_008");
        targeted_play(&mut g, "BG28_604", undead);
        assert_eq!(
            (g.players[0].stats.undead_atk, g.players[0].stats.undead_hp),
            (a.saturating_add(8), h)
        );
        let future = g.with_recruit(0, |c| c.new_minion(0, cid("BG25_009")));
        assert_eq!(future.pbonus_hp, h);
    }
}

#[test]
fn patient_scout_triple_retains_the_highest_tier_not_the_sum() {
    let mut g = game(61025);
    g.cfg.triple_copies = 3;
    let first = add(&mut g, "BG24_715");
    let second = add(&mut g, "BG24_715");
    g.with_recruit(0, |c| {
        c.minion_mut(first).unwrap().counter = 2;
        c.minion_mut(second).unwrap().counter = 4;
    });
    give(&mut g, "BG24_715");
    assert!(g.players[0].board.is_empty());
    let card = g.players[0]
        .hand
        .iter()
        .find_map(|h| h.item.as_minion())
        .unwrap();
    assert!(card.golden);
    assert_eq!(card.counter, 4);
}

#[test]
fn paused_amalgam_gains_are_not_replayed_when_timewarp_ends() {
    for temporary in [false, true] {
        for golden in [false, true] {
            let mut g = game(61026);
            let card = cid("TB_BaconShop_HERO_33_Buddy");
            let watcher = if golden { db().golden_of(card) } else { card };
            let mishmash = add(&mut g, &db().get(watcher).id);
            let amalgam = add(&mut g, "TB_BaconShop_HP_033t");
            // Establish the watcher's active baseline before the controlled visit.
            g.with_recruit(0, |c| c.buff(amalgam, 0, 0));
            let before = g.players[0]
                .board
                .iter()
                .find(|m| m.eid == mishmash)
                .map(|m| (m.atk, m.max_hp))
                .unwrap();
            let item = g.with_recruit(0, |c| Item::Minion(c.new_minion(0, cid("BG36_764"))));
            g.players[0].pending.push(Pending::Timewarp {
                options: vec![(item, 1)],
                chronum: 1,
                major: true,
            });
            // The gain is a controlled stat-source fixture, not a claim about a new
            // natural Timewarp producer. Exiting uses the real final-purchase action.
            g.with_recruit(0, |c| {
                if temporary {
                    c.buff_temp(amalgam, 10, 11);
                } else {
                    c.buff(amalgam, 10, 11);
                }
                let m = c.minion(mishmash).unwrap();
                assert_eq!((m.atk, m.max_hp), before);
            });
            g.apply(0, Action::Choose(0)).unwrap();
            assert!(
                !g.players[0]
                    .pending
                    .iter()
                    .any(|p| matches!(p, Pending::Timewarp { .. }))
            );
            g.with_recruit(0, |c| {
                let m = c.minion(mishmash).unwrap();
                assert_eq!((m.atk, m.max_hp), before, "no catch-up after resuming");
                c.buff(amalgam, 2, 3);
                let m = c.minion(mishmash).unwrap();
                let factor = if golden { 2 } else { 1 };
                assert_eq!(
                    (m.atk, m.max_hp),
                    (before.0 + 2 * factor, before.1 + 3 * factor)
                );
            });
        }
    }
}
