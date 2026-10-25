//! `Script::shown` of the trinkets in `content/trinkets{,_p1..p4}.rs`: the text once the
//! trinket's state has changed (`tests/shown.rs` covers the fresh state of every hook).

use taverngym::actions::Action;
use taverngym::cards::{CardId, cid};
use taverngym::debug::{DebugCmd, live_text, run_debug};
use taverngym::script::{Me, Shown, script};
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
    g.players[0].board.clear(); // some heroes start with a minion
    g
}

/// Give player 0 the trinket `id` (its `on_attach` runs).
fn give(g: &mut Game, id: &str) -> CardId {
    run_debug(
        g,
        DebugCmd::GiveTrinket {
            p: 0,
            card: id.into(),
        },
    )
    .unwrap();
    g.players[0].pending.clear();
    cid(id)
}

fn add_minion(g: &mut Game, zone: &str, id: &str) {
    run_debug(
        g,
        DebugCmd::AddMinion {
            p: 0,
            zone: zone.into(),
            card: id.into(),
            pos: None,
            golden: false,
            atk: None,
            hp: None,
        },
    )
    .unwrap();
}

/// The English text player 0 sees on `card` (None = as printed).
fn text(g: &mut Game, card: CardId) -> Option<String> {
    g.with_recruit(0, |c| {
        let sh = Shown {
            side: 0,
            card,
            golden: false,
            minion: None,
            data: 0,
        };
        live_text(c, &sh).map(|t| t.0)
    })
}

fn me(card: CardId) -> Me {
    Me {
        side: 0,
        eid: 0,
        card,
        golden: false,
        opt: 0,
        pos: 0,
        data: 0,
    }
}

/// "After N..., get a card. (N left!)": Lava Lamp counts the minions sold and starts over.
#[test]
fn lava_lamp_counts_the_minions_left_to_sell() {
    let mut g = game(11);
    let lamp = give(&mut g, "BG30_MagicItem_951");
    for _ in 0..6 {
        add_minion(&mut g, "board", "BGS_004");
    }
    assert_eq!(text(&mut g, lamp), None);
    g.apply(0, Action::Sell(0)).unwrap();
    g.apply(0, Action::Sell(0)).unwrap();
    assert_eq!(
        text(&mut g, lamp).as_deref(),
        Some("After you sell 6 minions, get a random Elemental. (4 left!)")
    );
    for _ in 0..3 {
        g.apply(0, Action::Sell(0)).unwrap();
    }
    assert_eq!(
        text(&mut g, lamp).as_deref(),
        Some("After you sell 6 minions, get a random Elemental. (1 left!)")
    );
    g.apply(0, Action::Sell(0)).unwrap();
    assert_eq!(text(&mut g, lamp), None, "the count starts over");
}

/// "At the start of every 2 turns, repeat this. (2 turns left!)": Conch Portrait.
#[test]
fn conch_portrait_counts_the_turns_left() {
    let mut g = game(12);
    let conch = give(&mut g, "BG35_MagicItem_305");
    let start_of_turn = script(conch).unwrap().start_of_turn.unwrap();
    assert_eq!(text(&mut g, conch), None);
    g.with_recruit(0, |c| start_of_turn(c, me(conch)));
    assert_eq!(
        text(&mut g, conch).as_deref(),
        Some("Get a Cloning Conch. At the start of every 2 turns, repeat this. (1 turn left!)")
    );
    g.with_recruit(0, |c| start_of_turn(c, me(conch)));
    assert_eq!(text(&mut g, conch), None, "repeated: 2 turns left again");
}

