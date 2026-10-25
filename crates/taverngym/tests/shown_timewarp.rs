//! Live card text (`Script::shown`) of the Timewarp cards (`content/tw_*.rs`): the text after
//! the state changed. The fresh state of every hook is covered by `tests/shown.rs`.

mod common;
#[allow(unused_imports)]
use common::*;

use taverngym::debug::live_text;
use taverngym::script::Shown;

const BOAR: &str = "BG34_Giant_201";
const FROGS: &str = "BG34_Giant_309";
const SWARMER: &str = "BG34_Giant_687";
const KING: &str = "BG34_PreMadeChamp_004";
const KILJAEDEN: &str = "BG34_Giant_313";
const IMPFILTRATOR: &str = "BG34_PreMadeChamp_013";
const PROMO_DRAKE: &str = "BG34_Giant_088";
const RED_WHELP: &str = "BG34_Giant_091";
const MOLTEN_ROCK: &str = "BG34_Giant_006";
const LUBBER: &str = "BG34_Giant_066";
const STONE_DRAKE: &str = "BG34_Giant_675";
const TRUMPETER: &str = "BG34_Giant_676";
const ANNOY: &str = "BG34_Giant_007"; // Tier 3 Mech
const SCOURFIN: &str = "BG34_Giant_017";
const MURKY: &str = "BG34_Giant_206"; // 5/5 Murloc
const PAINTER: &str = "BG34_Giant_319";
const MYSTIC: &str = "BG34_Giant_320";
const ELISE: &str = "BG34_Giant_038";
const LUCKY_EGG: &str = "BG34_Giant_683";
const SCOUT: &str = "BG34_Giant_333";
const EMBALMER: &str = "BG34_Giant_332";
const RAT: &str = "BG34_Treasure_905";
const NEW_RECRUIT: &str = "BG34_Treasure_917";

/// Sellemental: a plain 3/3.
const SELLEMENTAL: &str = "BGS_115";
/// Skeleton: a plain 1/1.
const SKELETON: &str = "BG_ICC_026t";
const BEETLE: &str = "BG28_603t";

/// The live English text of minion `m` as player 0 sees it (None = as printed).
fn text_of(g: &mut Game, m: &Minion) -> Option<String> {
    g.with_recruit(0, |c| {
        let sh = Shown {
            side: 0,
            card: m.card,
            golden: m.golden,
            minion: Some(m),
            data: 0,
        };
        live_text(c, &sh).map(|t| t.0)
    })
}

/// The live text of player 0's board minion at `i`.
fn board_text(g: &mut Game, i: usize) -> Option<String> {
    let m = g.players[0].board[i].clone();
    text_of(g, &m)
}

/// The live text of a fresh minion `id` whose script counter is `n`.
fn counter_text(g: &mut Game, id: &str, n: i32) -> Option<String> {
    let mut m = minion(g, 0, id);
    m.counter = n;
    text_of(g, &m)
}

/// The live text of spell `id` for player 0.
fn spell_text(g: &mut Game, id: &str) -> Option<String> {
    g.with_recruit(0, |c| {
        let sh = Shown {
            side: 0,
            card: cid(id),
            golden: false,
            minion: None,
            data: 0,
        };
        live_text(c, &sh).map(|t| t.0)
    })
}

#[test]
fn jungle_king_shows_its_improved_buff() {
    let mut g = game(12);
    set_board(&mut g, 0, &[KING]);
    assert_eq!(board_text(&mut g, 0), None, "as printed before any spell");
    // Casting a spell improves it.
    give(&mut g, 0, "BG28_810");
    play_last(&mut g, 0);
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some("Stealth After you summon a Beast, give it +8/+6. Improves after you cast a spell.")
    );
    // The text is what the next Beast gets (a 1/1 Tabbycat).
    give(&mut g, 0, "BG_CFM_315t");
    play_last(&mut g, 0);
    assert_eq!(stats(&g, 0)[1], (1 + 8, 1 + 6));

    set_board(&mut g, 0, &["BG34_PreMadeChamp_004_G"]);
    give(&mut g, 0, "BG28_810");
    play_last(&mut g, 0);
    give(&mut g, 0, "BG28_810");
    play_last(&mut g, 0);
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some("Stealth After you summon a Beast, give it +24/+18. Improves after you cast a spell.")
    );
}

