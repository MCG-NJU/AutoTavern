//! Tests for the Timewarp cards of `content/tw_undead.rs`.

mod common;
#[allow(unused_imports)]
use common::*;

const SKELETON: &str = "BG_ICC_026t";
const MICROBOT: &str = "BG_BOT_312t";
const COIN: &str = "BG28_810";
const GEIST: &str = "BG34_Giant_034";
const DEATHSWARMER: &str = "BG34_Giant_081";
const RECYCLER: &str = "BG34_Giant_082";
const PILLAGER: &str = "BG34_Giant_204";
const RADIO_STAR: &str = "BG34_Giant_330";
const WARGHOUL: &str = "BG34_Giant_331";
const EMBALMER: &str = "BG34_Giant_332";
const HAG: &str = "BG34_Giant_342";
const FESTERGUT: &str = "BG34_Giant_590";
const GHOUL: &str = "BG34_Giant_609";
const CARETAKER: &str = "BG34_Giant_618";
const DEADSTOMPER: &str = "BG34_Giant_654";
const ANUBARAK: &str = "BG34_PreMadeChamp_083";

/// Kill player 0's minion at `idx` in a combat against player 1's warband.
fn kill(g: &mut Game, idx: usize) {
    let mut c = combat_ctx(g);
    let e = c.boards[0][idx].eid;
    c.destroy(e);
    c.resolve_deaths();
}

fn hand_cards(g: &Game, p: PlayerId) -> Vec<CardId> {
    g.players[p].hand.iter().map(|h| h.item.card()).collect()
}

#[test]
fn geist_deathrattle_improves_tavern_spells() {
    for (id, n) in [(GEIST, 2), ("BG34_Giant_034_G", 4)] {
        let mut g = game(1);
        set_board(&mut g, 0, &[id]);
        kill(&mut g, 0);
        let st = &g.players[0].stats;
        assert_eq!((st.spell_atk, st.spell_hp), (n, n));
    }
}

#[test]
fn deathswarmer_damage_gives_undead_attack() {
    for (id, n) in [(DEATHSWARMER, 1), ("BG34_Giant_081_G", 2)] {
        let mut g = game(2);
        set_board(&mut g, 0, &[id, SKELETON, MICROBOT]);
        let base = stats(&g, 0);
        let mut c = combat_ctx(&mut g);
        let e = c.boards[0][0].eid;
        c.damage(0, e, 1);
        c.damage(0, e, 1);
        let atk: Vec<i32> = c.boards[0].iter().map(|m| m.atk).collect();
        drop(c);
        assert_eq!(g.players[0].stats.undead_atk, 2 * n);
        // Undead only, in combat and on the warband ("wherever they are").
        let want = vec![base[0].0 + 2 * n, base[1].0 + 2 * n, base[2].0];
        assert_eq!(atk, want);
        let kept: Vec<i32> = stats(&g, 0).iter().map(|s| s.0).collect();
        assert_eq!(kept, want);
    }
}

#[test]
fn recycler_avenge_raises_maximum_gold() {
    for (id, n) in [(RECYCLER, 1), ("BG34_Giant_082_G", 2)] {
        let mut g = game(3);
        set_board(&mut g, 0, &[id, SKELETON, SKELETON, SKELETON, SKELETON]);
        let mut c = combat_ctx(&mut g);
        for i in 1..=3 {
            let e = c.boards[0][i].eid;
            c.destroy(e);
        }
        c.resolve_deaths();
        drop(c);
        assert_eq!(g.players[0].max_gold_bonus, 0);
        // The fourth death (on a fresh combat copy the count restarts).
        let mut c = combat_ctx(&mut g);
        for i in 1..=4 {
            let e = c.boards[0][i].eid;
            c.destroy(e);
        }
        c.resolve_deaths();
        drop(c);
        assert_eq!(g.players[0].max_gold_bonus, n);
    }
}

#[test]
fn pillager_deathrattle_gets_tavern_coins() {
    for (id, n) in [(PILLAGER, 1), ("BG34_Giant_204_G", 2)] {
        let mut g = game(4);
        set_board(&mut g, 0, &[id]);
        assert!(g.players[0].board[0].has(Kw::TAUNT.union(Kw::REBORN)));
        kill(&mut g, 0);
        assert_eq!(hand_cards(&g, 0), vec![cid(COIN); n]);
    }
}

