//! Combat must not change the persistent warband, except effects whose text says so ("permanently",
//! "this game... wherever they are", which go through player-level bonuses).

use taverngym::actions::Action;
use taverngym::cards::{CardKind, db};
use taverngym::rng::Rng;
use taverngym::state::Minion;
use taverngym::{Game, RulesConfig};

/// Stats net of player-level "this game" bonuses, plus keywords.
fn core(b: &[Minion]) -> Vec<(u32, i32, i32, u32)> {
    b.iter()
        .map(|m| (m.eid, m.atk - m.pbonus_atk, m.max_hp - m.pbonus_hp, m.kw.0))
        .collect()
}

#[test]
fn combat_never_leaks_into_persistent_boards() {
    let d = db();
    let base = {
        let mut g = Game::new(99, RulesConfig::default());
        for p in 0..8 {
            g.apply(p, Action::Choose(0)).unwrap();
        }
        g
    };
    let minions: Vec<_> = d
        .iter()
        .filter(|c| c.pools.minion && taverngym::script::implemented(c.cid))
        // Cards whose combat effects are "permanent" by text legitimately change the warband.
        .filter(|c| !c.text.to_lowercase().contains("permanent"))
        .map(|c| c.cid)
        .collect();
    let heroes: Vec<_> = d
        .iter()
        .filter(|c| c.kind == CardKind::Hero && c.pools.hero)
        .filter(|c| {
            c.hero_power
                .is_some_and(|hp| taverngym::script::script(hp).is_some())
        })
        // Powers that change the warband "permanently" by text (Deathwing, Rokara...).
        .filter(|c| {
            c.hero_power
                .is_some_and(|hp| !d.get(hp).text.to_lowercase().contains("permanent"))
        })
        .map(|c| c.cid)
        .collect();
    let trinkets: Vec<_> = d
        .iter()
        .filter(|c| c.pools.trinket && taverngym::script::script(c.cid).is_some())
        .filter(|c| !c.text.to_lowercase().contains("permanent"))
        .map(|c| c.cid)
        .collect();
    let mut rng = Rng::new(7);
    let mut checked = 0;
    for (i, &card) in minions.iter().enumerate() {
        for rep in 0..6 {
            let mut g = base.clone();
            for p in [0usize, 1] {
                let h = heroes[rng.below(heroes.len())];
                g.players[p].hero = Some(h);
                if let Some(hp) = d.get(h).hero_power {
                    g.players[p].set_hero_power(0, hp);
                }
                g.players[p].trinkets = vec![trinkets[rng.below(trinkets.len())]];
                let mut board = vec![];
                let n = 3 + rng.below(5);
                for k in 0..n {
                    let c = if p == 0 && k == 0 {
                        card
                    } else {
                        minions[rng.below(minions.len())]
                    };
                    let c = if rng.below(4) == 0 { d.golden_of(c) } else { c };
                    board.push(g.with_recruit(p, |ctx| ctx.new_minion(0, c)));
                }
                g.players[p].board = board;
            }
            let before = [core(&g.players[0].board), core(&g.players[1].board)];
            g.simulate_combat(0, Some(1), None);
            let after = [core(&g.players[0].board), core(&g.players[1].board)];
            assert_eq!(
                before,
                after,
                "combat leaked into a warband (minion #{i} {} rep {rep}); p0 {:?} p1 {:?}",
                d.get(card).id,
                g.players[0]
                    .board
                    .iter()
                    .map(|m| m.name())
                    .collect::<Vec<_>>(),
                g.players[1]
                    .board
                    .iter()
                    .map(|m| m.name())
                    .collect::<Vec<_>>()
            );
            checked += 1;
        }
    }
    assert!(checked > 300);
}