#[test]
fn boar_shows_the_deaths_left_once_its_count_exists() {
    let mut g = game(13);
    // Never had a Boar: the printed text.
    assert_eq!(counter_text(&mut g, BOAR, 0), None);
    give(&mut g, 0, BOAR);
    play_last(&mut g, 0);
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some(
            "Whenever every third friendly Timewarped Boar dies, get a random Golden Beast. \
             (3 left!)"
        )
    );
    // One Boar dies in combat: two to go (the count is the player's).
    set_board(&mut g, 1, &[SKELETON]);
    {
        let mut c = combat_ctx(&mut g);
        let boar = c.boards[0][0].eid;
        c.destroy(boar);
        c.resolve_deaths();
    }
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some(
            "Whenever every third friendly Timewarped Boar dies, get a random Golden Beast. \
             (2 left!)"
        )
    );
    assert_eq!(
        counter_text(&mut g, "BG34_Giant_201_G", 0).as_deref(),
        Some(
            "Whenever every third friendly Timewarped Boar dies, get 2 random Golden Beasts. \
             (2 left!)"
        )
    );
}

#[test]
fn nine_frogs_count_their_spells_down() {
    let mut g = game(14);
    assert_eq!(
        counter_text(&mut g, FROGS, 4).as_deref(),
        Some("After you buy a minion, get a random Tavern spell from the same Tier. (5 left!)")
    );
    assert_eq!(
        counter_text(&mut g, "BG34_Giant_309_G", 9).as_deref(),
        Some("After you buy a minion, get 2 random Tavern spells from the same Tier. (0 left!)")
    );
}

#[test]
fn nest_swarmer_shows_the_beetle_it_summons() {
    let mut g = game(15);
    give(&mut g, 0, SWARMER);
    play_last(&mut g, 0);
    // Its Battlecry gave Beetles +2/+2 and summoned one.
    let beetle = &g.players[0].board[1];
    assert_eq!((beetle.card, beetle.atk, beetle.hp), (cid(BEETLE), 4, 4));
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some(
            "Battlecry, Deathrattle, and Rally: Your Beetles have +2/+2 this game. Summon a \
             4/4 Beetle."
        )
    );
    assert_eq!(
        counter_text(&mut g, "BG34_Giant_687_G", 0).as_deref(),
        Some(
            "Battlecry, Deathrattle, and Rally: Your Beetles have +4/+4 this game. Summon two \
             4/4 Beetles."
        )
    );
}

#[test]
fn murky_shows_the_battlecries_triggered_this_game() {
    for (id, k) in [(MURKY, 1), ("BG34_Giant_206_G", 2)] {
        let mut g = game(16);
        set_board(&mut g, 0, &[id]);
        assert_eq!(board_text(&mut g, 0), None);
        g.players[0].stats.battlecries_triggered = 3;
        let v = 8 * k;
        assert_eq!(
            board_text(&mut g, 0),
            Some(format!(
                "At the end of your turn, gain +{v}/+{v}. (Improved by each Battlecry you've \
                 triggered this game!)"
            ))
        );
        // The text is what the end of turn gives.
        let before = stats(&g, 0)[0];
        g.apply(0, Action::EndTurn).unwrap();
        assert_eq!(stats(&g, 0)[0], (before.0 + v, before.1 + v));
    }
}