#[test]
fn radio_star_copies_its_killer_with_full_health() {
    for (id, n) in [(RADIO_STAR, 1), ("BG34_Giant_330_G", 2)] {
        let mut g = game(5);
        set_board(&mut g, 0, &[id]);
        set_board(&mut g, 1, &[MICROBOT]);
        let mut c = combat_ctx(&mut g);
        let (star, killer) = (c.boards[0][0].eid, c.boards[1][0].eid);
        c.buff(killer, 5, 7);
        c.give_kw(killer, Kw::TAUNT);
        let full = c.minion(killer).map(|m| (m.atk, m.max_hp)).unwrap();
        c.damage(0, killer, 3);
        c.damage(killer, star, 99);
        c.resolve_deaths();
        drop(c);
        let hand = &g.players[0].hand;
        assert_eq!(hand.len(), n);
        for h in hand {
            let Item::Minion(m) = &h.item else {
                panic!("expected a minion")
            };
            assert_eq!(m.card, cid(MICROBOT));
            assert_eq!((m.atk, m.hp, m.max_hp), (full.0, full.1, full.1));
            assert!(m.has(Kw::TAUNT) && m.alive());
            assert_ne!(m.eid, killer);
        }
    }
}

#[test]
fn radio_star_needs_an_enemy_killer() {
    let mut g = game(6);
    set_board(&mut g, 0, &[RADIO_STAR, SKELETON]);
    set_board(&mut g, 1, &[MICROBOT]);
    // Destroyed by an effect: no killer.
    kill(&mut g, 0);
    assert!(g.players[0].hand.is_empty());
    // Killed by a friendly minion.
    let mut c = combat_ctx(&mut g);
    let (star, friend) = (c.boards[0][0].eid, c.boards[0][1].eid);
    c.damage(friend, star, 99);
    c.resolve_deaths();
    drop(c);
    assert!(g.players[0].hand.is_empty());
}

#[test]
fn radio_star_copies_a_killer_that_died_too() {
    let mut g = game(7);
    set_board(&mut g, 0, &[RADIO_STAR]);
    set_board(&mut g, 1, &[MICROBOT]);
    let mut c = combat_ctx(&mut g);
    let (star, killer) = (c.boards[0][0].eid, c.boards[1][0].eid);
    c.buff(killer, 4, 4);
    let full = c.minion(killer).map(|m| (m.atk, m.max_hp)).unwrap();
    c.damage(killer, star, 99);
    c.damage(star, killer, 99);
    c.resolve_deaths();
    drop(c);
    let Some(Item::Minion(m)) = g.players[0].hand.first().map(|h| &h.item) else {
        panic!("expected a minion")
    };
    assert_eq!((m.card, m.atk, m.hp), (cid(MICROBOT), full.0, full.1));
}

/// A traded Eternal Knight comes back with its recorded stats: the enemy's "this game" bonuses
/// (which grow with that very death) are not taken off them.
#[test]
fn radio_star_copy_of_a_dead_knight_keeps_its_stats() {
    let mut g = game(9);
    g.players[1].stats.eternal_knight_deaths = 3;
    set_board(&mut g, 0, &[RADIO_STAR]);
    set_board(&mut g, 1, &["BG25_008"]);
    let mut c = combat_ctx(&mut g);
    let (star, killer) = (c.boards[0][0].eid, c.boards[1][0].eid);
    let full = c.minion(killer).map(|m| (m.atk, m.max_hp)).unwrap();
    c.damage(killer, star, 99);
    c.damage(star, killer, 99);
    c.resolve_deaths();
    drop(c);
    let Some(Item::Minion(m)) = g.players[0].hand.first().map(|h| &h.item) else {
        panic!("expected a minion")
    };
    assert!(full.0 > 4, "the Knight has its owner's bonus");
    assert_eq!((m.atk, m.hp, m.max_hp), (full.0, full.1, full.1));
}

#[test]
fn warghoul_triggers_adjacent_deathrattles() {
    // Golden: both neighbours.
    let mut g = game(8);
    set_board(&mut g, 0, &[PILLAGER, "BG34_Giant_331_G", GEIST]);
    kill(&mut g, 1);
    assert_eq!(hand_cards(&g, 0), vec![cid(COIN)]);
    assert_eq!(g.players[0].stats.spell_atk, 2);

    // Normal: one of them, and only minions with a Deathrattle are picked.
    let mut g = game(8);
    set_board(&mut g, 0, &[PILLAGER, WARGHOUL, GEIST]);
    kill(&mut g, 1);
    let coins = g.players[0].hand.len() as i32;
    assert_eq!(coins + g.players[0].stats.spell_atk / 2, 1);

    let mut g = game(8);
    set_board(&mut g, 0, &[SKELETON, WARGHOUL, GEIST]);
    kill(&mut g, 1);
    assert_eq!(g.players[0].stats.spell_atk, 2);

    // Another Warghoul is never triggered.
    let mut g = game(8);
    set_board(&mut g, 0, &[GEIST, WARGHOUL, WARGHOUL]);
    kill(&mut g, 2);
    assert_eq!(g.players[0].stats.spell_atk, 0);
}

