//! `Script::shown` of the hero group (Hero Powers and the minions they give): the text
//! follows the state the scripts keep. The fresh state is covered by `tests/shown.rs`.

use taverngym::actions::Action;
use taverngym::cards::{CardId, cid, db};
use taverngym::debug::{DebugCmd, live_text, run_debug};
use taverngym::script::Shown;
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

/// A game on Turn 1 where player 0 has Hero Power `id` (slot 0, freshly gained).
fn with_power(seed: u64, id: &str) -> (Game, CardId) {
    let mut g = game(seed);
    let hp = cid(id);
    g.set_hero_power(0, 0, hp);
    g.players[0].pending.clear();
    (g, hp)
}

/// The English text player 0 reads on `card` now (None: as printed).
fn text(g: &mut Game, card: CardId) -> Option<String> {
    g.with_recruit(0, |c| {
        let sh = Shown {
            side: 0,
            card,
            golden: db().get(card).normal.is_some(),
            minion: None,
            data: 0,
        };
        live_text(c, &sh).map(|t| t.0)
    })
}

/// The English text of a minion `card` whose script counter is `counter`.
fn minion_text(g: &mut Game, card: CardId, counter: i32) -> Option<String> {
    g.with_recruit(0, |c| {
        let mut m = c.new_minion(0, card);
        m.counter = counter;
        let sh = Shown {
            side: 0,
            card,
            golden: db().get(card).normal.is_some(),
            minion: Some(&m),
            data: 0,
        };
        live_text(c, &sh).map(|t| t.0)
    })
}

/// Put a plain minion at the right end of player 0's warband and sell it.
fn sell_one(g: &mut Game) {
    run_debug(
        g,
        DebugCmd::AddMinion {
            p: 0,
            zone: "board".into(),
            card: "BGS_119".into(), // Crackling Cyclone
            pos: None,
            golden: false,
            atk: None,
            hp: None,
        },
    )
    .unwrap();
    let last = g.players[0].board.len() - 1;
    g.apply(0, Action::Sell(last)).unwrap();
}

#[test]
fn selling_counts_down_flurgl_and_grows_ozumats_tentacle() {
    let (mut g, flurgl) = with_power(11, "TB_BaconShop_HP_056");
    assert_eq!(text(&mut g, flurgl), None);
    sell_one(&mut g);
    sell_one(&mut g);
    assert_eq!(
        text(&mut g, flurgl).as_deref(),
        Some("After you sell 5 minions, get a random Murloc. (3 left!)")
    );
    for _ in 0..3 {
        sell_one(&mut g);
    }
    assert_eq!(text(&mut g, flurgl), None, "the count starts again");

    let (mut g, ozumat) = with_power(12, "BG23_HERO_201p");
    for _ in 0..3 {
        sell_one(&mut g);
    }
    assert_eq!(
        text(&mut g, ozumat).as_deref(),
        Some(
            "When you have space in combat, summon a 5/5 Tentacle with Taunt. (Gains +1/+1 \
             after you sell a minion!)"
        )
    );
}

#[test]
fn gold_spent_improves_wagtoggle_and_counts_down_thorim() {
    let (mut g, wag) = with_power(13, "TB_BaconShop_HP_037a");
    g.with_recruit(0, |c| c.spend_gold(0, 23));
    assert_eq!(
        text(&mut g, wag).as_deref(),
        Some(
            "Start of Combat: Give a friendly minion of each type +3/+3. (Improves after you \
             spend 10 Gold!)"
        )
    );

    let (mut g, thorim) = with_power(14, "BG27_HERO_801p2");
    // The Tier 7 pick waits in the power's memory; Gold counts only while it does.
    g.players[0].powers[0].memory = vec![cid("BGS_119")];
    g.with_recruit(0, |c| c.spend_gold(0, 25));
    assert_eq!(
        text(&mut g, thorim).as_deref(),
        Some(
            "Passive. At the start of the game, Discover a Tier 7 minion to get after you spend \
             60 Gold. (35 left!)"
        )
    );
    g.with_recruit(0, |c| c.spend_gold(0, 40));
    assert!(
        text(&mut g, thorim).unwrap().ends_with("(0 left!)"),
        "never below 0"
    );
}

