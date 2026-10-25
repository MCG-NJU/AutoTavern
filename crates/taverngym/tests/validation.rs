//! Validation cases T01-T30 of the rules reference the card data was compiled with (written for
//! 36.6.1; the numbers here follow the 36.6.3 target).

use taverngym::actions::Action;
use taverngym::cards::{CardKind, cid, db};
use taverngym::state::{Minion, Pending};
use taverngym::types::{PlayerId, Races};
use taverngym::{Game, RulesConfig};

/// A game past hero selection with Aberrations in the lobby, every seat pinned to a passive
/// hero.
fn game(seed: u64) -> Game {
    let cfg = RulesConfig {
        aberration_forced: true,
        ..RulesConfig::default()
    };
    let mut g = Game::new(seed, cfg);
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

fn minion(g: &mut Game, p: PlayerId, id: &str) -> Minion {
    g.with_recruit(p, |c| c.new_minion(0, cid(id)))
}

fn end_turn_all(g: &mut Game) {
    for p in 0..8 {
        while g.players[p].pending.last().is_some() {
            g.apply(p, Action::Choose(0)).unwrap();
        }
        if g.needs_action(p) {
            g.apply(p, Action::EndTurn).unwrap();
        }
    }
}

/// Every pool minion resolves to a golden form and back; golden texts are their own.
#[test]
fn t03_every_pool_minion_has_a_golden_form() {
    let d = db();
    let mut n = 0;
    for c in d.iter().filter(|c| c.pools.minion || c.pools.tier7) {
        let g = d.golden_of(c.cid);
        assert_ne!(g, c.cid, "{}", c.id);
        assert_eq!(d.normal_of(g), c.cid, "{}", c.id);
        let gd = d.get(g);
        assert_eq!(gd.tier, c.tier, "{}", c.id);
        assert_eq!(gd.text.is_empty(), c.text.is_empty(), "{}", c.id);
        n += 1;
    }
    assert!(n > 240);
}

/// T05 / T06: damage caps 5, 10, 10, 15 on turns 3, 4, 7, 8 with more than 4 players
/// alive; no cap once 4 are left.
#[test]
fn t05_t06_damage_caps_and_top_four() {
    let lose = |turn: u32, alive: Option<usize>| -> i32 {
        let mut g = game(700 + turn as u64);
        g.turn = turn;
        if let Some(a) = alive {
            g.turn_alive = a;
        }
        let mut big: Vec<Minion> = (0..7).map(|_| minion(&mut g, 0, "BG36_345")).collect();
        for m in big.iter_mut() {
            m.set_stats(50, 50);
            m.tier = 6;
        }
        g.players[0].board = big;
        g.players[0].tier = 6;
        g.players[1].health = 100;
        let out = g.simulate_combat(0, Some(1), None);
        assert_eq!(out.winner, 0);
        let before = g.players[1].health + g.players[1].armor;
        g.apply_combat_result(0, Some(1), out);
        before - (g.players[1].health + g.players[1].armor)
    };
    assert_eq!(lose(3, None), 5);
    assert_eq!(lose(4, None), 10);
    assert_eq!(lose(7, None), 10);
    assert_eq!(lose(8, None), 15);
    assert_eq!(
        lose(9, Some(4)),
        48,
        "uncapped: hero tier 6 + 7 minions of tier 6"
    );
}

/// The Dark Gift button unlocks on turn 3, costs 3 Gold, once a turn, 3 per game.
#[test]
fn t10_dark_gift_button_limits() {
    let mut g = game(710);
    g.players[0].gold = 10;
    assert!(!g.can_dark_gift(0), "turn 1");
    end_turn_all(&mut g);
    g.players[0].gold = 10;
    assert!(!g.can_dark_gift(0), "turn 2");
    for turn in 3..=5 {
        end_turn_all(&mut g);
        assert_eq!(g.turn, turn);
        g.players[0].gold = 10;
        assert!(g.can_dark_gift(0));
        g.apply(0, Action::DarkGift).unwrap();
        assert_eq!(g.players[0].gold, 7);
        g.apply(0, Action::Choose(0)).unwrap();
        assert!(!g.can_dark_gift(0), "once per turn");
        g.players[0].hand.clear();
    }
    end_turn_all(&mut g);
    while g.players[0].pending.last().is_some() {
        g.players[0].gold = 10;
        g.apply(0, Action::Choose(0)).unwrap();
    }
    g.players[0].gold = 10;
    assert!(!g.can_dark_gift(0), "three per game");
}

/// Naga-only minions are rotated out; a minion that is also another type may stay.
#[test]
fn t19_naga_only_minions_are_out_of_the_pool() {
    for c in db().iter().filter(|c| c.pools.minion || c.pools.tier7) {
        assert_ne!(c.races, Races::NAGA, "{} {}", c.id, c.name);
    }
}

/// A Y'Shaarj lobby bans four heroes and the Tier 5 / 6 Dark Paradox forms.
#[test]
fn t20_yshaarj_lobby_bans() {
    let cfg = RulesConfig {
        aberration_forced: true,
        ..RulesConfig::default()
    };
    let banned = [
        "TB_BaconShop_HERO_33",
        "TB_BaconShop_HERO_22",
        "BG25_HERO_103",
        "TB_BaconShop_HERO_93",
    ];
    let (mut yshaarj, mut cthun) = (0, 0);
    for seed in 0..60 {
        let g = Game::new(seed, cfg.clone());
        let deity = g.players[0].deity.as_ref().unwrap().card;
        let offered: Vec<String> = g
            .players
            .iter()
            .flat_map(|p| match p.pending.last() {
                Some(Pending::HeroSelect { options }) => options.clone(),
                _ => vec![],
            })
            .map(|h| db().get(h).id.clone())
            .collect();
        // The forms are banned by Tavern Tier, not by id suffix: Tier 5 = BG36_360t4 (3-hit
        // Divine Shield), Tier 6 = BG36_360t9 (Deathrattle Golem).
        let paradox = db().get(g.paradox.unwrap());
        assert!(g.universe.contains(&paradox.cid), "seed {seed}");
        if deity == cid("BGFYM_011") {
            yshaarj += 1;
            assert!(
                !offered.iter().any(|h| banned.contains(&h.as_str())),
                "seed {seed}"
            );
            assert!(paradox.tier < 5, "seed {seed}: {}", paradox.id);
            assert!(!["BG36_360t4", "BG36_360t9"].contains(&paradox.id.as_str()));
        } else {
            cthun += 1;
        }
        // The Tier 5 form is also banned in Undead lobbies, whatever the Deity.
        if g.lobby_races.intersects(Races::UNDEAD) {
            assert_ne!(paradox.id, "BG36_360t4", "seed {seed}");
        }
    }
    assert!(yshaarj > 5 && cthun > 5);
    // Without a ban every form can be the lobby's.
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..200 {
        let g = Game::new(seed, RulesConfig::default());
        seen.insert(db().get(g.paradox.unwrap()).id.clone());
    }
    for id in taverngym::flow::PARADOX_VARIANTS {
        assert!(seen.contains(id), "{id} never chosen");
    }
}

/// Temporarily banned objects are in no pool.
#[test]
fn t21_temporary_bans_are_in_no_pool() {
    for id in [
        "BG34_Giant_585",
        "BG34_Giant_585_G",
        "BGS_121",
        "BG30_MagicItem_973",
        "BG36_MagicItem_412",
        "BG36_MagicItem_412t2",
    ] {
        let p = db().get(cid(id)).pools;
        assert!(!p.minion && !p.tier7 && !p.trinket && !p.timewarp, "{id}");
    }
    // Trinket offers never contain them either.
    let mut g = game(721);
    g.turn = 9;
    for _ in 0..50 {
        for t in g.trinket_offer(0, CardKind::TrinketGreater) {
            assert!(!db().get(t).id.starts_with("BG36_MagicItem_412"));
        }
    }
}

/// Drustfallen Butcher's Avenge is 4 (the 3 seen live before 36.6.2 was a bug).
#[test]
fn t22_drustfallen_butcher_avenge_four() {
    let mut g = game(722);
    let ids = ["BG32_324", "BG36_345", "BG36_345", "BG36_345", "BG36_345"];
    let b: Vec<Minion> = ids.iter().map(|id| minion(&mut g, 0, id)).collect();
    g.players[0].board = b;
    let boards = [g.players[0].board.clone(), g.players[1].board.clone()];
    let mut c = taverngym::engine::Ctx::new(&mut g, boards, [Some(0), Some(1)], true);
    for k in 1..=4 {
        let e = c.boards[0][1].eid;
        c.destroy(e);
        c.resolve_deaths();
        let got = c.player_ref(0).unwrap().hand.len();
        assert_eq!(got, usize::from(k == 4), "after {k} deaths");
    }
    drop(c);
    assert_eq!(g.players[0].hand[0].item.card(), cid("BG28_604"));
}

/// A combat where nobody can deal damage ends as a tie through the simulator's own cutoff (flagged
/// `truncated`: not an official result).
#[test]
fn t30_endless_combat_is_cut_off_as_a_tie() {
    let mut g = game(730);
    for p in 0..2 {
        let mut m = minion(&mut g, p, "BG36_345");
        m.set_stats(0, 10);
        g.players[p].board = vec![m];
    }
    let out = g.simulate_combat(0, Some(1), None);
    assert_eq!(out.winner, 2);
    let (a, b) = (g.players[0].health, g.players[1].health);
    g.apply_combat_result(0, Some(1), out);
    assert_eq!((g.players[0].health, g.players[1].health), (a, b));
}

fn trinket_offer_kind(g: &Game, p: PlayerId) -> Option<CardKind> {
    g.players[p].pending.iter().find_map(|x| match x {
        Pending::Trinket { options, .. } => options.first().map(|t| db().get(*t).kind),
        _ => None,
    })
}

/// Advance to the start of `turn`, answering every offer on the way (player 0 keeps enough
/// Gold to buy what is offered).
fn advance_to(g: &mut Game, turn: u32) {
    while g.turn < turn {
        g.players[0].gold = 20;
        end_turn_all(g);
    }
}

/// T15 / T16: Lesser trinkets on turn 6 and Greater ones on turn 9; Marin gets an extra Lesser
/// offer on turn 5 and Buttons an extra Greater one on turn 8, on top of the normal ones (their
/// powers' timing).
#[test]
fn t15_t16_trinket_turns_and_early_trinket_heroes() {
    let offers = |power: Option<&str>| -> Vec<(u32, CardKind)> {
        let mut g = game(715);
        if let Some(hp) = power {
            g.set_hero_power(0, 0, cid(hp));
        }
        let mut seen = vec![];
        for turn in 2..=9 {
            advance_to(&mut g, turn);
            if let Some(k) = trinket_offer_kind(&g, 0) {
                seen.push((turn, k));
            }
        }
        seen
    };
    use CardKind::{TrinketGreater as G, TrinketLesser as L};
    assert_eq!(offers(None), vec![(6, L), (9, G)]);
    assert_eq!(offers(Some("BG30_HERO_304p")), vec![(5, L), (6, L), (9, G)]);
    assert_eq!(offers(Some("BG32_HERO_002p")), vec![(6, L), (8, G), (9, G)]);
}

/// An effect that triggers a Deathrattle does not kill the minion.
#[test]
fn t18_triggered_deathrattle_is_not_a_death() {
    let mut g = game(718);
    let flapper = minion(&mut g, 0, "BG34_682"); // Deathrattle: get a Blood Gem Barrage
    g.players[0].board = vec![flapper];
    let deaths = g.players[0].stats.clone();
    g.with_recruit(0, |c| {
        let e = c.boards[0][0].eid;
        c.trigger_deathrattles(0, e);
    });
    assert_eq!(g.players[0].board.len(), 1, "still alive");
    assert_eq!(g.players[0].hand.len(), 1, "its Deathrattle ran");
    let st = &g.players[0].stats;
    assert_eq!(st.deathrattles_triggered, deaths.deathrattles_triggered + 1);
}

// ---------------------------------------------------------------------------- Timewarp

/// (options with their Chronum cost, Chronum, major?) of an open Timewarp visit.
type Visit = (Vec<(taverngym::state::Item, i32)>, i32, bool);

fn timewarp(g: &Game, p: PlayerId) -> Option<Visit> {
    match g.players[p].pending.last() {
        Some(Pending::Timewarp {
            options,
            chronum,
            major,
        }) => Some((options.clone(), *chronum, *major)),
        _ => None,
    }
}

/// The offer: 5 cards arranged by cost, 3 minions + 2 non-minions, three costing 1 and two
/// costing 2 or more; a second Hero Power in the Minor Timewarp; typed cards only for types
/// the player is in, at most one per type other than the most common one.
#[test]
fn timewarp_offer_follows_the_official_rules() {
    use taverngym::content::timewarp;
    for seed in 0..40u64 {
        let mut g = game(800 + seed);
        g.cfg.content = taverngym::config::ContentMode::AllVanilla;
        // Three minions of one lobby type in the warband (when the lobby has such a card).
        let lobby = g.lobby_races;
        let typed: Vec<_> = g
            .universe
            .iter()
            .filter(|c| db().get(**c).races != Races::ALL && db().get(**c).races.intersects(lobby))
            .take(1)
            .map(|c| [*c; 3])
            .next()
            .unwrap()
            .to_vec();
        let ids: Vec<String> = typed.iter().map(|c| db().get(*c).id.clone()).collect();
        let b: Vec<Minion> = ids.iter().map(|id| minion(&mut g, 0, id)).collect();
        g.players[0].board = b;
        for major in [false, true] {
            let offer = g.with_recruit(0, |c| timewarp::offer(c, 0, major));
            assert_eq!(offer.len(), 5, "seed {seed} major {major}");
            let costs: Vec<i32> = offer.iter().map(|o| o.1).collect();
            assert!(costs.windows(2).all(|w| w[0] <= w[1]), "by cost: {costs:?}");
            assert_eq!(costs.iter().filter(|c| **c == 1).count(), 3, "{costs:?}");
            let minions = offer.iter().filter(|o| o.0.as_minion().is_some()).count();
            assert_eq!(minions, 3);
            let powers = offer
                .iter()
                .filter(|o| db().get(o.0.card()).id.starts_with("BG34_HeroPowerSpell_"))
                .count();
            assert_eq!(powers, usize::from(!major), "seed {seed} major {major}");
            // Two of the three minions are tied to the most common type (the warband is
            // three minions of one type), when the pool has that many.
            let most = db().get(typed[0]).races;
            let tied = offer
                .iter()
                .filter(|o| {
                    o.0.as_minion().is_some() && db().get(o.0.card()).tw_types.intersects(most)
                })
                .count();
            let tier0 = if major { 5 } else { 3 };
            let available = db()
                .iter()
                .filter(|d| {
                    d.pools.timewarp
                        && d.normal.is_none()
                        && d.tier == tier0
                        && d.kind == CardKind::Minion
                        && d.tw_types.intersects(most)
                })
                .count();
            if typed.len() == 3 && available >= 2 {
                assert!(
                    tied >= 2,
                    "seed {seed} major {major}: {tied} of the most common type"
                );
            }
            let tier = if major { 5 } else { 3 };
            let mut seen = Races::NONE;
            for (it, cost) in &offer {
                let d = db().get(it.card());
                assert!(d.pools.timewarp && d.tier == tier, "{}", d.name);
                assert_eq!(*cost, d.cost);
                assert!(
                    d.tw_types.is_none() || d.tw_types.intersects(lobby),
                    "{}",
                    d.name
                );
                seen = seen.union(d.tw_types);
            }
            let _ = seen;
        }
    }
}

#[test]
fn timewarp_visit_spends_chronum_and_keeps_the_rest() {
    let mut g = game(850);
    g.cfg.content = taverngym::config::ContentMode::AllVanilla;
    g.players[0].hand.clear();
    g.with_recruit(0, |c| taverngym::content::timewarp::open(c, 0, true));
    let (options, chronum, major) = timewarp(&g, 0).expect("a visit");
    assert!(major && chronum == 2 && options.len() == 5);
    // Only Choose is legal: the five cards that are affordable, and "leave".
    let legal = g.legal_actions(0);
    assert!(
        legal
            .iter()
            .all(|a| matches!(Action::decode(*a), Some(Action::Choose(_))))
    );
    assert!(legal.contains(&Action::Choose(5).encode()), "leave");
    // Buy a 1-Chronum card: it is in hand (no Gold spent), the visit goes on with 1 Chronum.
    let gold = g.players[0].gold;
    let first = options[0].0.card();
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].gold, gold);
    assert!(g.players[0].hand.iter().any(|h| h.item.card() == first));
    let (rest, chronum, _) = timewarp(&g, 0).expect("still visiting");
    assert_eq!((rest.len(), chronum), (4, 1));
    // The 2-Chronum cards are no longer affordable.
    let dear = rest.iter().position(|o| o.1 >= 2).unwrap();
    assert!(g.apply(0, Action::Choose(dear)).is_err());
    // Leave with 1 Chronum: it is lost; the next visit has 2 again.
    g.apply(0, Action::Choose(rest.len())).unwrap();
    assert!(timewarp(&g, 0).is_none());
    assert_eq!(g.players[0].chronum_saved, 0);
    g.with_recruit(0, |c| taverngym::content::timewarp::open(c, 0, true));
    assert_eq!(timewarp(&g, 0).unwrap().1, 2);
    // Two 1-Chronum cards: the visit ends by itself when nothing is affordable.
    for left in [1, 0] {
        let (opts, _, _) = timewarp(&g, 0).unwrap();
        let cheap = opts.iter().position(|o| o.1 == 1).unwrap();
        g.apply(0, Action::Choose(cheap)).unwrap();
        assert_eq!(timewarp(&g, 0).map_or(0, |v| v.1), left);
    }
    assert!(timewarp(&g, 0).is_none(), "0 Chronum: the visit is over");
    // With a full hand the cards wait for space instead of being lost.
    let mut g = game(851);
    g.cfg.content = taverngym::config::ContentMode::AllVanilla;
    g.cfg.triple_copies = 99;
    g.players[0].hand.clear();
    for _ in 0..g.cfg.hand_size {
        g.with_recruit(0, |c| {
            c.give_card(0, cid("BG36_345"));
        });
    }
    g.with_recruit(0, |c| taverngym::content::timewarp::open(c, 0, true));
    let (opts, _, _) = timewarp(&g, 0).unwrap();
    let i = opts
        .iter()
        .position(|o| o.0.as_minion().is_some() && o.1 == 1)
        .unwrap();
    g.apply(0, Action::Choose(i)).unwrap();
    assert_eq!(g.players[0].hand.len(), g.cfg.hand_size);
    assert_eq!(g.players[0].hand_queue.len(), 1);
}