#[test]
fn painter_shows_its_improvements() {
    for (id, k) in [(PAINTER, 1), ("BG34_Giant_319_G", 2)] {
        let mut g = game(17);
        set_board(&mut g, 0, &[ANNOY, id, SCOURFIN]);
        // Two Tier 3 cards played: improved twice (the "Tier 3" of the text stays).
        for _ in 0..2 {
            give(&mut g, 0, ANNOY);
            play_last(&mut g, 0);
        }
        let (a, h) = (9 * k, 6 * k);
        assert_eq!(
            board_text(&mut g, 1),
            Some(format!(
                "At the end of your turn, give adjacent minions +{a}/+{h}. After you play a \
                 card from Tier 3 or below, improve this."
            ))
        );
        let before = stats(&g, 0)[0];
        g.apply(0, Action::EndTurn).unwrap();
        assert_eq!(stats(&g, 0)[0], (before.0 + a, before.1 + h));
    }
}

#[test]
fn mystic_shows_the_murlocs_left_to_sell() {
    for (id, a, h) in [(MYSTIC, 2, 1), ("BG34_Giant_320_G", 4, 2)] {
        let mut g = game(18);
        set_board(&mut g, 0, &[id, MURKY, MURKY, MURKY]);
        g.apply(0, Action::Sell(1)).unwrap();
        g.apply(0, Action::Sell(1)).unwrap();
        assert_eq!(
            board_text(&mut g, 0),
            Some(format!(
                "After you sell 3 Murlocs, your Tavern spells give an extra +{a}/+{h} this \
                 game. (1 left!)"
            ))
        );
        // The third sale starts the count again: as printed.
        g.apply(0, Action::Sell(1)).unwrap();
        assert_eq!(board_text(&mut g, 0), None);
    }
}

#[test]
fn stone_drake_shows_the_stats_sold_this_turn() {
    let mut g = game(19);
    set_board(&mut g, 0, &[STONE_DRAKE, SELLEMENTAL, SKELETON]);
    assert_eq!(board_text(&mut g, 0), None);
    g.apply(0, Action::Sell(1)).unwrap(); // 3/3
    g.apply(0, Action::Sell(1)).unwrap(); // 1/1
    assert_eq!(
        board_text(&mut g, 0).as_deref(),
        Some("Start of Combat: Gain the stats of all the minions you sold this turn. (4/4)")
    );
    // The golden card shows the same tally (it gains double).
    assert_eq!(
        counter_text(&mut g, "BG34_Giant_675_G", 0).as_deref(),
        Some(
            "Start of Combat: Gain double the stats of all the minions you sold this turn. \
             (4/4)"
        )
    );
}

#[test]
fn timewarp_spells_show_the_tavern_spell_bonus() {
    let mut g = game(20);
    assert_eq!(spell_text(&mut g, RAT), None);
    assert_eq!(spell_text(&mut g, NEW_RECRUIT), None);
    g.players[0].stats.spell_atk = 2;
    g.players[0].stats.spell_hp = 1;
    // Timewarped Lubber on the board: your Tavern spells give an extra +1/+1.
    set_board(&mut g, 0, &[SKELETON, LUBBER]);
    assert_eq!(
        spell_text(&mut g, RAT).as_deref(),
        Some("Give a minion +5/+4, then double its stats.")
    );
    assert_eq!(
        spell_text(&mut g, NEW_RECRUIT).as_deref(),
        Some(
            "Give minions in the Tavern +5/+4 this game. The Tavern always has 7 cards this \
             game."
        )
    );
    // The Rat really gives that much: (1+5, 1+4), doubled.
    give(&mut g, 0, RAT);
    cast_on(&mut g, 0, 0);
    assert_eq!(stats(&g, 0)[0], (12, 10));
    // So does the New Recruit ("this game" Tavern bonus).
    give(&mut g, 0, NEW_RECRUIT);
    play_last(&mut g, 0);
    let st = &g.players[0].stats;
    assert_eq!((st.tavern_atk, st.tavern_hp), (5, 4));
}

