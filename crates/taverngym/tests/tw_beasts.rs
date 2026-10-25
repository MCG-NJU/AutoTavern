//! Tests for the Timewarp cards of `content/tw_beasts.rs`.

mod common;
#[allow(unused_imports)]
use common::*;

use taverngym::cards::CardKind;

const ALLEYCAT: &str = "BG34_Giant_009";
const TABBYCAT: &str = "BG_CFM_315t";
const CHAMELEON: &str = "BG34_Giant_042";
const SHIP: &str = "BG34_Giant_074t";
const BOAR: &str = "BG34_Giant_201";
const FROGS: &str = "BG34_Giant_309";
const HAWKSTRIDER: &str = "BG34_Giant_370";
const HYENA: &str = "BG34_Giant_581";
const SPOREBAT: &str = "BG34_Giant_582";
const TAMUZO: &str = "BG34_Giant_595";
const SWARMER: &str = "BG34_Giant_687";
const KING: &str = "BG34_PreMadeChamp_004";
const BEETLE: &str = "BG28_603t";

/// A game whose lobby has all of `races` (the first such seed from `seed` on).
fn game_with(seed: u64, races: Races) -> Game {
    (seed..seed + 200)
        .map(game)
        .find(|g| g.lobby_races.contains(races))
        .expect("a lobby with these types")
}

fn hand_cards(g: &Game, p: PlayerId) -> Vec<CardId> {
    g.players[p].hand.iter().map(|h| h.item.card()).collect()
}

/// Run the Start of Combat hooks of side 0's board.
fn start_of_combat(c: &mut Ctx) {
    c.run_board_hook(0, |s| s.start_of_combat, 1, "Start of Combat");
    c.resolve_deaths();
}

fn kill(c: &mut Ctx, eid: u32) {
    c.destroy(eid);
    c.resolve_deaths();
}

#[test]
fn alleycat_summons_tabbycats_with_its_stats() {
    let mut g = game(1);
    set_board(&mut g, 0, &[ALLEYCAT]);
    g.players[0].board[0].atk = 9;
    g.players[0].board[0].hp = 8;
    g.with_recruit(0, |c| {
        c.run_board_hook(0, |s| s.end_of_turn, 1, "end of turn")
    });
    assert_eq!(stats(&g, 0), vec![(9, 8), (9, 8)]);
    assert_eq!(g.players[0].board[1].card, cid(TABBYCAT));

    // Golden: 2 Tabbycats.
    set_board(&mut g, 0, &["BG34_Giant_009_G"]);
    g.with_recruit(0, |c| {
        c.run_board_hook(0, |s| s.end_of_turn, 1, "end of turn")
    });
    assert_eq!(stats(&g, 0), vec![(14, 14), (14, 14), (14, 14)]);
    assert!(
        g.players[0].board[1..]
            .iter()
            .all(|m| m.card == cid(TABBYCAT))
    );
}

#[test]
fn chameleon_becomes_a_copy_of_its_left_neighbour() {
    let mut g = game(2);
    set_board(&mut g, 0, &[HYENA, CHAMELEON, "BG34_Giant_042_G"]);
    g.players[0].board[0].atk = 10;
    let eids: Vec<u32> = g.players[0].board.iter().map(|m| m.eid).collect();
    {
        let mut c = combat_ctx(&mut g);
        start_of_combat(&mut c);
        let b = &c.boards[0];
        // The normal one copies the Hyena; the golden one makes a Golden copy of that copy.
        assert_eq!((b[1].card, b[1].atk, b[1].hp), (cid(HYENA), 10, 5));
        assert_eq!(b[2].card, db().golden_of(cid(HYENA)));
        assert_eq!((b[2].atk, b[2].hp), (13, 10));
        assert!(b[2].golden);
        assert_eq!(b.iter().map(|m| m.eid).collect::<Vec<_>>(), eids);
    }
    // For this combat only.
    assert_eq!(g.players[0].board[1].card, cid(CHAMELEON));

    // Nothing to the left: no transform.
    set_board(&mut g, 0, &[CHAMELEON]);
    let mut c = combat_ctx(&mut g);
    start_of_combat(&mut c);
    assert_eq!(c.boards[0][0].card, cid(CHAMELEON));
}