#[test]
fn timewarp_system_is_off_but_morchie_and_murozond_visit() {
    let visits = |power: Option<&str>, enabled: bool| -> Vec<(u32, bool)> {
        let mut g = game(860);
        g.cfg.content = taverngym::config::ContentMode::AllVanilla;
        g.cfg.timewarp_enabled = enabled;
        if let Some(hp) = power {
            g.set_hero_power(0, 0, cid(hp));
        }
        let mut seen = vec![];
        for turn in 2..=9 {
            while g.turn < turn {
                g.players[0].gold = 20;
                // Answer every offer; leave a Timewarp visit at once (after recording it).
                for p in 0..8 {
                    while let Some(top) = g.players[p].pending.last() {
                        let i = match top {
                            Pending::Timewarp { options, major, .. } => {
                                if p == 0 {
                                    seen.push((g.turn, *major));
                                }
                                options.len()
                            }
                            _ => 0,
                        };
                        g.apply(p, Action::Choose(i)).unwrap();
                    }
                    if g.needs_action(p) {
                        g.apply(p, Action::EndTurn).unwrap();
                    }
                }
            }
        }
        if let Some((_, _, major)) = timewarp(&g, 0) {
            seen.push((g.turn, major));
        }
        seen
    };
    assert_eq!(visits(None, false), vec![], "off in 36.6.3");
    assert_eq!(visits(None, true), vec![(6, false), (9, true)]);
    assert_eq!(
        visits(Some("BG34_HERO_004p"), false),
        vec![(5, false)],
        "Morchie"
    );
    assert_eq!(
        visits(Some("BG34_HERO_000p"), false),
        vec![(8, true)],
        "Murozond"
    );
    assert_eq!(
        visits(Some("BG34_HERO_004p"), true),
        vec![(5, false), (6, false), (9, true)],
        "the hero's visit is an extra one"
    );
}