#[test]
fn progress_counters_show_what_is_left() {
    let mut g = game(21);
    assert_eq!(
        counter_text(&mut g, EMBALMER, 1).as_deref(),
        Some("One minion you summon each turn gains Reborn. (0 left!)")
    );
    assert_eq!(
        counter_text(&mut g, "BG34_Giant_332_G", 1).as_deref(),
        Some("Two minions you summon each turn gain Reborn. (1 left!)")
    );
    assert_eq!(
        counter_text(&mut g, LUCKY_EGG, 1).as_deref(),
        Some(
            "In two turns, choose from three Golden Tier 7 minions to transform into. (1 \
             turn left!)"
        )
    );
    // While its choice is open (counter = the "waiting" mark) nothing is left.
    assert_eq!(
        counter_text(&mut g, LUCKY_EGG, 100).as_deref(),
        Some(
            "In two turns, choose from three Golden Tier 7 minions to transform into. (0 \
             turns left!)"
        )
    );
    assert_eq!(
        counter_text(&mut g, TRUMPETER, 3).as_deref(),
        Some("After you sell 5 Elementals, get a random Elemental. (2 left!)")
    );
    assert_eq!(
        counter_text(&mut g, "BG34_Giant_676_G", 4).as_deref(),
        Some("After you sell 5 Elementals, get two random Elementals. (1 left!)")
    );
    assert_eq!(
        counter_text(&mut g, ELISE, 4).as_deref(),
        Some(
            "After you Refresh 5 times, make the highest-Tier minion in the Tavern Golden. \
             (1 left!)"
        )
    );
    assert_eq!(
        counter_text(&mut g, IMPFILTRATOR, 5).as_deref(),
        Some(
            "After you spend 8 Gold, give minions in the Tavern +3/+3 this game. (3 Gold \
             left!)"
        )
    );
    assert_eq!(
        counter_text(&mut g, "BG34_PreMadeChamp_013_G", 7).as_deref(),
        Some(
            "After you spend 8 Gold, give minions in the Tavern +6/+6 this game. (1 Gold \
             left!)"
        )
    );
}

#[test]
fn improved_amounts_follow_the_counter() {
    let mut g = game(22);
    assert_eq!(
        counter_text(&mut g, "BG34_Giant_313_G", 1).as_deref(),
        Some(
            "The Tavern offers two extra Demons with +21/+21 whenever it is Refreshed. \
             (Upgrades each turn!)"
        )
    );
    assert_eq!(
        counter_text(&mut g, KILJAEDEN, 2).as_deref(),
        Some(
            "The Tavern offers two extra Demons with +21/+21 whenever it is Refreshed. \
             (Upgrades each turn!)"
        )
    );
    assert_eq!(
        counter_text(&mut g, PROMO_DRAKE, 2).as_deref(),
        Some(
            "Start of Combat: Give your minions +15/+15. At the end of your turn, improve \
             this."
        )
    );
    // Counters store retained normal-unit growth, not a multiplier of the Golden
    // base. Red Whelp stores the actual damage increase because its own golden
    // text has a different base (2 instead of 3); target counts stay unchanged.
    assert_eq!(
        counter_text(&mut g, RED_WHELP, 1).as_deref(),
        Some(
            "Start of Combat: Deal 4 damage to two random enemy minions. (Improves after \
             you play a Dragon!)"
        )
    );
    assert_eq!(
        counter_text(&mut g, "BG34_Giant_091_G", 3).as_deref(),
        Some(
            "Start of Combat: Deal 5 damage to four random enemy minions. (Improves after \
             you play a Dragon!)"
        )
    );
    assert_eq!(
        counter_text(&mut g, MOLTEN_ROCK, 3).as_deref(),
        Some("After you play an Elemental, gain +4/+4 and improve this.")
    );
    assert_eq!(
        counter_text(&mut g, "BG34_Giant_006_G", 1).as_deref(),
        Some("After you play an Elemental, gain +3/+3 and improve this.")
    );
    // Timewarped Scout: one more Tier 7 minion per turn held (the "7" stays).
    assert_eq!(
        counter_text(&mut g, SCOUT, 2).as_deref(),
        Some("When you sell this, get 3 random minion from Tier 7. (Improves each turn!)")
    );
    assert_eq!(
        counter_text(&mut g, "BG34_Giant_333_G", 2).as_deref(),
        Some("When you sell this, get 4 random minions from Tier 7. (Improves each turn!)")
    );
}
