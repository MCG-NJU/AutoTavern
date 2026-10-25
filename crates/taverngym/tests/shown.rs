//! `Script::shown` (the numbers a card's text shows right now, display only).

use taverngym::actions::Action;
use taverngym::cards::{CardKind, cid, db};
use taverngym::debug::{DebugCmd, live_text, run_debug, text_numbers};
use taverngym::script::{Shown, script};
use taverngym::state::Pending;
use taverngym::{Game, RulesConfig};

fn game(seed: u64) -> Game {
    let mut g = Game::new(seed, RulesConfig::default());
    for p in 0..g.players.len() {
        if matches!(
            g.players[p].pending.last(),
            Some(Pending::HeroSelect { .. })
        ) {
            g.apply(p, Action::Choose(0)).unwrap();
        }
        g.players[p].pending.clear();
    }
    g
}

/// Printed numbers the client does not count by: Gunpowder Courier's "(6 Gold left!)" next
/// to its "spend 5 Gold" - the client counts 5.
const STALE_PRINTED: [&str; 2] = ["BG26_810", "BG26_810_G"];

/// A card in its initial state shows exactly its printed numbers: this catches a hook that
/// returns the wrong count, the wrong order, or wrong base values (golden cards included).
#[test]
fn fresh_cards_show_their_printed_numbers() {
    let mut bad = vec![];
    let mut hooks = 0;
    for d in db().iter() {
        let Some(f) = script(d.cid).and_then(|s| s.shown) else {
            continue;
        };
        hooks += 1;
        if STALE_PRINTED.contains(&d.id.as_str()) {
            continue;
        }
        let mut g = game(1);
        let id = d.id.clone();
        // Put the card where its state lives. A failure here (no room, wrong lobby) just
        // leaves the hook to its defaults.
        let _ = match d.kind {
            CardKind::HeroPower => {
                g.set_hero_power(0, 0, d.cid);
                Ok(serde_json::Value::Null)
            }
            CardKind::TrinketLesser | CardKind::TrinketGreater => {
                run_debug(&mut g, DebugCmd::GiveTrinket { p: 0, card: id })
            }
            CardKind::QuestReward => run_debug(&mut g, DebugCmd::GiveReward { p: 0, card: id }),
            _ => Ok(serde_json::Value::Null),
        };
        g.players[0].pending.clear();
        let shown = g.with_recruit(0, |c| {
            let m = (d.kind == CardKind::Minion).then(|| c.new_minion(0, d.cid));
            let sh = Shown {
                side: 0,
                card: d.cid,
                golden: d.normal.is_some(),
                minion: m.as_ref(),
                data: 0,
            };
            f(c, &sh)
        });
        let printed = text_numbers(&d.text);
        if shown != printed {
            bad.push(format!(
                "{} {}: shows {shown:?}, text has {printed:?}: {}",
                d.id, d.name, d.text
            ));
        }
    }
    assert!(hooks > 0);
    assert!(
        bad.is_empty(),
        "{} of {hooks} hooks disagree with the card text:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn edwin_shows_his_improved_buff_and_the_cards_left() {
    let mut g = game(2);
    let edwin = cid("TB_BaconShop_HP_001");
    g.set_hero_power(0, 0, edwin);
    let text = |g: &mut Game| {
        g.with_recruit(0, |c| {
            let sh = Shown {
                side: 0,
                card: edwin,
                golden: false,
                minion: None,
                data: 0,
            };
            live_text(c, &sh).map(|t| t.0)
        })
    };
    assert_eq!(text(&mut g), None, "as printed while nothing changed");
    g.players[0].powers[0].counter = 5; // five cards bought
    assert_eq!(
        text(&mut g).as_deref(),
        Some("Give a minion +4/+4. Improves after you buy 4 cards. (3 left!)")
    );
    // The view carries the live text in both languages.
    let v = taverngym::debug::view(&g, 0);
    assert_eq!(
        v["me"]["hero_power"]["text"],
        "Give a minion +4/+4. Improves after you buy 4 cards. (3 left!)"
    );
    assert!(
        v["me"]["hero_power"]["text_zh"]
            .as_str()
            .unwrap()
            .contains("+4/+4")
    );
    assert_eq!(v["me"]["pending"], serde_json::Value::Null);
}

#[test]
fn banana_shows_the_tavern_spell_bonus() {
    let mut g = game(3);
    g.players[0].stats.spell_atk = 2;
    g.players[0].stats.spell_hp = 1;
    let banana = cid("BG28_897");
    let t = g.with_recruit(0, |c| {
        let sh = Shown {
            side: 0,
            card: banana,
            golden: false,
            minion: None,
            data: 0,
        };
        live_text(c, &sh)
    });
    assert_eq!(t.unwrap().0, "Give a minion +4/+3.");
}

#[test]
fn saurfang_and_the_cheese_wheel_show_their_improved_bonus() {
    // Saurfang: "+1/+1... Improves after you buy 3 minions. (3 left!)". Buying three minions
    // improves it once.
    let mut g = game(4);
    let saurfang = cid("BG20_HERO_102p");
    g.set_hero_power(0, 0, saurfang);
    for _ in 0..4 {
        g.players[0].gold = 10;
        let i = g.players[0]
            .shop
            .iter()
            .position(|s| s.item.as_minion().is_some())
            .unwrap();
        g.apply(0, Action::Buy(i)).unwrap();
        g.players[0].hand.clear();
        if g.players[0]
            .shop
            .iter()
            .all(|s| s.item.as_minion().is_none())
        {
            g.apply(0, Action::Refresh).unwrap();
        }
    }
    let v = taverngym::debug::view(&g, 0);
    assert_eq!(
        v["me"]["hero_power"]["text"],
        "Minions in the Tavern have +2/+2. Improves after you buy 3 minions. (2 left!)"
    );
    assert_eq!(
        g.players[0].stats.tavern_atk, 2,
        "the text matches the effect"
    );

    // Dalaran Cheese Wheel: "+2/+2. Improves every 4 times you Refresh. (4 left!)".
    let mut g = game(5);
    run_debug(
        &mut g,
        DebugCmd::GiveTrinket {
            p: 0,
            card: "BG30_MagicItem_879t".into(),
        },
    )
    .unwrap();
    g.players[0].pending.clear();
    for _ in 0..5 {
        g.players[0].gold = 10;
        g.apply(0, Action::Refresh).unwrap();
    }
    let v = taverngym::debug::view(&g, 0);
    assert_eq!(
        v["me"]["trinkets"][0]["text"],
        "Minions in the Tavern have +4/+4. Improves every 4 times you Refresh. (3 left!)"
    );
    assert_eq!(
        g.players[0].stats.tavern_atk, 4,
        "the text matches the effect"
    );
}

#[test]
fn dark_gifts_on_a_minion_show_their_counters() {
    let mut g = game(6);
    g.players[0].board.clear();
    run_debug(
        &mut g,
        DebugCmd::AddMinion {
            p: 0,
            zone: "board".into(),
            card: "BG36_345".into(),
            pos: None,
            golden: false,
            atk: None,
            hp: None,
        },
    )
    .unwrap();
    for gift in ["BG36_MidGameEffect_000t28", "BG36_MidGameEffect_000t51"] {
        run_debug(
            &mut g,
            DebugCmd::EditMinion {
                p: 0,
                zone: "board".into(),
                index: 0,
                atk: None,
                hp: None,
                keywords: None,
                golden: None,
                attach: Some(gift.into()),
            },
        )
        .unwrap();
    }
    g.players[0].stats.battlecries_triggered = 4;
    let v = taverngym::debug::view(&g, 0);
    let att = &v["me"]["board"][0]["attached"];
    assert_eq!(
        att[0]["text"],
        "Has +3/+3 for each Battlecry you've triggered this game. (4)"
    );
    assert!(att[0]["text_zh"].as_str().unwrap().contains("已触发4次"));
    let growth = att[1]["text"].as_str().unwrap();
    assert!(
        growth.contains("gain +") && !growth.contains("+0/+0"),
        "{growth}"
    );
}

#[test]
fn counts_of_one_read_in_the_singular() {
    let mut g = game(7);
    let marin = cid("BG30_HERO_304p"); // "On Turn 5, choose a Lesser Trinket to buy. (4 turns left!)"
    g.set_hero_power(0, 0, marin);
    g.turn = 4;
    let v = taverngym::debug::view(&g, 0);
    assert_eq!(
        v["me"]["hero_power"]["text"],
        "On Turn 5, choose a Lesser Trinket to buy. (1 turn left!)"
    );
}