#[test]
fn embalmer_gives_reborn_to_summoned_minions() {
    for (id, n) in [(EMBALMER, 1), ("BG34_Giant_332_G", 2)] {
        // In combat.
        let mut g = game(9);
        set_board(&mut g, 0, &[id]);
        let mut c = combat_ctx(&mut g);
        let new = c.summon_card(0, 1, cid(SKELETON), 3);
        let reborn = new
            .iter()
            .filter(|e| c.minion(**e).is_some_and(|m| m.has(Kw::REBORN)))
            .count();
        drop(c);
        assert_eq!(reborn, n);

        // In the Tavern: the first minions played each turn.
        let reborn = |g: &Game| -> usize {
            let b = &g.players[0].board;
            b.iter().filter(|m| m.has(Kw::REBORN)).count()
        };
        for _ in 0..3 {
            give(&mut g, 0, SKELETON);
            play_last(&mut g, 0);
        }
        assert_eq!(reborn(&g), n);
        assert!(g.players[0].board[1].has(Kw::REBORN));
        // A minion that has Reborn already doesn't use a charge.
        let mut g = game(9);
        set_board(&mut g, 0, &[id]);
        give(&mut g, 0, PILLAGER);
        play_last(&mut g, 0);
        for _ in 0..3 {
            give(&mut g, 0, SKELETON);
            play_last(&mut g, 0);
        }
        assert_eq!(reborn(&g), n + 1);
        // The charges come back at the start of the next turn.
        g.players[0].board.truncate(1);
        end_turn_all(&mut g);
        for _ in 0..3 {
            give(&mut g, 0, SKELETON);
            play_last(&mut g, 0);
        }
        assert_eq!(reborn(&g), n);
    }
}

#[test]
fn hag_gives_full_reborn() {
    // Normal: the Undead to the right only.
    let mut g = game(10);
    set_board(&mut g, 0, &[SKELETON, HAG, SKELETON]);
    let mut c = combat_ctx(&mut g);
    c.run_board_hook(0, |s| s.start_of_combat, 1, "Start of Combat");
    let (left, right) = (c.boards[0][0].eid, c.boards[0][2].eid);
    assert!(!c.minion(left).unwrap().has(Kw::REBORN));
    assert!(c.minion(right).unwrap().has(Kw::REBORN));
    c.buff(right, 10, 20);
    c.give_kw(right, Kw::TAUNT);
    let full = c.minion(right).map(|m| (m.atk, m.max_hp)).unwrap();
    c.damage(0, right, 5);
    c.destroy(right);
    c.resolve_deaths();
    let back = &c.boards[0][2];
    assert_eq!(
        (back.card, back.atk, back.hp),
        (cid(SKELETON), full.0, full.1)
    );
    assert!(back.has(Kw::TAUNT) && !back.has(Kw::REBORN));
    drop(c);

    // Not an Undead: nothing.
    let mut g = game(10);
    set_board(&mut g, 0, &[HAG, MICROBOT]);
    let mut c = combat_ctx(&mut g);
    c.run_board_hook(0, |s| s.start_of_combat, 1, "Start of Combat");
    assert!(!c.boards[0][1].has(Kw::REBORN));
    drop(c);

    // Golden: both adjacent Undead.
    let mut g = game(10);
    set_board(
        &mut g,
        0,
        &[SKELETON, "BG34_Giant_342_G", SKELETON, SKELETON],
    );
    let mut c = combat_ctx(&mut g);
    c.run_board_hook(0, |s| s.start_of_combat, 1, "Start of Combat");
    let reborn: Vec<bool> = c.boards[0].iter().map(|m| m.has(Kw::REBORN)).collect();
    assert_eq!(reborn, vec![true, false, true, false]);
}

