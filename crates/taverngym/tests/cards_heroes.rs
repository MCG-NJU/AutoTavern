//! Hero, Quest and Buddy rules.
//! Fixtures install the stated cards/stats; actions and shared hook dispatch are real.

use std::collections::BTreeSet;

use taverngym::actions::{A_HERO_POWER, Action};
use taverngym::cards::{CardId, cid, db};
use taverngym::engine::Ctx;
use taverngym::script::{Me, Shown, script};
use taverngym::state::{DeityState, Item, Pending, ShopSlot};
use taverngym::types::{EntityId, Kw, Races};
use taverngym::{Game, RulesConfig};

fn game(seed: u64) -> Game {
    let noz = db()
        .iter()
        .find(|d| d.hero_power == Some(cid("TB_BaconShop_HP_063")))
        .unwrap();
    let mut g = Game::new(
        seed,
        RulesConfig {
            experimental_mode: true,
            num_players: 2,
            hero_choices: 1,
            hero_pool: vec!["TB_BaconShop_HERO_34".into(), noz.id.clone()],
            lobby_types: vec![
                "PIRATE".into(),
                "MECHANICAL".into(),
                "DRAGON".into(),
                "ABERRATION".into(),
                "QUILBOAR".into(),
            ],
            timewarp_enabled: true,
            timewarp_turns: [0, 0],
            lesser_trinket_turn: 0,
            greater_trinket_turn: 0,
            ..RulesConfig::default()
        },
    );
    for p in 0..2 {
        g.apply(p, Action::Choose(0)).unwrap();
    }
    for p in 0..2 {
        let pl = &mut g.players[p];
        pl.board.clear();
        pl.hand.clear();
        pl.shop.clear();
        pl.pending.clear();
        pl.rewards.clear();
        pl.quests.clear();
        pl.effects.clear();
        pl.trinkets.clear();
        pl.counters.clear();
        pl.stats = Default::default();
        pl.gold = 99;
        pl.health = 100;
        pl.armor = 0;
        pl.powers.clear();
        pl.set_hero_power(0, cid("TB_BaconShop_HP_063"));
    }
    g
}

fn board(g: &mut Game, p: usize, ids: &[&str]) {
    g.players[p].board = g.with_recruit(p, |c| {
        ids.iter().map(|id| c.new_minion(0, cid(id))).collect()
    });
}

fn give(g: &mut Game, id: &str) -> EntityId {
    g.with_recruit(0, |c| c.give_card(0, cid(id)).unwrap())
}

fn me(card: CardId, eid: EntityId, golden: bool) -> Me {
    Me {
        side: 0,
        eid,
        card,
        golden,
        opt: 0,
        pos: 0,
        data: 0,
    }
}

fn combat(g: &mut Game) -> Ctx<'_> {
    let boards = [g.players[0].board.clone(), g.players[1].board.clone()];
    Ctx::new(g, boards, [Some(0), Some(1)], true)
}

fn stock(g: &mut Game, id: &str, stats: Option<(i32, i32)>) {
    let mut m = g.with_recruit(0, |c| c.new_minion(0, cid(id)));
    if let Some((a, h)) = stats {
        m.set_stats(a, h);
    }
    g.players[0].shop = vec![ShopSlot::new(m.eid, Item::Minion(m))];
}