/// "...give your Pirates +1/+1 and improve this. (7 Gold left!)": Shark Cannon shows the
/// amount its next trigger gives and the Gold left to spend.
#[test]
fn shark_cannon_shows_its_improved_buff_and_the_gold_left() {
    let mut g = game(13);
    let cannon = give(&mut g, "BG32_MagicItem_232");
    add_minion(&mut g, "board", "BG31_826"); // Doubloon Grifter, a 6/6 Pirate
    let stats = |g: &Game| (g.players[0].board[0].atk, g.players[0].board[0].hp);
    assert_eq!(stats(&g), (6, 6));
    g.players[0].gold = 30;
    g.with_recruit(0, |c| c.spend_gold(0, 10));
    assert_eq!(stats(&g), (7, 7), "+1/+1 after the first 7 Gold");
    assert_eq!(
        text(&mut g, cannon).as_deref(),
        Some("After you spend 7 Gold, give your Pirates +2/+2 and improve this. (4 Gold left!)")
    );
    g.with_recruit(0, |c| c.spend_gold(0, 4));
    assert_eq!(stats(&g), (9, 9), "the +2/+2 the text announced");
    assert_eq!(
        text(&mut g, cannon).as_deref(),
        Some("After you spend 7 Gold, give your Pirates +3/+3 and improve this. (7 Gold left!)")
    );
}

/// "Improve this": the amounts kept as a card level (`Ctx::improve_card`).
#[test]
fn improved_trinkets_show_their_current_amount() {
    let mut g = game(14);
    let snarky = give(&mut g, "BG36_MagicItem_450");
    let kibble = give(&mut g, "BG32_MagicItem_200");
    let anklet = give(&mut g, "BG35_MagicItem_701");
    g.with_recruit(0, |c| {
        for _ in 0..2 {
            c.improve_card(0, snarky);
            c.improve_card(0, kibble);
        }
        c.improve_card(0, anklet);
    });
    assert_eq!(
        text(&mut g, snarky).as_deref(),
        Some(
            "Get 2 Snarky Sharks. Whenever a Fishbait dies, give your Beasts +12/+12 and \
             improve this."
        )
    );
    assert_eq!(
        text(&mut g, kibble).as_deref(),
        Some("Whenever a friendly Beast attacks, give it +9 Attack and permanently improve this.")
    );
    assert_eq!(
        text(&mut g, anklet).as_deref(),
        Some(
            "Start of Combat: Your Beasts have +2/+2 this combat. After you summon a Beast, \
             improve this permanently."
        )
    );
}

/// "(2 left!)" each turn: Guiding Candle's Tier 6 Refreshes, counted by real Refreshes.
#[test]
fn guiding_candle_counts_its_refreshes_this_turn() {
    let mut g = game(15);
    let candle = give(&mut g, "BG32_MagicItem_366");
    g.players[0].gold = 10;
    assert_eq!(text(&mut g, candle), None);
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(
        text(&mut g, candle).as_deref(),
        Some("Your first two Refreshes each turn contain only Tier 6 cards. (1 left!)")
    );
    g.apply(0, Action::Refresh).unwrap();
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(
        text(&mut g, candle).as_deref(),
        Some("Your first two Refreshes each turn contain only Tier 6 cards. (0 left!)")
    );
    // A new turn clears the turn data.
    g.players[0].turn_data.clear();
    assert_eq!(text(&mut g, candle), None);
}

/// Demonic Tapestry stays armed ("0 left") until it is used; the Eye of Sargeras shows 0
/// when the next card is the fourth.
#[test]
fn health_cost_trinkets_count_down_to_zero() {
    let mut g = game(16);
    let tapestry = give(&mut g, "BG35_MagicItem_152");
    let sargeras = give(&mut g, "BG30_MagicItem_701");
    g.players[0].gold = 10;
    g.apply(0, Action::Refresh).unwrap();
    assert_eq!(
        text(&mut g, tapestry).as_deref(),
        Some(
            "After you Refresh 4 times, the highest-Tier minion in the Tavern costs Health \
             instead of Gold to buy. (3 left!)"
        )
    );
    for _ in 0..4 {
        g.apply(0, Action::Refresh).unwrap();
    }
    assert!(taverngym::content::tapestry_armed(&g.players[0]));
    assert_eq!(
        text(&mut g, tapestry).as_deref(),
        Some(
            "After you Refresh 4 times, the highest-Tier minion in the Tavern costs Health \
             instead of Gold to buy. (0 left!)"
        )
    );

    assert_eq!(text(&mut g, sargeras), None);
    g.with_recruit(0, |c| {
        for _ in 0..3 {
            c.tick_counter(0, sargeras, 4); // three cards bought
        }
    });
    assert!(taverngym::content::sargeras_ready(&g.players[0]));
    assert_eq!(
        text(&mut g, sargeras).as_deref(),
        Some("Every fourth card you buy costs Health instead of Gold. (0 left!)")
    );
}