#[test]
fn nellies_ship_crews_a_discovered_pirate() {
    let mut g = game_with(3, Races::PIRATE);
    g.players[0].tier = 3;
    set_board(&mut g, 0, &[SHIP]);
    g.with_recruit(0, |c| {
        c.run_board_hook(0, |s| s.start_of_turn, 1, "start of turn")
    });
    let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
        panic!("expected a Discover")
    };
    assert!(options.iter().all(|o| {
        let d = db().get(o.card());
        d.races.intersects(Races::PIRATE) && d.tier <= 3
    }));
    let pick = options[0].card();
    g.apply(0, Action::Choose(0)).unwrap();
    // The Pirate boards the ship: nothing in hand.
    assert!(g.players[0].pending.is_empty());
    assert!(g.players[0].hand.is_empty());
    assert_ne!(g.players[0].board[0].counter, 0);

    // Deathrattle: summon and get that Pirate.
    {
        let mut c = combat_ctx(&mut g);
        let ship = c.boards[0][0].eid;
        kill(&mut c, ship);
        assert_eq!(c.boards[0].len(), 1);
        assert_eq!(c.boards[0][0].card, pick);
    }
    assert_eq!(hand_cards(&g, 0), vec![pick]);

    // A ship without crew summons nothing.
    g.players[0].hand.clear();
    set_board(&mut g, 0, &[SHIP]);
    let mut c = combat_ctx(&mut g);
    let ship = c.boards[0][0].eid;
    kill(&mut c, ship);
    assert!(c.boards[0].is_empty());
}

#[test]
fn golden_nellies_ship_crews_two_pirates_and_replaces_them_each_turn() {
    let mut g = game_with(4, Races::PIRATE);
    g.players[0].tier = 3;
    set_board(&mut g, 0, &["BG34_Giant_074t_G"]);
    let mut picks = vec![];
    for turn in 0..2 {
        g.with_recruit(0, |c| {
            c.run_board_hook(0, |s| s.start_of_turn, 1, "start of turn")
        });
        assert_eq!(g.players[0].pending.len(), 2);
        picks.clear();
        for _ in 0..2 {
            let Some(Pending::Discover { options, .. }) = g.players[0].pending.last() else {
                panic!("expected a Discover")
            };
            picks.push(options[turn].card());
            g.apply(0, Action::Choose(turn)).unwrap();
        }
        assert!(g.players[0].hand.is_empty());
    }
    let mut c = combat_ctx(&mut g);
    let ship = c.boards[0][0].eid;
    kill(&mut c, ship);
    // Only the last turn's two Pirates.
    let summoned: Vec<CardId> = c.boards[0].iter().map(|m| m.card).collect();
    assert_eq!(summoned, picks);
    drop(c);
    assert_eq!(hand_cards(&g, 0), picks);
}

#[test]
fn every_third_boar_death_gives_a_golden_beast() {
    let mut g = game_with(5, Races::BEAST);
    for _ in 0..3 {
        give(&mut g, 0, BOAR);
        play_last(&mut g, 0);
    }
    // Two die in the recruit phase: nothing yet.
    g.with_recruit(0, |c| {
        for e in c.friendly(0).into_iter().take(2) {
            kill(c, e);
        }
    });
    assert!(g.players[0].hand.is_empty());
    assert_eq!(g.players[0].board.len(), 1);
    // The third dies in combat (the last Boar on the board).
    {
        let mut c = combat_ctx(&mut g);
        let boar = c.boards[0][0].eid;
        kill(&mut c, boar);
    }
    assert_eq!(g.players[0].hand.len(), 1);
    let m = g.players[0].hand[0].item.as_minion().unwrap();
    assert!(m.golden && m.is(Races::BEAST) && m.tier <= g.players[0].tier);

    // The count restarts; a golden third Boar gives 2.
    g.players[0].hand.clear();
    set_board(&mut g, 0, &[BOAR, BOAR, "BG34_Giant_201_G"]);
    g.with_recruit(0, |c| {
        for e in c.friendly(0) {
            kill(c, e);
            if c.friendly(0).len() == 1 {
                assert!(c.g.players[0].hand.is_empty());
            }
        }
    });
    assert_eq!(g.players[0].hand.len(), 2);
    assert!(
        g.players[0]
            .hand
            .iter()
            .all(|h| h.item.as_minion().unwrap().golden)
    );
}