#[test]
fn timewarp_powers_keep_printed_visit_turn_and_dynamic_countdown() {
    for (id, acquired, visit, major) in [
        ("BG34_HERO_004p", 4, 5, false),
        ("BG34_HERO_000p", 7, 8, true),
    ] {
        let mut g = game(12001);
        g.turn = acquired;
        g.set_hero_power(0, 0, cid(id));
        assert_eq!(g.players[0].powers[0].counter, visit as i32);
        g.with_recruit(0, |c| {
            let sh = Shown {
                side: 0,
                card: cid(id),
                golden: false,
                minion: None,
                data: 0,
            };
            assert_eq!(
                script(cid(id)).unwrap().shown.unwrap()(c, &sh),
                vec![visit as i32, 1]
            );
        });
        g.start_turn();
        assert_eq!(g.turn, visit);
        assert!(
            g.players[0]
                .pending
                .iter()
                .any(|p| matches!(p, Pending::Timewarp { major: m, .. } if *m == major))
        );
        // Acquiring the same power after its fixed turn does not schedule a later visit.
        let mut late = game(12002);
        late.turn = visit + 1;
        late.set_hero_power(0, 0, cid(id));
        late.start_turn();
        assert!(
            !late.players[0]
                .pending
                .iter()
                .any(|p| matches!(p, Pending::Timewarp { .. }))
        );
    }
}

#[test]
fn shredder_excludes_locked_cards_before_ranking_and_randomizes_health_ties() {
    for golden in [false, true] {
        let mut outcomes = BTreeSet::new();
        for seed in 0..24 {
            let mut g = game(12100 + seed);
            board(
                &mut g,
                0,
                &[if golden {
                    "BG21_HERO_030t_G"
                } else {
                    "BG21_HERO_030t"
                }],
            );
            for id in ["BG26_146", "BG36_345", "BG24_715", "BG34_145"] {
                give(&mut g, id);
            }
            for h in &mut g.players[0].hand {
                h.item.as_minion_mut().unwrap().set_stats(1, 10);
            }
            g.players[0].hand[3].locked = 1;
            g.players[0].hand[3]
                .item
                .as_minion_mut()
                .unwrap()
                .set_stats(1, 1000);
            let locked = g.players[0].hand[3].hid;
            let mut c = combat(&mut g);
            let dead = c.boards[0][0].eid;
            c.destroy(dead);
            c.resolve_deaths();
            assert_eq!(c.cs[0].from_hand.len(), if golden { 2 } else { 1 });
            assert!(!c.cs[0].from_hand.contains(&locked));
            assert!(c.boards[0].iter().all(|m| m.has(Kw::DIVINE_SHIELD)));
            outcomes.insert(c.cs[0].from_hand.clone());
        }
        assert!(
            outcomes.len() > 1,
            "equal Health must not always prefer the first hand slot"
        );
    }
}

#[test]
fn fish_copies_acquired_deathrattles_with_ordinary_and_golden_multiplicity() {
    for golden in [false, true] {
        let mut g = game(12200);
        board(
            &mut g,
            0,
            &[
                if golden {
                    "TB_BaconUps_307"
                } else {
                    "TB_BaconShop_HP_105t"
                },
                "BG26_146",
                "BG36_345",
            ],
        );
        g.players[0].board[1].set_stats(3, 2);
        g.players[0].board[1].gained_dr.push(cid("BG20_HERO_282p"));
        g.players[0].board[2].set_stats(10, 1000);
        let mut c = combat(&mut g);
        let (fish, donor, target) = (c.boards[0][0].eid, c.boards[0][1].eid, c.boards[0][2].eid);
        c.destroy(donor);
        c.resolve_deaths();
        let acquired = &c.minion(fish).unwrap().gained_dr;
        assert_eq!(
            acquired
                .iter()
                .filter(|x| **x == cid("BG20_HERO_282p"))
                .count(),
            if golden { 2 } else { 1 }
        );
        let before = c.minion(target).unwrap().atk;
        let fish_attack = c.minion(fish).unwrap().atk;
        c.destroy(fish);
        c.resolve_deaths();
        assert_eq!(
            c.minion(target).unwrap().atk,
            before + fish_attack * if golden { 2 } else { 1 }
        );
    }
}

#[test]
fn mutanus_keeps_full_i32_stats() {
    let mut g = game(12300);
    board(&mut g, 0, &["BG26_146", "BG36_345"]);
    g.players[0].board[0].set_stats(40000, 70000);
    g.set_hero_power(0, 0, cid("BG20_HERO_301p"));
    g.apply(0, Action::HeroPower(0)).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(
        (g.players[0].board[0].atk, g.players[0].board[0].max_hp),
        (40003, 70003)
    );
}