/// Transcribing Typewriter keeps the copies left in its card data: 0 there means "used up"
/// for an owned trinket, but "as printed" for one the player doesn't own.
#[test]
fn typewriter_shows_the_copies_left() {
    let mut g = game(17);
    let greater = cid("BG35_MagicItem_931t");
    assert_eq!(text(&mut g, greater), None, "not owned: as printed");
    let typewriter = give(&mut g, "BG35_MagicItem_931");
    assert_eq!(text(&mut g, typewriter), None);
    g.players[0].gold = 10;
    let slot = g.players[0]
        .shop
        .iter()
        .position(|s| s.item.as_minion().is_some())
        .unwrap();
    g.apply(0, Action::Buy(slot)).unwrap();
    assert_eq!(g.players[0].hand.len(), 2, "the minion and its copy");
    assert_eq!(
        text(&mut g, typewriter).as_deref(),
        Some("Get an extra copy of the next 2 minions you buy. (1 left!)")
    );
    g.with_recruit(0, |c| c.set_card_data(0, typewriter, 0));
    assert_eq!(
        text(&mut g, typewriter).as_deref(),
        Some("Get an extra copy of the next 2 minions you buy. (0 left!)")
    );
}

/// "(Improved by... this game!)": amounts scaled by a per-player counter.
#[test]
fn this_game_counters_scale_the_shown_amounts() {
    let mut g = game(18);
    let mallet = give(&mut g, "BG36_MagicItem_302t");
    let murky = give(&mut g, "BG35_MagicItem_753");
    g.players[0].stats.golden_played = 3;
    g.players[0].stats.battlecries_triggered = 4;
    assert_eq!(
        text(&mut g, mallet).as_deref(),
        Some(
            "At the end of your turn, give your minions +16/+8. (Improved by Golden minions \
             you've played this game!)"
        )
    );
    assert_eq!(
        text(&mut g, murky).as_deref(),
        Some(
            "At the end of each turn, give your two left-most minions +5/+10. (Improved by \
             your Battlecries triggered this game!)"
        )
    );
    // The end-of-turn effects give exactly what the texts say.
    add_minion(&mut g, "board", "BGS_004"); // Wrath Weaver 1/3
    for t in [mallet, murky] {
        let end_of_turn = script(t).unwrap().end_of_turn.unwrap();
        g.with_recruit(0, |c| end_of_turn(c, me(t)));
    }
    let m = &g.players[0].board[0];
    assert_eq!((m.atk, m.hp), (1 + 16 + 5, 3 + 8 + 10));
}

/// Hammer of Twilight: the shown Attack is the aura the minions really have.
#[test]
fn hammer_of_twilight_shows_the_aura_of_the_discards() {
    let mut g = game(19);
    add_minion(&mut g, "board", "BGS_004"); // Wrath Weaver 1/3
    let hammer = give(&mut g, "BG36_MagicItem_403");
    assert_eq!(g.players[0].board[0].atk, 2);
    assert_eq!(text(&mut g, hammer), None);
    g.with_recruit(0, |c| c.bump_player_stat(0, |s| s.cards_discarded += 2));
    assert_eq!(g.players[0].board[0].atk, 4);
    assert_eq!(
        text(&mut g, hammer).as_deref(),
        Some("Your minions have +3 Attack. (Improved by each card you've discarded this game!)")
    );

    let mut g = game(19);
    let greater = give(&mut g, "BG36_MagicItem_403t");
    g.players[0].stats.cards_discarded = 2;
    assert_eq!(
        text(&mut g, greater).as_deref(),
        Some("Your minions have +6/+3. (Improved by each card you've discarded this game!)")
    );
}