#[test]
fn turn_countdowns_follow_the_turn() {
    let (mut g, marin) = with_power(15, "BG30_HERO_304p");
    g.turn = 3;
    assert_eq!(
        text(&mut g, marin).as_deref(),
        Some("On Turn 5, choose a Lesser Trinket to buy. (2 turns left!)")
    );
    g.turn = 9;
    assert_eq!(
        text(&mut g, marin).as_deref(),
        Some("On Turn 5, choose a Lesser Trinket to buy. (0 turns left!)")
    );

    let (mut g, buttons) = with_power(15, "BG32_HERO_002p");
    g.turn = 6;
    assert_eq!(
        text(&mut g, buttons).as_deref(),
        Some("On Turn 8, choose a Greater Trinket to buy. (2 turns left!)")
    );

    let (mut g, genn) = with_power(15, "BG35_HERO_001p");
    g.turn = 2;
    assert_eq!(
        text(&mut g, genn).as_deref(),
        Some("On Turn 4, Discover two Hero Powers to replace this. (2 turns left!)")
    );

    let (mut g, trastath) = with_power(15, "BG36_HERO_101p");
    g.turn = 4;
    assert_eq!(
        text(&mut g, trastath).as_deref(),
        Some(
            "At the start of the game, Discover a Tier 5 minion with a Dark Gift. It unlocks on \
             Turn 7. (3 turns left!)"
        )
    );
}

#[test]
fn kragg_and_cthun_improve_each_turn() {
    let (mut g, kragg) = with_power(16, "TB_BaconShop_HP_076");
    g.turn = 4;
    assert_eq!(
        text(&mut g, kragg).as_deref(),
        Some("Gain 5 Gold. Increases by 1 each turn. (Once per game.)")
    );
    // What the power gives is what the text says.
    g.players[0].gold = 0;
    g.apply(0, Action::HeroPower(0)).unwrap();
    assert_eq!(g.players[0].gold, 5);

    let (mut g, cthun) = with_power(16, "TB_BaconShop_HP_104");
    g.turn = 3;
    assert_eq!(
        text(&mut g, cthun).as_deref(),
        Some("At end of turn, give a friendly minion +1/+1. Repeat 2 times. (Improves each turn!)")
    );
}

#[test]
fn uses_left_follow_the_uses() {
    // Eudora, used for real: every 4th Dig finds the minion.
    let (mut g, eudora) = with_power(17, "TB_BaconShop_HP_074");
    g.players[0].gold = 10;
    g.apply(0, Action::HeroPower(0)).unwrap();
    assert_eq!(
        text(&mut g, eudora).as_deref(),
        Some("Dig for a Golden minion! (3 Digs left.)")
    );
    g.players[0].powers[0].counter = 3;
    assert_eq!(
        text(&mut g, eudora).as_deref(),
        Some("Dig for a Golden minion! (1 Dig left.)")
    );

    let (mut g, zephrys) = with_power(17, "TB_BaconShop_HP_102");
    g.players[0].powers[0].uses_game = 2;
    assert_eq!(
        text(&mut g, zephrys).as_deref(),
        Some("If you have two copies of a minion, find the third. (1 Wish left!)")
    );

    let (mut g, putricide) = with_power(17, "BG25_HERO_100p");
    g.players[0].powers[0].uses_game = 3;
    assert_eq!(
        text(&mut g, putricide).as_deref(),
        Some("Craft a custom Undead. (0 Creations left!)")
    );
}

#[test]
fn counters_kept_in_the_power() {
    let (mut g, guff) = with_power(18, "BG20_HERO_242p");
    g.players[0].powers[0].counter = 7;
    assert_eq!(
        text(&mut g, guff).as_deref(),
        Some("After you buy 20 Tiers' worth of cards, get a Triple Reward. (13 left!)")
    );

    // Tae'thelan: the counter is the Tavern spells bought; the third one is free.
    let (mut g, tae) = with_power(18, "BG28_HERO_800p");
    g.players[0].powers[0].counter = 1;
    assert_eq!(
        text(&mut g, tae).as_deref(),
        Some("Every third Tavern spell you buy costs (0). (1 left!)")
    );
    g.players[0].powers[0].counter = 2;
    assert_eq!(
        text(&mut g, tae).as_deref(),
        Some("Every third Tavern spell you buy costs (0). (0 left!)")
    );
    assert_eq!(g.spell_price(0, cid("BG28_897")), 0, "and it is free now");

    // Aranna: the counter stays at 14 once reached.
    let (mut g, aranna) = with_power(18, "TB_BaconShop_HP_065");
    g.players[0].powers[0].counter = 14;
    assert_eq!(
        text(&mut g, aranna).as_deref(),
        Some(
            "After 14 friendly minions attack, the first minion you buy each turn is free. (0 \
             left!)"
        )
    );

    let (mut g, jaraxxus) = with_power(18, "TB_BaconShop_HP_036");
    g.players[0].powers[0].counter = 42;
    assert_eq!(
        text(&mut g, jaraxxus).as_deref(),
        Some(
            "After friendly minions deal 150 damage, open a portal to the Twisting Nether! (108 \
             left!)"
        )
    );
}