#[test]
fn galakrond_replaces_a_tavern_card_even_with_a_full_hand() {
    let mut g = game(12400);
    for _ in 0..10 {
        give(&mut g, "BG28_810");
    }
    stock(&mut g, "BG36_345", None);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_011"));
    assert!(g.legal_mask(0)[A_HERO_POWER]);
    g.apply(0, Action::HeroPower(0)).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(matches!(
        g.players[0].pending.last(),
        Some(Pending::Discover { .. })
    ));
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].hand.len(), 10);
    assert_eq!(g.players[0].shop.len(), 1);
    assert_eq!(db().get(g.players[0].shop[0].item.card()).tier, 2);
}

#[test]
fn inge_alternates_displayed_power_and_preserves_its_two_use_limit() {
    let mut g = game(12500);
    board(&mut g, 0, &["BG36_345"]);
    g.set_hero_power(0, 0, cid("BG26_HERO_102p"));
    for (turn, power) in [
        (1, "BG26_HERO_102p"),
        (2, "BG26_HERO_102p2"),
        (3, "BG26_HERO_102p"),
    ] {
        if turn > 1 {
            g.start_turn();
        }
        assert_eq!(g.players[0].powers[0].card, cid(power));
        let before = (g.players[0].board[0].atk, g.players[0].board[0].max_hp);
        for _ in 0..2 {
            g.apply(0, Action::HeroPower(0)).unwrap();
            g.apply(0, Action::Choose(0)).unwrap();
        }
        assert!(!g.legal_mask(0)[A_HERO_POWER]);
        let after = &g.players[0].board[0];
        assert_eq!(
            (after.atk - before.0, after.max_hp - before.1),
            if turn % 2 == 1 { (2, 0) } else { (0, 2) }
        );
    }
}

#[test]
fn maxwell_follows_the_current_ragnaros_power_before_and_after_its_real_upgrade() {
    for golden in [false, true] {
        for bought in [11, 12] {
            let mut g = game(12600);
            g.set_hero_power(0, 0, cid("TB_BaconShop_HP_087"));
            board(
                &mut g,
                0,
                &[if golden {
                    "TB_BaconShop_HERO_40_Buddy_G"
                } else {
                    "TB_BaconShop_HERO_40_Buddy"
                }],
            );
            for _ in 0..bought {
                stock(&mut g, "BG36_345", None);
                g.apply(0, Action::Buy(0)).unwrap();
                g.apply(0, Action::Play { hand: 0, pos: 1 }).unwrap();
                g.apply(0, Action::Sell(1)).unwrap();
            }
            assert_eq!(
                g.players[0].powers[0].card,
                cid(if bought == 12 {
                    "TB_BaconShop_HP_087t"
                } else {
                    "TB_BaconShop_HP_087"
                })
            );
            g.apply(0, Action::Sell(0)).unwrap();
            let rag = db()
                .iter()
                .find(|d| d.hero_power == Some(cid("TB_BaconShop_HP_087")))
                .unwrap();
            assert_eq!(g.players[0].hand.len(), if golden { 2 } else { 1 });
            assert!(
                g.players[0]
                    .hand
                    .iter()
                    .all(|h| h.item.card() == rag.buddy.unwrap())
            );
        }
    }
}

fn make_reward_via_real_triple(g: &mut Game) {
    for _ in 0..3 {
        give(g, "BG36_345");
    }
    let index = g.players[0]
        .hand
        .iter()
        .position(|h| {
            h.item
                .as_minion()
                .is_some_and(|m| m.card == cid("BG36_345_G"))
        })
        .unwrap();
    let pos = g.players[0].board.len();
    g.apply(0, Action::Play { hand: index, pos }).unwrap();
    // Make room for a second genuine triple without triggering extra death effects.
    g.apply(0, Action::Sell(pos)).unwrap();
}