fn buy_first_minion(g: &mut Game) -> u8 {
    let i = g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_some())
        .unwrap();
    let tier = db().get(g.players[0].shop[i].item.card()).tier;
    g.apply(0, Action::Buy(i)).unwrap();
    tier
}

fn spells_in_hand(g: &Game) -> Vec<CardId> {
    hand_cards(g, 0)
        .into_iter()
        .filter(|c| db().get(*c).kind == CardKind::TavernSpell)
        .collect()
}

#[test]
fn nine_frogs_gives_spells_of_the_bought_minions_tier_nine_times() {
    let mut g = game(6);
    g.players[0].gold = 10;
    set_board(&mut g, 0, &[FROGS]);
    let tier = buy_first_minion(&mut g);
    let spells = spells_in_hand(&g);
    assert_eq!(spells.len(), 1);
    assert_eq!(db().get(spells[0]).tier, tier);
    assert_eq!(g.players[0].board[0].counter, 1);

    // Golden: 2 spells, but only what is left of the 9.
    g.players[0].hand.clear();
    set_board(&mut g, 0, &["BG34_Giant_309_G"]);
    buy_first_minion(&mut g);
    assert_eq!(spells_in_hand(&g).len(), 2);
    g.players[0].hand.clear();
    g.players[0].board[0].counter = 8;
    buy_first_minion(&mut g);
    assert_eq!(spells_in_hand(&g).len(), 1);
    assert_eq!(g.players[0].board[0].counter, 9);
}

#[test]
fn hawkstrider_triggers_friendly_deathrattles() {
    let mut g = game(7);
    set_board(&mut g, 0, &[SWARMER, HAWKSTRIDER]);
    {
        let mut c = combat_ctx(&mut g);
        start_of_combat(&mut c);
        let cards: Vec<CardId> = c.boards[0].iter().map(|m| m.card).collect();
        assert_eq!(cards, vec![cid(SWARMER), cid(BEETLE), cid(HAWKSTRIDER)]);
        assert_eq!((c.boards[0][1].atk, c.boards[0][1].hp), (4, 4));
    }
    // Golden: twice.
    g.players[0].stats = Default::default();
    set_board(&mut g, 0, &[SWARMER, "BG34_Giant_370_G"]);
    let mut c = combat_ctx(&mut g);
    start_of_combat(&mut c);
    let b = &c.boards[0];
    assert_eq!(b.len(), 4);
    assert_eq!(b[0].card, cid(SWARMER));
    for m in &b[1..3] {
        assert_eq!((m.card, m.atk, m.hp), (cid(BEETLE), 6, 6));
    }
}

#[test]
fn hyena_grows_permanently_when_a_friendly_beast_dies() {
    let mut g = game(8);
    set_board(&mut g, 0, &[HYENA, TABBYCAT, TABBYCAT, "BG34_Giant_581_G"]);
    set_board(&mut g, 1, &[TABBYCAT]);
    g.with_recruit(0, |c| {
        let cat = c.friendly(0)[1];
        kill(c, cat);
    });
    assert_eq!(stats(&g, 0), vec![(5, 7), (1, 1), (10, 14)]);
    {
        let mut c = combat_ctx(&mut g);
        // An enemy Beast doesn't count.
        let enemy = c.boards[1][0].eid;
        kill(&mut c, enemy);
        let cat = c.boards[0][1].eid;
        kill(&mut c, cat);
    }
    // The combat gain is kept by the warband.
    assert_eq!(stats(&g, 0), vec![(7, 9), (1, 1), (14, 18)]);
}

#[test]
fn sporebat_gives_spells_costing_two_or_more() {
    let mut g = game(9);
    g.players[0].tier = 4;
    set_board(&mut g, 0, &[SPOREBAT, "BG34_Giant_582_G"]);
    assert!(g.players[0].board[0].has(Kw::TAUNT));
    {
        let mut c = combat_ctx(&mut g);
        let bat = c.boards[0][0].eid;
        kill(&mut c, bat);
    }
    assert_eq!(g.players[0].hand.len(), 1);
    {
        let mut c = combat_ctx(&mut g);
        let bat = c.boards[0][1].eid;
        kill(&mut c, bat);
    }
    assert_eq!(g.players[0].hand.len(), 3);
    for card in hand_cards(&g, 0) {
        let d = db().get(card);
        assert_eq!(d.kind, CardKind::TavernSpell);
        assert!(
            d.cost >= 2 && d.tier <= 4,
            "{} {} {}",
            d.name,
            d.cost,
            d.tier
        );
    }
}

