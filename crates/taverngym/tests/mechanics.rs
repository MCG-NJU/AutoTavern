//! Targeted mechanism tests on hand-built situations.

use taverngym::actions::Action;
use taverngym::bot::run_bot_game;
use taverngym::cards::{CardId, cid, db};
use taverngym::state::{Item, Minion, Pending};
use taverngym::types::{Kw, PlayerId, Races};
use taverngym::{Game, RulesConfig};

/// A game past hero selection (everyone picks option 0), turn 1 recruit phase.
fn game(seed: u64) -> Game {
    // Aberrations in every lobby (the 36.6.1 guarantee, off by default since 36.6.3): the
    // Deity tests need them and the fixtures keep their lobbies.
    let cfg = RulesConfig {
        aberration_forced: true,
        ..RulesConfig::default()
    };
    let mut g = Game::new(seed, cfg);
    for p in 0..8 {
        g.apply(p, Action::Choose(0)).unwrap();
    }
    assert_eq!(g.turn, 1);
    // Pin every seat to Patchwerk (a passive, non-combat hero) for predictable tests; drop
    // choices the picked heroes may have opened (Finley...).
    for p in 0..8 {
        g.players[p].hero = Some(cid("TB_BaconShop_HERO_34"));
        g.players[p].pending.clear();
        g.players[p].hand.clear(); // e.g. King Mukla's turn-1 Bananas
        // Undo other heroes' start-of-game effects (Rakanishu, Saurfang, quests...).
        g.players[p].stats = Default::default();
        g.players[p].effects.clear();
        g.players[p].quests.clear();
        g.players[p].counters.clear();
        g.players[p].max_gold_bonus = 0;
        g.players[p].skipping = false; // Faelin / A. F. Kay
        g.players[p].board.clear();
        g.players[p].powers.clear();
        g.players[p].set_hero_power(0, cid("TB_BaconShop_HP_035"));
    }
    g
}

fn minion(g: &mut Game, p: PlayerId, id: &str) -> Minion {
    g.with_recruit(p, |c| c.new_minion(0, cid(id)))
}

fn set_board(g: &mut Game, p: PlayerId, ids: &[&str]) {
    let b: Vec<Minion> = ids.iter().map(|id| minion(g, p, id)).collect();
    g.players[p].board = b;
}

fn fight(g: &mut Game) -> taverngym::combat::CombatOutcome {
    g.simulate_combat(0, Some(1), None)
}

#[test]
fn economy_turn_one() {
    let mut g = game(1);
    // The fixture's first Tavern was rolled for the hero picked at the start (Sindragosa
    // has one minion less): roll it again for the pinned hero.
    g.with_recruit(0, |c| c.refresh_shop(0));
    let p = &g.players[0];
    assert_eq!(p.gold, 3);
    assert_eq!(p.tier, 1);
    assert_eq!(g.upgrade_price(0), 5);
    assert_eq!(g.minion_price(0), 3);
    assert_eq!(
        p.shop
            .iter()
            .filter(|s| matches!(s.item, Item::Minion(_)))
            .count(),
        3
    );
}

#[test]
fn gold_and_upgrade_discount_progress() {
    let mut g = game(2);
    for p in 0..8 {
        g.apply(p, Action::EndTurn).unwrap();
    }
    assert_eq!(g.turn, 2);
    let p = &g.players[0];
    assert!(p.alive);
    assert_eq!(p.gold, 4);
    assert_eq!(g.upgrade_price(0), 4);
}

#[test]
fn buy_sell_gold() {
    let mut g = game(3);
    let slot = g.players[0]
        .shop
        .iter()
        .position(|s| matches!(s.item, Item::Minion(_)))
        .unwrap();
    g.apply(0, Action::Buy(slot)).unwrap();
    assert_eq!(g.players[0].gold, 0);
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert_eq!(g.players[0].board.len(), 1);
    g.apply(0, Action::Sell(0)).unwrap();
    assert_eq!(g.players[0].gold, 1);
    assert!(g.players[0].board.is_empty());
}

#[test]
fn divine_shield_blocks_one_hit() {
    let mut g = game(4);
    set_board(&mut g, 0, &["BGS_034"]); // Bronze Warden 2/1 DS Reborn
    set_board(&mut g, 1, &["BG36_345"]); // Prisonguard 3/3
    let out = fight(&mut g);
    // Warden: DS absorbs first hit, dies to second, reborns as 2/1 and trades again.
    assert!(out.winner <= 2);
    assert!(!out.truncated);
}

#[test]
fn taunt_is_attacked_first() {
    for seed in 0..20 {
        let mut g = game(seed);
        set_board(&mut g, 0, &["BG36_345"]); // 3/3
        let mut b = vec![minion(&mut g, 1, "BG36_345"), minion(&mut g, 1, "BG25_001")];
        b[0].set_stats(1, 100);
        b[1].set_stats(0, 1); // Risen Rider has Taunt
        b[1].remove(Kw::REBORN);
        g.players[1].board = b;
        let cfg_log = RulesConfig {
            record_log: true,
            ..RulesConfig::default()
        };
        g.cfg = cfg_log;
        g.log.clear();
        fight(&mut g);
        let first_attack = g
            .log
            .iter()
            .find(|l| l.contains("side0") && l.contains("attacks"))
            .cloned();
        if let Some(l) = first_attack {
            assert!(l.contains("attacks Risen Rider"), "{l}");
        }
    }
}

#[test]
fn deathrattle_then_reborn_and_titus_doubles() {
    let mut g = game(5);
    g.cfg.triple_copies = 99; // recruit-phase tokens would otherwise triple (correct BG rule)
    // Harmless Bonehead: DR summon two Skeletons. With Titus: four.
    set_board(&mut g, 0, &["BG28_300", "BG25_354"]);
    g.with_recruit(0, |c| {
        let e = c.boards[0][0].eid;
        c.destroy(e);
        c.resolve_deaths();
        let skeletons = c.boards[0]
            .iter()
            .filter(|m| m.is_card("BG_ICC_026t"))
            .count();
        assert_eq!(skeletons, 4);
        assert_eq!(c.boards[0].len(), 5);
    });
}

#[test]
fn reborn_returns_with_one_health() {
    let mut g = game(6);
    set_board(&mut g, 0, &["BG25_001"]); // Risen Rider 2/1 Taunt Reborn
    g.with_recruit(0, |c| {
        let e = c.boards[0][0].eid;
        c.destroy(e);
        c.resolve_deaths();
        assert_eq!(c.boards[0].len(), 1);
        let m = &c.boards[0][0];
        assert_eq!(m.hp, 1);
        assert!(!m.has(Kw::REBORN));
        assert!(m.has(Kw::TAUNT));
    });
}

#[test]
fn venomous_kills_through_health() {
    let mut g = game(7);
    set_board(&mut g, 0, &["BGS_131"]); // Deadly Spore 1/1 Venomous
    let mut big = minion(&mut g, 1, "BG36_345");
    big.set_stats(0, 50);
    g.players[1].board = vec![big];
    let out = fight(&mut g);
    assert_eq!(out.winner, 0, "venomous should kill the 0/50");
}

#[test]
fn multi_hit_divine_shield() {
    let mut g = game(8);
    let m = minion(&mut g, 0, "BG36_360t4"); // Dark Paradox (tier 5): 3-hit shield
    assert_eq!(m.shield_hits, 3);
    g.players[0].board = vec![m];
    g.with_recruit(0, |c| {
        let e = c.boards[0][0].eid;
        for _ in 0..3 {
            c.damage(0, e, 100);
        }
        assert!(c.boards[0][0].alive());
        assert!(!c.boards[0][0].has(Kw::DIVINE_SHIELD));
        c.damage(0, e, 100);
        assert!(!c.boards[0][0].alive());
    });
}

#[test]
fn triple_makes_golden_and_reward_on_play() {
    let mut g = game(9);
    g.players[0].gold = 100;
    let pairs: Vec<Minion> = (0..3).map(|_| minion(&mut g, 0, "BG36_345")).collect();
    g.players[0].board = vec![pairs[0].clone(), pairs[1].clone()];
    g.with_recruit(0, |c| {
        c.add_to_hand(0, Item::Minion(pairs[2].clone()));
    });
    let p = &g.players[0];
    assert!(p.board.is_empty(), "board copies merged");
    let golden = p.hand.iter().find_map(|h| h.item.as_minion()).unwrap();
    assert!(golden.golden && golden.triple_reward);
    assert_eq!((golden.atk, golden.hp), (6, 6));
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    let reward = g.players[0]
        .hand
        .iter()
        .find(|h| db().get(h.item.card()).id == "TB_BaconShop_Triples_01");
    let Some(r) = reward else {
        panic!("no triple reward")
    };
    assert!(
        matches!(r.item, Item::Spell { data: 2, .. }),
        "reward tier = tier + 1"
    );
}

#[test]
fn magnetize_merges_stats_and_keywords() {
    let mut g = game(10);
    set_board(&mut g, 0, &["BG26_146"]); // Lullabot 2/2 Mech (host)
    let module = minion(&mut g, 0, "BG_BOT_911"); // Annoy-o-Module 2/4 DS Taunt Magnetic
    g.with_recruit(0, |c| {
        c.add_to_hand(0, Item::Minion(module));
    });
    assert!(g.can_magnetize(0, 0, 0));
    g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
    let host = &g.players[0].board[0];
    assert_eq!((host.atk, host.hp), (4, 6));
    assert!(host.has(Kw::DIVINE_SHIELD) && host.has(Kw::TAUNT));
    assert!(!host.has(Kw::MAGNETIC) || host.is_card("BG26_146"));
    assert_eq!(g.players[0].board.len(), 1);
}

#[test]
fn magnetic_extra_races_work_on_a_full_board() {
    for (card, extra) in [
        ("BG_DEEP_015", Races::UNDEAD),
        ("BG_DEEP_015_G", Races::UNDEAD),
        ("BG25_807t2", Races::DEMON),
        ("BG25_807t2_G", Races::DEMON),
        ("BG31_859", Races::ELEMENTAL),
        ("BG31_859_G", Races::ELEMENTAL),
    ] {
        let mut g = game(10);
        set_board(
            &mut g,
            0,
            &[
                "BG26_146", "BG25_008", "BGS_034", "BG29_611", "BG31_816", "BG21_029", "BG36_345",
            ],
        );
        g.players[0].gold = 0;
        g.players[0].board[1].races = extra;
        g.players[0].board[2].races = Races::BEAST;
        let module = minion(&mut g, 0, card);
        let attack = module.atk;
        let health = module.hp;
        g.with_recruit(0, |c| {
            c.add_to_hand(0, Item::Minion(module));
        });
        assert!(g.can_magnetize(0, 0, 0), "{card} onto a Mech");
        assert!(g.can_magnetize(0, 0, 1), "{card} onto its extra race");
        assert!(!g.can_magnetize(0, 0, 2), "{card} onto a Beast");
        let host = &g.players[0].board[1];
        let expected = (host.atk + attack, host.hp + health);
        g.apply(0, Action::Magnetize { hand: 0, board: 1 }).unwrap();
        let host = &g.players[0].board[1];
        assert_eq!((host.atk, host.hp), expected);
        assert_eq!(g.players[0].board.len(), 7);
        assert_eq!(
            g.players[0].hand.len(),
            usize::from(db().get(cid(card)).is_golden())
        );
        assert!(
            g.players[0]
                .hand
                .iter()
                .all(|h| h.item.card() == cid("TB_BaconShop_Triples_01"))
        );
    }
}

#[test]
fn battlecry_with_brann_triggers_twice() {
    let mut g = game(11);
    set_board(&mut g, 0, &["BG_LOE_077"]); // Brann
    let busker = minion(&mut g, 0, "BG26_135"); // Southsea Busker: 1 gold next turn
    g.with_recruit(0, |c| {
        c.add_to_hand(0, Item::Minion(busker));
    });
    g.apply(0, Action::Play { hand: 0, pos: 1 }).unwrap();
    assert_eq!(g.players[0].gold_next_turn, 2);
}

#[test]
fn deity_awakens_after_four_aberration_deaths() {
    let mut g = game(12);
    let mut b = vec![];
    for _ in 0..4 {
        let mut m = minion(&mut g, 0, "BG36_098"); // Zoatroid 3/2 Aberration
        m.set_stats(1, 1);
        b.push(m);
    }
    g.players[0].board = b;
    let mut enemy = minion(&mut g, 1, "BG36_345");
    enemy.set_stats(10, 200);
    g.players[1].board = vec![enemy];
    g.cfg.record_log = true;
    g.log.clear();
    fight(&mut g);
    assert!(
        g.log.iter().any(|l| l.contains("deity awakens")),
        "{:#?}",
        g.log
    );
}

#[test]
fn combat_reports_the_deity_and_what_yshaarj_brings_back() {
    // Diagnostics of a combat (CombatStats): the Deity awakened, and the largest minion
    // Y'Shaarj's Deathrattle summoned back, with the stats it had at its best.
    let mut g = game(12);
    let mut b = vec![];
    for stats in [(1, 60), (1, 1), (1, 1), (1, 1)] {
        let mut m = minion(&mut g, 0, "BG36_098"); // Zoatroid, an Aberration
        m.set_stats(stats.0, stats.1);
        b.push(m);
    }
    // The large one dies first: everything attacks the Taunt.
    b[0].give(taverngym::types::Kw::TAUNT);
    g.players[0].board = b;
    g.players[0].deity = Some(taverngym::state::DeityState {
        card: cid("BGFYM_011"), // Y'Shaarj
        atk: 1,
        hp: 1,
        golden: false,
        reborn: false,
    });
    let mut enemy = minion(&mut g, 1, "BG36_345");
    enemy.set_stats(100, 2000);
    g.players[1].board = vec![enemy];
    let out = fight(&mut g);
    assert_eq!(out.stats[0].deity_wakes, 1);
    assert_eq!(
        out.stats[0].returned_max, 61,
        "the 1/60 came back as it was"
    );
    assert_eq!(
        (out.stats[1].deity_wakes, out.stats[1].returned_max),
        (0, 0)
    );
    g.apply_combat_result(0, Some(1), out);
    let st = &g.players[0].stats;
    assert!(st.last_deity && st.last_returned_max == 61);
    assert!(!g.players[1].stats.last_deity);
}

#[test]
fn the_action_cap_can_differ_by_turn() {
    // `turn_action_caps`: 3 actions in turn 1, 5 from turn 2 on. Past the cap only End Turn
    // is legal.
    let mut g = game(302);
    g.cfg.turn_action_caps = vec![3, 5];
    // (Freeze is the operation that changes nothing else; its own limit of ten presses a
    // turn is not what is tested here.)
    g.cfg.freeze_presses_per_turn = 0;
    assert_eq!(
        (
            g.cfg.action_cap(1),
            g.cfg.action_cap(2),
            g.cfg.action_cap(9)
        ),
        (3, 5, 5)
    );
    g.players[0].actions_this_turn = 0;
    let legal = |g: &Game| -> usize { g.legal_mask(0).iter().filter(|x| **x).count() };
    for k in 0..3 {
        assert!(legal(&g) > 1, "action {k}");
        g.apply(0, Action::Freeze).unwrap();
    }
    assert_eq!(legal(&g), 1, "only End Turn");
    assert!(g.legal_mask(0)[Action::EndTurn.encode()]);
    // Without the table there is no cap at all: the number of operations is only limited
    // by time, which a caller turns into this table.
    g.cfg.turn_action_caps.clear();
    assert_eq!(g.cfg.action_cap(1), u32::MAX);
    for _ in 0..300 {
        g.apply(0, Action::Freeze).unwrap();
    }
    assert!(legal(&g) > 1);
    assert!(RulesConfig::default().turn_action_caps.is_empty());
}

#[test]
fn dark_gift_offers_distinct_gifts() {
    let mut g = game(13);
    // Fast-forward to turn 3 with bots.
    while g.turn < 3 {
        for p in 0..8 {
            if g.needs_action(p) {
                g.apply(p, Action::EndTurn).unwrap();
            }
        }
    }
    g.players[0].gold = 10;
    assert!(g.can_dark_gift(0));
    g.apply(0, Action::DarkGift).unwrap();
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!("no discover")
    };
    let gifts: Vec<_> = options
        .iter()
        .map(|o| o.as_minion().unwrap().attached[0])
        .collect();
    let mut d = gifts.clone();
    d.sort();
    d.dedup();
    assert_eq!(d.len(), gifts.len());
    assert!(
        options.iter().all(|o| o.as_minion().unwrap().tier == 2),
        "turn 3 gifts are tier 2"
    );
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].gold, 7);
    assert!(!g.can_dark_gift(0), "once per turn");
}

#[test]
fn linked_pair_discards_partner() {
    let mut g = game(14);
    g.players[0].gold = 10;
    g.with_recruit(0, |c| {
        taverngym::recruit::give_linked_pair(c, 0, cid("BG28_810"), cid("BG28_810"));
    });
    assert_eq!(g.players[0].hand.len(), 2);
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert!(g.players[0].hand.is_empty(), "partner discarded");
    assert_eq!(g.players[0].stats.cards_discarded, 1);
}

#[test]
fn damage_cap_applies_early() {
    let mut g = game(15);
    let mut big: Vec<Minion> = (0..7).map(|_| minion(&mut g, 0, "BG36_345")).collect();
    for m in big.iter_mut() {
        m.set_stats(50, 50);
        m.tier = 6;
    }
    g.players[0].board = big;
    let out = fight(&mut g);
    assert_eq!(out.winner, 0);
    let before = g.players[1].health + g.players[1].armor;
    g.apply_combat_result(0, Some(1), out);
    let after = g.players[1].health + g.players[1].armor;
    assert_eq!(before - after, 5, "turn 1 cap is 5");
}

#[test]
fn pool_is_conserved() {
    let cfg = RulesConfig::default();
    for seed in 0..10 {
        let g = run_bot_game(seed, cfg.clone());
        let initial: u64 = g
            .universe
            .iter()
            .map(|c| cfg.pool_copies_by_tier[db().get(*c).tier as usize - 1] as u64)
            .sum();
        let mut held: u64 = g.pool.counts.iter().map(|n| *n as u64).sum();
        for p in g.players.iter().filter(|p| p.alive) {
            held += p.board.iter().map(|m| m.pool_copies as u64).sum::<u64>();
            held += p
                .hand
                .iter()
                .filter_map(|h| h.item.as_minion())
                .map(|m| m.pool_copies as u64)
                .sum::<u64>();
            held += p
                .shop
                .iter()
                .filter_map(|s| s.item.as_minion())
                .map(|m| m.pool_copies as u64)
                .sum::<u64>();
            held += p
                .hand_queue
                .iter()
                .filter_map(|i| i.as_minion())
                .map(|m| m.pool_copies as u64)
                .sum::<u64>();
        }
        assert_eq!(held, initial, "seed {seed}");
    }
}

#[test]
fn targeted_spell_stays_in_hand_until_target_and_can_cancel() {
    let mut g = game(20);
    g.players[0].gold = 5;
    set_board(&mut g, 0, &["BG36_345"]);
    g.with_recruit(0, |c| {
        c.give_card(0, cid("BG28_897")); // Tavern Dish Banana: give a minion +2/+2
    });
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    // Waiting for a target: card still in hand, nothing paid, only choose/cancel legal.
    assert!(matches!(
        g.players[0].pending.last(),
        Some(Pending::Target { .. })
    ));
    assert_eq!(g.players[0].hand.len(), 1);
    assert_eq!(g.players[0].gold, 5);
    assert!(g.legal_mask(0)[Action::Cancel.encode()]);
    assert!(!g.legal_mask(0)[Action::EndTurn.encode()]);
    g.apply(0, Action::Cancel).unwrap();
    assert!(g.players[0].pending.is_empty());
    assert_eq!(g.players[0].hand.len(), 1);
    // Cast for real: target our minion (index of its eid among the options).
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    let eid = g.players[0].board[0].eid;
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
        panic!()
    };
    let i = options.iter().position(|e| *e == eid).unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
    assert!(g.players[0].hand.is_empty());
    assert_eq!(
        g.players[0].gold, 5,
        "Tavern spells are paid when bought, casting is free"
    );
    assert_eq!(
        (g.players[0].board[0].atk, g.players[0].board[0].hp),
        (5, 5)
    );
}

#[test]
fn banana_can_target_tavern_minion() {
    let mut g = game(21);
    g.with_recruit(0, |c| {
        c.give_card(0, cid("BG28_897"));
    });
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!()
    };
    let sid = g.players[0]
        .shop
        .iter()
        .find(|s| s.item.as_minion().is_some())
        .unwrap()
        .sid;
    let before = g.players[0]
        .shop
        .iter()
        .find(|s| s.sid == sid)
        .unwrap()
        .item
        .as_minion()
        .unwrap()
        .atk;
    let i = options.iter().position(|e| *e == sid).unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
    let after = g.players[0]
        .shop
        .iter()
        .find(|s| s.sid == sid)
        .unwrap()
        .item
        .as_minion()
        .unwrap()
        .atk;
    assert_eq!(after, before + 2);
}

#[test]
fn hero_power_targeting_cancel_does_not_consume_use() {
    let mut g = game(22);
    g.players[0].hero = Some(cid("TB_BaconShop_HERO_15")); // George: give a minion Divine Shield (1)
    g.players[0].set_hero_power(0, cid("TB_BaconShop_HP_010"));
    g.players[0].gold = 3;
    set_board(&mut g, 0, &["BG36_345"]);
    g.apply(0, Action::HeroPower(0)).unwrap();
    g.apply(0, Action::Cancel).unwrap();
    assert_eq!(
        (g.players[0].gold, g.players[0].powers[0].uses_turn),
        (3, 0)
    );
    g.apply(0, Action::HeroPower(0)).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(
        (g.players[0].gold, g.players[0].powers[0].uses_turn),
        (2, 1)
    );
    assert!(g.players[0].board[0].has(Kw::DIVINE_SHIELD));
    assert!(!g.can_hero_power(0, 0));
}

#[test]
fn trinket_offer_blocks_the_turn_and_charges_on_pick() {
    let mut g = game(23);
    g.players[0].gold = 8;
    g.offer_trinkets(0, taverngym::cards::CardKind::TrinketLesser);
    let Some(Pending::Trinket { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("no trinket choice")
    };
    assert!(options.iter().all(|t| db().get(*t).cost <= 8));
    // Buying from the tavern (or anything else) is illegal until a trinket is chosen.
    assert!(g.apply(0, Action::Buy(0)).is_err());
    assert!(!g.legal_mask(0)[Action::Buy(0).encode()]);
    assert!(!g.legal_mask(0)[Action::Cancel.encode()]);
    let cost = db().get(options[0]).cost;
    g.apply(0, Action::Choose(0)).unwrap();
    // The pick is owned first (Orb of the Unknown may add another one after it).
    assert_eq!(g.players[0].trinkets[0], options[0]);
    // The offer is resolved (the trinket itself may open a new choice, e.g. Mystery Cube's
    // declinable swap).
    assert!(!matches!(
        g.players[0].pending.last(),
        Some(Pending::Trinket { replace: None, .. })
    ));
    assert!(
        g.players[0].gold <= 8 - cost + 10,
        "paid (some trinkets refund gold)"
    );
}

#[test]
fn pool_tag_rule_excludes_rot_hide_gnoll_by_default() {
    let gnoll = cid("BG25_013");
    let g = Game::new(1, RulesConfig::default());
    assert!(!g.universe.contains(&gnoll));
    // Force an Undead lobby check across seeds: never in the universe with the strict rule.
    for seed in 0..20 {
        assert!(
            !Game::new(seed, RulesConfig::default())
                .universe
                .contains(&gnoll)
        );
    }
    let lax = RulesConfig {
        strict_pool_tag: false,
        ..RulesConfig::default()
    };
    let found = (0..20).any(|seed| Game::new(seed, lax.clone()).universe.contains(&gnoll));
    assert!(
        found,
        "with strict_pool_tag=false the gnoll appears in Undead lobbies"
    );
    // Only the non-standard tag is dropped: every other current candidate stays eligible.
    let eligible = db()
        .iter()
        .filter(|c| c.pools.minion && c.pool_tag <= 1)
        .count();
    assert_eq!(eligible, 244, "245 bundle candidates minus Rot Hide Gnoll");
    let excl = RulesConfig {
        excluded_cards: vec!["BG25_001".into()],
        ..RulesConfig::default()
    };
    assert!(!(0..20).any(|seed| {
        Game::new(seed, excl.clone())
            .universe
            .contains(&cid("BG25_001"))
    }));
}

#[test]
fn titus_rivendare_extra_triggers_stack() {
    // "Your Deathrattles trigger an extra time": each copy adds one (golden: two).
    let mut g = game(30);
    g.cfg.triple_copies = 99;
    // Harmless Bonehead + two normal Titus: deathrattle runs three times -> six Skeletons,
    // but only 5 fit next to the two Tituses on a 7-slot board.
    set_board(&mut g, 0, &["BG28_300", "BG25_354", "BG25_354"]);
    g.with_recruit(0, |c| {
        assert_eq!(c.deathrattle_mult(0), 3);
        let e = c.boards[0][0].eid;
        c.destroy(e);
        c.resolve_deaths();
        let skeletons = c.boards[0]
            .iter()
            .filter(|m| m.is_card("BG_ICC_026t"))
            .count();
        assert_eq!(skeletons, 5);
    });
    // Golden + normal Titus: 1 + 2 + 1 = 4 triggers.
    let mut g = game(31);
    let golden = g.with_recruit(0, |c| c.new_minion(0, db().golden_of(cid("BG25_354"))));
    let normal = minion(&mut g, 0, "BG25_354");
    g.players[0].board = vec![golden, normal];
    g.with_recruit(0, |c| assert_eq!(c.deathrattle_mult(0), 4));
    // Brann does not stack ("trigger twice"): two Branns still give 2.
    let mut g = game(33);
    set_board(&mut g, 0, &["BG_LOE_077", "BG_LOE_077"]);
    g.with_recruit(0, |c| assert_eq!(c.battlecry_mult(0), 2));
}

#[test]
fn trinket_offers_respect_lobby_types_and_card_conditions() {
    let d = db();
    let wishbone = cid("BG30_MagicItem_804");
    for seed in 0..30 {
        let g = Game::new(seed, RulesConfig::default());
        for t in &g.trinket_universe {
            let a = d.get(*t).assoc;
            assert!(
                a.is_none() || a.intersects(g.lobby_races),
                "{} offered without its type",
                d.get(*t).id
            );
        }
    }
    // Passive hero power (Patchwerk, pinned by game()) never sees Ancient Wishbone.
    let mut g = game(32);
    assert!(!g.trinket_eligible(0, wishbone));
    g.players[0].set_hero_power(0, cid("TB_BaconShop_HP_010")); // George: active
    assert!(g.trinket_eligible(0, wishbone));
    // Stuffed Coin Purse is not offered at Tier 6.
    let purse = cid("BG35_MagicItem_814");
    assert!(g.trinket_eligible(0, purse));
    g.players[0].tier = 6;
    assert!(!g.trinket_eligible(0, purse));
    // Offers are drawn only from eligible cards: 200 draws for a passive hero never include it.
    g.players[0].set_hero_power(0, cid("TB_BaconShop_HP_035"));
    for _ in 0..200 {
        g.players[0].pending.clear();
        g.players[0].gold = 10;
        g.offer_trinkets(0, taverngym::cards::CardKind::TrinketLesser);
        if let Some(Pending::Trinket { options, .. }) = g.players[0].pending.last() {
            assert!(!options.contains(&wishbone));
        }
    }
}

#[test]
fn next_opponent_is_known_during_recruit_and_used_in_combat() {
    for seed in 0..10 {
        let mut g = game(seed);
        for _turn in 0..6 {
            // Symmetric pairing, one ghost at most, and each living player has an opponent.
            let alive: Vec<usize> = (0..8).filter(|p| g.players[*p].alive).collect();
            for &p in &alive {
                match g.players[p].next_opponent {
                    Some(o) => assert_eq!(g.players[o].next_opponent, Some(p)),
                    None => assert!(g.players[p].next_is_ghost),
                }
            }
            let expected: Vec<(usize, Option<usize>)> = alive
                .iter()
                .map(|&p| (p, g.players[p].next_opponent))
                .collect();
            for p in 0..8 {
                // Resolve blocking choices (e.g. the turn-6 trinket offer) first.
                while !g.players[p].pending.is_empty() {
                    let a = g.legal_actions(p)[0];
                    g.apply_index(p, a).unwrap();
                }
                if g.needs_action(p) {
                    g.apply(p, Action::EndTurn).unwrap();
                }
            }
            if g.is_over() {
                break;
            }
            // The combat that just happened matched the announced opponents.
            for (p, o) in expected {
                assert_eq!(g.players[p].last_opponent, o, "seed {seed}");
            }
        }
    }
}

/// Board of side `s` right after Start of Combat (needs `record_log`).
fn after_soc(g: &Game, s: usize) -> Vec<(String, i32, i32, bool)> {
    g.last_frames[1].boards[s]
        .iter()
        .map(|m| (m.name().to_string(), m.atk, m.hp, m.combat_only))
        .collect()
}

fn give(g: &mut Game, p: PlayerId, id: &str) -> u32 {
    g.with_recruit(p, |c| c.give_card(0, cid(id)).unwrap())
}

#[test]
fn hand_start_of_combat_and_summon_from_hand() {
    let mut g = game(40);
    g.cfg.record_log = true;
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG36_345"]);
    set_board(&mut g, 1, &["BG36_345"]);
    give(&mut g, 0, "BG32_330"); // Flighty Scout in hand
    let hand_before = g.players[0].hand.clone();
    let board_before = g.players[0].board.clone();
    g.simulate_combat(0, Some(1), None);
    let b = after_soc(&g, 0);
    assert!(
        b.iter()
            .any(|(n, a, h, c)| n == "Flighty Scout" && (*a, *h) == (3, 3) && *c),
        "{b:?}"
    );
    // Nothing persistent changed: the Scout stays in hand, the warband is untouched.
    assert_eq!(g.players[0].hand, hand_before);
    assert_eq!(g.players[0].board, board_before);
}

#[test]
fn diremuck_forager_summons_highest_attack_murloc_once() {
    let mut g = game(41);
    g.cfg.record_log = true;
    g.cfg.triple_copies = 99;
    // Two Foragers compete for the same hand Murloc: it can only be summoned once.
    set_board(&mut g, 0, &["BG27_556", "BG27_556"]);
    set_board(&mut g, 1, &["BG36_345"]);
    let small = give(&mut g, 0, "BG32_330"); // 3/3 Murloc
    let big = give(&mut g, 0, "BG26_137"); // Bream Counter 6/6 Murloc
    let _ = (small, big);
    g.simulate_combat(0, Some(1), None);
    let b = after_soc(&g, 0);
    let summoned: Vec<_> = b.iter().filter(|x| x.3).map(|x| x.0.clone()).collect();
    // Hand effects go first: the Scout's own copy, then one summon per Forager.
    assert_eq!(
        summoned,
        vec![
            "Flighty Scout".to_string(),
            "Bream Counter".to_string(),
            "Flighty Scout".to_string()
        ],
        "{b:?}"
    );
}

#[test]
fn bream_counter_grows_only_while_in_hand() {
    let mut g = game(42);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 50;
    let bream = give(&mut g, 0, "BG26_137");
    give(&mut g, 0, "BG32_330"); // a Murloc to play
    let hi = g.players[0]
        .hand
        .iter()
        .position(|h| h.hid != bream)
        .unwrap();
    g.apply(0, Action::Play { hand: hi, pos: 0 }).unwrap();
    let m = g.players[0]
        .hand
        .iter()
        .find(|h| h.hid == bream)
        .unwrap()
        .item
        .as_minion()
        .unwrap();
    assert_eq!((m.atk, m.hp), (12, 12));
    // Once on the board it no longer triggers.
    let bi = g.players[0]
        .hand
        .iter()
        .position(|h| h.hid == bream)
        .unwrap();
    g.apply(0, Action::Play { hand: bi, pos: 0 }).unwrap();
    give(&mut g, 0, "BG32_330");
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    let on_board = g.players[0].board.iter().find(|m| m.eid == bream).unwrap();
    assert_eq!((on_board.atk, on_board.hp), (12, 12));
}

#[test]
fn choral_mrrrglr_gains_hand_stats_for_combat_only() {
    let mut g = game(43);
    g.cfg.record_log = true;
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG26_354"]); // 6/6
    set_board(&mut g, 1, &["BG36_345"]);
    give(&mut g, 0, "BG32_330"); // 3/3
    give(&mut g, 0, "BG26_137"); // 6/6
    g.simulate_combat(0, Some(1), None);
    let b = after_soc(&g, 0);
    assert_eq!((b[0].1, b[0].2), (15, 15), "{b:?}");
    assert_eq!(
        (g.players[0].board[0].atk, g.players[0].board[0].hp),
        (6, 6)
    );
}

#[test]
fn when_you_have_space_waits_for_room() {
    // Ozumat with a full board: the Tentacle appears only after a friendly minion dies.
    let mut g = game(50);
    g.cfg.record_log = true;
    g.cfg.triple_copies = 99;
    g.players[0].set_hero_power(0, cid("BG23_HERO_201p"));
    g.players[0].powers[0].counter = 2; // sold two minions -> 4/4 Tentacle
    set_board(&mut g, 0, &["BG36_345"; 7]);
    set_board(&mut g, 1, &["BG36_345"]);
    g.simulate_combat(0, Some(1), None);
    let frames = &g.last_frames;
    assert!(
        !frames[1].boards[0]
            .iter()
            .any(|m| m.is_card("BG23_HERO_201pt")),
        "no room after SoC"
    );
    let first = frames
        .iter()
        .position(|f| f.boards[0].iter().any(|m| m.is_card("BG23_HERO_201pt")));
    let first = first.expect("tentacle appears once there is space");
    let t = frames[first].boards[0]
        .iter()
        .find(|m| m.is_card("BG23_HERO_201pt"))
        .unwrap();
    assert_eq!((t.atk, t.max_hp), (4, 4));
    assert!(t.has(Kw::TAUNT));
}

#[test]
fn drekthar_copies_highest_attack_from_turn_seven() {
    let mut g = game(51);
    g.cfg.record_log = true;
    g.cfg.triple_copies = 99;
    g.players[0].set_hero_power(0, cid("BG22_HERO_002p"));
    let mut b = vec![minion(&mut g, 0, "BG36_345"), minion(&mut g, 0, "BG36_345")];
    b[1].set_stats(9, 2);
    g.players[0].board = b;
    set_board(&mut g, 1, &["BG36_345"]);
    g.simulate_combat(0, Some(1), None);
    assert_eq!(g.last_frames[1].boards[0].len(), 2, "locked before turn 7");
    g.turn = 7;
    g.simulate_combat(0, Some(1), None);
    let b = &g.last_frames[1].boards[0];
    assert_eq!(b.len(), 3);
    assert!(
        b[2].combat_only && b[2].atk == 9,
        "{:?}",
        b.iter().map(|m| (m.name(), m.atk)).collect::<Vec<_>>()
    );
}

fn tavern_stat_sum(g: &Game, p: PlayerId) -> i32 {
    g.players[p]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| m.atk + m.max_hp)
        .sum()
}

#[test]
fn this_game_refresh_effects_stack_and_fire_on_refresh() {
    let mut g = game(60);
    g.cfg.triple_copies = 99;
    // Golden Waveling dies in the tavern phase: two stacks of "+4/+4 on refresh".
    let w = g.with_recruit(0, |c| c.new_minion(0, db().golden_of(cid("BG34_856"))));
    g.players[0].board = vec![w];
    g.with_recruit(0, |c| {
        let e = c.boards[0][0].eid;
        c.destroy(e);
        c.resolve_deaths();
    });
    assert_eq!(g.players[0].effects.len(), 1);
    assert_eq!(g.players[0].effects[0].stacks, 2);
    g.players[0].gold = 10;
    g.players[0].free_refreshes = 0;
    let before_draw = g.players[0].shop.len();
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(g.players[0].shop.len(), before_draw);
    // Fresh tavern: base stats + 2 x (+4/+4) = +16 over the card base stats.
    let base: i32 = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| db().get(m.card).atk + db().get(m.card).hp)
        .sum();
    assert_eq!(tavern_stat_sum(&g, 0) - base, 16);
}

#[test]
fn extra_offers_apply_on_refresh_but_not_when_unfreezing() {
    // Ysera needs Dragons in the lobby.
    let seed = (61..200)
        .find(|s| {
            Game::new(*s, RulesConfig::default())
                .lobby_races
                .intersects(taverngym::types::Races::DRAGON)
        })
        .unwrap();
    let mut g = game(seed);
    g.players[0].set_hero_power(0, cid("TB_BaconShop_HP_062")); // Ysera
    g.players[0].gold = 10;
    g.apply(0, Action::Refresh).unwrap();
    let minions = |g: &Game| {
        g.players[0]
            .shop
            .iter()
            .filter(|s| s.item.as_minion().is_some())
            .count()
    };
    let dragons = |g: &Game| {
        g.players[0]
            .shop
            .iter()
            .filter_map(|s| s.item.as_minion())
            .filter(|m| m.is(taverngym::types::Races::DRAGON))
            .count()
    };
    assert_eq!(minions(&g), 4, "3 slots + Ysera's extra Dragon");
    assert!(dragons(&g) >= 1);
    // Freeze, end the turn for everyone: next turn the tavern is kept, no extra added.
    g.apply(0, Action::Freeze).unwrap();
    for p in 0..8 {
        if g.needs_action(p) {
            g.apply(p, Action::EndTurn).unwrap();
        }
    }
    assert_eq!(minions(&g), 4);
}

#[test]
fn trigger_other_minions_effects() {
    // Kelp Keeper: only Battlecry minions are valid targets; triggering one works.
    let mut g = game(70);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG36_701", "BG26_135", "BG36_345"]); // Keeper, Busker, Prisonguard
    g.apply(0, Action::Activate(0)).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!()
    };
    assert_eq!(
        options,
        vec![g.players[0].board[1].eid],
        "only the Battlecry minion"
    );
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].gold_next_turn, 1);
    assert_eq!(g.players[0].gold, 9);

    // Sky-hatch Runaway triggers Glim Guardian's Rally in the tavern: permanent +2 Attack.
    let mut g = game(71);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG36_243", "BG29_888"]);
    g.apply(0, Action::Activate(0)).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].board[1].atk, 3);

    // Herald Sticker: Start of Combat triggers Deathrattles without killing the minion.
    let mut g = game(72);
    g.cfg.record_log = true;
    g.cfg.triple_copies = 99;
    g.players[0].trinkets = vec![cid("BG32_MagicItem_306")];
    set_board(&mut g, 0, &["BG28_300"]);
    set_board(&mut g, 1, &["BG36_345"]);
    g.simulate_combat(0, Some(1), None);
    let b = &g.last_frames[1].boards[0];
    assert_eq!(
        b.len(),
        3,
        "Bonehead alive + 2 Skeletons: {:?}",
        b.iter().map(|m| m.name()).collect::<Vec<_>>()
    );
    assert!(b[0].is_card("BG28_300"));

    // Shudderwock unlocks on Turn 3.
    let mut g = game(73);
    g.players[0].set_hero_power(0, cid("TB_BaconShop_HP_022"));
    set_board(&mut g, 0, &["BG26_135"]);
    assert!(!g.can_hero_power(0, 0));
    g.turn = 3;
    assert!(g.can_hero_power(0, 0));
}

fn end_all(g: &mut Game) {
    for p in 0..8 {
        while !g.players[p].pending.is_empty() {
            let a = g.legal_actions(p)[0];
            g.apply_index(p, a).unwrap();
        }
        if g.needs_action(p) {
            g.apply(p, Action::EndTurn).unwrap();
        }
    }
}

#[test]
fn gold_spend_counter_fires_per_threshold_and_keeps_remainder() {
    let mut g = game(80);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG26_810", "BG26_135"]); // Courier + a Pirate (Busker)
    g.with_recruit(0, |c| {
        c.spend_gold(0, 3);
        assert_eq!(c.boards[0][1].atk, 3, "not yet");
        c.spend_gold(0, 10); // total 13 -> two thresholds of 5, remainder 3
        assert_eq!(c.boards[0][1].atk, 3 + 6);
        assert_eq!(c.boards[0][0].counter, 3);
    });
}

#[test]
fn lockbox_counts_down_hastens_and_opens_golden() {
    let mut g = game(81);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 50;
    let lb = cid("BG36_520t");
    let lock = |g: &Game| {
        g.players[0].hand.iter().find_map(|h| match &h.item {
            taverngym::state::Item::Spell { card, data } if *card == lb => Some(*data),
            _ => None,
        })
    };
    give(&mut g, 0, "BG36_520"); // Bilgewater Breakout
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert_eq!(lock(&g), Some(5));
    assert!(
        !g.legal_mask(0)[Action::Play { hand: 0, pos: 0 }.encode()],
        "Lockbox is unplayable"
    );
    give(&mut g, 0, "BG36_520");
    let bi = g.players[0]
        .hand
        .iter()
        .position(|h| h.item.card() != lb)
        .unwrap();
    g.apply(0, Action::Play { hand: bi, pos: 1 }).unwrap();
    assert_eq!(lock(&g), Some(4), "second one hastens instead");
    for _ in 0..3 {
        end_all(&mut g);
    }
    assert_eq!(lock(&g), Some(1));
    end_all(&mut g);
    assert_eq!(lock(&g), None, "opened");
    let golden = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .find(|m| m.golden)
        .unwrap();
    assert!(golden.triple_reward && !golden.races.is_none());
}

#[test]
fn fishbait_is_attacked_in_the_tavern_and_rewards_the_killer() {
    let mut g = game(82);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    // Flittering Bat (Beast, Rally: summon a 1/1 Beast) is the left-most Beast; Lionfish activates.
    set_board(&mut g, 0, &["BG36_200", "BG36_201"]);
    let shop_before = g.players[0].shop.len();
    g.apply(0, Action::Activate(1)).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    let b = &g.players[0].board;
    assert_eq!(
        (b[0].atk, b[0].hp),
        (1 + 5, 4 + 5),
        "killer permanently gains +5/+5"
    );
    assert!(
        b.iter().any(|m| m.is_card("BG36_200t")),
        "Rally fired: a bat was summoned"
    );
    assert_eq!(
        g.players[0].shop.len(),
        shop_before - 1,
        "Fishbait died, replaced card gone"
    );
    assert!(
        !g.players[0]
            .shop
            .iter()
            .any(|s| s.item.card() == cid("BG36_205"))
    );
}

#[test]
fn patient_scout_improves_each_turn_on_the_board_only() {
    let mut g = game(83);
    g.cfg.triple_copies = 99;
    give(&mut g, 0, "BG24_715");
    // In the hand nothing happens: a card only acts there when its text says so.
    end_all(&mut g);
    end_all(&mut g);
    let hi = g.players[0]
        .hand
        .iter()
        .position(|h| h.item.card() == cid("BG24_715"))
        .unwrap();
    assert_eq!(g.players[0].hand[hi].item.as_minion().unwrap().counter, 0);
    g.apply(0, Action::Play { hand: hi, pos: 0 }).unwrap();
    end_all(&mut g);
    end_all(&mut g);
    let i = g.players[0]
        .board
        .iter()
        .position(|m| m.is_card("BG24_715"))
        .unwrap();
    g.apply(0, Action::Sell(i)).unwrap();
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!("no discover")
    };
    assert!(options.iter().all(|o| o.as_minion().unwrap().tier == 3));
}

fn stats(g: &Game, p: PlayerId) -> Vec<(i32, i32)> {
    g.players[p].board.iter().map(|m| (m.atk, m.hp)).collect()
}

/// Cast the most recently given hand card on a friendly board minion.
fn cast_on(g: &mut Game, p: PlayerId, board_idx: usize) {
    let hand = g.players[p].hand.len() - 1;
    g.apply(p, Action::Play { hand, pos: 0 }).unwrap();
    let eid = g.players[p].board[board_idx].eid;
    let Some(Pending::Target { options, .. }) = g.players[p].pending.last() else {
        panic!("expected target")
    };
    let i = options.iter().position(|e| *e == eid).unwrap();
    g.apply(p, Action::Choose(i)).unwrap();
}

#[test]
fn blood_gems_are_tracked_and_can_be_confiscated() {
    let mut g = game(60);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG36_345", "BG36_345", "BG36_345"]); // 3/3s
    g.players[0].stats.gem_atk = 1; // gems are +2/+1
    let (l, r) = (g.players[0].board[0].eid, g.players[0].board[2].eid);
    g.with_recruit(0, |c| {
        c.play_gem(0, l);
        c.play_gem(0, r);
        c.play_gem(0, r);
    });
    assert_eq!(stats(&g, 0), vec![(5, 4), (3, 3), (7, 5)]);
    assert_eq!(g.players[0].board[2].gems, 2);
    give(&mut g, 0, "BG28_698"); // Gem Confiscation on the middle minion
    cast_on(&mut g, 0, 1);
    // Middle: 3/3 + 3 own gems (+6/+3) + stolen 3 gems (+6/+3).
    assert_eq!(stats(&g, 0), vec![(3, 3), (15, 9), (3, 3)]);
    assert_eq!(g.players[0].board[1].gems, 6);
    assert_eq!(g.players[0].board[0].gems, 0);
}

#[test]
fn gem_bonus_cards_and_roogug_chain() {
    let mut g = game(61);
    set_board(&mut g, 0, &["BG28_583", "BG36_345"]); // Roogug, Prisonguard
    let roo = g.players[0].board[0].eid;
    g.with_recruit(0, |c| c.play_gem(0, roo));
    // The gem on Roogug passes one on to the only other friendly minion.
    assert_eq!(g.players[0].board[1].gems, 1);
    assert_eq!(
        (g.players[0].board[1].atk, g.players[0].board[1].hp),
        (4, 4)
    );
    // Gem Day: Choose One (+1 Attack).
    give(&mut g, 0, "BG31_893");
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Play { hand, pos: 0 }).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].stats.gem_atk, 1);
    assert_eq!(g.players[0].stats.gem_hp, 0);
}

#[test]
fn hot_air_surveyor_doubles_blood_gems() {
    let mut g = game(62);
    set_board(&mut g, 0, &["BG30_121", "BG36_345"]);
    give(&mut g, 0, "BG20_GEM");
    cast_on(&mut g, 0, 1);
    assert_eq!(g.players[0].board[1].gems, 2);
    assert_eq!(
        (g.players[0].board[1].atk, g.players[0].board[1].hp),
        (5, 5)
    );
}

#[test]
fn vineweaver_permanent_gems_survive_combat() {
    let mut g = game(63);
    set_board(&mut g, 0, &["BG33_883", "BG33_430"]); // Vineweaver 5/5, Prodigious Tusker
    set_board(&mut g, 1, &["BG36_345"]);
    let before = stats(&g, 0);
    g.simulate_combat(0, Some(1), None);
    let vw = g.players[0]
        .board
        .iter()
        .find(|m| m.name() == "Razorfen Vineweaver")
        .unwrap();
    // Rally: 4 permanent gems (+4/+4); Tusker's gem on the attacker is combat-only.
    assert_eq!((vw.atk, vw.hp), (before[0].0 + 4, before[0].1 + 4));
    assert_eq!(vw.gems, 4);
}

fn shop_minions(g: &Game, p: PlayerId) -> Vec<(String, i32, i32)> {
    g.players[p]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| (m.name().to_string(), m.atk, m.hp))
        .collect()
}

fn play_last(g: &mut Game, p: PlayerId) {
    let hand = g.players[p].hand.len() - 1;
    let pos = g.players[p].board.len();
    g.apply(p, Action::Play { hand, pos }).unwrap();
}

#[test]
fn picky_eater_consumes_a_tavern_minion_and_portrait_picks_highest_health() {
    let mut g = game(70);
    set_board(&mut g, 0, &[]);
    let before = shop_minions(&g, 0);
    let total: i32 = before.iter().map(|m| m.1 + m.2).sum();
    give(&mut g, 0, "BG24_009"); // Picky Eater 1/1
    play_last(&mut g, 0);
    let after = shop_minions(&g, 0);
    assert_eq!(after.len(), before.len() - 1);
    let eaten: i32 = total - after.iter().map(|m| m.1 + m.2).sum::<i32>();
    let pe = &g.players[0].board[0];
    assert_eq!(pe.atk + pe.hp, 2 + eaten);

    // Implicator Portrait: Demons always eat the highest-Health tavern minion.
    let mut g = game(71);
    set_board(&mut g, 0, &[]);
    g.players[0].trinkets.push(cid("BG32_MagicItem_824"));
    let best = shop_minions(&g, 0).iter().map(|m| m.2).max().unwrap();
    give(&mut g, 0, "BG24_009");
    play_last(&mut g, 0);
    assert_eq!(g.players[0].board[0].hp, 1 + best);
}

#[test]
fn fodder_is_added_to_next_refreshes_and_feeds_demons() {
    let mut g = game(72);
    g.players[0].gold = 20;
    set_board(&mut g, 0, &["BG35_155"]); // Twisted Wrathguard 8/8 (a Demon)
    give(&mut g, 0, "BG35_150"); // Laboratory Assistant: a Fodder on the next 3 Refreshes
    play_last(&mut g, 0);
    assert_eq!(g.players[0].fodder_queue, vec![1, 1, 1]);
    let slots = shop_minions(&g, 0).len();
    let demon_stats = |g: &Game| g.players[0].board.iter().map(|m| m.atk + m.hp).sum::<i32>();
    let s0 = demon_stats(&g);
    for k in 1..=4 {
        g.apply(0, Action::Refresh).unwrap();
        let shop = shop_minions(&g, 0);
        // The Fodder is an extra minion; after feeding, its slot is refilled.
        let extra = usize::from(k <= 3);
        assert_eq!(shop.len(), slots + extra, "refresh {k}");
        assert!(shop.iter().all(|m| m.0 != "Demon Fodder"));
        let gained = demon_stats(&g) - s0;
        assert_eq!(
            gained,
            4 * k.min(3),
            "refresh {k}: one 2/2 Fodder eaten per refresh"
        );
    }
    assert!(g.players[0].fodder_queue.is_empty());
}

#[test]
fn fodder_waits_in_the_tavern_until_a_demon_arrives() {
    let mut g = game(73);
    g.players[0].gold = 20;
    set_board(&mut g, 0, &["BG36_345"]); // not a Demon
    g.players[0].trinkets.push(cid("BG35_MagicItem_151")); // Fodders +4/+4
    g.with_recruit(0, |c| c.add_fodder(0, 1, 1));
    g.apply(0, Action::Refresh).unwrap();
    assert!(
        shop_minions(&g, 0)
            .iter()
            .any(|m| *m == ("Demon Fodder".to_string(), 6, 6))
    );
    give(&mut g, 0, "BG35_150"); // a Demon 3/4 enters: the Fodder feeds it at once
    play_last(&mut g, 0);
    assert!(shop_minions(&g, 0).iter().all(|m| m.0 != "Demon Fodder"));
    let la = g.players[0].board.last().unwrap();
    assert_eq!((la.atk, la.hp), (9, 10));
}

#[test]
fn fodder_arrives_with_the_tavern_buffs_of_this_game() {
    // "Minions in the Tavern have +X/+X this game" applies to a Fodder too: it is a minion in the
    // Tavern.
    let mut g = game(76);
    g.players[0].gold = 20;
    set_board(&mut g, 0, &["BG36_345"]); // not a Demon: the Fodder waits in the Tavern
    g.players[0].stats.tavern_atk = 4;
    g.players[0].stats.tavern_hp = 3;
    g.with_recruit(0, |c| c.add_fodder(0, 1, 1));
    g.apply(0, Action::Refresh).unwrap();
    assert!(
        shop_minions(&g, 0)
            .iter()
            .any(|m| *m == ("Demon Fodder".to_string(), 6, 5))
    );
    give(&mut g, 0, "BG35_150"); // a Demon 3/4 enters and is fed the buffed Fodder
    play_last(&mut g, 0);
    let la = g.players[0].board.last().unwrap();
    assert_eq!((la.atk, la.hp), (3 + 6, 4 + 5));
}

#[test]
fn soul_devourer_and_consume_listeners() {
    let mut g = game(74);
    g.cfg.triple_copies = 99;
    // Shivarra and Conqueror listen; Consuming Claw adds keywords and +5/+5.
    set_board(&mut g, 0, &["BG34_Giant_311", "BG35_153", "BGS_034"]);
    g.players[0].trinkets.push(cid("BG36_MagicItem_801"));
    let gold = g.players[0].gold;
    let shiv = (g.players[0].board[0].atk, g.players[0].board[0].hp);
    let tav: Vec<_> = shop_minions(&g, 0);
    // Soul Devourer eats the Shivarra? No: target the Conqueror (a Demon).
    give(&mut g, 0, "BGS_059");
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Play { hand, pos: 3 }).unwrap();
    let target = g.players[0].board[1].eid;
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
        panic!("expected target")
    };
    let i = options.iter().position(|e| *e == target).unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
    assert_eq!(g.players[0].gold, gold + 3);
    let b = &g.players[0].board;
    assert_eq!(b.len(), 3, "Conqueror consumed");
    let sd = b.iter().find(|m| m.name() == "Soul Devourer").unwrap();
    // 3/3 + Conqueror 9/7 + Claw +5/+5.
    assert_eq!((sd.atk, sd.hp), (17, 15));
    // Shivarra gained the Conqueror's stats (friendly consumes count too) and the Claw
    // did not fire for it (it did not consume).
    let sh = &b[0];
    assert_eq!((sh.atk, sh.hp), (shiv.0 + 9, shiv.1 + 7));
    // The Conqueror buffed the tavern before being eaten? It was the one consumed, so no.
    assert_eq!(shop_minions(&g, 0), tav);
}

#[test]
fn statue_of_hireek_counts_tavern_consumes() {
    let mut g = game(75);
    set_board(&mut g, 0, &[]);
    g.players[0].trinkets.push(cid("BG32_MagicItem_902"));
    give(&mut g, 0, "BG27_009"); // Peckish Feldrake consumes 3
    let hand_before = g.players[0].hand.len();
    play_last(&mut g, 0);
    // 3 consumes: one spell after the 2nd, counter left at 1.
    assert_eq!(g.players[0].hand.len(), hand_before - 1 + 1);
    assert_eq!(g.players[0].counters[0].1, 1);
}

/// Play a Red Volumizer, then Magnetize a Green one onto an Apexis Guardian; returns the
/// board stats after that and after a later +2 Health Volumizer bonus.
fn volumizer_sequence(attached_grow: bool) -> [Vec<(i32, i32)>; 2] {
    let mut g = game(80);
    g.cfg.volumizer_attached_grow = attached_grow;
    g.players[0].gold = 20;
    set_board(&mut g, 0, &["BG34_173"]); // Apexis Guardian 7/5 Mech
    give(&mut g, 0, "BG34_170t3"); // Green Volumizer 3/3
    give(&mut g, 0, "BG34_170t"); // Red Volumizer 3/1
    play_last(&mut g, 0); // play the Red: Volumizers +3 Attack this game
    let st = &g.players[0].stats;
    assert_eq!((st.volumizer_atk, st.volumizer_hp), (3, 0));
    let red = &g.players[0].board[1];
    assert_eq!((red.atk, red.hp), (6, 1));
    let green = g.players[0].hand.last().unwrap().item.as_minion().unwrap();
    assert_eq!((green.atk, green.hp), (6, 3), "in hand too");
    // Magnetize the Green onto the Guardian: Volumizers +1/+1 more.
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Magnetize { hand, board: 0 }).unwrap();
    let st = &g.players[0].stats;
    assert_eq!((st.volumizer_atk, st.volumizer_hp), (4, 1));
    let after_magnetize = stats(&g, 0);
    g.with_recruit(0, |c| {
        c.bump_player_stat(0, |s| s.volumizer_hp += 2);
    });
    [after_magnetize, stats(&g, 0)]
}

#[test]
fn volumizers_buff_all_volumizers_wherever_they_are() {
    // Default: the attached Green is a fixed enchantment worth its stats at that moment
    // (3/3 + the 3 Attack so far). Later bonuses reach the Red on the board, not the host.
    let [a, b] = volumizer_sequence(false);
    assert_eq!(a, vec![(7 + 6, 5 + 3), (3 + 4, 1 + 1)]);
    assert_eq!(b, vec![(13, 8), (7, 4)]);
    // Old assumption (`volumizer_attached_grow`): the host has the current bonus once per
    // attached Volumizer, so later bonuses reach it too.
    let [a, b] = volumizer_sequence(true);
    assert_eq!(a, vec![(7 + 3 + 4, 5 + 3 + 1), (3 + 4, 1 + 1)]);
    assert_eq!(b, vec![(14, 11), (7, 4)]);
}

#[test]
fn chromadrakes() {
    let mut g = game(81);
    set_board(&mut g, 0, &["BG34_636t"]); // a Dragon on board (Green Chromadrake 3/5)
    give(&mut g, 0, "BG34_637t"); // Bronze: other Dragons +3/+1
    play_last(&mut g, 0);
    assert_eq!(g.players[0].board[0].atk, 6);
    assert_eq!(g.players[0].board[0].hp, 6);
    give(&mut g, 0, "BG34_638t"); // Red: Tavern spells +1 Attack this game
    play_last(&mut g, 0);
    assert_eq!(g.players[0].stats.spell_atk, 1);
    let n = g.players[0].hand.len();
    give(&mut g, 0, "BG34_634t"); // Blue: a 2-Cost Tavern spell
    play_last(&mut g, 0);
    let s = g.players[0].hand.last().unwrap();
    assert_eq!(g.players[0].hand.len(), n + 1);
    assert_eq!(db().get(s.item.card()).cost, 2);
}

fn hand_ids(g: &Game, p: PlayerId) -> Vec<String> {
    g.players[p]
        .hand
        .iter()
        .map(|h| db().get(h.item.card()).id.clone())
        .collect()
}

fn end_turn_all(g: &mut Game) {
    for p in 0..8 {
        while let Some(pend) = g.players[p].pending.last() {
            let _ = pend;
            g.apply(p, Action::Choose(0)).unwrap();
        }
        if g.needs_action(p) {
            g.apply(p, Action::EndTurn).unwrap();
        }
    }
}

#[test]
fn spellcraft_spells_are_temporary_and_buffs_last_until_next_turn() {
    let mut g = game(90);
    set_board(&mut g, 0, &[]);
    give(&mut g, 0, "BG23_000"); // Mini-Myrmidon 1/4
    play_last(&mut g, 0);
    assert_eq!(hand_ids(&g, 0), vec!["BG23_000t"], "spell on play");
    assert!(g.players[0].hand[0].temporary);
    cast_on(&mut g, 0, 0);
    assert_eq!(g.players[0].board[0].atk, 3);
    assert_eq!(g.players[0].stats.spellcraft_cast, 1);
    end_turn_all(&mut g);
    // Next turn: the buff has expired and a new spell arrived.
    assert_eq!(g.players[0].board[0].atk, 1);
    assert_eq!(hand_ids(&g, 0), vec!["BG23_000t"]);
    // Unused: removed at the end of the turn (not a discard).
    end_turn_all(&mut g);
    assert_eq!(hand_ids(&g, 0), vec!["BG23_000t"]);
    assert_eq!(g.players[0].stats.cards_discarded, 0);
}

#[test]
fn lava_lurker_makes_the_first_spellcraft_permanent() {
    let mut g = game(91);
    set_board(&mut g, 0, &["BG23_009", "BG23_000"]); // Lava Lurker 2/5, Mini-Myrmidon
    assert_eq!(g.players[0].board[0].counter, 1);
    for _ in 0..2 {
        g.with_recruit(0, |c| c.give_temp_spell(0, cid("BG23_000t"), 0));
        cast_on(&mut g, 0, 0);
    }
    let l = &g.players[0].board[0];
    assert_eq!(
        (l.atk, l.temp_atk),
        (6, 2),
        "first permanent, second temporary"
    );
}

#[test]
fn proud_privateer_casts_bounties_twice_or_three_times() {
    for (golden, mult) in [(false, 2), (true, 3)] {
        let mut g = game(92);
        let id = if golden { "BG33_825_G" } else { "BG33_825" };
        set_board(&mut g, 0, &[id]);
        let gold = g.players[0].gold;
        give(&mut g, 0, "BG33_815"); // Wealthy Bounty: gain 2 Gold
        play_last(&mut g, 0);
        assert_eq!(g.players[0].gold, gold + 2 * mult);
    }
}

#[test]
fn choose_one_effects_combined() {
    // Trailblazer Sticker: every Choose One card has both effects.
    let mut g = game(93);
    set_board(&mut g, 0, &[]);
    g.players[0].trinkets.push(cid("BG36_MagicItem_308"));
    give(&mut g, 0, "BG31_893"); // Gem Day
    play_last(&mut g, 0);
    assert!(g.players[0].pending.is_empty());
    assert_eq!(
        (g.players[0].stats.gem_atk, g.players[0].stats.gem_hp),
        (1, 1)
    );

    // Thorned Trailblazer: one card per turn.
    let mut g = game(94);
    set_board(&mut g, 0, &["BG31_327"]);
    give(&mut g, 0, "BG31_893");
    play_last(&mut g, 0);
    assert!(g.players[0].pending.is_empty());
    give(&mut g, 0, "BG31_893");
    play_last(&mut g, 0);
    assert!(matches!(
        g.players[0].pending.last(),
        Some(Pending::ChooseOne { .. })
    ));
    g.apply(0, Action::Choose(1)).unwrap();
    assert_eq!(
        (g.players[0].stats.gem_atk, g.players[0].stats.gem_hp),
        (1, 2)
    );
}

#[test]
fn spitescale_sushi_roll_first_two_spellcrafts_cast_twice() {
    let mut g = game(95);
    set_board(&mut g, 0, &["BG36_345"]);
    g.players[0].trinkets.push(cid("BG30_MagicItem_920"));
    for _ in 0..3 {
        g.with_recruit(0, |c| c.give_temp_spell(0, cid("BG23_000t"), 0));
        cast_on(&mut g, 0, 0);
    }
    assert_eq!(g.players[0].stats.spellcraft_cast, 5);
    assert_eq!(g.players[0].board[0].temp_atk, 10);
}

fn power_ids(g: &Game, p: PlayerId) -> Vec<String> {
    g.players[p]
        .powers
        .iter()
        .map(|h| db().get(h.card).id.clone())
        .collect()
}

#[test]
fn finley_discovers_a_replacement_hero_power() {
    let mut g = game(100);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_057"));
    let Some(Pending::HeroPowerChoice { options, slot }) = g.players[0].pending.last().cloned()
    else {
        panic!("expected a Hero Power choice")
    };
    assert_eq!((options.len(), slot), (3, 0));
    g.apply(0, Action::Choose(1)).unwrap();
    assert_eq!(g.players[0].powers.len(), 1);
    assert_eq!(g.players[0].powers[0].card, options[1]);
}

#[test]
fn genn_gets_two_hero_powers_on_turn_four() {
    let mut g = game(101);
    g.set_hero_power(0, 0, cid("BG35_HERO_001p"));
    for _ in 0..3 {
        end_turn_all(&mut g);
    }
    assert_eq!(g.turn, 4);
    assert_eq!(g.players[0].pending.len(), 2);
    g.apply(0, Action::Choose(0)).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    let ids = power_ids(&g, 0);
    assert_eq!(ids.len(), 2, "{ids:?}");
    assert_ne!(ids[0], ids[1]);
    assert!(!ids.contains(&"BG35_HERO_001p".to_string()));
}

#[test]
fn hero_power_unlocks_cooldowns_and_discounts() {
    let mut g = game(102);
    g.players[0].gold = 10;
    // Alexstrasza: "(Unlocks at Tier 4.)" parsed from the text.
    let alex = cid("TB_BaconShop_HP_064");
    assert!(!taverngym::content::hero_power_available(&g, 0, alex));
    g.players[0].tier = 4;
    assert!(taverngym::content::hero_power_available(&g, 0, alex));

    // Snake Eyes: gain 1-6 Gold, then unusable for that many turns.
    g.set_hero_power(0, 0, cid("BG28_HERO_400p"));
    let gold = g.players[0].gold;
    g.apply(0, Action::HeroPower(0)).unwrap();
    let roll = g.players[0].gold - gold + 1; // paid 1
    assert!((1..=6).contains(&roll));
    // The turn of the roll is the first of the count: a 1 is ready again next turn.
    for _ in 1..roll {
        end_turn_all(&mut g);
        assert!(!g.can_hero_power(0, 0), "still on cooldown");
    }
    end_turn_all(&mut g);
    g.players[0].gold = 10;
    assert!(g.can_hero_power(0, 0));

    // Patches: buying a Pirate discounts the next use; using it resets.
    let mut g = game(103);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_072"));
    let base = g.hero_power_price(0, 0).unwrap();
    g.with_recruit(0, |c| {
        c.emit(taverngym::event::Event::Bought {
            side: 0,
            card: cid("BG33_825"), // a Pirate
            eid: 0,
            sid: 0,
            price: 3,
        })
    });
    assert_eq!(g.hero_power_price(0, 0).unwrap(), base - 1);
    g.players[0].gold = 10;
    g.apply(0, Action::HeroPower(0)).unwrap();
    assert_eq!(g.hero_power_price(0, 0).unwrap(), base);
}

#[test]
fn power_of_spells_cast_when_bought_and_add_a_second_hero_power() {
    let mut g = game(104);
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG36_345"]); // a target for Boon of Light
    let sid = 999_999;
    g.players[0].shop.push(taverngym::state::ShopSlot::new(
        sid,
        Item::spell(cid("BG34_HeroPowerSpell_006")), // Power of George
    ));
    let i = g.players[0].shop.len() - 1;
    let hand = g.players[0].hand.len();
    g.apply(0, Action::Buy(i)).unwrap();
    assert_eq!(g.players[0].hand.len(), hand, "cast on purchase");
    assert_eq!(
        power_ids(&g, 0),
        vec!["TB_BaconShop_HP_035", "TB_BaconShop_HP_010"]
    );
    // The second Hero Power is usable through its own action.
    assert!(g.legal_mask(0)[Action::HeroPower(1).encode()]);
}

#[test]
fn denathrius_quest_progress_and_reward() {
    let mut g = game(110);
    g.set_hero_power(0, 0, cid("BG24_HERO_100p"));
    let Some(Pending::Quest { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("expected a Quest choice")
    };
    assert_eq!(options.len(), 2);
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].quests.len(), 1);
    // Force a known quest: Track the Footprints (Refresh N times) for Evil Twin.
    g.players[0].quests.clear();
    g.with_recruit(0, |c| {
        c.start_quest(0, cid("BG24_Quest_112"), cid("BG24_Reward_111"))
    });
    let goal = g.players[0].quests[0].goal;
    // The base number scaled by Evil Twin's 110% (8 -> 9).
    assert_eq!(db().get(cid("BG24_Quest_112")).quest_goal, 8);
    assert_eq!(goal, 9);
    g.players[0].gold = 100;
    for k in 1..=goal {
        g.apply(0, Action::Refresh).unwrap();
        if k < goal {
            assert_eq!(g.players[0].quests[0].progress, k);
        }
    }
    assert!(g.players[0].quests.is_empty());
    assert_eq!(g.players[0].rewards, vec![cid("BG24_Reward_111")]);
    // Evil Twin: Start of Combat copy of the highest-Health minion.
    g.cfg.record_log = true;
    set_board(&mut g, 0, &["BG36_345", "BG35_155"]); // 3/3, 8/8
    set_board(&mut g, 1, &["BG36_345"]);
    g.simulate_combat(0, Some(1), None);
    let b = after_soc(&g, 0);
    assert_eq!(b.len(), 3, "{b:?}");
    assert_eq!(b[2].0, "Twisted Wrathguard");
    assert!(b[2].3, "combat-only copy");
}

#[test]
fn humongozz_aura_and_balinda_multicast() {
    let mut g = game(120);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG36_345", "BG32_341"]); // Prisonguard 3/3, Humon'gozz
    give(&mut g, 0, "BG28_897"); // Banana +2/+2 (+1/+2 aura)
    cast_on(&mut g, 0, 0);
    assert_eq!(stats(&g, 0)[0], (6, 7));
    // Golden Balinda: three casts.
    set_board(&mut g, 0, &["BG36_345", "BG35_883_G"]);
    give(&mut g, 0, "BG28_897");
    cast_on(&mut g, 0, 0);
    assert_eq!(stats(&g, 0)[0], (9, 9));
}

#[test]
fn undead_destroy_effects() {
    let mut g = game(121);
    g.cfg.triple_copies = 99;
    // Disguised Graverobber destroys a Skeleton and gets a plain copy.
    set_board(&mut g, 0, &["BG_ICC_026t"]);
    g.players[0].board[0].atk = 9;
    give(&mut g, 0, "BG28_303");
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Play { hand, pos: 1 }).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("target")
    };
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(g.players[0].board.len(), 1, "Skeleton destroyed");
    let copy = g.players[0].hand.last().unwrap().item.as_minion().unwrap();
    assert_eq!((copy.card, copy.atk), (cid("BG_ICC_026t"), 1), "plain copy");

    // Butchering: destroy an Undead; Undead +8 Attack this game, wherever they are.
    set_board(&mut g, 0, &["BG_ICC_026t", "BG_ICC_026t"]);
    give(&mut g, 0, "BG28_604");
    cast_on(&mut g, 0, 0);
    assert_eq!(g.players[0].stats.undead_atk, 8);
    assert_eq!(stats(&g, 0), vec![(9, 1)]);
    let h = g.players[0]
        .hand
        .iter()
        .find_map(|h| h.item.as_minion())
        .unwrap();
    assert_eq!(h.atk, 9, "Skeleton in hand too");
}

#[test]
fn dead_bellringer_and_snazzy_phantom() {
    let mut g = game(122);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 5;
    // Bellringer, Skeleton, Phantom (right-most Undead).
    set_board(&mut g, 0, &["BG36_511", "BG_ICC_026t", "BG36_515"]);
    g.players[0].board[1].atk = 4;
    g.apply(0, Action::Activate(0)).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("target")
    };
    let skel = g.players[0].board[1].eid;
    let i = options.iter().position(|e| *e == skel).unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
    let b = stats(&g, 0);
    assert_eq!(b.len(), 3, "Skeleton came back with Reborn");
    assert_eq!(b[0], (3 + 4, 6 + 4), "Bellringer +4/+4");
    assert_eq!(b[1], (1, 1), "reborn with base stats and 1 Health");
    assert_eq!(
        b[2],
        (6 + 1, 8 + 1),
        "Phantom gained the reborn minion's Attack"
    );
}

#[test]
fn treasure_parrot_counts_damage_across_combats() {
    let mut g = game(123);
    set_board(&mut g, 0, &["BG36_763"]); // 5/5
    set_board(&mut g, 1, &["BG36_345"]); // 3/3
    g.players[0].board[0].counter = 33;
    g.simulate_combat(0, Some(1), None);
    assert!(g.players[0].board[0].counter >= 35);
    assert!(hand_ids(&g, 0).contains(&"BG28_830".to_string()));
}

#[test]
fn lurking_leviathan_improves_permanently() {
    let mut g = game(124);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG35_602"]);
    for (k, expect) in [(0, 4 + 3), (1, 4 + 6)] {
        give(&mut g, 0, "BG26_800"); // Manasaber 4/1 Beast
        play_last(&mut g, 0);
        assert_eq!(g.players[0].board[k + 1].atk, expect);
    }
    assert_eq!(g.players[0].board[0].counter, 2);
}

#[test]
fn ravaging_scorpid_grows_beetles_and_decoy_conjurer_steals() {
    let mut g = game(125);
    set_board(&mut g, 0, &["BG36_209"]); // 6/7
    set_board(&mut g, 1, &["BG35_602", "BG35_602"]); // 3/9s: the Scorpid gets to attack
    g.simulate_combat(0, Some(1), None);
    assert!(g.players[0].stats.beetle_atk >= 4, "persists after combat");

    let mut g = game(126);
    g.players[0].gold = 5;
    set_board(&mut g, 0, &["BG36_354"]);
    let best = shop_minions(&g, 0).iter().map(|m| m.1).max().unwrap();
    g.apply(0, Action::Activate(0)).unwrap();
    let m = g.players[0].hand.last().unwrap().item.as_minion().unwrap();
    assert_eq!(m.atk, best);
    assert_eq!(g.players[0].gold, 3);
}

#[test]
fn treasure_parrot_fires_once_and_golden_resumes_best_progress() {
    let mut g = game(127);
    g.cfg.triple_copies = 3;
    set_board(&mut g, 0, &["BG36_763", "BG36_763"]);
    g.players[0].board[0].counter = 40; // already fired
    g.players[0].board[1].counter = 20;
    give(&mut g, 0, "BG36_763");
    let hand = g.players[0].hand.clone();
    let golden = hand
        .iter()
        .find_map(|h| h.item.as_minion().filter(|m| m.golden))
        .unwrap();
    assert_eq!(golden.counter, 20);
}

#[test]
fn tarecgosa_and_poet_keep_combat_gains() {
    let mut g = game(130);
    // Hopebringer gives +4/+3 at Start of Combat; Tarecgosa keeps it, a Dragon next to the
    // golden Poet keeps double, the Prisonguard keeps nothing.
    set_board(
        &mut g,
        0,
        &["BG36_364", "BG21_015", "BG36_345", "BG29_813_G", "BG33_924"],
    );
    set_board(&mut g, 1, &["BG36_345"]);
    let before = stats(&g, 0);
    g.simulate_combat(0, Some(1), None);
    let after = stats(&g, 0);
    assert_eq!(after[1], (before[1].0 + 4, before[1].1 + 3), "Tarecgosa");
    assert_eq!(
        after[4],
        (before[4].0 + 8, before[4].1 + 6),
        "Blue Whelp next to golden Poet"
    );
    assert_eq!(after[2], before[2], "not a Dragon: not kept");
}

#[test]
fn scarlet_survivor_shield_at_six_attack() {
    let mut g = game(131);
    set_board(&mut g, 0, &["BG35_814"]); // 3/3
    let e = g.players[0].board[0].eid;
    g.with_recruit(0, |c| c.buff(e, 2, 0));
    assert!(!g.players[0].board[0].has(Kw::DIVINE_SHIELD));
    g.with_recruit(0, |c| c.buff(e, 1, 0));
    assert!(g.players[0].board[0].has(Kw::DIVINE_SHIELD));
}

#[test]
fn elemental_of_surprise_triples_with_any_elemental() {
    let mut g = game(132);
    set_board(&mut g, 0, &["BGS_126", "BGS_126"]); // two Wildfire Elementals
    give(&mut g, 0, "BG26_175");
    let golden = g.players[0]
        .hand
        .iter()
        .find_map(|h| h.item.as_minion().filter(|m| m.golden));
    assert_eq!(
        golden.map(|m| db().get(m.card).id.clone()),
        Some("TB_BaconUps_166".into())
    );
    assert!(g.players[0].board.is_empty());
}

#[test]
fn frostling_counts_elementals_wherever() {
    let mut g = game(133);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &[]);
    give(&mut g, 0, "BG26_537"); // Frostling 2/1 in hand
    for _ in 0..2 {
        give(&mut g, 0, "BG36_181"); // Air Baller (Elemental)
        play_last(&mut g, 0);
    }
    let f = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((f.atk, f.hp), (2 + 4, 1 + 2));
}

#[test]
fn wildfire_deals_excess_damage() {
    for (golden, hit) in [("BGS_126", 1), ("TB_BaconUps_166", 2)] {
        let mut g = game(134);
        let a = vec![minion(&mut g, 0, golden)];
        let b: Vec<Minion> = ["BG35_602", "BG_ICC_026t", "BG35_602"]
            .iter()
            .map(|id| minion(&mut g, 1, id))
            .collect();
        let (att, def) = (a[0].eid, b[1].eid);
        let mut ctx = taverngym::engine::Ctx::new(&mut g, [a, b], [Some(0), Some(1)], true);
        taverngym::combat::do_attack(&mut ctx, 0, att, def);
        let hurt = ctx.boards[1]
            .iter()
            .filter(|m| m.atk == 3 && m.hp < 9)
            .count();
        let excess = if golden == "BGS_126" { 6 - 1 } else { 12 - 1 };
        assert_eq!(hurt, hit);
        assert!(
            ctx.boards[1]
                .iter()
                .filter(|m| m.atk == 3)
                .all(|m| m.hp == 9 || m.hp == 9 - excess)
        );
    }
}

#[test]
fn living_prison_conflagration_and_air_baller() {
    let mut g = game(135);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 20;
    set_board(&mut g, 0, &["BG36_180"]); // Living Prison 4/5
    g.apply(0, Action::Activate(0)).unwrap();
    let slot = g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_some())
        .unwrap();
    let (a, h) = g.players[0].shop[slot]
        .item
        .as_minion()
        .map(|m| (m.atk, m.hp))
        .unwrap();
    g.apply(0, Action::Buy(slot)).unwrap();
    assert_eq!(stats(&g, 0)[0], (4 + a, 5 + h));
    // Conflagration: +4/+4 per (1 + Elementals played this turn).
    set_board(&mut g, 0, &["BG36_345"]);
    give(&mut g, 0, "BG36_181");
    play_last(&mut g, 0);
    give(&mut g, 0, "BG35_910");
    cast_on(&mut g, 0, 0);
    assert_eq!(stats(&g, 0)[0], (3 + 8, 3 + 8));
    // Air Baller sold: +2/+2 to the others.
    g.apply(0, Action::Sell(1)).unwrap();
    assert_eq!(stats(&g, 0)[0], (13, 13));
}

#[test]
fn mech_magnetize_synergies() {
    let mut g = game(140);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 20;
    // Drone Duplicator 5/2 armed (golden: 2 extra) + Interpreter.
    set_board(&mut g, 0, &["BG36_506_G", "BG31_177"]);
    g.apply(0, Action::Activate(0)).unwrap();
    give(&mut g, 0, "BG35_341"); // Enchanted Sentinel 3/5 Magnetic
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Magnetize { hand, board: 0 }).unwrap();
    let d = &g.players[0].board[0];
    // 10/4 + 3 x (3/5) + 3 x Interpreter (+3/+1).
    assert_eq!((d.atk, d.hp), (10 + 9 + 9, 4 + 15 + 3));
    assert_eq!(d.attached.len(), 3);
    // Three magnetized Sentinels: Tavern spells +3/+3.
    let e = g.with_recruit(0, |c| c.spell_extra(0));
    assert_eq!(e, (3, 3));
}

#[test]
fn spark_snapper_and_utility_drone() {
    let mut g = game(141);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG36_851", "BG26_152"]); // Spark Snapper, Utility Drone
    for expect in [(2 + 2, 1 + 3), (2 + 4, 1 + 6)] {
        give(&mut g, 0, "BG36_854"); // Rescue Bot 2/1 Mech
        play_last(&mut g, 0);
        let m = g.players[0].board.last().unwrap();
        assert_eq!((m.atk, m.hp), expect);
    }
    let before = stats(&g, 0);
    g.apply(0, Action::EndTurn).unwrap(); // end-of-turn hooks only (others still recruiting)
    let after = stats(&g, 0);
    // Utility Drone: +4/+5 per Magnetization (each played Mech got one Satellite).
    assert_eq!(after[2], (before[2].0 + 4, before[2].1 + 5));
    assert_eq!(after[0], before[0]);
}

#[test]
fn falling_sky_golem_counts_deathrattles() {
    let mut g = game(142);
    set_board(&mut g, 0, &["BG35_342"]); // 4/2
    g.with_recruit(0, |c| {
        c.bump_player_stat(0, |s| s.deathrattles_triggered += 3)
    });
    assert_eq!(stats(&g, 0)[0], (4 + 12, 2 + 6));
}

#[test]
fn bonus_keyword_counting_murlocs() {
    let mut g = game(150);
    g.cfg.triple_copies = 99;
    // Taunt (Gourmet) + Divine Shield/Windfury (Straight Shot) = 3 different Bonus Keywords.
    set_board(&mut g, 0, &["BG32_336", "BG25_016", "BG36_345"]);
    give(&mut g, 0, "BG32_337"); // Seafood Stew: +1/+1 x (1 + 3)
    cast_on(&mut g, 0, 2);
    assert_eq!(stats(&g, 0)[2], (7, 7));
    give(&mut g, 0, "BG31_148"); // Hackerfin: +3/+2 x (1 + 3)
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0)[2], (7 + 12, 7 + 8));
}

#[test]
fn shamanic_tidecaller_buffs_hand_and_board_murlocs() {
    let mut g = game(151);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG36_704", "BG32_860"]); // Tidecaller, Shoalfin Mystic 4/4
    give(&mut g, 0, "BG32_860"); // Mystic in hand
    give(&mut g, 0, "BG28_897"); // Banana on the board Mystic
    cast_on(&mut g, 0, 1);
    assert_eq!(stats(&g, 0)[1], (4 + 2 + 3, 4 + 2 + 3));
    let h = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((h.atk, h.hp), (7, 7));
}

#[test]
fn a_lone_magicfin_apprentice_has_no_target_for_its_taught_spell() {
    // Alone on the board (and an empty Tavern: Banana may also aim at a Tavern minion) its own body
    // is the only minion: no target, and the play does not wait on a choice.
    let mut g = game(153);
    g.players[0].hand.clear();
    g.players[0].shop.clear();
    set_board(&mut g, 0, &[]);
    let mut a = minion(&mut g, 0, "BG33_890t");
    a.counter = i32::from(cid("BG28_897").0) + 1; // taught Banana (CardId + 1)
    g.with_recruit(0, |c| c.add_to_hand(0, Item::Minion(a)));
    play_last(&mut g, 0);
    assert!(
        g.players[0].pending.is_empty(),
        "{:?}",
        g.players[0].pending
    );
    let m = &g.players[0].board[0];
    assert_eq!((m.atk, m.hp), (1, 1), "no Banana on itself");
}

#[test]
fn magicfin_apprentice_casts_its_taught_spell() {
    let mut g = game(152);
    g.players[0].gold = 20;
    set_board(&mut g, 0, &["BG33_891"]);
    g.players[0].shop.push(taverngym::state::ShopSlot::new(
        999_998,
        Item::spell(cid("BG28_897")), // Banana
    ));
    let i = g.players[0].shop.len() - 1;
    g.apply(0, Action::Buy(i)).unwrap();
    let ids = hand_ids(&g, 0);
    assert!(ids.contains(&"BG33_890t".to_string()), "{ids:?}");
    let hand = ids.iter().position(|x| x == "BG33_890t").unwrap();
    g.apply(0, Action::Play { hand, pos: 1 }).unwrap();
    // The Banana has a target, so the Battlecry asks for one: the Mycologist is chosen here.
    let myco = g.players[0].board[0].eid;
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
        panic!("the Apprentice asks for the spell's target")
    };
    // Not its own body (the client does not allow it).
    let apprentice = g.players[0].board.last().unwrap().eid;
    assert!(!options.contains(&apprentice), "{options:?}");
    let i = options.iter().position(|e| *e == myco).unwrap();
    let before = (g.players[0].board[0].atk, g.players[0].board[0].hp);
    g.apply(0, Action::Choose(i)).unwrap();
    let m = &g.players[0].board[0];
    assert_eq!(
        (m.atk, m.hp),
        (before.0 + 2, before.1 + 2),
        "Banana on the target"
    );
    let a = g.players[0].board.last().unwrap();
    assert_eq!((a.atk, a.hp), (1, 1));
}

#[test]
fn golden_pirates() {
    let mut g = game(160);
    g.cfg.triple_copies = 99;
    let l = minion(&mut g, 0, "BG32_236");
    assert!(
        l.golden && !l.triple_reward,
        "Aureate Laureate is always Golden"
    );
    // Extortionist counts Golden minions played, wherever it is.
    set_board(&mut g, 0, &["BG36_345", "BG33_830"]); // Prisonguard (T1 neutral), Cutlassier
    give(&mut g, 0, "BG36_524"); // Extortionist 7/7 in hand
    g.players[0].hand.push(taverngym::state::HandCard {
        hid: l.eid,
        item: Item::Minion(l),
        locked: 0,
        link: 0,
        group: 0,
        group_left: 0,
        temporary: false,
        src: 0,
        combined: false,
        held: false,
        held_gold: 0,
    });
    play_last(&mut g, 0); // play the golden Laureate
    let x = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((x.atk, x.hp), (14, 14));
    assert_eq!(g.players[0].hand.len(), 1, "no Triple Reward");
    // Elite Navigator: a targeted Battlecry; the chosen (Tier <= 4) Pirate Cutlassier becomes
    // Golden.
    give(&mut g, 0, "BG32_231");
    play_last(&mut g, 0);
    let cutlassier = g.players[0].board[1].eid;
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
        panic!("Elite Navigator asks for a target");
    };
    let i = options.iter().position(|e| *e == cutlassier).unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
    assert!(g.players[0].board[1].golden);
}

#[test]
fn greedy_conniver_and_hooktusk_discover() {
    let mut g = game(161);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG36_369_G", "BG36_344", "BG33_830"]);
    g.apply(0, Action::Sell(0)).unwrap();
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("Tier 7 Discover")
    };
    assert!(options.iter().all(|o| db().get(o.card()).pools.tier7));
    let before = stats(&g, 0)[1];
    g.apply(0, Action::Choose(0)).unwrap();
    // Hooktusk: other Pirates +1/+1 x (1 + Golden played = 0).
    assert_eq!(stats(&g, 0)[1], (before.0 + 1, before.1 + 1));
}

#[test]
fn malchezaar_refreshes_cost_health_and_soul_rewinder_rewinds() {
    let mut g = game(170);
    g.players[0].gold = 0;
    set_board(&mut g, 0, &["BG26_524", "BG26_523"]); // Malchezaar, Tichondrius
    let (hp, armor) = (g.players[0].health, g.players[0].armor);
    assert!(g.legal_mask(0)[Action::Refresh.encode()]);
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(g.players[0].health + g.players[0].armor, hp + armor - 1);
    assert_eq!(stats(&g, 0)[1], (8, 8), "Tichondrius: Demons +4/+4");
    // With Soul Rewinder the damage is rewound (once, however many rewinders).
    set_board(&mut g, 0, &["BG26_524", "BG26_174", "BG26_174"]);
    let (hp, armor) = (g.players[0].health, g.players[0].armor);
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!((g.players[0].health, g.players[0].armor), (hp, armor));
    assert_eq!(stats(&g, 0)[1], (4, 4), "Rewinder +2 Health");
    g.apply(0, Action::Refresh).unwrap();
    assert!(
        !g.legal_mask(0)[Action::Refresh.encode()],
        "two uses per turn, no gold"
    );
}

#[test]
fn imposing_percussionist_damages_by_discovered_tier() {
    let mut g = game(171);
    g.players[0].tier = 3;
    set_board(&mut g, 0, &[]);
    give(&mut g, 0, "BG26_525");
    play_last(&mut g, 0);
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("Discover")
    };
    let tier = db().get(options[0].card()).tier as i32;
    let total = g.players[0].health + g.players[0].armor;
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].health + g.players[0].armor, total - tier);
}

#[test]
fn blood_gem_barrage_stacks_on_refresh() {
    let mut g = game(180);
    g.players[0].gold = 10;
    set_board(&mut g, 0, &[]);
    for _ in 0..2 {
        give(&mut g, 0, "BG34_689");
        play_last(&mut g, 0);
    }
    g.apply(0, Action::Refresh).unwrap();
    let shop: Vec<_> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .collect();
    // Two casts: each gives every Tavern minion one Blood Gem of +2/+2.
    assert!(!shop.is_empty());
    for m in &shop {
        let d = db().get(m.card);
        assert_eq!(
            (m.gems, m.atk - d.atk, m.hp - d.hp),
            (2, 4, 4),
            "{}",
            d.name
        );
    }
}

#[test]
fn turbo_hogrider_and_geomant() {
    let mut g = game(181);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG31_323", "BG31_326"]); // Hogrider, Gem Rat 4/4 (Quilboar)
    give(&mut g, 0, "BG36_330"); // Sly Infiltrator (Choose One)
    play_last(&mut g, 0);
    g.apply(0, Action::Choose(0)).unwrap(); // 2 free Refreshes
    assert_eq!(g.players[0].free_refreshes, 2);
    assert_eq!(g.players[0].board[0].gems, 2, "itself too (36.6.3)");
    assert_eq!(g.players[0].board[1].gems, 2);
    assert_eq!(g.players[0].board[2].gems, 2, "the played Quilboar too");
    // Victorious Geomant with a nearly full hand: overflow Gems go to the left-most minion.
    let mut g = game(182);
    g.players[0].gold = 5;
    set_board(&mut g, 0, &["BG36_345", "BG36_370"]);
    for _ in 0..8 {
        give(&mut g, 0, "BG28_897");
    }
    g.apply(0, Action::Activate(1)).unwrap();
    assert_eq!(g.players[0].hand.len(), 10);
    assert_eq!(g.players[0].board[0].gems, 4);
}

#[test]
fn devolitionist_hits_highest_health_after_attacking() {
    let mut g = game(190);
    let a = vec![minion(&mut g, 0, "BG36_102")]; // 4/8
    let b: Vec<Minion> = ["BG36_345", "BG35_602"]
        .iter()
        .map(|id| minion(&mut g, 1, id))
        .collect();
    let (att, def) = (a[0].eid, b[0].eid);
    let mut ctx = taverngym::engine::Ctx::new(&mut g, [a, b], [Some(0), Some(1)], true);
    taverngym::combat::do_attack(&mut ctx, 0, att, def);
    assert_eq!(
        ctx.boards[1][1].hp,
        9 - 4,
        "Leviathan (highest Health) took 4"
    );
}

#[test]
fn discard_and_tea_set_aberrations() {
    let mut g = game(191);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG36_300", "BGFYM_005"]); // Frostcaller, Aph'lass
    g.players[0].deity = Some(taverngym::state::DeityState {
        card: cid("BGFYM_000"),
        atk: 0,
        hp: 0,
        golden: false,
        reborn: false,
    });
    give(&mut g, 0, "BG28_897");
    give(&mut g, 0, "BG28_897");
    for _ in 0..2 {
        g.players[0].board[0].activations = 0; // once per turn: pretend a new turn
        g.apply(0, Action::Activate(0)).unwrap();
        g.apply(0, Action::Choose(0)).unwrap(); // discard a hand card
    }
    assert_eq!(
        (g.players[0].stats.spell_atk, g.players[0].stats.spell_hp),
        (2, 2)
    );
    let d = g.players[0].deity.as_ref().unwrap();
    assert_eq!(
        (d.atk, d.hp),
        (2 + 4, 1 + 2),
        "Aph'lass improves by the base"
    );
    // Misplaced Tea Set: one minion per type.
    set_board(&mut g, 0, &["BG36_345", "BG26_800", "BG26_800"]); // neutral, 2 Beasts
    give(&mut g, 0, "BG28_888");
    play_last(&mut g, 0);
    let buffed = stats(&g, 0).iter().filter(|s| s.0 > 4).count();
    assert_eq!(buffed, 1, "only the Beast type is present");
}

#[test]
fn stone_age_slab_and_beatboxer() {
    let mut g = game(200);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 20;
    set_board(&mut g, 0, &["BG34_950"]);
    for k in 0..2 {
        let slot = g.players[0]
            .shop
            .iter()
            .position(|s| s.item.as_minion().is_some())
            .unwrap();
        let (a, h) = g.players[0].shop[slot]
            .item
            .as_minion()
            .map(|m| (m.atk, m.hp))
            .unwrap();
        g.apply(0, Action::Buy(slot)).unwrap();
        let m = g.players[0].hand.last().unwrap().item.as_minion().unwrap();
        if k == 0 {
            assert_eq!((m.atk, m.hp), ((a + 20) * 2, (h + 20) * 2));
        } else {
            assert_eq!((m.atk, m.hp), (a, h), "once per turn");
        }
    }
    // Polarizing Beatboxer: a Magnetization to another minion also Magnetizes to it.
    set_board(&mut g, 0, &["BG26_149", "BG36_854"]); // Beatboxer 5/10, Rescue Bot 2/1
    give(&mut g, 0, "BG35_341"); // Enchanted Sentinel 3/5
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Magnetize { hand, board: 1 }).unwrap();
    assert_eq!(stats(&g, 0), vec![(8, 15), (5, 6)]);
}

#[test]
fn stitched_salvager_stores_exact_copies() {
    let mut g = game(201);
    g.cfg.record_log = true;
    set_board(&mut g, 0, &["BG35_602", "BG31_999"]); // Leviathan (buffed), Salvager
    g.players[0].board[0].atk = 20;
    set_board(&mut g, 1, &["BG34_322"]); // Stalwart Kodo 16/32 kills the Salvager
    g.simulate_combat(0, Some(1), None);
    let b = after_soc(&g, 0);
    assert_eq!(
        b.len(),
        1,
        "left minion destroyed at Start of Combat: {b:?}"
    );
    // Deathrattle: an exact (20 Attack) copy comes back.
    let copied = g.last_frames.iter().any(|f| {
        f.boards[0]
            .iter()
            .any(|m| m.name() == "Lurking Leviathan" && m.atk == 20 && m.combat_only)
    });
    assert!(copied);
}

#[test]
fn stalwart_kodo_three_times_per_combat() {
    let mut g = game(202);
    let a = vec![minion(&mut g, 0, "BG34_322")];
    let kodo = a[0].eid;
    let mut ctx = taverngym::engine::Ctx::new(&mut g, [a, vec![]], [Some(0), None], true);
    for _ in 0..4 {
        let m = ctx.new_minion(0, cid("BG_ICC_026t"));
        ctx.summon(0, 1, m);
    }
    let buffed = ctx.boards[0]
        .iter()
        .filter(|m| m.eid != kodo && m.atk > 1)
        .count();
    assert_eq!(buffed, 3);
    assert_eq!(
        ctx.boards[0].last().unwrap().atk,
        1 + 16,
        "first summon buffed"
    );
}

#[test]
fn each_type_picks_the_assignment_covering_most_types() {
    for seed in 210..230 {
        let mut g = game(seed);
        g.cfg.triple_copies = 99;
        // Felboar (Demon/Quilboar) + Picky Eater (Demon): both get buffed (Quilboar + Demon).
        set_board(&mut g, 0, &["BG28_633", "BG24_009"]);
        let before = stats(&g, 0);
        give(&mut g, 0, "BG28_888");
        play_last(&mut g, 0);
        let after = stats(&g, 0);
        assert!(
            before.iter().zip(&after).all(|(b, a)| a.0 == b.0 + 4),
            "seed {seed}"
        );
    }
    let mut g = game(231);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG_GIL_681"; 7]);
    let n = g.with_recruit(0, |c| taverngym::content::distinct_type_count(c, 0));
    assert_eq!(n, 7, "each minion counts once");
}

#[test]
fn hero_damage_in_combat_is_rewound_but_losing_damage_is_not() {
    let mut g = game(232);
    let hp = g.players[0].health + g.players[0].armor;
    let a = vec![minion(&mut g, 0, "BG26_174")]; // Soul Rewinder
    let mut ctx = taverngym::engine::Ctx::new(&mut g, [a, vec![]], [Some(0), None], true);
    ctx.hero_damage(0, 3);
    drop(ctx);
    assert_eq!(
        g.players[0].health + g.players[0].armor,
        hp,
        "rewound in combat"
    );
    // Losing: the Rewinder is dead by then, the warband copy doesn't react.
    set_board(&mut g, 0, &["BG26_174"]);
    set_board(&mut g, 1, &["BG34_322"]);
    g.simulate_combat(0, Some(1), None);
    g.players[0].health = 30; // simulate_combat alone doesn't apply the hero damage
    let hp = g.players[0].health + g.players[0].armor;
    let dmg = 5;
    let mut ctx = taverngym::engine::Ctx::new(&mut g, [vec![], vec![]], [Some(0), None], false);
    ctx.hero_damage_ex(0, dmg, true);
    drop(ctx);
    assert_eq!(g.players[0].health + g.players[0].armor, hp - dmg);
}

#[test]
fn beatboxer_echoes_the_whole_magnet() {
    let mut g = game(233);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG26_149", "BG36_854"]); // Beatboxer 5/10, Rescue Bot 2/1
    // A Sentinel that already carries two magnetized Sentinels (9/15, 3 aura copies).
    let mut magnet = minion(&mut g, 0, "BG35_341");
    magnet.attached = vec![cid("BG35_341"), cid("BG35_341")];
    magnet.atk = 9;
    magnet.max_hp = 15;
    magnet.hp = 15;
    let bot = g.players[0].board[1].eid;
    g.with_recruit(0, |c| taverngym::recruit::magnetize_onto(c, 0, bot, magnet));
    let b = &g.players[0].board;
    assert_eq!(
        b[0].attached.len(),
        3,
        "echo keeps the magnet's attachments"
    );
    assert_eq!((b[0].atk, b[0].hp), (5 + 9, 10 + 15));
    let e = g.with_recruit(0, |c| c.spell_extra(0));
    assert_eq!(e, (6, 6), "six Sentinel auras in total");
}

fn put_in_shop(g: &mut Game, p: PlayerId, id: &str) -> usize {
    let sid = 900_000 + g.players[p].shop.len() as u32;
    g.players[p]
        .shop
        .push(taverngym::state::ShopSlot::new(sid, Item::spell(cid(id))));
    g.players[p].shop.len() - 1
}

#[test]
fn hasty_excavation_costs_health() {
    let mut g = game(240);
    g.players[0].gold = 0;
    let i = put_in_shop(&mut g, 0, "BG28_571");
    let total = g.players[0].health + g.players[0].armor;
    assert!(g.legal_mask(0)[Action::Buy(i).encode()]);
    g.apply(0, Action::Buy(i)).unwrap();
    assert!(g.players[0].health + g.players[0].armor < total);
    assert_eq!(g.players[0].gold, 0);
    play_last(&mut g, 0);
    assert_eq!(g.players[0].gold, 1);
}

#[test]
fn next_combat_spells_and_boon_of_beetles() {
    let mut g = game(241);
    g.cfg.record_log = true;
    set_board(&mut g, 0, &["BG36_345"]);
    set_board(&mut g, 1, &["BG34_322"]); // Kodo 16/32
    give(&mut g, 0, "BG28_573"); // Upper Hand
    play_last(&mut g, 0);
    give(&mut g, 0, "BG28_603"); // Boon of Beetles
    play_last(&mut g, 0);
    g.simulate_combat(0, Some(1), None);
    let enemy = &g.last_frames[1].boards[1];
    assert_eq!(enemy[0].hp, 1, "Upper Hand set the Kodo's Health to 1");
    assert!(
        g.players[0]
            .effects
            .iter()
            .all(|x| db().get(x.card).id != "BG28_573"),
        "used up"
    );
    let beetles = g
        .last_frames
        .iter()
        .map(|f| {
            f.boards[0]
                .iter()
                .filter(|m| m.name() == "Beetle" && m.has(Kw::TAUNT))
                .count()
        })
        .max()
        .unwrap();
    assert_eq!(beetles, 2);
    assert!(
        g.players[0]
            .effects
            .iter()
            .all(|x| db().get(x.card).id != "BG28_603"),
        "all owed Beetles summoned"
    );
}

#[test]
fn tavern_spell_variety() {
    let mut g = game(242);
    g.cfg.triple_copies = 99;
    // Saloon's Finest: only spells in the Tavern.
    give(&mut g, 0, "BG28_849");
    play_last(&mut g, 0);
    assert!(
        g.players[0]
            .shop
            .iter()
            .all(|s| s.item.as_minion().is_none())
    );
    // Menagerie Tableware: +3/+3 x (1 + 1 type: a dual-type minion counts once).
    set_board(&mut g, 0, &["BG28_633", "BG36_345"]); // Demon/Quilboar + neutral
    give(&mut g, 0, "BG34_272");
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0)[1], (3 + 6, 3 + 6));
    // Channel the Devourer: sell the Felboar, its stats go to the Prisonguard.
    let gold = g.players[0].gold;
    give(&mut g, 0, "EBG_Spell_032");
    cast_on(&mut g, 0, 0);
    assert_eq!(g.players[0].board.len(), 1);
    assert_eq!(g.players[0].gold, gold + 1);
    assert_eq!(stats(&g, 0)[0], (9 + 2 + 6, 9 + 6 + 6));
}

#[test]
fn tomb_turning_undead_dies_if_played_this_turn() {
    let seed = (243..400)
        .find(|s| {
            Game::new(*s, RulesConfig::default())
                .lobby_races
                .intersects(taverngym::types::Races::UNDEAD)
        })
        .unwrap();
    let mut g = game(seed);
    g.cfg.triple_copies = 99;
    g.players[0].tier = 6;
    set_board(&mut g, 0, &[]);
    give(&mut g, 0, "BG34_888");
    play_last(&mut g, 0);
    g.apply(0, Action::Choose(0)).unwrap();
    let doomed = g.players[0]
        .hand
        .last()
        .unwrap()
        .item
        .as_minion()
        .unwrap()
        .doomed_turn;
    assert_eq!(doomed, g.turn);
    // The observation shows it: the hand item's "frozen" flag (a hand card is never frozen).
    let flag = |g: &Game| {
        use taverngym::obs::{GLOBAL_F, ITEM_F, MINION_F, N_QUESTS, PENDING_KINDS, QUEST_F};
        let hand = GLOBAL_F + N_QUESTS * QUEST_F + PENDING_KINDS + 7 * MINION_F;
        let slot = g.players[0].hand.len() - 1;
        taverngym::obs::observe(g, 0).floats[hand + slot * ITEM_F + 6]
    };
    assert_eq!(flag(&g), 1.0);
    let mut later = g.clone();
    later.turn += 1;
    assert_eq!(flag(&later), 0.0, "next turn it is a normal card");
    play_last(&mut g, 0);
    assert!(
        g.players[0].board.iter().all(|m| m.doomed_turn != g.turn),
        "it died when played"
    );
}

#[test]
fn mass_blood_gem_hits_every_friendly_minion() {
    let mut g = game(250);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG36_345", "BG36_345"]);
    g.players[0].stats.gem_atk = 2;
    give(&mut g, 0, "BG20_GEMt");
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0), vec![(6, 4), (6, 4)]);
}

// ---------------------------------------------------------------------------- P3

#[test]
fn economy_heroes() {
    // Tae'thelan: every third Tavern spell bought costs 0.
    let mut g = game(260);
    g.players[0].gold = 30;
    g.set_hero_power(0, 0, cid("BG28_HERO_800p"));
    for k in 0..3 {
        let i = put_in_shop(&mut g, 0, "BG28_897");
        let before = g.players[0].gold;
        g.apply(0, Action::Buy(i)).unwrap();
        let paid = before - g.players[0].gold;
        assert_eq!(paid == 0, k == 2, "buy {k} paid {paid}");
    }
    // Aranna: once unlocked, the first minion each turn is free.
    let mut g = game(261);
    g.players[0].gold = 10;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_065"));
    g.players[0].powers[0].counter = 14;
    assert_eq!(g.minion_price(0), 0);
    let slot = g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_some())
        .unwrap();
    g.apply(0, Action::Buy(slot)).unwrap();
    assert_eq!(g.players[0].gold, 10);
    assert_eq!(g.minion_price(0), 3);
    // Guff: 20 Tiers' worth of cards -> Triple Reward.
    let mut g = game(262);
    g.set_hero_power(0, 0, cid("BG20_HERO_242p"));
    g.players[0].powers[0].counter = 19;
    g.players[0].gold = 10;
    let slot = g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_some())
        .unwrap();
    g.apply(0, Action::Buy(slot)).unwrap();
    assert!(hand_ids(&g, 0).contains(&"TB_BaconShop_Triples_01".to_string()));
}

fn choose_eid(g: &mut Game, p: PlayerId, eid: u32) {
    let Some(Pending::Target { options, .. }) = g.players[p].pending.last().cloned() else {
        panic!("target expected")
    };
    let i = options.iter().position(|e| *e == eid).unwrap();
    g.apply(p, Action::Choose(i)).unwrap();
}

#[test]
fn two_target_hero_powers() {
    // Vol'jin: two minions gain each other's Attack until next turn (3 and 8 -> 11 and 11).
    let mut g = game(270);
    g.cfg.triple_copies = 99;
    g.set_hero_power(0, 0, cid("BG20_HERO_201p"));
    set_board(&mut g, 0, &["BG36_345", "BG35_155"]); // 3/3, 8/8
    let (a, b) = (g.players[0].board[0].eid, g.players[0].board[1].eid);
    g.apply(0, Action::HeroPower(0)).unwrap();
    choose_eid(&mut g, 0, a);
    choose_eid(&mut g, 0, b);
    assert_eq!(stats(&g, 0), vec![(11, 3), (11, 8)]);
    // Mutanus: sell one, its stats go to another (random; no second menu).
    let mut g = game(271);
    g.cfg.triple_copies = 99;
    g.set_hero_power(0, 0, cid("BG20_HERO_301p"));
    set_board(&mut g, 0, &["BG36_345", "BG35_155"]);
    let b = g.players[0].board[1].eid;
    let gold = g.players[0].gold;
    g.apply(0, Action::HeroPower(0)).unwrap();
    choose_eid(&mut g, 0, b);
    assert!(
        g.players[0].pending.is_empty(),
        "the recipient is not chosen"
    );
    assert_eq!(stats(&g, 0), vec![(11, 11)]);
    assert_eq!(g.players[0].gold, gold + 1);
    // With several others the recipient varies, and it is never the devoured one.
    let mut got = std::collections::BTreeSet::new();
    for seed in 0..30 {
        let mut g = game(2710 + seed);
        g.cfg.triple_copies = 99;
        g.set_hero_power(0, 0, cid("BG20_HERO_301p"));
        set_board(&mut g, 0, &["BG35_155", "BG36_345", "BG36_345", "BG36_345"]);
        let eaten = g.players[0].board[0].eid;
        g.apply(0, Action::HeroPower(0)).unwrap();
        choose_eid(&mut g, 0, eaten);
        let fed: Vec<usize> = (0..3)
            .filter(|i| g.players[0].board[*i].atk == 11)
            .collect();
        assert_eq!(fed.len(), 1, "seed {seed}");
        got.insert(fed[0]);
    }
    assert!(got.len() > 1);
}

#[test]
fn galakrond_replaces_in_place() {
    let mut g = game(272);
    g.players[0].gold = 5;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_011"));
    let t = g.players[0].shop[1].sid;
    let tier = g.players[0].shop[1].item.as_minion().unwrap().tier;
    g.apply(0, Action::HeroPower(0)).unwrap();
    choose_eid(&mut g, 0, t);
    g.apply(0, Action::Choose(0)).unwrap();
    let m = g.players[0].shop[1].item.as_minion().unwrap();
    assert_eq!(m.tier, tier + 1);
    assert!(g.players[0].hand.is_empty());
}

#[test]
fn start_of_combat_heroes() {
    // Bru'kan: choose Fire; Start of Combat doubles the left-most minion's Attack.
    let mut g = game(280);
    g.cfg.record_log = true;
    g.set_hero_power(0, 0, cid("BG22_HERO_001p"));
    g.apply(0, Action::HeroPower(0)).unwrap();
    g.apply(0, Action::Choose(1)).unwrap(); // Fire
    assert!(
        g.players[0].hand.is_empty(),
        "the Element isn't kept in hand"
    );
    set_board(&mut g, 0, &["BG36_345"]);
    set_board(&mut g, 1, &["BG36_345"]);
    g.simulate_combat(0, Some(1), None);
    assert_eq!(after_soc(&g, 0)[0].1, 6);
    // N'Zoth: the starting Fish gains the Deathrattles of friendly minions that die.
    let mut g = game(281);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_105"));
    assert_eq!(g.players[0].board.len(), 1, "Fish on the board");
    let fish = g.players[0].board[0].eid;
    let rat = minion(&mut g, 0, "BG19_010"); // Sewer Rat (Deathrattle)
    g.players[0].board.insert(0, rat);
    set_board(&mut g, 1, &["BG34_322"]);
    g.simulate_combat(0, Some(1), None);
    let f = g.players[0].board.iter().find(|m| m.eid == fish).unwrap();
    assert!(f.gained_dr.is_empty(), "not kept after combat");
    // In the combat itself the Fish has the Deathrattle.
    let mut c = combat_ctx(&mut g);
    let rat = c.boards[0][0].eid;
    c.destroy(rat);
    c.resolve_deaths();
    let f = c.minion(fish).unwrap();
    assert_eq!(f.gained_dr, vec![cid("BG19_010")]);
}

#[test]
fn generation_heroes() {
    // Zephrys: two copies -> the third arrives and triples.
    let mut g = game(290);
    g.players[0].gold = 10;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_102"));
    set_board(&mut g, 0, &["BG36_345", "BG36_345"]);
    g.apply(0, Action::HeroPower(0)).unwrap();
    assert!(
        g.players[0]
            .hand
            .iter()
            .any(|h| h.item.as_minion().is_some_and(|m| m.golden))
    );
    // Dancin' Deryl: the hat passes on when sold.
    let mut g = game(291);
    g.cfg.triple_copies = 99;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_042"));
    set_board(&mut g, 0, &["BG36_345"]);
    give(&mut g, 0, "BG35_155");
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0)[1], (9, 9));
    g.apply(0, Action::Sell(1)).unwrap();
    assert_eq!(stats(&g, 0)[0], (4, 4), "hat passed to the Prisonguard");
    // Lord Jaraxxus: 150 damage -> Nether Portal.
    let mut g = game(292);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_036"));
    g.players[0].powers[0].counter = 149;
    set_board(&mut g, 0, &["BG35_155"]);
    set_board(&mut g, 1, &["BG36_345"]);
    g.simulate_combat(0, Some(1), None);
    assert_eq!(power_ids(&g, 0), vec!["TB_BaconShop_HP_036t"]);
}

fn earn(g: &mut Game, p: PlayerId, reward: &str) {
    // A 1-goal quest completes on its first progress: easiest is to push the reward directly.
    g.with_recruit(p, |c| {
        c.start_quest(0, cid("BG24_Quest_313"), cid(reward));
        let q = cid("BG24_Quest_313");
        let goal = c.player_ref(0).unwrap().quests.last().unwrap().goal;
        c.quest_progress(0, q, goal);
    });
}

#[test]
fn quest_rewards() {
    // Pilfered Lamps: two copies make a golden.
    let mut g = game(300);
    earn(&mut g, 0, "BG24_Reward_350");
    set_board(&mut g, 0, &["BG36_345"]);
    give(&mut g, 0, "BG36_345");
    assert!(
        g.players[0]
            .hand
            .iter()
            .any(|h| h.item.as_minion().is_some_and(|m| m.golden))
    );
    // Gilnean War Horn: get Murozond; Battlecries trigger an extra time.
    let mut g = game(301);
    earn(&mut g, 0, "BG27_Reward_802");
    assert!(hand_ids(&g, 0).contains(&"BGS_043".to_string()));
    assert_eq!(g.with_recruit(0, |c| c.battlecry_mult(0)), 2);
    // Norgannon's Reward: Tier 7 unlocked and a free upgrade next turn.
    let mut g = game(302);
    g.players[0].tier = 6;
    earn(&mut g, 0, "BG33_Reward_010");
    end_turn_all(&mut g);
    assert_eq!(g.players[0].tier, 7);
    // Quest offers never include anomaly / non-BG quests.
    let mut g = game(303);
    for _ in 0..20 {
        g.with_recruit(0, |c| c.offer_quests(0, 3));
        let Some(Pending::Quest { options, .. }) = g.players[0].pending.pop() else {
            panic!()
        };
        for (q, r) in options {
            assert!(taverngym::content::quest_offerable(
                g.lobby_races,
                &db().get(q).id
            ));
            assert!(taverngym::content::quest_offerable(
                g.lobby_races,
                &db().get(r).id
            ));
        }
    }
}

#[test]
fn darkmoon_prizes() {
    // Tickatus: every 4 turns, on the game's Turns 4, 8... also when attached after Turn 1 started:
    // tier-1 prizes on Turn 4.
    let mut g = game(310);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_106"));
    for _ in 0..3 {
        end_turn_all(&mut g);
    }
    assert_eq!(g.turn, 4);
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("prize Discover")
    };
    assert!(options.iter().all(|o| db().get(o.card()).prize_tier == 1));
    // Friends and Family Discount: minions cost 2 this game.
    let mut g = game(311);
    give(&mut g, 0, "BGS_Treasures_022");
    play_last(&mut g, 0);
    assert_eq!(g.minion_price(0), 2);
    // The Unlimited Coin comes back.
    give(&mut g, 0, "BGS_Treasures_014");
    let gold = g.players[0].gold;
    play_last(&mut g, 0);
    assert_eq!(g.players[0].gold, gold + 1);
    // It returns at the end of the turn; Drakkari Enchanter doubles it.
    set_board(&mut g, 0, &["BG26_ICC_901"]);
    g.apply(0, Action::EndTurn).unwrap();
    let coins = |g: &Game| {
        hand_ids(g, 0)
            .iter()
            .filter(|x| *x == "BGS_Treasures_014")
            .count()
    };
    assert_eq!(coins(&g), 2);
    assert!(
        g.players[0]
            .effects
            .iter()
            .all(|x| x.card != cid("BGS_Treasures_014"))
    );
    // Gruul Rules: the minion grows at the end of each turn.
    let mut g = game(312);
    set_board(&mut g, 0, &["BG36_345"]);
    give(&mut g, 0, "BGS_Treasures_009");
    cast_on(&mut g, 0, 0);
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(stats(&g, 0)[0], (7, 7));
}

#[test]
fn akazamzarak_secrets() {
    let mut g = game(320);
    g.cfg.record_log = true;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_020"));
    g.apply(0, Action::HeroPower(0)).unwrap();
    assert_eq!(
        g.players[0].pending.last().unwrap().len(),
        4,
        "choose 1 of 4 Secrets"
    );
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].secrets.len(), 1);
    assert!(g.players[0].hand.is_empty());
    // Autodefense Matrix: the attacked minion gets Divine Shield; the Secret is used up.
    g.players[0].secrets = vec![cid("TB_Bacon_Secrets_07")];
    set_board(&mut g, 0, &["BG35_602"]); // 3/9
    set_board(&mut g, 1, &["BG35_602"]); // 3/9: it will attack at some point
    g.simulate_combat(0, Some(1), None);
    assert!(g.players[0].secrets.is_empty(), "used up");
    // Ice Block: fatal damage is prevented once.
    let mut g = game(321);
    g.players[0].secrets = vec![cid("TB_Bacon_Secrets_12")];
    g.players[0].health = 3;
    g.players[0].armor = 0;
    let mut ctx = taverngym::engine::Ctx::new(&mut g, [vec![], vec![]], [Some(0), None], false);
    ctx.hero_damage_ex(0, 10, true);
    drop(ctx);
    assert_eq!(g.players[0].health, 3);
    assert!(g.players[0].secrets.is_empty());
}

#[test]
fn starcraft_heroes() {
    // Jim Raynor: Battlecruiser on the board; Refreshes offer Upgrades.
    let mut g = game(330);
    g.players[0].gold = 20;
    g.set_hero_power(0, 0, cid("BG31_HERO_801p"));
    assert_eq!(g.players[0].board.len(), 1);
    g.apply(0, Action::Refresh).unwrap();
    let up = g.players[0]
        .shop
        .iter()
        .position(|s| db().get(s.item.card()).id.starts_with("BG31_HERO_801pt"));
    assert!(up.is_some(), "an Upgrade is in the Tavern");
    give(&mut g, 0, "BG31_HERO_801pta"); // +6 Attack
    play_last(&mut g, 0);
    assert_eq!(g.players[0].board[0].atk, 2 + 6);
    // Kerrigan: the Larva morphs at the start of the turn (stats kept).
    let mut g = game(331);
    g.set_hero_power(0, 0, cid("BG31_HERO_811p"));
    g.players[0].board[0].atk = 5;
    end_turn_all(&mut g);
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("morph choice")
    };
    assert_eq!(options.len(), 3, "Tier 1 Zerg only");
    let pick = options[1].card();
    g.apply(0, Action::Choose(1)).unwrap();
    let z = &g.players[0].board[0];
    assert_eq!(z.card, pick);
    assert!(z.atk >= 5, "stats kept");
}

#[test]
fn galewing_and_thorim() {
    let mut g = game(332);
    g.set_hero_power(0, 0, cid("BG20_HERO_283p"));
    g.apply(0, Action::HeroPower(0)).unwrap();
    g.apply(0, Action::Choose(1)).unwrap(); // Ironforge: 2 Gold in 2 turns
    assert!(!g.can_hero_power(0, 0), "a flightpath is in progress");
    end_turn_all(&mut g);
    end_turn_all(&mut g);
    assert!(g.can_hero_power(0, 0), "completed");
    g.apply(0, Action::HeroPower(0)).unwrap();
    match g.players[0].pending.last() {
        Some(Pending::Discover { options, .. }) => {
            assert_eq!(options.len(), 2, "the completed path is not offered again");
            assert!(options.iter().all(|o| o.card() != cid("BG20_HERO_283p_t2")));
        }
        p => panic!("{p:?}"),
    }
    // Thorim: the Tier 7 pick is in the hand from the start, locked until 60 Gold is spent.
    let mut g = game(333);
    g.set_hero_power(0, 0, cid("BG27_HERO_801p2"));
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(
        g.players[0].hand.len(),
        1,
        "a hand card like any other locked card"
    );
    let h = &g.players[0].hand[0];
    assert!(h.held && h.locked == 1 && db().get(h.item.card()).pools.tier7);
    g.players[0].gold = 10;
    assert!(
        g.apply(0, Action::Play { hand: 0, pos: 0 }).is_err(),
        "locked"
    );
    // The lock does not count down with the turns.
    end_turn_all(&mut g);
    end_turn_all(&mut g);
    assert!(g.players[0].hand[0].held && g.players[0].hand[0].locked == 1);
    g.with_recruit(0, |c| {
        c.gain_gold(0, 60);
        c.spend_gold(0, 60);
    });
    assert_eq!(g.players[0].hand.len(), 1, "the same card, no second copy");
    let h = &g.players[0].hand[0];
    assert!(!h.held && h.locked == 0);
    g.players[0].gold = 10;
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
}

#[test]
fn hero_offer_policy() {
    for seed in 0..40 {
        let g = Game::new(seed, RulesConfig::default());
        let offered: Vec<CardId> = g
            .players
            .iter()
            .flat_map(|p| match p.pending.last() {
                Some(Pending::HeroSelect { options }) => options.clone(),
                _ => vec![],
            })
            .collect();
        // Drest'agath is an Aberration-subset hero (tag 4757): guaranteed only when the
        // lobby has Aberrations.
        let aberrations = g
            .lobby_races
            .intersects(taverngym::types::Races::ABERRATION);
        for (id, want) in [
            ("BG36_HERO_000", usize::from(aberrations)),
            ("BG36_HERO_002", 1),
        ] {
            assert_eq!(
                offered.iter().filter(|h| **h == cid(id)).count(),
                want,
                "seed {seed}: {id}"
            );
        }
        for h in &offered {
            let a = db().get(*h).assoc;
            assert!(
                a.is_none() || a.intersects(g.lobby_races),
                "seed {seed}: type-specific hero"
            );
        }
    }
}

#[test]
fn elemental_of_surprise_converges_to_one_golden() {
    let mut g = game(340);
    for _ in 0..9 {
        let m = minion(&mut g, 0, "BG26_175");
        g.with_recruit(0, |c| {
            c.add_to_hand(0, Item::Minion(m));
        });
    }
    let surprises: Vec<&Minion> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .collect();
    assert_eq!(surprises.len(), 1, "9 Surprises -> 1 golden Surprise");
    assert!(surprises[0].golden);
    // A golden Surprise is still a wildcard: golden Surprise + 2 x X = golden X.
    let mut g = game(341);
    let gs = g.with_recruit(0, |c| {
        let mut m = c.new_minion(0, cid("BG26_175"));
        m.card = db().get(m.card).golden.unwrap();
        m.golden = true;
        m
    });
    set_board(&mut g, 0, &["BG31_815", "BG31_815"]);
    g.with_recruit(0, |c| {
        c.add_to_hand(0, Item::Minion(gs));
    });
    assert!(g.players[0].board.is_empty());
    let m = g.players[0]
        .hand
        .iter()
        .find_map(|h| h.item.as_minion())
        .unwrap();
    assert!(m.golden && db().normal_of(m.card) == cid("BG31_815"));
}

#[test]
fn per_card_freeze_and_varden() {
    let mut g = game(342);
    let keep = g.players[0].shop[0].sid;
    let others: Vec<u32> = g.players[0].shop[1..].iter().map(|s| s.sid).collect();
    g.players[0].shop[0].frozen = true;
    end_turn_all(&mut g);
    let sids: Vec<u32> = g.players[0].shop.iter().map(|s| s.sid).collect();
    assert!(sids.contains(&keep), "the frozen card stays");
    assert!(
        others.iter().all(|s| !sids.contains(s)),
        "unfrozen cards are replaced"
    );
    assert!(!g.players[0].is_frozen(), "freeze lasts one turn");
    // The Freeze button freezes only what is in the Tavern now.
    g.apply(0, Action::Freeze).unwrap();
    assert!(g.players[0].shop.iter().all(|s| s.frozen));
    // Varden: after a Refresh, the highest-Tier minion and its copy are frozen (only those).
    let mut g = game(343);
    g.players[0].gold = 10;
    g.set_hero_power(0, 0, cid("BG22_HERO_004p"));
    g.apply(0, Action::Refresh).unwrap();
    let shop = &g.players[0].shop;
    let frozen: Vec<CardId> = shop
        .iter()
        .filter(|s| s.frozen)
        .map(|s| s.item.card())
        .collect();
    assert_eq!(frozen.len(), 2);
    assert_eq!(frozen[0], frozen[1]);
    let top = shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| m.tier)
        .max()
        .unwrap();
    assert_eq!(db().get(frozen[0]).tier, top);
}

#[test]
fn lost_staff_with_an_absent_type_never_fills_other_types() {
    let mut g = game(344);
    let absent = (0..11)
        .map(|i| taverngym::types::Races(1 << i))
        .find(|r| !r.intersects(g.lobby_races))
        .expect("5 types are out of the lobby");
    g.players[0].tier = 6;
    g.with_recruit(0, |c| c.refresh_shop_with(0, absent, false));
    for s in &g.players[0].shop {
        if let Some(m) = s.item.as_minion() {
            // Amalgams, or multi-type minions pooled through their other type.
            assert!(m.races.intersects(absent), "{}", db().get(m.card).id);
        }
    }
}

#[test]
fn pending_start_of_combat_spell_cannot_be_recast() {
    let mut g = game(345);
    g.players[0].gold = 20;
    give(&mut g, 0, "BG28_603");
    play_last(&mut g, 0);
    give(&mut g, 0, "BG28_603");
    let mask = g.legal_mask(0);
    assert!(!mask[taverngym::actions::A_PLAY]);
    assert!(g.apply(0, Action::Play { hand: 0, pos: 0 }).is_err());
}

#[test]
fn tavish_summons_the_minion_when_there_is_space() {
    let mut g = game(346);
    g.players[0].gold = 10;
    g.set_hero_power(0, 0, cid("BG22_HERO_000p_Alt"));
    let before = g.players[0].shop.len();
    g.apply(0, Action::HeroPower(0)).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].shop.len(), before - 1);
    assert_eq!(g.players[0].owed.len(), 1);
    // No space all combat: it stays owed.
    set_board(&mut g, 0, &["BG36_345"; 7]);
    set_board(&mut g, 1, &[]);
    fight(&mut g);
    assert_eq!(g.players[0].owed.len(), 1);
    // Space: summoned (and used up).
    set_board(&mut g, 0, &[]);
    set_board(&mut g, 1, &["BG35_602"]);
    fight(&mut g);
    assert!(g.players[0].owed.is_empty());
}

#[test]
fn tier7_minions_are_pooled_with_five_copies() {
    let g = game(347);
    let amalgam = cid("BG34_320"); // all types: always in the lobby
    assert!(g.universe.contains(&amalgam));
    assert_eq!(g.pool.get(amalgam), 5);
}

#[test]
fn skipped_turn_allows_only_freeze_and_end() {
    let mut g = game(348);
    g.players[0].skipping = true;
    g.players[0].gold = 10;
    let legal = g.legal_actions(0);
    assert_eq!(
        legal,
        vec![taverngym::actions::A_END, taverngym::actions::A_FREEZE]
    );
    assert!(g.apply(0, Action::Refresh).is_err());
    g.apply(0, Action::Freeze).unwrap();
}

#[test]
fn barov_guesses_own_combat_and_ghost_combats() {
    let heroes = [
        "TB_BaconShop_HERO_34",
        "TB_BaconShop_HERO_27",
        "TB_BaconShop_HERO_01",
    ];
    let mut g = game(349);
    for p in 0..8 {
        g.players[p].alive = p < 2;
    }
    g.players[1].hero = Some(cid(heroes[1]));
    g.pairs = vec![(0, Some(1))];
    g.players[0].gold = 10;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_081"));
    g.apply(0, Action::HeroPower(0)).unwrap();
    assert_eq!(
        g.players[0].pending.last().unwrap().len(),
        2,
        "your own combat"
    );
    // Odd number alive: player 2 vs the ghost of player 3.
    let mut g = game(350);
    for p in 0..8 {
        g.players[p].alive = p < 3;
    }
    g.players[2].hero = Some(cid(heroes[1]));
    g.players[3].hero = Some(cid(heroes[2]));
    g.eliminated = vec![3];
    g.pairs = vec![(0, Some(1)), (2, None)];
    g.players[0].gold = 10;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_081"));
    g.apply(0, Action::HeroPower(0)).unwrap();
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("guess")
    };
    let ghost = options
        .iter()
        .position(|o| o.card() == cid(heroes[2]))
        .unwrap();
    g.apply(0, Action::Choose(ghost)).unwrap();
    assert_eq!(
        g.players[0].powers[0].counter, -3,
        "the ghost beats player 2"
    );
}

/// Buy trinket `id` through the offer flow (gold is topped up to cover it).
fn buy_trinket(g: &mut Game, p: PlayerId, id: &str) {
    g.players[p].gold = g.players[p].gold.max(db().get(cid(id)).cost);
    g.players[p].pending.push(Pending::Trinket {
        options: vec![cid(id)],
        free: false,
        replace: None,
    });
    g.apply(p, Action::Choose(0)).unwrap();
    assert!(g.players[p].has_trinket(id));
}

fn put_shop(g: &mut Game, p: PlayerId, id: &str) -> usize {
    let m = minion(g, p, id);
    let sid = m.eid;
    g.players[p]
        .shop
        .push(taverngym::state::ShopSlot::new(sid, Item::Minion(m)));
    g.players[p].shop.len() - 1
}

#[test]
fn portraits_give_their_minion_and_change_it() {
    // Grifter Portrait: the first Pirate each turn is free.
    let mut g = game(400);
    buy_trinket(&mut g, 0, "BG32_MagicItem_957");
    assert!(hand_ids(&g, 0).contains(&"BG31_826".to_string()));
    g.players[0].shop.clear();
    let a = put_shop(&mut g, 0, "BG31_826");
    put_shop(&mut g, 0, "BG31_826");
    assert_eq!(g.slot_price(0, a).unwrap().0, 0);
    g.players[0].gold = 3;
    g.apply(0, Action::Buy(a)).unwrap();
    assert_eq!(g.players[0].gold, 3);
    assert_eq!(g.slot_price(0, 0).unwrap().0, 3, "only the first one");
    // Bronzebeard Portrait: Branns are Murlocs and Dragons.
    let mut g = game(401);
    buy_trinket(&mut g, 0, "BG30_MagicItem_418");
    let brann = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .find(|m| m.is_card("BG_LOE_077"))
        .unwrap();
    assert!(brann.is(taverngym::types::Races::MURLOC));
    assert!(brann.is(taverngym::types::Races::DRAGON));
    // Surveyor Portrait: a Blood Gem from hand gives +7/+7.
    let mut g = game(402);
    buy_trinket(&mut g, 0, "BG30_MagicItem_943");
    set_board(&mut g, 0, &["BG36_345"]);
    give(&mut g, 0, "BG20_GEM");
    cast_on(&mut g, 0, 0);
    assert_eq!(g.players[0].board[0].atk, 3 + 7);
    // Only the gem card itself: gems an effect plays while another hand spell resolves
    // (Blood Gem Barrage on the Refresh of Lost Staff of Hamuul) are plain gems.
    let mut g = game(402);
    buy_trinket(&mut g, 0, "BG30_MagicItem_943");
    g.players[0].hand.clear();
    // A minion with a type of this lobby, for the Staff to choose.
    let typed = *g
        .universe
        .iter()
        .find(|c| !db().get(**c).races.is_none() && db().get(**c).tier == 1)
        .unwrap();
    let m = g.with_recruit(0, |c| c.new_minion(0, typed));
    g.players[0].board = vec![m];
    g.with_recruit(0, |c| c.add_player_effect(0, cid("BG34_689"), 1));
    give(&mut g, 0, "EBG_Spell_038");
    cast_on(&mut g, 0, 0);
    let shop: Vec<_> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .collect();
    assert!(!shop.is_empty());
    for m in shop {
        assert_eq!(
            (m.gems, m.atk - db().get(m.card).atk),
            (1, 2),
            "{}",
            m.name()
        );
    }
    // Eternal Portrait (greater): Undead deaths grow Eternal Knights.
    let mut g = game(403);
    buy_trinket(&mut g, 0, "BG36_MagicItem_216");
    g.with_recruit(0, |c| {
        c.emit(taverngym::event::Event::Died {
            side: 0,
            eid: 999,
            card: cid("BG34_690"),
            races: taverngym::types::Races::UNDEAD,
        })
    });
    let k = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .find(|m| m.is_card("BG25_008"))
        .unwrap();
    assert_eq!((k.atk, k.hp), (8 + 4, 4 + 2));
    // Accord-o-Tron Portrait: one Accord-o-Tron each on the left- and right-most Mechs.
    let mut g = game(404);
    buy_trinket(&mut g, 0, "BG35_MagicItem_742");
    set_board(&mut g, 0, &["BG26_147", "BG36_345", "BG26_147"]);
    g.apply(0, Action::EndTurn).unwrap();
    let b = &g.players[0].board;
    assert_eq!((b[0].attached.len(), b[2].attached.len()), (1, 1));
    // Escapee Portrait: a held Lockbox opens a turn sooner.
    let mut g = game(405);
    give(&mut g, 0, "BG36_520t");
    let before = 5;
    if let Item::Spell { data, .. } = &mut g.players[0].hand[0].item {
        *data = before;
    }
    buy_trinket(&mut g, 0, "BG36_MagicItem_363");
    let lb = g.players[0]
        .hand
        .iter()
        .find_map(|h| match h.item {
            Item::Spell { card, data } if card == cid("BG36_520t") => Some(data),
            _ => None,
        })
        .unwrap();
    assert_eq!(lb, before - 1);
}

#[test]
fn sky_golem_portrait_grants_a_permanent_deathrattle() {
    let mut g = game(406);
    buy_trinket(&mut g, 0, "BG35_MagicItem_740");
    set_board(&mut g, 0, &["BG36_345", "BG35_602"]);
    set_board(&mut g, 1, &["BG35_602"]);
    let hp0 = g.players[0].board[1].hp;
    let boards = [g.players[0].board.clone(), g.players[1].board.clone()];
    let mut c = taverngym::engine::Ctx::new(&mut g, boards, [Some(0), Some(1)], true);
    c.run_player_hook(0, |s| s.start_of_combat, "soc");
    assert_eq!(c.boards[0][0].attached.len(), 1);
    let first = c.boards[0][0].eid;
    c.destroy(first);
    c.resolve_deaths();
    drop(c);
    assert_eq!(g.players[0].board[1].hp, hp0 + 2, "kept after combat");
}

/// A combat context over the two players' boards (for trinket combat hooks).
fn combat_ctx(g: &mut Game) -> taverngym::engine::Ctx<'_> {
    let boards = [g.players[0].board.clone(), g.players[1].board.clone()];
    taverngym::engine::Ctx::new(g, boards, [Some(0), Some(1)], true)
}

#[test]
fn trinket_avenge_and_first_deathrattle_bonus() {
    // Fridge Magnet: Avenge (3) in combat gets a random Magnetic minion.
    let mut g = game(410);
    buy_trinket(&mut g, 0, "BG30_MagicItem_545");
    g.players[0].hand.clear();
    g.players[0].tier = 4; // "random" cards are tier <= yours
    set_board(&mut g, 0, &["BG36_345", "BG36_345", "BG36_345", "BG35_602"]);
    let mut c = combat_ctx(&mut g);
    for _ in 0..3 {
        let e = c.boards[0][0].eid;
        c.destroy(e);
        c.resolve_deaths();
    }
    drop(c);
    let hand: Vec<_> = g.players[0].hand.iter().map(|h| h.item.card()).collect();
    assert_eq!(hand.len(), 1);
    assert!(db().get(hand[0]).mech.has(taverngym::types::Mech::MAGNETIC));
    // Deathly Phylactery: the first Deathrattle each combat triggers an extra time.
    let mut g = game(411);
    buy_trinket(&mut g, 0, "BG30_MagicItem_700");
    g.players[0].pending.clear();
    set_board(&mut g, 0, &["BG28_300", "BG28_300"]);
    let mut c = combat_ctx(&mut g);
    let skel = |c: &taverngym::engine::Ctx| {
        c.boards[0]
            .iter()
            .filter(|m| m.is_card("BG_ICC_026t"))
            .count()
    };
    let e = c.boards[0][0].eid;
    c.destroy(e);
    c.resolve_deaths();
    assert_eq!(skel(&c), 4, "first: twice");
    let e = c.boards[0]
        .iter()
        .find(|m| m.is_card("BG28_300"))
        .unwrap()
        .eid;
    c.destroy(e);
    c.resolve_deaths();
    assert_eq!(skel(&c), 6, "second: once (board full at 7 is not reached)");
}

#[test]
fn immediate_and_recurring_card_trinkets() {
    // Portable Factory: the Discovered minion is copied each turn.
    let mut g = game(412);
    buy_trinket(&mut g, 0, "BG32_MagicItem_361");
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("discover")
    };
    let pick = options[0].card();
    g.apply(0, Action::Choose(0)).unwrap();
    g.players[0].hand.clear();
    end_turn_all(&mut g);
    assert!(g.players[0].hand.iter().any(|h| h.item.card() == pick));
    // Water Wheel: twice per turn.
    let mut g = game(413);
    buy_trinket(&mut g, 0, "BG35_MagicItem_851");
    g.players[0].gold = 20;
    for _ in 0..3 {
        give(&mut g, 0, "BGS_119"); // Crackling Cyclone (Elemental)
        g.players[0].board.clear();
        play_last(&mut g, 0);
    }
    let spells = g.players[0]
        .hand
        .iter()
        .filter(|h| matches!(h.item, Item::Spell { .. }))
        .count();
    assert_eq!(spells, 2);
    // Rendle Sticker: steals the highest-Tier Tavern card.
    let mut g = game(414);
    g.players[0].shop.clear();
    put_shop(&mut g, 0, "BG36_345");
    put_shop(&mut g, 0, "BG35_602");
    let top = g.players[0].shop[1].item.card();
    buy_trinket(&mut g, 0, "BG32_MagicItem_817");
    assert!(g.players[0].hand.iter().any(|h| h.item.card() == top));
    assert_eq!(g.players[0].shop.len(), 1);
    // Knockoff Wisdomball: the next Refresh has a copy of a warband minion.
    let mut g = game(415);
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG36_345"]);
    give(&mut g, 0, "BG30_802");
    play_last(&mut g, 0);
    g.apply(0, Action::Refresh).unwrap();
    assert!(
        g.players[0]
            .shop
            .iter()
            .any(|s| s.item.card() == cid("BG36_345"))
    );
    // Duplicating Lens: only Tier 3 or below.
    let mut g = game(416);
    set_board(&mut g, 0, &["BG35_602"]);
    give(&mut g, 0, "BG35_MagicItem_817t");
    g.players[0].shop.clear();
    let low = put_shop(&mut g, 0, "BG36_345");
    let sid = g.players[0].shop[low].sid;
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Play { hand, pos: 0 }).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("target")
    };
    let i = options.iter().position(|x| *x == sid).unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
    assert!(
        g.players[0]
            .hand
            .iter()
            .any(|h| h.item.card() == cid("BG36_345"))
    );
    // Sinstone Sticker: a Discover gives an extra copy.
    let mut g = game(417);
    buy_trinket(&mut g, 0, "BG30_MagicItem_801");
    g.players[0].hand.clear();
    g.with_recruit(0, |c| {
        c.discover_minion(0, cid("BG30_MagicItem_801"), 3, |d| d.tier == 1)
    });
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].hand.len(), 2);
}

#[test]
fn economy_trinkets_change_prices_and_refreshes() {
    // The Eye of Sargeras: every fourth card costs Health.
    let mut g = game(420);
    buy_trinket(&mut g, 0, "BG30_MagicItem_701");
    g.players[0].gold = 20;
    let hp = g.players[0].health + g.players[0].armor;
    for i in 0..4 {
        g.players[0].shop.clear();
        put_shop(&mut g, 0, "BG36_345");
        let (_, health, _) = g.slot_price(0, 0).unwrap();
        assert_eq!(health, i == 3, "purchase {i}");
        g.apply(0, Action::Buy(0)).unwrap();
        g.players[0].hand.clear();
    }
    assert_eq!(g.players[0].gold, 20 - 9);
    assert_eq!(g.players[0].health + g.players[0].armor, hp - 3);
    // Pilgrimp Sticker: one Demon each turn costs Health.
    let mut g = game(421);
    buy_trinket(&mut g, 0, "BG32_MagicItem_821");
    g.players[0].shop.clear();
    put_shop(&mut g, 0, "BG26_174");
    put_shop(&mut g, 0, "BG26_174");
    assert!(g.slot_price(0, 0).unwrap().1);
    g.apply(0, Action::Buy(0)).unwrap();
    assert!(!g.slot_price(0, 0).unwrap().1, "once per turn");
    // Warcry Totem: the first two Battlecry minions each turn are free.
    let mut g = game(422);
    buy_trinket(&mut g, 0, "BG36_MagicItem_202");
    g.players[0].gold = 10;
    g.players[0].shop.clear();
    for _ in 0..3 {
        put_shop(&mut g, 0, "BG20_100");
    }
    g.apply(0, Action::Buy(0)).unwrap();
    g.apply(0, Action::Buy(0)).unwrap();
    assert_eq!(g.players[0].gold, 10);
    assert_eq!(g.slot_price(0, 0).unwrap().0, 3);
    // Guiding Candle: the first two Refreshes each turn are all Tier 6 cards.
    let mut g = game(423);
    buy_trinket(&mut g, 0, "BG32_MagicItem_366");
    g.players[0].gold = 10;
    for _ in 0..2 {
        g.apply(0, Action::Refresh).unwrap();
        for s in &g.players[0].shop {
            assert_eq!(
                db().get(s.item.card()).tier,
                6,
                "{}",
                db().get(s.item.card()).name
            );
        }
    }
    g.apply(0, Action::Refresh).unwrap();
    assert!(
        g.players[0]
            .shop
            .iter()
            .any(|s| db().get(s.item.card()).tier < 6)
    );
    // Dalaran Cheese Wheel: +2/+2, improved after 4 of your Refreshes.
    let mut g = game(424);
    buy_trinket(&mut g, 0, "BG30_MagicItem_879t");
    g.players[0].free_refreshes = 4;
    for _ in 0..4 {
        g.apply(0, Action::Refresh).unwrap();
    }
    assert_eq!(g.players[0].stats.tavern_atk, 4);
    // Transcribing Typewriter: an extra copy of the next 2 minions bought.
    let mut g = game(425);
    buy_trinket(&mut g, 0, "BG35_MagicItem_931");
    g.players[0].gold = 10;
    g.players[0].shop.clear();
    for id in ["BG36_345", "BG35_602", "BG20_100"] {
        put_shop(&mut g, 0, id);
    }
    for _ in 0..3 {
        g.apply(0, Action::Buy(0)).unwrap();
    }
    assert_eq!(
        g.players[0].hand.len(),
        5,
        "3 bought + 2 copies (no triple: all different)"
    );
    // Trickster's Sleeve: minion, then spell, then minion...
    let mut g = game(426);
    buy_trinket(&mut g, 0, "BG36_MagicItem_609");
    g.players[0].gold = 10;
    g.players[0].shop.clear();
    put_shop(&mut g, 0, "BG36_345");
    put_shop(&mut g, 0, "BG36_345");
    g.apply(0, Action::Buy(0)).unwrap();
    assert_eq!(g.players[0].gold, 8);
    g.apply(0, Action::Buy(0)).unwrap();
    assert_eq!(g.players[0].gold, 5, "now it waits for a spell");
}

#[test]
fn hammer_of_twilight_is_not_on_tavern_minions() {
    // "your minions" are those in play and in hand; the Tavern's minions do not have it, and a
    // bought one gets it.
    let at_base = |g: &Game| {
        for m in g.players[0].shop.iter().filter_map(|s| s.item.as_minion()) {
            let d = db().get(m.card);
            assert_eq!((m.atk, m.hp), (d.atk, d.hp), "{} in the Tavern", d.id);
        }
    };
    let mut g = game(435);
    put_shop(&mut g, 0, "BG36_345"); // offered before the Hammer
    buy_trinket(&mut g, 0, "BG36_MagicItem_403t");
    at_base(&g);
    give(&mut g, 0, "BG36_345");
    g.with_recruit(0, |c| {
        let hid = c.player_ref(0).unwrap().hand[0].hid;
        taverngym::recruit::discard_card(c, 0, hid);
    });
    at_base(&g);
    let k = 2; // 1 + one discarded card
    put_shop(&mut g, 0, "BG36_345"); // made with the owner's bonuses
    g.with_recruit(0, |_| {});
    at_base(&g);
    g.players[0].gold = 10;
    g.apply(0, Action::Refresh).unwrap();
    at_base(&g);
    let slot = g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_some())
        .unwrap();
    let card = g.players[0].shop[slot].item.card();
    g.apply(0, Action::Buy(slot)).unwrap();
    let h = g.players[0].hand.last().unwrap().item.as_minion().unwrap();
    let d = db().get(card);
    assert_eq!((h.card, h.atk, h.hp), (card, d.atk + 2 * k, d.hp + k));
}

#[test]
fn stat_and_aura_trinkets() {
    // Hammer of Twilight: your minions +1 Attack, improved by each discarded card.
    let mut g = game(430);
    set_board(&mut g, 0, &["BG36_345"]);
    buy_trinket(&mut g, 0, "BG36_MagicItem_403");
    assert_eq!(g.players[0].board[0].atk, 3 + 1);
    give(&mut g, 0, "BG36_345");
    g.with_recruit(0, |c| {
        let hid = c.player_ref(0).unwrap().hand[0].hid;
        taverngym::recruit::discard_card(c, 0, hid);
    });
    assert_eq!(g.players[0].board[0].atk, 3 + 2);
    // Artisanal Urn: Undead +20 Attack.
    let mut g = game(431);
    set_board(&mut g, 0, &["BG34_690", "BG36_345"]);
    buy_trinket(&mut g, 0, "BG30_MagicItem_989t");
    assert_eq!(g.players[0].board[0].atk, 4 + 20);
    assert_eq!(g.players[0].board[1].atk, 3);
    // Multilayered Shield: +1/+2 per different type plus the base.
    let mut g = game(432);
    set_board(&mut g, 0, &["BG34_690", "BG26_174"]); // Undead, Demon
    buy_trinket(&mut g, 0, "BG36_MagicItem_373");
    assert_eq!(g.with_recruit(0, |c| c.spell_extra(0)), (3, 6));
    // Weighted Gauntlet: +1/+1 at the end of this turn, +2/+2 at the end of the next.
    let mut g = game(433);
    set_board(&mut g, 0, &["BG35_602"]);
    buy_trinket(&mut g, 0, "BG36_MagicItem_414");
    end_turn_all(&mut g);
    let a = g.players[0].board[0].atk;
    assert_eq!(a, 3 + 1);
    g.players[0].hand.clear();
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(g.players[0].board[0].atk, a + 2);
    // Training Certificate: SoC doubles the two lowest-Attack minions.
    let mut g = game(434);
    buy_trinket(&mut g, 0, "BG30_MagicItem_962");
    set_board(&mut g, 0, &["BG26_174", "BG36_345", "BG35_602"]); // 4/2, 3/3, 3/9
    let mut c = combat_ctx(&mut g);
    c.run_player_hook(0, |s| s.start_of_combat, "soc");
    let stats: Vec<(i32, i32)> = c.boards[0].iter().map(|m| (m.atk, m.hp)).collect();
    assert_eq!(stats, vec![(4, 2), (6, 6), (6, 18)]);
}

#[test]
fn trigger_multiplier_trinkets() {
    // War Drum: the first Battlecry each turn triggers two extra times.
    let gems = |g: &Game| hand_ids(g, 0).iter().filter(|x| *x == "BG20_GEM").count();
    let mut g = game(440);
    g.players[0].gold = 10;
    give(&mut g, 0, "BG20_100"); // Razorfen Geomancer: Battlecry: get Blood Gems
    play_last(&mut g, 0);
    let per = gems(&g);
    assert!(per > 0);
    let mut g = game(440);
    buy_trinket(&mut g, 0, "BG32_MagicItem_416");
    g.players[0].hand.clear();
    g.players[0].gold = 10;
    give(&mut g, 0, "BG20_100");
    play_last(&mut g, 0);
    assert_eq!(gems(&g), 3 * per);
    give(&mut g, 0, "BG20_100");
    play_last(&mut g, 0);
    assert_eq!(gems(&g), 4 * per, "once per turn");
    // Lovely Locket: a spell on a friendly minion is cast on another one too.
    let mut g = game(441);
    buy_trinket(&mut g, 0, "BG36_MagicItem_211");
    set_board(&mut g, 0, &["BG36_345", "BG36_345"]);
    g.cfg.triple_copies = 99;
    give(&mut g, 0, "BG36_624"); // Repair Job: +4/+8
    cast_on(&mut g, 0, 0);
    let b = &g.players[0].board;
    assert_eq!((b[0].atk, b[1].atk), (7, 7));
    // Wolfhead Flail: at the end of your turn, trigger the two left-most Deathrattles.
    let mut g = game(442);
    g.cfg.triple_copies = 99;
    buy_trinket(&mut g, 0, "BG36_MagicItem_212");
    set_board(&mut g, 0, &["BG28_300"]); // Harmless Bonehead: two Skeletons
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(g.players[0].board.len(), 3);
    // A Deathrattle triggered on a living minion reads that minion's stats: Golemancy's Golem
    // is a copy of the host's Attack and Health, and in the Tavern it stays.
    let mut g = game(444);
    buy_trinket(&mut g, 0, "BG36_MagicItem_212");
    set_board(&mut g, 0, &["BG36_345"]);
    g.players[0].board[0].set_stats(9, 12);
    g.players[0].board[0]
        .attached
        .push(cid("BG36_MidGameEffect_000t61"));
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(stats(&g, 0), vec![(9, 12), (9, 12)]);
    assert!(
        g.players[0].board[1].attached.is_empty(),
        "the Golem carries no gift"
    );
    // Soul Fermenter: the 3 left-most die at SoC and come back when your last minion dies.
    let mut g = game(443);
    buy_trinket(&mut g, 0, "BG35_MagicItem_732");
    set_board(&mut g, 0, &["BG36_345", "BG35_602", "BG26_174", "BG20_100"]);
    let mut c = combat_ctx(&mut g);
    c.run_player_hook(0, |s| s.start_of_combat, "soc");
    assert_eq!(c.boards[0].len(), 1);
    let last = c.boards[0][0].eid;
    c.destroy(last);
    c.resolve_deaths();
    let names: Vec<&str> = c.boards[0].iter().map(|m| m.name()).collect();
    assert_eq!(names.len(), 3, "{names:?}");
}

#[test]
fn trinkets_that_replace_or_move_trinkets() {
    use taverngym::cards::CardKind;
    // Mystery Cube: a free swap between 2 Lesser Trinkets; it can't be declined.
    let mut g = game(450);
    buy_trinket(&mut g, 0, "BG30_MagicItem_703");
    let Some(Pending::Trinket {
        options,
        free,
        replace,
    }) = g.players[0].pending.last().cloned()
    else {
        panic!("swap offer")
    };
    assert!(free && replace == Some(cid("BG30_MagicItem_703")) && options.len() == 2);
    assert!(g.apply(0, Action::Cancel).is_err());
    g.players[0].gold = 0;
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(!g.players[0].has_trinket("BG30_MagicItem_703"));
    assert!(g.players[0].trinkets.contains(&options[0]));
    // Souvenir Stand: buying a Greater Trinket turns it into a copy of that trinket.
    let mut g = game(451);
    buy_trinket(&mut g, 0, "BG30_MagicItem_888");
    buy_trinket(&mut g, 0, "BG30_MagicItem_403");
    let anvil = cid("BG30_MagicItem_403");
    assert_eq!(g.players[0].trinkets, vec![anvil.copy(), anvil]);
    assert!(!g.players[0].has_trinket("BG30_MagicItem_888"));
    assert_eq!(db().get(anvil.copy()).id, "BG30_MagicItem_403");
    // Ornate Clock: the Greater Trinket offer comes next turn.
    let mut g = game(452);
    buy_trinket(&mut g, 0, "BG32_MagicItem_271");
    end_turn_all(&mut g);
    let Some(Pending::Trinket { options, .. }) = g.players[0].pending.last() else {
        panic!("greater offer next turn")
    };
    assert!(
        options
            .iter()
            .all(|t| db().get(*t).kind == CardKind::TrinketGreater)
    );
    // Mysterious Orb: the next offer is Lesser.
    let mut g = game(453);
    buy_trinket(&mut g, 0, "BG35_MagicItem_818");
    g.players[0].gold = 20;
    g.offer_trinkets(0, CardKind::TrinketGreater);
    let Some(Pending::Trinket { options, .. }) = g.players[0].pending.last() else {
        panic!("offer")
    };
    assert!(
        options
            .iter()
            .all(|t| db().get(*t).kind == CardKind::TrinketLesser)
    );
    // Corrupted Tome: a triple gives a Triple Prize instead of a Triple Reward.
    let mut g = game(454);
    buy_trinket(&mut g, 0, "BG35_MagicItem_812");
    g.players[0].hand.clear();
    g.players[0].gold = 20;
    let copies: Vec<Minion> = (0..3).map(|_| minion(&mut g, 0, "BG36_345")).collect();
    g.players[0].board = copies[..2].to_vec();
    g.with_recruit(0, |c| {
        c.add_to_hand(0, Item::Minion(copies[2].clone()));
    });
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert!(hand_ids(&g, 0).contains(&"BG35_MagicItem_812t".to_string()));
    assert!(!hand_ids(&g, 0).contains(&"TB_BaconShop_Triples_01".to_string()));
    // Quartermaster's Hook: after 50 Gold, a Greater Pirate Trinket.
    let mut g = game(455);
    buy_trinket(&mut g, 0, "BG36_MagicItem_408");
    g.with_recruit(0, |c| {
        c.gain_gold(0, 50);
        c.spend_gold(0, 50);
    });
    assert!(!g.players[0].has_trinket("BG36_MagicItem_408"));
    let t = *g.players[0].trinkets.last().unwrap();
    assert_eq!(db().get(t).kind, CardKind::TrinketGreater);
    assert!(
        db().get(t)
            .assoc
            .intersects(taverngym::types::Races::PIRATE)
    );
    // Shark Cannon (player-level Gold counter): 10 Gold -> Pirates +1/+1, then +2/+2.
    let mut g = game(456);
    buy_trinket(&mut g, 0, "BG32_MagicItem_232");
    set_board(&mut g, 0, &["BG31_826"]);
    g.with_recruit(0, |c| {
        c.gain_gold(0, 20);
        c.spend_gold(0, 20);
    });
    assert_eq!(g.players[0].board[0].atk, 6 + 1 + 2);
}

#[test]
fn keyword_magnetic_and_deity_trinkets() {
    use taverngym::state::DeityState;
    // Mask of Ancient Ones: the Deity becomes Golden (golden base stats added).
    let mut g = game(460);
    g.players[0].deity = Some(DeityState {
        card: cid("BGFYM_000"),
        atk: 5,
        hp: 5,
        golden: false,
        reborn: false,
    });
    buy_trinket(&mut g, 0, "BG36_MagicItem_602");
    let d = g.players[0].deity.clone().unwrap();
    assert!(d.golden);
    assert_eq!((d.atk, d.hp), (6, 6));
    // Corrupted Baton: Tavern spells grow the Deity.
    buy_trinket(&mut g, 0, "BG36_MagicItem_404");
    give(&mut g, 0, "BG28_603"); // Boon of Beetles (a Tavern spell without target)
    g.players[0].gold = 10;
    play_last(&mut g, 0);
    assert_eq!(g.players[0].deity.as_ref().unwrap().atk, 10);
    // Copper Coil: each Magnetization gives +2/+2, then +4/+4.
    let mut g = game(461);
    buy_trinket(&mut g, 0, "BG35_MagicItem_300t");
    set_board(&mut g, 0, &["BG26_147"]);
    g.players[0].gold = 20;
    for _ in 0..2 {
        give(&mut g, 0, "BG34_Giant_610t");
        let hand = g.players[0].hand.len() - 1;
        g.apply(0, Action::Magnetize { hand, board: 0 }).unwrap();
    }
    assert_eq!(g.players[0].board[0].atk, 3 + 4 + 4 + 2 + 4);
    // Gold Pendant: a friendly minion (Tier 4 or below) becomes Golden.
    let mut g = game(462);
    set_board(&mut g, 0, &["BG36_345"]);
    buy_trinket(&mut g, 0, "BG32_MagicItem_951");
    assert!(g.players[0].board[0].golden);
    // Shrine of Evolution: every minion becomes a Tier 4 minion.
    let mut g = game(463);
    set_board(&mut g, 0, &["BG36_345", "BG20_100"]);
    buy_trinket(&mut g, 0, "BG32_MagicItem_400");
    assert!(g.players[0].board.iter().all(|m| m.tier == 4));
    // Beetle Band: Avenge (5) summons a Taunt Beetle.
    let mut g = game(464);
    buy_trinket(&mut g, 0, "BG32_MagicItem_860");
    set_board(&mut g, 0, &["BG36_345"; 6]);
    let mut c = combat_ctx(&mut g);
    for _ in 0..5 {
        let e = c.boards[0][0].eid;
        c.destroy(e);
        c.resolve_deaths();
    }
    let b: Vec<(bool, bool)> = c.boards[0]
        .iter()
        .map(|m| (m.is_card("BG28_603t"), m.has(Kw::TAUNT)))
        .collect();
    assert_eq!(b, vec![(false, false), (true, true)]);
    drop(c);
    // Jarred Frostling: SoC two Elementals get "Deathrattle: summon a Flourishing Frostling".
    let mut g = game(465);
    buy_trinket(&mut g, 0, "BG30_MagicItem_952");
    set_board(&mut g, 0, &["BGS_119", "BGS_119"]);
    let mut c = combat_ctx(&mut g);
    c.run_player_hook(0, |s| s.start_of_combat, "soc");
    let e = c.boards[0][0].eid;
    c.destroy(e);
    c.resolve_deaths();
    assert!(c.boards[0].iter().any(|m| m.is_card("BG26_537")));
}

#[test]
fn gem_spellcraft_and_timewarp_trinkets() {
    use taverngym::state::DeityState;
    // Great Boar Sticker: 3 Blood Gems; they give an extra +2/+1.
    let mut g = game(470);
    g.players[0].hand.clear();
    buy_trinket(&mut g, 0, "BG30_MagicItem_988");
    assert_eq!(
        hand_ids(&g, 0).iter().filter(|x| *x == "BG20_GEM").count(),
        3
    );
    set_board(&mut g, 0, &["BG20_100"]);
    let (a, h) = (g.players[0].board[0].atk, g.players[0].board[0].hp);
    cast_on(&mut g, 0, 0);
    assert_eq!(
        (g.players[0].board[0].atk, g.players[0].board[0].hp),
        (a + 3, h + 2)
    );
    // Jewelry Box: its Blood Gem also gives a Quilboar a keyword.
    let mut g = game(471);
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG20_100"]);
    buy_trinket(&mut g, 0, "BG35_MagicItem_434");
    cast_on(&mut g, 0, 0);
    let m = &g.players[0].board[0];
    assert!(m.has(Kw::TAUNT) || m.has(Kw::DIVINE_SHIELD) || m.has(Kw::REBORN));
    // Token of the Old Gods: Spellcraft; the chosen minion becomes a Tier-higher pick.
    let mut g = game(472);
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG36_345"]);
    buy_trinket(&mut g, 0, "BG30_MagicItem_416");
    assert!(g.players[0].hand[0].temporary, "a Spellcraft spell");
    cast_on(&mut g, 0, 0);
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("discover")
    };
    let pick = options[0].card();
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].board[0].card, pick);
    assert_eq!(g.players[0].board[0].tier, 2);
    assert!(g.players[0].hand.is_empty());
    // Chillmere Mosaic: a Tavern of Battlecry minions that cost (1).
    let mut g = game(473);
    g.players[0].hand.clear();
    g.players[0].tier = 3;
    buy_trinket(&mut g, 0, "BG35_MagicItem_755");
    g.players[0].gold = 5;
    play_last(&mut g, 0);
    let shop = &g.players[0].shop;
    for (i, s) in shop.iter().enumerate() {
        if let Some(m) = s.item.as_minion() {
            assert!(db().get(m.card).mech.has(taverngym::types::Mech::BATTLECRY));
            assert_eq!(g.slot_price(0, i).unwrap().0, 1);
        }
    }
    // Timeworn Candelabra: 2-Cost Minor Timewarp (Tier 3) minions.
    let mut g = game(474);
    buy_trinket(&mut g, 0, "BG35_MagicItem_823");
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!("discover")
    };
    for o in options {
        let d = db().get(o.card());
        assert!(d.pools.timewarp && d.cost == 2 && d.tier == 3, "{}", d.id);
    }
    // Blood Golem Sticker: a dying Quilboar leaves a Golem with its Blood Gem stats.
    let mut g = game(475);
    buy_trinket(&mut g, 0, "BG30_MagicItem_442");
    set_board(&mut g, 0, &["BG20_100"]);
    g.with_recruit(0, |c| {
        let e = c.boards[0][0].eid;
        gems_on_test(c, e);
    });
    let mut c = combat_ctx(&mut g);
    let e = c.boards[0][0].eid;
    c.destroy(e);
    c.resolve_deaths();
    let golem = c.boards[0]
        .iter()
        .find(|m| m.is_card("BG30_MagicItem_442t"))
        .unwrap();
    assert_eq!((golem.atk, golem.hp), (2, 2));
    drop(c);
    // Makeshift Master: the chosen minion's gains this turn also go to the Deity.
    let mut g = game(476);
    g.players[0].hand.clear();
    g.players[0].deity = Some(DeityState {
        card: cid("BGFYM_000"),
        atk: 1,
        hp: 1,
        golden: false,
        reborn: false,
    });
    set_board(&mut g, 0, &["BG36_345"]);
    buy_trinket(&mut g, 0, "BG36_MagicItem_417");
    cast_on(&mut g, 0, 0);
    let e = g.players[0].board[0].eid;
    g.with_recruit(0, |c| c.buff(e, 2, 3));
    let d = g.players[0].deity.clone().unwrap();
    assert_eq!((d.atk, d.hp), (3, 4));
}

fn gems_on_test(c: &mut taverngym::engine::Ctx, e: u32) {
    c.play_gem(0, e);
    c.play_gem(0, e);
}

fn gifted(g: &mut Game, p: PlayerId, id: &str, gifts: &[&str]) -> Minion {
    let mut m = minion(g, p, id);
    for x in gifts {
        m.attached.push(cid(x));
    }
    m
}

#[test]
fn remaining_dark_gifts_and_stages() {
    // Steady Growth: stage by offer turn (3: +2/+2, 4: +2/+3, 5+: +3/+4); Time Turning also
    // fires it at the start of the turn.
    let mut g = game(480);
    let mut m = gifted(
        &mut g,
        0,
        "BG35_602",
        &["BG36_MidGameEffect_000t51", "BG36_MidGameEffect_000t21"],
    );
    m.gift_data = 1; // offered on turn 4
    g.players[0].board = vec![m];
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(
        (g.players[0].board[0].atk, g.players[0].board[0].hp),
        (3 + 2, 9 + 3)
    );
    end_turn_all(&mut g);
    let b = &g.players[0].board[0];
    assert_eq!((b.atk, b.max_hp), (3 + 4, 9 + 6), "EOT, then SoT again");
    // Polarization: a random Mech is Magnetized at the end of the turn.
    let mut g = (481..600)
        .map(game)
        .find(|g| g.lobby_races.intersects(taverngym::types::Races::MECH))
        .unwrap();
    g.players[0].tier = 4;
    let m = gifted(&mut g, 0, "BG26_147", &["BG36_MidGameEffect_000t65"]);
    g.players[0].board = vec![m];
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(g.players[0].board[0].attached.len(), 2);
    // Affinity: every 2 turns, a random minion of this type.
    let mut g = (482..600)
        .map(game)
        .find(|g| g.lobby_races.intersects(taverngym::types::Races::DEMON))
        .unwrap();
    g.players[0].tier = 3;
    let m = gifted(&mut g, 0, "BG26_174", &["BG36_MidGameEffect_000t82"]); // Demon
    g.players[0].board = vec![m];
    g.players[0].hand.clear();
    g.apply(0, Action::EndTurn).unwrap();
    assert!(g.players[0].hand.is_empty());
    end_turn_all(&mut g);
    g.players[0].hand.clear();
    g.apply(0, Action::EndTurn).unwrap();
    let h = g.players[0]
        .hand
        .first()
        .and_then(|h| h.item.as_minion())
        .unwrap();
    assert!(h.is(taverngym::types::Races::DEMON));
    // Tarecgosa's Blessing: combat gains are kept, doubled.
    let mut g = game(483);
    let m = gifted(&mut g, 0, "BG35_602", &["BG36_MidGameEffect_000t50"]);
    g.players[0].board = vec![m];
    let e = g.players[0].board[0].eid;
    let mut c = combat_ctx(&mut g);
    c.buff(e, 1, 1);
    drop(c);
    assert_eq!(
        (g.players[0].board[0].atk, g.players[0].board[0].hp),
        (3 + 2, 9 + 2)
    );
}

#[test]
fn dark_gift_compatibility() {
    use taverngym::content::gift_fits;
    let mut g = game(484);
    g.turn = 7;
    let fits = |g: &mut Game, gift: &str, m: &str| {
        g.with_recruit(0, |c| gift_fits(c, 0, cid(gift), db().get(cid(m)), 1))
    };
    let fort = "BG36_MidGameEffect_000t73"; // Fortitude (+5/+5)
    // Battlecry minions only take Double Vision / Replication / Gilding / Echoing Voice and
    // the three the client added (Hostility, Jaws of Death, Violent Toxicity).
    assert!(!fits(&mut g, fort, "BG20_100"));
    assert!(fits(&mut g, "BG36_MidGameEffect_000t11", "BG20_100"));
    // Deathrattle minions don't take stat gifts, except Sharpened Sword.
    assert!(!fits(&mut g, fort, "BG28_300"));
    assert!(fits(&mut g, "BG36_MidGameEffect_000t74", "BG28_300"));
    // Named requirements.
    assert!(
        !fits(&mut g, "BG36_MidGameEffect_000t15", "BG36_345"),
        "Toreth: Divine Shield"
    );
    assert!(
        !fits(&mut g, "BG36_MidGameEffect_000t69", "BG36_345"),
        "Toxicity: Murloc"
    );
    assert!(
        !fits(&mut g, "BG36_MidGameEffect_000t3", "BG35_602"),
        "Charisma exclusion"
    );
    assert!(
        !fits(&mut g, "BG36_MidGameEffect_000t50", "BG26_174"),
        "Tarecgosa: Dragon"
    );
    assert!(
        fits(&mut g, "BG36_MidGameEffect_000t66", "BG26_174"),
        "Demonology: Demon"
    );
    assert!(
        !fits(&mut g, "BG36_MidGameEffect_000t28", "BG35_602"),
        "no Battlecry yet"
    );
    g.players[0].stats.battlecries_triggered = 1;
    assert!(fits(&mut g, "BG36_MidGameEffect_000t28", "BG35_602"));
    // Every button offer: different gifts, each fitting its minion.
    for seed in 0..30 {
        let mut g = game(500 + seed);
        g.turn = 3 + (seed as u32 % 8);
        g.players[0].gold = 10;
        g.apply(0, Action::DarkGift).unwrap();
        let Some(Pending::Discover { options, .. }) = g.players[0].pending.last().cloned() else {
            panic!("seed {seed}: no offer")
        };
        assert_eq!(options.len(), 3, "seed {seed}: a full offer");
        let mut seen = vec![];
        for o in &options {
            let m = o.as_minion().unwrap();
            let gift = m.attached[0];
            assert!(!seen.contains(&gift), "different gifts");
            seen.push(gift);
            let tier = m.tier; // Gilding: only on the offer's lowest Tier
            assert!(g.with_recruit(0, |c| gift_fits(c, 0, gift, db().get(m.card), tier)));
        }
    }
    // Tras'tath: its own list, never Toxicity.
    let list: Vec<CardId> = taverngym::content::TRASTATH_GIFT_IDS
        .iter()
        .map(|x| cid(x))
        .collect();
    for seed in 0..20 {
        let mut g = game(540 + seed);
        g.set_hero_power(0, 0, cid("BG36_HERO_101p"));
        if let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() {
            for o in options {
                let gift = o.as_minion().unwrap().attached[0];
                assert!(list.contains(&gift) && gift != cid("BG36_MidGameEffect_000t69"));
            }
        }
    }
    // A gifted Discover from a trinket runs the gift's on-gain effect (Fortitude +5/+5).
    let mut g = game(560);
    g.players[0].hand.clear();
    let m = gifted(&mut g, 0, "BG35_602", &[fort]);
    g.players[0].pending.push(Pending::Discover {
        source: cid("BG36_MagicItem_206"), // Ominous Stone
        options: vec![Item::Minion(m)],
    });
    g.apply(0, Action::Choose(0)).unwrap();
    let h = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((h.atk, h.hp), (3 + 5, 9 + 5));
}

#[test]
fn m1_generated_minions_use_the_pool() {
    let mut g = game(600);
    let card = cid("BG36_345");
    let before = g.pool.get(card);
    g.players[0].hand.clear();
    g.with_recruit(0, |c| {
        c.give_card(0, card);
    });
    assert_eq!(
        g.pool.get(card),
        before - 1,
        "a generated copy comes from the pool"
    );
    g.players[0].board = vec![
        g.players[0]
            .hand
            .remove(0)
            .item
            .as_minion()
            .unwrap()
            .clone(),
    ];
    g.apply(0, Action::Sell(0)).unwrap();
    assert_eq!(g.pool.get(card), before, "and goes back when sold");
    // With no copy left the card is an extra one: selling it doesn't grow the pool.
    while g.pool.take(card) {}
    g.with_recruit(0, |c| {
        c.give_card(0, card);
    });
    g.players[0].board = vec![
        g.players[0]
            .hand
            .remove(0)
            .item
            .as_minion()
            .unwrap()
            .clone(),
    ];
    g.apply(0, Action::Sell(0)).unwrap();
    assert_eq!(g.pool.get(card), 0);
}

#[test]
fn m2_untargeted_text_allows_tavern_minions() {
    let mut g = game(601);
    g.players[0].shop.clear();
    let slot = put_shop(&mut g, 0, "BG36_345");
    let sid = g.players[0].shop[slot].sid;
    g.players[0].board.clear();
    give(&mut g, 0, "BG20_GEM"); // "Give a minion +1/+1."
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Play { hand, pos: 0 }).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("target")
    };
    assert!(options.contains(&sid));
    // Cloning Gallery says "a friendly minion": Tavern minions stay illegal.
    let s = taverngym::script::script(cid("BG31_HERO_005p")).unwrap();
    assert_eq!(s.hero_power_target, taverngym::script::TargetSpec::Friendly);
    // Lost Staff of Hamuul needs a minion with a type.
    let s = taverngym::script::script(cid("EBG_Spell_038")).unwrap();
    assert_eq!(s.spell_target, taverngym::script::TargetSpec::AnyTyped);
}

#[test]
fn m3_ties_are_random_and_m4_most_common_type() {
    let mut seen = std::collections::HashSet::new();
    for seed in 0..40 {
        let mut g = game(602 + seed);
        g.players[0].shop.clear();
        put_shop(&mut g, 0, "BG36_345");
        put_shop(&mut g, 0, "BG36_345");
        let best = g.with_recruit(0, |c| c.highest_health_tavern(0)).unwrap();
        seen.insert(
            g.players[0]
                .shop
                .iter()
                .position(|s| s.sid == best)
                .unwrap(),
        );
    }
    assert_eq!(seen.len(), 2, "either of two equal minions");
    use taverngym::types::Races;
    let mut g = game(650);
    g.lobby_races = Races::ALL; // only the lobby's types count
    // Amalgams count toward types some other minion has; alone they give no type.
    set_board(&mut g, 0, &["TB_BaconShop_HP_033t", "TB_BaconShop_HP_033t"]);
    assert_eq!(g.with_recruit(0, |c| c.most_common_race(0)), None);
    set_board(
        &mut g,
        0,
        &[
            "TB_BaconShop_HP_033t",
            "TB_BaconShop_HP_033t",
            "BG26_174",
            "BG36_201",
            "BG36_201",
        ],
    );
    assert_eq!(
        g.with_recruit(0, |c| c.most_common_race(0)),
        Some(Races::BEAST)
    );
    // Trinkets also count the hand.
    g.players[0].hand.clear();
    for id in ["BG23_357", "BG23_357", "BG26_174"] {
        give(&mut g, 0, id); // Demons (no triple)
    }
    assert_eq!(
        g.with_recruit(0, |c| c.most_common_race_ex(0, true)),
        Some(Races::DEMON)
    );
}

#[test]
fn m7_stacked_refresh_buffs_hit_one_minion() {
    let mut g = game(651);
    g.with_recruit(0, |c| c.add_player_effect(0, cid("BG34_444"), 2));
    g.players[0].gold = 5;
    g.apply(0, Action::Refresh).unwrap();
    let gains: Vec<i32> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| m.atk - db().get(m.card).atk)
        .filter(|x| *x > 0)
        .collect();
    assert_eq!(gains, vec![18]);
}

#[test]
fn m8_copies_keep_the_minion_as_it_is() {
    let mut g = game(652);
    g.players[0].hand.clear();
    let mut m = gifted(&mut g, 0, "BG35_602", &["BG36_MidGameEffect_000t11"]);
    m.add_stats(5, 5);
    g.players[0].pending.push(Pending::Discover {
        source: cid("BG36_MidGameEffect_000t11"),
        options: vec![Item::Minion(m)],
    });
    g.apply(0, Action::Choose(0)).unwrap();
    let stats: Vec<(i32, i32)> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .map(|m| (m.atk, m.hp))
        .collect();
    assert_eq!(stats, vec![(8, 14), (8, 14)], "Double Vision: a full copy");
}

#[test]
fn m9_external_battlecry_skips_choose_one() {
    let mut g = game(653);
    set_board(&mut g, 0, &["BG27_084", "BG36_201"]);
    let before = stats(&g, 0);
    let e = g.players[0].board[0].eid;
    g.with_recruit(0, |c| c.trigger_battlecry_ext(0, e));
    assert_eq!(stats(&g, 0), before, "a Choose One is not a Battlecry");
}

#[test]
fn m9_brann_does_not_repeat_a_choose_one() {
    // The chosen effect of a Choose One minion happens once with Brann on the board, and it
    // is not counted as a Battlecry triggered (Dark Paradox, Battle Scars, War Drum...).
    let mut g = game(657);
    set_board(&mut g, 0, &["BG_LOE_077"]); // Brann
    give(&mut g, 0, "BG36_330"); // Sly Infiltrator: Choose One - gain 2 free Refreshes; or...
    play_last(&mut g, 0);
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].free_refreshes, 2);
    assert_eq!(g.players[0].stats.battlecries_triggered, 0);
    // A Battlecry still triggers twice and counts twice.
    give(&mut g, 0, "BG26_135"); // Southsea Busker: 1 Gold next turn
    play_last(&mut g, 0);
    assert_eq!(g.players[0].gold_next_turn, 2);
    assert_eq!(g.players[0].stats.battlecries_triggered, 2);
}

#[test]
fn m10_turn_start_refill_triggers_but_does_not_count() {
    // Frigid Blossom says "after you Refresh": the turn-start refill doesn't trigger it, your own
    // Refresh does.
    let mut g = game(654);
    buy_trinket(&mut g, 0, "BG36_MagicItem_300"); // Frigid Blossom
    g.players[0].upgrade_cost = 5;
    end_turn_all(&mut g);
    assert_eq!(g.players[0].upgrade_cost, 5 - 1, "turn discount only");
    g.players[0].gold = 5;
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(
        g.players[0].upgrade_cost,
        5 - 1 - 1,
        "your Refresh triggers it"
    );
    let mut g = game(655);
    buy_trinket(&mut g, 0, "BG30_MagicItem_879t"); // improves every 4 Refreshes
    for _ in 0..4 {
        end_turn_all(&mut g);
    }
    assert_eq!(
        g.players[0].stats.tavern_atk, 2,
        "turn-start refills don't count"
    );
}

#[test]
fn m11_mutual_kills_count() {
    let mut g = game(656);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_053"));
    g.players[0].powers[0].counter = 1; // armed for this combat
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG36_345"]);
    set_board(&mut g, 1, &["BG36_345"]);
    let mut c = combat_ctx(&mut g);
    let (a, b) = (c.boards[0][0].eid, c.boards[1][0].eid);
    c.damage(a, b, 10);
    c.damage(b, a, 10);
    c.resolve_deaths();
    drop(c);
    assert!(
        hand_ids(&g, 0).contains(&"BG36_345".to_string()),
        "Rafaam steals it"
    );
}

#[test]
fn m12_end_of_turn_multiplier_reaches_trinkets() {
    let mut g = game(657);
    buy_trinket(&mut g, 0, "BG30_MagicItem_847"); // Goblin Wallet: +1 max Gold
    set_board(&mut g, 0, &["BG26_ICC_901"]); // Drakkari Enchanter
    let before = g.players[0].max_gold_bonus;
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(g.players[0].max_gold_bonus, before + 2);
}

#[test]
fn m13_deity_buffs_reach_the_awakened_deity() {
    use taverngym::state::DeityState;
    let mut g = game(658);
    g.players[0].deity = Some(DeityState {
        card: cid("BGFYM_000"),
        atk: 1,
        hp: 1,
        golden: false,
        reborn: false,
    });
    set_board(&mut g, 0, &["BGFYM_000"]);
    let mut c = combat_ctx(&mut g);
    let e = c.boards[0][0].eid;
    c.cs[0].deity_eid = e;
    taverngym::content::buff_deity(&mut c, 0, 2, 3);
    assert_eq!((c.boards[0][0].atk, c.boards[0][0].hp), (1 + 2, 1 + 3));
    drop(c);
    assert_eq!(g.players[0].deity.as_ref().unwrap().atk, 3);
}

#[test]
fn m14_evolving_stops_at_tier_6() {
    let mut g = game(659);
    g.players[0].tier7 = true;
    g.players[0].tier = 7;
    set_board(&mut g, 0, &["BG34_173"]); // Tier 6
    give(&mut g, 0, "BG30_804"); // Robust Evolution
    cast_on(&mut g, 0, 0);
    assert_eq!(g.players[0].board[0].tier, 6);
}

#[test]
fn m16_magnetizing_from_hand_is_playing_it() {
    let mut g = game(660);
    g.players[0].hand.clear();
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG34_173"]);
    let card = cid("BG35_341");
    let mut m = minion(&mut g, 0, "BG35_341_G");
    m.triple_reward = true;
    m.pool_copies = 1;
    let pool = g.pool.get(card);
    g.with_recruit(0, |c| {
        c.add_to_hand(0, Item::Minion(m));
    });
    g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
    assert_eq!(g.players[0].stats.minions_played_turn, 1);
    assert!(hand_ids(&g, 0).contains(&"TB_BaconShop_Triples_01".to_string()));
    assert_eq!(
        g.pool.get(card),
        pool + 1,
        "the magnet's copy returns right away"
    );
}

#[test]
fn m17_wheel_of_yogg_saron_spins() {
    for seed in 0..60 {
        let mut g = game(700 + seed);
        g.players[0].tier = 3;
        set_board(&mut g, 0, &["BG36_345", "BG35_602"]);
        g.with_recruit(0, |c| {
            taverngym::content::wheel_of_yogg(c, 0, seed % 2 == 0);
        });
    }
}

#[test]
fn m19_deathrattles_summon_where_their_minion_died() {
    let mut g = game(661);
    set_board(&mut g, 0, &["BG28_300", "BGS_115", "BG28_300"]);
    let mut c = combat_ctx(&mut g);
    let (a, b) = (c.boards[0][0].eid, c.boards[0][2].eid);
    c.destroy(a);
    c.destroy(b);
    c.resolve_deaths();
    let names: Vec<bool> = c.boards[0].iter().map(|m| m.is_card("BGS_115")).collect();
    assert_eq!(names, vec![false, false, true, false, false]);
}

#[test]
fn n1_higher_tier_attacks_first_on_equal_boards() {
    for seed in 0..10 {
        let mut g = game(800 + seed);
        g.cfg.record_log = true;
        set_board(&mut g, 0, &["BG36_345"]);
        set_board(&mut g, 1, &["BG36_345"]);
        g.players[1].tier = 3;
        g.log.clear();
        g.simulate_combat(0, Some(1), None);
        let first = g.log.iter().find(|l| l.contains(" attacks ")).unwrap();
        assert!(first.contains("p1"), "{first}");
    }
}

#[test]
fn n3_summons_left_of_the_attacker_wait() {
    let mut g = game(801);
    set_board(&mut g, 0, &["BG36_345", "BG35_602"]);
    set_board(&mut g, 1, &["BG35_602"]);
    let mut c = combat_ctx(&mut g);
    c.cs[0].last_attacker = c.boards[0][1].eid;
    let left = c.new_minion(0, cid("BG36_345"));
    let right = c.new_minion(0, cid("BG36_345"));
    let (l, r) = (left.eid, right.eid);
    c.summon(0, 0, left);
    c.summon(0, 3, right);
    assert!(c.minion(l).unwrap().attacked, "left of it: next round");
    assert!(!c.minion(r).unwrap().attacked, "right of it: this round");
}

#[test]
fn n7_cleave_carries_venomous_and_n6_damage_triggers_wait() {
    let mut g = game(802);
    set_board(&mut g, 0, &["BG36_345"]);
    set_board(&mut g, 1, &["BG35_602", "BG35_602", "BG35_602"]);
    let mut c = combat_ctx(&mut g);
    let a = c.boards[0][0].eid;
    c.give_kw(a, Kw::CLEAVE);
    c.give_kw(a, Kw::VENOMOUS);
    let d = c.boards[1][1].eid;
    taverngym::combat::do_attack(&mut c, 0, a, d);
    c.resolve_deaths();
    assert!(c.boards[1].is_empty(), "all three hits were Venomous");
}

#[test]
fn n10_n11_reborn_goes_right_of_its_summons_as_a_fresh_card() {
    let mut g = game(803);
    set_board(&mut g, 0, &["BG28_300"]); // Harmless Bonehead: two Skeletons
    g.players[0].board[0].give(Kw::REBORN);
    g.players[0].board[0].attached.push(cid("BG35_341")); // a Magnetic part
    let mut c = combat_ctx(&mut g);
    let e = c.boards[0][0].eid;
    c.destroy(e);
    c.resolve_deaths();
    let b = &c.boards[0];
    assert_eq!(b.len(), 3);
    assert!(b[0].is_card("BG_ICC_026t") && b[1].is_card("BG_ICC_026t"));
    assert!(b[2].is_card("BG28_300"), "reborn on the right");
    assert!(b[2].attached.is_empty(), "a normal Reborn keeps nothing");
}

#[test]
fn a_reborn_creation_is_assembled_from_its_two_cards_again() {
    // A Putricide Creation that comes back from Reborn is not a bare 1/1 without its effects:
    // the Creation is its two cards, so the fresh copy has them again (and their stats and
    // keywords), but nothing the dead one had gained since. Built through the Hero Power; the picks
    // are tried until the Creation has Reborn.
    let picks = (0..3).flat_map(|i| (0..3).map(move |j| (i, j)));
    let built = (900..940u64)
        .flat_map(|seed| picks.clone().map(move |(i, j)| (seed, i, j)))
        .find_map(|(seed, i, j)| {
            let mut g = game(seed);
            g.players[0].tier = 4;
            g.players[0].gold = 10;
            g.set_hero_power(0, 0, cid("BG25_HERO_100p"));
            g.apply(0, Action::HeroPower(0)).ok()?;
            g.apply(0, Action::Choose(i)).ok()?;
            g.apply(0, Action::Choose(j)).ok()?;
            let m = g.players[0]
                .hand
                .iter()
                .filter_map(|h| h.item.as_minion())
                .find(|m| m.is_card("BG25_HERO_100pt"))?
                .clone();
            m.has(Kw::REBORN).then_some((g, m))
        });
    let (mut g, made) = built.expect("a Creation with Reborn among the picks tried");
    assert_eq!(made.attached.len(), 2);
    let mut on_board = made.clone();
    on_board.set_stats(made.atk + 10, made.max_hp + 10); // a buff: not part of the card
    on_board.attached.push(cid("BG35_341")); // a Magnetic part: neither
    g.players[0].hand.clear();
    g.players[0].board = vec![on_board];
    let mut c = combat_ctx(&mut g);
    let e = c.boards[0][0].eid;
    c.destroy(e);
    c.resolve_deaths();
    let back = c.boards[0]
        .iter()
        .find(|x| x.is_card("BG25_HERO_100pt"))
        .expect("the Creation came back");
    assert_eq!(back.attached, made.attached, "its two cards, nothing else");
    assert_eq!(
        (back.atk, back.hp, back.max_hp),
        (made.atk, 1, made.max_hp),
        "the two cards' stats, 1 Health"
    );
    assert!(!back.has(Kw::REBORN));
    assert_eq!(back.has(Kw::TAUNT), made.has(Kw::TAUNT));
    assert!(taverngym::script::implemented(cid("BG25_HERO_100pt")));
}

#[test]
fn n16_n17_n19_n20_n21_triples() {
    // A locked hand card doesn't triple.
    let mut g = game(805);
    g.players[0].hand.clear();
    g.players[0].board.clear();
    let m = minion(&mut g, 0, "BG36_345");
    g.with_recruit(0, |c| {
        c.add_to_hand(0, Item::Minion(m));
    });
    g.players[0].hand[0].locked = 2;
    set_board(&mut g, 0, &["BG36_345", "BG36_345"]);
    g.with_recruit(0, |c| c.check_triples(0));
    assert_eq!(g.players[0].board.len(), 2, "the locked copy doesn't count");
    // Cards gained while the turn ends triple at the next turn start.
    let mut g = game(806);
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG36_345", "BG36_345"]);
    g.ending_turn = true;
    give(&mut g, 0, "BG36_345");
    assert_eq!(g.players[0].board.len(), 2);
    g.ending_turn = false;
    g.with_recruit(0, |c| c.check_triples(0));
    assert!(g.players[0].board.is_empty());
    // Losses of a part count too (golden base 6/6, one part -2 Attack).
    let mut g = game(807);
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG36_345", "BG36_345"]);
    g.players[0].board[0].atk = 1;
    give(&mut g, 0, "BG36_345");
    let gold = g.players[0]
        .hand
        .iter()
        .find_map(|h| h.item.as_minion())
        .unwrap();
    assert_eq!(gold.atk, 6 - 2);
    // A Tier 6 triple's reward is Tier 7 only in a Tier 7 Tavern.
    let mut g = game(808);
    g.players[0].tier = 6;
    let item = g.with_recruit(0, |c| taverngym::content::triple_reward_item(c, 0, 6));
    assert!(matches!(item, Item::Spell { data: 6, .. }));
    g.players[0].tier = 7;
    let item = g.with_recruit(0, |c| taverngym::content::triple_reward_item(c, 0, 6));
    assert!(matches!(item, Item::Spell { data: 7, .. }));
    // With a full hand the golden card is lost.
    let mut g = game(809);
    g.players[0].hand.clear();
    for _ in 0..10 {
        give(&mut g, 0, "BG28_810"); // Tavern Coins fill the hand
    }
    set_board(&mut g, 0, &["BG36_345", "BG36_345", "BG36_345"]);
    g.with_recruit(0, |c| c.check_triples(0));
    assert_eq!(g.players[0].hand.len(), 10);
    assert!(g.players[0].board.is_empty());
}

#[test]
fn n26_freeze_n33_ranking_n34_gold_cap() {
    // With part of the Tavern frozen, the button freezes everything.
    let mut g = game(810);
    g.players[0].shop[0].frozen = true;
    g.apply(0, Action::Freeze).unwrap();
    assert!(g.players[0].shop.iter().all(|s| s.frozen));
    g.apply(0, Action::Freeze).unwrap();
    assert!(g.players[0].shop.iter().all(|s| !s.frozen));
    // The refill is capped at 99; extra Gold at the start of the turn is not.
    let mut g = game(811);
    g.players[0].max_gold_bonus = 500;
    g.players[0].gold_next_turn = 300;
    end_turn_all(&mut g);
    assert_eq!(g.players[0].gold, 99 + 300);
}

fn me(c: &taverngym::engine::Ctx, side: usize, eid: u32) -> taverngym::script::Me {
    let m = c.minion(eid).unwrap();
    taverngym::script::Me {
        side,
        eid,
        card: m.card,
        golden: m.golden,
        opt: 0,
        pos: 0,
        data: 0,
    }
}

#[test]
fn after_attack_triggers_wait_for_the_attack() {
    // Ravaging Scorpid: "after a friendly minion attacks" = AttackDone, not Attacks.
    let mut g = game(700);
    set_board(&mut g, 0, &["BG36_209", "BG36_200"]);
    set_board(&mut g, 1, &["BG36_200"]);
    let mut c = combat_ctx(&mut g);
    let (a, d) = (c.boards[0][1].eid, c.boards[1][0].eid);
    c.emit(taverngym::event::Event::Attacks {
        side: 0,
        attacker: a,
        defender: d,
    });
    assert_eq!(c.player_ref(0).unwrap().stats.beetle_atk, 0);
    c.emit(taverngym::event::Event::AttackDone {
        side: 0,
        attacker: a,
    });
    assert_eq!(c.player_ref(0).unwrap().stats.beetle_atk, 4);
}

#[test]
fn humming_bird_is_a_combat_aura() {
    // Beasts summoned after the Bird died still get +1 Attack; nobody gets it twice.
    let mut g = game(701);
    set_board(&mut g, 0, &["BG26_805", "BG36_200"]);
    let mut c = combat_ctx(&mut g);
    let (bird, bat) = (c.boards[0][0].eid, c.boards[0][1].eid);
    let fx = taverngym::script::script(cid("BG26_805"))
        .unwrap()
        .start_of_combat
        .unwrap();
    let m = me(&c, 0, bird);
    fx(&mut c, m);
    assert_eq!(c.minion(bat).unwrap().atk, 2);
    c.destroy(bird);
    c.resolve_deaths();
    let late = c.new_minion(0, cid("BG36_200"));
    let e = c.summon(0, 1, late).unwrap();
    assert_eq!(c.minion(e).unwrap().atk, 2);
    assert_eq!(c.minion(bat).unwrap().atk, 2);
}

#[test]
fn golden_all_seeing_elder_gains_twice() {
    let mut g = game(702);
    let elder = db().golden_of(cid("BG33_300"));
    let b = vec![
        minion(&mut g, 0, "BG36_200"), // 1/4
        g.with_recruit(0, |c| c.new_minion(0, elder)),
        minion(&mut g, 0, "BG36_200"),
    ];
    g.players[0].board = b;
    let mut c = combat_ctx(&mut g);
    let e = c.boards[0][1].eid;
    let (a0, h0) = (c.boards[0][1].atk, c.boards[0][1].hp);
    let fx = taverngym::script::script(elder)
        .unwrap()
        .start_of_combat
        .unwrap();
    let m = me(&c, 0, e);
    fx(&mut c, m);
    assert_eq!(
        (c.minion(e).unwrap().atk, c.minion(e).unwrap().hp),
        (a0 + 2, h0 + 8)
    );
}

#[test]
fn big_brann_play_stacks_per_copy() {
    let mut g = game(703);
    g.with_recruit(0, |c| {
        c.cast_spell(0, cid("BGS_Treasures_030"), 0, 0, 0);
        c.cast_spell(0, cid("BGS_Treasures_030"), 0, 0, 0);
        assert_eq!(c.battlecry_mult(0), 3);
    });
}

#[test]
fn new_recruit_buffs_only_its_own_minion() {
    let mut g = game(704);
    while g.players[0].shop.len() < g.cfg.shop_max_cards {
        put_shop(&mut g, 0, "BG36_200");
    }
    // A Tavern full of cards that are not the default fill (kept or added by effects): an
    // extra offer has nothing to replace.
    for s in g.players[0].shop.iter_mut() {
        s.filled = false;
    }
    let before: Vec<(i32, i32)> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion().map(|m| (m.atk, m.hp)))
        .collect();
    g.with_recruit(0, |c| {
        let fx = taverngym::script::script(cid("BGS_Treasures_033"))
            .unwrap()
            .on_fill_shop
            .unwrap();
        let m = taverngym::script::Me {
            side: 0,
            eid: 0,
            card: cid("BGS_Treasures_033"),
            golden: false,
            opt: 0,
            pos: 0,
            data: 0,
        };
        fx(c, m);
    });
    let after: Vec<(i32, i32)> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion().map(|m| (m.atk, m.hp)))
        .collect();
    assert_eq!(
        before, after,
        "full Tavern: no extra minion, no stray +2/+2"
    );
}

#[test]
fn hero_rules_follow_the_hero_power() {
    // Clocksworth's "triples need 2" comes from owning Double Time, not from the hero.
    let mut g = game(705);
    g.players[0].hero = Some(cid("BG34_HERO_002"));
    g.with_recruit(0, |c| assert_eq!(c.triple_need(0, cid("BG36_200")), 3));
    g.set_hero_power(0, 0, cid("BG34_HERO_002p"));
    g.players[0].hero = Some(cid("TB_BaconShop_HERO_34"));
    g.with_recruit(0, |c| assert_eq!(c.triple_need(0, cid("BG36_200")), 2));
}

#[test]
fn clone_exhibit_needs_board_space() {
    let mut g = game(706);
    g.players[0].gold = 10;
    g.players[0].tier = 6;
    g.turn = 10;
    g.set_hero_power(0, 0, cid("BG31_HERO_005p"));
    set_board(&mut g, 0, &["BG36_200"; 7]);
    assert!(!g.can_hero_power(0, 0));
    g.players[0].board.pop();
    assert!(g.can_hero_power(0, 0));
}

#[test]
fn silas_tickets_do_not_touch_minion_counters() {
    let mut g = game(707);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_101"));
    g.cfg.silas_ticket_pct = 100;
    g.players[0].gold = 10;
    g.apply(0, Action::Refresh).unwrap();
    assert!(
        g.players[0]
            .shop
            .iter()
            .filter_map(|s| s.item.as_minion())
            .all(|m| m.counter != -7_777)
    );
    let hp = &g.players[0].powers[0];
    assert_eq!(hp.values.len(), g.players[0].shop.len());
    let sid = hp.values[0] as u32;
    assert!(g.players[0].shop.iter().any(|s| s.sid == sid));
}

#[test]
fn kidnap_sack_takes_spells_but_not_goldens() {
    let mut g = game(708);
    g.players[0].shop.clear();
    let gold = g.with_recruit(0, |c| c.new_minion(0, db().golden_of(cid("BG36_200"))));
    let gsid = gold.eid;
    g.players[0]
        .shop
        .push(taverngym::state::ShopSlot::new(gsid, Item::Minion(gold)));
    put_shop(&mut g, 0, "BG36_200");
    let ssid = 999_999;
    g.players[0].shop.push(taverngym::state::ShopSlot::new(
        ssid,
        Item::spell(cid("BG28_503")),
    ));
    g.with_recruit(0, |c| {
        let t = c.legal_targets(0, taverngym::script::TargetSpec::TavernCard, 0);
        assert_eq!(t.len(), 2);
        assert!(!t.contains(&gsid) && t.contains(&ssid));
        c.cast_spell(0, cid("BG24_Reward_718t"), 0, ssid, 0);
    });
    assert!(hand_ids(&g, 0).contains(&"BG28_503".to_string()));
}

#[test]
fn buddy_guards_and_golden_uses() {
    // Death's Head Sage normal + golden: one Blood Gem gives 1 + 2 copies, no chain.
    let mut g = game(709);
    let sage = "BG20_HERO_103_Buddy";
    let b = vec![
        minion(&mut g, 0, sage),
        g.with_recruit(0, |c| c.new_minion(0, db().golden_of(cid(sage)))),
    ];
    g.players[0].board = b;
    give(&mut g, 0, "BG20_GEM");
    assert_eq!(hand_ids(&g, 0).len(), 4);
    // Golden Relic Squire: once per turn, 2 copies.
    let mut g = game(710);
    let sq = "BG28_HERO_800_Buddy";
    let b = vec![g.with_recruit(0, |c| c.new_minion(0, db().golden_of(cid(sq))))];
    g.players[0].board = b;
    g.with_recruit(0, |c| {
        c.cast_from_hand = true; // casts you make
        c.cast_spell(0, cid("BG28_503"), 0, 0, 0);
        c.cast_spell(0, cid("BG28_503"), 0, 0, 0);
    });
    assert_eq!(hand_ids(&g, 0).len(), 2);
}

#[test]
fn armor_stash_sets_armor() {
    let mut g = game(711);
    g.players[0].armor = 12;
    g.with_recruit(0, |c| c.cast_spell(0, cid("BG28_500"), 0, 0, 0));
    assert_eq!(g.players[0].armor, 5);
}

#[test]
fn golden_street_magician_second_pick_drops_the_first() {
    let mut g = game(712);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_020"));
    let b = vec![g.with_recruit(0, |c| {
        c.new_minion(0, db().golden_of(cid("TB_BaconShop_HERO_21_Buddy")))
    })];
    g.players[0].board = b;
    g.players[0].gold = 10;
    g.apply(0, Action::HeroPower(0)).unwrap();
    let first = match g.players[0].pending.last() {
        Some(Pending::Discover { options, .. }) => options[0].card(),
        p => panic!("{p:?}"),
    };
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].secrets.contains(&first));
    for q in &g.players[0].pending {
        if let Pending::Discover { options, .. } = q {
            assert!(options.iter().all(|o| o.card() != first));
        }
    }
}

#[test]
fn spell_copies_limit_the_tavern() {
    // Tavern spells have copies; those in Taverns and hands are out of the pool.
    let mut g = game(720);
    let s = cid("BG28_503"); // Tier 1: 5 copies
    let used: u64 = g
        .players
        .iter()
        .map(|p| {
            (p.shop.iter().filter(|x| x.item.card() == s).count()
                + p.hand.iter().filter(|x| x.item.card() == s).count()) as u64
        })
        .sum();
    assert_eq!(g.spell_copies_left(s), 5 - used);
    for _ in 0..5 {
        g.with_recruit(0, |c| {
            c.add_to_hand(0, Item::spell(s));
        });
    }
    assert_eq!(g.spell_copies_left(s), 0);
    // None left: a Tavern slot never shows it.
    for _ in 0..20 {
        let t = g.with_recruit(0, |c| c.draw_pool_spell(|d| d.tier <= 1));
        assert_ne!(t, Some(s));
    }
}

#[test]
fn turn_start_refill_counts_all_cards() {
    // 4 frozen spells on Tier 1 fill the Tavern (4 cards): no new cards come in.
    let mut g = game(721);
    g.players[0].shop.clear();
    for i in 0..4 {
        let mut slot = taverngym::state::ShopSlot::new(900_000 + i, Item::spell(cid("BG28_500")));
        slot.frozen = true;
        g.players[0].shop.push(slot);
    }
    g.with_recruit(0, |c| c.fill_shop(0));
    assert_eq!(g.players[0].shop.len(), 4);
    g.players[0].shop.truncate(1);
    g.with_recruit(0, |c| c.fill_shop(0));
    let minions = g.players[0]
        .shop
        .iter()
        .filter(|s| s.item.as_minion().is_some())
        .count();
    assert_eq!((g.players[0].shop.len(), minions), (4, 3));
}

#[test]
fn trinket_offers_follow_the_2025_rules() {
    // 4 distinct options, one priced <= 2, pivots (a type you're not in) 2 cheaper and no two of
    // the same type, at most one of Souvenir Stand / Trip Vouchers / Ornate Clock.
    for seed in 0..40 {
        let mut g = game(730 + seed);
        g.turn = 6;
        let offer = g.trinket_offer(0, taverngym::cards::CardKind::TrinketLesser);
        assert_eq!(offer.len(), 4, "seed {seed}");
        let mut ids = offer.clone();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 4);
        assert!(
            offer.iter().any(|t| g.trinket_price(0, *t) <= 2),
            "seed {seed}"
        );
        let excl = [
            "BG30_MagicItem_888",
            "BG30_MagicItem_891",
            "BG32_MagicItem_271",
        ];
        assert!(
            offer
                .iter()
                .filter(|t| excl.contains(&db().get(**t).id.as_str()))
                .count()
                <= 1
        );
        // Empty board: every typed trinket is a pivot, and no two of them share a type.
        let typed: Vec<Races> = offer
            .iter()
            .map(|t| taverngym::content::trinket_offers::types(*t))
            .filter(|r| !r.is_none())
            .collect();
        for (i, a) in typed.iter().enumerate() {
            assert!(
                typed[i + 1..].iter().all(|b| !a.intersects(*b)),
                "seed {seed}"
            );
        }
        assert_eq!(
            g.players[0].trinket_pivots.len(),
            offer
                .iter()
                .filter(|t| {
                    let id = db().get(**t).id.as_str();
                    !taverngym::content::trinket_offers::types(**t).is_none()
                        || taverngym::content::trinket_offers::MENAGERIE.contains(&id)
                })
                .count()
        );
        for pv in g.players[0].trinket_pivots.clone() {
            assert_eq!(g.trinket_price(0, pv), (db().get(pv).cost - 2).max(0));
        }
    }
}

#[test]
fn matchmaking_avoids_recent_opponents() {
    // Nobody meets an opponent (or the ghost) from their last 2 combats.
    let mut g = game(740);
    g.players[7].alive = false;
    g.eliminated = vec![7];
    for p in 0..7 {
        g.players[p].recent_opponents = vec![Some((p + 1) % 7), Some((p + 6) % 7)];
    }
    g.players[0].recent_opponents = vec![None, Some(1)];
    for _ in 0..20 {
        g.decide_pairs();
        for &(a, b) in &g.pairs {
            assert!(!g.players[a].recent_opponents.contains(&b), "{a} vs {b:?}");
            if let Some(b) = b {
                assert!(!g.players[b].recent_opponents.contains(&Some(a)));
            }
        }
    }
}

#[test]
fn effect_casts_are_not_your_casts() {
    // A spell a card casts itself ("this casts") resolves but doesn't count or trigger "after you
    // cast".
    let mut g = game(741);
    g.players[0].armor = 9;
    g.with_recruit(0, |c| c.cast_spell(0, cid("BG28_500"), 0, 0, 0));
    assert_eq!(g.players[0].armor, 5, "the spell still resolves");
    assert_eq!(g.players[0].stats.spells_cast, 0);
    g.with_recruit(0, |c| {
        c.cast_from_hand = true;
        c.cast_spell(0, cid("BG28_500"), 0, 0, 0);
    });
    assert_eq!(g.players[0].stats.spells_cast, 1);
}

#[test]
fn cast_without_this_is_your_cast() {
    // "cast X" on a card (no "this casts") is a spell you cast. Veteran Brigand: "Choose One - This
    // plays 3 Blood Gems on all your minions; or cast Blood Gem Barrage 3 times." Felboar: "After
    // you cast 3 spells, consume a minion in the Tavern to gain its stats."
    for (opt, casts) in [(0, 0), (1, 3)] {
        let mut g = game(8401);
        g.players[0].tier = 6;
        set_board(&mut g, 0, &["BG28_633"]);
        let before = stats(&g, 0)[0];
        let shop = g.players[0].shop.len();
        give(&mut g, 0, "BG36_341");
        g.apply(0, Action::Play { hand: 0, pos: 1 }).unwrap();
        g.apply(0, Action::Choose(opt)).unwrap();
        let st = &g.players[0].stats;
        assert_eq!((st.spells_cast, st.tavern_spells_cast), (casts, casts));
        // The Felboar ate one Tavern minion after the third Barrage only.
        let ate = shop - g.players[0].shop.len();
        assert_eq!(ate, if casts == 3 { 1 } else { 0 }, "option {opt}");
        if casts == 0 {
            // "This plays" Blood Gems: +3/+3 on the Felboar, nothing counted.
            assert_eq!(stats(&g, 0)[0], (before.0 + 3, before.1 + 3));
        }
    }
    // Balinda ("Your spells that target friendly minions cast twice") doubles a spell a
    // card has you cast on a minion (Emergency Gearblade: "cast Repair Job on your left-most
    // Mech", +4/+8), and the cast counts.
    let mut g = game(8402);
    g.players[0].trinkets.push(cid("BG36_MagicItem_812"));
    set_board(&mut g, 0, &["BG26_146", "BG35_883"]);
    let before = stats(&g, 0)[0];
    end_all(&mut g);
    // (Lullabot also gains +1 Health at the end of the turn.)
    assert_eq!(stats(&g, 0)[0], (before.0 + 8, before.1 + 16 + 1));
    assert_eq!(g.players[0].stats.tavern_spells_cast, 2);
}

#[test]
fn only_your_refreshes_count() {
    // An effect's Refresh fires "after Refresh" but doesn't count as you Refreshing.
    let mut g = game(742);
    g.with_recruit(0, |c| c.refresh_shop(0));
    assert_eq!(g.players[0].stats.refreshes, 0);
    g.players[0].gold = 5;
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(g.players[0].stats.refreshes, 1);
}

#[test]
fn lobby_without_forced_aberrations() {
    // With the guarantee off, lobbies draw 5 of all types; Dark Paradox is in every lobby, one form
    // each.
    let cfg = RulesConfig {
        aberration_forced: false,
        ..Default::default()
    };
    let mut without = 0;
    for seed in 0..30 {
        let g = Game::new(seed, cfg.clone());
        assert_eq!(g.lobby_races.iter().count(), 5);
        if !g
            .lobby_races
            .intersects(taverngym::types::Races::ABERRATION)
        {
            without += 1;
            assert_eq!(
                g.universe
                    .iter()
                    .filter(|c| db().get(**c).id.starts_with("BG36_360t"))
                    .count(),
                1
            );
        }
    }
    assert!(without > 0);
}

#[test]
fn trinket_offer_conditions() {
    // Hero exclusions and per-trinket conditions.
    let mut g = game(750);
    let wallet = cid("BG30_MagicItem_847");
    assert!(g.trinket_eligible(0, wallet));
    g.players[0].hero = Some(cid("TB_BaconShop_HERO_10")); // Gallywix
    assert!(!g.trinket_eligible(0, wallet));
    // Shrine of Evolution needs 6+ minions.
    let shrine = cid("BG32_MagicItem_400");
    set_board(&mut g, 0, &["BG36_200"; 5]);
    assert!(!g.trinket_eligible(0, shrine));
    set_board(&mut g, 0, &["BG36_200"; 6]);
    assert!(g.trinket_eligible(0, shrine));
    // Health rewinder trinkets need a rewinder (Soul Rewinder).
    let tapestry = cid("BG35_MagicItem_152");
    assert!(!g.trinket_eligible(0, tapestry));
    set_board(&mut g, 0, &["BG26_174"]);
    assert!(g.trinket_eligible(0, tapestry));
    // Ur'zul Sticker: only after buffing the Tavern.
    let urzul = cid("BG35_MagicItem_154");
    assert!(!g.trinket_eligible(0, urzul));
    let sid = g.players[0]
        .shop
        .iter()
        .find(|s| s.item.as_minion().is_some())
        .unwrap()
        .sid;
    g.with_recruit(0, |c| c.buff(sid, 1, 1));
    assert!(g.trinket_eligible(0, urzul));
}

#[test]
fn trinket_offers_guarantee_type_and_no_type() {
    // Every offer has a no-type trinket and one of the most common type you're in; at most one
    // trinket per other type.
    let mut checked = 0;
    for seed in 0..60 {
        let mut g = game(760 + seed);
        if !g.lobby_races.intersects(taverngym::types::Races::BEAST) {
            continue;
        }
        set_board(&mut g, 0, &["BG36_200", "BG36_200", "BG36_200"]); // 3 Beasts
        for kind in [
            taverngym::cards::CardKind::TrinketLesser,
            taverngym::cards::CardKind::TrinketGreater,
        ] {
            let offer = g.trinket_offer(0, kind);
            let assoc = |t: &CardId| db().get(*t).assoc;
            assert!(
                offer
                    .iter()
                    .any(|t| assoc(t).is_none() && db().get(*t).id != "BG36_MagicItem_611"),
                "seed {seed}: no no-type trinket"
            );
            let beast = taverngym::types::Races::BEAST;
            let has_beast_cand = g.trinket_universe.iter().any(|t| {
                db().get(*t).kind == kind && assoc(t).intersects(beast) && g.trinket_eligible(0, *t)
            });
            if has_beast_cand {
                assert!(
                    offer.iter().any(|t| assoc(t).intersects(beast)),
                    "seed {seed}"
                );
            }
            let others: Vec<_> = offer
                .iter()
                .filter(|t| !assoc(t).is_none() && !assoc(t).intersects(beast))
                .collect();
            for (i, a) in others.iter().enumerate() {
                for b in &others[i + 1..] {
                    assert!(
                        !assoc(a).intersects(assoc(b)),
                        "seed {seed}: two of one type"
                    );
                }
            }
            checked += 1;
        }
    }
    assert!(checked > 0);
}

#[test]
fn dark_gift_rules() {
    use taverngym::recruit::gift_compatible;
    let d = db();
    // Data-driven exclusions; "in your hand" text alone does not exclude Choral Mrrrglr.
    assert!(gift_compatible(d.get(cid("BG26_354")), 6));
    assert!(!gift_compatible(d.get(cid("BG36_369")), 6)); // Greedy Conniver (tag 4785)
    assert!(!gift_compatible(d.get(cid("BG36_206")), 6)); // Snarky Shark (tag 4955)
    // Tras'tath's 20 gifts, no early variants.
    assert_eq!(taverngym::content::TRASTATH_GIFT_IDS.len(), 20);
    assert!(!taverngym::content::TRASTATH_GIFT_IDS.contains(&"BG36_MidGameEffect_000t28t"));
    let mut g = game(770);
    g.with_recruit(0, |c| {
        let fits = |c: &taverngym::engine::Ctx, gift: &str, minion: &str| {
            taverngym::content::gift_fits(c, 0, cid(gift), d.get(cid(minion)), 1)
        };
        // Consanguinity: only on Quilboar.
        assert!(!fits(c, "BG36_MidGameEffect_000t80", "BG36_200"));
        // Battle Scars / Death's Embrace: only the most-triggered one.
        if let Some(p) = c.player(0) {
            p.stats.battlecries_triggered = 3;
            p.stats.deathrattles_triggered = 1;
        }
        assert!(fits(c, "BG36_MidGameEffect_000t28", "BG36_200"));
        assert!(!fits(c, "BG36_MidGameEffect_000t29", "BG36_200"));
    });
}

#[test]
fn xavius_discovers_by_turn_table() {
    // Xavius uses the Dark Gift turn -> tier table.
    let mut g = game(771);
    g.turn = 8;
    g.players[0].tier = 6;
    let hp = cid("BG36_HERO_105p");
    let fx = taverngym::script::script(hp)
        .unwrap()
        .start_of_turn
        .unwrap();
    g.with_recruit(0, |c| {
        for _ in 0..4 {
            let me = taverngym::script::Me {
                side: 0,
                eid: 0,
                card: hp,
                golden: false,
                opt: 0,
                pos: 0,
                data: 0,
            };
            fx(c, me);
        }
    });
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!("no Discover");
    };
    for o in options {
        assert!(
            [4, 5].contains(&db().get(o.card()).tier),
            "{:?}",
            db().get(o.card()).tier
        );
    }
}

#[test]
fn sanders_is_targeted_and_buddy_bans_apply() {
    // BG25_034: the chosen minion becomes Golden.
    let mut g = game(772);
    set_board(&mut g, 0, &["BG36_200", "BG26_174"]);
    let target = g.players[0].board[1].eid;
    let spec = taverngym::script::script(cid("BG25_034"))
        .unwrap()
        .battlecry_target;
    assert_eq!(
        spec,
        taverngym::script::TargetSpec::FriendlyNonGoldenMaxTier(6)
    );
    let fx = taverngym::script::script(cid("BG25_034"))
        .unwrap()
        .battlecry
        .unwrap();
    g.with_recruit(0, |c| {
        let me = taverngym::script::Me {
            side: 0,
            eid: 0,
            card: cid("BG25_034"),
            golden: false,
            opt: 0,
            pos: 0,
            data: 0,
        };
        fx(c, me, target);
    });
    assert!(g.players[0].board[1].golden && !g.players[0].board[0].golden);
    // BG28_Reward_513: Vaelastrasz (temporarily banned) is never a buddy option.
    for seed in 0..30 {
        let mut g = game(780 + seed);
        g.with_recruit(0, |c| {
            taverngym::content::discover_buddy(c, 0, cid("BG28_Reward_513"))
        });
        if let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() {
            assert!(
                options
                    .iter()
                    .all(|o| db().get(o.card()).id != "TB_BaconShop_HERO_56_Buddy")
            );
        }
    }
}

// ------------------------------------------------- A-class fixes

#[test]
fn rallies_that_need_an_attack_cant_be_triggered() {
    // Sky-hatch Runaway can't pick Jailbird Juggernaut (BG36_333); Flittering Bat can.
    let mut g = game(790);
    set_board(&mut g, 0, &["BG36_243", "BG36_333", "BG36_200"]);
    let (jail, bat) = (g.players[0].board[1].eid, g.players[0].board[2].eid);
    g.with_recruit(0, |c| {
        let t = c.legal_targets(
            0,
            taverngym::script::TargetSpec::FriendlyRallyTriggerable,
            0,
        );
        assert!(!t.contains(&jail) && t.contains(&bat));
    });
}

#[test]
fn protected_cards_wait_for_hand_space() {
    // A Trinket's card with a full hand waits and arrives when there is room.
    let mut g = game(791);
    g.cfg.triple_copies = 99;
    for _ in 0..10 {
        give(&mut g, 0, "BG36_200");
    }
    g.with_recruit(0, |c| {
        c.src = Some(cid("BG30_MagicItem_888"));
        c.give_card(0, cid("BG28_503"));
    });
    assert_eq!(g.players[0].hand.len(), 10);
    assert_eq!(g.players[0].hand_queue.len(), 1);
    g.players[0].hand.pop();
    g.flush_hand_queue(0);
    assert!(g.players[0].hand_queue.is_empty());
    assert!(hand_ids(&g, 0).contains(&"BG28_503".to_string()));
}

#[test]
fn raynor_once_per_game_upgrades() {
    // After buying Advanced Construction it is never offered again.
    let mut g = game(792);
    g.players[0].tier = 6;
    g.set_hero_power(0, 0, cid("BG31_HERO_801p"));
    g.players[0].powers[0].memory.push(cid("BG31_HERO_801ptf"));
    for _ in 0..200 {
        let u = g.with_recruit(0, |c| taverngym::content::starcraft::random_upgrade(c, 0));
        assert!(!db().get(u.unwrap()).id.starts_with("BG31_HERO_801ptf"));
    }
}

#[test]
fn quest_pool_and_endless_rewards() {
    // Only the current pool is offerable; Perpetual Incantation can be earned again.
    assert!(taverngym::content::quest_offerable(
        taverngym::types::Races::NONE,
        "BG24_Quest_123"
    ));
    assert!(!taverngym::content::quest_offerable(
        taverngym::types::Races::NONE,
        "BG24_Quest_120"
    ));
    assert!(!taverngym::content::quest_offerable(
        taverngym::types::Races::NONE,
        "BG24_Reward_311"
    ));
    assert!(taverngym::content::ENDLESS_REWARDS.contains(&"BG33_Reward_020"));
}

#[test]
fn upgrades_are_not_tavern_spells() {
    let mut g = game(793);
    set_board(&mut g, 0, &["BG31_HERO_801pt"]);
    g.with_recruit(0, |c| {
        c.cast_from_hand = true;
        c.cast_spell(0, cid("BG31_HERO_801pta"), 0, 0, 0);
    });
    assert_eq!(g.players[0].stats.spells_cast, 1);
    assert_eq!(g.players[0].stats.tavern_spells_cast, 0);
}

#[test]
fn goldrinn_and_holy_vanguard_in_combat() {
    // BGS_018: Beasts summoned after Goldrinn's Deathrattle still get +7/+7.
    let mut g = game(794);
    set_board(&mut g, 0, &["BGS_018"]);
    let mut c = combat_ctx(&mut g);
    let gold = c.boards[0][0].eid;
    c.destroy(gold);
    c.resolve_deaths();
    let late = c.new_minion(0, cid("BG36_200"));
    let e = c.summon(0, 0, late).unwrap();
    assert_eq!((c.minion(e).unwrap().atk, c.minion(e).unwrap().hp), (8, 11));
    drop(c);
    // A Holy Vanguard summoned mid-combat at <= 15 Health gets +30/+30 once.
    let mut g = game(795);
    g.players[0].health = 10;
    let mut c = combat_ctx(&mut g);
    let v = c.new_minion(0, cid("BG36_372"));
    let (a, h) = (v.atk, v.hp);
    assert_eq!((a, h), (40, 40));
    let e = c.summon(0, 0, v).unwrap();
    assert_eq!((c.minion(e).unwrap().atk, c.minion(e).unwrap().hp), (a, h));
}

#[test]
fn snow_elemental_offers_frozen_elementals() {
    let seed = (796..900)
        .find(|s| {
            game(*s)
                .lobby_races
                .intersects(taverngym::types::Races::ELEMENTAL)
        })
        .expect("a lobby with Elementals");
    let mut g = game(seed);
    set_board(&mut g, 0, &["TB_BaconShop_HERO_78_Buddy"]);
    g.players[0].gold = 5;
    g.apply(0, Action::Refresh).unwrap();
    assert!(g.players[0].shop.iter().any(|s| s.frozen));
}

#[test]
fn putricide_builds_a_creation_from_two_pools() {
    // Two picks; the Creation has both cards attached and their stats added.
    let mut g = game(797);
    g.players[0].tier = 4;
    g.players[0].gold = 10;
    g.set_hero_power(0, 0, cid("BG25_HERO_100p"));
    g.apply(0, Action::HeroPower(0)).unwrap();
    let pick = |g: &Game| match g.players[0].pending.last() {
        Some(Pending::Discover { options, .. }) => options[0].card(),
        p => panic!("{p:?}"),
    };
    let base = pick(&g);
    g.apply(0, Action::Choose(0)).unwrap();
    let part = pick(&g);
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].pending.is_empty());
    let m = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .find(|m| db().get(m.card).id == "BG25_HERO_100pt")
        .expect("a Creation");
    assert_eq!(m.attached, vec![base, part]);
    let (b, p) = (db().get(base), db().get(part));
    assert_eq!((m.atk, m.max_hp), (b.atk + p.atk, b.hp + p.hp));
    assert_eq!(
        g.players[0].hand.len(),
        1,
        "the picks themselves are not kept"
    );
}

#[test]
fn combat_with_a_dead_minion_on_a_board_terminates() {
    // A board minion at 0 Health (an injected snapshot, a debug edit) dies at Start of
    // Combat; whatever is left must never make the attack loop spin (it stalls out).
    let mut g = game(9001);
    set_board(&mut g, 0, &["BG25_001"]);
    set_board(&mut g, 1, &["BG25_001"]);
    g.players[0].board[0].hp = 0;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out = fight(&mut g);
        tx.send(out).unwrap();
    });
    let out = rx
        .recv_timeout(std::time::Duration::from_secs(20))
        .expect("the combat did not terminate");
    assert!(!out.truncated);
    assert_eq!(out.winner, 1, "the living side wins");
}

#[test]
fn fishbait_cannot_gain_temporary_stats_either() {
    let mut g = game(9002);
    set_board(&mut g, 0, &["BG36_205"]);
    let (a, h) = (g.players[0].board[0].atk, g.players[0].board[0].hp);
    g.with_recruit(0, |c| {
        let e = c.boards[0][0].eid;
        c.buff_temp(e, 3, 3);
        c.buff(e, 3, 3);
    });
    let m = &g.players[0].board[0];
    assert_eq!((m.atk, m.hp, m.temp_atk), (a, h, 0));
}

#[test]
fn demon_card_rules() {
    // Sacrificial Wrathguard: improvements survive its death (the record, not the board).
    let mut g = game(9101);
    set_board(&mut g, 0, &["BG36_362"]);
    g.players[0].board[0].counter = 2;
    g.with_recruit(0, |c| {
        let e = c.boards[0][0].eid;
        c.destroy(e);
        c.resolve_deaths();
    });
    // +2/+2 improved twice = +6/+6 on every Tavern minion (base 1-tier stats + 6).
    let s = &g.players[0].stats;
    assert_eq!((s.tavern_atk, s.tavern_hp), (6, 6));

    // Mind Muck with no other Demon consumes nothing.
    let mut g = game(9102);
    g.players[0].gold = 10;
    g.players[0].hand.clear();
    set_board(&mut g, 0, &[]);
    let shop_before = g.players[0].shop.len();
    let m = minion(&mut g, 0, "BG23_357");
    g.players[0].hand.push(taverngym::state::HandCard {
        hid: m.eid,
        item: Item::Minion(m),
        locked: 0,
        link: 0,
        group: 0,
        group_left: 0,
        temporary: false,
        src: 0,
        combined: false,
        held: false,
        held_gold: 0,
    });
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert!(g.players[0].pending.is_empty());
    assert_eq!(
        g.players[0].shop.len(),
        shop_before,
        "no tavern minion eaten"
    );
}

#[test]
fn every_masked_legal_action_applies() {
    // Mask/apply consistency: random play over many seeds; an action the mask marks legal
    // must never be rejected.
    use taverngym::rng::Rng;
    for seed in 0..12u64 {
        let mut g = Game::new(seed, RulesConfig::default());
        let mut rng = Rng::new(seed ^ 0xABCD);
        for _ in 0..6000 {
            if g.is_over() {
                break;
            }
            let seats: Vec<usize> = (0..8).filter(|p| g.needs_action(*p)).collect();
            if seats.is_empty() {
                break;
            }
            let p = seats[rng.below(seats.len())];
            let legal = g.legal_actions(p);
            // Mostly end the turn when that is possible, so games progress.
            let a = if legal.contains(&Action::EndTurn.encode()) && rng.chance(1, 12) {
                Action::EndTurn.encode()
            } else {
                legal[rng.below(legal.len())]
            };
            if let Err(e) = g.apply_index(p, a) {
                panic!(
                    "seed {seed} p{p}: masked-legal action {a} ({:?}) rejected: {e}",
                    Action::decode(a)
                );
            }
        }
    }
}

#[test]
fn action_cap_leaves_only_end_turn() {
    let mut g = game(9201);
    g.cfg.turn_action_caps = vec![5];
    for _ in 0..5 {
        g.apply(0, Action::Freeze).unwrap();
    }
    let legal = g.legal_actions(0);
    assert_eq!(legal, vec![Action::EndTurn.encode()]);
    assert!(g.apply(0, Action::Freeze).is_err());
    assert!(g.apply(0, Action::EndTurn).is_ok());
}

#[test]
fn observation_hides_this_turns_opponent_boards() {
    let mut g = game(9202);
    set_board(&mut g, 1, &["BG25_001", "BG25_001"]);
    g.apply(1, Action::EndTurn).unwrap(); // p1's finished board is now `last_board`
    assert_eq!(g.players[1].last_board.len(), 2);
    let o = taverngym::obs::observe(&g, 0);
    let mf = taverngym::obs::MINION_F;
    // Opponent boards block: the last OBS_OPP_BOARD_F floats; p1 is the first opponent of p0.
    let start = o.floats.len() - taverngym::obs::OPP_BOARD_F;
    assert!(
        o.floats[start..start + 7 * mf].iter().all(|x| *x == 0.0),
        "unseen board leaked"
    );
}

#[test]
fn observation_shows_only_the_warbands_the_player_fought() {
    use taverngym::obs::{MINION_F, OPP_BOARD_F, observe};
    let mut g = game(9203);
    // Per opponent slot of p0's observation: how many minions it holds.
    let seen = |g: &Game| -> Vec<usize> {
        let o = observe(g, 0);
        let start = o.floats.len() - OPP_BOARD_F;
        (0..7)
            .map(|i| {
                (0..7)
                    .filter(|j| o.floats[start + (i * 7 + j) * MINION_F] > 0.5)
                    .count()
            })
            .collect()
    };
    let end_turns = |g: &mut Game| {
        for p in 0..8 {
            if g.needs_action(p) {
                g.apply(p, Action::EndTurn).unwrap();
            }
        }
    };
    // Turn 1: seat p fields p % 2 + 1 Riders (never three: they would combine).
    for p in 0..8 {
        set_board(&mut g, p, &vec!["BG25_001"; p % 2 + 1]);
    }
    let first = g.players[0].next_opponent.expect("eight players: no ghost");
    end_turns(&mut g);
    assert_eq!(g.turn, 2);
    // p0 fought `first` and has seen its warband; the other six are unknown to it, although
    // each of them showed a warband to somebody this round.
    assert!(g.players.iter().all(|p| !p.seen_board.is_empty()));
    let slot = |o: usize| o - 1; // p0's opponents are listed in seat order
    let now = seen(&g);
    for o in 1..8 {
        let expect = if o == first { o % 2 + 1 } else { 0 };
        assert_eq!(now[slot(o)], expect, "opponent {o}, fought {first}");
        assert_eq!(
            g.known_board(0, o).1,
            (o == first) as u32,
            "turn it was seen"
        );
    }
    // Turn 2: `first` grows its warband. p0 keeps what it saw unless the two meet again.
    let grown = ["BG25_001", "BG25_001", "BGS_034", "BGS_034", "BGS_131"];
    set_board(&mut g, first, &grown);
    end_turns(&mut g);
    let later = seen(&g);
    let second = g.players[0].last_opponent.unwrap();
    assert_eq!(
        later[slot(first)],
        if second == first { 5 } else { first % 2 + 1 }
    );
    assert_eq!(
        later.iter().filter(|n| **n > 0).count(),
        1 + (second != first) as usize
    );
    // The diagnostic switch shows every warband of the last round.
    g.players[0].sees_all_boards = true;
    assert!(seen(&g).iter().all(|n| *n > 0));
}

#[test]
fn gifted_discover_completing_a_triple_keeps_its_gift() {
    // The pick triples on arrival: the gift's on_attach must run before the merge, and the
    // golden minion must carry the gift.
    let mut g = game(9301);
    let gift = g.gift_universe[0];
    set_board(&mut g, 0, &["BG25_001", "BG25_001"]);
    let mut pick = minion(&mut g, 0, "BG25_001");
    pick.attached.push(gift);
    g.players[0].pending.push(Pending::Discover {
        source: gift,
        options: vec![Item::Minion(pick)],
    });
    g.apply(0, Action::Choose(0)).unwrap();
    let golden: Vec<_> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .filter(|m| m.golden)
        .collect();
    assert_eq!(golden.len(), 1, "the three Riders merged");
    assert!(
        golden[0].attached.contains(&gift),
        "the gift survived the merge"
    );
}

#[test]
fn partial_config_keeps_guaranteed_heroes() {
    // A field-level serde default must not wipe the struct default.
    let cfg: RulesConfig = serde_json::from_str(r#"{"record_log": true}"#).unwrap();
    assert_eq!(
        cfg.guaranteed_heroes,
        RulesConfig::default().guaranteed_heroes
    );
    assert_eq!(cfg.guaranteed_heroes.len(), 2);
}

#[test]
fn golden_merge_keeps_a_multi_hit_shield_full() {
    // Parts carrying a 3-hit Divine Shield (Toreth's Blessing) merge into a golden with a
    // full 3-hit shield.
    let mut g = game(9401);
    let mut parts = vec![];
    for _ in 0..3 {
        let mut m = minion(&mut g, 0, "BG25_001");
        m.shield_max = 3;
        m.give(Kw::DIVINE_SHIELD);
        parts.push(m);
    }
    g.players[0].board = parts;
    g.with_recruit(0, |c| c.check_triples(0));
    let golden = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .find(|m| m.golden)
        .expect("merged");
    assert_eq!((golden.shield_max, golden.shield_hits), (3, 3));
}

#[test]
fn stealth_only_boards_are_not_attacked() {
    // Only Stealth minions left: the attacker doesn't attack. A Stealth minion with no
    // Attack never attacks either, so nobody can act and the combat is a tie.
    let mut g = game(800);
    set_board(&mut g, 0, &["BG36_200"]);
    set_board(&mut g, 1, &["BG36_200"]);
    g.players[1].board[0].kw.set(Kw::STEALTH);
    g.players[1].board[0].atk = 0;
    let out = fight(&mut g);
    assert_eq!(out.winner, 2);
    assert_eq!(out.damage, 0);
    assert!(!out.truncated, "a tie by rule, not a cutoff");
    // With Attack, the Stealth minion attacks on its turn (and loses Stealth).
    let mut g = game(801);
    set_board(&mut g, 0, &["BG36_200"]);
    set_board(&mut g, 1, &["BG36_200"]);
    g.players[1].board[0].kw.set(Kw::STEALTH);
    g.players[1].board[0].atk = 50;
    let out = fight(&mut g);
    assert_eq!(out.winner, 1);
}

#[test]
fn friendly_fire_is_not_a_kill() {
    // Killed fires only for enemy victims.
    let mut g = game(802);
    set_board(&mut g, 0, &["BG36_200", "BG36_200"]);
    set_board(&mut g, 1, &["BG36_200"]);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_053")); // Rafaam: copy the first minion you kill
    g.players[0].powers[0].counter = 1;
    g.players[0].hand.clear();
    let mut c = combat_ctx(&mut g);
    let (a, b) = (c.boards[0][0].eid, c.boards[0][1].eid);
    c.damage(a, b, 99); // a friendly minion kills a friendly minion
    c.resolve_deaths();
    drop(c);
    assert!(g.players[0].hand.is_empty(), "no kill credited");
}

#[test]
fn linked_group_of_four_plays_two() {
    // Golden Wandering Willbreaker gives 4 linked cards; after 2 plays the rest go.
    let mut g = game(803);
    g.players[0].tier = 3;
    g.with_recruit(0, |c| {
        let cards = [
            cid("BG28_503"),
            cid("BG28_500"),
            cid("BG28_503"),
            cid("BG28_500"),
        ];
        taverngym::recruit::give_linked_group(c, 0, &cards, 2);
    });
    assert_eq!(g.players[0].hand.len(), 4);
    g.players[0].gold = 10;
    g.apply(0, Action::Play { hand: 1, pos: 0 }).unwrap(); // Armor Stash (no target)
    assert_eq!(g.players[0].hand.len(), 3);
    g.apply(0, Action::Play { hand: 2, pos: 0 }).unwrap(); // the other Armor Stash
    assert!(
        g.players[0].hand.is_empty(),
        "the two unplayed cards are discarded"
    );
}

#[test]
fn lethal_recruit_damage_eliminates_at_once() {
    // A hero at 0 Health in the recruit phase is out immediately.
    let mut g = game(804);
    g.players[0].health = 2;
    g.players[0].armor = 0;
    g.players[0].gold = 10;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_049")); // Lich Baz'hial: take 2 damage
    g.apply(0, Action::HeroPower(0)).unwrap();
    if !g.players[0].pending.is_empty() {
        g.apply(0, Action::Choose(0)).unwrap();
    }
    assert!(!g.players[0].alive);
    assert_eq!(g.players[0].placement, 8);
}

#[test]
fn frostling_is_out_of_the_pool_and_reserve_prices_stack() {
    // Flourishing Frostling is not dealt by default.
    for seed in 0..20 {
        let g = Game::new(seed, RulesConfig::default());
        assert!(!g.universe.contains(&cid("BG26_537")));
    }
    // Two Reserve Prices in a turn = (2) less.
    let mut g = game(805);
    let base = g.spell_price(0, cid("BG28_827"));
    g.with_recruit(0, |c| {
        c.cast_spell(0, cid("BGS_Treasures_104"), 0, 0, 0);
        c.cast_spell(0, cid("BGS_Treasures_104"), 0, 0, 0);
    });
    assert_eq!(g.spell_price(0, cid("BG28_827")), (base - 2).max(0));
}

#[test]
fn pyramad_needs_a_tavern_minion() {
    // R18 (official 36.6 fix).
    let mut g = game(806);
    g.players[0].gold = 10;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_040"));
    assert!(g.can_hero_power(0, 0));
    g.players[0].shop.retain(|s| s.item.as_minion().is_none());
    assert!(!g.can_hero_power(0, 0));
}

#[test]
fn helpful_refresh_sources_and_outcomes() {
    use taverngym::content::helpful;
    // Knockoff Wisdomball: the next 2 Refreshes are helpful.
    let mut g = game(807);
    g.cfg.record_log = true;
    g.players[0].gold = 10;
    g.with_recruit(0, |c| c.cast_spell(0, cid("BG30_802"), 0, 0, 0));
    let helpful_logs = |g: &Game| {
        g.log
            .iter()
            .filter(|l| l.contains("helpful Refresh"))
            .count()
    };
    for _ in 0..3 {
        g.apply(0, Action::Refresh).unwrap();
    }
    assert_eq!(helpful_logs(&g), 2);
    // Outcome 2: every minion is Tier 6; outcome 11: a page of 7 spells; outcome 3: the
    // utility page; outcome 4: Divine Shield and +5/+5.
    let mut g = game(808);
    g.with_recruit(0, |c| helpful::apply(c, 0, 2));
    assert!(
        g.players[0]
            .shop
            .iter()
            .filter_map(|s| s.item.as_minion())
            .all(|m| m.tier == 6)
    );
    g.with_recruit(0, |c| helpful::apply(c, 0, 11));
    assert!(
        g.players[0]
            .shop
            .iter()
            .all(|s| s.item.as_minion().is_none())
    );
    assert!(g.players[0].shop.len() <= 7 && !g.players[0].shop.is_empty());
    g.with_recruit(0, |c| helpful::apply(c, 0, 3));
    let ids: Vec<String> = g.players[0]
        .shop
        .iter()
        .map(|s| db().get(s.item.card()).id.clone())
        .collect();
    assert_eq!(ids.len(), 7);
    assert_eq!(ids[0], "BG36_620");
    assert_eq!(ids[6], "BG35_883");
    let before: Vec<i32> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion().map(|m| m.atk))
        .collect();
    g.with_recruit(0, |c| helpful::apply(c, 0, 4));
    for (s, a) in g.players[0].shop.iter().zip(before) {
        let m = s.item.as_minion().unwrap();
        assert!(m.kw.has(Kw::DIVINE_SHIELD) && m.atk == a + 5);
    }
    // Outcome 8: with one copy held, two more appear.
    let mut g = game(809);
    set_board(&mut g, 0, &["BG36_200"]);
    g.players[0].hand.clear();
    g.with_recruit(0, |c| helpful::apply(c, 0, 8));
    let n = g.players[0]
        .shop
        .iter()
        .filter(|s| s.item.card() == cid("BG36_200"))
        .count();
    assert!(n >= 2);
    // Wondrous Wisdomball: the first Refresh after getting it is helpful.
    let mut g = game(810);
    g.cfg.record_log = true;
    g.players[0].rewards.push(cid("BG24_Reward_313"));
    g.players[0].gold = 10;
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(helpful_logs(&g), 1);
}

#[test]
fn hotfix_36_6_2_tiers_and_texts() {
    // Server-side hotfix: applied on top of the client data by tools/gen_carddb.py.
    for (id, tier) in [
        ("BG36_101", 3),
        ("BG36_101_G", 3),
        ("BG36_100", 2),
        ("BG36_100_G", 2),
    ] {
        assert_eq!(db().get(cid(id)).tier, tier, "{id}");
    }
    assert!(db().get(cid("BG36_110")).text.contains("+2/+1"));
    assert!(db().get(cid("BG36_109")).text.contains("+4/+5"));
    assert!(db().get(cid("BG36_108_G")).text.contains("+2/+4"));
}

#[test]
fn hero_powers_unavailable_with_nothing_to_act_on() {
    use taverngym::content::hero_power_available as avail;
    let mut g = game(301);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    // Tess: needs a last opponent that had minions.
    let tess = cid("TB_BaconShop_HP_077");
    g.set_hero_power(0, 0, tess);
    assert!(!avail(&g, 0, tess));
    g.players[0].last_opponent = Some(1);
    assert!(!avail(&g, 0, tess));
    let m = minion(&mut g, 1, "BG36_345");
    g.players[1].seen_board = vec![m];
    assert!(avail(&g, 0, tess));
    // Zephrys: needs a pair.
    let zeph = cid("TB_BaconShop_HP_102");
    g.set_hero_power(0, 0, zeph);
    set_board(&mut g, 0, &["BG36_345"]);
    assert!(!avail(&g, 0, zeph));
    set_board(&mut g, 0, &["BG36_345", "BG36_345"]);
    assert!(avail(&g, 0, zeph));
    // Vol'jin: two minions among the warband and the Tavern.
    let vol = cid("BG20_HERO_201p");
    g.set_hero_power(0, 0, vol);
    assert!(avail(&g, 0, vol));
    g.players[0].board.clear();
    g.players[0].shop.clear();
    assert!(!avail(&g, 0, vol));
    // Jandice: a non-Golden friendly minion and a Tavern minion.
    let jan = cid("TB_BaconShop_HP_084");
    g.set_hero_power(0, 0, jan);
    set_board(&mut g, 0, &["BG36_345"]);
    assert!(!avail(&g, 0, jan), "empty Tavern");
    // Xyrella / Maiev / Baz'hial: need room in the hand.
    let xy = cid("BG20_HERO_101p");
    g.set_hero_power(0, 0, xy);
    assert!(avail(&g, 0, xy));
    for _ in 0..g.cfg.hand_size {
        give(&mut g, 0, "BG36_345");
    }
    for id in [
        "BG20_HERO_101p",
        "TB_BaconShop_HP_068",
        "TB_BaconShop_HP_049",
    ] {
        g.set_hero_power(0, 0, cid(id));
        assert!(!avail(&g, 0, cid(id)), "{id} with a full hand");
    }
}

/// A lobby with every type but Aberrations (no hero is banned or missing its type).
fn no_aberrations() -> taverngym::types::Races {
    use taverngym::types::Races;
    Races(Races::ALL.0 & !Races::ABERRATION.0)
}

#[test]
fn discover_hero_power_pool_exclusions() {
    let mut g = game(302);
    // Every type but Aberrations: no power is left out because of the lobby.
    g.lobby_races = no_aberrations();
    let options = g.with_recruit(0, |c| c.offer_hero_powers(0, 500, 0, &[]));
    assert!(options.len() > 20);
    for id in [
        "TB_BaconShop_HP_001",
        "BG21_HERO_000p",
        "TB_BaconShop_HP_056",
    ] {
        assert!(!options.contains(&cid(id)), "{id} is excluded (S8)");
    }
    // Tier-locked powers are not offered below their Tier (Alexstrasza: Tier 4).
    let alex = cid("TB_BaconShop_HP_064");
    assert!(!options.contains(&alex));
    g.players[0].pending.clear();
    g.players[0].tier = 4;
    let options = g.with_recruit(0, |c| c.offer_hero_powers(0, 500, 0, &[]));
    assert!(options.contains(&alex));
}

#[test]
fn discover_hero_power_skips_powers_whose_moment_has_passed() {
    let kay = cid("TB_BaconShop_HP_044"); // A. F. Kay: skip your first two turns
    let nzoth = cid("TB_BaconShop_HP_105"); // start the game with a Fish
    let sneed = cid("BG21_HERO_030p"); // start the game with a Shredder
    let marin = cid("BG30_HERO_304p"); // on Turn 5
    let buttons = cid("BG32_HERO_002p"); // on Turn 8
    let george = cid("TB_BaconShop_HP_010");
    let offer = |turn: u32| {
        let mut g = game(303);
        g.lobby_races = no_aberrations();
        g.turn = turn;
        g.with_recruit(0, |c| c.offer_hero_powers(0, 500, 0, &[]))
    };
    // Before the first turn (Sir Finley) everything is still in time.
    let o = offer(0);
    for c in [kay, nzoth, sneed, marin, buttons, george] {
        assert!(o.contains(&c), "{} at the start", db().get(c).id);
    }
    let o = offer(4);
    for c in [kay, nzoth, sneed] {
        assert!(!o.contains(&c), "{} on turn 4", db().get(c).id);
    }
    assert!(o.contains(&marin) && o.contains(&buttons) && o.contains(&george));
    let o = offer(5);
    assert!(!o.contains(&marin) && o.contains(&buttons));
    let o = offer(8);
    assert!(!o.contains(&buttons) && o.contains(&george));
}

#[test]
fn rat_king_start_type_varies_per_game() {
    let mut firsts = std::collections::HashSet::new();
    for seed in 0..40 {
        let g = game(400 + seed);
        let now = g.rat_king_type().unwrap();
        assert!(g.lobby_races.intersects(now));
        let order: Vec<_> = (0..taverngym::types::Races::COUNT)
            .map(|i| taverngym::types::Races(1 << i))
            .filter(|r| g.lobby_races.intersects(*r))
            .collect();
        firsts.insert(order.iter().position(|r| *r == now).unwrap());
    }
    assert!(firsts.len() > 2, "{firsts:?}");
    // Later turns go through the lobby's types in order.
    let mut g = game(400);
    let a = g.rat_king_type().unwrap();
    end_turn_all(&mut g);
    assert_ne!(g.rat_king_type().unwrap(), a);
}

#[test]
fn kurtrus_count_restarts_each_turn() {
    let mut g = game(303);
    g.set_hero_power(0, 0, cid("BG20_HERO_280p5"));
    let buy = |g: &mut Game| {
        g.with_recruit(0, |c| {
            c.emit(taverngym::event::Event::Bought {
                side: 0,
                card: cid("BG36_345"),
                eid: 0,
                sid: 0,
                price: 3,
            })
        })
    };
    buy(&mut g);
    buy(&mut g);
    end_turn_all(&mut g);
    g.players[0].hand.clear();
    buy(&mut g);
    assert!(
        g.players[0].hand.is_empty(),
        "two buys last turn don't count (T12)"
    );
    buy(&mut g);
    buy(&mut g);
    assert_eq!(g.players[0].hand.len(), 1);
}

#[test]
fn silas_tickets_on_any_tavern_card() {
    let mut g = game(304);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_101"));
    g.cfg.silas_ticket_pct = 0;
    g.with_recruit(0, |c| c.refresh_shop(0));
    assert!(g.players[0].powers[0].values.is_empty());
    g.cfg.silas_ticket_pct = 100;
    g.with_recruit(0, |c| c.refresh_shop(0));
    let n = g.players[0].shop.len();
    assert_eq!(
        g.players[0].powers[0].values.len(),
        n,
        "spells carry tickets too"
    );
    // Buying the Tavern spell collects its ticket.
    let i = g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_none())
        .expect("a Tavern spell");
    g.players[0].gold = 10;
    g.apply(0, Action::Buy(i)).unwrap();
    assert_eq!(g.players[0].powers[0].values.len(), n - 1);
    assert_eq!(g.players[0].powers[0].counter, 1);
}

#[test]
fn surprise_portrait_three_forms() {
    let surprise = cid("BG26_175");
    let mut g = game(305);
    g.players[0].trinkets.push(cid("BG30_MagicItem_555"));
    // On Refresh: about 10% a whole page, about 10% only the Elementals replaced.
    let (mut pages, rounds) = (0, 500);
    for _ in 0..rounds {
        g.with_recruit(0, |c| c.refresh_shop(0));
        let ms: Vec<_> = g.players[0]
            .shop
            .iter()
            .filter_map(|s| s.item.as_minion())
            .collect();
        if ms.iter().all(|m| m.card == surprise) {
            pages += 1;
            assert!(
                ms.iter().all(|m| m.pool_copies == 0),
                "extra cards, not pooled"
            );
        }
    }
    assert!((20..=100).contains(&pages), "{pages}");
    // On buying an Elemental: about 10% for an extra one in hand.
    let mut extra = 0;
    for _ in 0..rounds {
        g.players[0].hand.clear();
        g.with_recruit(0, |c| {
            c.emit(taverngym::event::Event::Bought {
                side: 0,
                card: surprise,
                eid: 0,
                sid: 0,
                price: 3,
            })
        });
        extra += g.players[0].hand.len();
    }
    assert!((20..=100).contains(&extra), "{extra}");
}

#[test]
fn trinket_weights_and_discountable_list() {
    use taverngym::content::trinket_offers::{DISCOUNTABLE, weight};
    let mut g = game(306);
    assert_eq!(weight(&g, "BG30_MagicItem_888"), 250);
    // Goldenizer Supply: weight 50, a "more likely" trinket (x2 by default).
    assert_eq!(weight(&g, "BG30_MagicItem_435"), 100);
    assert_eq!(weight(&g, "BG30_MagicItem_426"), 200);
    assert_eq!(weight(&g, "BG30_MagicItem_301"), 100);
    assert_eq!(DISCOUNTABLE.len(), 34);
    g.cfg.trinket_likely_pct = 300;
    assert_eq!(
        weight(&g, "BG30_MagicItem_301"),
        100,
        "not a 'more likely' trinket"
    );
    assert_eq!(weight(&g, "BG30_MagicItem_426"), 300);
}

// ---------------------------------------------------------------------------- 36.6.3

#[test]
fn patch_36_6_3_data_and_defaults() {
    let cfg = RulesConfig::default();
    assert!(!cfg.aberration_forced);
    assert_eq!(cfg.deity_threshold, 4);
    let d = db();
    assert_eq!(d.get(cid("BGFYM_000")).tier, 1, "C'Thun");
    assert_eq!(d.get(cid("BGFYM_011")).tier, 1, "Y'Shaarj");
    for id in ["BG33_828", "BG34_403", "BG28_583", "BG36_333"] {
        let c = d.get(cid(id));
        assert!(c.pools.minion || c.pools.tier7, "{id} in the pool");
    }
    for id in ["BG34_683", "BG26_537"] {
        assert!(!d.get(cid(id)).pools.minion, "{id} out of the pool");
    }
    assert!(d.get(cid("BG36_MagicItem_606")).pools.trinket);
    assert!(!d.get(cid("BG32_MagicItem_894")).pools.trinket);
    let eudora = d.get(cid("BG33_828"));
    assert_eq!((eudora.tier, eudora.atk, eudora.hp), (5, 8, 3));
    assert_eq!(d.get(cid("BG28_604")).cost, 3, "Butchering");
    assert_eq!(d.get(cid("BG34_689")).tier, 4, "Blood Gem Barrage");
    // Aberrations are in about half of the lobbies now.
    let with = (0..200)
        .filter(|s| {
            Game::new(*s, RulesConfig::default())
                .lobby_races
                .intersects(taverngym::types::Races::ABERRATION)
        })
        .count();
    assert!((70..=130).contains(&with), "{with}");
}

#[test]
fn rafaam_and_victims_specter_banned_with_aberrations() {
    use taverngym::types::Races;
    let rafaam = cid("TB_BaconShop_HERO_45");
    let mut seen_without = false;
    for seed in 0..60 {
        let g = Game::new(seed, RulesConfig::default());
        let offered = g.players.iter().any(|p| match p.pending.last() {
            Some(Pending::HeroSelect { options }) => options.contains(&rafaam),
            _ => false,
        });
        if g.lobby_races.intersects(Races::ABERRATION) {
            assert!(!offered, "seed {seed}");
        } else {
            seen_without |= offered;
        }
    }
    assert!(seen_without, "still offered in other lobbies");
    let specter = "BG24_Reward_138";
    assert!(taverngym::content::quest_offerable(Races::BEAST, specter));
    assert!(!taverngym::content::quest_offerable(
        Races::ABERRATION,
        specter
    ));
}

#[test]
fn corroder_kthir_and_murk_eye_end_of_turn() {
    // Nightmare Corroder: a Sludge Corrosion at the end of the turn.
    let mut g = game(900);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG36_115"]);
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(g.players[0].hand.len(), 1);
    assert_eq!(g.players[0].hand[0].item.card(), cid("BG36_301t"));
    // Golden K'Thir: discards 6 Tavern spells, +7/+7 each.
    let mut g = game(901);
    set_board(&mut g, 0, &["BG36_320_G"]);
    for _ in 0..7 {
        give(&mut g, 0, "BG28_810");
    }
    let before = stats(&g, 0)[0];
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(g.players[0].hand.len(), 1);
    assert_eq!(stats(&g, 0)[0], (before.0 + 42, before.1 + 42));
    // Young Murk-Eye: the Battlecries of both neighbours (Joyous: Deity +2/+1).
    let mut g = game(902);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG36_110", "BG22_403", "BG36_110"]);
    let d = g.players[0].deity.as_ref().unwrap();
    let before = (d.atk, d.hp);
    g.apply(0, Action::EndTurn).unwrap();
    let d = g.players[0].deity.as_ref().unwrap();
    assert_eq!((d.atk, d.hp), (before.0 + 4, before.1 + 2));
}

#[test]
fn auto_reveille_magnetizes_and_gives_a_copy() {
    let mut g = game(903);
    set_board(&mut g, 0, &["BG36_367"]);
    for _ in 0..3 {
        g.with_recruit(0, |c| {
            c.emit(taverngym::event::Event::Bought {
                side: 0,
                card: cid("BG36_345"),
                eid: 0,
                sid: 0,
                price: 3,
            })
        });
    }
    let m = &g.players[0].board[0];
    assert_eq!(m.attached.len(), 1, "a Volumizer is magnetized to it");
    let vol = m.attached[0];
    assert_eq!(g.players[0].hand.len(), 1);
    assert_eq!(
        g.players[0].hand[0].item.card(),
        vol,
        "and a copy of it in hand"
    );
}

#[test]
fn flapper_battlecry_and_eudora_and_tycoon() {
    // Razorfen Flapper: Battlecry and Deathrattle.
    let mut g = game(904);
    give(&mut g, 0, "BG34_682");
    play_last(&mut g, 0);
    assert_eq!(g.players[0].hand.len(), 1);
    assert_eq!(g.players[0].hand[0].item.card(), cid("BG34_689"));
    // Ship Master Eudora: +6/+6 to your minions; Golden ones keep it.
    let mut g = game(905);
    set_board(&mut g, 0, &["BG33_828", "BG36_345", "BG36_345_G"]);
    set_board(&mut g, 1, &["BG36_345"]);
    let before = stats(&g, 0);
    let mut c = combat_ctx(&mut g);
    let e = c.boards[0][0].eid;
    c.destroy(e);
    c.resolve_deaths();
    let now: Vec<(i32, i32)> = c.boards[0].iter().map(|m| (m.atk, m.hp)).collect();
    assert_eq!(now[0], (before[1].0 + 6, before[1].1 + 6));
    drop(c);
    let after = stats(&g, 0);
    assert_eq!(after[1], before[1], "combat-only on a plain minion");
    assert_eq!(
        after[2],
        (before[2].0 + 6, before[2].1 + 6),
        "kept by the Golden one"
    );
    // Eternal Tycoon: Avenge (5) summons an Eternal Knight that attacks at once.
    let mut g = game(906);
    g.cfg.record_log = true;
    set_board(
        &mut g,
        0,
        &[
            "BG34_403", "BG36_345", "BG36_345", "BG36_345", "BG36_345", "BG36_345",
        ],
    );
    let mut wall = minion(&mut g, 1, "BG36_345");
    wall.set_stats(0, 500);
    g.players[1].board = vec![wall];
    let mut c = combat_ctx(&mut g);
    for _ in 0..5 {
        let e = c.boards[0][1].eid;
        c.destroy(e);
        c.resolve_deaths();
    }
    let names: Vec<&str> = c.boards[0].iter().map(|m| m.name()).collect();
    assert_eq!(names, vec!["Eternal Tycoon", "Eternal Knight"]);
    assert!(c.boards[1][0].hp < 500, "the Knight attacked");
}

#[test]
fn trinkets_36_6_3() {
    // Sludge Portrait: only discarded spells give a Sludge Corrosion.
    let mut g = game(907);
    g.cfg.triple_copies = 99;
    buy_trinket(&mut g, 0, "BG36_MagicItem_430");
    g.players[0].hand.clear();
    give(&mut g, 0, "BG36_345");
    g.with_recruit(0, |c| {
        let hid = c.player_ref(0).unwrap().hand[0].hid;
        taverngym::recruit::discard_card(c, 0, hid);
    });
    assert!(
        g.players[0].hand.is_empty(),
        "a discarded minion gives nothing"
    );
    give(&mut g, 0, "BG28_810");
    g.with_recruit(0, |c| {
        let hid = c.player_ref(0).unwrap().hand[0].hid;
        taverngym::recruit::discard_card(c, 0, hid);
    });
    assert_eq!(g.players[0].hand.len(), 1);
    assert_eq!(g.players[0].hand[0].item.card(), cid("BG36_301t"));
    // Writhing Tentacles: the first minion discarded each turn returns with double stats.
    let mut g = game(908);
    g.cfg.triple_copies = 99;
    buy_trinket(&mut g, 0, "BG36_MagicItem_606");
    g.players[0].hand.clear();
    for _ in 0..2 {
        give(&mut g, 0, "BG36_345");
        g.with_recruit(0, |c| {
            let hid = c.player_ref(0).unwrap().hand.last().unwrap().hid;
            taverngym::recruit::discard_card(c, 0, hid);
        });
    }
    assert_eq!(g.players[0].hand.len(), 1, "once per turn");
    let m = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((m.atk, m.hp), (6, 6));
    // Maldraxxus Dagger: a Discover now, then at the start of every second turn.
    let mut g = game(909);
    g.turn = 6; // Dark Gifts are on offer
    set_board(&mut g, 0, &["BG36_345"]);
    buy_trinket(&mut g, 0, "BG36_MagicItem_370");
    assert_eq!(g.players[0].pending.len(), 1);
    g.apply(0, Action::Choose(0)).unwrap();
    end_turn_all(&mut g);
    assert!(g.players[0].pending.is_empty(), "nothing on the next turn");
    end_turn_all(&mut g);
    assert_eq!(g.players[0].pending.len(), 1, "again two turns later");
}

#[test]
fn echoing_voice_is_not_offered_on_maw_caster() {
    let mut g = game(910);
    let voice = cid("BG36_MidGameEffect_000t10");
    let (maw, other) = (db().get(cid("BG32_340")), db().get(cid("BG36_110")));
    g.with_recruit(0, |c| {
        assert!(!taverngym::content::gift_fits(c, 0, voice, maw, 1));
        assert!(taverngym::content::gift_fits(c, 0, voice, other, 1));
    });
}

#[test]
fn sinister_invitation_follows_dark_gift_tiers() {
    // Turn 9: Dark Gift minions are Tier 4-6, whatever the player's Tavern Tier.
    let mut g = game(911);
    g.turn = 9;
    g.players[0].tier = 2;
    buy_trinket(&mut g, 0, "BG36_MagicItem_610");
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!("expected a Discover")
    };
    assert!(!options.is_empty());
    for o in options {
        let m = o.as_minion().unwrap();
        assert!(
            m.golden && (4..=6).contains(&m.tier),
            "{} T{}",
            m.name(),
            m.tier
        );
    }
}

#[test]
fn aberration_subset_needs_aberrations_in_the_lobby() {
    use taverngym::types::Races;
    let (mut with, mut without) = (0, 0);
    for seed in 0..40 {
        let mut g = Game::new(seed, RulesConfig::default());
        let aberrations = g.lobby_races.intersects(Races::ABERRATION);
        // The Deity exists only with Aberrations.
        assert!(g.players.iter().all(|p| p.deity.is_some() == aberrations));
        // Dark Paradox: one form in every lobby.
        let forms = g
            .universe
            .iter()
            .filter(|c| db().get(**c).id.starts_with("BG36_360t"))
            .count();
        assert_eq!(forms, 1, "seed {seed}");
        // Energizing Chamber ("give your Deity +7/+7") and Sludge Portrait follow the lobby.
        let chamber = cid("BG36_371");
        let spell = g.with_recruit(0, |c| c.random_spell(|d| d.cid == chamber));
        assert_eq!(spell.is_some(), aberrations, "seed {seed}");
        g.turn = 9;
        g.players[0].tier = 5;
        let offer = g.trinket_offer(0, taverngym::cards::CardKind::TrinketGreater);
        let subset = offer.iter().any(|t| !db().get(*t).lobby_req.is_none());
        assert!(aberrations || !subset, "seed {seed}");
        if aberrations {
            with += 1;
        } else {
            without += 1;
        }
    }
    assert!(with > 5 && without > 5);
}

#[test]
fn cards_kept_by_freeze_are_not_new_cards() {
    // Silas: a kept card without a ticket is not rolled again at the turn-start refill;
    // Surprise Portrait never replaces a kept card.
    let mut g = game(912);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_101"));
    g.players[0].trinkets.push(cid("BG30_MagicItem_555"));
    g.cfg.silas_ticket_pct = 0;
    g.with_recruit(0, |c| c.refresh_shop(0));
    g.cfg.silas_ticket_pct = 100;
    // Turns 2-5: before the first trinket offer.
    for round in 0..4 {
        // Keep only the first card; the rest is refilled next turn.
        let keep = g.players[0].shop[0].clone();
        g.players[0].shop.truncate(1);
        g.apply(0, Action::Freeze).unwrap();
        end_turn_all(&mut g);
        if !g.players[0].alive {
            break;
        }
        let shop = &g.players[0].shop;
        assert_eq!(shop[0].sid, keep.sid, "round {round}");
        assert_eq!(shop[0].item.card(), keep.item.card(), "kept card untouched");
        let tickets = &g.players[0].powers[0].values;
        assert!(
            !tickets.contains(&(keep.sid as i32)),
            "no new roll for a kept card"
        );
        assert_eq!(
            tickets.len(),
            shop.len() - 1,
            "every new card got its ticket"
        );
    }
}

#[test]
fn sylvanas_discovers_a_minion_that_died_last_combat() {
    let mut g = game(913);
    let hp = cid("BG23_HERO_306p");
    g.set_hero_power(0, 0, hp);
    g.turn = 3;
    g.players[0].gold = 10;
    assert!(!g.can_hero_power(0, 0), "nothing died yet");
    set_board(&mut g, 0, &["BG36_345_G"]);
    let mut big = minion(&mut g, 1, "BG36_098");
    big.set_stats(50, 50);
    g.players[1].board = vec![big];
    g.simulate_combat(0, Some(1), None);
    assert_eq!(
        g.players[0].powers[0].memory,
        vec![cid("BG36_345")],
        "plain form"
    );
    assert!(g.can_hero_power(0, 0));
    g.players[0].hand.clear();
    g.apply(0, Action::HeroPower(0)).unwrap();
    assert_eq!(g.players[0].gold, 8);
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!("expected a Discover")
    };
    assert_eq!(options.len(), 1);
    g.apply(0, Action::Choose(0)).unwrap();
    let m = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((m.card, m.golden), (cid("BG36_345"), false));
    // The next combat starts a new list.
    set_board(&mut g, 0, &[]);
    g.players[1].board.clear();
    g.simulate_combat(0, Some(1), None);
    assert!(g.players[0].powers[0].memory.is_empty());
}

#[test]
fn a_failed_summon_returns_its_pool_copy() {
    // Found by the fuzzer: a copy summoned onto a full board kept its pool copy.
    let mut g = game(914);
    let ids = ["BG36_345"; 7];
    set_board(&mut g, 0, &ids);
    let card = cid("BG36_345");
    let before = g.pool.get(card);
    g.with_recruit(0, |c| {
        let orig = c.boards[0][0].clone();
        let copy = c.full_copy(&orig);
        assert_eq!(copy.pool_copies, 1);
        assert!(c.summon(0, 7, copy).is_none());
    });
    assert_eq!(g.pool.get(card), before);
}

/// A game past hero selection with the DEFAULT configuration and no Aberrations in the
/// lobby (half of the lobbies since 36.6.3); seats pinned like `game`.
fn game_without_aberrations(from_seed: u64) -> Game {
    use taverngym::types::Races;
    let seed = (from_seed..from_seed + 200)
        .find(|s| {
            !Game::new(*s, RulesConfig::default())
                .lobby_races
                .intersects(Races::ABERRATION)
        })
        .unwrap();
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
        g.players[p].max_gold_bonus = 0;
        g.players[p].skipping = false;
        g.players[p].board.clear();
        g.players[p].powers.clear();
        g.players[p].set_hero_power(0, cid("TB_BaconShop_HP_035"));
    }
    g
}

#[test]
fn lobby_without_aberrations_has_no_deity_system() {
    use taverngym::types::Races;
    let mut g = game_without_aberrations(5000);
    g.cfg.record_log = true;
    assert!(g.players.iter().all(|p| p.deity.is_none()));
    // No Aberration minion is in the pool; Dark Paradox (all types) is.
    for c in &g.universe {
        let d = db().get(*c);
        assert!(d.races != Races::ABERRATION, "{}", d.name);
    }
    // All-type minions count as Aberrations, but with no Deity nothing awakens.
    let mut b = vec![];
    for _ in 0..5 {
        let mut m = minion(&mut g, 0, "BG36_360t3"); // Dark Paradox (all types)
        m.set_stats(1, 1);
        b.push(m);
    }
    g.players[0].board = b;
    let mut enemy = minion(&mut g, 1, "BG36_345");
    enemy.set_stats(10, 200);
    g.players[1].board = vec![enemy];
    g.log.clear();
    fight(&mut g);
    assert!(
        !g.log.iter().any(|l| l.contains("deity awakens")),
        "{:#?}",
        g.log
    );
    // Deity buffs are simply lost (Joyous: "give your Deity +2/+1").
    give(&mut g, 0, "BG36_110");
    g.players[0].board.clear();
    play_last(&mut g, 0);
    assert!(g.players[0].deity.is_none());
    // Dark Gifts, trinkets and the turn flow work the same.
    for _ in 0..6 {
        end_turn_all(&mut g);
    }
    assert!(g.turn >= 6);
}

#[test]
fn golden_forms_of_the_36_6_3_minions() {
    // Golden Ship Master Eudora: +6/+6 twice; Golden minions keep it.
    let mut g = game(920);
    set_board(&mut g, 0, &["BG33_828_G", "BG36_345", "BG36_345_G"]);
    let before = stats(&g, 0);
    let mut c = combat_ctx(&mut g);
    let e = c.boards[0][0].eid;
    c.destroy(e);
    c.resolve_deaths();
    let now: Vec<(i32, i32)> = c.boards[0].iter().map(|m| (m.atk, m.hp)).collect();
    assert_eq!(now[0], (before[1].0 + 12, before[1].1 + 12));
    assert_eq!(
        now[1],
        (before[2].0 + 12, before[2].1 + 12),
        "applied once per trigger"
    );
    drop(c);
    let after = stats(&g, 0);
    assert_eq!(after[1], before[1]);
    assert_eq!(after[2], (before[2].0 + 12, before[2].1 + 12));
    // Golden Eternal Tycoon summons a Golden Eternal Knight.
    let mut g = game(921);
    let ids = [
        "BG34_403_G",
        "BG36_345",
        "BG36_345",
        "BG36_345",
        "BG36_345",
        "BG36_345",
    ];
    set_board(&mut g, 0, &ids);
    let mut wall = minion(&mut g, 1, "BG36_345");
    wall.set_stats(0, 500);
    g.players[1].board = vec![wall];
    let mut c = combat_ctx(&mut g);
    for _ in 0..5 {
        let e = c.boards[0][1].eid;
        c.destroy(e);
        c.resolve_deaths();
    }
    let knight = c.boards[0].last().unwrap();
    assert_eq!(knight.card, cid("BG25_008_G"));
    assert!(knight.golden);
    drop(c);
    // Golden Young Murk-Eye: the adjacent Battlecries twice each (Joyous: Deity +2/+1).
    let mut g = game(922);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG36_110", "BG22_403_G", "BG36_110"]);
    let d = g.players[0].deity.as_ref().unwrap();
    let before = (d.atk, d.hp);
    g.apply(0, Action::EndTurn).unwrap();
    let d = g.players[0].deity.as_ref().unwrap();
    assert_eq!((d.atk, d.hp), (before.0 + 8, before.1 + 4));
}

#[test]
fn surprise_portrait_swaps_only_elementals() {
    use taverngym::types::Races;
    // A lobby with Elementals; the extra Surprises are the ones holding no pool copy.
    let seed = (930u64..1100)
        .find(|s| game(*s).lobby_races.intersects(Races::ELEMENTAL))
        .unwrap();
    let mut g = game(seed);
    g.players[0].tier = 4;
    g.players[0].trinkets.push(cid("BG30_MagicItem_555"));
    let surprise = cid("BG26_175");
    let mut swaps = 0;
    for _ in 0..1500 {
        g.with_recruit(0, |c| c.refresh_shop(0));
        let ms: Vec<_> = g.players[0]
            .shop
            .iter()
            .filter_map(|s| s.item.as_minion())
            .collect();
        let extra = ms
            .iter()
            .filter(|m| m.card == surprise && m.pool_copies == 0)
            .count();
        if extra > 0 && extra < ms.len() {
            swaps += 1;
            // The Elementals were replaced; the other minions stayed.
            assert!(
                ms.iter()
                    .all(|m| m.card == surprise || !m.races.intersects(Races::ELEMENTAL)),
                "{:?}",
                ms.iter().map(|m| m.name()).collect::<Vec<_>>()
            );
        }
    }
    assert!(swaps > 20, "{swaps}");
}

#[test]
fn barov_and_akazamzarak_availability() {
    use taverngym::content::hero_power_available as avail;
    let mut g = game(940);
    // Lord Barov: a real combat to bet on.
    let barov = cid("TB_BaconShop_HP_081");
    g.set_hero_power(0, 0, barov);
    assert!(avail(&g, 0, barov));
    g.pairs = vec![(0, None)];
    assert!(
        !avail(&g, 0, barov),
        "only a ghost pairing and nobody eliminated"
    );
    g.eliminated.push(7);
    assert!(
        avail(&g, 0, barov),
        "the ghost has a warband to bet against"
    );
    // Akazamzarak: a Secret that is not in play yet.
    let aka = cid("TB_BaconShop_HP_020");
    g.set_hero_power(0, 0, aka);
    assert!(avail(&g, 0, aka));
    g.players[0].secrets = [
        "TB_Bacon_Secrets_01",
        "TB_Bacon_Secrets_07",
        "TB_Bacon_Secrets_10",
        "TB_Bacon_Secrets_15",
    ]
    .iter()
    .map(|s| cid(s))
    .collect();
    assert!(!avail(&g, 0, aka));
    // The whole check goes through the action mask too.
    g.players[0].gold = 10;
    assert!(!g.can_hero_power(0, 0));
}

#[test]
fn observation_shows_pending_source_quests_and_tavern_flags() {
    use taverngym::actions::{N_BOARD, N_HAND, N_POWERS};
    use taverngym::obs::{GLOBAL_F, ITEM_F, MINION_F, N_QUESTS, PENDING_KINDS, QUEST_F, observe};
    let mut g = game(950);
    // Tavern cards: frozen, kept from last turn, Darkmoon Ticket.
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_101"));
    g.cfg.silas_ticket_pct = 100;
    g.with_recruit(0, |c| c.refresh_shop(0));
    g.apply(0, Action::Freeze).unwrap();
    let shop0 =
        GLOBAL_F + N_QUESTS * QUEST_F + PENDING_KINDS + N_BOARD * MINION_F + N_HAND * ITEM_F;
    let o = observe(&g, 0);
    assert_eq!(o.floats.len(), taverngym::obs::OBS_FLOATS);
    assert_eq!(o.cards.len(), taverngym::obs::OBS_CARDS);
    assert_eq!(
        &o.floats[shop0 + 6..shop0 + 9],
        &[1.0, 0.0, 1.0],
        "frozen, not kept, ticket"
    );
    end_turn_all(&mut g);
    let o = observe(&g, 0);
    assert_eq!(o.floats[shop0 + 7], 1.0, "kept from last turn");
    // A per-card price is what the observation reports.
    g.players[0].shop[0].price = Some(1);
    let o = observe(&g, 0);
    assert_eq!(o.floats[shop0 + 2], 1.0 / 5.0);
    // Deity, the source of a pending choice, an active Quest with its Reward.
    let deity = g.players[0].deity.as_ref().unwrap().card;
    let quest = cid("BG24_Quest_123");
    let reward = cid("BG24_Reward_107");
    g.players[0].quests.push(taverngym::state::QuestState {
        card: quest,
        reward,
        progress: 2,
        goal: 4,
        data: 0,
        reward_data: 0,
    });
    let source = cid("BG28_303");
    let pick = minion(&mut g, 0, "BG36_345");
    g.players[0].pending.push(Pending::Discover {
        source,
        options: vec![Item::Minion(pick)],
    });
    let o = observe(&g, 0);
    let c0 = 1 + N_POWERS;
    assert_eq!(o.cards[c0], deity.0 as i32 + 1);
    assert_eq!(o.cards[c0 + 1], source.0 as i32 + 1);
    assert_eq!(o.cards[c0 + 2], quest.0 as i32 + 1);
    assert_eq!(o.cards[c0 + 3], reward.0 as i32 + 1);
    assert_eq!(&o.floats[GLOBAL_F..GLOBAL_F + 2], &[1.0, 0.5]);
}

#[test]
fn tavern_and_warband_never_exceed_seven() {
    // The Tavern and the warband hold at most 7 cards, whatever happens.
    let mut g = game(951);
    assert_eq!(g.cfg.shop_max_cards, 7);
    assert_eq!(g.cfg.board_size, 7);
    for _ in 0..12 {
        put_shop(&mut g, 0, "BG36_345");
    }
    let card = cid("BG36_345");
    let before = g.pool.get(card);
    // Every effect context ends by trimming the Tavern (cards only enter it through one).
    g.with_recruit(0, |_| ());
    assert_eq!(g.players[0].shop.len(), 7);
    assert!(
        g.pool.get(card) >= before,
        "dropped cards return their copies"
    );
    // A Refresh at any Tier fills at most 7 cards, the spell included; extra offers
    // (Fodder, "an extra minion each Refresh") never push it past 7.
    for tier in 1..=7 {
        g.players[0].tier = tier;
        g.players[0].fodder_queue = vec![1; 5];
        g.with_recruit(0, |c| c.refresh_shop(0));
        let shop = &g.players[0].shop;
        assert!(shop.len() <= 7, "tier {tier}: {}", shop.len());
        if tier >= 6 {
            assert_eq!(shop.len(), 7, "tier {tier}");
            assert!(
                shop.iter().any(|s| s.item.as_minion().is_none()),
                "the spell keeps its slot"
            );
        }
    }
    // The warband: an 8th minion can't be played or summoned.
    g.cfg.triple_copies = 99; // (no triple merge freeing a slot)
    let ids = ["BG36_345"; 7];
    set_board(&mut g, 0, &ids);
    give(&mut g, 0, "BG36_345");
    let hand = g.players[0].hand.len() - 1;
    assert!(g.apply(0, Action::Play { hand, pos: 7 }).is_err());
    g.with_recruit(0, |c| {
        let m = c.new_minion(0, cid("BG36_345"));
        assert!(c.summon(0, 0, m).is_none());
    });
    assert_eq!(g.players[0].board.len(), 7);
}

#[test]
fn extra_offers_have_priority_over_the_default_fill() {
    // In a Tavern of 7, cards added by effects come first and the default Refresh fill takes what
    // is left (minions give way before the spell).
    let fodder = cid("BG35_150t");
    let count = |g: &Game| {
        let shop = &g.players[0].shop;
        let f = shop.iter().filter(|s| s.item.card() == fodder).count();
        let spells = shop.iter().filter(|s| s.item.as_minion().is_none()).count();
        (shop.len(), f, shop.len() - f - spells, spells)
    };
    let mut g = game(960);
    g.players[0].tier = 6;
    g.with_recruit(0, |c| c.refresh_shop(0));
    assert_eq!(count(&g), (7, 0, 6, 1), "Tier 6: 6 minions + the spell");
    // Two Fodders arrive with the next Refresh: they replace two default minions.
    g.players[0].fodder_queue = vec![2];
    g.with_recruit(0, |c| c.player_refresh_shop(0)); // (a Fodder needs the Refresh button)
    assert_eq!(count(&g), (7, 2, 4, 1));
    // Six extras: the default minions are all gone, then the spell gives way too.
    g.players[0].fodder_queue = vec![6];
    g.with_recruit(0, |c| c.player_refresh_shop(0));
    assert_eq!(count(&g), (7, 6, 0, 1));
    g.players[0].fodder_queue = vec![7];
    g.with_recruit(0, |c| c.player_refresh_shop(0));
    assert_eq!(count(&g), (7, 7, 0, 0));
    // More extras than slots: the Tavern still holds 7.
    g.players[0].fodder_queue = vec![9];
    g.with_recruit(0, |c| c.player_refresh_shop(0));
    assert_eq!(count(&g).0, 7);
    // Cards kept by a Freeze are not default-fill cards: an extra can't push them out.
    g.players[0].fodder_queue.clear();
    g.with_recruit(0, |c| c.refresh_shop(0));
    let kept: Vec<_> = g.players[0].shop.iter().map(|s| s.sid).collect();
    g.apply(0, Action::Freeze).unwrap();
    g.players[0].fodder_queue = vec![1];
    end_turn_all(&mut g);
    let now: Vec<_> = g.players[0].shop.iter().map(|s| s.sid).collect();
    assert_eq!(now, kept, "a fully kept Tavern has no room for the Fodder");
    // Pool copies of the replaced minions went back.
    let (held, initial) = taverngym::invariants::pool_balance(&g);
    assert_eq!(held, initial);
}

#[test]
fn efficiency_skips_activate_minions_that_gain_nothing_from_it() {
    let mut g = game(970);
    let eff = cid("BG36_MidGameEffect_000t78");
    g.with_recruit(0, |c| {
        let fits = |c: &taverngym::engine::Ctx, id: &str| {
            taverngym::content::gift_fits(c, 0, eff, db().get(cid(id)), 1)
        };
        // Offered on ordinary Activate minions...
        assert!(fits(c, "BG36_345") && fits(c, "BG36_346"));
        // ...not on the tagged ones (36.6.2), nor on minions without Activate.
        for id in ["BG36_099", "BG36_356", "BG36_511", "BG28_582", "BG36_201"] {
            assert!(!fits(c, id), "{id}");
        }
        assert!(!fits(c, "BG36_110"));
    });
}

#[test]
fn lobby_types_are_always_in_the_lobby() {
    use taverngym::types::Races;
    for seed in 0..30 {
        let cfg = RulesConfig {
            lobby_types: vec!["MURLOC".into(), "MECHANICAL".into()],
            ..RulesConfig::default()
        };
        let g = Game::new(seed, cfg);
        assert!(g.lobby_races.intersects(Races::MURLOC) && g.lobby_races.intersects(Races::MECH));
        assert_eq!(
            g.lobby_races.iter().count(),
            5,
            "the other slots are random"
        );
    }
    let bad = RulesConfig {
        lobby_types: vec!["GNOME".into()],
        ..RulesConfig::default()
    };
    assert!(bad.validate().is_err());
}

#[test]
fn hero_pool_limits_the_offers_and_repeats_heroes() {
    let patchwerk = "TB_BaconShop_HERO_34".to_string();
    let cfg = RulesConfig {
        hero_pool: vec![patchwerk.clone()],
        hero_choices: 1,
        ..RulesConfig::default()
    };
    assert!(cfg.validate().is_ok());
    let g = Game::new(5, cfg);
    let d = db();
    for p in &g.players {
        let Some(Pending::HeroSelect { options }) = p.pending.last() else {
            panic!("hero offer expected");
        };
        assert_eq!(options.len(), 1);
        assert_eq!(
            d.get(options[0]).id,
            patchwerk,
            "every seat gets the only hero"
        );
    }
    let bad = RulesConfig {
        hero_pool: vec!["BG20_100".into()],
        ..RulesConfig::default()
    };
    assert!(bad.validate().is_err(), "a minion is not a hero");
}

#[test]
fn debug_offer_hero_puts_the_hero_among_the_choices() {
    use taverngym::debug::{DebugCmd, run_debug};
    let mut g = Game::new(11, RulesConfig::default());
    let nzoth = "TB_BaconShop_HERO_93".to_string();
    run_debug(
        &mut g,
        DebugCmd::OfferHero {
            p: 0,
            hero: nzoth.clone(),
        },
    )
    .unwrap();
    let Some(Pending::HeroSelect { options }) = g.players[0].pending.last() else {
        panic!("hero selection expected")
    };
    assert_eq!(db().get(options[0]).id, nzoth);
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(
        g.players[0].hero.map(|h| db().get(h).id.clone()),
        Some(nzoth)
    );
    // Outside hero selection the command is refused.
    assert!(
        run_debug(
            &mut g,
            DebugCmd::OfferHero {
                p: 0,
                hero: "TB_BaconShop_HERO_16".into()
            }
        )
        .is_err()
    );
}

#[test]
fn replay_frames_record_who_attacked_whom() {
    let mut g = game(77);
    g.cfg.record_log = true;
    set_board(&mut g, 0, &["BG36_345"]);
    set_board(&mut g, 1, &["BG36_345"]);
    let (a, b) = (g.players[0].board[0].eid, g.players[1].board[0].eid);
    g.simulate_combat(0, Some(1), None);
    let attacks: Vec<_> = g
        .last_frames
        .iter()
        .flat_map(|f| f.attacks.clone())
        .collect();
    assert!(!attacks.is_empty());
    assert!(
        attacks.iter().all(|x| *x == (a, b) || *x == (b, a)),
        "{attacks:?}"
    );
    assert!(
        g.last_frames[0].attacks.is_empty(),
        "nothing attacks before the first frame"
    );
}

#[test]
fn typed_tavern_spells_need_their_type_in_the_lobby() {
    use taverngym::types::Races;
    let conch = cid("BG28_601"); // Cloning Conch: Murloc
    let winds = cid("BG34_444"); // Easterly Winds: Elemental
    assert_eq!(db().get(conch).lobby_req, Races::MURLOC);
    assert_eq!(db().get(winds).lobby_req, Races::ELEMENTAL);
    let (mut with, mut without) = (0, 0);
    for seed in 0..60 {
        let g = Game::new(seed, RulesConfig::default());
        for s in &g.spell_universe {
            let req = db().get(*s).lobby_req;
            assert!(
                req.is_none() || req.intersects(g.lobby_races),
                "{} in a lobby without its type",
                db().get(*s).name
            );
        }
        let murlocs = g.lobby_races.intersects(Races::MURLOC);
        assert_eq!(g.spell_universe.contains(&conch), murlocs);
        if murlocs {
            with += 1;
        } else {
            without += 1;
        }
    }
    assert!(with > 0 && without > 0);
}

/// A game on `turn` whose player 0 has five Murlocs and Gold for the Dark Gift button.
fn murloc_game(seed: u64, turn: u32, cfg: RulesConfig) -> Game {
    let board = ["BG22_202", "BG29_300", "BG32_330", "BG34_140", "BG26_137"];
    gift_game(seed, turn, cfg, &["MURLOC"], &board)
}

fn discover_options(g: &Game) -> Vec<Minion> {
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!("a Discover is expected")
    };
    options
        .iter()
        .filter_map(|o| o.as_minion().cloned())
        .collect()
}

/// A game on `turn` whose player 0 has `board`, `types` always in the lobby, and Gold for the
/// Dark Gift button.
fn gift_game(seed: u64, turn: u32, cfg: RulesConfig, types: &[&str], board: &[&str]) -> Game {
    let cfg = RulesConfig {
        lobby_types: types.iter().map(|t| t.to_string()).collect(),
        ..cfg
    };
    let mut g = Game::new(seed, cfg);
    for p in 0..g.players.len() {
        if matches!(
            g.players[p].pending.last(),
            Some(Pending::HeroSelect { .. })
        ) {
            g.apply(p, Action::Choose(0)).unwrap();
        }
        g.players[p].pending.clear();
    }
    g.turn = turn;
    g.players[0].gold = 10;
    g.players[0].tier = 6;
    g.players[0].skipping = false; // a hero whose turn this is not
    set_board(&mut g, 0, board);
    g
}

#[test]
fn dark_gift_button_follows_the_client_slots() {
    use std::collections::BTreeSet;
    use taverngym::types::Races;
    // The options' Tiers by place, the first option of the most common type from turn 6, the
    // second one too from turn 10; no Tier 6 guarantee.
    let tier = |m: &Minion| db().get(m.card).tier;
    let murloc = |m: &Minion| taverngym::content::goes_with_type(db().get(m.card), Races::MURLOC);
    let offer = |seed: u64, turn: u32| {
        let mut g = murloc_game(seed, turn, RulesConfig::default());
        g.apply(0, Action::DarkGift).unwrap();
        discover_options(&g)
    };
    let (mut t9, mut late) = (BTreeSet::new(), BTreeSet::new());
    for seed in 0..60 {
        for (turn, want) in [
            (3, [2, 2, 2]),
            (4, [2, 2, 3]),
            (5, [3, 3, 3]),
            (6, [3, 4, 4]),
            (7, [4, 4, 4]),
            (8, [4, 5, 5]),
        ] {
            let o = offer(seed, turn);
            assert_eq!(
                o.iter().map(tier).collect::<Vec<_>>(),
                want,
                "seed {seed} turn {turn}"
            );
            assert!(
                turn < 6 || murloc(&o[0]),
                "seed {seed} turn {turn}: the first is typed"
            );
        }
        let o = offer(seed, 9);
        assert!(murloc(&o[0]), "seed {seed} turn 9");
        t9.insert(o.iter().map(tier).collect::<Vec<_>>());
        for turn in [10, 13] {
            let o = offer(seed, turn);
            assert_eq!(tier(&o[0]), 5, "seed {seed} turn {turn}");
            assert!(
                murloc(&o[0]) && murloc(&o[1]),
                "seed {seed} turn {turn}: two typed"
            );
            late.insert(o.iter().map(tier).collect::<Vec<_>>());
        }
    }
    let set = |v: &[[u8; 3]]| v.iter().map(|x| x.to_vec()).collect::<BTreeSet<_>>();
    assert_eq!(t9, set(&[[4, 5, 6], [5, 5, 6]]));
    assert_eq!(late, set(&[[5, 5, 5], [5, 5, 6], [5, 6, 5], [5, 6, 6]]));
    // Settings at 0 / 100 pin the two-Tier places.
    let at = |t9_pct: u64, late_pct: u64, turn: u32| {
        let cfg = RulesConfig {
            dark_gift_t9_first_t5_pct: t9_pct,
            dark_gift_late_t6_pct: late_pct,
            ..RulesConfig::default()
        };
        let mut g = murloc_game(7, turn, cfg);
        g.apply(0, Action::DarkGift).unwrap();
        discover_options(&g).iter().map(tier).collect::<Vec<_>>()
    };
    assert_eq!(at(0, 0, 9), [4, 5, 6]);
    assert_eq!(at(100, 0, 9), [5, 5, 6]);
    assert_eq!(at(0, 0, 11), [5, 5, 5]);
    assert_eq!(at(0, 100, 11), [5, 6, 6]);
    // The type is counted on the board only: two Mechs and a Dragon there, two Dragons in
    // hand -> the first option is a Mech.
    for seed in 0..30 {
        let mut g = gift_game(
            seed,
            6,
            RulesConfig::default(),
            &["MECHANICAL", "DRAGON"],
            &["BG29_611", "BG31_177", "BG29_888"],
        );
        give(&mut g, 0, "BGS_034");
        give(&mut g, 0, "BG35_814");
        g.players[0].pending.clear(); // what the hero opened on the gain
        g.apply(0, Action::DarkGift).unwrap();
        let first = db().get(discover_options(&g)[0].card);
        assert!(
            taverngym::content::goes_with_type(first, Races::MECH),
            "seed {seed}: {}",
            first.name
        );
    }
    // Every offer: different minions, different gifts, no Battlecry / Choose One minion on
    // turns 3 and 4, Gilding only at the offer's lowest Tier.
    let gilding = cid("BG36_MidGameEffect_000t14");
    let mut gilded = 0;
    for seed in 0..200 {
        let turn = 3 + (seed as u32 % 9);
        let mut g = game(9000 + seed);
        g.turn = turn;
        g.players[0].gold = 10;
        g.apply(0, Action::DarkGift).unwrap();
        let o = discover_options(&g);
        assert_eq!(o.len(), 3, "seed {seed}");
        let cards: BTreeSet<_> = o.iter().map(|m| m.card).collect();
        let gifts: BTreeSet<_> = o.iter().map(|m| m.attached[0]).collect();
        assert_eq!((cards.len(), gifts.len()), (3, 3), "seed {seed}");
        let lowest = o.iter().map(tier).min().unwrap();
        for m in &o {
            let def = db().get(m.card);
            let once = def.mech.has(taverngym::types::Mech::BATTLECRY)
                || def.mech.has(taverngym::types::Mech::CHOOSE_ONE);
            assert!(
                turn >= 5 || !once,
                "seed {seed}: {} on turn {turn}",
                def.name
            );
            if m.attached[0] == gilding {
                gilded += 1;
                assert_eq!(
                    def.tier, lowest,
                    "seed {seed}: Gilding above the lowest Tier"
                );
            }
        }
    }
    assert!(gilded > 0);
}

#[test]
fn dark_gift_allows_the_client_battlecry_pairs() {
    use taverngym::content::gift_fits;
    // Pairs the client offers that a four-gift list for Battlecry / Choose One minions would
    // refuse.
    let mut g = game(9300);
    g.turn = 5;
    for (minion, gift) in [
        ("BG36_112", "BG36_MidGameEffect_000t71"), // Fetid Corroder + Hostility, turn 5
        ("BG30_123", "BG36_MidGameEffect_000t71"), // Fearless Foodie (Choose One)
        ("BG27_084", "BG36_MidGameEffect_000t71"), // Sprightly Scarab (Choose One)
        ("BG36_330", "BG36_MidGameEffect_000t71"), // Sly Infiltrator (Choose One)
        ("BG34_682", "BG36_MidGameEffect_000t16"), // Razorfen Flapper + Jaws of Death
        ("BG23_017", "BG36_MidGameEffect_000t16"), // Sanguine Champion + Jaws of Death
        ("BG31_148", "BG36_MidGameEffect_000t69"), // Hackerfin + Violent Toxicity
    ] {
        let def = db().get(cid(minion));
        assert!(
            g.with_recruit(0, |c| gift_fits(c, 0, cid(gift), def, def.tier)),
            "{minion} + {gift}"
        );
    }
    // Still not every gift: a stat gift stays off Battlecry minions.
    let fort = cid("BG36_MidGameEffect_000t73");
    assert!(!g.with_recruit(0, |c| gift_fits(c, 0, fort, db().get(cid("BG36_112")), 3)));
    // Hostility is in the turn-5 offer pool (client: eight times on turn 5).
    let hostility = cid("BG36_MidGameEffect_000t71");
    assert!(g.with_recruit(0, |c| {
        taverngym::content::window_gifts(c).contains(&hostility)
    }));
    g.turn = 4;
    assert!(!g.with_recruit(0, |c| {
        taverngym::content::window_gifts(c).contains(&hostility)
    }));
}

#[test]
fn dark_gift_type_table_helpers() {
    // A typeless minion tied to a type in the client data counts for it (Nadina the Red: Dragons;
    // Disguised Graverobber: Undead); a general neutral minion does not (Brann, Balinda).
    let goes = |id: &str, r: Races| taverngym::content::goes_with_type(db().get(cid(id)), r);
    assert!(goes("BGS_040", Races::DRAGON) && !goes("BGS_040", Races::MURLOC));
    assert!(goes("BG28_303", Races::UNDEAD));
    for r in [Races::MURLOC, Races::DRAGON, Races::BEAST] {
        assert!(!goes("BG_LOE_077", r) && !goes("BG35_883", r));
    }
    // Minions no gifted Discover may offer (official exclusions and the 36.2.2 ban).
    for id in ["BG32_237", "BG24_715", "BGS_131", "BG23_318", "BG36_620"] {
        assert!(
            !taverngym::recruit::gift_compatible(db().get(cid(id)), 10),
            "{id}"
        );
    }
    // Suspicious Prisonguard is Tier 1: below every Tier the button offers.
    assert!((1..=20).all(|t| !RulesConfig::default().dark_gift_tiers(t).contains(&1)));
}

#[test]
fn timeworn_candelabra_follows_the_timewarp_offer_rules() {
    use taverngym::debug::{DebugCmd, run_debug};
    use taverngym::types::Races;
    for seed in 0..40 {
        // Major Timewarp, five Murlocs: two of the three options are tied to Murlocs, and no
        // option is tied to a type that isn't in the lobby.
        let mut g = murloc_game(seed, 9, RulesConfig::default());
        let lobby = g.lobby_races;
        run_debug(
            &mut g,
            DebugCmd::GiveTrinket {
                p: 0,
                card: "BG35_MagicItem_823t".into(),
            },
        )
        .unwrap();
        let o = discover_options(&g);
        assert_eq!(o.len(), 3, "seed {seed}");
        let tied = |m: &Minion| db().get(m.card).tw_types;
        assert!(
            o.iter()
                .all(|m| db().get(m.card).cost == 2 && db().get(m.card).tier == 5)
        );
        assert!(
            o.iter()
                .all(|m| tied(m).is_none() || tied(m).intersects(lobby))
        );
        assert!(
            o.iter()
                .filter(|m| tied(m).intersects(Races::MURLOC))
                .count()
                >= 2,
            "seed {seed}"
        );
        // Minor Timewarp with an empty warband: cards that need four minions are not offered.
        let mut g = murloc_game(seed, 6, RulesConfig::default());
        g.players[0].board.clear();
        run_debug(
            &mut g,
            DebugCmd::GiveTrinket {
                p: 0,
                card: "BG35_MagicItem_823".into(),
            },
        )
        .unwrap();
        let o = discover_options(&g);
        assert!(!o.is_empty());
        assert!(
            o.iter().all(|m| db().get(m.card).tw_min_minions == 0),
            "seed {seed}"
        );
    }
}

// ---------------------------------------------------------------- offers and the lobby

/// A started game (heroes chosen) whose lobby has / doesn't have `t`.
fn lobby_game(t: taverngym::types::Races, present: bool) -> Game {
    for seed in 0..200 {
        let mut g = Game::new(seed, RulesConfig::default());
        if g.lobby_races.intersects(t) != present {
            continue;
        }
        for p in 0..g.players.len() {
            if matches!(
                g.players[p].pending.last(),
                Some(Pending::HeroSelect { .. })
            ) {
                g.apply(p, Action::Choose(0)).unwrap();
            }
            g.players[p].pending.clear();
        }
        return g;
    }
    panic!("no such lobby in 200 seeds")
}

#[test]
fn minions_and_rewards_tied_to_a_type_need_it_in_the_lobby() {
    use taverngym::types::Races;
    let (nadina, robber) = (cid("BGS_040"), cid("BG28_303"));
    assert!(lobby_game(Races::DRAGON, true).universe.contains(&nadina));
    assert!(!lobby_game(Races::DRAGON, false).universe.contains(&nadina));
    assert!(lobby_game(Races::UNDEAD, true).universe.contains(&robber));
    assert!(!lobby_game(Races::UNDEAD, false).universe.contains(&robber));
    // Magicfin Relic gives Murlocs.
    let relic = "BG33_Reward_011";
    assert!(taverngym::content::quest_offerable(
        lobby_game(Races::MURLOC, true).lobby_races,
        relic
    ));
    assert!(!taverngym::content::quest_offerable(
        lobby_game(Races::MURLOC, false).lobby_races,
        relic
    ));
    // War Drum is a Dragon and Murloc trinket.
    assert_eq!(
        db().get(cid("BG32_MagicItem_416")).assoc,
        Races::DRAGON.union(Races::MURLOC)
    );
}

#[test]
fn hero_power_and_buddy_discovers_follow_the_lobby() {
    use taverngym::types::Races;
    let patches = cid("TB_BaconShop_HP_072"); // "Get a Pirate."
    let offer = |g: &mut Game| g.with_recruit(0, |c| c.offer_hero_powers(0, 500, 0, &[]));
    let mut g = lobby_game(Races::PIRATE, true);
    g.turn = 0;
    assert!(offer(&mut g).contains(&patches));
    let mut g = lobby_game(Races::PIRATE, false);
    g.turn = 0;
    assert!(!offer(&mut g).contains(&patches));
    // In an Aberration lobby the powers of the banned heroes are not offered.
    let mut g = lobby_game(Races::ABERRATION, true);
    g.turn = 0;
    let o = offer(&mut g);
    for hp in ["TB_BaconShop_HP_053", "BG23_HERO_306p"] {
        assert!(!o.contains(&cid(hp)), "{hp}");
    }
    // Buddies: scripted ones of the lobby's heroes only.
    let g = lobby_game(Races::PIRATE, false);
    let buddies = taverngym::content::buddy_pool(&g);
    assert!(buddies.len() > 20);
    assert!(
        buddies
            .iter()
            .all(|b| taverngym::script::script(*b).is_some())
    );
    let sailor = db().get(cid("TB_BaconShop_HERO_18")).buddy.unwrap(); // Patches' Buddy
    assert!(!buddies.contains(&sailor));
    assert!(taverngym::content::buddy_pool(&lobby_game(Races::PIRATE, true)).contains(&sailor));
}

#[test]
fn timewarp_discovers_and_menagerie_cards_follow_the_visit_rules() {
    use taverngym::content::timewarp;
    for seed in 0..40 {
        let mut g = murloc_game(seed, 6, RulesConfig::default());
        let lobby = g.lobby_races;
        for (major, cost, minions) in [(false, 1, true), (false, 2, true), (true, 1, false)] {
            let cards = g.with_recruit(0, |c| {
                timewarp::discover_cards(c, 0, major, cost, 3, minions)
            });
            assert!(!cards.is_empty());
            for x in cards {
                let d = db().get(x);
                assert!(
                    d.tw_types.is_none() || d.tw_types.intersects(lobby),
                    "{}",
                    d.name
                );
                assert!(
                    taverngym::script::script(x).is_some(),
                    "{} has no script",
                    d.name
                );
                assert_eq!(d.cost, cost);
            }
        }
        // Five Murlocs are not a menagerie: no "minion of each type" card in the visit.
        for major in [false, true] {
            let offer = g.with_recruit(0, |c| timewarp::offer(c, 0, major));
            for (it, _) in offer {
                let id = db().get(it.card()).id.as_str();
                assert!(!["BG34_Giant_679", "BG34_Giant_205", "BG34_Giant_684"].contains(&id));
            }
        }
    }
}

#[test]
fn gold_plated_compass_minion_gives_a_triple_reward_when_played() {
    // The minion the Compass made Golden pays a Triple Reward when played, like a Golden made of
    // three copies (the card texts' reading). Only the next minion of the type is gilded. (The fixture's warband is five Murlocs:
    // the Compass's type.)
    let mut g = murloc_game(3, 6, RulesConfig::default());
    g.players[0].hand.clear();
    buy_trinket(&mut g, 0, "BG32_MagicItem_901");
    g.players[0].shop.clear();
    g.players[0].gold = 10;
    put_shop(&mut g, 0, "BG_UNG_073"); // Rockpool Hunter, a Murloc
    put_shop(&mut g, 0, "BG_UNG_073");
    g.apply(0, Action::Buy(0)).unwrap();
    g.apply(0, Action::Buy(0)).unwrap();
    let hand: Vec<(bool, bool)> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .map(|m| (m.golden, m.triple_reward))
        .collect();
    assert_eq!(hand, vec![(true, true), (false, false)]);
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    // Rockpool Hunter's Battlecry asks for a target first.
    while !g.players[0].pending.is_empty() {
        g.apply(0, Action::Choose(0)).unwrap();
    }
    let rewards = g.players[0]
        .hand
        .iter()
        .filter(|h| db().get(h.item.card()).id == "TB_BaconShop_Triples_01")
        .count();
    assert_eq!(
        rewards, 1,
        "a Triple Reward for the Compass's Golden minion"
    );
}

#[test]
fn compass_gives_minions_of_the_most_common_type() {
    use taverngym::types::Races;
    let mut g = murloc_game(3, 6, RulesConfig::default());
    g.players[0].hand.clear();
    buy_trinket(&mut g, 0, "BG30_MagicItem_426");
    let got = g.players[0]
        .hand
        .last()
        .and_then(|h| h.item.as_minion())
        .unwrap();
    assert!(got.races.intersects(Races::MURLOC), "{}", got.name());
    let v = taverngym::debug::view(&g, 0);
    assert_eq!(
        v["me"]["trinkets"][0]["text"],
        "Get a random Murloc. At the start of each turn, get another."
    );
    assert_eq!(
        v["me"]["trinkets"][0]["text_zh"],
        "随机获取一张鱼人牌。在每个回合开始时，再获取一张。"
    );
    // The most common type ignores types that are not in the lobby (two Demon / Naga
    // minions don't make Naga the answer).
    let mut g = lobby_game(Races::DEMON, true);
    set_board(&mut g, 0, &["BG31_330", "BG31_330"]);
    let seer = &g.players[0].board[0];
    assert!(seer.races.intersects(Races::NAGA) && seer.races.intersects(Races::DEMON));
    for _ in 0..10 {
        assert_eq!(
            g.with_recruit(0, |c| c.most_common_race(0)),
            Some(Races::DEMON)
        );
    }
}

#[test]
fn reward_minions_and_tier_seven_triple_rewards() {
    // Secret Culprit names Moira Bronzebeard, and she doubles Battlecries and Deathrattles.
    let mut g = game(404);
    let v = {
        use taverngym::debug::{DebugCmd, run_debug};
        run_debug(
            &mut g,
            DebugCmd::GiveReward {
                p: 0,
                card: "BG28_Reward_510".into(),
            },
        )
        .unwrap();
        taverngym::debug::view(&g, 0)
    };
    assert_eq!(
        v["me"]["rewards"][0]["text"],
        "Get a copy of Moira Bronzebeard from Tier 7."
    );
    assert!(hand_ids(&g, 0).contains(&"BG27_518".to_string()));
    use taverngym::script::{flag, script};
    let moira = script(cid("BG27_518")).unwrap();
    assert!(moira.has_flag(flag::EXTRA_BATTLECRY) && moira.has_flag(flag::EXTRA_DEATHRATTLE));
    // Lightfang Enforcer: a friendly minion of each type gets +4/+4 at the end of the turn.
    let mut g = game(405);
    set_board(&mut g, 0, &["BGS_009", "BG36_345"]);
    let typed = *g
        .universe
        .iter()
        .find(|c| db().get(**c).races.index().is_some() && db().get(**c).tier == 1)
        .unwrap();
    let m = g.with_recruit(0, |c| c.new_minion(0, typed));
    let before = (m.atk, m.hp);
    g.players[0].board.push(m);
    end_turn_all(&mut g);
    let after = g.players[0]
        .board
        .iter()
        .find(|m| m.card == typed)
        .map(|m| (m.atk, m.max_hp));
    if let Some(a) = after {
        assert!(
            a.0 >= before.0 + 4 && a.1 >= before.1 + 4,
            "{before:?} -> {a:?}"
        );
    }
    // A Tier 7 Triple Reward Discovers Tier 7 minions.
    let mut g = game(406);
    g.with_recruit(0, |c| {
        let item = Item::Spell {
            card: cid("TB_BaconShop_Triples_01"),
            data: 7,
        };
        c.add_to_hand(0, item);
    });
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Play { hand, pos: 0 }).unwrap();
    let o = discover_options(&g);
    assert!(!o.is_empty() && o.iter().all(|m| db().get(m.card).tier == 7));
}

fn shop_tiers(g: &Game) -> Vec<(bool, u8)> {
    g.players[0]
        .shop
        .iter()
        .map(|s| match &s.item {
            Item::Minion(m) => (true, db().get(m.card).tier),
            Item::Spell { card, .. } => (false, db().get(*card).tier),
        })
        .collect()
}

#[test]
fn avenge_only_counts_deaths_in_combat() {
    // Budding Greenthumb-like Avenge minion: any scripted Avenge minion of the pool.
    let avenger = db()
        .iter()
        .find(|d| {
            d.pools.minion
                && d.normal.is_none()
                && taverngym::script::script(d.cid).is_some_and(|s| s.avenge.is_some())
        })
        .map(|d| d.id.clone())
        .unwrap();
    let mut g = game(700);
    set_board(&mut g, 0, &[avenger.as_str(), "BG36_345"]);
    // A death in the Tavern doesn't advance Avenge.
    g.with_recruit(0, |c| {
        let e = c.boards[0][1].eid;
        c.destroy(e);
        c.resolve_deaths();
    });
    assert_eq!(g.players[0].board[0].avenge, 0);
    // A death in combat does.
    set_board(&mut g, 0, &[avenger.as_str(), "BG36_345"]);
    set_board(&mut g, 1, &["BG36_345"]);
    let mut c = combat_ctx(&mut g);
    let e = c.boards[0][1].eid;
    c.destroy(e);
    c.resolve_deaths();
    assert!(c.boards[0][0].avenge > 0 || !c.boards[0][0].alive());
}

#[test]
fn random_cards_stay_at_or_below_the_tavern_tier() {
    use taverngym::types::Races;
    for seed in 0..25 {
        // Gearfin's 1-Cost spells on a Tier 1 Tavern.
        let mut g = game(710 + seed);
        g.lobby_races = Races::ALL;
        g.players[0].tier = 1;
        g.players[0].hand.clear();
        set_board(&mut g, 0, &["BG36_764"]);
        end_turn_all(&mut g);
        for h in &g.players[0].hand {
            let d = db().get(h.item.card());
            assert!(
                d.tier <= 1 || d.kind != taverngym::cards::CardKind::TavernSpell,
                "{}",
                d.name
            );
        }
        // Ghastcoiler on a Tier 3 Tavern summons Tier 3 or lower.
        let mut g = game(710 + seed);
        g.players[0].tier = 3;
        set_board(&mut g, 0, &["BGS_008"]);
        set_board(&mut g, 1, &["BG36_345"]);
        let mut c = combat_ctx(&mut g);
        let e = c.boards[0][0].eid;
        c.destroy(e);
        c.resolve_deaths();
        assert!(!c.boards[0].is_empty());
        for m in &c.boards[0] {
            assert!(db().get(m.card).tier <= 3, "{}", m.name());
        }
    }
}

#[test]
fn tavern_cards_bob_blehead_tier_seven_spells_evolving_tavern() {
    // Bob-blehead: no Tier 1-2 card in the Tavern, spells and extra offers included.
    for seed in 0..15 {
        let mut g = game(730 + seed);
        g.players[0].tier = 4;
        buy_trinket(&mut g, 0, "BG30_MagicItem_998");
        g.players[0].gold = 10;
        g.apply(0, Action::Refresh).unwrap();
        g.with_recruit(0, |c| {
            c.add_shop_minion(0, 1, 4, taverngym::types::Races::NONE);
        });
        assert!(
            shop_tiers(&g).iter().all(|x| x.1 >= 3),
            "{:?}",
            shop_tiers(&g)
        );
    }
    // Tier 7 spells are in the pool; a Tier 6 Tavern never shows one, a Tier 7 Tavern can.
    let t7 = cid("BG28_507"); // Sacred Gift
    let mut seen = false;
    for seed in 0..40 {
        let mut g = game(760 + seed);
        assert!(g.spell_universe.contains(&t7));
        g.players[0].tier = 6;
        g.players[0].gold = 10;
        g.apply(0, Action::Refresh).unwrap();
        assert!(shop_tiers(&g).iter().all(|x| x.1 <= 6));
        g.players[0].tier7 = true;
        g.players[0].tier = 7;
        for _ in 0..4 {
            g.players[0].gold = 10;
            g.apply(0, Action::Refresh).unwrap();
            seen |= shop_tiers(&g).iter().any(|x| !x.0 && x.1 == 7);
        }
    }
    assert!(seen, "a Tier 7 Tavern offers Tier 7 spells");
    // Evolving Tavern: the Tavern spell goes up a Tier too.
    let mut g = game(790);
    g.players[0].tier = 3;
    g.players[0].shop.clear();
    put_shop(&mut g, 0, "BG36_345"); // Tier 1 minion
    let sid = g.alloc_eid();
    g.players[0].shop.push(taverngym::state::ShopSlot::new(
        sid,
        Item::spell(cid("BG28_897")),
    )); // Tier 1
    give(&mut g, 0, "BGS_Treasures_006");
    play_last(&mut g, 0);
    assert_eq!(shop_tiers(&g), vec![(true, 2), (false, 2)]);
}

#[test]
fn toki_adds_two_minions_and_tess_shows_only_the_warband() {
    // Infinite Toki: the Refresh plus two minions of the next Tier.
    let mut g = game(800);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_028"));
    g.players[0].tier = 2;
    g.players[0].gold = 10;
    g.apply(0, Action::Refresh).unwrap();
    let plain = shop_tiers(&g).iter().filter(|x| x.0).count();
    g.apply(0, Action::HeroPower(0)).unwrap();
    let t = shop_tiers(&g);
    assert_eq!(t.iter().filter(|x| x.0).count(), plain + 2, "{t:?}");
    assert_eq!(t.iter().filter(|x| x.0 && x.1 == 3).count(), 2, "{t:?}");
    // Tess: the last opponent's warband is the whole Refresh.
    let mut g = game(801);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_077"));
    g.players[0].gold = 10;
    g.players[0].last_opponent = Some(1);
    set_board(&mut g, 1, &["BG36_345", "BG19_010"]);
    g.players[1].seen_board = g.players[1].board.clone();
    g.apply(0, Action::HeroPower(0)).unwrap();
    let cards: Vec<_> = g.players[0].shop.iter().map(|s| s.item.card()).collect();
    assert_eq!(cards, vec![cid("BG36_345"), cid("BG19_010")]);
}

#[test]
fn warband_whistle_waits_for_the_refresh_button() {
    let mut g = game(810);
    set_board(&mut g, 0, &["BG36_345", "BG19_010"]);
    let free = g.players[0].free_refreshes;
    let before: Vec<_> = g.players[0].shop.iter().map(|s| s.sid).collect();
    buy_trinket(&mut g, 0, "BG35_MagicItem_930");
    assert_eq!(g.players[0].free_refreshes, free + 1, "a free Refresh");
    let now: Vec<_> = g.players[0].shop.iter().map(|s| s.sid).collect();
    assert_eq!(now, before, "nothing is refreshed by buying it");
    g.apply(0, Action::Refresh).unwrap();
    let mut got: Vec<_> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| m.card)
        .collect();
    got.sort();
    let mut want = [cid("BG36_345"), cid("BG19_010")];
    want.sort();
    assert!(want.iter().all(|c| got.contains(c)), "{got:?}");
    // Only once.
    g.players[0].gold = 10;
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(g.players[0].free_refreshes, free);
}

#[test]
fn glowing_gauntlet_keeps_seven_cards_in_the_tavern() {
    let mut g = game(820);
    g.players[0].gold = 30;
    buy_trinket(&mut g, 0, "BG30_MagicItem_841");
    assert_eq!(g.players[0].shop.len(), 7);
    // Buy a minion: a minion comes in. Buy the spell: a spell comes in.
    let i = g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_some())
        .unwrap();
    g.apply(0, Action::Buy(i)).unwrap();
    assert_eq!(g.players[0].shop.len(), 7);
    assert_eq!(shop_tiers(&g).iter().filter(|x| !x.0).count(), 1);
    let i = g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_none())
        .unwrap();
    g.apply(0, Action::Buy(i)).unwrap();
    assert_eq!(g.players[0].shop.len(), 7);
    assert_eq!(shop_tiers(&g).iter().filter(|x| !x.0).count(), 1);
}

#[test]
fn stein_lockbox_kodo_and_other_card_rules() {
    use taverngym::types::Races;
    // Innkeeper's Stein on Tier 6: an extra Tier 6 minion.
    let mut g = game(830);
    g.players[0].tier = 6;
    buy_trinket(&mut g, 0, "BG30_MagicItem_423");
    g.players[0].gold = 10;
    g.apply(0, Action::Refresh).unwrap();
    assert!(shop_tiers(&g).iter().any(|x| x.0 && x.1 == 6));
    // Lockbox: the Golden minion gives its Triple Reward.
    let mut g = game(831);
    g.players[0].hand.clear();
    g.with_recruit(0, |c| taverngym::content::open_lockbox(c, 0));
    let m = g.players[0].hand[0].item.as_minion().unwrap();
    assert!(m.golden && m.triple_reward);
    // Baby Kodo on Tier 2: one minion of each Tier 1-6.
    let mut g = game(832);
    g.players[0].tier = 2;
    give(&mut g, 0, "BG20_HERO_242_Buddy");
    play_last(&mut g, 0);
    let mut tiers: Vec<u8> = shop_tiers(&g).iter().filter(|x| x.0).map(|x| x.1).collect();
    tiers.sort();
    assert_eq!(tiers, vec![1, 2, 3, 4, 5, 6]);
    // Cookie: only minions with a type go into the pot.
    let mut g = game(833);
    g.set_hero_power(0, 0, cid("BG21_HERO_020p"));
    g.players[0].shop.clear();
    set_board(&mut g, 0, &["BG36_345"]); // typeless
    assert!(g.apply(0, Action::HeroPower(0)).is_err() || g.players[0].pending.is_empty());
    // Candle dragon: an eliminated opponent's last warband counts.
    let mut g = game(834);
    for p in 1..8 {
        g.players[p].board.clear();
        g.players[p].last_board.clear();
    }
    let six = *g.universe.iter().find(|c| db().get(**c).tier == 6).unwrap();
    let m = g.with_recruit(0, |c| c.new_minion(0, six));
    g.players[3].alive = false;
    g.players[3].last_board = vec![m];
    give(&mut g, 0, "BG23_HERO_305_Buddy");
    play_last(&mut g, 0);
    assert!(g.players[0].shop.iter().any(|s| s.item.card() == six));
    // Gem Confiscation on a Tavern minion takes the gems of its Tavern neighbours.
    let mut g = game(835);
    g.lobby_races = Races::ALL;
    g.players[0].shop.clear();
    for _ in 0..3 {
        put_shop(&mut g, 0, "BG36_345");
    }
    let sids: Vec<_> = g.players[0].shop.iter().map(|s| s.sid).collect();
    g.with_recruit(0, |c| {
        c.play_gem(0, sids[0]);
        c.play_gem(0, sids[2]);
    });
    give(&mut g, 0, "BG28_698");
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Play { hand, pos: 0 }).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
        panic!("target")
    };
    let i = options.iter().position(|e| *e == sids[1]).unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
    let gems: Vec<i32> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .map(|m| m.gems as i32)
        .collect();
    assert_eq!(gems, vec![0, 5, 0], "3 played + 1 + 1 stolen");
}

#[test]
fn raynor_brukan_phylactery_and_the_dragonhead_staff() {
    // Jim Raynor: the upgrade is the variant of exactly the Tavern Tier.
    let mut g = game(840);
    g.players[0].tier = 3;
    for _ in 0..20 {
        let u = g
            .with_recruit(0, |c| taverngym::content::starcraft::random_upgrade(c, 0))
            .unwrap();
        assert_eq!(db().get(u).tier, 3, "{}", db().get(u).id);
    }
    // Bru'kan: the chosen Element is used up by the next combat.
    let mut g = game(841);
    g.set_hero_power(0, 0, cid("BG22_HERO_001p"));
    g.players[0].powers[0].counter = 1;
    set_board(&mut g, 0, &["BG36_345"]);
    set_board(&mut g, 1, &["BG36_345"]);
    g.simulate_combat(0, Some(1), None);
    assert_eq!(g.players[0].powers[0].counter, 0);
    // Deathly Phylactery: one Deathrattle effect triggers an extra time, not every
    // Deathrattle of the first minion.
    let mut g = game(842);
    buy_trinket(&mut g, 0, "BG30_MagicItem_700");
    g.players[0].pending.clear();
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG34_Giant_204"]); // Timewarped Pillager: Deathrattle, get a Coin
    g.players[0].board[0].gained_dr.push(cid("BG34_Giant_204"));
    g.players[0].board[0].kw.clear(Kw::REBORN);
    set_board(&mut g, 1, &["BG36_345"]);
    let mut c = combat_ctx(&mut g);
    let e = c.boards[0][0].eid;
    c.destroy(e);
    c.resolve_deaths();
    drop(c);
    assert_eq!(
        g.players[0].hand.len(),
        3,
        "2 Deathrattles + 1 extra trigger of the first"
    );
    // Dragonhead Staff: 13 attacks in combat arm it; it is replaced at the start of the turn.
    let mut g = game(843);
    buy_trinket(&mut g, 0, "BG36_MagicItem_407");
    let staff = cid("BG36_MagicItem_407");
    set_board(&mut g, 0, &["BG36_345"]);
    set_board(&mut g, 1, &["BG36_345"]);
    {
        let mut c = combat_ctx(&mut g);
        let (a, d) = (c.boards[0][0].eid, c.boards[1][0].eid);
        for _ in 0..13 {
            c.emit(taverngym::event::Event::Attacks {
                side: 0,
                attacker: a,
                defender: d,
            });
        }
    }
    assert!(
        g.players[0].trinkets.contains(&staff),
        "not replaced in combat"
    );
    end_turn_all(&mut g);
    assert!(
        !g.players[0].trinkets.contains(&staff),
        "replaced at the start of the turn"
    );
    assert_eq!(g.players[0].trinkets.len(), 1);
}

#[test]
fn souvenir_stand_copy_has_its_own_state_and_repeats_the_immediate_effect() {
    // Greater Timeworn Candelabra ("Discover a 2-Cost minion from the Major Timewarp"): with
    // the Stand the Discover happens twice.
    let mut g = murloc_game(3, 9, RulesConfig::default());
    buy_trinket(&mut g, 0, "BG30_MagicItem_888");
    g.players[0].pending.clear();
    buy_trinket(&mut g, 0, "BG35_MagicItem_823t");
    let discovers = g.players[0]
        .pending
        .iter()
        .filter(|p| matches!(p, Pending::Discover { .. }))
        .count();
    assert_eq!(discovers, 2);
    // The copy keeps its own counter.
    let mut g = game(851);
    buy_trinket(&mut g, 0, "BG30_MagicItem_888");
    buy_trinket(&mut g, 0, "BG36_MagicItem_407");
    let staff = cid("BG36_MagicItem_407");
    g.with_recruit(0, |c| {
        c.set_card_data(0, staff, 5);
        assert_eq!(c.card_data(0, staff.copy()), 0);
    });
    assert!(g.players[0].has_trinket("BG36_MagicItem_407"));
    assert!(g.players[0].has_trinket_id(staff));
}

#[test]
fn voone_copies_a_triple_reward_with_its_tier() {
    // Rock Master Voone's plain copy of a Triple Reward Discovers from the same Tier: the Tier
    // is the card, not an enchantment.
    let reward = cid("TB_BaconShop_Triples_01");
    let mut g = game(860);
    g.set_hero_power(0, 0, cid("BG26_HERO_104p"));
    g.players[0].pending.clear();
    g.players[0].hand.clear();
    g.players[0].powers[0].counter = 2; // the third turn ends now
    g.with_recruit(0, |c| {
        c.add_to_hand(
            0,
            Item::Spell {
                card: reward,
                data: 5,
            },
        );
    });
    g.apply(0, Action::EndTurn).unwrap();
    let tiers: Vec<i32> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| match h.item {
            Item::Spell { card, data } if card == reward => Some(data),
            _ => None,
        })
        .collect();
    assert_eq!(tiers, vec![5, 5]);
    // Any other card: the plain card, without what it gained.
    let mut g = game(861);
    g.set_hero_power(0, 0, cid("BG26_HERO_104p"));
    g.players[0].pending.clear();
    g.players[0].hand.clear();
    g.players[0].powers[0].counter = 2;
    give(&mut g, 0, "BG36_345");
    g.players[0].hand[0]
        .item
        .as_minion_mut()
        .unwrap()
        .set_stats(9, 9);
    g.apply(0, Action::EndTurn).unwrap();
    let hand: Vec<(i32, i32)> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .map(|m| (m.atk, m.hp))
        .collect();
    assert_eq!(hand, vec![(9, 9), (3, 3)]);
}

#[test]
fn portrait_minions_sell_get_and_copy_effects() {
    // Arcane Behemoth: after you sell an Elemental, gain its stats (golden: double).
    for (id, k) in [("BG31_360", 1), ("BG31_360_G", 2)] {
        let mut g = game(870);
        g.cfg.triple_copies = 99;
        set_board(&mut g, 0, &[id, "BGS_115", "BG36_345"]); // Behemoth, Sellemental, a 3/3
        g.players[0].board[1].set_stats(5, 6);
        let before = stats(&g, 0)[0];
        g.apply(0, Action::Sell(1)).unwrap();
        assert_eq!(stats(&g, 0)[0], (before.0 + 5 * k, before.1 + 6 * k));
        g.apply(0, Action::Sell(1)).unwrap(); // not an Elemental
        assert_eq!(stats(&g, 0), vec![(before.0 + 5 * k, before.1 + 6 * k)]);
    }
    // Doubloon Grifter: every 4th Pirate you get raises your maximum Gold by 1 (golden: 2).
    for (id, k) in [("BG31_826", 1), ("BG31_826_G", 2)] {
        let mut g = game(871);
        set_board(&mut g, 0, &[id]);
        for n in 1..=5 {
            give(&mut g, 0, "BG26_135"); // Southsea Busker, a Pirate
            g.players[0].hand.clear();
            assert_eq!(
                g.players[0].max_gold_bonus,
                if n >= 4 { k } else { 0 },
                "{n}"
            );
        }
        give(&mut g, 0, "BG36_345"); // not a Pirate
        assert_eq!(g.players[0].board[0].counter, 1);
        assert_eq!(g.players[0].max_gold_bonus, k);
    }
    // Faceless Manipulator: becomes a full copy of the chosen friendly minion...
    let mut g = game(872);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG36_345"]);
    g.players[0].board[0].set_stats(9, 8);
    let target = g.players[0].board[0].eid;
    give(&mut g, 0, "BG_EX1_564");
    play_last(&mut g, 0);
    choose_eid(&mut g, 0, target);
    let b = &g.players[0].board;
    assert_eq!(b.len(), 2);
    assert!(b[1].is_card("BG36_345") && b[1].eid != target);
    assert_eq!(stats(&g, 0), vec![(9, 8), (9, 8)]);
    // ...or of a Tavern minion (the text has no "friendly"), which stays in the Tavern.
    let mut g = game(873);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &[]);
    g.players[0].shop.clear();
    let i = put_shop(&mut g, 0, "BG36_345");
    g.players[0].shop[i]
        .item
        .as_minion_mut()
        .unwrap()
        .set_stats(4, 7);
    let sid = g.players[0].shop[i].sid;
    give(&mut g, 0, "BG_EX1_564");
    play_last(&mut g, 0);
    choose_eid(&mut g, 0, sid);
    assert!(g.players[0].board[0].is_card("BG36_345"));
    assert_eq!(stats(&g, 0), vec![(4, 7)]);
    assert_eq!(g.players[0].shop.len(), 1);
}

#[test]
fn portrait_minions_in_combat() {
    // Bassgill: Deathrattle: the highest-Health Murloc in hand (golden: the 2 highest) joins
    // for this combat only; other minions in hand don't count.
    for (id, n) in [("BG26_350", 1), ("BG26_350_G", 2)] {
        let mut g = game(880);
        g.cfg.triple_copies = 99;
        set_board(&mut g, 0, &[id]);
        g.players[0].hand.clear();
        give(&mut g, 0, "BG34_Giant_206"); // Timewarped Murky 5/5 Murloc
        give(&mut g, 0, "BG34_Giant_321"); // Timewarped Mrrrglr 8/8 Murloc
        give(&mut g, 0, "BG35_155"); // Twisted Wrathguard 8/8, not a Murloc
        g.players[0].hand[2]
            .item
            .as_minion_mut()
            .unwrap()
            .set_stats(8, 30);
        let board: Vec<(i32, i32, bool)> = {
            let mut c = combat_ctx(&mut g);
            let e = c.boards[0][0].eid;
            c.destroy(e);
            c.resolve_deaths();
            c.boards[0]
                .iter()
                .map(|m| (m.atk, m.hp, m.combat_only))
                .collect()
        };
        let expect = [(8, 8, true), (5, 5, true)];
        assert_eq!(board, expect[..n].to_vec());
        assert_eq!(g.players[0].hand.len(), 3, "the cards stay in hand");
    }
    // Tide Oracle Morgl: when it attacks and kills, a hand minion gains the victim's stats
    // (golden: double); a kill while defending doesn't count.
    for (id, k) in [("BG27_513", 1), ("BG27_513_G", 2)] {
        for morgl_attacks in [true, false] {
            let mut g = game(881);
            set_board(&mut g, 0, &[id]);
            set_board(&mut g, 1, &["BG36_345"]);
            g.players[1].board[0].set_stats(7, 9);
            g.players[0].hand.clear();
            give(&mut g, 0, "BG36_345"); // 3/3 in hand
            {
                let mut c = combat_ctx(&mut g);
                let side = usize::from(!morgl_attacks);
                let eid = c.boards[side][0].eid;
                taverngym::combat::attack_now(&mut c, side, eid);
                assert!(c.boards[1].is_empty(), "Poisonous");
            }
            let m = g.players[0].hand[0].item.as_minion().unwrap();
            let gain = if morgl_attacks { k } else { 0 };
            assert_eq!((m.atk, m.hp), (3 + 7 * gain, 3 + 9 * gain));
        }
    }
    // Timewarped Poet: all your Dragons keep the stats they gain in combat (golden: double),
    // adjacent or not.
    for (id, k) in [("BG34_Giant_314", 1), ("BG34_Giant_314_G", 2)] {
        let mut g = game(882);
        set_board(&mut g, 0, &[id, "BG36_345", "BG34_636t"]); // Poet, a 3/3, a 3/5 Dragon
        set_board(&mut g, 1, &["BG36_345"]);
        {
            let mut c = combat_ctx(&mut g);
            let (plain, drake) = (c.boards[0][1].eid, c.boards[0][2].eid);
            c.buff(plain, 2, 2);
            c.buff(drake, 2, 3);
        }
        assert_eq!(stats(&g, 0)[1], (3, 3), "not a Dragon");
        assert_eq!(stats(&g, 0)[2], (3 + 2 * k, 5 + 3 * k));
    }
}

#[test]
fn boom_monster_counts_magnetizations_and_is_a_fixed_enchantment_once_attached() {
    let mut g = game(890);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG26_146"]); // Lullabot 2/2 Mech (host)
    give(&mut g, 0, "BG31_176"); // Dr. Boom's Monster 2/2
    give(&mut g, 0, "BG_BOT_911"); // Annoy-o-Module 2/4 Magnetic
    g.apply(0, Action::Magnetize { hand: 1, board: 0 }).unwrap();
    assert_eq!(g.players[0].stats.magnetized, 1);
    let monster = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((monster.atk, monster.hp), (4, 4), "+2/+2 wherever it is");
    // Attached: the host gains the Monster as it is now (2/2 + one Magnetization).
    g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
    assert_eq!(stats(&g, 0), vec![(2 + 2 + 4, 2 + 4 + 4)]);
    assert_eq!(g.players[0].stats.magnetized, 2);
    // Later Monsters count both Magnetizations (golden: +4/+4 each); the host is unchanged.
    give(&mut g, 0, "BG31_176");
    give(&mut g, 0, "BG31_176_G");
    let hand: Vec<(i32, i32)> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .map(|m| (m.atk, m.hp))
        .collect();
    assert_eq!(hand, vec![(2 + 4, 2 + 4), (4 + 8, 4 + 8)]);
    assert_eq!(stats(&g, 0), vec![(8, 10)]);
}

fn triple_rewards(g: &Game, p: PlayerId) -> usize {
    g.players[p]
        .hand
        .iter()
        .filter(|h| db().get(h.item.card()).id == "TB_BaconShop_Triples_01")
        .count()
}

#[test]
fn golden_touch_minion_bought_and_played_gives_a_triple_reward() {
    let mut g = game(8101);
    g.players[0].gold = 20;
    g.players[0].shop.clear();
    let m = minion(&mut g, 0, "BG36_345");
    g.players[0]
        .shop
        .push(taverngym::state::ShopSlot::new(m.eid, Item::Minion(m)));
    give(&mut g, 0, "BG28_830");
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert!(g.players[0].shop[0].item.as_minion().unwrap().golden);
    g.apply(0, Action::Buy(0)).unwrap();
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert!(g.players[0].board[0].golden);
    assert_eq!(triple_rewards(&g, 0), 1);
}

#[test]
fn eudora_dug_golden_gives_a_triple_reward() {
    let mut g = game(8102);
    g.players[0].set_hero_power(0, cid("TB_BaconShop_HP_074"));
    for _ in 0..4 {
        g.players[0].gold = 10;
        g.players[0].powers[0].uses_turn = 0;
        g.apply(0, Action::HeroPower(0)).unwrap();
    }
    let dug = g.players[0].hand[0].item.as_minion().unwrap();
    assert!(dug.golden && dug.triple_reward);
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    while !g.players[0].pending.is_empty() {
        g.apply(0, Action::Choose(0)).unwrap();
    }
    assert_eq!(triple_rewards(&g, 0), 1);
}

#[test]
fn copies_of_golden_minions_give_a_triple_reward_when_played() {
    // Every Golden minion gives its Triple Reward when played unless a text says it doesn't; a copy
    // of a Golden minion is no exception.
    let mut g = game(8103);
    let hid = give(&mut g, 0, "BG36_345_G");
    g.with_recruit(0, |c| {
        c.give_copy_of_hand(0, hid);
    });
    for n in 1..=2 {
        let hand = g.players[0]
            .hand
            .iter()
            .position(|h| h.item.as_minion().is_some())
            .unwrap();
        g.apply(0, Action::Play { hand, pos: 0 }).unwrap();
        assert_eq!(triple_rewards(&g, 0), n);
    }
    // Time Thief: a Golden minion Discovered from the last opponent's warband.
    let mut g = game(8104);
    g.players[1].seen_board = vec![minion(&mut g, 1, "BG36_345_G")];
    g.players[0].last_opponent = Some(1);
    give(&mut g, 0, "BGS_Treasures_010");
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert_eq!(triple_rewards(&g, 0), 1);
    // A copy of a Golden minion whose text denies the reward gives none either.
    let mut g = game(8105);
    let hid = give(&mut g, 0, "BG32_236");
    g.with_recruit(0, |c| {
        c.give_copy_of_hand(0, hid);
    });
    for _ in 0..2 {
        g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    }
    assert_eq!(g.players[0].board.len(), 2);
    assert_eq!(triple_rewards(&g, 0), 0);
}

fn hand_ids_of(g: &Game, p: PlayerId) -> Vec<String> {
    g.players[p]
        .hand
        .iter()
        .map(|h| db().get(h.item.card()).id.clone())
        .collect()
}

#[test]
fn a_minions_random_or_discovered_minion_is_never_itself() {
    // The universe is cut down to the minion itself (nothing can come) or the minion plus one other
    // card of its type (always the other one). Tad: "When you sell this, get a random Murloc."
    let mut g = game(8201);
    g.players[0].tier = 2;
    g.universe = vec![cid("BG22_202")];
    set_board(&mut g, 0, &["BG22_202"]);
    g.apply(0, Action::Sell(0)).unwrap();
    assert!(hand_ids_of(&g, 0).is_empty(), "a sold Tad gives no Tad");
    g.universe = vec![cid("BG22_202"), cid("BG29_300")];
    for _ in 0..2 {
        set_board(&mut g, 0, &["BG22_202"]);
        g.apply(0, Action::Sell(0)).unwrap();
    }
    assert_eq!(hand_ids_of(&g, 0), vec!["BG29_300"; 2]);

    // Primalfin Lookout: "Battlecry: If you control another Murloc, Discover a Murloc."
    let mut g = game(8202);
    g.players[0].tier = 5;
    g.universe = vec![cid("BGS_020"), cid("BG29_300")];
    for c in g.universe.clone() {
        g.pool.put(c, 5);
    }
    set_board(&mut g, 0, &["BG29_300"]);
    give(&mut g, 0, "BGS_020");
    g.apply(0, Action::Play { hand: 0, pos: 1 }).unwrap();
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!("expected a Discover")
    };
    let ids: Vec<&str> = options
        .iter()
        .map(|o| db().get(o.card()).id.as_str())
        .collect();
    assert_eq!(ids, vec!["BG29_300"]);

    // Snare Trapper: "Choose One - Get a random Quilboar; or...".
    let mut g = game(8203);
    g.players[0].tier = 4;
    g.universe = vec![cid("BG36_332"), cid("BG20_101")];
    for _ in 0..2 {
        give(&mut g, 0, "BG36_332");
        let (hand, pos) = (g.players[0].hand.len() - 1, g.players[0].board.len());
        g.apply(0, Action::Play { hand, pos }).unwrap();
        g.apply(0, Action::Choose(0)).unwrap();
    }
    assert_eq!(hand_ids_of(&g, 0), vec!["BG20_101"; 2]);

    // A Deathrattle (Kil'rek: "Get a random Demon") and a Rally (Headhunter Gryphon: "Get a
    // random Beast") in combat.
    let mut g = game(8204);
    g.universe = vec![
        cid("TB_BaconShop_HERO_37_Buddy"),
        cid("BGS_004"),
        cid("BG36_204"),
        cid("BG31_803"),
    ];
    g.players[0].tier = 4;
    set_board(&mut g, 0, &["TB_BaconShop_HERO_37_Buddy", "BG36_204"]);
    set_board(&mut g, 1, &["BG36_345", "BG36_345", "BG36_345"]);
    for m in &mut g.players[1].board {
        m.set_stats(50, 50);
    }
    fight(&mut g);
    let got = hand_ids_of(&g, 0);
    assert!(!got.is_empty());
    assert!(
        got.iter().all(|id| id == "BGS_004" || id == "BG31_803"),
        "{got:?}"
    );
}

#[test]
fn a_spell_can_discover_itself() {
    // Boundless Potential (Tier 4): "Choose One - Discover a minion of your Tier; or a Tavern
    // spell of your Tier": the limit on minions does not apply to spells.
    let mut g = game(8205);
    g.players[0].tier = 4;
    g.spell_universe = vec![cid("BG31_890")];
    give(&mut g, 0, "BG31_890");
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    g.apply(0, Action::Choose(1)).unwrap();
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!("expected a Discover")
    };
    assert_eq!(db().get(options[0].card()).id, "BG31_890");
}

#[test]
fn spells_and_tavern_spells_are_told_apart() {
    // Blood Gems and Triple Rewards are spells but not Tavern spells; a Tavern Coin is both.
    let mut g = game(8301);
    // Timecap'n Hooktail: "Whenever you cast a Tavern spell, give your minions +1 Attack."
    set_board(&mut g, 0, &["BG27_005"]);
    let atk = g.players[0].board[0].atk;
    let counts = |g: &Game| {
        let st = &g.players[0].stats;
        (st.spells_cast, st.tavern_spells_cast)
    };
    give(&mut g, 0, "BG20_GEM");
    cast_on(&mut g, 0, 0);
    assert_eq!(counts(&g), (1, 0));
    assert_eq!(g.players[0].board[0].atk, atk + 1, "only the gem's +1");
    g.with_recruit(0, |c| taverngym::content::give_triple_reward(c, 0));
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    while !g.players[0].pending.is_empty() {
        g.apply(0, Action::Choose(0)).unwrap();
    }
    assert_eq!(counts(&g), (2, 0));
    assert_eq!(g.players[0].board[0].atk, atk + 1);
    let hand = g.players[0].hand.len();
    give(&mut g, 0, "BG28_810");
    g.apply(0, Action::Play { hand, pos: 0 }).unwrap();
    assert_eq!(counts(&g), (3, 1));
    assert_eq!(
        g.players[0].board[0].atk,
        atk + 2,
        "Hooktail saw the Tavern Coin"
    );

    // Replica Cathedral: "Your first spell each turn casts an extra time" -- any spell.
    let mut g = game(8302);
    g.players[0].trinkets.push(cid("BG30_MagicItem_434"));
    set_board(&mut g, 0, &["BG36_345"]);
    let (a, h) = stats(&g, 0)[0];
    give(&mut g, 0, "BG20_GEM");
    cast_on(&mut g, 0, 0);
    assert_eq!(stats(&g, 0), vec![(a + 2, h + 2)]);
    assert_eq!(counts(&g), (2, 0));
    // And a Triple Reward the next turn: two Discovers.
    g.players[0].stats.spells_cast_turn = 0;
    g.with_recruit(0, |c| taverngym::content::give_triple_reward(c, 0));
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    let discovers = g.players[0]
        .pending
        .iter()
        .filter(|p| matches!(p, Pending::Discover { .. }))
        .count();
    assert_eq!(discovers, 2);
}

#[test]
fn triples_wait_for_the_discovers_to_finish() {
    // Brann + Imposing Percussionist: two Discovers in a row. The first pick is the third Ashen
    // Corruptor, but the triple (which takes the two rewinders off the board) only happens after
    // both Discovers and their damage.
    let mut g = game(8501);
    g.players[0].tier = 5;
    g.players[0].health = 10;
    g.players[0].armor = 0;
    g.universe = vec![cid("BG32_873")];
    g.pool.put(cid("BG32_873"), 10);
    set_board(&mut g, 0, &["BG_LOE_077", "BG32_873", "BG32_873"]);
    give(&mut g, 0, "BG26_525");
    g.apply(0, Action::Play { hand: 0, pos: 3 }).unwrap();
    let discovers = |g: &Game| {
        g.players[0]
            .pending
            .iter()
            .filter(|p| matches!(p, Pending::Discover { .. }))
            .count()
    };
    assert_eq!(discovers(&g), 2);
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(
        g.players[0].health, 10,
        "the first pick's damage is rewound"
    );
    assert_eq!(hand_ids(&g, 0), vec!["BG32_873"], "no triple yet");
    assert_eq!(g.players[0].board.len(), 4);
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].health, 10, "and so is the second pick's");
    assert!(g.players[0].alive);
    // Now the triple.
    assert!(hand_ids(&g, 0).contains(&"BG32_873_G".to_string()));
}

#[test]
fn xavius_presses_the_dark_gift_button_and_no_gift_discover_offers_the_paradox() {
    use taverngym::types::Races;
    // Xavius's Hero Power is a free press of the button, with its places (the client gave 4/5/5 on
    // turn 8, 5/5/6 and 5/5/5 on 12).
    let typed = |m: &Minion| taverngym::content::goes_with_type(db().get(m.card), Races::MURLOC);
    for seed in 0..40 {
        // (On the game's Turns 8 and 12)
        for (turn, least) in [(7, 1), (11, 2)] {
            let mut g = murloc_game(seed, turn, RulesConfig::default());
            let hp = cid("BG36_HERO_105p");
            g.set_hero_power(0, 0, hp);
            let (gold, uses) = (g.players[0].gold, g.players[0].dark_gift_uses);
            end_turn_all(&mut g);
            assert_eq!(g.turn, turn + 1);
            let o = discover_options(&g);
            assert_eq!(o.len(), 3, "seed {seed}");
            assert!(o[..least].iter().all(typed), "seed {seed} turn {turn}");
            let tiers: Vec<u8> = o.iter().map(|m| db().get(m.card).tier).collect();
            if turn + 1 == 8 {
                assert_eq!(tiers, [4, 5, 5], "seed {seed}");
            } else {
                assert!(tiers[0] == 5 && tiers.iter().all(|t| [5, 6].contains(t)));
            }
            assert_eq!(g.players[0].dark_gift_uses, uses, "no use of the button");
            let _ = gold;
        }
    }
    // Dark Paradox has a Dark Gift of its own: no gifted Discover offers it, but Maldraxxus
    // Dagger can copy a friendly one (its gifts stack).
    let mut forms = 0;
    for d in db().iter().filter(|d| d.id.starts_with("BG36_360")) {
        assert!(!taverngym::recruit::gift_discoverable(d, 10), "{}", d.id);
        forms += usize::from(taverngym::recruit::gift_compatible(d, 10));
    }
    assert!(forms > 0);
    let mut g = game(8520);
    g.turn = 10;
    g.players[0].tier = 6;
    set_board(&mut g, 0, &["BG36_360t9"]);
    g.with_recruit(0, |c| {
        taverngym::content::grant_trinket(c, 0, cid("BG36_MagicItem_370"))
    });
    let o = discover_options(&g);
    assert!(!o.is_empty());
    assert!(
        o.iter()
            .all(|m| db().get(m.card).id == "BG36_360t9" && !m.attached.is_empty())
    );
}

#[test]
fn trinket_offers_follow_the_reference_rules() {
    use taverngym::cards::CardKind;
    use taverngym::content::trinket_offers::{DISCOUNTABLE, MENAGERIE, types};
    use taverngym::types::Races;
    let id = |t: CardId| db().get(t).id.as_str();
    // An empty board and hand: every typed trinket offered is a discounted pivot.
    let (mut pivots, mut most) = (0, 0);
    for seed in 0..60 {
        let mut g = game(8600 + seed);
        let offer = g.trinket_offer(0, CardKind::TrinketLesser);
        for t in &offer {
            if !types(*t).is_none() {
                assert!(
                    g.players[0].trinket_pivots.contains(t),
                    "seed {seed}: {}",
                    id(*t)
                );
                assert!(DISCOUNTABLE.contains(&id(*t)));
                assert_eq!(g.trinket_price(0, *t), (db().get(*t).cost - 2).max(0));
            }
            // No menagerie trinket without three types.
            assert!(!MENAGERIE.contains(&id(*t)) || DISCOUNTABLE.contains(&id(*t)));
        }
        assert!(offer.iter().any(|t| g.trinket_price(0, *t) <= 2));
        pivots = pivots.max(g.players[0].trinket_pivots.len());
        // Only the board counts (client offers). One Dragon on board and one in hand: you are not
        // in Dragons, any Dragon trinket is a pivot.
        let mut g = game(8600 + seed);
        set_board(&mut g, 0, &["BG32_823"]);
        give(&mut g, 0, "BG26_529");
        let offer = g.trinket_offer(0, CardKind::TrinketLesser);
        assert!(
            !offer
                .iter()
                .any(|t| types(*t).intersects(Races::DRAGON)
                    && !g.players[0].trinket_pivots.contains(t)),
            "seed {seed}"
        );
        // Two Dragons on board: you are in Dragons, and a Dragon trinket at full price is
        // always there (when the lobby has Dragon trinkets at all).
        let mut g = game(8600 + seed);
        set_board(&mut g, 0, &["BG32_823", "BG26_529"]);
        // (Only a type of the lobby counts: a two-type trinket can be in the universe for
        // its other type.)
        let has = g.lobby_races.intersects(Races::DRAGON)
            && g.trinket_universe.clone().into_iter().any(|t| {
                db().get(t).kind == CardKind::TrinketLesser
                    && types(t).intersects(Races::DRAGON)
                    && g.trinket_eligible(0, t)
            });
        let offer = g.trinket_offer(0, CardKind::TrinketLesser);
        let dragon = offer.iter().any(|t| {
            types(*t).intersects(Races::DRAGON) && !g.players[0].trinket_pivots.contains(t)
        });
        assert_eq!(dragon, has, "seed {seed}");
        most += usize::from(dragon);
    }
    assert!(pivots >= 2, "pivots of different types can come together");
    assert!(most >= 20);
    // Per-card conditions.
    let mut g = game(8700);
    let (sous, ghastly) = (cid("BG35_MagicItem_801"), cid("BG32_MagicItem_367"));
    assert!(!g.trinket_eligible(0, sous), "a passive Hero Power");
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_001"));
    assert!(g.trinket_eligible(0, sous));
    // Ghastly Sticker: no end of turn minion needed (client offers).
    assert!(g.trinket_eligible(0, ghastly));
    // Menagerie: Glowing Crystal needs three distinct types in the warband.
    let crystal = cid("BG36_MagicItem_220");
    let seen = |board: &[&str]| {
        (0..150).any(|seed| {
            let mut g = game(8800 + seed);
            g.lobby_races = Races::ALL;
            set_board(&mut g, 0, board);
            g.trinket_offer(0, CardKind::TrinketLesser)
                .contains(&crystal)
        })
    };
    assert!(!seen(&["BG32_823", "BG26_529", "BG22_202"])); // two types
    assert!(seen(&["BG26_529", "BG22_202", "BGS_004"])); // Dragon, Murloc, Demon
    // A third type in hand does not count.
    let third_in_hand = (0..150).any(|seed| {
        let mut g = game(8800 + seed);
        g.lobby_races = Races::ALL;
        set_board(&mut g, 0, &["BG26_529", "BG22_202"]);
        give(&mut g, 0, "BGS_004");
        g.trinket_offer(0, CardKind::TrinketLesser)
            .contains(&crystal)
    });
    assert!(!third_in_hand);
}

#[test]
fn trinket_offers_of_the_client_runs_1019() {
    use taverngym::cards::CardKind;
    use taverngym::content::trinket_offers::MENAGERIE;
    use taverngym::types::Races;
    // Ironforge Anvil (Greater) / The Eye of Dalaran (Lesser): "in" no type, 3 / 2 minions
    // with no type on board; the hand does not count.
    let mut g = game(8950);
    let (anvil, eye) = (cid("BG30_MagicItem_403"), cid("BG30_MagicItem_981"));
    assert!(!g.trinket_eligible(0, anvil) && !g.trinket_eligible(0, eye));
    set_board(&mut g, 0, &["BG36_345", "BG36_345"]); // no type
    give(&mut g, 0, "BG36_354"); // no type (another card: no triple)
    assert!(g.trinket_eligible(0, eye));
    assert!(!g.trinket_eligible(0, anvil));
    set_board(&mut g, 0, &["BG36_345", "BG36_345", "BG36_200"]); // a Beast
    assert!(!g.trinket_eligible(0, anvil));
    set_board(&mut g, 0, &["BG36_345"; 3]);
    assert!(g.trinket_eligible(0, anvil));
    // Baller Portrait: no "sold a Baller" condition.
    assert_eq!(g.players[0].stats.ballers_sold, 0);
    assert!(g.trinket_eligible(0, cid("BG36_MagicItem_390")));
    // No menagerie trinket is guaranteed: three types, in no type.
    let without = (0..150).any(|seed| {
        let mut g = game(8960 + seed);
        g.lobby_races = Races::ALL;
        set_board(&mut g, 0, &["BG26_529", "BG22_202", "BGS_004"]); // Dragon, Murloc, Demon
        !g.trinket_offer(0, CardKind::TrinketLesser)
            .iter()
            .any(|t| MENAGERIE.contains(&db().get(*t).id.as_str()))
    });
    assert!(without);
}

#[test]
fn a_seat_can_be_exempt_from_the_action_caps() {
    // There is no cap by default; where a host caps some seats, a person's seat can be
    // exempt.
    use taverngym::debug::{DebugCmd, run_debug};
    let mut g = game(8901);
    g.cfg.turn_action_caps = vec![3];
    g.cfg.freeze_presses_per_turn = 0; // (the Freeze button's own limit is not tested here)
    for _ in 0..3 {
        g.apply(0, Action::Freeze).unwrap();
    }
    assert!(g.apply(0, Action::Freeze).is_err());
    assert!(!g.legal_mask(0)[taverngym::actions::A_FREEZE]);
    run_debug(&mut g, DebugCmd::ActionCap { p: 1, on: false }).unwrap();
    for _ in 0..10 {
        g.apply(1, Action::Freeze).unwrap();
    }
    assert!(g.legal_mask(1)[taverngym::actions::A_FREEZE]);
    // The next turn starts fresh for the capped seat.
    end_turn_all(&mut g);
    g.apply(0, Action::Freeze).unwrap();
}

#[test]
fn master_nguyen_is_not_offered_locked_or_quest_powers() {
    let out = |turn: u32, id: &str| taverngym::content::nguyen_excluded(turn).contains(&cid(id));
    // "Unlocks on Turn 3" powers: not on Turn 2, fine from Turn 3.
    for id in ["BG31_HERO_006p", "BG23_HERO_306p", "TB_BaconShop_HP_022"] {
        assert!(out(2, id), "{id}");
        assert!(!out(3, id), "{id}");
    }
    assert!(out(6, "BG22_HERO_002p") && !out(7, "BG22_HERO_002p")); // Drek'Thar: Turn 7
    // Quest-like powers: never.
    for id in [
        "TB_BaconShop_HP_065",
        "BG20_HERO_242p",
        "BG33_HERO_001p_ALT",
    ] {
        assert!(out(12, id), "{id}");
    }
    // An ordinary power is not excluded.
    assert!(!out(2, "TB_BaconShop_HP_001"));
    // Nguyen's own offer on Turn 2 holds none of them.
    for seed in 0..30 {
        let mut g = game(8950 + seed);
        g.lobby_races = no_aberrations();
        g.players[0].hero = Some(cid("BG20_HERO_202"));
        g.turn = 2;
        g.with_recruit(0, |c| taverngym::content::hero_start_of_turn(c, 0));
        let Some(Pending::HeroPowerChoice { options, .. }) = g.players[0].pending.last() else {
            panic!("a Hero Power choice is expected")
        };
        let banned = taverngym::content::nguyen_excluded(2);
        assert!(options.iter().all(|o| !banned.contains(o)), "seed {seed}");
    }
}

#[test]
fn assorted_card_rules() {
    // Golden minions given directly give their Triple Reward when played -- Golden Snarky Shark,
    // Golden Fishbait, and Splinter of Aurum's Golden Tier 5 minion.
    for card in ["BG36_206_G", "BG36_205_G", "BG28_633_G"] {
        let mut g = game(9001);
        give(&mut g, 0, card);
        g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
        assert_eq!(triple_rewards(&g, 0), 1, "{card}");
    }
    let mut g = game(9002);
    g.players[0].tier = 5;
    g.players[0].trinkets.push(cid("BG32_MagicItem_350"));
    g.players[0].gold = 15;
    set_board(&mut g, 0, &["BG36_345"]);
    g.apply(0, Action::Sell(0)).unwrap();
    let got = g.players[0].hand[0].item.as_minion().unwrap().clone();
    assert!(got.golden && got.triple_reward && got.tier == 5);
    // A sold Faceless Operative gives two Aberrations, never itself.
    let mut g = game(9003);
    g.players[0].tier = 4;
    g.universe = vec![cid("BG36_308"), cid("BG36_101")];
    set_board(&mut g, 0, &["BG36_308"]);
    g.apply(0, Action::Sell(0)).unwrap();
    assert_eq!(hand_ids(&g, 0), vec!["BG36_101"; 2]);
    // Imposing Percussionist never Discovers itself.
    let mut g = game(9004);
    g.players[0].tier = 4;
    g.universe = vec![cid("BG26_525"), cid("BGS_004")];
    for c in g.universe.clone() {
        g.pool.put(c, 5);
    }
    give(&mut g, 0, "BG26_525");
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    let ids: Vec<String> = discover_options(&g)
        .iter()
        .map(|m| db().get(m.card).id.clone())
        .collect();
    assert_eq!(ids, vec!["BGS_004"]);
}

#[test]
fn quest_goals_scale_with_the_reward_and_rewards_are_weighted() {
    use taverngym::content::{quest_goal, reward_weight};
    // Client tag 2467 of the Reward is the percentage applied to the Quest's base number; tag 2653
    // is the Reward's offer weight.
    let money = cid("BG24_Quest_126"); // Follow the Money: base 27
    for (reward, goal) in [
        ("BG24_Reward_107", 19), // Snicker Snacks, 70
        ("BG24_Reward_313", 27), // Wondrous Wisdomball, 100
        ("BG24_Reward_109", 46), // Stolen Gold, 170
        ("BG24_Reward_350", 68), // Pilfered Lamps, 250
    ] {
        assert_eq!(quest_goal(Races::ALL, money, cid(reward)), goal, "{reward}");
    }
    assert_eq!(
        quest_goal(Races::ALL, cid("BG24_Quest_151"), cid("BG24_Reward_350")),
        10
    ); // 4 x 2.5
    assert_eq!(
        quest_goal(Races::ALL, cid("BG24_Quest_352"), cid("BG28_Reward_501")),
        23
    ); // 9 x 2.5
    assert_eq!(reward_weight(cid("BG24_Reward_350")), 33);
    assert_eq!(reward_weight(cid("BG28_Reward_501")), 100);
    assert_eq!(reward_weight(cid("BG24_Reward_107")), 100, "no tag: 100");
    // The offer shows the goal of the pair, and taking the Quest keeps it.
    let mut g = game(9101);
    g.with_recruit(0, |c| c.offer_quests(0, 2));
    let Some(Pending::Quest { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("a Quest offer is expected")
    };
    let (q, r) = options[1];
    g.apply(0, Action::Choose(1)).unwrap();
    let taken = &g.players[0].quests[0];
    assert_eq!(
        (taken.card, taken.reward, taken.goal),
        (q, r, quest_goal(g.lobby_races, q, r))
    );
    // A weight-33 Reward is offered about a third as often as a weight-100 one.
    let (lamps, tampering) = (cid("BG24_Reward_350"), cid("BG28_Reward_501"));
    let (mut a, mut b) = (0, 0);
    let mut g = game(9102);
    for _ in 0..4000 {
        g.players[0].pending.clear();
        g.with_recruit(0, |c| c.offer_quests(0, 2));
        let Some(Pending::Quest { options, .. }) = g.players[0].pending.last() else {
            panic!("a Quest offer is expected")
        };
        a += options.iter().filter(|o| o.1 == lamps).count();
        b += options.iter().filter(|o| o.1 == tampering).count();
    }
    assert!(b > 100 && a * 2 < b, "{a} vs {b}");
    assert!(a * 5 > b, "{a} vs {b}");
}

#[test]
fn quest_reward_rules() {
    use taverngym::script::{Me, script};
    let me = |card: CardId| Me {
        side: 0,
        eid: 0,
        card,
        golden: false,
        opt: 0,
        pos: 0,
        data: 0,
    };
    let count = |g: &Game, id: &str| hand_ids(g, 0).iter().filter(|x| *x == id).count();

    // A Spellcraft Reward earned in combat gives its first spell at the next turn start,
    // not one in combat and another at the turn start.
    let mut g = game(9201);
    g.with_recruit(0, |c| {
        c.start_quest(0, cid("BG24_Quest_352"), cid("BG24_Reward_719"))
    });
    g.players[0].quests[0].goal = 1; // Crack the Case: one attack
    set_board(&mut g, 0, &["BG35_155"]);
    set_board(&mut g, 1, &["BG36_345"]);
    fight(&mut g);
    assert_eq!(g.players[0].rewards, vec![cid("BG24_Reward_719")]);
    assert_eq!(count(&g, "BG24_Reward_719t"), 0);
    end_turn_all(&mut g);
    assert_eq!(count(&g, "BG24_Reward_719t"), 1);

    // Cooked Book: the buy that completes a triple still gets its +2/+2 (the copy is buffed
    // before it merges).
    let mut g = game(9202);
    g.players[0].rewards.push(cid("BG24_Reward_306"));
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG36_345", "BG36_345"]);
    g.players[0].shop.clear();
    let m = minion(&mut g, 0, "BG36_345");
    g.players[0]
        .shop
        .push(taverngym::state::ShopSlot::new(m.eid, Item::Minion(m)));
    g.apply(0, Action::Buy(0)).unwrap();
    let golden = g.players[0].hand[0].item.as_minion().unwrap();
    assert!(golden.golden);
    assert_eq!((golden.atk, golden.hp), (6 + 2, 6 + 2));

    // Double-Headed Reward with a full hand: one extra copy waits for space, not two.
    let mut g = game(9203);
    g.players[0].rewards.push(cid("BG28_Reward_506"));
    g.players[0].gold = 10;
    for _ in 0..9 {
        give(&mut g, 0, "BG28_810");
    }
    g.players[0].shop.clear();
    let m = minion(&mut g, 0, "BG36_345");
    g.players[0]
        .shop
        .push(taverngym::state::ShopSlot::new(m.eid, Item::Minion(m)));
    g.apply(0, Action::Buy(0)).unwrap();
    assert_eq!(g.players[0].hand.len(), 10);
    assert_eq!(g.players[0].hand_queue.len(), 1);

    // Ethereal Evidence with a full hand: the pick becomes the Reward and no card is left
    // waiting for the hand.
    let mut g = game(9204);
    let evidence = cid("BG24_Reward_363");
    g.players[0].rewards.push(evidence);
    for _ in 0..10 {
        give(&mut g, 0, "BG28_810");
    }
    g.with_recruit(0, |c| {
        let fx = script(evidence).unwrap().start_of_turn.unwrap();
        fx(c, me(evidence));
    });
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].rewards.len(), 2);
    assert_eq!(g.players[0].hand.len(), 10);
    // (The new Reward's own cards may wait for space; the Reward card itself must not.)
    let reward_card = |i: &Item| db().get(i.card()).kind == taverngym::cards::CardKind::QuestReward;
    assert!(!g.players[0].hand_queue.iter().any(reward_card));
    assert!(!g.players[0].hand.iter().any(|h| reward_card(&h.item)));

    // The Golden Hammer: "Golden until next turn" also ends for the reward flag, and a
    // minion that is not Golden never gives a Triple Reward.
    let mut g = game(9205);
    set_board(&mut g, 0, &["BG36_345"]);
    give(&mut g, 0, "BG24_Reward_719t");
    cast_on(&mut g, 0, 0);
    assert!(g.players[0].board[0].golden);
    end_turn_all(&mut g);
    let m = &g.players[0].board[0];
    assert!(!m.golden && !m.triple_reward);
    assert_eq!((m.atk, m.max_hp), (3, 3));
    let mut stale = minion(&mut g, 0, "BG36_345");
    stale.triple_reward = true;
    g.with_recruit(0, |c| {
        c.add_to_hand(0, Item::Minion(stale));
    });
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Play { hand, pos: 0 }).unwrap();
    assert_eq!(triple_rewards(&g, 0), 0);

    // Round Up the Suspects counts an enemy minion destroyed without a killer; Find the
    // Murder Weapon counts a temporary stat gain.
    let mut g = game(9206);
    g.with_recruit(0, |c| {
        c.start_quest(0, cid("BG27_Quest_802"), cid("BG24_Reward_313"))
    });
    g.with_recruit(0, |c| {
        c.start_quest(0, cid("BG24_Quest_123"), cid("BG24_Reward_107"))
    });
    set_board(&mut g, 0, &["BG36_345"]);
    set_board(&mut g, 1, &["BG36_345"]);
    {
        let boards = [g.players[0].board.clone(), g.players[1].board.clone()];
        let mut c = taverngym::engine::Ctx::new(&mut g, boards, [Some(0), Some(1)], true);
        let enemy = c.boards[1][0].eid;
        c.destroy(enemy);
        c.resolve_deaths();
    }
    let progress = |g: &Game, q: &str| {
        g.players[0]
            .quests
            .iter()
            .find(|x| x.card == cid(q))
            .map(|x| x.progress)
    };
    assert_eq!(progress(&g, "BG27_Quest_802"), Some(1));
    let eid = g.players[0].board[0].eid;
    g.with_recruit(0, |c| c.buff_temp(eid, 1, 1));
    assert_eq!(progress(&g, "BG24_Quest_123"), Some(1));
}

#[test]
fn whenever_listeners_act_before_the_spell_and_the_battlecry() {
    use taverngym::content::acts_whenever;
    use taverngym::event::ev;
    // "Whenever you cast" acts before the spell's effect,
    // "After you cast" after it. Hooktail (10/10) and a 20/20; Channel the Devourer on the
    // 20/20: Hooktail's +1 Attack lands first, the spell then moves 21/20 -> 32/30.
    let mut g = game(9301);
    set_board(&mut g, 0, &["BG27_005", "BG36_345"]);
    g.players[0].board[0].set_stats(10, 10);
    g.players[0].board[1].set_stats(20, 20);
    give(&mut g, 0, "EBG_Spell_032");
    cast_on(&mut g, 0, 1);
    assert_eq!(stats(&g, 0), vec![(32, 30)]);
    assert!(acts_whenever(ev::SPELL_CAST, cid("BG36_108"))); // Vicious Mindslasher
    assert!(acts_whenever(ev::SPELL_CAST, cid("BG27_005_G")));
    assert!(!acts_whenever(ev::SPELL_CAST, cid("BG36_MagicItem_404"))); // Corrupted Baton
    assert!(!acts_whenever(ev::SPELL_CAST, cid("BG28_633"))); // Felboar: "After you cast 3"
    assert!(acts_whenever(ev::CARD_PLAYED, cid("BG30_MagicItem_900"))); // Dragonwing Glider
    assert!(acts_whenever(
        ev::CARD_PLAYED,
        cid("BG36_MidGameEffect_000t75")
    ));

    // "Whenever you play a card" acts before the played minion's Battlecry: Dragonwing
    // Glider has buffed the Dragon while Faceless Manipulator still asks for its target.
    let mut g = game(9302);
    g.players[0].trinkets.push(cid("BG30_MagicItem_900"));
    set_board(&mut g, 0, &["BG26_529"]);
    let before = stats(&g, 0)[0];
    give(&mut g, 0, "BG_EX1_564");
    g.apply(0, Action::Play { hand: 0, pos: 1 }).unwrap();
    assert!(matches!(
        g.players[0].pending.last(),
        Some(Pending::Target { .. })
    ));
    assert_eq!(stats(&g, 0)[0], (before.0 + 4, before.1 + 4));
    assert_eq!(g.players[0].stats.minions_played_turn, 1);
    // The copy is taken after the buff, and nothing fires twice.
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(stats(&g, 0), vec![(before.0 + 4, before.1 + 4); 2]);
}

#[test]
fn every_n_turns_counts_turns_not_triggers() {
    // "at the end of every 2 turns" goes no, yes, no, yes from the turn the minion is played; extra
    // end of turn triggers repeat the effect in the firing turn and do not move the count faster.
    let replication = cid("BG36_MidGameEffect_000t18");
    let copies = |g: &Game| hand_ids(g, 0).iter().filter(|x| *x == "BG36_345").count();
    for (drakkari, per_fire) in [(false, 1), (true, 2)] {
        let mut g = game(9310);
        let mut board = vec!["BG36_345"];
        if drakkari {
            board.push("BG26_ICC_901"); // Drakkari Enchanter: end of turn effects twice
        }
        set_board(&mut g, 0, &board);
        g.players[0].board[0].attached.push(replication);
        let mut got = vec![];
        for _ in 0..4 {
            g.players[0].hand.clear();
            g.cfg.triple_copies = 99;
            g.apply(0, Action::EndTurn).unwrap();
            got.push(copies(&g));
            for p in 1..8 {
                if g.needs_action(p) {
                    g.apply(p, Action::EndTurn).unwrap();
                }
            }
        }
        assert_eq!(got, vec![0, per_fire, 0, per_fire], "drakkari {drakkari}");
    }
}

#[test]
fn xavius_specter_and_other_card_rules() {
    use taverngym::script::{Me, script};
    let me = |card: CardId| Me {
        side: 0,
        eid: 0,
        card,
        golden: false,
        opt: 0,
        pos: 0,
        data: 0,
    };
    let start_of_turn = |g: &mut Game, id: &str| {
        let card = cid(id);
        g.with_recruit(0, |c| {
            (script(card).unwrap().start_of_turn.unwrap())(c, me(card))
        });
    };
    // Xavius's Discover has hand protection (client tag 4701): with a full hand the pick
    // waits for space.
    let mut g = murloc_game(1, 9, RulesConfig::default());
    let hp = cid("BG36_HERO_105p");
    while g.players[0].hand.len() < 10 {
        give(&mut g, 0, "BG28_810");
    }
    g.with_recruit(0, |c| {
        let options = c.dark_gift_options(0);
        c.offer_discover(0, hp, options);
    });
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(
        (g.players[0].hand.len(), g.players[0].hand_queue.len()),
        (10, 1)
    );

    // Victim's Specter keeps its record: the last friendly minion that died, again after a
    // combat without a death.
    let mut g = game(9401);
    g.cfg.triple_copies = 99;
    g.players[0].rewards.push(cid("BG24_Reward_138"));
    set_board(&mut g, 0, &["BG36_345"]);
    set_board(&mut g, 1, &["BG35_155", "BG35_155"]);
    fight(&mut g);
    for n in 1..=2 {
        start_of_turn(&mut g, "BG24_Reward_138");
        assert_eq!(hand_ids(&g, 0), vec!["BG36_345"; n]);
    }

    // A ghost is the last opponent too: Doppelganger's Locket Discovers from its warband.
    let mut g = game(9402);
    g.turn = 5;
    g.players[1].last_board = vec![minion(&mut g, 1, "BG35_155")];
    g.players[1].alive = false;
    g.eliminated = vec![1];
    g.ghost_player = Some(1);
    g.players[0].last_opponent = None;
    assert_eq!(g.last_fought(0), Some(1));
    g.players[0].rewards.push(cid("BG27_Reward_806"));
    start_of_turn(&mut g, "BG27_Reward_806");
    assert_eq!(discover_options(&g).len(), 1);

    // Ethereal Evidence never offers a Reward that counts or grows.
    let counting = [
        "BG24_Reward_306",
        "BG24_Reward_361",
        "BG28_Reward_504",
        "BG28_Reward_508",
        "BG28_Reward_509",
    ];
    let mut g = game(9403);
    g.players[0].rewards.push(cid("BG24_Reward_363"));
    for _ in 0..300 {
        g.players[0].pending.clear();
        start_of_turn(&mut g, "BG24_Reward_363");
        let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
            panic!("a Reward choice is expected")
        };
        for o in options {
            assert!(!counting.contains(&db().get(o.card()).id.as_str()));
        }
    }

    // Chef's Choice can only be aimed at a minion with a type by hand.
    let mut g = game(9404);
    set_board(&mut g, 0, &["BG36_345", "BG22_202"]);
    g.players[0].shop.clear();
    give(&mut g, 0, "BG28_518");
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
        panic!("a target choice is expected")
    };
    assert_eq!(options, &vec![g.players[0].board[1].eid]);
}

#[test]
fn copies_of_a_dark_gift_are_separate_entries() {
    // Every Dark Gift on a minion is its own entry. Two Transcendences ("Start of Combat: Triple
    // this minion's stats") fire one after the other: x9.
    let mut g = game(9501);
    g.cfg.record_log = true;
    set_board(&mut g, 0, &["BG36_345"]);
    let t = cid("BG36_MidGameEffect_000t81");
    g.players[0].board[0].attached = vec![t, t];
    set_board(&mut g, 1, &["BG36_345"]);
    g.simulate_combat(0, Some(1), None);
    let b = after_soc(&g, 0);
    assert_eq!((b[0].1, b[0].2), (27, 27), "{b:?}");
}

#[test]
fn mystery_cube_replaces_its_pick_every_turn() {
    // The Cube goes on, like Ethereal Evidence -- each turn choose from 2 new Lesser Trinkets to
    // replace what it became; it never offers itself, counting / growing / self-replacing trinkets
    // or ones that pay off turns later.
    let out = [
        "BG30_MagicItem_703",
        "BG36_MagicItem_414",
        "BG36_MagicItem_407",
        "BG30_MagicItem_435",
        "BG30_MagicItem_891",
        "BG36_MagicItem_411", // Reinvigorating Light: no second Hero Power
        // Nothing that acts on other trinkets or pays off later.
        "BG35_MagicItem_816", // Orb of the Unknown
        "BG35_MagicItem_818", // Mysterious Orb
        "BG30_MagicItem_888", // Souvenir Stand
        "BG32_MagicItem_428", // Worn Treasure Map
        "BG36_MagicItem_301", // Lockbox Portrait
    ];
    // The first offers of many lobbies: never an excluded trinket.
    for seed in 0..80 {
        let mut g = game(9600 + seed);
        g.with_recruit(0, |c| {
            taverngym::content::grant_trinket(c, 0, cid("BG30_MagicItem_703"))
        });
        let Some(Pending::Trinket { options, .. }) = g.players[0].pending.last() else {
            panic!("seed {seed}: a trinket choice is expected")
        };
        for o in options {
            assert!(!out.contains(&db().get(*o).id.as_str()), "seed {seed}");
        }
    }
    for seed in 0..12 {
        let mut g = game(9600 + seed);
        g.with_recruit(0, |c| {
            taverngym::content::grant_trinket(c, 0, cid("BG30_MagicItem_703"))
        });
        let mut last = None;
        for turn in 0..3 {
            let Some(Pending::Trinket {
                options,
                free,
                replace,
            }) = g.players[0].pending.last().cloned()
            else {
                panic!("seed {seed} turn {turn}: a free trinket choice is expected")
            };
            assert!(free && replace.is_some());
            for o in &options {
                assert!(!out.contains(&db().get(*o).id.as_str()), "seed {seed}");
            }
            g.apply(0, Action::Choose(0)).unwrap();
            assert_eq!(
                g.players[0].trinkets.len(),
                1,
                "one trinket in the Cube's place"
            );
            assert_ne!(Some(g.players[0].trinkets[0]), last);
            last = Some(g.players[0].trinkets[0]);
            // The pick's own choices (a Discover it gives...) are answered, then next turn.
            end_turn_all_but_trinket(&mut g);
        }
    }
}

/// End the turn for everyone, answering pending choices with option 0, but leave a
/// trinket choice of player 0 open at the start of the next turn.
fn end_turn_all_but_trinket(g: &mut Game) {
    for p in 0..8 {
        while g.players[p].pending.last().is_some() {
            g.apply(p, Action::Choose(0)).unwrap();
        }
        if g.needs_action(p) {
            g.apply(p, Action::EndTurn).unwrap();
        }
    }
    // Start-of-turn choices other than the Cube's (Discovers) come first on the stack.
    while matches!(
        g.players[0].pending.last(),
        Some(p) if !matches!(p, Pending::Trinket { .. })
    ) {
        g.apply(0, Action::Choose(0)).unwrap();
    }
}

#[test]
fn holy_vanguard_in_the_tavern_follows_your_health() {
    // The +30/+30 is an aura shown wherever the Vanguard is; it changes the moment your Health goes
    // from 16 to 15.
    let mut g = game(9701);
    g.players[0].health = 16;
    g.players[0].armor = 0;
    g.players[0].shop.clear();
    let m = minion(&mut g, 0, "BG36_372");
    let base = (m.atk, m.hp);
    g.players[0]
        .shop
        .push(taverngym::state::ShopSlot::new(m.eid, Item::Minion(m)));
    g.with_recruit(0, |c| c.hero_damage(0, 1));
    let v = g.players[0].shop[0].item.as_minion().unwrap();
    assert_eq!((v.atk, v.hp), (base.0 + 30, base.1 + 30));
}

fn choose_target(g: &mut Game, p: PlayerId, eid: u32) {
    let Some(Pending::Target { options, .. }) = g.players[p].pending.last().cloned() else {
        panic!("expected a target choice")
    };
    let i = options.iter().position(|e| *e == eid).unwrap();
    g.apply(p, Action::Choose(i)).unwrap();
}

#[test]
fn murloc_holmes_has_nothing_to_guess_on_turn_one() {
    // No opponent had a warband "last combat" on Turn 1.
    let mut g = game(9801);
    g.set_hero_power(0, 0, cid("BG23_HERO_303p2"));
    g.players[0].gold = 10;
    assert!(!g.can_hero_power(0, 0), "Turn 1");
    for p in 0..8 {
        set_board(&mut g, p, &["BG36_345"]);
    }
    end_turn_all(&mut g);
    g.players[0].gold = 10;
    assert!(
        g.can_hero_power(0, 0),
        "the opponents fought with a warband"
    );
    g.apply(0, Action::HeroPower(0)).unwrap();
    assert!(matches!(
        g.players[0].pending.last(),
        Some(Pending::Discover { options, .. }) if options.len() == 2
    ));
}

#[test]
fn maievs_locked_card_takes_no_part_in_a_triple() {
    // The card is locked as it arrives, so it cannot complete a triple, be played or be discarded;
    // buffs for minions in hand still reach it.
    let mut g = game(9802);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_068"));
    g.players[0].gold = 10;
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG36_345", "BG36_345"]);
    let m = minion(&mut g, 0, "BG36_345");
    let sid = m.eid;
    g.players[0].shop.clear();
    g.players[0]
        .shop
        .push(taverngym::state::ShopSlot::new(sid, Item::Minion(m)));
    g.apply(0, Action::HeroPower(0)).unwrap();
    choose_target(&mut g, 0, sid);
    assert_eq!(
        g.players[0].board.len(),
        2,
        "no triple with the locked copy"
    );
    assert_eq!(g.players[0].hand.len(), 1);
    let h = &g.players[0].hand[0];
    let (hid, atk) = (h.hid, h.item.as_minion().unwrap().atk);
    assert_eq!(h.locked, 2);
    assert!(!h.item.as_minion().unwrap().golden);
    assert!(g.apply(0, Action::Play { hand: 0, pos: 0 }).is_err());
    g.with_recruit(0, |c| c.buff(hid, 2, 2));
    assert_eq!(g.players[0].hand[0].item.as_minion().unwrap().atk, atk + 2);
    // Once the lock is gone the three copies combine.
    end_turn_all(&mut g);
    assert_eq!(g.players[0].hand[0].locked, 1);
    assert_eq!(g.players[0].board.len(), 2);
    end_turn_all(&mut g);
    assert!(
        g.players[0]
            .hand
            .iter()
            .any(|h| h.item.as_minion().is_some_and(|m| m.golden)),
        "unlocked: the triple completes"
    );
}

#[test]
fn mishmash_follows_every_rise_of_the_amalgams_stats() {
    // Mishmash follows the Amalgam's stats themselves. Whatever raises them - stats set to a higher
    // value (Tyrael's 50/50), stats until next turn, Magnetized stats, a "this game" bonus -
    // Mishmash gains the rise for good; a drop changes nothing.
    let mut g = game(9803);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    g.players[0].hand.clear();
    set_board(
        &mut g,
        0,
        &[
            "BG36_356",
            "TB_BaconShop_HP_033t",
            "TB_BaconShop_HERO_33_Buddy",
        ],
    );
    let stats = |g: &Game, i: usize| (g.players[0].board[i].atk, g.players[0].board[i].max_hp);
    let (amalgam, (a0, h0), (m0a, m0h)) = (g.players[0].board[1].eid, stats(&g, 1), stats(&g, 2));
    g.apply(0, Action::Activate(0)).unwrap();
    choose_target(&mut g, 0, amalgam);
    assert_eq!(stats(&g, 1), (50, 50));
    let m1 = (m0a + 50 - a0, m0h + 50 - h0);
    assert_eq!(stats(&g, 2), m1, "what the set added");
    // A set that adds nothing is no gain, and neither is a drop.
    g.with_recruit(0, |c| c.set_stats(amalgam, 50, 50));
    g.with_recruit(0, |c| c.set_stats(amalgam, 40, 50));
    assert_eq!(stats(&g, 2), m1);
    // A rise after the drop counts from the lower value; stats until next turn are a rise
    // and Mishmash keeps them.
    g.with_recruit(0, |c| c.buff_temp(amalgam, 3, 0));
    assert_eq!(stats(&g, 1), (43, 50));
    assert_eq!(stats(&g, 2), (m1.0 + 3, m1.1));
    // Magnetized stats.
    let mag = minion(&mut g, 0, "BG34_Giant_610t"); // Satellite 4/4
    let (ma, mh) = (mag.atk, mag.max_hp);
    g.with_recruit(0, |c| {
        c.add_to_hand(0, Item::Minion(mag));
    });
    assert!(g.can_magnetize(0, 0, 1));
    g.apply(0, Action::Magnetize { hand: 0, board: 1 }).unwrap();
    assert_eq!(stats(&g, 1), (43 + ma, 50 + mh));
    let m2 = (m1.0 + 3 + ma, m1.1 + mh);
    assert_eq!(stats(&g, 2), m2);
    // "Your Undead have +2 Attack this game": the Amalgam has every type, and so has
    // Mishmash - its own +2 and the Amalgam's rise.
    g.with_recruit(0, |c| c.bump_player_stat(0, |st| st.undead_atk += 2));
    assert_eq!(stats(&g, 1), (45 + ma, 50 + mh));
    let m3 = (m2.0 + 4, m2.1);
    assert_eq!(stats(&g, 2), m3);
    // A stat written directly (no buff call) is seen when the next action starts.
    g.players[0].board[1].atk += 6;
    g.with_recruit(0, |_| ());
    let m4 = (m3.0 + 6, m3.1);
    assert_eq!(stats(&g, 2), m4);
    // A Mishmash that arrives later only watches from then on; a Golden one gains twice.
    let base = {
        let m = minion(&mut g, 0, "TB_BaconShop_HERO_33_Buddy_G");
        (m.atk, m.max_hp)
    };
    give(&mut g, 0, "TB_BaconShop_HERO_33_Buddy_G");
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 3), base, "nothing from before it arrived");
    assert_eq!(stats(&g, 2), m4);
    g.with_recruit(0, |c| c.buff(amalgam, 1, 2));
    assert_eq!(stats(&g, 2), (m4.0 + 1, m4.1 + 2));
    assert_eq!(stats(&g, 3), (base.0 + 2, base.1 + 4));
    // Next turn the Amalgam's temporary Attack is gone; Mishmash keeps what it gained, and
    // the combat in between adds nothing in the Tavern.
    end_turn_all(&mut g);
    assert_eq!(stats(&g, 1), (52 + ma - 3, 52 + mh));
    assert_eq!(stats(&g, 2), (m4.0 + 1, m4.1 + 2));
    assert_eq!(stats(&g, 3), (base.0 + 2, base.1 + 4));
    // An Amalgam that enters the board gains nothing by entering.
    g.with_recruit(0, |c| {
        let mut m = c.new_minion(0, cid("TB_BaconShop_HP_033t"));
        m.set_stats(50, 50);
        c.summon(0, 0, m);
    });
    let m5 = (m4.0 + 1, m4.1 + 2);
    assert_eq!(stats(&g, 3), m5);
    // In combat Mishmash gains with its Amalgam like any minion gains there: for the
    // combat. A permanent gain goes to the Amalgam's warband copy; the Mishmash in the
    // Tavern was not there to see it.
    let before = stats(&g, 2);
    let mut c = combat_ctx(&mut g);
    c.buff_perm(0, amalgam, 5, 5);
    let mish = c.boards[0][3].eid;
    let seen = c.minion(mish).map(|m| (m.atk, m.max_hp)).unwrap();
    assert_eq!(seen, (m5.0 + 5, m5.1 + 5), "in the combat");
    drop(c);
    g.with_recruit(0, |_| ());
    assert_eq!(
        stats(&g, 2),
        (before.0 + 5, before.1 + 5),
        "the Amalgam kept it"
    );
    assert_eq!(stats(&g, 3), m5, "Mishmash did not");
}

#[test]
fn galewings_eastern_plaguelands_discovers_a_minion_that_is_kept() {
    // The reward Discover belongs to the same Hero Power as the flightpath choice: the pick
    // was taken for a flightpath and removed from the hand.
    let mut g = game(9804);
    g.set_hero_power(0, 0, cid("BG20_HERO_283p"));
    g.players[0].hand.clear();
    g.apply(0, Action::HeroPower(0)).unwrap();
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!()
    };
    let i = options
        .iter()
        .position(|o| o.card() == cid("BG20_HERO_283p_t3"))
        .unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
    assert!(g.players[0].hand.is_empty());
    for turn in 0..3 {
        assert!(!g.can_hero_power(0, 0), "in flight (turn {turn})");
        assert!(g.players[0].pending.is_empty());
        for p in 1..8 {
            while g.players[p].pending.last().is_some() {
                g.apply(p, Action::Choose(0)).unwrap();
            }
        }
        for p in 0..8 {
            if g.needs_action(p) {
                g.apply(p, Action::EndTurn).unwrap();
            }
        }
    }
    let tier = g.players[0].tier;
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("the path ends with a Discover: {:?}", g.players[0].pending)
    };
    assert_eq!(options.len(), 3);
    assert!(
        options
            .iter()
            .all(|o| o.as_minion().is_some_and(|m| m.tier == tier))
    );
    let pick = options[1].card();
    g.apply(0, Action::Choose(1)).unwrap();
    assert_eq!(g.players[0].hand.len(), 1, "the minion stays in the hand");
    assert_eq!(g.players[0].hand[0].item.card(), pick);
    assert!(g.can_hero_power(0, 0), "the path is over");
    g.apply(0, Action::HeroPower(0)).unwrap();
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!()
    };
    assert_eq!(options.len(), 2, "the completed path is not offered again");
    assert!(options.iter().all(|o| o.card() != cid("BG20_HERO_283p_t3")));
}

#[test]
fn silas_tickets_thirty_percent_with_a_full_page_now_and_then() {
    // Each new card has a ticket 30% of the time, and one Refresh in twenty tickets the whole page.
    // With 4 cards a page: 0.05 + 0.95 * 0.3^4 = 5.8% full pages, 0.05 + 0.95 * 0.3 = 33.5% of the
    // cards.
    let (mut refreshes, mut full, mut cards, mut marked) = (0u32, 0u32, 0u32, 0u32);
    for seed in 0..40 {
        let mut g = game(9900 + seed);
        g.set_hero_power(0, 0, cid("TB_BaconShop_HP_101"));
        for _ in 0..50 {
            g.with_recruit(0, |c| c.refresh_shop(0));
            let tickets = &g.players[0].powers[0].values;
            let n = g.players[0].shop.len() as u32;
            let k = g.players[0]
                .shop
                .iter()
                .filter(|s| tickets.contains(&(s.sid as i32)))
                .count() as u32;
            refreshes += 1;
            cards += n;
            marked += k;
            full += (n > 0 && k == n) as u32;
        }
    }
    assert_eq!(refreshes, 2000);
    assert!((80..=155).contains(&full), "full pages: {full}");
    let share = marked as f64 / cards as f64;
    assert!((0.31..=0.36).contains(&share), "tickets: {share}");

    // A Freeze keeps what is there: the card with a ticket keeps it, the card without one
    // stays without, whatever the odds of the next Refresh.
    let mut g = game(9941);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_101"));
    g.cfg.silas_ticket_pct = 100;
    g.with_recruit(0, |c| c.refresh_shop(0));
    g.players[0].shop.truncate(2);
    let (with, without) = (g.players[0].shop[0].sid, g.players[0].shop[1].sid);
    g.players[0].powers[0].values.retain(|x| *x == with as i32);
    g.apply(0, Action::Freeze).unwrap();
    end_turn_all(&mut g);
    let tickets = g.players[0].powers[0].values.clone();
    let shop: Vec<u32> = g.players[0].shop.iter().map(|s| s.sid).collect();
    assert_eq!(&shop[..2], &[with, without], "both kept");
    assert!(tickets.contains(&(with as i32)));
    assert!(!tickets.contains(&(without as i32)), "not rolled again");
    assert!(
        shop[2..].iter().all(|s| tickets.contains(&(*s as i32))),
        "the new cards are rolled"
    );
}

#[test]
fn hot_air_surveyor_and_balinda_add_their_blood_gem_casts() {
    // Surveyor's extra casts and Balinda's "twice" add, like the other "extra time" sources.
    for (board, casts) in [
        (vec!["BG30_121", "BG36_345"], 2),
        (vec!["BG35_883", "BG36_345"], 2),
        (vec!["BG30_121", "BG35_883", "BG36_345"], 3),
        (vec!["BG30_121_G", "BG35_883", "BG36_345"], 4),
        (vec!["BG30_121_G", "BG30_121_G", "BG35_883", "BG36_345"], 6),
        (vec!["BG30_121", "BG35_883_G", "BG36_345"], 4),
    ] {
        let mut g = game(9910);
        g.cfg.triple_copies = 99;
        set_board(&mut g, 0, &board);
        g.players[0].hand.clear();
        give(&mut g, 0, "BG20_GEM");
        let last = board.len() - 1;
        cast_on(&mut g, 0, last);
        assert_eq!(g.players[0].board[last].gems as u32, casts, "{board:?}");
    }
    // Temporal Tampering ("your Tavern spells cast an extra time") and Balinda: 3 casts of a
    // +2/+2 banana; a Golden Balinda: 4.
    for (balinda, casts) in [("BG35_883", 3), ("BG35_883_G", 4)] {
        let mut g = game(9911);
        g.players[0].gold = 10;
        g.players[0].rewards.push(cid("BG28_Reward_501"));
        set_board(&mut g, 0, &[balinda, "BG36_345"]);
        g.players[0].hand.clear();
        let before = g.players[0].board[1].atk;
        give(&mut g, 0, "BG28_897");
        cast_on(&mut g, 0, 1);
        assert_eq!(g.players[0].board[1].atk, before + 2 * casts, "{balinda}");
    }
}

#[test]
fn heroes_banned_by_a_type_in_the_lobby() {
    // Client tag 2703 on the hero (and Gallywix: out of lobbies with Elementals).
    use taverngym::types::Races;
    let bans = [
        ("TB_BaconShop_HERO_10", Races::ELEMENTAL), // Trade Prince Gallywix
        ("TB_BaconShop_HERO_91", Races::ELEMENTAL), // Zephrys, the Great
        ("TB_BaconShop_HERO_95", Races::MECH),      // Greybough (needs Beasts or Undead too)
        ("BG20_HERO_282", Races::DRAGON),           // Tamsin Roame
        ("TB_BaconShop_HERO_08", Races::DRAGON),    // Illidan Stormrage
        ("BG20_HERO_301", Races::NAGA),             // Mutanus the Devourer
        ("BG23_HERO_306", Races::ABERRATION),       // Sylvanas Windrunner
    ];
    let mut g = game(9920);
    let has = |g: &Game, id: &str| g.lobby_heroes().any(|h| h.id == id);
    for (id, race) in bans {
        g.lobby_races = Races(Races::ALL.0 & !race.0);
        assert!(has(&g, id), "{id} without {race:?}");
        g.lobby_races = Races::ALL;
        assert!(!has(&g, id), "{id} with {race:?}");
    }
    // The other direction (a hero that needs a type) was already there.
    for (id, race) in [
        ("TB_BaconShop_HERO_37", Races::DEMON),  // Lord Jaraxxus
        ("TB_BaconShop_HERO_25", Races::DEMON),  // Lich Baz'hial
        ("TB_BaconShop_HERO_17", Races::MECH),   // Millificent Manastorm
        ("TB_BaconShop_HERO_55", Races::MURLOC), // Fungalmancer Flurgl
    ] {
        g.lobby_races = Races(Races::ALL.0 & !Races::ABERRATION.0);
        assert!(has(&g, id), "{id} with {race:?}");
        g.lobby_races = Races(g.lobby_races.0 & !race.0);
        assert!(!has(&g, id), "{id} without {race:?}");
    }
}

#[test]
fn buddy_discovers_stay_within_the_tavern_tier() {
    // A Discover or a random card that names no Tier never reaches above your Tavern Tier. E.T.C.
    // offered Buddies of any Tier.
    let etc = cid("BG25_HERO_105p");
    for tier in [2u8, 3, 6] {
        let (mut top, mut seen) = (0u8, 0u32);
        for seed in 0..30 {
            let mut g = game(9930 + seed);
            g.set_hero_power(0, 0, etc);
            g.players[0].tier = tier;
            g.players[0].gold = 10;
            g.players[0].hand.clear();
            g.apply(0, Action::HeroPower(0)).unwrap();
            let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
                panic!("no Discover at Tier {tier}")
            };
            assert_eq!(options.len(), 3);
            for o in options {
                let t = o.as_minion().unwrap().tier;
                assert!(t <= tier, "a Tier {t} Buddy at Tavern Tier {tier}");
                top = top.max(t);
                seen += 1;
            }
        }
        assert_eq!(seen, 90);
        assert_eq!(top, tier, "Buddies of your own Tier are offered");
    }
    // Open Auditions (Reward): the same Discover at the start of each turn.
    let mut g = game(9960);
    g.players[0].tier = 1;
    g.players[0].hand.clear();
    g.with_recruit(0, |c| {
        taverngym::content::discover_buddy(c, 0, cid("BG28_Reward_513"))
    });
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!()
    };
    assert!(options.iter().all(|o| o.as_minion().unwrap().tier == 1));
}

/// The Freeze button answers ten times a turn, as on the client: freezing and unfreezing each count,
/// the eleventh press is not possible, and the next turn starts with all ten again.
#[test]
fn the_freeze_button_answers_ten_times_a_turn() {
    let a = taverngym::actions::A_FREEZE;
    let mut g = game(7);
    assert!(!g.players[0].shop.is_empty());
    for i in 0..10 {
        assert!(g.legal_mask(0)[a], "press {i}");
        g.apply(0, Action::Freeze).unwrap();
        assert_eq!(g.players[0].shop.iter().all(|s| s.frozen), i % 2 == 0);
    }
    assert_eq!(g.players[0].freezes_this_turn, 10);
    assert!(!g.legal_mask(0)[a]);
    assert!(g.apply(0, Action::Freeze).is_err());
    // The other seats have theirs.
    assert!(g.legal_mask(1)[a]);
    // Without the limit it goes on.
    let mut h = game(7);
    h.cfg.freeze_presses_per_turn = 0;
    for _ in 0..25 {
        h.apply(0, Action::Freeze).unwrap();
    }
    assert!(h.legal_mask(0)[a]);
    // The next turn starts with all ten again.
    for p in 0..8 {
        g.apply(p, Action::EndTurn).unwrap();
    }
    assert_eq!(g.turn, 2);
    assert_eq!(g.players[0].freezes_this_turn, 0);
    assert!(g.legal_mask(0)[a]);
    // The default config is written without the field.
    let json = serde_json::to_string(&RulesConfig::default()).unwrap();
    assert!(!json.contains("freeze_presses_per_turn"));
    let cfg: RulesConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg.freeze_presses_per_turn, 10);
    let off = RulesConfig {
        freeze_presses_per_turn: 0,
        ..RulesConfig::default()
    };
    assert!(
        serde_json::to_string(&off)
            .unwrap()
            .contains("\"freeze_presses_per_turn\":0")
    );
}

#[test]
fn turning_golden_gives_back_this_turns_uses() {
    // A minion made Golden in place can be Activated again, and its per-turn allowance starts over
    // at the Golden amount.
    use taverngym::content::make_golden;
    // Malchezaar: both Health Refreshes used, then Golden -> 4 left.
    let mut g = game(9970);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 0;
    set_board(&mut g, 0, &["BG26_524"]);
    let health = g.players[0].health + g.players[0].armor;
    g.apply(0, Action::Refresh).unwrap();
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(g.players[0].board[0].counter, 0);
    assert!(
        g.apply(0, Action::Refresh).is_err(),
        "no Gold and none left"
    );
    make_golden(&mut g.players[0].board[0]);
    assert_eq!(
        g.players[0].board[0].counter, 4,
        "the Golden allowance, in full"
    );
    for _ in 0..4 {
        g.apply(0, Action::Refresh).unwrap();
    }
    assert!(g.apply(0, Action::Refresh).is_err());
    assert_eq!(g.players[0].health + g.players[0].armor, health - 6);
    // An Activate minion (Clever Castaway, Activate (2)): once more after it turns Golden.
    let mut g = game(9971);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG36_342"]);
    g.apply(0, Action::Activate(0)).unwrap();
    while g.players[0].pending.last().is_some() {
        g.apply(0, Action::Choose(0)).unwrap();
    }
    assert!(!g.can_activate(0, 0), "once a turn");
    make_golden(&mut g.players[0].board[0]);
    assert!(
        g.can_activate(0, 0),
        "the Golden one has not been Activated"
    );
    // Stone Age Slab ("once per turn", counted up): available again.
    let mut g = game(9972);
    let mut slab = minion(&mut g, 0, "BG34_950");
    slab.counter = 1;
    make_golden(&mut slab);
    assert_eq!(slab.counter, 0);
    // A progress counter is not a per-turn use: Felboar keeps its count.
    let mut boar = minion(&mut g, 0, "BG28_633");
    boar.counter = 2;
    make_golden(&mut boar);
    assert_eq!(boar.counter, 2);
}

#[test]
fn a_refused_action_does_not_use_the_turns_budget() {
    // A refused action was counted as one.
    let mut g = game(9973);
    g.cfg.turn_action_caps = vec![3];
    set_board(&mut g, 0, &[]);
    let before = g.players[0].actions_this_turn;
    for _ in 0..5 {
        assert!(g.apply(0, Action::Sell(0)).is_err(), "nothing to sell");
    }
    assert_eq!(g.players[0].actions_this_turn, before);
    g.players[0].gold = 10;
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(g.players[0].actions_this_turn, before + 1);
}

#[test]
fn adopted_bonuses_are_not_added_again() {
    // A state built from what the client shows has the "this game" bonuses in the stats already.
    // Once adopted, a re-sync adds nothing.
    let mut g = game(9980);
    g.cfg.triple_copies = 99;
    g.players[0].hand.clear();
    g.players[0].shop.clear();
    // Eternal Knight (+4/+2 per Knight that died), Maritime Extortionist (+7/+7 per Golden
    // minion played), Falling Sky Golem (+4/+2 per Deathrattle), an Undead (+Attack).
    let ids = ["BG25_008", "BG36_524", "BG35_342", "BG25_011"];
    set_board(&mut g, 0, &ids);
    let give_hand = minion(&mut g, 0, "BG25_008_G");
    g.with_recruit(0, |c| {
        c.add_to_hand(0, Item::Minion(give_hand));
    });
    let shown: Vec<(i32, i32)> = g.players[0].board.iter().map(|m| (m.atk, m.hp)).collect();
    let hand_shown = {
        let m = g.players[0].hand[0].item.as_minion().unwrap();
        (m.atk, m.hp)
    };
    // The counters as read from the client; the minions carry no record of them yet.
    let st = &mut g.players[0].stats;
    st.eternal_knight_deaths = 3;
    st.golden_played = 2;
    st.deathrattles_triggered = 5;
    st.undead_atk = 4;
    g.adopt_bonuses(0);
    g.with_recruit(0, |_| ());
    let now: Vec<(i32, i32)> = g.players[0].board.iter().map(|m| (m.atk, m.hp)).collect();
    assert_eq!(
        now, shown,
        "nothing is added to stats that already hold the bonuses"
    );
    let m = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((m.atk, m.hp), hand_shown, "the hand as well");
    // What changes afterwards is added once.
    g.with_recruit(0, |c| {
        c.bump_player_stat(0, |st| st.eternal_knight_deaths += 1)
    });
    let knight = &g.players[0].board[0];
    assert_eq!((knight.atk, knight.hp), (shown[0].0 + 4, shown[0].1 + 2));
    // Without adopting, the same state gains every bonus a second time.
    let mut g2 = game(9980);
    set_board(&mut g2, 0, &ids);
    g2.players[0].stats.eternal_knight_deaths = 3;
    g2.with_recruit(0, |_| ());
    assert_eq!(g2.players[0].board[0].atk, shown[0].0 + 12);
}

/// `RulesConfig::hero_bans`: the heroes named there are in no lobby - not offered to anybody
/// and not among the lobby's heroes that Discovers draw from - and nothing else changes.
#[test]
fn banned_heroes_are_in_no_lobby() {
    let bans = [
        "BG24_HERO_100",
        "BG25_HERO_105",
        "BG34_HERO_000",
        "BG34_HERO_004",
    ];
    let offered = |cfg: &RulesConfig, seed: u64| -> Vec<String> {
        let g = Game::new(seed, cfg.clone());
        let mut ids: Vec<String> = g
            .players
            .iter()
            .flat_map(|p| match p.pending.last() {
                Some(Pending::HeroSelect { options }) => options.clone(),
                _ => vec![],
            })
            .map(|c| db().get(c).id.clone())
            .collect();
        ids.extend(g.lobby_heroes().map(|c| c.id.clone()));
        ids
    };
    let free = RulesConfig {
        hero_choices: 4,
        ..RulesConfig::default()
    };
    let banned = RulesConfig {
        hero_bans: bans.iter().map(|s| s.to_string()).collect(),
        ..free.clone()
    };
    banned.validate().unwrap();
    let (mut seen, mut all) = (0, 0);
    for seed in 0..40 {
        seen += offered(&free, seed)
            .iter()
            .filter(|id| bans.contains(&id.as_str()))
            .count();
        let ids = offered(&banned, seed);
        assert!(
            ids.iter().all(|id| !bans.contains(&id.as_str())),
            "seed {seed}"
        );
        all += ids.len();
    }
    assert!(seen > 0, "without the list they are offered");
    assert!(all > 40 * 32, "every seat still has its four offers");
    // Not a hero: refused. Empty: the config is written as before.
    let bad = RulesConfig {
        hero_bans: vec!["BG28_810".into()],
        ..RulesConfig::default()
    };
    assert!(bad.validate().is_err());
    let json = serde_json::to_string(&RulesConfig::default()).unwrap();
    assert!(!json.contains("hero_bans"));
}

#[test]
fn faelins_three_picks_wait_in_the_hand_until_their_tier() {
    // The picks are hand cards, locked, from the start.
    let mut g = game(9990);
    g.players[0].hand.clear();
    g.set_hero_power(0, 0, cid("BG22_HERO_201p"));
    while g.players[0].pending.last().is_some() {
        g.apply(0, Action::Choose(0)).unwrap();
    }
    let tiers = |g: &Game, held: bool| -> Vec<u8> {
        let mut v: Vec<u8> = g.players[0]
            .hand
            .iter()
            .filter(|h| h.held == held)
            .map(|h| db().get(h.item.card()).tier)
            .collect();
        v.sort();
        v
    };
    assert_eq!(tiers(&g, true), vec![2, 4, 6]);
    assert!(g.players[0].hand.iter().all(|h| h.locked == 1));
    // Reaching Tier 2 lets go of the Tier 2 minion only.
    g.players[0].gold = 20;
    g.apply(0, Action::Upgrade).unwrap();
    assert_eq!(g.players[0].tier, 2);
    assert_eq!(tiers(&g, true), vec![4, 6]);
    assert_eq!(tiers(&g, false), vec![2]);
    assert_eq!(g.players[0].hand.len(), 3, "no card is added: it was there");
    let free = g.players[0].hand.iter().position(|h| !h.held).unwrap();
    assert_eq!(g.players[0].hand[free].locked, 0);
    g.apply(0, Action::Play { hand: free, pos: 0 }).unwrap();
}

#[test]
fn sindragosa_has_no_freeze_and_her_tavern_is_frozen_for_the_end_of_turn() {
    // Thawed Champion ("at the end of your turn, make a random Frozen minion in the Tavern Golden")
    // found nothing frozen: the Tavern froze after it acted.
    let mut g = game(9991);
    g.cfg.triple_copies = 99;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_014"));
    assert!(!g.can_freeze(0));
    assert!(g.apply(0, Action::Freeze).is_err(), "no Freeze to press");
    assert!(!g.legal_mask(0)[taverngym::actions::A_FREEZE]);
    set_board(&mut g, 0, &["TB_BaconShop_HERO_27_Buddy"]);
    g.players[0].shop.clear();
    let mut sids = vec![];
    for _ in 0..2 {
        let m = minion(&mut g, 0, "BG36_345");
        sids.push(m.eid);
        g.players[0]
            .shop
            .push(taverngym::state::ShopSlot::new(m.eid, Item::Minion(m)));
    }
    end_turn_all(&mut g);
    let kept: Vec<bool> = g.players[0]
        .shop
        .iter()
        .filter(|s| sids.contains(&s.sid))
        .map(|s| s.item.as_minion().is_some_and(|m| m.golden))
        .collect();
    assert_eq!(kept.len(), 2, "the frozen Tavern is kept");
    assert_eq!(
        kept.iter().filter(|g| **g).count(),
        1,
        "one of them is Golden"
    );
}

#[test]
fn a_reborn_eternal_knight_has_its_bonus_on_top_of_one_health() {
    // The bonus is an aura wherever the Knight is, the Reborn one included.
    let mut g = game(9992);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG25_008", "BG25_008"]);
    g.players[0].board[0].give(Kw::REBORN);
    set_board(&mut g, 1, &["BG36_345"]);
    let mut c = combat_ctx(&mut g);
    let first = c.boards[0][0].eid;
    c.destroy(first);
    c.resolve_deaths();
    let stats: Vec<(i32, i32)> = c.boards[0].iter().map(|m| (m.atk, m.hp)).collect();
    // One Knight died: +4/+2. The Reborn one: 4 + 4 Attack, 1 + 2 Health.
    assert_eq!(stats, vec![(8, 3), (8, 4)]);
}

#[test]
fn terons_copy_returns_where_the_minion_stood() {
    // Not at the far right. With a Deathrattle that summons, the copy is right of those minions (as
    // a Reborn one is) and left of its old right neighbour.
    for (target, expect) in [
        ("BG26_963", vec!["BG36_345", "BG26_963", "BG36_345"]),
        // Harmless Bonehead: Deathrattle: summon two 1/1 Skeletons.
        (
            "BG28_300",
            vec!["BG36_345", "SKEL", "SKEL", "BG28_300", "BG36_345"],
        ),
    ] {
        let mut g = game(9993);
        g.cfg.record_log = true;
        g.cfg.triple_copies = 99;
        let teron = cid("BG25_HERO_103p");
        g.set_hero_power(0, 0, teron);
        set_board(&mut g, 0, &["BG36_345", target, "BG36_345"]);
        set_board(&mut g, 1, &["BG36_345"]);
        g.players[0].gold = 10;
        g.apply(0, Action::HeroPower(0)).unwrap();
        let eid = g.players[0].board[1].eid;
        choose_target(&mut g, 0, eid);
        g.simulate_combat(0, Some(1), None);
        let after: Vec<String> = g.last_frames[1].boards[0]
            .iter()
            .map(|m| match m.name() {
                "Skeleton" => "SKEL".to_string(),
                _ => db().get(db().normal_of(m.card)).id.clone(),
            })
            .collect();
        let expect: Vec<String> = expect.iter().map(|x| x.to_string()).collect();
        assert_eq!(after, expect, "{target}");
    }
}

#[test]
fn cast_a_spell_on_this_means_a_spell_you_aimed_at_it() {
    // Only a spell played from hand and aimed at the minion triggers it. Victorious Geomant's Blood
    // Gems that do not fit are cast on the left-most minion by the effect: 4 casts on the
    // Distractor, no trigger.
    let tavern = |g: &Game| (g.players[0].stats.tavern_atk, g.players[0].stats.tavern_hp);
    let mut g = game(9995);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG36_762", "BG36_370"]);
    for _ in 0..8 {
        give(&mut g, 0, "BG28_897");
    }
    g.apply(0, Action::Activate(1)).unwrap();
    assert_eq!(g.players[0].board[0].gems, 4, "the Gems did land on it");
    assert_eq!(tavern(&g), (0, 0), "but nobody aimed them");
    // A Blood Gem played from hand on it: +1/+2 for the Tavern.
    cast_on(&mut g, 0, 0);
    assert_eq!(tavern(&g), (1, 2));
    // Aimed at another minion: nothing.
    cast_on(&mut g, 0, 1);
    assert_eq!(tavern(&g), (1, 2));
    // Balinda casts the aimed spell twice: two triggers.
    let mut g = game(9996);
    g.cfg.triple_copies = 99;
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG36_762", "BG35_883"]);
    give(&mut g, 0, "BG20_GEM");
    cast_on(&mut g, 0, 0);
    assert_eq!(tavern(&g), (2, 4));
    // Gatekeeper Amalgam has the same words: effect-made casts do not make it cast Tea Set.
    let mut g = game(9997);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG36_640", "BG36_370"]);
    for _ in 0..8 {
        give(&mut g, 0, "BG28_897");
    }
    let other = g.players[0].board[1].atk;
    g.apply(0, Action::Activate(1)).unwrap();
    assert_eq!(g.players[0].board[0].gems, 4);
    assert_eq!(g.players[0].board[1].atk, other, "no Tea Set was cast");
}

#[test]
fn triples_use_the_copies_owned_longest() {
    // One copy on the board, one in hand, and a Discover brings the third while Sinstone Sticker
    // adds the fourth. The triple takes the three owned first (board, hand, the pick); the extra
    // copy stays in hand.
    const GUARD: &str = "BG36_345";
    let mut g = game(8611);
    buy_trinket(&mut g, 0, "BG30_MagicItem_801");
    g.players[0].hand.clear();
    g.universe = vec![cid(GUARD)];
    g.pool.put(cid(GUARD), 10);
    set_board(&mut g, 0, &[GUARD]);
    give(&mut g, 0, GUARD);
    let owned = [g.players[0].board[0].eid, g.players[0].hand[0].hid];
    g.with_recruit(0, |c| {
        c.discover_minion(0, cid("BG30_MagicItem_801"), 3, |d| d.id == GUARD)
    });
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].board.is_empty(), "the board copy is used");
    let mut hand = hand_ids(&g, 0);
    hand.sort();
    assert_eq!(hand, vec![GUARD.to_string(), format!("{GUARD}_G")]);
    let left = g.players[0]
        .hand
        .iter()
        .find(|h| h.item.card() == cid(GUARD))
        .unwrap()
        .hid;
    assert!(
        owned.iter().all(|e| left > *e),
        "the newest copy is the one left"
    );

    // The order is by age, not by place: here the hand copy is older than the board copy, and
    // two more arrive in one step.
    let mut g = game(8612);
    g.players[0].hand.clear();
    give(&mut g, 0, GUARD);
    set_board(&mut g, 0, &[GUARD]);
    g.with_recruit(0, |c| {
        c.no_triples = true;
        let _ = c.give_card(0, cid(GUARD));
        let _ = c.give_card(0, cid(GUARD));
        c.no_triples = false;
        c.check_triples(0);
    });
    assert!(g.players[0].board.is_empty());
    let hand = hand_ids(&g, 0);
    assert_eq!(hand, vec![GUARD.to_string(), format!("{GUARD}_G")]);
}

#[test]
fn ashen_corruptor_bonus_stays_in_the_tavern_for_the_turn() {
    // "give minions in the Tavern +2/+2 this turn" also reaches the minions that enter the Tavern
    // later in the turn. Before, a Health Refresh put it on the Tavern being replaced and the new
    // one had nothing. Golden: +4/+4.
    let temps = |g: &Game| -> Vec<(i32, i32)> {
        g.players[0]
            .shop
            .iter()
            .filter_map(|s| s.item.as_minion())
            .map(|m| (m.temp_atk, m.temp_hp))
            .collect()
    };
    for (corruptor, k) in [("BG32_873", 2), ("BG32_873_G", 4)] {
        let mut g = game(8613);
        g.players[0].gold = 0;
        g.players[0].free_refreshes = 0;
        set_board(&mut g, 0, &["BG26_524", corruptor]);
        let life = (g.players[0].health, g.players[0].armor);
        // Two Refreshes paid with Health: each adds to the turn's bonus.
        for n in 1..=2 {
            g.apply(0, Action::Refresh).unwrap();
            let t = temps(&g);
            assert!(!t.is_empty());
            assert!(t.iter().all(|x| *x == (n * k, n * k)), "{corruptor} {t:?}");
            assert_eq!((g.players[0].health, g.players[0].armor), life);
        }
        // A Refresh paid with Gold deals no damage; the new minions still have the bonus.
        assert_eq!(g.players[0].board[0].counter, 0);
        g.players[0].gold = 1;
        g.apply(0, Action::Refresh).unwrap();
        assert_eq!(g.players[0].gold, 0);
        let t = temps(&g);
        assert!(
            !t.is_empty() && t.iter().all(|x| *x == (2 * k, 2 * k)),
            "{t:?}"
        );
        // "This turn": gone from the frozen minions and from new ones next turn.
        g.apply(0, Action::Freeze).unwrap();
        end_turn_all(&mut g);
        assert_eq!(g.turn, 2);
        let t = temps(&g);
        assert!(!t.is_empty() && t.iter().all(|x| *x == (0, 0)), "{t:?}");
        g.with_recruit(0, |c| c.refresh_shop(0));
        let t = temps(&g);
        assert!(!t.is_empty() && t.iter().all(|x| *x == (0, 0)), "{t:?}");
    }
}

#[test]
fn sacrificial_wrathguard_bonus_from_combat_reaches_the_tavern() {
    // "Deathrattle: Give minions in the Tavern +2/+2 this game": dying in combat, it buffs
    // the minions kept in the Tavern and every minion of the next Taverns.
    let mut g = game(8614);
    set_board(&mut g, 0, &["BG36_362"]);
    for p in 1..8 {
        set_board(&mut g, p, &["BG36_345"]);
        let m = &mut g.players[p].board[0];
        (m.atk, m.hp, m.max_hp) = (50, 50, 50);
    }
    g.apply(0, Action::Freeze).unwrap();
    let kept: Vec<(u32, i32, i32)> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion().map(|m| (s.sid, m.atk, m.max_hp)))
        .collect();
    assert!(!kept.is_empty());
    end_turn_all(&mut g);
    assert_eq!(g.turn, 2);
    let s = &g.players[0].stats;
    assert_eq!((s.tavern_atk, s.tavern_hp), (2, 2));
    for (sid, a, h) in kept {
        let slot = g.players[0].shop.iter().find(|s| s.sid == sid).unwrap();
        let m = slot.item.as_minion().unwrap();
        assert_eq!((m.atk, m.max_hp), (a + 2, h + 2));
    }
    g.with_recruit(0, |c| c.refresh_shop(0));
    for m in g.players[0].shop.iter().filter_map(|s| s.item.as_minion()) {
        let d = db().get(m.card);
        assert_eq!((m.atk, m.max_hp), (d.atk + 2, d.hp + 2), "{}", d.id);
    }
}

#[test]
fn power_of_the_storm_belongs_to_whoever_holds_it() {
    // Master Nguyen's power got by another hero (Sir Finley, a power swap, Reinvigorating Light)
    // offers two Hero Powers at the start of every turn; it only did so for Master Nguyen himself.
    let storm = cid("BG20_HERO_202p");
    let offer = |g: &Game| match g.players[0].pending.last() {
        Some(Pending::HeroPowerChoice { options, slot }) => Some((options.len(), *slot)),
        _ => None,
    };
    // As a second Hero Power (Reinvigorating Light), got on Turn 3: nothing this turn, then
    // a choice for that slot at every turn start.
    let mut g = game(9601);
    end_turn_all(&mut g);
    end_turn_all(&mut g);
    assert_eq!(g.turn, 3);
    g.set_hero_power(0, 1, storm);
    assert_eq!(offer(&g), None, "it starts with the next turn");
    for turn in 4..=6 {
        end_turn_all(&mut g);
        assert_eq!(g.turn, turn);
        assert_eq!(offer(&g), Some((2, 1)), "turn {turn}");
        let first = g.players[0].powers[0].card;
        g.apply(0, Action::Choose(0)).unwrap();
        let pl = &g.players[0];
        assert_eq!(pl.powers.len(), 2);
        assert_eq!(pl.powers[0].card, first, "the other power is not touched");
        assert_ne!(
            pl.powers[1].card, storm,
            "the Storm is not among its own offers"
        );
    }
    // In the first slot on Turn 1 (Finley's Discover is answered after the turn began): the
    // choice of that turn is made at once, and again every turn.
    let mut g = game(9602);
    g.set_hero_power(0, 0, storm);
    assert_eq!(offer(&g), Some((2, 0)));
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(offer(&g), None, "once a turn");
    end_turn_all(&mut g);
    assert_eq!(offer(&g), Some((2, 0)));
}

#[test]
fn lockbox_never_gives_aureate_laureate() {
    // Aureate Laureate ("always Golden") is the only minion with a type here: the Lockbox gives
    // nothing rather than it.
    let mut g = game(9603);
    g.players[0].hand.clear();
    g.universe = vec![cid("BG32_236")];
    g.with_recruit(0, |c| taverngym::content::open_lockbox(c, 0));
    assert!(g.players[0].hand.is_empty());
    // With another typed minion it is always that one.
    g.universe = vec![cid("BG32_236"), cid("BGS_004")];
    for _ in 0..20 {
        g.players[0].hand.clear();
        g.with_recruit(0, |c| taverngym::content::open_lockbox(c, 0));
        assert_eq!(hand_ids(&g, 0), vec!["TB_BaconUps_079"]);
    }
}

#[test]
fn refresh_buffs_of_every_kind_land_on_one_minion() {
    // "after the Tavern is Refreshed, give a random minion in it +X/+X" - Easterly Winds (+9/+9),
    // Waveling (+4/+4) and En-Djinn Blazer (+10/+10) put their buffs on the same minion of a
    // Refresh.
    for seed in 9604..9610 {
        let mut g = game(seed);
        g.players[0].tier = 4;
        g.with_recruit(0, |c| {
            for id in ["BG34_444", "BG34_856", "BG34_865"] {
                c.add_player_effect(0, cid(id), 1);
            }
            c.add_player_effect(0, cid("BG34_856"), 1); // a second Waveling
        });
        for _ in 0..3 {
            g.players[0].gold = 1;
            g.apply(0, Action::Refresh).unwrap();
            let mut gains: Vec<(i32, i32)> = g.players[0]
                .shop
                .iter()
                .filter_map(|s| s.item.as_minion())
                .map(|m| {
                    let d = db().get(m.card);
                    (m.atk - d.atk, m.max_hp - d.hp)
                })
                .filter(|x| *x != (0, 0))
                .collect();
            gains.sort();
            assert_eq!(gains, vec![(9 + 4 + 4 + 10, 9 + 4 + 4 + 10)], "seed {seed}");
        }
    }
}

#[test]
fn jaraxxus_counts_every_point_his_minions_deal() {
    // Bloodfury ("after friendly minions deal 150 damage") was slow. A minion that dies in the
    // exchange still dealt its damage - attacking or struck back.
    let bloodfury = cid("TB_BaconShop_HP_036");
    let mut g = game(9611);
    g.set_hero_power(0, 0, bloodfury);
    set_board(&mut g, 0, &["BG36_345"]);
    (g.players[0].board[0].atk, g.players[0].board[0].hp) = (5, 1);
    for p in 1..8 {
        set_board(&mut g, p, &["BG36_345"]);
        let m = &mut g.players[p].board[0];
        (m.atk, m.hp, m.max_hp) = (3, 100, 100);
    }
    end_turn_all(&mut g);
    assert_eq!(g.players[0].powers[0].counter, 5, "dealt 5 and died");
    // Damage its minions' effects deal to its own hero counts, rewound or not.
    let mut g = game(9612);
    g.set_hero_power(0, 0, bloodfury);
    set_board(&mut g, 0, &["BGS_004", "BG26_174"]); // Wrath Weaver, Soul Rewinder
    let life = g.players[0].health + g.players[0].armor;
    give(&mut g, 0, "BGS_004");
    play_last(&mut g, 0);
    assert_eq!(g.players[0].health + g.players[0].armor, life, "rewound");
    assert_eq!(g.players[0].powers[0].counter, 1);
    // A Refresh paid with Health through Malchezaar counts too.
    let mut g = game(9613);
    g.set_hero_power(0, 0, bloodfury);
    set_board(&mut g, 0, &["BG26_524"]);
    g.players[0].gold = 0;
    g.players[0].free_refreshes = 0;
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(g.players[0].powers[0].counter, 1);
}

#[test]
fn held_cards_unlock_after_the_hero_power_is_replaced() {
    // The lock goes with the hand card, not with the Hero Power. Faelin swaps his power early: the
    // picks still unlock at their Tiers.
    let other = cid("TB_BaconShop_HP_035");
    let mut g = game(9614);
    g.players[0].hand.clear();
    g.set_hero_power(0, 0, cid("BG22_HERO_201p"));
    while g.players[0].pending.last().is_some() {
        g.apply(0, Action::Choose(0)).unwrap();
    }
    g.set_hero_power(0, 0, other);
    let held = |g: &Game| -> Vec<u8> {
        let mut v: Vec<u8> = g.players[0]
            .hand
            .iter()
            .filter(|h| h.held)
            .map(|h| db().get(h.item.card()).tier)
            .collect();
        v.sort();
        v
    };
    assert_eq!(held(&g), vec![2, 4, 6]);
    for (tier, left) in [
        (2u8, vec![4u8, 6]),
        (3, vec![4, 6]),
        (4, vec![6]),
        (5, vec![6]),
        (6, vec![]),
    ] {
        g.players[0].gold = 20;
        g.apply(0, Action::Upgrade).unwrap();
        assert_eq!(g.players[0].tier, tier);
        assert_eq!(held(&g), left, "tier {tier}");
    }
    assert_eq!(g.players[0].hand.len(), 3, "the same three cards");
    assert!(g.players[0].hand.iter().all(|h| h.locked == 0));
    // Thorim swaps his power after 25 Gold: the Tier 7 minion unlocks at 60 all the same.
    let mut g = game(9615);
    g.players[0].hand.clear();
    g.set_hero_power(0, 0, cid("BG27_HERO_801p2"));
    g.apply(0, Action::Choose(0)).unwrap();
    let spend = |g: &mut Game, n: i32| {
        g.with_recruit(0, |c| {
            c.gain_gold(0, n);
            c.spend_gold(0, n);
        })
    };
    spend(&mut g, 25);
    g.set_hero_power(0, 0, other);
    spend(&mut g, 34);
    assert!(g.players[0].hand[0].held, "59 of 60");
    spend(&mut g, 1);
    assert_eq!(g.players[0].hand.len(), 1);
    let h = &g.players[0].hand[0];
    assert!(!h.held && h.locked == 0 && db().get(h.item.card()).pools.tier7);
    // With the power kept, the count on it still reaches "0 left" and no second copy comes.
    let mut g = game(9616);
    g.players[0].hand.clear();
    g.set_hero_power(0, 0, cid("BG27_HERO_801p2"));
    g.apply(0, Action::Choose(0)).unwrap();
    spend(&mut g, 59);
    assert!(g.players[0].hand[0].held);
    spend(&mut g, 5);
    assert_eq!(g.players[0].hand.len(), 1);
    assert!(!g.players[0].hand[0].held);
    assert_eq!(g.players[0].powers[0].counter, 60);
}

#[test]
fn boon_of_beetles_taunt_is_respected() {
    // Not reproduced - pinned here. The Beetles are attacked before anything else, when they are
    // summoned at the start of combat and when they arrive later because the board was full.
    for full in [false, true] {
        let mut g = game(9617);
        g.cfg.record_log = true;
        g.cfg.triple_copies = 99;
        g.players[0].hand.clear();
        g.players[0].gold = 10;
        set_board(
            &mut g,
            0,
            if full {
                &["BG36_345"; 7]
            } else {
                &["BG36_345"; 3]
            },
        );
        set_board(&mut g, 1, &["BG36_345"; 5]);
        for m in g.players[1].board.iter_mut() {
            (m.atk, m.hp, m.max_hp) = (5, 60, 60);
        }
        give(&mut g, 0, "BG28_603");
        play_last(&mut g, 0);
        let from = g.log.len();
        g.simulate_combat(0, Some(1), None);
        let log = &g.log[from..];
        // Every attack of the enemy made while a Beetle stands is on a Beetle.
        let mut beetles = 0;
        let mut on_beetles = 0;
        for l in log {
            if l.contains("p0 summons Beetle") {
                beetles += 1;
            } else if l.contains("p0's Beetle dies") {
                beetles -= 1;
            } else if l.contains("p1's") && l.contains(" attacks ") && beetles > 0 {
                assert!(l.ends_with("attacks Beetle"), "full board {full}: {l}");
                on_beetles += 1;
            }
        }
        assert_eq!(
            on_beetles, 2,
            "full board {full}: both Beetles drew an attack"
        );
    }
}

#[test]
fn every_numbered_tavern_spell_takes_the_tavern_spell_bonus() {
    // The numbers of a Tavern spell take "your Tavern spells give an extra +a/+h" - both parts,
    // also on a spell that names only Attack or only Health, on a "set to" and on Butchering. Here
    // the bonus is +2/+1.
    let with_bonus = |seed: u64| {
        let mut g = game(seed);
        g.cfg.triple_copies = 99;
        g.players[0].hand.clear();
        g.players[0].stats.spell_atk = 2;
        g.players[0].stats.spell_hp = 1;
        g.players[0].gold = 10;
        g
    };
    let stat = |g: &Game| (g.players[0].board[0].atk, g.players[0].board[0].max_hp);
    // Fortify: "+3 Health and Taunt".
    let mut g = with_bonus(9620);
    set_board(&mut g, 0, &["BG36_345"]);
    give(&mut g, 0, "BG28_503");
    cast_on(&mut g, 0, 0);
    assert_eq!(stat(&g), (3 + 2, 3 + 3 + 1));
    // Healthy / Hostile Bounty: "+4 Health" / "+4 Attack" to four friendly minions.
    for (bounty, a, h) in [("BG33_811", 0, 4), ("BG33_812", 4, 0)] {
        let mut g = with_bonus(9621);
        set_board(&mut g, 0, &["BG36_345"]);
        give(&mut g, 0, bounty);
        play_last(&mut g, 0);
        assert_eq!(stat(&g), (3 + a + 2, 3 + h + 1), "{bounty}");
    }
    // Perfect Vision: "set a minion's stats to 20/20".
    let mut g = with_bonus(9622);
    set_board(&mut g, 0, &["BG36_345"]);
    give(&mut g, 0, "BG28_838");
    cast_on(&mut g, 0, 0);
    assert_eq!(stat(&g), (22, 21));
    // Butchering: "your Undead have +8 Attack this game".
    let mut g = with_bonus(9623);
    set_board(&mut g, 0, &["BG25_008", "BG25_008"]); // two Eternal Knights
    give(&mut g, 0, "BG28_604");
    cast_on(&mut g, 0, 1);
    assert_eq!(g.players[0].stats.undead_atk, 8 + 2);
    // Energizing Chamber: "give your Deity +7/+7".
    let mut g = with_bonus(9624);
    g.players[0].deity = Some(taverngym::state::DeityState {
        card: cid("BGFYM_000"),
        atk: 5,
        hp: 5,
        golden: false,
        reborn: false,
    });
    give(&mut g, 0, "BG36_371");
    play_last(&mut g, 0);
    let d = g.players[0].deity.as_ref().unwrap();
    assert_eq!((d.atk, d.hp), (5 + 7 + 2, 5 + 7 + 1));
}

#[test]
fn a_set_to_on_a_tavern_minion_keeps_the_tavern_buff() {
    // Perfect Vision with a +9/+5 Tavern-spell bonus on a Tavern minion with the Tavern's +5/+4
    // ("minions in the Tavern have... this game"): the client set it to 29/25 and showed 34/29, its
    // BG_ShopBuff_Ench recomputed on top.
    for golden in [false, true] {
        let mut g = game(9631);
        g.cfg.triple_copies = 99;
        g.players[0].hand.clear();
        g.players[0].gold = 10;
        g.players[0].stats.spell_atk = 9;
        g.players[0].stats.spell_hp = 5;
        g.players[0].stats.tavern_atk = 5;
        g.players[0].stats.tavern_hp = 4;
        set_board(&mut g, 0, &["BG36_345"]);
        if golden {
            let m = g.players[0].shop[0].item.as_minion_mut().unwrap();
            m.card = db().golden_of(m.card);
            m.golden = true;
        }
        let target = g.players[0].shop[0].sid;
        give(&mut g, 0, "BG28_838");
        play_last(&mut g, 0);
        let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
            panic!("expected target")
        };
        let i = options.iter().position(|e| *e == target).unwrap();
        g.apply(0, Action::Choose(i)).unwrap();
        let m = g.players[0].shop[0].item.as_minion().unwrap();
        assert_eq!((m.atk, m.max_hp), (34, 29), "golden {golden}");
        // Bought, it keeps them, as every Tavern buff.
        g.apply(0, Action::Buy(0)).unwrap();
        let m = g.players[0].hand.last().unwrap().item.as_minion().unwrap();
        assert_eq!((m.atk, m.max_hp), (34, 29), "golden {golden}");
    }
    // On the board there is no Tavern buff to keep.
    let mut g = game(9632);
    g.players[0].hand.clear();
    g.players[0].gold = 10;
    g.players[0].stats.spell_atk = 9;
    g.players[0].stats.spell_hp = 5;
    g.players[0].stats.tavern_atk = 5;
    g.players[0].stats.tavern_hp = 4;
    set_board(&mut g, 0, &["BG36_345"]);
    give(&mut g, 0, "BG28_838");
    cast_on(&mut g, 0, 0);
    let m = &g.players[0].board[0];
    assert_eq!((m.atk, m.max_hp), (29, 25));
}

#[test]
fn golden_minions_played_count_for_the_whole_game() {
    // The count is of every Golden minion played this game - before the reader was owned too, and
    // the always Golden Aureate Laureate counts. Checked, not changed.
    let mut g = game(9701);
    g.players[0].hand.clear();
    g.players[0].tier = 6;
    g.players[0].gold = 50;
    set_board(&mut g, 0, &[]);
    give(&mut g, 0, "BG36_524"); // Maritime Extortionist 7/7 stays in the hand
    let extortionist = |g: &Game| {
        let m = g.players[0].hand[0].item.as_minion().unwrap();
        (m.atk, m.hp)
    };
    assert_eq!(extortionist(&g), (7, 7));
    give(&mut g, 0, "BG32_236"); // Aureate Laureate
    play_last(&mut g, 0);
    assert_eq!(g.players[0].stats.golden_played, 1);
    assert_eq!(extortionist(&g), (14, 14), "wherever it is");
    give(&mut g, 0, "BG36_345_G");
    play_last(&mut g, 0);
    assert_eq!(g.players[0].stats.golden_played, 2);
    assert_eq!(extortionist(&g), (21, 21));
    // Hooktusk arrives afterwards: its "+1/+1" is improved by the two played before it.
    set_board(&mut g, 0, &["BG36_344", "BG36_523"]); // Hooktusk, Enterprising Escapee
    let before = stats(&g, 0)[1];
    let reward = g.players[0]
        .hand
        .iter()
        .position(|h| h.item.as_minion().is_none())
        .expect("the Golden minion's Triple Reward");
    g.apply(
        0,
        Action::Play {
            hand: reward,
            pos: 0,
        },
    )
    .unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(stats(&g, 0)[1], (before.0 + 3, before.1 + 3));
    assert_eq!(stats(&g, 0)[0], (4, 4), "not Hooktusk itself");
}

#[test]
fn tasty_lobster_improvement_from_combat_is_kept_for_the_game() {
    // The Lobster's growth is the player's for the whole game, and Deathrattles triggered in combat count towards it.
    let mut g = game(9801);
    g.cfg.triple_copies = 99;
    // Three Lobsters and a sturdy Hyena (its Rally summons a fourth); every other seat has
    // one minion that kills them all.
    set_board(
        &mut g,
        0,
        &["BG36_202", "BG36_202", "BG36_202_G", "BG36_210"],
    );
    {
        let m = &mut g.players[0].board[3];
        (m.atk, m.hp, m.max_hp) = (1, 400, 400);
    }
    for p in 1..8 {
        set_board(&mut g, p, &["BG36_345"]);
        let m = &mut g.players[p].board[0];
        (m.atk, m.hp, m.max_hp) = (30, 60, 60);
    }
    end_turn_all(&mut g);
    assert_eq!(g.turn, 2);
    assert_eq!(
        g.players[0].stats.lobster_bonus, 4,
        "four Lobsters died in the combat"
    );
    // What they gave in combat stayed there; the improvement did not.
    assert_eq!(stats(&g, 0)[..3], [(2, 1), (2, 1), (4, 2)]);
    // The next Lobster's Deathrattle, in the Tavern: +2/+1 improved four times, for good.
    set_board(&mut g, 0, &["BG36_202", "BG36_210"]);
    g.with_recruit(0, |c| {
        let e = c.boards[0][0].eid;
        c.destroy(e);
        c.resolve_deaths();
    });
    assert_eq!(stats(&g, 0), vec![(4 + 2 * 5, 6 + 5)]);
    assert_eq!(g.players[0].stats.lobster_bonus, 5);
}

#[test]
fn a_triple_keeps_improve_this_growth() {
    // Two Spark Snappers whose Satellites had grown to 8/12 and 12/18 merged into a Golden at its
    // base 4/6. The growth of the parts adds up on the Golden: 4/6 + 6/9 + 10/15 = 20/30, and a
    // further improvement adds the Golden's 4/6.
    const SNAPPER: &str = "BG36_851";
    let mut g = game(9901);
    g.players[0].hand.clear();
    g.players[0].gold = 10;
    set_board(&mut g, 0, &[SNAPPER, SNAPPER]);
    g.players[0].board[0].counter = 3; // 2/3 x 4 = 8/12
    g.players[0].board[1].counter = 5; // 2/3 x 6 = 12/18
    give(&mut g, 0, SNAPPER);
    assert!(g.players[0].board.is_empty(), "tripled");
    let golden = g.players[0].hand[0].item.as_minion().unwrap().clone();
    assert!(golden.golden);
    assert_eq!(golden.counter, 8);
    // Played, then a Mech: its Satellite is 20/30 and the next one 24/36.
    play_last(&mut g, 0);
    while !g.players[0].pending.is_empty() {
        g.apply(0, Action::Choose(0)).unwrap();
    }
    g.players[0].hand.clear();
    for want in [(20, 30), (24, 36)] {
        give(&mut g, 0, "BG36_345"); // not a Mech: nothing
        let i = g.players[0].hand.len() - 1;
        g.apply(0, Action::Play { hand: i, pos: 1 }).unwrap();
        let mech = minion(&mut g, 0, "BG21_022"); // a plain Mech in hand
        let (a, h) = (mech.atk, mech.hp);
        g.with_recruit(0, |c| {
            c.add_to_hand(0, Item::Minion(mech));
        });
        let i = g.players[0].hand.len() - 1;
        g.apply(0, Action::Play { hand: i, pos: 1 }).unwrap();
        let m = &g.players[0].board[1];
        assert_eq!((m.atk - a, m.max_hp - h), want);
        g.players[0].board.truncate(1);
    }

    // The same rule for the other "improve this" minions of the pool; a card that never
    // merged behaves as before (normal: base x (1 + improvements), Golden: twice that).
    for id in ["BG35_602", "BG36_362", "BG36_364", "BG31_810", "BGFYM_005"] {
        let mut g = game(9902);
        g.players[0].hand.clear();
        set_board(&mut g, 0, &[id, id]);
        g.players[0].board[0].counter = 2;
        g.players[0].board[1].counter = 1;
        give(&mut g, 0, id);
        let m = g.players[0].hand[0].item.as_minion().unwrap();
        assert!(m.golden, "{id}");
        assert_eq!(m.counter, 3, "{id}");
    }
    // Sacrificial Wrathguard, Golden, improved once by its own Activate: 4/4 + 4/4.
    let mut g = game(9903);
    g.players[0].gold = 5;
    set_board(&mut g, 0, &["BG36_362_G"]);
    g.apply(0, Action::Activate(0)).unwrap();
    assert_eq!(
        g.players[0].board[0].counter, 2,
        "a Golden's improvement is two steps"
    );
    g.with_recruit(0, |c| {
        let e = c.boards[0][0].eid;
        c.destroy(e);
        c.resolve_deaths();
    });
    assert_eq!(g.players[0].stats.tavern_atk, 8);
    // Patient Scout: the Golden keeps the highest Tier among its parts.
    let mut g = game(9904);
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG24_715", "BG24_715"]);
    g.players[0].board[0].counter = 2;
    g.players[0].board[1].counter = 3;
    give(&mut g, 0, "BG24_715");
    assert_eq!(g.players[0].hand[0].item.as_minion().unwrap().counter, 3);
}

#[test]
fn jaraxxus_and_other_card_rules() {
    // Jaraxxus: a Refresh paid with Health through Malchezaar is damage the minion deals.
    let mut g = game(9905);
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_036"));
    set_board(&mut g, 0, &["BG26_524"]);
    g.players[0].gold = 0;
    g.players[0].free_refreshes = 0;
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(g.players[0].powers[0].counter, 1);

    // Butchering with a Tavern-spell bonus of +2/+1: Undead get 8 + 2 Attack and 1 Health
    // this game, wherever they are.
    let mut g = game(9906);
    g.cfg.triple_copies = 99;
    g.players[0].hand.clear();
    g.players[0].stats.spell_atk = 2;
    g.players[0].stats.spell_hp = 1;
    set_board(&mut g, 0, &["BG25_011", "BG25_011"]);
    let before = stats(&g, 0)[0];
    give(&mut g, 0, "BG28_604");
    cast_on(&mut g, 0, 1);
    let st = &g.players[0].stats;
    assert_eq!((st.undead_atk, st.undead_hp), (10, 1));
    assert_eq!(stats(&g, 0)[0], (before.0 + 10, before.1 + 1));
    give(&mut g, 0, "BG25_011");
    let m = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!(
        (m.atk - before.0, m.hp - before.1),
        (10, 1),
        "in the hand too"
    );

    // Power of the Storm ends when a power swap replaces the slot it sits in; the turn's
    // own choice does not end it.
    let storm = cid("BG20_HERO_202p");
    let offer = |g: &Game| match g.players[0].pending.last() {
        Some(Pending::HeroPowerChoice { options, .. }) => options.len(),
        _ => 0,
    };
    let mut g = game(9907);
    g.set_hero_power(0, 0, storm);
    assert_eq!(offer(&g), 2);
    g.apply(0, Action::Choose(0)).unwrap();
    end_turn_all(&mut g);
    assert_eq!(offer(&g), 2, "the Storm is still underneath");
    g.apply(0, Action::Choose(0)).unwrap();
    // Unmasked Identity: "Discover a new Hero Power".
    g.players[0].hand.clear();
    give(&mut g, 0, "EBG_Spell_037");
    play_last(&mut g, 0);
    assert_eq!(offer(&g), 3);
    g.apply(0, Action::Choose(0)).unwrap();
    let kept = g.players[0].powers[0].card;
    end_turn_all(&mut g);
    assert_eq!(offer(&g), 0, "no longer Master Nguyen's power");
    assert_eq!(g.players[0].powers[0].card, kept);

    // The Gryphon hands out Beasts (all-type minions count) but never a Gryphon.
    let mut g = game(9908);
    g.players[0].tier = 6;
    g.universe = vec![cid("BG36_204"), cid("BG36_210"), cid("BG32_111")];
    for seed in 0..40 {
        g.players[0].hand.clear();
        set_board(
            &mut g,
            0,
            &[if seed % 2 == 0 {
                "BG36_204"
            } else {
                "BG36_204_G"
            }],
        );
        g.with_recruit(0, |c| {
            let e = c.boards[0][0].eid;
            c.trigger_rally(0, e, 0);
        });
        let got = hand_ids(&g, 0);
        assert!(!got.is_empty());
        assert!(
            got.iter().all(|id| id == "BG36_210" || id == "BG32_111"),
            "{got:?}"
        );
    }

    // A hand card costs nothing to play and its cost input is 0, as on the client.
    let mut g = game(9909);
    g.players[0].hand.clear();
    g.players[0].gold = 0;
    set_board(&mut g, 0, &["BG36_345"]);
    give(&mut g, 0, "BG28_825"); // Defender's Rites, bought for 2 Gold in the Tavern
    cast_on(&mut g, 0, 0);
    assert_eq!(g.players[0].gold, 0);
    assert!(g.players[0].board[0].has(Kw::TAUNT));
}

#[test]
fn a_tripled_magnet_pile_returns_to_the_hand_and_is_magnetized_again() {
    // A Mech line: Magnetic minions that carry
    // other magnets are tripled, the Golden one comes to the hand with everything that was
    // attached to its parts, and is magnetized again - onto a Mech, and through Polarizing
    // Beatboxer onto the Beatboxer a second time.
    let mut g = game(9950);
    g.players[0].hand.clear();
    g.players[0].gold = 10;
    // Beatboxer, a plain Mech, two Red Volumizers standing as minions.
    set_board(
        &mut g,
        0,
        &["BG26_149", "BG21_022", "BG34_170t", "BG34_170t"],
    );
    for host in [2usize, 3] {
        give(&mut g, 0, "BG34_170t2"); // a Blue Volumizer onto each Red one
        let hand = g.players[0].hand.len() - 1;
        g.apply(0, Action::Magnetize { hand, board: host }).unwrap();
    }
    assert_eq!(
        g.players[0].board[0].attached.len(),
        2,
        "the Beatboxer took both"
    );
    give(&mut g, 0, "BG34_170t"); // the third Red Volumizer
    assert_eq!(
        g.players[0].board.len(),
        2,
        "both Red Volumizers left the board"
    );
    let golden = g.players[0].hand[0].item.as_minion().unwrap().clone();
    assert!(golden.golden);
    assert_eq!(golden.attached.len(), 2, "the Blue Volumizers came along");
    let before = stats(&g, 0);
    g.apply(0, Action::Magnetize { hand: 0, board: 1 }).unwrap();
    let pl = &g.players[0];
    let after = stats(&g, 0);
    // The Mech has the Golden's whole size and the three cards.
    assert_eq!(
        (after[1].0 - before[1].0, after[1].1 - before[1].1),
        (golden.atk, golden.hp)
    );
    assert_eq!(pl.board[1].attached.len(), 3);
    // The Beatboxer took the whole pile again.
    assert_eq!(pl.board[0].attached.len(), 2 + 3);
    assert!(after[0].0 - before[0].0 >= golden.atk && after[0].1 - before[0].1 >= golden.hp);
    // A Golden minion magnetized from the hand gives its Triple Reward.
    assert_eq!(hand_ids(&g, 0), vec!["TB_BaconShop_Triples_01"]);
}

/// Play the most recently given hand minion at the right end and aim its Battlecry at the
/// board minion `target`.
fn play_last_on(g: &mut Game, p: PlayerId, target: usize) {
    let eid = g.players[p].board[target].eid;
    play_last(g, p);
    let Some(Pending::Target { options, .. }) = g.players[p].pending.last() else {
        panic!("expected target")
    };
    let i = options.iter().position(|e| *e == eid).unwrap();
    g.apply(p, Action::Choose(i)).unwrap();
}

#[test]
fn nathanos_sells_a_minion_and_splits_its_stats_among_its_neighbours() {
    // Sylvanas' Buddy had no script, so E.T.C. never offered it.
    let mut g = game(11);
    let lieutenant = "BGS_039"; // 2/3 Taunt
    set_board(&mut g, 0, &[lieutenant, lieutenant, lieutenant]);
    {
        let m = &mut g.players[0].board[1];
        (m.atk, m.hp, m.max_hp) = (9, 7, 7);
    }
    let gold = g.players[0].gold;
    give(&mut g, 0, "BG23_HERO_306_Buddy");
    play_last_on(&mut g, 0, 1);
    // The middle 9/7 is sold; each former neighbour gets half, rounded down.
    assert_eq!(g.players[0].gold, gold + 1);
    assert_eq!(stats(&g, 0), vec![(2 + 4, 3 + 3), (2 + 4, 3 + 3), (6, 7)]);

    // Sold from the end of the row: its only neighbours are the other minion and Nathanos
    // himself. Golden: double its stats.
    let mut g = game(11);
    set_board(&mut g, 0, &[lieutenant, lieutenant]);
    {
        let m = &mut g.players[0].board[1];
        (m.atk, m.hp, m.max_hp) = (5, 8, 8);
    }
    give(&mut g, 0, "BG23_HERO_306_Buddy_G");
    play_last_on(&mut g, 0, 1);
    assert_eq!(stats(&g, 0), vec![(2 + 5, 3 + 8), (12 + 5, 14 + 8)]);

    // A single neighbour takes everything.
    let mut g = game(11);
    set_board(&mut g, 0, &[lieutenant, lieutenant, lieutenant]);
    give(&mut g, 0, "BG23_HERO_306_Buddy");
    play_last_on(&mut g, 0, 0);
    assert_eq!(stats(&g, 0), vec![(2 + 2, 3 + 3), (2, 3), (6, 7)]);

    // It is one of the Buddies E.T.C. can be offered.
    let pool = g.with_recruit(0, |c| taverngym::content::buddy_pool(c.g));
    let sylvanas = g.lobby_heroes().any(|h| h.id == "BG23_HERO_306");
    assert_eq!(pool.contains(&cid("BG23_HERO_306_Buddy")), sylvanas);
}

fn goldens(g: &Game, p: PlayerId) -> Vec<bool> {
    g.players[p].board.iter().map(|m| m.golden).collect()
}

#[test]
fn talent_scout_and_baby_nzoth_make_the_chosen_minion_golden() {
    // Both Battlecries have a targeting arrow in the client data: the player chooses.
    let (frostwolf, stormpike, plain) = ("BG22_HERO_002_Buddy", "BG22_HERO_003_Buddy", "BGS_039");
    for target in [0usize, 1] {
        let mut g = game(21);
        set_board(&mut g, 0, &[frostwolf, stormpike, plain]);
        give(&mut g, 0, "BG25_HERO_105_Buddy");
        play_last(&mut g, 0);
        let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
            panic!("Talent Scout asks which Buddy")
        };
        let buddies: Vec<u32> = g.players[0].board[..2].iter().map(|m| m.eid).collect();
        assert_eq!(*options, buddies, "only the Buddies can be chosen");
        g.apply(0, Action::Choose(target)).unwrap();
        assert_eq!(goldens(&g, 0), [target == 0, target == 1, false, false]);
    }
    // The Golden one: the chosen Buddy and one more.
    let mut g = game(21);
    set_board(&mut g, 0, &[frostwolf, stormpike, plain]);
    give(&mut g, 0, "BG25_HERO_105_Buddy_G");
    play_last_on(&mut g, 0, 1);
    assert_eq!(goldens(&g, 0), [true, true, false, true]);
    // No Buddy to choose: it is simply played.
    let mut g = game(21);
    set_board(&mut g, 0, &[plain]);
    give(&mut g, 0, "BG25_HERO_105_Buddy");
    play_last(&mut g, 0);
    assert!(g.players[0].pending.is_empty());
    assert_eq!(goldens(&g, 0), [false, false]);

    // Baby N'Zoth: the chosen Deathrattle minion; the Golden one takes them all and asks
    // for no target.
    let (bonehead, vermin) = ("BG28_300", "BG31_803");
    let mut g = game(21);
    set_board(&mut g, 0, &[bonehead, vermin, plain]);
    give(&mut g, 0, "TB_BaconShop_HERO_93_Buddy");
    play_last_on(&mut g, 0, 1);
    assert_eq!(goldens(&g, 0), [false, true, false, false]);
    let mut g = game(21);
    set_board(&mut g, 0, &[bonehead, vermin, plain]);
    give(&mut g, 0, "TB_BaconShop_HERO_93_Buddy_G");
    play_last(&mut g, 0);
    assert!(g.players[0].pending.is_empty());
    assert_eq!(goldens(&g, 0), [true, true, false, true]);
}

#[test]
fn akali_copies_a_triple_reward_with_its_tier() {
    // The copy of a Triple Reward had no Tier and Discovered from Tier 1.
    let mut g = game(31);
    set_board(&mut g, 0, &["BG26_HERO_104_Buddy_G"]); // Golden: every turn
    let reward = cid(taverngym::recruit::TRIPLE_REWARD);
    g.with_recruit(0, |c| {
        c.add_to_hand(
            0,
            Item::Spell {
                card: reward,
                data: 5,
            },
        );
    });
    g.apply(0, Action::EndTurn).unwrap();
    let tiers: Vec<i32> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| match h.item {
            Item::Spell { card, data } if card == reward => Some(data),
            _ => None,
        })
        .collect();
    assert_eq!(tiers, vec![5, 5]);
}

#[test]
fn living_nightmare_bonus_stays_in_the_tavern_for_the_turn() {
    // "After you buy a card, minions in the Tavern have +2/+2 this turn": read as Ashen Corruptor's
    // "this turn" - a Tavern that comes later in the turn has it too.
    let mut g = game(41);
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG20_HERO_280_Buddy"]);
    let extra = |g: &Game| -> Vec<(i32, i32)> {
        g.players[0]
            .shop
            .iter()
            .filter_map(|s| s.item.as_minion())
            .map(|m| (m.atk - db().get(m.card).atk, m.hp - db().get(m.card).hp))
            .collect()
    };
    assert!(extra(&g).iter().all(|x| *x == (0, 0)));
    let slot = g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_some())
        .unwrap();
    g.apply(0, Action::Buy(slot)).unwrap();
    assert!(!extra(&g).is_empty() && extra(&g).iter().all(|x| *x == (2, 2)));
    g.apply(0, Action::Refresh).unwrap();
    assert!(!extra(&g).is_empty() && extra(&g).iter().all(|x| *x == (2, 2)));
}

#[test]
fn titanic_guardian_gains_health_given_until_next_turn() {
    // Stats until next turn are stats gained; the Guardian missed them.
    let mut g = game(42);
    set_board(&mut g, 0, &["TB_BaconShop_HERO_39_Buddy", "BGS_039"]); // 5/2 and 2/3
    let t = g.players[0].board[1].eid;
    g.with_recruit(0, |c| c.buff_temp(t, 3, 4));
    assert_eq!(stats(&g, 0), vec![(5, 2 + 4), (2 + 3, 3 + 4)]);
    // The Golden one takes twice the Health.
    let mut g = game(42);
    set_board(&mut g, 0, &["TB_BaconShop_HERO_39_Buddy_G", "BGS_039"]); // 10/4
    let t = g.players[0].board[1].eid;
    g.with_recruit(0, |c| c.buff_temp(t, 0, 4));
    assert_eq!(stats(&g, 0)[0], (10, 4 + 8));
}

#[test]
fn improve_this_buddies_keep_their_growth_when_made_golden() {
    // "Improve this": a triple keeps the parts' growth, and a minion
    // made Golden in place (Talent Scout) keeps its growth without doubling it.
    let (elekk, skeleton) = ("BG20_HERO_101_Buddy", "BG_ICC_026t");
    let mut g = game(43);
    set_board(&mut g, 0, &[elekk]); // 4/4, +1/+1 and improve
    g.players[0].board[0].counter = 4; // improved four times: +5/+5
    give(&mut g, 0, "BG25_HERO_105_Buddy"); // Talent Scout, 4 Attack: no trigger
    play_last_on(&mut g, 0, 0);
    assert!(g.players[0].board[0].golden);
    give(&mut g, 0, skeleton);
    play_last(&mut g, 0);
    // Golden: +2/+2 and the four improvements, not twice the +5/+5.
    assert_eq!(g.players[0].board.last().unwrap().atk, 1 + 2 + 4);
    assert_eq!(
        g.players[0].board[0].counter,
        4 + 2,
        "a Golden improves by 2"
    );

    let mut g = game(43);
    set_board(&mut g, 0, &[elekk, elekk]);
    g.players[0].board[0].counter = 1;
    g.players[0].board[1].counter = 2;
    give(&mut g, 0, elekk);
    let golden = g.players[0].hand[0].item.as_minion().unwrap();
    assert!(golden.golden);
    assert_eq!(
        golden.counter, 3,
        "the triple keeps the growth of its parts"
    );

    // Burth: +2/+2 a step.
    let mut g = game(43);
    set_board(
        &mut g,
        0,
        &["TB_BaconShop_HERO_90_Buddy", "TB_BaconShop_HERO_90_Buddy"],
    );
    g.players[0].board[0].counter = 3;
    give(&mut g, 0, "TB_BaconShop_HERO_90_Buddy");
    let golden = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((golden.golden, golden.counter), (true, 3));

    // Crazy Monkey: one more Banana after every two spells.
    let (monkey, plain) = ("TB_BaconShop_HERO_38_Buddy", "BGS_039");
    let mut g = game(43);
    set_board(&mut g, 0, &[monkey, plain]);
    for _ in 0..3 {
        give(&mut g, 0, "BG28_810"); // Tavern Coin
        play_last(&mut g, 0);
    }
    g.apply(0, Action::Sell(0)).unwrap();
    assert_eq!(stats(&g, 0), vec![(2 + 2, 3 + 2)]);
    // Made Golden after two spells: 2 Bananas and the one improvement, not 4.
    let mut g = game(43);
    set_board(&mut g, 0, &[monkey, plain]);
    for _ in 0..2 {
        give(&mut g, 0, "BG28_810");
        play_last(&mut g, 0);
    }
    give(&mut g, 0, "BG25_HERO_105_Buddy");
    play_last_on(&mut g, 0, 0);
    // Two more spells improve the Golden by its own amount, 2.
    for _ in 0..2 {
        give(&mut g, 0, "BG28_810");
        play_last(&mut g, 0);
    }
    g.apply(0, Action::Sell(0)).unwrap();
    assert_eq!(stats(&g, 0)[0], (2 + 2 + 1 + 2, 3 + 2 + 1 + 2));
}

#[test]
fn asher_hats_pass_on_with_every_sale() {
    // "+1/+1 hats that pass to a friendly minion when sold": a hat keeps passing, it used
    // to be a plain +1/+1 after its first move.
    let (asher, plain) = ("TB_BaconShop_HERO_36_Buddy", "BGS_039");
    let mut g = game(45);
    set_board(&mut g, 0, &[asher, plain, plain]);
    g.apply(0, Action::Sell(2)).unwrap();
    assert_eq!(stats(&g, 0), vec![(2 + 2, 2 + 2), (2, 3)]);
    assert_eq!(g.players[0].hats.len(), 2);
    g.apply(0, Action::Sell(0)).unwrap(); // Asher: his two hats go to the Lieutenant
    assert_eq!(stats(&g, 0), vec![(2 + 2, 3 + 2)]);
    let next = minion(&mut g, 0, plain);
    let wearer = next.eid;
    g.players[0].board.push(next);
    g.apply(0, Action::Sell(0)).unwrap(); // the Lieutenant: the hats move again
    assert_eq!(stats(&g, 0), vec![(2 + 2, 3 + 2)]);
    assert_eq!(g.players[0].hats, vec![wearer; 2]);

    // A tripled Asher keeps the hats of its parts.
    let mut g = game(45);
    set_board(&mut g, 0, &[asher, asher, plain]);
    g.apply(0, Action::Sell(2)).unwrap(); // two hats each
    give(&mut g, 0, asher);
    let golden = g.players[0].hand[0].item.as_minion().unwrap().eid;
    assert_eq!(g.players[0].hats, vec![golden; 4]);
}

#[test]
fn weebomination_counts_missing_health_from_the_heros_full_health() {
    let mut g = game(46);
    set_board(&mut g, 0, &["BGS_039", "TB_BaconShop_HERO_34_Buddy"]);
    g.players[0].health = 45; // Patchwerk, 60 Health: 15 missing (it counted from 30)
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(stats(&g, 0)[0], (2, 3 + 1 + 15));
    let mut g = game(46);
    set_board(&mut g, 0, &["BGS_039", "TB_BaconShop_HERO_34_Buddy"]);
    g.players[0].hero = Some(cid("BG25_HERO_105")); // E.T.C., 30 Health
    g.players[0].health = 25;
    g.apply(0, Action::EndTurn).unwrap();
    assert_eq!(stats(&g, 0)[0], (2, 3 + 1 + 5));
}

#[test]
fn sharkbait_refreshes_a_once_per_game_hero_power() {
    let mut g = game(47);
    let piggy = cid("TB_BaconShop_HP_076"); // Piggy Bank, once per game
    g.with_recruit(0, |c| c.set_hero_power(0, 0, piggy));
    set_board(&mut g, 0, &["TB_BaconShop_HERO_68_Buddy"]);
    g.apply(0, Action::HeroPower(0)).unwrap();
    assert!(g.apply(0, Action::HeroPower(0)).is_err());
    g.apply(0, Action::Sell(0)).unwrap();
    g.apply(0, Action::HeroPower(0)).unwrap();
}

#[test]
fn the_ghost_stays_the_last_opponent_after_its_combat() {
    // A ghost is "your last opponent" (Doppelganger's Locket, Murozond, Tess). The combat took the
    // ghost's identity and nothing gave it back, so after a real ghost fight those effects found no
    // opponent.
    let mut g = game(48);
    g.players[7].alive = false;
    g.eliminated = vec![7];
    g.players[7].last_board = vec![minion(&mut g, 7, "BGS_039")];
    g.decide_pairs();
    end_all(&mut g);
    let p = (0..7)
        .find(|p| g.players[*p].last_opponent.is_none())
        .expect("one of seven fought the ghost");
    assert_eq!(g.last_fought(p), Some(7));
    g.players[p].rewards.push(cid("BG27_Reward_806"));
    g.players[p].pending.clear();
    let locket = cid("BG27_Reward_806");
    g.with_recruit(p, |c| {
        let me = taverngym::script::Me {
            side: 0,
            eid: 0,
            card: locket,
            golden: false,
            opt: 0,
            pos: 0,
            data: 0,
        };
        (taverngym::script::script(locket)
            .unwrap()
            .start_of_turn
            .unwrap())(c, me)
    });
    assert!(matches!(
        g.players[p].pending.last(),
        Some(Pending::Discover { options, .. }) if options.len() == 1
    ));
}

#[test]
fn perpetual_incantation_is_earned_again_each_time_its_quest_is_completed() {
    // "(+2/+1 can be earned endlessly!)": the Quest starts over after each +2/+1.
    let mut g = game(49);
    let (q, r) = (cid("BG24_Quest_313"), cid("BG33_Reward_020"));
    g.with_recruit(0, |c| c.start_quest(0, q, r));
    let goal = g.players[0].quests[0].goal;
    for n in 1..=3 {
        g.with_recruit(0, |c| c.quest_progress(0, q, goal));
        let pl = &g.players[0];
        assert_eq!((pl.stats.spell_atk, pl.stats.spell_hp), (2 * n, n));
        assert_eq!(pl.quests.len(), 1, "the Quest starts over");
        assert_eq!((pl.quests[0].progress, pl.quests[0].goal), (0, goal));
        assert_eq!(pl.rewards, vec![r]);
    }
    // Any other Reward ends its Quest.
    let mut g = game(49);
    g.with_recruit(0, |c| {
        c.start_quest(0, q, cid("BG24_Reward_107"));
        let goal = c.player_ref(0).unwrap().quests[0].goal;
        c.quest_progress(0, q, goal);
    });
    assert!(g.players[0].quests.is_empty());
}

#[test]
fn pressure_the_authorities_shows_the_total_attack_reached() {
    let mut g = game(50);
    let q = cid("BG27_Quest_801");
    set_board(&mut g, 0, &["BGS_039"]);
    g.with_recruit(0, |c| c.start_quest(0, q, cid("BG24_Reward_107"))); // 28 x 70% = 20
    give(&mut g, 0, "BGS_039");
    play_last(&mut g, 0);
    let quest = &g.players[0].quests[0];
    assert_eq!((quest.progress, quest.goal), (2 + 2, 20));
}

#[test]
fn quest_goals_are_shorter_without_the_types_they_lean_on() {
    use taverngym::content::quest_goal;
    // Client tags 2643..2652: a Quest (and a Reward) has a
    // percentage per minion type that applies when the type is not in the game.
    let without = |types: &[Races]| types.iter().fold(Races::ALL, |l, r| Races(l.0 & !r.0));
    let (snacks, wisdomball) = (cid("BG24_Reward_107"), cid("BG24_Reward_313")); // 70, 100
    // Assemble a Lineup, base 16: Beast / Demon / Mech 90, Undead 80.
    let lineup = cid("BG24_Quest_114");
    assert_eq!(quest_goal(Races::ALL, lineup, snacks), 11); // 11.2
    assert_eq!(quest_goal(without(&[Races::BEAST]), lineup, snacks), 10); // 10.08
    assert_eq!(
        quest_goal(without(&[Races::BEAST, Races::UNDEAD]), lineup, snacks),
        8
    ); // 8.06
    let none = without(&[Races::BEAST, Races::DEMON, Races::MECH, Races::UNDEAD]);
    assert_eq!(quest_goal(none, lineup, snacks), 7); // 6.53
    // A type the Quest does not name changes nothing.
    assert_eq!(quest_goal(without(&[Races::DRAGON]), lineup, snacks), 11);
    // Dust for Prints, base 23: Quilboar 70, Naga 70 - and Naga are in no lobby.
    let prints = cid("BG24_Quest_314");
    assert_eq!(quest_goal(Races::ALL, prints, wisdomball), 23);
    assert_eq!(quest_goal(without(&[Races::NAGA]), prints, wisdomball), 16); // 16.1
    assert_eq!(
        quest_goal(without(&[Races::NAGA, Races::QUILBOAR]), prints, wisdomball),
        11
    ); // 11.27
    // The Reward's own percentages: Evil Twin (110) is 120 without Murlocs, Bloodsoaked
    // Tome (175) is 80 without Pirates. Follow the Money, base 27, has none itself.
    let money = cid("BG24_Quest_126");
    let (twin, tome) = (cid("BG24_Reward_111"), cid("BG27_Reward_811"));
    assert_eq!(quest_goal(Races::ALL, money, twin), 30); // 29.7
    assert_eq!(quest_goal(without(&[Races::MURLOC]), money, twin), 36); // 35.64
    assert_eq!(quest_goal(Races::ALL, money, tome), 47); // 47.25
    assert_eq!(quest_goal(without(&[Races::PIRATE]), money, tome), 38); // 37.8
    // A game uses its own types: the Quest taken has the goal of the offer.
    let mut g = game(9201);
    g.with_recruit(0, |c| c.start_quest(0, prints, wisdomball));
    let quilboar = g.lobby_races.intersects(Races::QUILBOAR);
    assert_eq!(g.players[0].quests[0].goal, if quilboar { 16 } else { 11 });
}

#[test]
fn a_quest_offer_shows_the_minion_types_of_its_cards() {
    // Unlikely Duo's two types and the type of Scepter of Guidance / The Friends Along the Way are
    // on the offered cards; they were drawn after the choice.
    let (duo, scepter, friends) = (
        cid("BG24_Quest_151"),
        cid("BG27_Reward_812"),
        cid("BG24_Reward_134"),
    );
    let money = cid("BG24_Quest_126");
    let mut g = game(9202);
    let lobby = g.lobby_races;
    let choice = g.with_recruit(0, |c| {
        c.quest_choice(vec![(duo, scepter), (money, friends)])
    });
    let Pending::Quest { types, .. } = &choice else {
        panic!("a Quest choice")
    };
    let types = types.clone();
    let (two, one) = types[0];
    assert_eq!(two.count_ones(), 2, "Unlikely Duo: two different types");
    assert_eq!(one.count_ones(), 1, "Scepter of Guidance: one type");
    assert_eq!(types[1].0, 0, "Follow the Money has none");
    assert_eq!(types[1].1.count_ones(), 1);
    assert!(lobby.0 & two == two && lobby.0 & one == one && lobby.0 & types[1].1 != 0);
    // Taking the pair keeps what the offer showed, to the Reward's effect.
    g.players[0].pending.push(choice);
    g.apply(0, Action::Choose(0)).unwrap();
    let q = g.players[0].quests[0].clone();
    assert_eq!((q.data, q.reward_data), (two as i32, one as i32));
    g.with_recruit(0, |c| c.quest_progress(0, duo, q.goal));
    assert_eq!(g.players[0].rewards, vec![scepter]);
    assert_eq!(g.with_recruit(0, |c| c.card_data(0, scepter)), one as i32);
    // A pair taken without an offer draws its types when it is taken.
    let mut g = game(9202);
    g.with_recruit(0, |c| c.start_quest(0, duo, friends));
    let q = g.players[0].quests[0].clone();
    assert_eq!((q.data as u16).count_ones(), 2);
    g.with_recruit(0, |c| c.quest_progress(0, duo, q.goal));
    assert_eq!(
        (g.with_recruit(0, |c| c.card_data(0, friends)) as u16).count_ones(),
        1
    );
}

#[test]
fn sr_tomb_diver_gilds_for_good_only_outside_combat() {
    // The Deathrattle follows the base rule - a minion made Golden in combat is Golden for that
    // combat, one made Golden in the Tavern stays Golden.
    let (diver, plain) = ("TB_BaconShop_HERO_41_Buddy", "BG36_345");
    let mut g = game(9203);
    set_board(&mut g, 0, &[diver, plain]);
    set_board(&mut g, 1, &["BG35_155", "BG35_155"]);
    fight(&mut g);
    assert_eq!(goldens(&g, 0), [false, false]);
    let e = g.players[0].board[0].eid;
    g.with_recruit(0, |c| c.trigger_deathrattles(0, e));
    assert_eq!(goldens(&g, 0), [false, true]);
}

#[test]
fn a_reborn_minion_starts_its_avenge_count_again() {
    // A minion that comes back by Reborn is in its initial state; Drustfallen Butcher (Avenge (4))
    // does not keep the deaths it had counted.
    let mut g = game(9204);
    set_board(&mut g, 0, &["BG32_324", "BGS_039"]);
    set_board(&mut g, 1, &["BG35_155"]);
    let butcher = &mut g.players[0].board[0];
    butcher.give(Kw::REBORN);
    butcher.give(Kw::TAUNT);
    butcher.avenge = 3;
    (butcher.atk, butcher.hp, butcher.max_hp) = (1, 1, 1);
    g.cfg.record_log = true;
    fight(&mut g);
    // The first frame that shows the Butcher back (its Reborn used up).
    let back = g
        .last_frames
        .iter()
        .flat_map(|f| f.boards[0].iter())
        .find(|m| m.card == cid("BG32_324") && !m.has(Kw::REBORN))
        .expect("the Butcher dies and comes back");
    assert_eq!(back.avenge, 0, "the Reborn Butcher counts from zero");
}

#[test]
fn a_keyword_given_for_good_outlasts_its_until_next_turn_copy() {
    // A minion that had a keyword until next turn and then got the same keyword for good lost it at
    // the next turn start.
    let ds = Kw::DIVINE_SHIELD;
    // (steps: true = for good, false = until next turn) -> still there next turn?
    for (steps, kept) in [
        (&[false][..], false),
        (&[false, true][..], true),
        (&[true, false][..], true),
        (&[false, false][..], false),
        (&[false, true, false][..], true),
    ] {
        let mut g = game(9301);
        set_board(&mut g, 0, &["BG36_345"]);
        let e = g.players[0].board[0].eid;
        g.with_recruit(0, |c| {
            for for_good in steps {
                if *for_good {
                    c.give_kw(e, ds);
                } else {
                    c.give_kw_temp(e, ds);
                }
            }
        });
        assert!(g.players[0].board[0].has(ds), "{steps:?}");
        end_all(&mut g);
        assert_eq!(g.turn, 2);
        assert_eq!(g.players[0].board[0].has(ds), kept, "{steps:?}");
    }
}

#[test]
fn the_action_cap_binds_a_skipped_turn_too() {
    // In a skipped turn (A. F. Kay, Faelin) the mask kept Freeze legal past the turn's action cap
    // while `apply` refused it.
    let mut g = game(9302);
    g.cfg.turn_action_caps = vec![1];
    g.cfg.freeze_presses_per_turn = 0;
    g.players[0].skipping = true;
    g.players[0].actions_this_turn = 0;
    let freeze = Action::Freeze.encode();
    assert!(g.legal_mask(0)[freeze]);
    g.apply(0, Action::Freeze).unwrap();
    assert!(!g.legal_mask(0)[freeze], "the mask agrees with apply");
    assert!(g.apply(0, Action::Freeze).is_err());
    assert!(g.legal_mask(0)[Action::EndTurn.encode()]);
}

#[test]
fn a_configuration_must_fit_the_action_space_and_the_tiers() {
    // 17 hero options cannot be chosen through 16 choice slots, and `max_regular_tier` 8 let a
    // Tavern reach a Tier that does not exist.
    let ok = |cfg: RulesConfig| cfg.validate().is_ok();
    assert!(ok(RulesConfig::default()));
    assert!(ok(RulesConfig {
        hero_choices: 16,
        ..RulesConfig::default()
    }));
    assert!(!ok(RulesConfig {
        hero_choices: 17,
        ..RulesConfig::default()
    }));
    assert!(!ok(RulesConfig {
        trinket_choices: 17,
        ..RulesConfig::default()
    }));
    for tier in [0, 7, 8] {
        assert!(!ok(RulesConfig {
            max_regular_tier: tier,
            ..RulesConfig::default()
        }));
    }
}

#[test]
fn permanent_gains_in_combat_leave_no_stat_history_without_the_log() {
    // `buff_perm` writes no stat history (debug data) and builds no log line with
    // `record_log` off.
    for log in [false, true] {
        let mut g = game(9303);
        g.cfg.record_log = log;
        // Icesnarl the Mighty: after a friendly minion kills an enemy, +3 Health for good.
        set_board(&mut g, 0, &["BG20_HERO_100_Buddy", "BGS_039"]);
        set_board(&mut g, 1, &["BG_ICC_026t", "BG_ICC_026t"]);
        let big = &mut g.players[0].board[1];
        (big.atk, big.hp, big.max_hp) = (50, 50, 50);
        let (hp, history) = (g.players[0].board[0].hp, g.players[0].board[0].mods.len());
        fight(&mut g);
        let icesnarl = &g.players[0].board[0];
        assert!(icesnarl.hp > hp, "the gain is kept either way");
        assert_eq!(icesnarl.mods.len() > history, log);
    }
}

#[test]
fn a_death_batch_runs_deathrattles_before_queued_avenge_summons() {
    // Deathrattles resolve before Avenge - when the Deathrattle's summons fill the board, Eternal
    // Tycoon's Knight has no place yet. The Knight (a "when you have space" summon asked for inside
    // the death batch) does not take the place the dead minion has just freed ahead of that
    // minion's own Deathrattle.
    let mut g = game(9401);
    g.cfg.record_log = true;
    let plain = "BG36_345";
    // Tycoon one death short of Avenge (5), a Taunt Bonehead, five bystanders: a board of 7.
    set_board(
        &mut g,
        0,
        &["BG34_403", "BG28_300", plain, plain, plain, plain, plain],
    );
    for (i, m) in g.players[0].board.iter_mut().enumerate() {
        match i {
            0 => m.avenge = 4,
            1 => m.give(Kw::TAUNT),
            _ => {}
        }
        if i != 1 {
            (m.atk, m.hp, m.max_hp) = (1, 1000, 1000);
        }
    }
    set_board(&mut g, 1, &[plain]);
    let enemy = &mut g.players[1].board[0];
    (enemy.atk, enemy.hp, enemy.max_hp) = (1, 100_000, 100_000);
    fight(&mut g);
    let log = g.log.join("\n");
    let skeleton = log
        .find("summons Skeleton")
        .expect("the Bonehead's Deathrattle takes the place it left");
    if let Some(knight) = log.find("summons Eternal Knight") {
        assert!(
            skeleton < knight,
            "the Knight waits for the next free place"
        );
    }
}

#[test]
fn magnetize_and_triples_keep_this_turn_stats_as_temporary() {
    // Magnetize made "this turn" stats and keywords permanent; a triple dropped them. Both carry
    // them as temporary now (a triple inherits, nothing is dropped).
    let mut g = game(9402);
    set_board(&mut g, 0, &["BG26_147"]); // Accord-o-Tron 3/3
    give(&mut g, 0, "BG_BOT_911"); // Annoy-o-Module 2/4, Magnetic, Divine Shield, Taunt
    {
        let m = g.players[0].hand[0].item.as_minion_mut().unwrap();
        m.add_stats(3, 3);
        (m.temp_atk, m.temp_hp) = (3, 3);
        m.give(Kw::WINDFURY);
        m.temp_kw = Kw::WINDFURY;
    }
    g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
    let host = &g.players[0].board[0];
    assert_eq!((host.atk, host.hp), (3 + 2 + 3, 3 + 4 + 3));
    assert_eq!((host.temp_atk, host.temp_hp), (3, 3));
    assert!(host.has(Kw::WINDFURY) && host.temp_kw.has(Kw::WINDFURY));
    assert!(host.has(Kw::TAUNT) && !host.temp_kw.has(Kw::TAUNT));
    end_all(&mut g);
    let host = &g.players[0].board[0];
    assert_eq!((host.atk, host.hp), (3 + 2, 3 + 4), "the turn's part ended");
    assert!(!host.has(Kw::WINDFURY) && host.has(Kw::TAUNT) && host.has(Kw::DIVINE_SHIELD));

    let mut g = game(9402);
    set_board(&mut g, 0, &["BG36_345", "BG36_345"]); // 3/3 each
    for m in g.players[0].board.iter_mut() {
        m.add_stats(4, 4);
        (m.temp_atk, m.temp_hp) = (4, 4);
    }
    g.players[0].board[0].give(Kw::WINDFURY);
    g.players[0].board[0].temp_kw = Kw::WINDFURY;
    give(&mut g, 0, "BG36_345");
    let golden = g.players[0].hand[0].item.as_minion().unwrap();
    assert!(golden.golden);
    assert_eq!((golden.temp_atk, golden.temp_hp), (8, 8));
    assert_eq!((golden.atk, golden.hp), (6 + 8, 6 + 8));
    assert!(golden.has(Kw::WINDFURY) && golden.temp_kw.has(Kw::WINDFURY));
    end_all(&mut g);
    let golden = g.players[0].hand[0].item.as_minion().unwrap();
    assert_eq!((golden.atk, golden.hp), (6, 6));
    assert!(!golden.has(Kw::WINDFURY));
}

#[test]
fn another_players_warband_is_read_as_it_last_fought() {
    // A seat that has ended its turn already shows this turn's warband in `last_board`;
    // effects that read an opponent's warband must not see it (hidden, and it made the
    // result depend on who ended first).
    let mut g = game(9403);
    set_board(&mut g, 1, &["BG36_345"]);
    end_all(&mut g);
    assert_eq!(g.turn, 2);
    let m = minion(&mut g, 1, "BGS_039");
    g.players[1].board.push(m);
    while !g.players[1].pending.is_empty() {
        let a = g.legal_actions(1)[0];
        g.apply_index(1, a).unwrap();
    }
    g.apply(1, Action::EndTurn).unwrap();
    assert_eq!(g.players[1].last_board.len(), 2, "this turn's warband");
    assert_eq!(g.fought_board(1).len(), 1, "the warband it fought with");
    // Murozond: a plain copy of a minion from the last opponent's warband.
    g.players[0].last_opponent = Some(1);
    g.players[0].hand.clear();
    set_board(&mut g, 0, &[]);
    for _ in 0..2 {
        give(&mut g, 0, "BGS_043");
        play_last(&mut g, 0);
    }
    let got: Vec<String> = hand_ids(&g, 0);
    assert_eq!(got, vec!["BG36_345"; 2], "never this turn's Lieutenant");
    // A dead player's warband is its last one (the ghost's).
    g.players[1].alive = false;
    assert_eq!(g.fought_board(1).len(), 2);
}

#[test]
fn a_bought_card_is_in_hand_before_the_payment_sets_anything_off() {
    // The card enters the hand first; with nine cards in hand the Bounty that "after you spend 9
    // Gold" gives finds no place; the bought card is kept.
    let mut g = game(9404);
    set_board(&mut g, 0, &["BG33_823"]); // Sky Admiral Rogers
    g.players[0].board[0].counter = 6;
    g.players[0].gold = 10;
    while g.players[0].hand.len() < 9 {
        give(&mut g, 0, "BG28_810");
    }
    let slot = g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_some())
        .unwrap();
    let bought = g.players[0].shop[slot].item.card();
    g.apply(0, Action::Buy(slot)).unwrap();
    let hand = &g.players[0].hand;
    assert_eq!(hand.len(), 10);
    assert_eq!(hand.last().unwrap().item.card(), bought);
    assert_eq!(g.players[0].gold, 7);
}

#[test]
fn locked_hand_cards_can_be_read_and_copied_but_not_summoned() {
    // Read/copy effects include locked cards, but play/summon stays forbidden.
    let mut g = game(9405);
    give(&mut g, 0, "BG36_345");
    give(&mut g, 0, "BGS_039");
    g.players[0].hand[0].locked = 1;
    let locked = g.players[0].hand[0].hid;
    assert_eq!(g.with_recruit(0, |c| c.hand_minions(0)).len(), 2);
    assert!(
        g.with_recruit(0, |c| c.summon_from_hand(0, locked, 0))
            .is_none()
    );
    assert!(!g.legal_mask(0)[Action::Play { hand: 0, pos: 0 }.encode()]);
    // Akali copies the actual left-most card, even when the original is locked.
    set_board(&mut g, 0, &["BG26_HERO_104_Buddy_G"]);
    g.apply(0, Action::EndTurn).unwrap();
    let ids = hand_ids(&g, 0);
    assert_eq!(ids.iter().filter(|x| *x == "BGS_039").count(), 1, "{ids:?}");
    assert_eq!(
        ids.iter().filter(|x| *x == "BG36_345").count(),
        2,
        "{ids:?}"
    );
}

#[test]
fn a_card_that_waits_for_hand_space_arrives_as_it_was_given() {
    // A hand-protected card queued with a full hand arrived as a bare card: a linked pair
    // came unlinked, and "no card in hand" was read as "lost" (Typewriter gave two).
    let mut g = game(9406);
    while g.players[0].hand.len() < 10 {
        give(&mut g, 0, "BG28_810");
    }
    let sticker = cid("BG36_MagicItem_852"); // a trinket: its cards wait for space
    g.with_recruit(0, |c| {
        let prev = c.src.replace(sticker);
        taverngym::recruit::give_linked_pair(c, 0, cid("BG36_345"), cid("BGS_039"));
        assert!(c.queued.is_some(), "waiting, not lost");
        c.src = prev;
    });
    assert_eq!(g.players[0].hand_queue.len(), 2);
    g.players[0].hand.clear();
    g.flush_hand_queue(0);
    let hand = &g.players[0].hand;
    assert_eq!(hand.len(), 2);
    assert_eq!((hand[0].link, hand[1].link), (hand[1].hid, hand[0].hid));
    // Playing one discards the other, as for a pair given with room in hand.
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert!(g.players[0].hand.is_empty());
}

#[test]
fn small_framework_rules() {
    // Stats until next turn count for "once this reaches N".
    let mut g = game(9407);
    set_board(&mut g, 0, &["BG35_814"]); // Scarlet Survivor 3/3: Divine Shield at 6 Attack
    let e = g.players[0].board[0].eid;
    g.with_recruit(0, |c| c.buff_temp(e, 4, 0));
    assert!(g.players[0].board[0].has(Kw::DIVINE_SHIELD));

    // An Activate with no legal target is not legal.
    let mut g = game(9407);
    g.players[0].gold = 5;
    set_board(&mut g, 0, &["BG36_511"]); // Dead Bellringer: a different friendly Undead
    let activate = Action::Activate(0).encode();
    assert!(!g.legal_mask(0)[activate]);
    assert!(g.apply(0, Action::Activate(0)).is_err());
    let other = minion(&mut g, 0, "BG36_345"); // not an Undead
    g.players[0].board.push(other);
    assert!(!g.legal_mask(0)[activate]);
    let undead = minion(&mut g, 0, "BG28_300");
    g.players[0].board.push(undead);
    assert!(g.legal_mask(0)[activate]);

    // Magnetizing a linked card from hand discards its partner, as playing it does.
    let mut g = game(9407);
    set_board(&mut g, 0, &["BG26_147"]);
    give(&mut g, 0, "BG26_146"); // Lullabot, Magnetic
    give(&mut g, 0, "BG36_345");
    let (a, b) = (g.players[0].hand[0].hid, g.players[0].hand[1].hid);
    g.players[0].hand[0].link = b;
    g.players[0].hand[1].link = a;
    g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
    assert!(g.players[0].hand.is_empty());

    // A Souvenir Stand copy of Heart of the Forest is a second trinket for Tavern spells.
    let mut g = game(9407);
    let heart = cid("BG32_MagicItem_801t");
    g.players[0].trinkets = vec![heart];
    let one = g.with_recruit(0, |c| c.spell_extra(0));
    g.players[0].trinkets = vec![heart, heart.copy()];
    let two = g.with_recruit(0, |c| c.spell_extra(0));
    assert_eq!((two.0 - one.0, two.1 - one.1), (1, 1));

    // No triple once the turn has ended: the warband for the combat is set.
    let mut g = game(9407);
    set_board(&mut g, 0, &["BG36_345", "BG36_345"]);
    g.players[0].ended_turn = true;
    give(&mut g, 0, "BG36_345");
    assert_eq!(g.players[0].board.len(), 2);

    // Equal Health + Armor: the lower seat places better, as at the turn cap.
    let mut g = game(9407);
    for p in [2, 5] {
        (g.players[p].health, g.players[p].armor) = (-3, 0);
    }
    g.apply(0, Action::Freeze).unwrap();
    assert_eq!((g.players[2].placement, g.players[5].placement), (7, 8));

    // A Choose One spell whose options all need a target is not playable without one.
    let mut g = game(9407);
    set_board(&mut g, 0, &[]);
    give(&mut g, 0, "BG31_880"); // Alliance Flag
    g.players[0].shop.clear();
    let play = Action::Play { hand: 0, pos: 0 }.encode();
    assert!(!g.legal_mask(0)[play]);
    // A cancelled combined play gives Thorned Trailblazer its use back.
    set_board(&mut g, 0, &["BG31_327", "BG36_345"]);
    g.players[0].board[0].counter = 1;
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert!(matches!(
        g.players[0].pending.last(),
        Some(Pending::Target { .. })
    ));
    assert_eq!(g.players[0].board[0].counter, 0);
    g.apply(0, Action::Cancel).unwrap();
    assert_eq!(g.players[0].board[0].counter, 1);
    assert_eq!(g.players[0].hand.len(), 1);
}

#[test]
fn two_trinket_offers_open_at_once_keep_their_own_discounts() {
    use taverngym::cards::CardKind;
    // A Reward's offer on a trinket turn: the second offer overwrote the first one's
    // discounts, and an offer left with nothing affordable had no legal option.
    for seed in 9450..9460 {
        let mut g = game(seed);
        g.turn = 6;
        g.players[0].gold = 20;
        g.offer_trinkets(0, CardKind::TrinketLesser);
        let first = g.players[0].trinket_pivots.clone();
        g.offer_trinkets(0, CardKind::TrinketLesser);
        assert_eq!(g.players[0].pending.len(), 2);
        assert_eq!(g.players[0].trinket_pivot_stack, vec![first.clone()]);
        // Just enough Gold for the top offer's first option.
        let Some(Pending::Trinket { options, .. }) = g.players[0].pending.last() else {
            panic!("a trinket offer")
        };
        g.players[0].gold = g.trinket_price(0, options[0]);
        g.apply(0, Action::Choose(0)).unwrap();
        let pl = &g.players[0];
        assert!(pl.trinket_pivot_stack.is_empty());
        if pl.pending.is_empty() {
            assert!(pl.trinket_pivots.is_empty(), "the offer was skipped");
        } else {
            assert_eq!(pl.trinket_pivots, first, "the first offer's own discounts");
        }
        assert!(
            !g.needs_action(0) || g.legal_mask(0).iter().any(|x| *x),
            "seed {seed}: a seat that has to act has a legal action"
        );
    }
}

#[test]
fn a_death_in_the_recruit_phase_keeps_the_other_pairings() {
    // The dead seat's opponent meets the ghost, with this turn's warband; everyone else
    // keeps the opponent announced at the turn start.
    let mut g = game(9408);
    let pairs = g.pairs.clone();
    assert_eq!(pairs.len(), 4);
    let (a, b) = (pairs[0].0, pairs[0].1.unwrap());
    set_board(&mut g, b, &["BG36_345", "BGS_039"]);
    (g.players[b].health, g.players[b].armor) = (0, 0);
    let other = (0..8).find(|p| *p != a && *p != b).unwrap();
    g.apply(other, Action::Freeze).unwrap();
    assert!(!g.players[b].alive);
    assert_eq!(g.players[b].last_board.len(), 2, "the ghost's warband");
    assert_eq!(g.players[a].next_opponent, None);
    assert!(g.players[a].next_is_ghost);
    for pair in &pairs[1..] {
        assert!(g.pairs.contains(pair), "{pair:?} stands");
    }
    assert!(g.pairs.contains(&(a, None)));
    // A second death: its opponent and the seat that had the ghost meet each other.
    let (c, d) = (pairs[1].0, pairs[1].1.unwrap());
    (g.players[d].health, g.players[d].armor) = (0, 0);
    g.apply(other, Action::Freeze).unwrap();
    let met = g.players[a].next_opponent;
    assert_eq!(met, Some(c));
    assert_eq!(g.players[c].next_opponent, Some(a));
    assert!(!g.players[a].next_is_ghost && !g.players[c].next_is_ghost);
}

#[test]
fn a_hand_written_config_is_checked_key_by_key() {
    // A misspelt key is an error, not silently dropped.
    assert!(RulesConfig::from_json(r#"{"hero_choices": 4}"#).is_ok());
    let err = RulesConfig::from_json(r#"{"hero_choice": 4}"#).unwrap_err();
    assert!(err.contains("hero_choice"), "{err}");
    assert!(RulesConfig::from_json("[1, 2]").is_err());
    // The two fields that are not written at their default are known all the same, and a
    // configuration written out in full reads back.
    let full = RulesConfig {
        hero_bans: vec!["TB_BaconShop_HERO_34".into()],
        freeze_presses_per_turn: 3,
        ..RulesConfig::default()
    };
    let json = serde_json::to_string(&full).unwrap();
    assert!(json.contains("hero_bans") && json.contains("freeze_presses_per_turn"));
    let back = RulesConfig::from_json(&json).unwrap();
    assert_eq!(back.freeze_presses_per_turn, 3);
    // Values the engine cannot run with are refused.
    for bad in [
        r#"{"max_combat_attacks": 0}"#,
        r#"{"max_effect_depth": 0}"#,
        r#"{"triple_copies": 0}"#,
        r#"{"shop_max_cards": 20}"#,
        r#"{"dark_gift_choices": 40}"#,
        r#"{"damage_caps": [[8, 15], [1, 5]]}"#,
        r#"{"pool_copies_by_tier": [70000, 15, 13, 11, 9, 7, 5]}"#,
        r#"{"gold_refill_cap": -1}"#,
        r#"{"excluded_cards": ["NO_SUCH_CARD"]}"#,
        r#"{"guaranteed_heroes": ["NO_SUCH_HERO"]}"#,
        r#"{"aberration_forced": true, "lobby_types": ["BEAST", "DEMON", "DRAGON", "MECHANICAL", "MURLOC"]}"#,
    ] {
        assert!(RulesConfig::from_json(bad).is_err(), "{bad}");
    }
    // Five forced types fit a lobby of five without the Aberration guarantee.
    assert!(
        RulesConfig::from_json(
            r#"{"lobby_types": ["BEAST", "DEMON", "DRAGON", "MECHANICAL", "MURLOC"]}"#
        )
        .is_ok()
    );
    // A hero whose power is not implemented cannot be the hero pool (every seat would
    // get an empty hero choice).
    let unscripted = db()
        .iter()
        .find(|c| {
            c.kind == taverngym::cards::CardKind::Hero
                && c.hero_power
                    .is_some_and(|hp| taverngym::script::script(hp).is_none())
        })
        .map(|c| c.id.clone());
    if let Some(id) = unscripted {
        let cfg = RulesConfig {
            hero_pool: vec![id.clone()],
            ..RulesConfig::default()
        };
        assert!(cfg.validate().is_err(), "{id}");
    }
}

#[test]
fn a_saved_game_is_checked_against_the_card_table() {
    let g = game(9501);
    assert_eq!(g.card_table, db().fingerprint);
    assert!(g.check_loaded().is_ok());
    let back: Game = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
    assert!(back.check_loaded().is_ok());
    // Another card table: every card id would mean another card.
    let mut other = g.clone();
    other.card_table ^= 1;
    assert!(other.check_loaded().is_err());
    // A state from before the field (0) is still refused when its pool table is not this
    // build's.
    let mut old = g.clone();
    old.card_table = 0;
    assert!(old.check_loaded().is_ok());
    old.pool.counts.truncate(3000);
    assert!(old.check_loaded().is_err());
    // An impossible configuration inside the state.
    let mut bad = g.clone();
    bad.cfg.board_size = 12;
    assert!(bad.check_loaded().is_err());
}

#[test]
fn a_seat_without_a_legal_action_stops_the_bots_at_once() {
    // A paid trinket offer nobody can afford and nobody can decline (built by hand: the
    // engine skips such an offer).
    let mut g = game(9502);
    let dear = *g
        .trinket_universe
        .iter()
        .find(|t| db().get(**t).cost > 0)
        .unwrap();
    g.players[0].gold = 0;
    g.players[0].pending.push(Pending::Trinket {
        options: vec![dear],
        free: false,
        replace: None,
    });
    assert!(g.needs_action(0) && !g.legal_mask(0).iter().any(|x| *x));
    assert!(!taverngym::bot::play_turn(&mut g, 0), "nothing was done");
    assert!(g.players[0].pending.len() == 1);
}

#[test]
fn debug_commands_keep_the_pool_and_start_quests_properly() {
    use taverngym::debug::{DebugCmd, run_debug};
    let mut g = game(9503);
    let pool = |g: &Game| -> u64 { g.pool.counts.iter().map(|n| *n as u64).sum() };
    let held = |g: &Game| -> u64 {
        g.players[0]
            .shop
            .iter()
            .filter_map(|s| s.item.as_minion())
            .map(|m| m.pool_copies as u64)
            .sum()
    };
    let total = pool(&g) + held(&g);
    run_debug(
        &mut g,
        DebugCmd::Remove {
            p: 0,
            zone: "shop".into(),
            index: 0,
        },
    )
    .unwrap();
    assert_eq!(
        pool(&g) + held(&g),
        total,
        "a removed Tavern minion goes back"
    );
    run_debug(
        &mut g,
        DebugCmd::ClearZone {
            p: 0,
            zone: "shop".into(),
        },
    )
    .unwrap();
    assert_eq!(pool(&g), total);
    // A Quest given by the debug command has its minion types.
    run_debug(
        &mut g,
        DebugCmd::GiveQuest {
            p: 0,
            quest: "BG24_Quest_151".into(),
            reward: Some("BG24_Reward_107".into()),
        },
    )
    .unwrap();
    assert_eq!((g.players[0].quests[0].data as u16).count_ones(), 2);
}

#[test]
fn reborn_happens_before_avenge_buffs_and_new_bodies_do_not_avenge_their_own_birth() {
    let mut g = game(9501);
    // Duskmaw's Avenge (1) buffs all Dragons, including the Warden just reborn.
    set_board(&mut g, 0, &["BG34_PreMadeChamp_020", "BGS_034"]);
    {
        let boards = [g.players[0].board.clone(), vec![]];
        let mut c = taverngym::engine::Ctx::new(&mut g, boards, [Some(0), None], true);
        c.boards[0][1].hp = 0;
        c.resolve_deaths();
        let reborn = &c.boards[0][1];
        assert!(reborn.is_card("BGS_034"));
        assert_eq!((reborn.atk, reborn.hp), (8, 5));
    }
    // A reborn Avenge body is new; the death that created it cannot count for it.
    set_board(&mut g, 0, &["BG32_324"]);
    {
        let boards = [g.players[0].board.clone(), vec![]];
        let mut c = taverngym::engine::Ctx::new(&mut g, boards, [Some(0), None], true);
        let m = &mut c.boards[0][0];
        m.give(Kw::REBORN);
        m.avenge = 3;
        m.hp = 0;
        c.resolve_deaths();
        assert_eq!(c.boards[0][0].avenge, 0);
    }
    assert!(g.players[0].hand.is_empty());
}

#[test]
fn a_plain_copy_of_a_held_card_is_fresh_and_voone_sees_the_actual_leftmost_card() {
    let mut g = game(9502);
    g.cfg.triple_copies = 99;
    let hid = give(&mut g, 0, "BG36_345");
    let h = &mut g.players[0].hand[0];
    h.locked = 1;
    h.held = true;
    h.held_gold = 42;
    h.item.as_minion_mut().unwrap().add_stats(99, 99);
    give(&mut g, 0, "BGS_039");
    g.set_hero_power(0, 0, cid("BG26_HERO_104p"));
    // Voone fires every third End Turn.
    for _ in 0..3 {
        end_all(&mut g);
    }
    let copies: Vec<_> = g.players[0]
        .hand
        .iter()
        .filter(|h| h.item.card() == cid("BG36_345"))
        .collect();
    assert_eq!(copies.len(), 2);
    let fresh = copies.iter().find(|h| h.hid != hid).unwrap();
    assert!(!fresh.held && fresh.locked == 0 && fresh.held_gold == 0);
    let base = db().get(cid("BG36_345"));
    let m = fresh.item.as_minion().unwrap();
    assert_eq!((m.atk, m.hp), (base.atk, base.hp));
}

#[test]
fn copied_lockboxes_are_unplayable_and_start_with_five_turns() {
    let mut g = game(9503);
    let lb = cid(taverngym::content::LOCKBOX);
    let hid = g.with_recruit(0, |c| {
        c.add_to_hand(0, Item::Spell { card: lb, data: 2 }).unwrap()
    });
    g.with_recruit(0, |c| {
        c.give_copy_of_hand(0, hid).unwrap();
        c.give_card(0, lb).unwrap();
    });
    let counters: Vec<_> = g.players[0]
        .hand
        .iter()
        .map(|h| match h.item {
            Item::Spell { data, .. } => data,
            _ => -1,
        })
        .collect();
    assert_eq!(counters, [2, 5, 5]);
    for i in 0..3 {
        assert!(!g.legal_mask(0)[Action::Play { hand: i, pos: 0 }.encode()]);
    }
    end_all(&mut g);
    let counters: Vec<_> = g.players[0]
        .hand
        .iter()
        .map(|h| match h.item {
            Item::Spell { data, .. } => data,
            _ => -1,
        })
        .collect();
    assert_eq!(counters, [1, 4, 4]);
}

#[test]
fn skyborn_drake_can_give_recruit_venom_through_bile_spitter_without_firing_deathstrider() {
    let mut g = game(9504);
    set_board(
        &mut g,
        0,
        &["BG36_243", "BG33_318", "BG26_354", "BG36_202", "BG36_208"],
    );
    g.players[0].gold = 10;
    let target = g.players[0].board[1].eid;
    g.apply(0, Action::Activate(0)).unwrap();
    choose_target(&mut g, 0, target);
    assert!(g.players[0].board[2].has(Kw::VENOMOUS));
    assert_eq!(
        g.players[0].stats.lobster_bonus, 0,
        "Rally activation is not an attack"
    );
    assert!(taverngym::script::rally_untriggerable(cid("BG34_140")));
    assert!(taverngym::script::rally_untriggerable(cid(
        "BG34_Giant_110"
    )));
}

#[test]
fn two_normal_deathstriders_and_golden_titus_make_six_golden_lobster_triggers_per_attack() {
    use taverngym::event::Event;
    let mut g = game(9505);
    set_board(
        &mut g,
        0,
        &[
            "BG36_204_G",
            "BG36_202_G",
            "BG36_208",
            "BG36_208",
            "BG25_354_G",
        ],
    );
    let initial: (i32, i32) = g.players[0]
        .board
        .iter()
        .filter(|m| m.is(Races::BEAST))
        .fold((0, 0), |(a, h), m| (a + m.atk, h + m.hp));
    let gryphon = g.players[0].board[0].eid;
    g.with_recruit(0, |c| {
        for _ in 0..10 {
            c.emit(Event::AttackDone {
                side: 0,
                attacker: gryphon,
            });
        }
    });
    assert_eq!(g.players[0].stats.lobster_bonus, 60);
    let now: (i32, i32) = g.players[0]
        .board
        .iter()
        .filter(|m| m.is(Races::BEAST))
        .fold((0, 0), |(a, h), m| (a + m.atk, h + m.hp));
    assert_eq!((now.0 - initial.0, now.1 - initial.1), (7320, 3660));
    {
        let boards = [g.players[0].board.clone(), vec![]];
        let mut c = taverngym::engine::Ctx::new(&mut g, boards, [Some(0), None], true);
        for _ in 0..4 {
            c.emit(Event::AttackDone {
                side: 0,
                attacker: gryphon,
            });
        }
        let combat: (i32, i32) = c.boards[0]
            .iter()
            .filter(|m| m.is(Races::BEAST))
            .fold((0, 0), |(a, h), m| (a + m.atk, h + m.hp));
        assert_eq!((combat.0 - now.0, combat.1 - now.1), (6960, 3480));
    }
    assert_eq!(
        g.players[0].stats.lobster_bonus, 84,
        "future lobster improvement persists"
    );
    let permanent: (i32, i32) = g.players[0]
        .board
        .iter()
        .filter(|m| m.is(Races::BEAST))
        .fold((0, 0), |(a, h), m| (a + m.atk, h + m.hp));
    assert_eq!(
        permanent, now,
        "the combat stat buffs themselves are temporary"
    );
}

#[test]
fn golden_lobster_growth_depends_on_the_real_accumulated_history() {
    use taverngym::event::Event;
    for b in [0, 50, 100] {
        let mut g = game(10517);
        set_board(
            &mut g,
            0,
            &[
                "BG36_204_G",
                "BG36_202_G",
                "BG36_208",
                "BG36_208",
                "BG25_354_G",
            ],
        );
        g.players[0].stats.lobster_bonus = b;
        let total = |g: &Game| {
            g.players[0]
                .board
                .iter()
                .filter(|m| m.is(Races::BEAST))
                .fold((0, 0), |(a, h), m| (a + m.atk, h + m.hp))
        };
        let before = total(&g);
        let gryphon = g.players[0].board[0].eid;
        g.with_recruit(0, |c| {
            for _ in 0..10 {
                c.emit(Event::AttackDone {
                    side: 0,
                    attacker: gryphon,
                });
            }
        });
        assert_eq!(g.players[0].stats.lobster_bonus, b + 60);
        let after = total(&g);
        assert_eq!(
            (after.0 - before.0, after.1 - before.1),
            (240 * b + 7320, 120 * b + 3660)
        );
    }
}

#[test]
fn fodder_feeds_after_the_refresh_and_its_refill_gets_no_refresh_effect() {
    // The Fodder of a Refresh gets the "after the Tavern is Refreshed" Blood Gems before a Demon
    // eats it; the minion that refills its slot came without a Refresh and gets none. It was the
    // other way round: the Fodder was eaten before the Refresh event.
    let mut g = game(9601);
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BGS_004"]); // Wrath Weaver, a Demon
    let (a0, h0) = (g.players[0].board[0].atk, g.players[0].board[0].hp);
    let fodder = db().get(cid("BG35_150t"));
    g.with_recruit(0, |c| {
        c.add_player_effect(0, cid("BG34_689"), 1); // Blood Gem Barrage: +2/+2 worth of Gems
        c.add_fodder(0, 1, 1);
    });
    g.apply(0, Action::Refresh).unwrap();
    let d = &g.players[0].board[0];
    assert_eq!(
        (d.atk - a0, d.hp - h0),
        (fodder.atk + 2, fodder.hp + 2),
        "the Demon ate the Fodder with its Gems"
    );
    let minions: Vec<&Minion> = g.players[0]
        .shop
        .iter()
        .filter_map(|s| s.item.as_minion())
        .collect();
    assert!(
        minions
            .iter()
            .all(|m| db().normal_of(m.card) != cid("BG35_150t"))
    );
    let gemmed = minions
        .iter()
        .filter(|m| m.gem_atk > 0 || m.gems > 0)
        .count();
    assert_eq!(gemmed, minions.len() - 1, "only the refill has no Gems");
}

#[test]
fn a_triggered_battlecry_discovers_at_random_in_the_recruit_phase() {
    // A Battlecry triggered by another effect can't ask during combat or at the end of the turn,
    // and the game picks its Discover at random in the recruit phase too. Rodeo Performer:
    // "Battlecry: Discover a Tavern spell."
    let spells = |g: &Game| {
        g.players[0]
            .hand
            .iter()
            .filter(|h| h.item.as_minion().is_none())
            .count()
    };
    // Kelp Keeper (Activate): the target is the player's, the Discover is not.
    let mut g = game(9301);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG36_701", "BG28_550"]);
    g.apply(0, Action::Activate(0)).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].pending.is_empty(), "no Discover to answer");
    assert_eq!(spells(&g), 1);

    // Golden Kelp Keeper: two triggers, two random picks.
    let mut g = game(9302);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG36_701_G", "BG28_550"]);
    g.apply(0, Action::Activate(0)).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].pending.is_empty());
    assert_eq!(spells(&g), 2);

    // Primalfin Lookout triggered by Kelp Keeper: its Murloc arrives without a choice.
    let mut g = game(9303);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    g.players[0].tier = 5;
    g.universe = vec![cid("BGS_020"), cid("BG29_300")];
    for c in g.universe.clone() {
        g.pool.put(c, 5);
    }
    set_board(&mut g, 0, &["BG36_701", "BGS_020", "BG29_300"]);
    g.apply(0, Action::Activate(0)).unwrap();
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].pending.is_empty());
    assert_eq!(hand_ids_of(&g, 0), vec!["BG29_300"]);

    // Snicker-snack (Shudderwock's Hero Power) is the same trigger.
    let mut g = game(9304);
    g.cfg.triple_copies = 99;
    g.turn = 3;
    g.players[0].gold = 10;
    g.players[0].set_hero_power(0, cid("TB_BaconShop_HP_022"));
    set_board(&mut g, 0, &["BG28_550"]);
    g.apply(0, Action::HeroPower(0)).unwrap();
    if !g.players[0].pending.is_empty() {
        g.apply(0, Action::Choose(0)).unwrap(); // the target
    }
    assert!(g.players[0].pending.is_empty());
    assert_eq!(spells(&g), 1);

    // Young Murk-Eye at the end of the turn: the spell is in hand when the turn is over.
    let mut g = game(9305);
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG28_550", "BG22_403"]);
    g.apply(0, Action::EndTurn).unwrap();
    assert!(g.players[0].pending.is_empty());
    assert_eq!(spells(&g), 1);

    // Played from hand, the Battlecry still asks.
    let mut g = game(9306);
    g.cfg.triple_copies = 99;
    give(&mut g, 0, "BG28_550");
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    assert!(matches!(
        g.players[0].pending.last(),
        Some(Pending::Discover { .. })
    ));
}

#[test]
fn two_hot_air_surveyors_equal_a_golden_one_with_or_without_balinda() {
    // Each Surveyor adds one cast (golden: two), so two normal Surveyors give what one Golden
    // gives; Balinda adds hers. Every cast is a whole Blood Gem, with the player's Gem bonus.
    let cases: [(&[&str], i32); 12] = [
        (&[], 1),
        (&["BG30_121"], 2),
        (&["BG30_121", "BG30_121"], 3),
        (&["BG30_121_G"], 3),
        (&["BG30_121", "BG30_121_G"], 4),
        (&["BG35_883"], 2),
        (&["BG30_121", "BG35_883"], 3),
        (&["BG30_121", "BG30_121", "BG35_883"], 4),
        (&["BG30_121_G", "BG35_883"], 4),
        (&["BG30_121", "BG30_121", "BG35_883_G"], 5),
        (&["BG30_121_G", "BG35_883_G"], 5),
        (&["BG30_121", "BG30_121_G", "BG35_883"], 5),
    ];
    for bonus in [(0, 0), (2, 1)] {
        for (before, casts) in cases {
            let mut g = game(9920);
            g.cfg.triple_copies = 99;
            let mut board = before.to_vec();
            board.push("BG36_345");
            set_board(&mut g, 0, &board);
            g.players[0].hand.clear();
            g.players[0].stats.gem_atk = bonus.0;
            g.players[0].stats.gem_hp = bonus.1;
            let t = board.len() - 1;
            let (a0, h0) = (g.players[0].board[t].atk, g.players[0].board[t].hp);
            give(&mut g, 0, "BG20_GEM");
            cast_on(&mut g, 0, t);
            let m = &g.players[0].board[t];
            assert_eq!(
                (m.atk - a0, m.hp - h0, m.gems as i32),
                (casts * (1 + bonus.0), casts * (1 + bonus.1), casts),
                "Gem bonus {bonus:?}, {before:?}"
            );
        }
    }
}

#[test]
fn murloc_holmes_decoy_is_a_likely_minion_the_opponent_did_not_have() {
    // The wrong option is a plausible decoy - Tier up to the next opponent's, of their main type or
    // typeless, a minion they did not have. The right one is their minion as it was (Golden stays
    // Golden); the decoy is Golden as often as their warband's minions are.
    for golden in [false, true] {
        let mut g = game(9930);
        g.set_hero_power(0, 0, cid("BG23_HERO_303p2"));
        for p in 0..8 {
            set_board(&mut g, p, &["BG36_345"]);
        }
        end_turn_all(&mut g);
        // Quilboar 1-4, a typeless minion, a Beast.
        g.universe = [
            "BG20_100", "BG33_430", "BG31_326", "BG34_682", "BG36_345", "BG36_201",
        ]
        .iter()
        .map(|x| cid(x))
        .collect();
        for c in g.universe.clone() {
            g.pool.put(c, 5);
        }
        g.lobby_races = Races(Races::QUILBOAR.0 | Races::BEAST.0);
        let o = g.players[0].next_opponent.unwrap();
        let theirs: Vec<&str> = if golden {
            vec!["BG20_100_G", "BG33_430_G"]
        } else {
            vec!["BG20_100", "BG33_430"]
        };
        let board: Vec<Minion> = theirs.iter().map(|x| minion(&mut g, o, x)).collect();
        g.players[o].seen_board = board;
        g.players[o].tier = 3;
        let decoys: Vec<&str> = if golden {
            vec!["BG31_326_G", "BG36_345_G"]
        } else {
            vec!["BG31_326", "BG36_345"]
        };
        for _ in 0..60 {
            g.players[0].gold = 10;
            g.players[0].hand.clear();
            g.players[0].powers[0].uses_turn = 0;
            g.apply(0, Action::HeroPower(0)).unwrap();
            let Some(Pending::Discover { options, .. }) = g.players[0].pending.last().cloned()
            else {
                panic!("expected the guess")
            };
            let ids: Vec<String> = options
                .iter()
                .map(|o| db().get(o.card()).id.clone())
                .collect();
            let right = ids
                .iter()
                .position(|x| theirs.contains(&x.as_str()))
                .expect("one option is theirs, Golden kept");
            assert!(decoys.contains(&ids[1 - right].as_str()), "{ids:?}");
            g.apply(0, Action::Choose(right)).unwrap();
            assert_eq!(
                hand_ids(&g, 0),
                vec!["BG28_810"],
                "a right guess: a Tavern Coin"
            );
        }
    }
}

#[test]
fn nraqi_frostcaller_raises_the_spell_bonus_before_the_discard() {
    // A Sludge Corrosion discarded by N'raqi Frostcaller casts
    // twice with the bonus that discard gives (+1/+1, golden +2/+2).
    for (frost, bonus) in [("BG36_300", 1), ("BG36_300_G", 2)] {
        let mut g = game(9940);
        g.cfg.triple_copies = 99;
        set_board(&mut g, 0, &[frost, "BG36_345"]);
        g.players[0].hand.clear();
        give(&mut g, 0, "BG36_301t"); // Sludge Corrosion: +1/+1; discarded: cast twice
        let (a0, h0) = (g.players[0].board[1].atk, g.players[0].board[1].hp);
        g.apply(0, Action::Activate(0)).unwrap();
        g.apply(0, Action::Choose(0)).unwrap();
        let m = &g.players[0].board[1];
        let per_cast = 1 + bonus;
        assert_eq!(
            (m.atk - a0, m.hp - h0),
            (2 * per_cast, 2 * per_cast),
            "{frost}"
        );
        assert_eq!(g.players[0].stats.spell_atk, bonus);
        assert!(g.players[0].hand.is_empty());
    }
}

#[test]
fn fodder_comes_with_the_refresh_button_only() {
    // "Add a Fodder to your next 3 Refreshes" - the Refresh button (paid or free); the turn-start
    // refill and an effect's Refresh bring none.
    let fodder = |g: &Game| {
        g.players[0]
            .shop
            .iter()
            .filter(|s| s.item.card() == cid("BG35_150t"))
            .count()
    };
    let mut g = game(9941);
    set_board(&mut g, 0, &["BG36_345"]); // no Demon: the Fodder stays in the Tavern
    g.with_recruit(0, |c| c.add_fodder(0, 1, 3));
    end_turn_all(&mut g);
    assert_eq!(fodder(&g), 0, "the turn-start refill brings no Fodder");
    g.with_recruit(0, |c| c.refresh_shop_ex(0, false));
    assert_eq!(fodder(&g), 0, "nor an effect's Refresh");
    assert_eq!(g.players[0].fodder_queue, vec![1, 1, 1]);
    for k in 0..4 {
        g.players[0].gold = 10;
        g.apply(0, Action::Refresh).unwrap();
        assert_eq!(fodder(&g), usize::from(k < 3), "press {k}");
    }
}

#[test]
fn every_four_turns_powers_count_the_games_turns() {
    // Xavius's and Tickatus's powers fire on the game's Turns 4, 8... also when gained later. A
    // power picked from Genn's Turn 4 offer fires at once; one a spell swaps in waits for its next
    // turn.
    let discover_from = |g: &Game, src: &str| {
        matches!(g.players[0].pending.last(),
            Some(Pending::Discover { source, .. }) if *source == cid(src))
    };
    for power in ["BG36_HERO_105p", "TB_BaconShop_HP_106"] {
        let mut g = game(9942);
        end_turn_all(&mut g);
        assert_eq!(g.turn, 2);
        g.set_hero_power(0, 0, cid(power));
        end_turn_all(&mut g);
        assert!(!discover_from(&g, power), "{power}: not on Turn 3");
        end_turn_all(&mut g);
        assert_eq!(g.turn, 4);
        assert!(discover_from(&g, power), "{power}: Turn 4, not 2 + 4");
    }
    // Genn: the first pick of the Turn 4 offer is Xavius's power - it fires now.
    let mut g = game(9943);
    g.set_hero_power(0, 0, cid("BG35_HERO_001p"));
    for _ in 0..3 {
        end_turn_all(&mut g);
    }
    assert_eq!(g.turn, 4);
    let Some(Pending::HeroPowerChoice { options, .. }) = g.players[0].pending.last_mut() else {
        panic!("Genn's offer")
    };
    options[0] = cid("BG36_HERO_105p");
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(
        discover_from(&g, "BG36_HERO_105p"),
        "picked from Genn: fires on Turn 4"
    );
    // A spell's swap on Turn 4: no Discover until Turn 8.
    let mut g = game(9944);
    for _ in 0..3 {
        end_turn_all(&mut g);
    }
    assert_eq!(g.turn, 4);
    g.players[0].pending.clear();
    g.players[0].hand.clear();
    give(&mut g, 0, "EBG_Spell_037"); // Discover a new Hero Power
    play_last(&mut g, 0);
    let Some(Pending::HeroPowerChoice { options, .. }) = g.players[0].pending.last_mut() else {
        panic!("the spell's offer")
    };
    options[0] = cid("BG36_HERO_105p");
    g.apply(0, Action::Choose(0)).unwrap();
    assert!(g.players[0].pending.is_empty(), "a spell's swap waits");
}

#[test]
fn warband_whistle_copies_the_whole_warband_first() {
    // The Whistle's Refresh holds a plain copy of every warband minion, beyond the Tavern's usual
    // minion count; the usual cards come after while there is room in the 7 slots - seven copies,
    // no spell.
    let board7 = [
        "BG36_345", "BG19_010", "BG20_100", "BG33_430", "BG31_326", "BG36_201", "BG29_888",
    ];
    for (board, tier, spells) in [(&board7[..], 3, 0), (&board7[..5], 1, 1)] {
        let mut g = game(9945);
        g.players[0].tier = tier;
        set_board(&mut g, 0, board);
        buy_trinket(&mut g, 0, "BG35_MagicItem_930");
        g.apply(0, Action::Refresh).unwrap();
        let mut got: Vec<CardId> = g.players[0]
            .shop
            .iter()
            .filter_map(|s| s.item.as_minion())
            .map(|m| m.card)
            .collect();
        got.sort();
        let mut want: Vec<CardId> = board.iter().map(|x| cid(x)).collect();
        want.sort();
        assert_eq!(got, want, "Tier {tier}: every warband minion, nothing else");
        let n_spells = g.players[0]
            .shop
            .iter()
            .filter(|s| s.item.as_minion().is_none())
            .count();
        assert_eq!(n_spells, spells, "Tier {tier}");
    }
}

#[test]
fn victorious_geomant_overflow_gems_are_not_your_spells() {
    // The Blood Gems that don't fit are cast by the effect - the client counts none of them as your
    // spells.
    let mut g = game(9950);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 5;
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG36_345", "BG36_370"]);
    for _ in 0..8 {
        give(&mut g, 0, "BG28_897");
    }
    let (cast, turn) = (
        g.players[0].stats.spells_cast,
        g.players[0].stats.spells_cast_turn,
    );
    g.apply(0, Action::Activate(1)).unwrap();
    assert_eq!(
        g.players[0].board[0].gems, 4,
        "four Gems did not fit and landed"
    );
    assert_eq!(
        (
            g.players[0].stats.spells_cast,
            g.players[0].stats.spells_cast_turn
        ),
        (cast, turn)
    );
}

#[test]
fn victorious_geomant_overflow_gems_cast_with_balinda() {
    // With a Golden Balinda and a Hot-Air Surveyor, the one Gem that did not fit was cast three
    // times on the left-most minion - Balinda's three times, not the Surveyor's (not played from
    // hand) - and still counts as no spell of yours.
    let mut g = game(9952);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 5;
    g.players[0].hand.clear();
    set_board(
        &mut g,
        0,
        &["BG36_345", "BG35_883_G", "BG30_121", "BG36_370"],
    );
    for _ in 0..5 {
        give(&mut g, 0, "BG28_897");
    }
    let cast = g.players[0].stats.spells_cast;
    g.apply(0, Action::Activate(3)).unwrap();
    assert_eq!(g.players[0].hand.len(), 10);
    assert_eq!(g.players[0].board[0].gems, 3, "one Gem, cast three times");
    assert_eq!(g.players[0].stats.spells_cast, cast);
}

#[test]
fn a_magnetized_minion_brings_its_this_game_bonuses() {
    // An Undead Prosthetic Hand Magnetized onto an Undead adds the Attack it has, its Undead Attack
    // bonus included - the host already has its own. They were subtracted (565 Attack short).
    let mut g = game(9951);
    g.cfg.triple_copies = 99;
    g.players[0].hand.clear();
    g.players[0].stats.undead_atk = 50;
    set_board(&mut g, 0, &["BG30_125"]); // Cadaver Caretaker, an Undead
    g.with_recruit(0, |_| {}); // (player bonuses are re-applied around it)
    let host = g.players[0].board[0].atk;
    give(&mut g, 0, "BG_DEEP_015"); // Prosthetic Hand 3/1, Undead and Mech
    g.with_recruit(0, |_| {}); // (player bonuses are re-applied around it)
    let hand = g.players[0].hand[0].item.as_minion().unwrap().atk;
    assert_eq!(hand, 3 + 50, "the Undead bonus is on the Hand in hand");
    g.apply(0, Action::Magnetize { hand: 0, board: 0 }).unwrap();
    assert_eq!(g.players[0].board[0].atk, host + hand);
}

#[test]
fn natural_blessing_on_an_amalgam_reaches_every_minion_with_a_type() {
    // "all minions that share a type with it" - an Amalgam has every type. It was narrowed to one
    // random type.
    let mut g = game(9952);
    g.cfg.triple_copies = 99;
    g.players[0].hand.clear();
    // An Amalgam, an Aberration, a Quilboar, a typeless minion.
    set_board(&mut g, 0, &["BG36_640", "BG36_104", "BG30_121", "BG36_345"]);
    let before: Vec<i32> = g.players[0].board.iter().map(|m| m.hp).collect();
    give(&mut g, 0, "BG28_845");
    cast_on(&mut g, 0, 0);
    let gained: Vec<i32> = g.players[0]
        .board
        .iter()
        .zip(&before)
        .map(|(m, b)| m.hp - b)
        .collect();
    assert!(
        gained[1] >= 1 && gained[2] >= 1,
        "{gained:?}: every typed minion"
    );
    assert_eq!(
        gained[3], 0,
        "{gained:?}: not the typeless one (no Tea Set reaches it)"
    );
}

#[test]
fn type_sharing_spells_choose_only_minions_with_a_type() {
    // The server's targets for Eonar's Favor and Natural Blessing never held the typeless (golden)
    // Balinda.
    for spell in ["BG35_912", "BG28_845"] {
        let mut g = game(9953);
        g.cfg.triple_copies = 99;
        g.players[0].hand.clear();
        set_board(&mut g, 0, &["BG35_883", "BG36_104"]); // Balinda (typeless), an Aberration
        give(&mut g, 0, spell);
        let hand = g.players[0].hand.len() - 1;
        g.apply(0, Action::Play { hand, pos: 0 }).unwrap();
        let Some(Pending::Target { options, .. }) = g.players[0].pending.last().cloned() else {
            panic!("{spell}: a target choice")
        };
        let (balinda, aberration) = (g.players[0].board[0].eid, g.players[0].board[1].eid);
        assert!(!options.contains(&balinda), "{spell}");
        assert!(options.contains(&aberration), "{spell}");
    }
}

#[test]
fn a_runaway_recursion_panics_with_its_trail_instead_of_overflowing_the_stack() {
    // A worker that runs out of stack is a bare SIGSEGV in the Python extension; the
    // guard panics first, naming the entry points of the loop, and writes the game to a file.
    fn dive(ctx: &mut taverngym::engine::Ctx, n: u32) -> u32 {
        let pad = std::hint::black_box([n as u8; 4096]);
        ctx.guard(taverngym::guard::Site::Summon, u32::from(cid("BG25_001").0));
        if n > 1_000_000 {
            return n;
        }
        dive(ctx, n + 1) + u32::from(pad[0])
    }
    let mut g = game(9003);
    let size = 16 << 20;
    let out = std::thread::Builder::new()
        .stack_size(size)
        .spawn(move || {
            taverngym::guard::thread_started(size);
            g.with_recruit(0, |ctx| dive(ctx, 0))
        })
        .unwrap()
        .join();
    let msg = *out
        .expect_err("the guard panics")
        .downcast::<String>()
        .unwrap();
    assert!(msg.contains("engine recursion guard"), "{msg}");
    assert!(msg.contains("64x summon BG25_001"), "{msg}");
    let file = msg.rsplit("game written to ").next().unwrap();
    let dump = std::fs::read_to_string(file).unwrap();
    std::fs::remove_file(file).unwrap();
    let dump: serde_json::Value = serde_json::from_str(&dump).unwrap();
    assert_eq!(dump["trail"].as_array().unwrap().len(), 64);
    assert!(dump["game"]["players"].is_array());
}

#[test]
fn new_recruit_offers_its_extra_minion_at_once_and_with_every_refresh() {
    // Right after the cast a sixth Tavern minion (+2/+2) stands beside the five already there.
    let mut g = game(9101);
    g.players[0].gold = 20;
    let before: Vec<u32> = g.players[0].shop.iter().map(|s| s.sid).collect();
    give(&mut g, 0, "BGS_Treasures_033");
    play_last(&mut g, 0);
    let shop = &g.players[0].shop;
    assert_eq!(shop.len(), before.len() + 1);
    assert!(
        before.iter().all(|sid| shop.iter().any(|s| s.sid == *sid)),
        "the cards there stay"
    );
    let Item::Minion(m) = &shop.last().unwrap().item else {
        panic!("the extra offer is a minion")
    };
    let d = db().get(m.card);
    assert_eq!((m.atk, m.hp), (d.atk + 2, d.hp + 2));
    assert!(d.tier <= g.players[0].tier);
    // Every Refresh: the usual fill and one more +2/+2 minion.
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(g.players[0].shop.len(), before.len() + 1);
}

#[test]
fn a_boon_of_beetles_cast_while_one_is_pending_does_not_happen() {
    // With 2 Beetles left, a Magicfin Apprentice taught Boon of Beetles played beside Brann: its
    // two casts stay set aside, still 2 left.
    let mut g = game(9102);
    let boon = cid("BG28_603");
    let left = |g: &Game| {
        g.players[0]
            .effects
            .iter()
            .find(|x| x.card == boon)
            .map_or(0, |x| x.stacks)
    };
    let teach = |g: &mut Game| {
        let mut apprentice = minion(g, 0, "BG33_890t");
        apprentice.counter = i32::from(boon.0) + 1; // the taught spell (CardId + 1)
        g.with_recruit(0, |c| c.add_to_hand(0, Item::Minion(apprentice)));
        play_last(g, 0);
    };
    // Nothing pending: the Apprentice's cast adds the two Beetles.
    teach(&mut g);
    assert_eq!(left(&g), 2);
    let mut g = game(9102);
    g.players[0].gold = 20;
    give(&mut g, 0, "BG28_603");
    play_last(&mut g, 0);
    assert_eq!(left(&g), 2);
    set_board(&mut g, 0, &["BG_LOE_077"]); // Brann
    teach(&mut g);
    assert_eq!(g.players[0].board.len(), 2, "the Apprentice is played");
    assert_eq!(left(&g), 2, "neither cast adds Beetles");
    // A cast by an effect is refused the same way when nothing else is in the way.
    g.with_recruit(0, |c| c.cast_spell(0, boon, 0, 0, 0));
    assert_eq!(left(&g), 2);
}

// Trinket offers and lifetimes.

fn deity_1_1(g: &mut Game) {
    g.players[0].deity = Some(taverngym::state::DeityState {
        card: cid("BGFYM_000"),
        atk: 1,
        hp: 1,
        golden: false,
        reborn: false,
    });
}

fn deity_stats(g: &Game) -> (i32, i32) {
    let d = g.players[0].deity.clone().unwrap();
    (d.atk, d.hp)
}

fn grant(g: &mut Game, id: &str) {
    g.with_recruit(0, |c| taverngym::content::grant_trinket(c, 0, cid(id)));
}

fn full_hand_of_coins(g: &mut Game) {
    g.players[0].hand.clear();
    for _ in 0..g.cfg.hand_size {
        give(g, 0, "BG28_810");
    }
}

#[test]
fn trinket_two_cubes_turning_into_one_trinket_keep_their_own_state() {
    // Two Mystery Cubes whose offers of the turn both hold Transcribing Typewriter, and both
    // swaps take it: two Typewriters, each with its own two copies. Two Prisonguards bought
    // get two extra copies each - two triples (they shared one counter: one Golden).
    let mut g = game(10101);
    let (cube, typewriter) = (cid("BG30_MagicItem_703"), cid("BG35_MagicItem_931"));
    let (a, b) = (cid("BG30_MagicItem_879"), cid("BG30_MagicItem_426"));
    g.with_recruit(0, |c| {
        for became in [a, b] {
            taverngym::content::grant_trinket(c, 0, cube);
            c.player(0).unwrap().pending.clear();
            taverngym::content::replace_trinket(c, 0, cube, became);
        }
    });
    for old in [a, b] {
        g.players[0].pending.push(Pending::Trinket {
            options: vec![typewriter],
            free: true,
            replace: Some(old),
        });
        g.apply(0, Action::Choose(0)).unwrap();
    }
    let p = &g.players[0];
    assert_eq!(p.trinket_count(typewriter), 2);
    assert_ne!(p.trinkets[0], p.trinkets[1], "two instances, two keys");
    g.players[0].hand.clear();
    g.players[0].shop.clear();
    g.players[0].gold = 20;
    put_shop(&mut g, 0, "BG36_345");
    put_shop(&mut g, 0, "BG36_345");
    g.apply(0, Action::Buy(0)).unwrap();
    g.apply(0, Action::Buy(0)).unwrap();
    let goldens = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .filter(|m| m.golden && m.is_card("BG36_345"))
        .count();
    assert_eq!(goldens, 2);
}

#[test]
fn trinket_a_staff_got_again_starts_over() {
    // Dragonhead Staff earned in combat (its level marks it) becomes a Greater Dragon Trinket
    // at the next turn start. The same Staff got again later has no attack yet: it stays.
    let mut g = game(10102);
    let staff = cid("BG36_MagicItem_407");
    grant(&mut g, "BG36_MagicItem_407");
    g.players[0].levels.push((staff, 1));
    end_turn_all(&mut g);
    assert!(!g.players[0].has_trinket_id(staff));
    grant(&mut g, "BG36_MagicItem_407");
    g.players[0].board.clear();
    end_turn_all(&mut g);
    assert!(
        g.players[0].has_trinket_id(staff),
        "replaced again by the old mark"
    );
}

#[test]
fn trinket_a_souvenir_whistle_copy_gives_its_own_copy_page() {
    // Souvenir Stand copies Warband Whistle: two free Refreshes, each with plain copies of
    // the warband (the copy's Refresh was an ordinary page).
    let mut g = game(10103);
    g.cfg.triple_copies = 99;
    let warband = ["BG36_345", "BG25_001", "BG26_805"];
    set_board(&mut g, 0, &warband);
    buy_trinket(&mut g, 0, "BG30_MagicItem_888");
    buy_trinket(&mut g, 0, "BG35_MagicItem_930");
    assert_eq!(g.players[0].trinket_count(cid("BG35_MagicItem_930")), 2);
    for press in 0..2 {
        g.apply(0, Action::Refresh).unwrap();
        let shop: Vec<String> = shop_minions(&g, 0).into_iter().map(|x| x.0).collect();
        for id in warband {
            let name = db().get(cid(id)).name.clone();
            assert!(
                shop.contains(&name),
                "press {press}: no copy of {name} in {shop:?}"
            );
        }
    }
}

#[test]
fn trinket_a_souvenir_candle_copy_shows_what_is_left() {
    // Souvenir Stand copies Guiding Candle; after three Refreshes neither shows a Tier 6
    // Refresh left (the copy showed "(2 left!)" all turn).
    let mut g = game(10104);
    buy_trinket(&mut g, 0, "BG30_MagicItem_888");
    buy_trinket(&mut g, 0, "BG32_MagicItem_366");
    for _ in 0..3 {
        g.players[0].gold = 10;
        g.apply(0, Action::Refresh).unwrap();
    }
    let view = taverngym::debug::view(&g, 0);
    let trinkets = view["me"]["trinkets"].as_array().unwrap();
    assert_eq!(trinkets.len(), 2);
    for t in trinkets {
        let numbers = taverngym::debug::text_numbers(t["text"].as_str().unwrap());
        assert_eq!(numbers.last(), Some(&0), "{}", t["text"]);
    }
}

#[test]
fn trinket_two_escapee_portraits_shorten_a_new_lockbox_twice() {
    // A Souvenir Stand copy of Escapee Portrait is a second Portrait: a new Lockbox opens
    // in 5 - 1 - 1 = 3 turns.
    let mut g = game(10105);
    buy_trinket(&mut g, 0, "BG30_MagicItem_888");
    buy_trinket(&mut g, 0, "BG36_MagicItem_363");
    g.players[0].hand.clear();
    grant(&mut g, "BG36_MagicItem_301"); // Lockbox Portrait: get a Lockbox
    let turns: Vec<i32> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| match h.item {
            Item::Spell { card, data } if card == cid("BG36_520t") => Some(data),
            _ => None,
        })
        .collect();
    assert_eq!(turns, vec![3]);
}

#[test]
fn trinket_putricide_sticker_creation_waits_for_hand_space() {
    // A trinket's cards wait for hand space; the Creation the Sticker crafts through the two
    // Discover steps of the Hero Power too.
    let mut g = game(10106);
    g.players[0].tier = 6;
    full_hand_of_coins(&mut g);
    grant(&mut g, "BG32_MagicItem_300");
    g.apply(0, Action::Choose(0)).unwrap(); // the base
    g.apply(0, Action::Choose(0)).unwrap(); // the part
    let queued: Vec<String> = g.players[0]
        .hand_queue
        .iter()
        .map(|x| db().get(x.card()).id.clone())
        .collect();
    assert_eq!(queued, vec!["BG25_HERO_100pt"]);
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap(); // a Coin makes room
    assert!(hand_ids(&g, 0).contains(&"BG25_HERO_100pt".to_string()));
}

#[test]
fn trinket_a_spellcraft_trinket_spell_waits_for_hand_space() {
    // Ophidian Staff with a full hand: its Spellcraft spell waits, arrives temporary when a
    // Coin is played, and leaves with the turn.
    let mut g = game(10107);
    full_hand_of_coins(&mut g);
    buy_trinket(&mut g, 0, "BG35_MagicItem_872");
    assert_eq!(g.players[0].hand_queue.len(), 1);
    g.apply(0, Action::Play { hand: 0, pos: 0 }).unwrap();
    let h = g.players[0].hand.last().unwrap();
    assert_eq!(db().get(h.item.card()).id, "BG35_MagicItem_872t");
    assert!(h.temporary);
    g.apply(0, Action::EndTurn).unwrap();
    assert!(!hand_ids(&g, 0).contains(&"BG35_MagicItem_872t".to_string()));
    // Still waiting when the turn ends: it leaves the queue as it would leave the hand.
    let mut g = game(10108);
    full_hand_of_coins(&mut g);
    buy_trinket(&mut g, 0, "BG35_MagicItem_872");
    assert_eq!(g.players[0].hand_queue.len(), 1);
    g.apply(0, Action::EndTurn).unwrap();
    assert!(g.players[0].hand_queue.is_empty());
}

#[test]
fn trinket_a_discover_waiting_for_hand_space_is_discovered() {
    // Full hand, Sinstone Sticker, Innkeeper's Hearth's Discover: the pick waits for space,
    // it counts as Discovered, and Sinstone's extra copy waits too.
    let mut g = game(10109);
    g.players[0].tier = 6;
    grant(&mut g, "BG30_MagicItem_801");
    full_hand_of_coins(&mut g);
    grant(&mut g, "BG32_MagicItem_362t");
    g.apply(0, Action::Choose(0)).unwrap();
    let queued: Vec<(CardId, i32, i32)> = g.players[0]
        .hand_queue
        .iter()
        .filter_map(|x| x.as_minion())
        .map(|m| (m.card, m.atk, m.hp))
        .collect();
    assert_eq!(queued.len(), 2, "the pick and its copy");
    assert_eq!(queued[0], queued[1]);
    assert_eq!((queued[0].1, queued[0].2), (30, 30));
    let used = g.players[0]
        .turn_data
        .iter()
        .find(|x| x.0 == cid("BG30_MagicItem_801"))
        .map_or(0, |x| x.1);
    assert_eq!(used, 1, "one of Sinstone's two uses");
}

#[test]
fn trinket_sinstone_copy_of_a_gifted_pick_has_the_gift() {
    // Maldraxxus Dagger Discovers a plain copy with Harpy's Talons (Divine Shield, Windfury);
    // Sinstone's full copy of it has them too.
    // The Golden Wrath Weaver on the board keeps the two plain ones from tripling.
    let mut g = game(10110);
    g.cfg.triple_copies = 99;
    g.turn = 12;
    g.players[0].tier = 6;
    g.players[0].hand.clear();
    g.gift_universe = vec![cid("BG36_MidGameEffect_000t13")];
    let weaver = g.with_recruit(0, |c| c.new_minion(0, db().golden_of(cid("BGS_004"))));
    g.players[0].board = vec![weaver];
    grant(&mut g, "BG30_MagicItem_801");
    grant(&mut g, "BG36_MagicItem_370");
    g.apply(0, Action::Choose(0)).unwrap();
    let hand: Vec<(bool, bool)> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .map(|m| (m.has(Kw::DIVINE_SHIELD), m.has(Kw::WINDFURY)))
        .collect();
    assert_eq!(hand, vec![(true, true), (true, true)]);
}

#[test]
fn trinket_glass_of_perspective_draws_minions_by_copies_left() {
    // Two Choose One minions with 1 and 13 copies left (no Choose One spell): the random
    // card follows the copies (about 1 in 14), not the kinds (1 in 2).
    use taverngym::types::Mech;
    let choose_one = |g: &Game| -> Vec<CardId> {
        g.universe
            .iter()
            .copied()
            .filter(|x| db().get(*x).mech.has(Mech::CHOOSE_ONE))
            .collect()
    };
    let mut g = (10111..10200)
        .map(game)
        .find(|g| choose_one(g).len() >= 2)
        .unwrap();
    g.players[0].tier = 6;
    g.spell_universe
        .retain(|x| !db().get(*x).mech.has(Mech::CHOOSE_ONE));
    let co = choose_one(&g);
    let (rare, common) = (co[0], co[1]);
    let glass = cid("BG36_MagicItem_303");
    let fx = taverngym::script::script(glass).unwrap().on_attach.unwrap();
    let mut rares = 0;
    for _ in 0..200 {
        for x in &co {
            g.pool.counts[x.0 as usize] = 0;
        }
        g.pool.counts[rare.0 as usize] = 1;
        g.pool.counts[common.0 as usize] = 13;
        g.players[0].hand.clear();
        g.with_recruit(0, |c| {
            let me = taverngym::script::Me {
                side: 0,
                eid: 0,
                card: glass,
                golden: false,
                opt: 0,
                pos: 0,
                data: 0,
            };
            fx(c, me)
        });
        let got = g.players[0].hand[0].item.card();
        if got == rare {
            rares += 1;
        } else {
            assert_eq!(got, common);
        }
    }
    assert!(
        rares < 50,
        "{rares} of 200 drew the card with one copy left"
    );
}

#[test]
fn trinket_bronze_timepiece_at_the_largest_attack() {
    // Half of 2,147,483,647 rounded up is 1,073,741,824: the Health is added, not wrapped
    // into a loss that killed the minion.
    let mut g = game(10112);
    buy_trinket(&mut g, 0, "BG30_MagicItem_995");
    let mut m = minion(&mut g, 0, "BG36_345");
    m.set_stats(i32::MAX, 100);
    g.players[0].board = vec![m];
    let mut c = combat_ctx(&mut g);
    c.run_player_hook(0, |s| s.start_of_combat, "soc");
    assert_eq!(c.boards[0][0].hp, 100 + 1_073_741_824);
}

#[test]
fn trinket_weighted_gauntlet_doubles_past_two_to_the_twentieth() {
    // The 2^20 cap was a safety limit; stats saturate at i32::MAX.
    let mut g = game(10113);
    buy_trinket(&mut g, 0, "BG36_MagicItem_414");
    let gauntlet = cid("BG36_MagicItem_414");
    let x = |g: &mut Game| g.with_recruit(0, |c| c.card_data(0, gauntlet));
    g.with_recruit(0, |c| c.set_card_data(0, gauntlet, 1 << 20));
    end_turn_all(&mut g);
    assert_eq!(x(&mut g), 1 << 21);
    g.with_recruit(0, |c| c.set_card_data(0, gauntlet, 1 << 30));
    end_turn_all(&mut g);
    assert_eq!(x(&mut g), i32::MAX);
}

#[test]
fn trinket_gold_plated_compass_keeps_aureate_laureate_rewardless() {
    // Aureate Laureate is always Golden "but doesn't give a Triple Reward"; the Compass
    // (Pirates) can't gild it again and must not give it the reward.
    let mut g = game(10114);
    buy_trinket(&mut g, 0, "BG32_MagicItem_901");
    let compass = cid("BG32_MagicItem_901");
    g.with_recruit(0, |c| c.set_card_data(0, compass, Races::PIRATE.0 as i32));
    g.players[0].hand.clear();
    g.players[0].shop.clear();
    g.players[0].gold = 10;
    put_shop(&mut g, 0, "BG32_236");
    g.apply(0, Action::Buy(0)).unwrap();
    play_last(&mut g, 0);
    assert_eq!(triple_rewards(&g, 0), 0);
}

#[test]
fn trinket_a_brann_from_the_tavern_has_the_portrait_types() {
    // Bronzebeard Portrait came while a Brann was in the Tavern: bought, it is a Murloc and a
    // Dragon like the others.
    let mut g = game(10115);
    g.players[0].shop.clear();
    put_shop(&mut g, 0, "BG_LOE_077");
    buy_trinket(&mut g, 0, "BG30_MagicItem_418");
    g.players[0].hand.clear();
    g.players[0].gold = 10;
    g.apply(0, Action::Buy(0)).unwrap();
    let brann = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .find(|m| m.is_card("BG_LOE_077"))
        .unwrap();
    assert!(brann.is(Races::MURLOC) && brann.is(Races::DRAGON));
}

#[test]
fn trinket_makeshift_master_on_a_tavern_minion() {
    // "Choose a minion" may choose a Tavern minion: a Blood Gem on it gives the Deity +1/+1 too.
    let mut g = game(10116);
    g.players[0].hand.clear();
    g.players[0].shop.clear();
    deity_1_1(&mut g);
    put_shop(&mut g, 0, "BG36_345");
    let sid = g.players[0].shop[0].sid;
    buy_trinket(&mut g, 0, "BG36_MagicItem_417");
    play_last(&mut g, 0);
    choose_target(&mut g, 0, sid);
    give(&mut g, 0, "BG20_GEM");
    play_last(&mut g, 0);
    choose_target(&mut g, 0, sid);
    let m = g.players[0].shop[0].item.as_minion().unwrap();
    assert_eq!((m.atk, m.hp), (4, 4));
    assert_eq!(deity_stats(&g), (2, 2));
}

#[test]
fn trinket_makeshift_master_and_a_this_game_bonus() {
    // Butchering on another Undead: "your Undead have +8 Attack this game" raises the marked
    // Risen Rider from 2/1 to 10/1, and the Deity gains it too.
    let mut g = game(10117);
    g.cfg.triple_copies = 99;
    g.players[0].hand.clear();
    deity_1_1(&mut g);
    let mut riders: Vec<Minion> = (0..2).map(|_| minion(&mut g, 0, "BG25_001")).collect();
    for m in riders.iter_mut() {
        m.remove(Kw::REBORN);
    }
    g.players[0].board = riders;
    buy_trinket(&mut g, 0, "BG36_MagicItem_417");
    cast_on(&mut g, 0, 0);
    give(&mut g, 0, "BG28_604");
    cast_on(&mut g, 0, 1);
    assert_eq!(stats(&g, 0), vec![(10, 1)]);
    assert_eq!(deity_stats(&g), (9, 1));
}

#[test]
fn trinket_each_makeshift_master_spell_is_its_own_effect() {
    // A Souvenir Stand copy gives a second spell. Two marks on two minions both act (a Blood
    // Gem on the first: Deity +1/+1); two on one minion give the Deity its gain twice.
    for same in [false, true] {
        let mut g = game(10118);
        g.cfg.triple_copies = 99;
        g.players[0].hand.clear();
        deity_1_1(&mut g);
        set_board(&mut g, 0, &["BG36_345", "BG36_345"]);
        buy_trinket(&mut g, 0, "BG30_MagicItem_888");
        buy_trinket(&mut g, 0, "BG36_MagicItem_417");
        assert_eq!(
            hand_ids(&g, 0),
            vec!["BG36_MagicItem_417t", "BG36_MagicItem_417t"]
        );
        cast_on(&mut g, 0, 0);
        cast_on(&mut g, 0, if same { 0 } else { 1 });
        give(&mut g, 0, "BG20_GEM");
        cast_on(&mut g, 0, 0);
        let want = if same { (3, 3) } else { (2, 2) };
        assert_eq!(deity_stats(&g), want, "same target: {same}");
    }
}

#[test]
fn trinket_lovely_locket_recasts_a_spell_that_destroyed_its_target() {
    // Butchering on a friendly Undead destroys it; the cast was on a friendly minion all the
    // same, so the Locket casts it on the other one.
    let mut g = game(10119);
    g.cfg.triple_copies = 99;
    buy_trinket(&mut g, 0, "BG36_MagicItem_211");
    let mut riders: Vec<Minion> = (0..2).map(|_| minion(&mut g, 0, "BG25_001")).collect();
    for m in riders.iter_mut() {
        m.remove(Kw::REBORN);
    }
    g.players[0].board = riders;
    g.players[0].hand.clear();
    give(&mut g, 0, "BG28_604");
    cast_on(&mut g, 0, 0);
    assert!(g.players[0].board.is_empty());
    assert_eq!(g.players[0].stats.undead_atk, 16);
}

#[test]
fn trinket_safety_patch_casts_ice_block() {
    // "Cast Ice Block": your spell - it counts, and Replica Cathedral's first spell of the
    // turn casts it twice (the Secret is in play once).
    for cathedral in [false, true] {
        let mut g = game(10120);
        if cathedral {
            grant(&mut g, "BG30_MagicItem_434");
        }
        let before = g.players[0].stats.spells_cast;
        grant(&mut g, "BG35_MagicItem_820");
        let ice = cid("TB_Bacon_Secrets_12");
        assert_eq!(g.players[0].secrets, vec![ice]);
        let casts = if cathedral { 2 } else { 1 };
        assert_eq!(g.players[0].stats.spells_cast, before + casts);
    }
}

#[test]
fn trinket_innkeepers_hearth_discovers_from_the_pool() {
    // One Tier 6 card has a copy left: both Discovers offer only it, at 30/30 (they offered
    // three cards each, uniformly, copies or not).
    let mut g = game(10121);
    let six: Vec<CardId> = g
        .universe
        .iter()
        .copied()
        .filter(|x| db().get(*x).tier == 6)
        .collect();
    for x in &six {
        g.pool.counts[x.0 as usize] = 0;
    }
    g.pool.counts[six[0].0 as usize] = 1;
    buy_trinket(&mut g, 0, "BG32_MagicItem_362t");
    let offers: Vec<Vec<(CardId, i32, i32)>> = g.players[0]
        .pending
        .iter()
        .filter_map(|p| match p {
            Pending::Discover { options, .. } => Some(
                options
                    .iter()
                    .filter_map(|o| o.as_minion())
                    .map(|m| (m.card, m.atk, m.hp))
                    .collect(),
            ),
            _ => None,
        })
        .collect();
    assert_eq!(offers, vec![vec![(six[0], 30, 30)]; 2]);
}

#[test]
fn trinket_splinter_of_aurum_fires_when_gold_comes_in() {
    // 14 Gold, then 4 more from an effect that is not a spell, a sale or a play (a Hero
    // Power: Lucky Roll gains its Gold the same way): the Golden Tier 5 minion comes now.
    let mut g = game(10122);
    g.players[0].tier = 5;
    g.players[0].hand.clear();
    g.players[0].gold = 14;
    grant(&mut g, "BG32_MagicItem_350");
    assert!(g.players[0].hand.is_empty());
    g.with_recruit(0, |c| c.gain_gold(0, 4));
    let got: Vec<(bool, u8)> = g.players[0]
        .hand
        .iter()
        .filter_map(|h| h.item.as_minion())
        .map(|m| (m.golden, m.tier))
        .collect();
    assert_eq!(got, vec![(true, 5)]);
}

#[test]
fn trinket_menagerie_offers_count_each_minion_once() {
    // Two Beasts and an all-type Amalgam make two different types, not three: menagerie
    // trinkets are offered only as discounted pivots.
    use taverngym::cards::CardKind;
    let menagerie = [
        "BG36_MagicItem_220",
        "BG32_MagicItem_284",
        "BG36_MagicItem_373",
        "BG36_MagicItem_611",
    ];
    let mut g = (10123..10200)
        .map(game)
        .find(|g| g.lobby_races.intersects(Races::BEAST))
        .unwrap();
    g.turn = 9;
    g.players[0].tier = 4;
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG26_805", "BG27_084", "BG36_640"]);
    for _ in 0..300 {
        for t in g.trinket_offer(0, CardKind::TrinketGreater) {
            let id = db().get(t).id.as_str();
            assert!(
                !menagerie.contains(&id) || g.players[0].trinket_pivots.contains(&t),
                "{id} offered as if the warband had three types"
            );
        }
    }
}

#[test]
fn trinket_greater_defiler_portrait_gives_a_woodland_defiler() {
    // Out of the 36.6.3 pool, but scripted: "Get a Woodland Defiler" was missing.
    let mut g = game(10124);
    g.players[0].hand.clear();
    grant(&mut g, "BG35_MagicItem_151t");
    assert_eq!(hand_ids(&g, 0), vec!["BG35_151"]);
}

#[test]
fn a_lockbox_or_a_locked_card_is_never_discarded() {
    // Discarding a Lockbox or a locked card is strictly forbidden. The Lockbox keeps `locked` 0
    // (its countdown is the spell's data), so it was offered as a discard target and could be
    // picked by a random discard.
    let lb = cid(taverngym::content::LOCKBOX);
    let mut g = game(9201);
    g.players[0].gold = 10;
    g.set_hero_power(0, 0, cid("BG36_HERO_000p")); // Drest'agath: discard a card...
    let lockbox = give(&mut g, 0, "BG36_520t");
    let locked = give(&mut g, 0, "BG28_897");
    g.players[0]
        .hand
        .iter_mut()
        .find(|h| h.hid == locked)
        .unwrap()
        .locked = 2;
    assert!(
        !g.legal_mask(0)[taverngym::actions::A_HERO_POWER],
        "nothing to discard"
    );
    let banana = give(&mut g, 0, "BG28_897");
    g.apply(0, Action::HeroPower(0)).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last() else {
        panic!("the Hero Power asks for the card to discard")
    };
    assert_eq!(options, &vec![banana], "only the unlocked card");
    // The discard itself refuses them too, whatever asks for it.
    for hid in [lockbox, locked] {
        assert_eq!(
            g.with_recruit(0, |c| taverngym::recruit::discard_card(c, 0, hid)),
            None
        );
    }
    assert!(g.players[0].hand.iter().any(|h| h.item.card() == lb));
    // Timewarped Bandit discards a random spell at the start of the turn: never the Lockbox.
    let mut g = game(9202);
    give(&mut g, 0, "BG36_520t");
    set_board(&mut g, 0, &["BG34_PreMadeChamp_078"]);
    let before = g.players[0].board[0].hp;
    g.with_recruit(0, |c| {
        let s = taverngym::script::script(cid("BG34_PreMadeChamp_078")).unwrap();
        let eid = c.boards[0][0].eid;
        let me = taverngym::script::Me {
            side: 0,
            eid,
            card: cid("BG34_PreMadeChamp_078"),
            golden: false,
            opt: 0,
            pos: 0,
            data: 0,
        };
        (s.start_of_turn.unwrap())(c, me);
    });
    assert!(
        g.players[0].hand.iter().any(|h| h.item.card() == lb),
        "the Lockbox stays"
    );
    assert_eq!(
        g.players[0].board[0].hp, before,
        "no spell discarded, no Gems"
    );
}

// Start-of-Combat results as the server resolves them in replayed client combats.

#[test]
fn double_combat_gains_are_doubled_as_they_happen() {
    // A Runic Arcanist 4/5 with Tarecgosa's Blessing cast Shiny Ring twice (+1/+1 each): 8/9 before
    // the first attack, and the warband keeps the same.
    let mut g = game(9401);
    let m = gifted(&mut g, 0, "BG35_602", &["BG36_MidGameEffect_000t50"]);
    let (a0, h0) = (m.atk, m.hp);
    g.players[0].board = vec![m];
    let e = g.players[0].board[0].eid;
    let mut c = combat_ctx(&mut g);
    c.buff(e, 1, 1);
    c.buff(e, 1, 1);
    let fighting = c.minion(e).map(|m| (m.atk, m.hp)).unwrap();
    drop(c);
    assert_eq!(fighting, (a0 + 4, h0 + 4), "doubled in combat");
    assert_eq!(
        (g.players[0].board[0].atk, g.players[0].board[0].hp),
        (a0 + 4, h0 + 4)
    );
    // Training Certificate doubles a golden Tarecgosa's stats: x3.
    let mut g = game(9402);
    set_board(&mut g, 0, &["BG21_015_G"]);
    g.players[0].board[0].set_stats(2490, 2216);
    let e = g.players[0].board[0].eid;
    let mut c = combat_ctx(&mut g);
    c.buff(e, 2490, 2216);
    let fighting = c.minion(e).map(|m| (m.atk, m.hp)).unwrap();
    drop(c);
    assert_eq!(fighting, (7470, 6648));
    // A plain Tarecgosa keeps the gain once, unchanged in combat.
    let mut g = game(9403);
    set_board(&mut g, 0, &["BG21_015"]);
    let (a0, h0) = (g.players[0].board[0].atk, g.players[0].board[0].hp);
    let e = g.players[0].board[0].eid;
    let mut c = combat_ctx(&mut g);
    c.buff(e, 3, 3);
    assert_eq!(
        c.minion(e).map(|m| (m.atk, m.hp)).unwrap(),
        (a0 + 3, h0 + 3)
    );
    drop(c);
    assert_eq!(
        (g.players[0].board[0].atk, g.players[0].board[0].hp),
        (a0 + 3, h0 + 3)
    );
}

#[test]
fn hostility_doubles_the_attack_and_the_undead_bonus_counts_on_top() {
    // A Snazzy Phantom 2853 with a 273 Undead Attack bonus became 2 x 2853 + 273.
    let hostility = cid("BG36_MidGameEffect_000t71");
    for (aura, want) in [(273, 5979), (0, 5706)] {
        let mut g = game(9404);
        let mut m = gifted(&mut g, 0, "BG28_309", &["BG36_MidGameEffect_000t71"]);
        m.set_stats(2853, 2584);
        m.pbonus_atk = aura;
        g.players[0].board = vec![m];
        let e = g.players[0].board[0].eid;
        let mut c = combat_ctx(&mut g);
        let soc = taverngym::script::script(hostility)
            .and_then(|s| s.start_of_combat)
            .unwrap();
        let me = taverngym::script::Me {
            side: 0,
            eid: e,
            card: hostility,
            golden: false,
            opt: 0,
            pos: 0,
            data: 0,
        };
        soc(&mut c, me);
        assert_eq!(c.minion(e).unwrap().atk, want, "Undead bonus {aura}");
    }
}

#[test]
fn reborn_goes_right_of_the_summons_of_minions_that_died_beside_it() {
    // Mummifier (Reborn), Plaguerunner and Handless Forsaken destroyed together: the Forsaken's
    // Hand stands left of the Reborn Mummifier.
    let mut g = game(9405);
    set_board(&mut g, 0, &["BG28_309", "BG34_690", "BG25_010", "BG32_880"]);
    g.players[0].board[0].give(Kw::REBORN);
    let ids: Vec<u32> = g.players[0].board[..3].iter().map(|m| m.eid).collect();
    let mut c = combat_ctx(&mut g);
    for e in ids {
        c.destroy(e);
    }
    c.resolve_deaths();
    let order: Vec<String> = c.boards[0]
        .iter()
        .map(|m| db().get(m.card).id.clone())
        .collect();
    assert_eq!(
        &order[..3],
        ["BG25_010t", "BG28_309", "BG32_880"],
        "{order:?}"
    );
}

#[test]
fn after_you_buy_acts_before_after_you_spend_gold() {
    // A Tavern spell bought with two Magicfin Mycologists and an Enterprising Escapee one Gold
    // short of its Lockbox. The hand: the spell, the two Apprentices, then the Lockbox.
    let mut g = game(9501);
    g.players[0].gold = 20;
    set_board(&mut g, 0, &["BG36_523", "BG33_891", "BG33_891"]);
    g.players[0].board[0].counter = 5; // 1 Gold left
    g.players[0].shop.push(taverngym::state::ShopSlot::new(
        999_996,
        Item::spell(cid("BG28_897")), // Banana
    ));
    let i = g.players[0].shop.len() - 1;
    g.apply(0, Action::Buy(i)).unwrap();
    let ids: Vec<String> = g.players[0]
        .hand
        .iter()
        .map(|h| db().get(h.item.card()).id.clone())
        .collect();
    assert_eq!(
        ids,
        ["BG28_897", "BG33_890t", "BG33_890t", "BG36_520t"],
        "{ids:?}"
    );
}

#[test]
fn spark_snapper_and_batteries_give_the_clients_satellite() {
    // Their Satellite is BG31_171t (enchantment BG31_171te).
    let mut g = game(9502);
    g.players[0].gold = 20;
    g.players[0].trinkets.push(cid("BG32_MagicItem_278")); // Reusable Batteries
    let i = g.players[0]
        .shop
        .iter()
        .position(|s| matches!(s.item, Item::Minion(_)))
        .unwrap();
    g.apply(0, Action::Buy(i)).unwrap();
    assert!(
        g.players[0]
            .hand
            .iter()
            .any(|h| h.item.card() == cid("BG31_171t"))
    );
}

#[test]
fn brann_repeats_a_golden_making_battlecry_on_its_target_only() {
    // Elite Navigator with Brann, and Captain Sanders with a Golden Brann, make only their target
    // Golden - the repeat finds it Golden and chooses no other minion.
    for (card, brann) in [("BG32_231", "BG_LOE_077"), ("BG25_034", "TB_BaconUps_045")] {
        let mut g = game(9993);
        g.cfg.triple_copies = 99;
        g.players[0].hand.clear();
        set_board(&mut g, 0, &[brann, "BG26_135", "BG36_342"]);
        give(&mut g, 0, card);
        let (target, other) = (g.players[0].board[1].eid, g.players[0].board[2].eid);
        play_last_on(&mut g, 0, 1);
        let golden = |eid| {
            g.players[0]
                .board
                .iter()
                .find(|m| m.eid == eid)
                .unwrap()
                .golden
        };
        assert!(golden(target), "{card}: the target");
        assert!(!golden(other), "{card}: no second minion");
    }
    // A Golden Navigator still makes two Pirates Golden: the target and one more.
    let mut g = game(9994);
    g.cfg.triple_copies = 99;
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG26_135", "BG36_342"]);
    give(&mut g, 0, "BG32_231_G");
    play_last_on(&mut g, 0, 0);
    assert_eq!(
        g.players[0].board.iter().filter(|m| m.golden).count(),
        3,
        "itself and two Pirates"
    );
}

#[test]
fn boon_of_light_does_not_target_a_divine_shield() {
    // The client's targets leave out the minions that have Divine Shield; the rest of the board and the Tavern minions stay.
    use taverngym::types::Kw;
    let mut g = game(9995);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_010"));
    set_board(&mut g, 0, &["BG36_345", "BG36_200"]);
    g.players[0].board[0].give(Kw::DIVINE_SHIELD);
    let shielded = g.players[0].board[0].eid;
    let plain = g.players[0].board[1].eid;
    g.apply(0, Action::HeroPower(0)).unwrap();
    let Some(Pending::Target { options, .. }) = g.players[0].pending.last().cloned() else {
        panic!("a target")
    };
    assert!(!options.contains(&shielded));
    assert!(options.contains(&plain));
    let tavern: Vec<_> = g.players[0]
        .shop
        .iter()
        .filter(|s| s.item.as_minion().is_some())
        .map(|s| s.sid)
        .collect();
    assert!(!tavern.is_empty() && tavern.iter().all(|t| options.contains(t)));
}

#[test]
fn silas_tickets_give_a_prize_that_discovers_when_cast() {
    // The third ticket gives a Prize card (TB_BaconShop_HP_101t2,
    // "Discover a minion from Tier {0}") with the Tier in it; it is played later and Discovers
    // then.
    let mut g = game(9996);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 20;
    g.players[0].tier = 4;
    g.players[0].hand.clear();
    g.set_hero_power(0, 0, cid("TB_BaconShop_HP_101"));
    let slot = g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_some())
        .unwrap();
    let sid = g.players[0].shop[slot].sid;
    g.players[0].powers[0].values = vec![sid as i32];
    g.players[0].powers[0].counter = 2;
    g.apply(0, Action::Buy(slot)).unwrap();
    assert!(g.players[0].pending.is_empty(), "no Discover yet");
    let prize = g.players[0]
        .hand
        .iter()
        .position(|h| h.item.card() == cid("TB_BaconShop_HP_101t2"))
        .expect("a Prize in hand");
    assert!(matches!(
        g.players[0].hand[prize].item,
        Item::Spell { data: 4, .. }
    ));
    assert_eq!(g.players[0].powers[0].counter, 0);
    g.apply(
        0,
        Action::Play {
            hand: prize,
            pos: 0,
        },
    )
    .unwrap();
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!("the Prize Discovers")
    };
    assert!(options.iter().all(|o| db().get(o.card()).tier == 4));
}

#[test]
fn upper_hand_cast_again_while_pending_does_nothing_overconfidence_stacks() {
    // Upper Hand does not stack while it is pending, like Boon of Beetles (a Magicfin Apprentice
    // with Brann cast it twice: one enemy minion set to 1, not two); Overconfidence resolves every
    // time.
    let mut g = game(9640);
    g.with_recruit(0, |c| {
        c.cast_spell(0, cid("BG28_573"), 0, 0, 0);
        c.cast_spell(0, cid("BG28_573"), 0, 0, 0);
        c.cast_spell(0, cid("BG28_603"), 0, 0, 0);
        c.cast_spell(0, cid("BG28_603"), 0, 0, 0);
        c.cast_spell(0, cid("BG28_884"), 0, 0, 0);
        c.cast_spell(0, cid("BG28_884"), 0, 0, 0);
    });
    let stacks = |card: &str| {
        g.players[0]
            .effects
            .iter()
            .find(|x| x.card == cid(card))
            .map(|x| x.stacks)
    };
    assert_eq!(stacks("BG28_573"), Some(1));
    assert_eq!(stacks("BG28_603"), Some(2), "Beetles owed by one cast");
    let paid = g.players[0]
        .scheduled
        .iter()
        .filter(|s| s.script == cid("BG28_884"))
        .count();
    assert_eq!(paid, 2, "each Overconfidence pays");
}

#[test]
fn sewer_escapee_can_aim_at_a_tavern_murloc() {
    // The Activate went to a Tavern Bile Spitter, 9/22 -> 16/29 with Windfury; the text has no
    // "friendly".
    let mut g = game(9641);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG36_700"]);
    let tavern = minion(&mut g, 1, "BG33_318");
    let sid = tavern.eid;
    g.players[0].shop[0].item = Item::Minion(tavern);
    g.players[0].shop[0].sid = sid;
    g.apply(0, Action::Activate(0)).unwrap();
    choose_target(&mut g, 0, sid);
    let m = g.players[0].shop[0].item.as_minion().unwrap();
    assert_eq!((m.atk, m.max_hp), (1 + 7, 10 + 7));
    assert!(m.bonus_kw != Kw::NONE || m.kw.0 != 0, "a Bonus Keyword");
}

#[test]
fn surprise_portrait_also_rolls_on_selling_an_elemental() {
    // Selling a Golden Elemental gave an Elemental of Surprise.
    let surprises = |g: &Game| {
        g.players[0]
            .hand
            .iter()
            .filter(|h| h.item.card() == cid("BG26_175"))
            .count()
    };
    let (mut from_elementals, mut from_others) = (0, 0);
    for seed in 0..60 {
        for (card, n) in [
            ("BGS_116", &mut from_elementals),
            ("BG36_345", &mut from_others),
        ] {
            let mut g = game(9700 + seed);
            g.cfg.triple_copies = 99;
            buy_trinket(&mut g, 0, "BG30_MagicItem_555");
            g.players[0].hand.clear();
            set_board(&mut g, 0, &[card]);
            g.apply(0, Action::Sell(0)).unwrap();
            *n += surprises(&g);
        }
    }
    assert!(from_elementals > 0, "an Elemental's sale rolls");
    assert_eq!(from_others, 0, "only Elementals");
}

#[test]
fn a_magnet_leaves_hammer_attack_and_its_gems_behind() {
    // Greater Hammer of Twilight at k = 23 (+46/+23 on own minions), a hand Annoy-o-Module 20/16
    // (+46/+23 = 66/39) Magnetized to a Gatekeeper added 20/39; the host's Blood Gem enchantment
    // kept its own numbers.
    let mut g = game(9642);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    g.players[0].stats.cards_discarded = 22;
    set_board(&mut g, 0, &["BG26_146"]); // a Mech host
    buy_trinket(&mut g, 0, "BG36_MagicItem_403t");
    let host_before = g.players[0].board[0].clone();
    let hid = give(&mut g, 0, "BG_BOT_911");
    let magnet = g.players[0]
        .hand
        .last()
        .unwrap()
        .item
        .as_minion()
        .unwrap()
        .clone();
    assert_eq!(
        magnet.pbonus_atk, 46,
        "the Hammer's +46 is on the hand card"
    );
    {
        let m = g.players[0]
            .hand
            .iter_mut()
            .find(|h| h.hid == hid)
            .unwrap()
            .item
            .as_minion_mut()
            .unwrap();
        m.gems = 3;
        m.gem_atk = 3;
        m.gem_hp = 3;
    }
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Magnetize { hand, board: 0 }).unwrap();
    let host = &g.players[0].board[0];
    assert_eq!(host.atk - host_before.atk, magnet.atk - 46);
    assert_eq!(host.max_hp - host_before.max_hp, magnet.max_hp);
    assert_eq!(
        (host.gems, host.gem_atk, host.gem_hp),
        (host_before.gems, host_before.gem_atk, host_before.gem_hp)
    );
}

#[test]
fn spark_snapper_on_a_magnetized_mech_puts_the_satellite_into_the_magnet() {
    // Magnetizing a Volumizer with Spark Snapper on the board gave the host the Satellite's stats
    // inside the Magnetization, no Satellite card of its own; a Mech played to the board still gets
    // one.
    let mut g = game(9643);
    g.cfg.triple_copies = 99;
    g.players[0].gold = 10;
    set_board(&mut g, 0, &["BG36_851", "BG26_146"]);
    give(&mut g, 0, "BG34_170t3");
    let before = g.players[0].board[1].clone();
    let hand = g.players[0].hand.len() - 1;
    g.apply(0, Action::Magnetize { hand, board: 1 }).unwrap();
    let host = &g.players[0].board[1];
    assert!(
        !host.attached.contains(&cid("BG31_171t")),
        "{:?}",
        host.attached
    );
    assert_eq!(
        host.attached.len(),
        before.attached.len() + 1,
        "the Volumizer only"
    );
    assert!(host.atk - before.atk >= 2 && host.max_hp - before.max_hp >= 3);
    give(&mut g, 0, "BG26_146");
    play_last(&mut g, 0);
    let played = g.players[0].board.last().unwrap();
    assert!(
        played.attached.contains(&cid("BG31_171t")),
        "{:?}",
        played.attached
    );
}

#[test]
fn poison_murlocs_come_from_hand_with_their_keywords_and_go_back() {
    // Venomous / Divine Shield Murlocs in hand summoned by Expert Aviator's Rallies and Diremuck
    // Forager are the poison Murloc line; each hand card once a combat, keywords kept, the hand and
    // warband unchanged afterwards.
    let mut g = game(9644);
    g.cfg.record_log = true;
    g.cfg.triple_copies = 99;
    set_board(&mut g, 0, &["BG34_140", "BG27_556"]);
    set_board(&mut g, 1, &["BG36_345"]);
    {
        let foe = &mut g.players[1].board[0];
        foe.atk = 1;
        foe.hp = 500;
        foe.max_hp = 500;
        foe.give(Kw::TAUNT);
    }
    let venom = give(&mut g, 0, "BG34_142"); // Costume Enthusiast: Divine Shield
    give(&mut g, 0, "BG31_149");
    give(&mut g, 0, "BG32_330");
    {
        let m = g.players[0]
            .hand
            .iter_mut()
            .find(|h| h.hid == venom)
            .unwrap()
            .item
            .as_minion_mut()
            .unwrap();
        m.give(Kw::VENOMOUS);
        m.atk = 30;
    }
    let hand_before = g.players[0].hand.clone();
    let board_before = g.players[0].board.clone();
    g.simulate_combat(0, Some(1), None);
    let summoned: Vec<&Minion> = g
        .last_frames
        .iter()
        .flat_map(|f| f.boards[0].iter())
        .filter(|m| m.combat_only)
        .collect();
    let enthusiast = summoned
        .iter()
        .find(|m| m.card == cid("BG34_142"))
        .expect("the strongest hand Murloc came in");
    assert!(enthusiast.has(Kw::VENOMOUS) && enthusiast.has(Kw::DIVINE_SHIELD));
    let distinct: std::collections::BTreeSet<u32> =
        summoned.iter().map(|m| m.card.0 as u32).collect();
    assert!(
        distinct.len() >= 2,
        "Forager and the Rallies bring more than one: {distinct:?}"
    );
    assert_eq!(g.players[0].hand, hand_before);
    assert_eq!(g.players[0].board, board_before);
}

#[test]
fn golden_made_in_place_restarts_a_fired_treasure_parrot() {
    // A Treasure Parrot that has dealt its 35 starts again at 0 of 35 when made Golden in place
    // (Golden Touch, Reno); a count under way is kept; a triple takes the unfired copy closest to
    // 35.
    use taverngym::content::make_golden;
    let mut g = game(9410);
    set_board(&mut g, 0, &["BG36_763", "BG36_763"]);
    g.players[0].board[0].counter = 35;
    g.players[0].board[1].counter = 20;
    for i in 0..2 {
        make_golden(&mut g.players[0].board[i]);
        assert!(g.players[0].board[i].golden);
    }
    assert_eq!(g.players[0].board[0].counter, 0, "fired: starts again");
    assert_eq!(g.players[0].board[1].counter, 20, "under way: kept");
    // Golden Touch on a fired Parrot in the Tavern goes the same way.
    use taverngym::debug::{DebugCmd, run_debug};
    g.players[0].shop.clear();
    run_debug(
        &mut g,
        DebugCmd::AddMinion {
            p: 0,
            zone: "shop".into(),
            card: "BG36_763".into(),
            pos: None,
            golden: false,
            atk: None,
            hp: None,
        },
    )
    .unwrap();
    if let Some(Item::Minion(m)) = g.players[0].shop.last_mut().map(|s| &mut s.item) {
        m.counter = 35;
    }
    give(&mut g, 0, "BG28_830");
    play_last(&mut g, 0);
    let m = g.players[0].shop[0].item.as_minion().unwrap();
    assert!(m.golden);
    assert_eq!(m.counter, 0);
}

#[test]
fn golden_made_malchezaar_has_four_health_refreshes_again() {
    // A plain Malchezaar with both Health Refreshes used, made Golden, has four more this turn.
    let mut g = game(9411);
    set_board(&mut g, 0, &["BG26_524"]);
    g.players[0].gold = 10;
    let paid_in_health = |g: &mut Game| {
        let life = |g: &Game| g.players[0].health + g.players[0].armor;
        let (gold, before) = (g.players[0].gold, life(g));
        g.apply(0, Action::Refresh).unwrap();
        g.players[0].gold == gold && life(g) < before
    };
    assert!(paid_in_health(&mut g) && paid_in_health(&mut g));
    assert!(!paid_in_health(&mut g), "the third costs Gold");
    taverngym::content::make_golden(&mut g.players[0].board[0]);
    for k in 0..4 {
        assert!(paid_in_health(&mut g), "golden Refresh {k}");
    }
    assert!(!paid_in_health(&mut g));
}

#[test]
fn drestagath_is_never_offered_ancient_wishbone() {
    // The client does not offer Ancient Wishbone with Incubate.
    let mut g = game(9412);
    let wishbone = cid("BG30_MagicItem_804");
    g.set_hero_power(0, 0, cid("BG36_HERO_000p"));
    assert!(!g.trinket_eligible(0, wishbone));
    g.set_hero_power(0, 0, cid("BG20_HERO_301p"));
    assert!(g.trinket_eligible(0, wishbone), "another active Hero Power");
}

#[test]
fn a_host_played_from_hand_fires_its_last_volumizer_once() {
    // A copy of a Volumizer host played from hand gives the bonus of the Volumizer Magnetized last,
    // once, at that Volumizer's size; Volumizers Magnetized before it don't fire.
    let (red, blue, green) = (cid("BG34_170t"), cid("BG34_170t2"), cid("BG34_170t3"));
    let play_host = |seed: u64, host: CardId, parts: Vec<CardId>| {
        let mut g = game(seed);
        g.cfg.triple_copies = 99;
        set_board(&mut g, 0, &[]);
        g.players[0].stats.volumizer_atk = 0;
        g.players[0].stats.volumizer_hp = 6;
        give(&mut g, 0, db().get(host).id.as_str());
        if let Item::Minion(m) = &mut g.players[0].hand.last_mut().unwrap().item {
            m.attached = parts;
        }
        play_last(&mut g, 0);
        let st = &g.players[0].stats;
        (st.volumizer_atk, st.volumizer_hp)
    };
    let robot = cid("BG36_366");
    assert_eq!(
        play_host(9420, robot, vec![red, blue, green]),
        (1, 7),
        "Green last"
    );
    assert_eq!(
        play_host(9421, cid("BG36_367"), vec![blue, cid("BG31_171t"), green]),
        (1, 7)
    );
    let golden = db().golden_of(robot);
    assert_eq!(
        play_host(9422, golden, vec![blue]),
        (0, 9),
        "Blue's +3, not doubled"
    );
    assert_eq!(
        play_host(9423, golden, vec![green, blue, red, green, red, blue]),
        (0, 9),
        "Blue last"
    );
    assert_eq!(
        play_host(9424, robot, vec![]),
        (0, 6),
        "no Volumizer, no bonus"
    );
}