/// Timewarped Investment: only in the Minor Timewarp of the global system; it carries one
/// Chronum to the next visit.
#[test]
fn timewarped_investment_saves_a_chronum_for_the_major_timewarp() {
    use taverngym::content::timewarp;
    let coin = cid("BG34_Treasure_300");
    let offered = |enabled: bool, major: bool| -> bool {
        // (Rare with an empty warband: few cards are eligible, so many seeds.)
        (0..300).any(|seed| {
            let mut g = game(880 + seed);
            g.cfg.timewarp_enabled = enabled;
            g.with_recruit(0, |c| timewarp::offer(c, 0, major))
                .iter()
                .any(|o| o.0.card() == coin)
        })
    };
    assert!(offered(true, false));
    assert!(!offered(true, true), "not in the Major Timewarp");
    assert!(
        !offered(false, false),
        "not on a hero's visit while the system is off"
    );
    // Bought: cast at once, +1 Chronum at the next visit.
    let mut g = game(881);
    g.players[0].pending.push(Pending::Timewarp {
        options: vec![(taverngym::state::Item::spell(coin), 1)],
        chronum: 2,
        major: false,
    });
    g.apply(0, Action::Choose(0)).unwrap();
    assert_eq!(g.players[0].chronum_saved, 1);
    assert!(g.players[0].hand.iter().all(|h| h.item.card() != coin));
    g.with_recruit(0, |c| timewarp::open(c, 0, true));
    assert_eq!(timewarp(&g, 0).unwrap().1, 3);
    assert_eq!(g.players[0].chronum_saved, 0);
}