#[test]
fn pots_and_bought_minions_are_counted_from_the_memory() {
    let (mut g, cookie) = with_power(19, "BG21_HERO_020p");
    g.players[0].powers[0].memory = vec![cid("BGS_119"), cid("BGS_004")];
    assert_eq!(
        text(&mut g, cookie).as_deref(),
        Some(
            "Throw a minion in your pot. When you've gathered 3, Discover from their types. (1 \
             left!)"
        )
    );

    let (mut g, kurtrus) = with_power(19, "BG20_HERO_280p5");
    g.players[0].powers[0].memory = vec![cid("BGS_119")];
    assert_eq!(
        text(&mut g, kurtrus).as_deref(),
        Some("Once per turn, after you buy 3 minions, get a plain copy of one of them. (2 left!)")
    );
}

#[test]
fn cariel_shows_her_improvements() {
    let (mut g, cariel) = with_power(20, "BG21_HERO_000p");
    // values = [extra minions, extra Attack, extra Health] (`improve_conviction`).
    g.players[0].powers[0].values = vec![1, 2, 0];
    assert_eq!(
        text(&mut g, cariel).as_deref(),
        Some(
            "Give 2 random friendly minion +3/+1. Passive: After each combat, choose an \
             improvement."
        )
    );
}

#[test]
fn xavius_counts_his_turns_and_onyxia_her_whelps() {
    let (mut g, xavius) = with_power(21, "BG36_HERO_105p");
    assert!(!g.with_recruit(0, |c| c.period_turn(0, xavius, 4)));
    assert_eq!(
        text(&mut g, xavius).as_deref(),
        Some("Every 4 turns, Discover a minion with a Dark Gift. (3 turns left!)")
    );

    let (mut g, onyxia) = with_power(21, "BG22_HERO_305p");
    g.players[0].powers[0].level = 2; // two Whelps summoned so far
    assert_eq!(
        text(&mut g, onyxia).as_deref(),
        Some("Avenge (4): Summon a 3/3 Whelp that attacks immediately. Improve this by +1/+1.")
    );

    let (mut g, voone) = with_power(21, "BG26_HERO_104p");
    g.players[0].powers[0].counter = 2;
    assert_eq!(
        text(&mut g, voone).as_deref(),
        Some(
            "At the end of every 3 turns, get a plain copy of the left- most card in your hand. \
             (1 turn left!)"
        )
    );
}

#[test]
fn artanis_and_his_protoss_minions() {
    let (mut g, artanis) = with_power(22, "BG31_HERO_802p");
    g.players[0].powers[0].counter = 9;
    assert_eq!(
        text(&mut g, artanis).as_deref(),
        Some(
            "At the start of the game, choose from 2 Protoss minions to get after you buy 14 \
             cards. (5 left!)"
        )
    );

    // Colossus stores damage already gained, not Gold spent: making this entity
    // golden preserves counter=5 rather than multiplying that growth a second time.
    let colossus = cid("BG31_HERO_802pt");
    assert_eq!(minion_text(&mut g, colossus, 0), None);
    assert_eq!(
        minion_text(&mut g, colossus, 5).as_deref(),
        Some("Rally: Deal 7 damage to the target's neighbors. (Improves when you spend Gold!)")
    );
    assert_eq!(
        minion_text(&mut g, db().golden_of(colossus), 5).as_deref(),
        Some("Rally: Deal 9 damage to the target's neighbors. (Improves when you spend Gold!)")
    );
    // Five Gold spent while already golden grants ten damage, so that entity shows 14.
    assert_eq!(
        minion_text(&mut g, db().golden_of(colossus), 10).as_deref(),
        Some("Rally: Deal 14 damage to the target's neighbors. (Improves when you spend Gold!)")
    );

    // Carrier stores growth steps: normal triggers add one, golden triggers add two.
    // Existing steps are preserved when the same entity becomes golden.
    let carrier = cid("BG31_HERO_802pt1");
    assert_eq!(
        minion_text(&mut g, carrier, 2).as_deref(),
        Some("Avenge (4): Summon a 21/21 Interceptor. Then improve this permanently.")
    );
    assert_eq!(
        minion_text(&mut g, db().golden_of(carrier), 1).as_deref(),
        Some("Avenge (4): Summon a 21/21 Interceptor. Then improve this permanently.")
    );
    assert_eq!(
        minion_text(&mut g, db().golden_of(carrier), 2).as_deref(),
        Some("Avenge (4): Summon a 28/28 Interceptor. Then improve this permanently.")
    );
}