/// "Your Tavern spells give an extra +A/+B": the trinkets show what `Ctx::spell_extra` adds.
#[test]
fn tavern_spell_trinkets_show_their_current_extra() {
    // Heart of the Forest: improved twice, four spells into the third improvement.
    let mut g = game(20);
    let heart = give(&mut g, "BG32_MagicItem_801t");
    let extra = g.with_recruit(0, |c| {
        c.improve_card(0, heart);
        c.improve_card(0, heart);
        for _ in 0..4 {
            c.tick_counter(0, heart, 6);
        }
        c.spell_extra(0)
    });
    assert_eq!(extra, (3, 3));
    assert_eq!(
        text(&mut g, heart).as_deref(),
        Some(
            "Your Tavern spells give an extra +3/+3. After you cast 6 Tavern spells from hand, \
             improve this. (2 left!)"
        )
    );

    // Honeycomb Ring: two spells cast on minions this turn.
    let mut g = game(20);
    let ring = give(&mut g, "BG36_MagicItem_371");
    let extra = g.with_recruit(0, |c| {
        c.add_turn_data(0, ring, 2);
        c.spell_extra(0)
    });
    assert_eq!(extra, (3, 3));
    assert_eq!(
        text(&mut g, ring).as_deref(),
        Some(
            "Your Tavern spells give an extra +3/+3. After you cast a spell on a minion, \
             improve for this turn only."
        )
    );

    // Multilayered Shield: a Beast and a Murloc in the warband.
    let mut g = game(20);
    let shield = give(&mut g, "BG36_MagicItem_373");
    assert_eq!(text(&mut g, shield), None);
    add_minion(&mut g, "board", "BG26_802");
    add_minion(&mut g, "board", "BG26_350");
    assert_eq!(g.with_recruit(0, |c| c.spell_extra(0)), (3, 6));
    assert_eq!(
        text(&mut g, shield).as_deref(),
        Some(
            "Your Tavern spells give an extra +3/+6. (Improved by each different friendly \
             minion type!)"
        )
    );
}

/// Weighted Gauntlet doubles at the start of each turn.
#[test]
fn weighted_gauntlet_shows_the_doubled_amount() {
    let mut g = game(21);
    let gauntlet = give(&mut g, "BG36_MagicItem_414");
    let start_of_turn = script(gauntlet).unwrap().start_of_turn.unwrap();
    assert_eq!(text(&mut g, gauntlet), None);
    for _ in 0..3 {
        g.with_recruit(0, |c| start_of_turn(c, me(gauntlet)));
    }
    assert_eq!(
        text(&mut g, gauntlet).as_deref(),
        Some(
            "At the end of your turn, give your minions +8/+8. (Doubles at the start of each \
             turn!)"
        )
    );
}

/// A count never goes below 0, whatever the stored value.
#[test]
fn counts_stop_at_zero() {
    let mut g = game(22);
    let heart = give(&mut g, "BG30_MagicItem_713");
    let pilgrimp = give(&mut g, "BG32_MagicItem_821");
    let totem = give(&mut g, "BG36_MagicItem_202");
    g.players[0].counters.push((heart, 50));
    g.players[0].turn_data.push((pilgrimp, 3));
    g.players[0].turn_data.push((totem, 1));
    assert_eq!(
        text(&mut g, heart).as_deref(),
        Some("After 8 friendly minions die, get a random Undead. (0 left!)")
    );
    assert_eq!(
        text(&mut g, pilgrimp).as_deref(),
        Some("One Demon each turn costs Health instead of Gold to buy. (0 left!)")
    );
    assert_eq!(
        text(&mut g, totem).as_deref(),
        Some("The first two Battlecry minions you buy each turn are free. (1 left!)")
    );
}