#[test]
fn stoneshell_applies_to_all_recasts_of_one_reward_and_counts_physical_rewards() {
    for golden in [false, true] {
        let mut g = game(12700);
        board(
            &mut g,
            0,
            &[if golden {
                "BG33_HERO_000_Buddy_G"
            } else {
                "BG33_HERO_000_Buddy"
            }],
        );
        g.players[0].trinkets.push(cid("BG30_MagicItem_434"));
        for physical in 0..2 {
            make_reward_via_real_triple(&mut g);
            let index = g.players[0]
                .hand
                .iter()
                .position(|h| h.item.card() == cid("TB_BaconShop_Triples_01"))
                .unwrap();
            g.apply(
                0,
                Action::Play {
                    hand: index,
                    pos: 0,
                },
            )
            .unwrap();
            let menus: Vec<_> = g.players[0]
                .pending
                .iter()
                .filter_map(|p| match p {
                    Pending::Discover { source, options }
                        if *source == cid("TB_BaconShop_Triples_01") =>
                    {
                        Some(options)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(menus.len(), if physical == 0 { 2 } else { 1 });
            assert!(menus.iter().all(|xs| {
                xs.iter()
                    .all(|x| x.as_minion().unwrap().golden == (physical == 0 || golden))
            }));
            while !g.players[0].pending.is_empty() {
                g.apply(0, Action::Choose(0)).unwrap();
            }
        }
    }
}

#[test]
fn deaths_head_sage_copies_gem_data_for_each_variant() {
    for golden in [false, true] {
        let mut g = game(12800);
        board(
            &mut g,
            0,
            &[
                if golden {
                    "BG20_HERO_103_Buddy_G"
                } else {
                    "BG20_HERO_103_Buddy"
                },
                "BG20_301",
            ],
        );
        g.with_recruit(0, |c| {
            c.add_to_hand(
                0,
                Item::Spell {
                    card: cid("BG20_GEM"),
                    data: 1,
                },
            );
        });
        assert_eq!(g.players[0].hand.len(), if golden { 3 } else { 2 });
        assert!(
            g.players[0]
                .hand
                .iter()
                .all(|h| matches!(h.item, Item::Spell { data: 1, .. }))
        );
        // Remove the original: an actual Sage-generated copy must grant the Gem keyword too.
        g.players[0].hand.remove(0);
        g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
        g.apply(0, Action::Choose(1)).unwrap();
        assert!(g.players[0].board[1].has(Kw::TAUNT));
    }
}

#[test]
fn pressure_completes_immediately_if_already_met_and_large_attack_cannot_wrap() {
    for (attack, complete) in [(1, false), (1200000000, true)] {
        let mut g = game(12900);
        board(&mut g, 0, &["BG26_146", "BG36_345"]);
        for m in &mut g.players[0].board {
            m.set_stats(attack, 10);
        }
        g.with_recruit(0, |c| {
            c.start_quest(0, cid("BG27_Quest_801"), cid("BG24_Reward_107"))
        });
        assert_eq!(g.players[0].quests.is_empty(), complete);
        assert_eq!(
            g.players[0].rewards.contains(&cid("BG24_Reward_107")),
            complete
        );
        if !complete {
            assert_eq!(g.players[0].quests[0].progress, 2);
        }
    }
}

#[test]
fn ethereal_evidence_fixes_typed_reward_data_at_offer_and_keeps_the_pick() {
    let mut typed = 0;
    for seed in 0..32 {
        let mut g = game(13000 + seed);
        g.players[0].rewards = db()
            .iter()
            .filter(|d| {
                d.kind == taverngym::cards::CardKind::QuestReward
                    && d.id != "BG24_Reward_134"
                    && d.id != "BG27_Reward_812"
            })
            .map(|d| d.cid)
            .collect();
        let source = cid("BG24_Reward_363");
        g.with_recruit(0, |c| {
            script(source).unwrap().start_of_turn.unwrap()(c, me(source, 0, false))
        });
        let pick = match g.players[0].pending.last().unwrap() {
            Pending::Discover { options, .. } => {
                options.iter().enumerate().find_map(|(i, x)| match *x {
                    Item::Spell { card, data }
                        if card == cid("BG24_Reward_134") || card == cid("BG27_Reward_812") =>
                    {
                        Some((i, card, data))
                    }
                    _ => None,
                })
            }
            _ => panic!("Ethereal Evidence should offer a Discover"),
        };
        if let Some((i, card, data)) = pick {
            typed += 1;
            assert_ne!(data, 0);
            assert_eq!((data as u16).count_ones(), 1);
            assert_ne!(g.lobby_races.0 & data as u16, 0);
            let view = taverngym::debug::view(&g, 0);
            let names = [
                "Beast",
                "Demon",
                "Dragon",
                "Elemental",
                "Mech",
                "Murloc",
                "Pirate",
                "Quilboar",
                "Undead",
                "Aberration",
                "Naga",
            ];
            let name = names[(data as u16).trailing_zeros() as usize];
            let text = view["me"]["pending"]["options"][i]["text"]
                .as_str()
                .unwrap();
            assert!(
                text.contains(name),
                "offered type must be visible before choosing: {text}"
            );
            assert!(!text.contains("92"));
            g.apply(0, Action::Choose(i)).unwrap();
            g.with_recruit(0, |c| assert_eq!(c.card_data(0, card), data));
        }
    }
    assert!(typed > 0);
}

#[test]
fn helpful_cards_have_three_independent_outcomes_and_exact_minion_tier() {
    let mut seen = BTreeSet::new();
    let mut mixed_pair = false;
    for seed in 0..48 {
        let mut g = game(13100 + seed);
        g.players[0].tier = 4;
        let pirate = g
            .universe
            .iter()
            .copied()
            .find(|x| db().get(*x).races == Races::PIRATE)
            .unwrap();
        board(&mut g, 0, &["BG30_HERO_304_Buddy_G"]);
        let m = g.with_recruit(0, |c| c.new_minion(0, pirate));
        g.players[0].board.push(m);
        let source = g.players[0].board[0].card;
        let eid = g.players[0].board[0].eid;
        g.with_recruit(0, |c| {
            script(source).unwrap().end_of_turn.unwrap()(c, me(source, eid, true))
        });
        assert_eq!(g.players[0].hand.len(), 2);
        let mut pair = vec![];
        for h in &g.players[0].hand {
            let d = db().get(h.item.card());
            let kind = if d.id == "BG32_MagicItem_205t" {
                0
            } else if d.is_minion() {
                assert_eq!(d.tier, 4);
                assert!(d.races.intersects(Races::PIRATE));
                1
            } else {
                assert_eq!(d.prize_tier, 3);
                2
            };
            seen.insert(kind);
            pair.push(kind);
        }
        mixed_pair |= pair[0] != pair[1];
    }
    assert_eq!(seen, BTreeSet::from([0, 1, 2]));
    assert!(mixed_pair, "the golden's two cards are independent draws");
    let mut g = game(13199);
    g.players[0].gold = 0;
    give(&mut g, "BG32_MagicItem_205t");
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert_eq!(g.players[0].gold, 3);
}

#[test]
fn sha_buffs_the_awakened_deity_once_per_spell() {
    for golden in [false, true] {
        let mut g = game(13200);
        board(
            &mut g,
            0,
            &[if golden { "BG36_111_G" } else { "BG36_111" }, "BGFYM_000"],
        );
        g.players[0].deity = Some(DeityState {
            card: cid("BGFYM_000"),
            atk: 1,
            hp: 1,
            golden: false,
            reborn: false,
        });
        let mut c = combat(&mut g);
        let e = c.boards[0][1].eid;
        c.cs[0].deity_eid = e;
        let before = (c.minion(e).unwrap().atk, c.minion(e).unwrap().max_hp);
        c.cast_from_hand = true;
        c.cast_spell(0, cid("BG28_500"), 0, 0, 0);
        let gain = if golden { 6 } else { 3 };
        assert_eq!(
            (c.minion(e).unwrap().atk, c.minion(e).unwrap().max_hp),
            (before.0 + gain, before.1 + gain)
        );
        assert_eq!(c.g.players[0].deity.as_ref().unwrap().atk, 1 + gain);
    }
}

#[test]
fn futurefin_and_stone_age_slab_saturate_stats_instead_of_wrapping() {
    let mut g = game(13300);
    board(&mut g, 0, &["BG34_145_G"]);
    g.players[0].board[0].set_stats(1200000000, 1200000000);
    let target = give(&mut g, "BG26_146");
    g.apply(0, Action::EndTurn).unwrap();
    let m = g.players[0]
        .hand
        .iter()
        .find(|h| h.hid == target)
        .unwrap()
        .item
        .as_minion()
        .unwrap();
    assert_eq!((m.atk, m.max_hp), (i32::MAX, i32::MAX));
    for golden in [false, true] {
        for a in [10, 1200000000] {
            let mut g = game(13301);
            board(&mut g, 0, &[if golden { "BG34_950_G" } else { "BG34_950" }]);
            stock(&mut g, "BG36_345", Some((a, a)));
            g.apply(0, Action::Buy(0)).unwrap();
            let m = g.players[0].hand[0].item.as_minion().unwrap();
            let expected =
                ((i64::from(a) + 20) * if golden { 3 } else { 2 }).min(i64::from(i32::MAX)) as i32;
            assert_eq!((m.atk, m.max_hp), (expected, expected));
        }
    }
}

#[test]
fn golden_nathanos_widens_before_halving_large_stats() {
    let mut g = game(13400);
    board(&mut g, 0, &["BG36_345", "BG26_146", "BG36_345"]);
    g.players[0].board[1].set_stats(1500000000, 1500000000);
    give(&mut g, "BG23_HERO_306_Buddy_G");
    g.apply(0, Action::Play { hand: 0, pos: 3 }).unwrap();
    g.apply(0, Action::Choose(1)).unwrap();
    for m in &g.players[0].board[..2] {
        assert_eq!((m.atk, m.max_hp), (1500000003, 1500000003));
    }
}

#[test]
fn losing_saurfang_keeps_bought_stats_but_removes_only_its_tavern_bonus() {
    let mut g = game(13500);
    g.players[0].stats.tavern_atk = 4;
    g.players[0].stats.tavern_hp = 4;
    stock(&mut g, "BG36_345", Some((7, 7))); // Other lasting Tavern bonus already applied.
    g.set_hero_power(0, 0, cid("BG20_HERO_102p"));
    g.apply(0, Action::Buy(0)).unwrap();
    let bought = g.players[0].hand[0].item.as_minion().unwrap().clone();
    stock(&mut g, "BG36_345", Some((8, 8)));
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_063"));
    assert_eq!(
        (g.players[0].stats.tavern_atk, g.players[0].stats.tavern_hp),
        (4, 4)
    );
    assert_eq!(g.players[0].shop[0].item.as_minion().unwrap().atk, 7);
    assert_eq!(
        g.players[0].hand[0].item.as_minion().unwrap().atk,
        bought.atk
    );
    g.with_recruit(0, |c| c.refresh_shop(0));
    for s in &g.players[0].shop {
        if let Some(m) = s.item.as_minion() {
            assert_eq!(m.atk, db().get(m.card).atk + 4);
        }
    }
}

#[test]
fn losing_rakanishu_removes_future_spell_bonus_but_keeps_resolved_stats() {
    let mut g = game(13600);
    board(&mut g, 0, &["BG36_345"]);
    g.players[0].stats.spell_atk = 4;
    g.players[0].stats.spell_hp = 4;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_085t"));
    give(&mut g, "BG_EX1_014t");
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    let before = g.players[0].board[0].atk;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_063"));
    assert_eq!(g.players[0].board[0].atk, before);
    assert_eq!(
        (g.players[0].stats.spell_atk, g.players[0].stats.spell_hp),
        (4, 4)
    );
    g.with_recruit(0, |c| assert_eq!(c.spell_extra(0), (4, 4)));
}

#[test]
fn combat_hero_power_discover_initializes_the_new_storm_power() {
    let mut g = game(13700);
    // This is the shared combat Discover path used by the Timewarped Student, with
    // all other choices excluded so this is not a random search for the desired result.
    let storm = cid("BG20_HERO_202p");
    let exclude: Vec<CardId> = db()
        .iter()
        .filter(|d| d.kind == taverngym::cards::CardKind::HeroPower && d.cid != storm)
        .map(|d| d.cid)
        .collect();
    let mut c = combat(&mut g);
    c.offer_hero_powers(0, 3, 0, &exclude);
    assert_eq!(c.g.players[0].powers[0].card, storm);
    assert_eq!(
        c.card_data(0, storm),
        1,
        "on_attach must arm the slot's later choices"
    );
}

#[test]
fn broken_horn_no_morph_enchantment_survives_a_triple() {
    let mut g = game(13800);
    // Three copies of Broken Horn's 6/6 non-morphing output, not a generic growth counter.
    for _ in 0..3 {
        g.with_recruit(0, |c| {
            let mut m = c.new_minion(0, cid("BG31_HERO_811t8"));
            m.set_stats(6, 6);
            m.counter = -1;
            c.add_to_hand(0, Item::Minion(m));
        });
    }
    assert_eq!(g.players[0].hand.len(), 1);
    let m = g.players[0].hand[0].item.as_minion().unwrap();
    assert!(m.golden);
    assert_eq!(m.counter, -1);
}

#[test]
fn upgrade_purchase_is_not_a_tavern_spell_for_hero_buddy_and_reward_listeners() {
    for (id, expected) in [("BG31_HERO_801ptf", 0), ("BG28_888", 2)] {
        let mut g = game(13900);
        g.set_hero_power(0, 0, cid("BG28_HERO_800p"));
        board(&mut g, 0, &["BG31_HERO_006_Buddy"]);
        g.players[0].rewards.push(cid("BG28_Reward_502"));
        g.with_recruit(0, |c| {
            c.emit(taverngym::event::Event::Bought {
                side: 0,
                card: cid(id),
                eid: 0,
                sid: 0,
                price: 0,
            })
        });
        assert_eq!(g.players[0].hand.len(), expected);
        assert_eq!(g.players[0].powers[0].counter, i32::from(expected > 0));
    }
}

/// Reconstruct a real valid Putricide pair through its shared assembly implementation.
fn creation(g: &mut Game, part: &str) -> EntityId {
    g.with_recruit(0, |c| {
        let mut template = c.new_minion(0, cid("BG25_HERO_100pt"));
        template.attached = vec![cid("BG25_005"), cid(part)]; // Jelly Belly + chosen pool-2 part.
        let m = taverngym::content::fresh_copy(c, 0, &template).unwrap();
        c.add_to_hand(0, Item::Minion(m)).unwrap()
    })
}

#[test]
fn creation_play_asks_for_the_component_target_and_finishes_after_the_choice() {
    let mut g = game(14000);
    board(&mut g, 0, &["BG36_345", "BG26_146"]);
    let before = (g.players[0].board[0].atk, g.players[0].board[1].atk);
    creation(&mut g, "BG_ICC_094"); // Fallen Sun Cleric: a friendly other minion +1/+1.
    g.apply(0, Action::Play { hand: 0, pos: 2 }).unwrap();
    assert!(
        matches!(g.players[0].pending.last(), Some(Pending::Target { options, .. }) if options.len() == 2)
    );
    assert!(!g.legal_mask(0)[taverngym::actions::A_CANCEL]);
    assert_eq!(g.players[0].stats.battlecries_triggered, 0);
    // Saved pending continuation retains the selected source and completion event.
    let mut g: Game = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
    g.apply(0, Action::Choose(1)).unwrap();
    assert_eq!(g.players[0].board[0].atk, before.0);
    assert_eq!(g.players[0].board[1].atk, before.1 + 1);
    assert_eq!(g.players[0].stats.cards_played_turn, 1);
    assert_eq!(g.players[0].stats.battlecries_triggered, 1);
    assert!(g.players[0].pending.is_empty());
}

#[test]
fn creation_component_with_no_legal_target_does_not_block_the_play() {
    let mut g = game(14100);
    creation(&mut g, "BG26_RLK_119"); // Embalmer cannot target the Creation itself.
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert!(g.players[0].pending.is_empty());
    assert!(!g.players[0].board[0].has(Kw::REBORN));
    assert_eq!(g.players[0].stats.cards_played_turn, 1);
    assert_eq!(g.players[0].stats.battlecries_triggered, 1);
}

#[test]
fn composite_sources_choose_separately_and_share_one_play_multiplier() {
    let mut g = game(14200);
    board(&mut g, 0, &["BG_LOE_077", "BG36_345"]);
    g.players[0].trinkets.push(cid("BG32_MagicItem_416"));
    let hid = creation(&mut g, "BG_ICC_094");
    // Synthetic composite with a second copy of an existing component: exercises the
    // generic source queue, not a claim that Putricide offers three construction parts.
    g.players[0]
        .hand
        .iter_mut()
        .find(|h| h.hid == hid)
        .unwrap()
        .item
        .as_minion_mut()
        .unwrap()
        .attached
        .push(cid("BG_ICC_094"));
    let before = (g.players[0].board[0].atk, g.players[0].board[1].atk);
    g.apply(0, Action::Play { hand: 0, pos: 2 }).unwrap();
    g.apply(0, Action::Choose(1)).unwrap();
    assert_eq!(g.players[0].board[1].atk, before.1 + 4); // Brann (2) + one Drum (2).
    assert!(matches!(
        g.players[0].pending.last(),
        Some(Pending::Target { .. })
    ));
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].board[0].atk, before.0 + 4);
    assert_eq!(g.players[0].stats.battlecries_triggered, 8);
    assert_eq!(g.players[0].stats.cards_played_turn, 1);
    assert!(g.players[0].pending.is_empty());
    g.with_recruit(0, |c| {
        assert_eq!(c.turn_data(0, cid("BG32_MagicItem_416")), 1)
    });
}

#[test]
fn effect_triggered_creation_battlecry_keeps_automatic_targeting() {
    let mut g = game(14300);
    board(&mut g, 0, &["BG36_345", "BG26_146"]);
    let before: i32 = g.players[0].board.iter().map(|m| m.atk).sum();
    let hid = creation(&mut g, "BG_ICC_094");
    let i = g.players[0].hand.iter().position(|h| h.hid == hid).unwrap();
    let Item::Minion(m) = g.players[0].hand.remove(i).item else {
        unreachable!()
    };
    let eid = m.eid;
    g.players[0].board.push(m);
    g.with_recruit(0, |c| c.trigger_battlecry_ext(0, eid));
    assert_eq!(
        g.players[0].board[..2].iter().map(|m| m.atk).sum::<i32>(),
        before + 1
    );
    assert!(g.players[0].pending.is_empty());
    assert_eq!(g.players[0].stats.cards_played_turn, 0);
}