#[test]
fn festergut_summons_and_gets_creations() {
    for (id, n) in [(FESTERGUT, 1), ("BG34_Giant_590_G", 2)] {
        let mut g = game(11);
        set_board(&mut g, 0, &[id]);
        let mut c = combat_ctx(&mut g);
        let e = c.boards[0][0].eid;
        c.destroy(e);
        c.resolve_deaths();
        let summoned: Vec<(CardId, Vec<CardId>)> = c.boards[0]
            .iter()
            .map(|m| (m.card, m.attached.clone()))
            .collect();
        drop(c);
        assert_eq!(summoned.len(), n);
        let got: Vec<(CardId, Vec<CardId>)> = g.players[0]
            .hand
            .iter()
            .map(|h| match &h.item {
                Item::Minion(m) => (m.card, m.attached.clone()),
                _ => panic!("expected a minion"),
            })
            .collect();
        assert_eq!(got, summoned);
        assert!(got.iter().all(|x| x.1.len() == 2));
    }
}

#[test]
fn ghoul_acabra_buffs_after_deathrattles() {
    for (id, a, h) in [(GHOUL, 3, 2), ("BG34_Giant_609_G", 6, 4)] {
        let mut g = game(12);
        set_board(&mut g, 0, &[id, GEIST, SKELETON]);
        let base = stats(&g, 0);
        // A death without a Deathrattle: nothing.
        kill(&mut g, 2);
        assert_eq!(stats(&g, 0), base);
        kill(&mut g, 1);
        // The minions alive when it triggered (not the dead Geist).
        let mut want: Vec<(i32, i32)> = base.iter().map(|s| (s.0 + a, s.1 + h)).collect();
        want[1] = base[1];
        assert_eq!(stats(&g, 0), want);
    }
}

#[test]
fn caretaker_skeletons_that_dont_fit() {
    for (id, n) in [(CARETAKER, 5), ("BG34_Giant_618_G", 10)] {
        let mut g = game(13);
        set_board(&mut g, 0, &[SKELETON, SKELETON, id, MICROBOT, MICROBOT]);
        let mut c = combat_ctx(&mut g);
        let e = c.boards[0][2].eid;
        c.destroy(e);
        c.resolve_deaths();
        let cards: Vec<CardId> = c.boards[0].iter().map(|m| m.card).collect();
        let first = c.boards[0][0].atk;
        drop(c);
        // Three fit (where it died); the others give +1 Attack each.
        let (s, m) = (cid(SKELETON), cid(MICROBOT));
        assert_eq!(cards, vec![s, s, s, s, s, m, m]);
        assert_eq!(g.players[0].stats.undead_atk, n - 3);
        assert_eq!(first, 1 + n - 3);
    }
}

#[test]
fn deadstomper_buffs_after_summons() {
    for (id, n) in [(DEADSTOMPER, 4), ("BG34_Giant_654_G", 8)] {
        // In combat: kept permanently.
        let mut g = game(14);
        set_board(&mut g, 0, &[id, MICROBOT]);
        let base = stats(&g, 0);
        let mut c = combat_ctx(&mut g);
        let new = c.summon_card(0, 2, cid(SKELETON), 2);
        let atk = c.minion(new[0]).unwrap().atk;
        drop(c);
        // The first Skeleton saw both summons.
        assert_eq!(atk, 1 + 2 * n);
        let want: Vec<(i32, i32)> = base.iter().map(|s| (s.0 + 2 * n, s.1)).collect();
        assert_eq!(stats(&g, 0), want);

        // In the Tavern.
        give(&mut g, 0, SKELETON);
        play_last(&mut g, 0);
        let mut want: Vec<(i32, i32)> = want.iter().map(|s| (s.0 + n, s.1)).collect();
        want.push((1 + n, 1));
        assert_eq!(stats(&g, 0), want);
    }
}

#[test]
fn anubarak_after_you_play_an_undead() {
    for (id, n) in [(ANUBARAK, 3), ("BG34_PreMadeChamp_083_G", 6)] {
        let mut g = game(15);
        // Playing it doesn't count.
        give(&mut g, 0, id);
        play_last(&mut g, 0);
        assert_eq!(g.players[0].stats.undead_atk, 0);
        let base = g.players[0].board[0].atk;
        give(&mut g, 0, MICROBOT);
        play_last(&mut g, 0);
        assert_eq!(g.players[0].stats.undead_atk, 0);
        give(&mut g, 0, SKELETON);
        play_last(&mut g, 0);
        assert_eq!(g.players[0].stats.undead_atk, n);
        let b = stats(&g, 0);
        assert_eq!((b[0].0, b[1].0, b[2].0), (base + n, 1, 1 + n));
    }
}