#[test]
fn tamuzo_doubles_or_triples_combat_summons() {
    let mut g = game(10);
    set_board(&mut g, 0, &[TAMUZO]);
    // Not in the recruit phase.
    g.with_recruit(0, |c| {
        c.summon_card(0, 1, cid(TABBYCAT), 1);
    });
    assert_eq!(stats(&g, 0), vec![(5, 5), (1, 1)]);
    {
        let mut c = combat_ctx(&mut g);
        c.summon_card(0, 2, cid(TABBYCAT), 1);
        assert_eq!((c.boards[0][2].atk, c.boards[0][2].hp), (2, 2));
    }
    set_board(&mut g, 0, &["BG34_Giant_595_G"]);
    let mut c = combat_ctx(&mut g);
    c.summon_card(0, 1, cid(TABBYCAT), 1);
    assert_eq!((c.boards[0][1].atk, c.boards[0][1].hp), (3, 3));
}

#[test]
fn nest_swarmer_battlecry_deathrattle_and_rally() {
    let mut g = game(11);
    set_board(&mut g, 1, &[TABBYCAT]);
    // Battlecry.
    give(&mut g, 0, SWARMER);
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0), vec![(7, 7), (4, 4)]);
    assert_eq!(g.players[0].board[1].card, cid(BEETLE));
    {
        let mut c = combat_ctx(&mut g);
        // Rally: the old Beetle grows too.
        let swarmer = c.boards[0][0].eid;
        taverngym::combat::attack_now(&mut c, 0, swarmer);
        let b: Vec<(CardId, i32, i32)> =
            c.boards[0].iter().map(|m| (m.card, m.atk, m.hp)).collect();
        assert_eq!(b[1..], [(cid(BEETLE), 6, 6), (cid(BEETLE), 6, 6)]);
        // Deathrattle.
        kill(&mut c, swarmer);
        assert_eq!(c.boards[0].len(), 3);
        assert!(
            c.boards[0]
                .iter()
                .all(|m| (m.card, m.atk, m.hp) == (cid(BEETLE), 8, 8))
        );
    }
    // "This game": the bonus outlives the combat.
    assert_eq!(g.players[0].stats.beetle_atk, 6);
    assert_eq!(stats(&g, 0), vec![(7, 7), (8, 8)]);

    // Golden: +4/+4 and two Beetles.
    g.players[0].stats = Default::default();
    g.players[0].board.clear();
    give(&mut g, 0, "BG34_Giant_687_G");
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0), vec![(14, 14), (6, 6), (6, 6)]);
}

#[test]
fn jungle_king_buffs_summoned_beasts_and_improves_with_spells() {
    let mut g = game(12);
    set_board(&mut g, 0, &[KING]);
    assert!(g.players[0].board[0].has(Kw::STEALTH));
    give(&mut g, 0, TABBYCAT);
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0), vec![(8, 12), (5, 4)]);
    // A spell improves it: +8/+6.
    give(&mut g, 0, "BG28_810");
    play_last(&mut g, 0);
    assert_eq!(g.players[0].board[0].counter, 1);
    give(&mut g, 0, TABBYCAT);
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0), vec![(8, 12), (5, 4), (9, 7)]);
    // A non-Beast gets nothing; in combat the buff applies to summons too.
    {
        let mut c = combat_ctx(&mut g);
        c.summon_card(0, 3, cid("BG_ICC_026t"), 1);
        c.summon_card(0, 4, cid(TABBYCAT), 1);
        assert_eq!((c.boards[0][3].atk, c.boards[0][3].hp), (1, 1));
        assert_eq!((c.boards[0][4].atk, c.boards[0][4].hp), (9, 7));
    }

    // Golden: +8/+6, improved by +8/+6.
    set_board(&mut g, 0, &["BG34_PreMadeChamp_004_G"]);
    give(&mut g, 0, "BG28_810");
    play_last(&mut g, 0);
    give(&mut g, 0, TABBYCAT);
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0), vec![(16, 24), (17, 13)]);
}
