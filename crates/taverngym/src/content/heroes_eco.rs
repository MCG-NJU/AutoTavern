//! Heroes: passive and economy Hero Powers.

use super::heroes::shown_left;
use super::*;

fn give_banana(c: &mut Ctx, side: Side) {
    let card = if c.g.rng.chance(33, 100) {
        "BG_TB_006"
    } else {
        "BG_EX1_014t"
    };
    c.give_card(side, cid(card));
}

fn passive(s: Script) -> Script {
    Script {
        flags: s.flags | flag::PASSIVE,
        hero_power_uses: 0,
        ..s
    }
}

fn active(spec: TargetSpec, f: crate::script::FxT) -> Script {
    Script {
        hero_power: Some(f),
        hero_power_target: spec,
        ..E
    }
}

/// Advance this power's counter by `n`; returns how many times `every` was completed.
fn tick(c: &mut Ctx, me: Me, n: i32, every: i32) -> i32 {
    let Some(h) = c.power_mut(me.side, me.card) else {
        return 0;
    };
    h.counter = h.counter.saturating_add(n);
    let k = h.counter / every;
    h.counter %= every;
    k
}

fn mine(me: Me, e: &Event) -> bool {
    e.side() == me.side
}

/// Track this power's own contribution separately from unrelated Tavern/spell bonuses.
fn note_owned_bonus(c: &mut Ctx, side: Side, card: CardId) {
    let n = c.card_data(side, card).saturating_add(1);
    c.set_card_data(side, card, n);
}

/// Remove a passive bonus when the owner loses the last copy of that Hero Power.
/// Already bought minions and resolved spells retain the stats they gained.
pub fn hero_power_removed(c: &mut Ctx, side: Side, card: CardId) {
    let saurfang = cid("BG20_HERO_102p");
    let rakanishu = cid("TB_BaconShop_HP_085t");
    if card != saurfang && card != rakanishu {
        return;
    }
    let n = c.card_data(side, card).max(0);
    c.set_card_data(side, card, 0);
    if n == 0 {
        return;
    }
    if let Some(p) = c.player(side) {
        if card == saurfang {
            p.stats.tavern_atk = p.stats.tavern_atk.saturating_sub(n);
            p.stats.tavern_hp = p.stats.tavern_hp.saturating_sub(n);
        } else {
            p.stats.spell_atk = p.stats.spell_atk.saturating_sub(n);
            p.stats.spell_hp = p.stats.spell_hp.saturating_sub(n);
        }
    }
    if card == saurfang {
        c.buff_tavern(side, -n, -n, Races::NONE);
    }
}

/// `shown` of Saurfang / Rakanishu: "+1/+1" with its improvements, the period of 3 and
/// what is left of it.
fn shown_improving(c: &Ctx, sh: &crate::script::Shown) -> Vec<i32> {
    let (level, n) = c
        .player_ref(sh.side)
        .and_then(|p| p.power_of(sh.card))
        .map_or((0, 0), |h| (h.level, h.counter));
    vec![
        level.saturating_add(1),
        level.saturating_add(1),
        3,
        (3 - n).max(0),
    ]
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Rokara: after a friendly minion kills an enemy, give it +1 Attack permanently.
        (
            "BG20_HERO_100p",
            passive(Script {
                on_event: Some(|c, me, e| {
                    // Mutual kills count: the warband copy keeps the Attack.
                    if let Event::Killed { side, killer, .. } = e
                        && *side == me.side
                    {
                        c.buff_perm(me.side, *killer, 1, 0);
                    }
                }),
                listens: ev::KILLED,
                ..E
            }),
        ),
        // Overlord Saurfang: minions in the Tavern have +1/+1; improves (by the base) after you
        // buy 3 minions.
        (
            "BG20_HERO_102p",
            passive(Script {
                on_attach: Some(|c, me| {
                    note_owned_bonus(c, me.side, me.card);
                    if let Some(p) = c.player(me.side) {
                        p.stats.tavern_atk = p.stats.tavern_atk.saturating_add(1);
                        p.stats.tavern_hp = p.stats.tavern_hp.saturating_add(1);
                    }
                    c.buff_tavern(me.side, 1, 1, Races::NONE);
                }),
                on_event: Some(|c, me, e| {
                    if let Event::Bought { side, card, .. } = e
                        && *side == me.side
                        && db().get(*card).is_minion()
                        && tick(c, me, 1, 3) > 0
                    {
                        note_owned_bonus(c, me.side, me.card);
                        if let Some(p) = c.player(me.side) {
                            p.stats.tavern_atk = p.stats.tavern_atk.saturating_add(1);
                            p.stats.tavern_hp = p.stats.tavern_hp.saturating_add(1);
                        }
                        c.buff_tavern(me.side, 1, 1, Races::NONE);
                        // The level is only kept for the card text (`shown`).
                        if let Some(h) = c.power_mut(me.side, me.card) {
                            h.level = h.level.saturating_add(1);
                        }
                    }
                }),
                listens: ev::BOUGHT,
                // "Minions in the Tavern have +1/+1. Improves after you buy 3 minions.
                // (3 left!)"
                shown: Some(shown_improving),
                ..E
            }),
        ),
        // Guff Runetotem: after you buy 20 Tiers' worth of cards, get a Triple Reward.
        (
            "BG20_HERO_242p",
            passive(Script {
                on_event: Some(|c, me, e| {
                    if let Event::Bought { side, card, .. } = e
                        && *side == me.side
                    {
                        for _ in 0..tick(c, me, db().get(*card).tier as i32, 20) {
                            give_triple_reward(c, me.side);
                        }
                    }
                }),
                listens: ev::BOUGHT,
                // "After you buy 20 Tiers' worth of cards, get a Triple Reward. (20 left!)"
                shown: Some(|c, sh| vec![20, shown_left(c, sh, 20)]),
                ..E
            }),
        ),
        // Kurtrus Ashfallen: once per turn, after you buy 3 minions, get a plain copy of one of
        // them. The count restarts each turn.
        (
            "BG20_HERO_280p5",
            passive(Script {
                start_of_turn: Some(|c, me| {
                    if let Some(h) = c.power_mut(me.side, me.card) {
                        h.memory.clear();
                    }
                }),
                on_event: Some(|c, me, e| {
                    let Event::Bought { side, card, .. } = e else {
                        return;
                    };
                    if *side != me.side || !db().get(*card).is_minion() {
                        return;
                    }
                    let Some(h) = c.power_mut(me.side, me.card) else {
                        return;
                    };
                    if h.uses_turn > 0 {
                        return;
                    }
                    h.memory.push(db().normal_of(*card));
                    if h.memory.len() < 3 {
                        return;
                    }
                    let got = std::mem::take(&mut h.memory);
                    h.uses_turn = 1;
                    if let Some(&x) = c.g.rng.pick(&got) {
                        c.give_card(me.side, x);
                    }
                }),
                listens: ev::BOUGHT,
                // "Once per turn, after you buy 3 minions, get a plain copy of one of them.
                // (3 left!)": the minions bought so far this turn are in `memory`.
                shown: Some(|c, sh| {
                    let n = c
                        .player_ref(sh.side)
                        .and_then(|p| p.power_of(sh.card))
                        .map_or(0, |h| h.memory.len() as i32);
                    vec![3, (3 - n).max(0)]
                }),
                ..E
            }),
        ),
        // Ini Stormcoil: after 9 friendly minions die, get a random Mech.
        (
            "BG22_HERO_200p",
            passive(Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Died { side, .. } if *side == me.side)
                        && tick(c, me, 1, 9) > 0
                    {
                        let tier = c.tier(me.side);
                        if let Some(m) =
                            c.random_minion(|d| d.races.intersects(Races::MECH) && d.tier <= tier)
                        {
                            c.give_card(me.side, m);
                        }
                    }
                }),
                listens: ev::DIED,
                ..E
            }),
        ),
        // Enhance-o Mechano: after the Tavern is Refreshed, give a random minion in it a random
        // Bonus Keyword, twice.
        (
            "BG24_HERO_204p",
            passive(Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Refreshed { side, .. } if *side == me.side) {
                        for _ in 0..2 {
                            let t = c.tavern_minions(me.side);
                            if let Some(&sid) = c.g.rng.pick(&t) {
                                give_random_bonus_keyword(c, sid);
                            }
                        }
                    }
                }),
                listens: ev::REFRESHED,
                ..E
            }),
        ),
        // Loh: after 15 friendly minions attack, get a Triple Reward.
        (
            "BG33_HERO_001p_ALT",
            passive(Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Attacks { .. }) && mine(me, e) && tick(c, me, 1, 15) > 0 {
                        give_triple_reward(c, me.side);
                    }
                }),
                listens: ev::ATTACKS,
                // "After 15 friendly minions attack, get a Triple Reward. (15 left!)"
                shown: Some(|c, sh| vec![15, shown_left(c, sh, 15)]),
                ..E
            }),
        ),
        // Tae'thelan: every third Tavern spell you buy costs (0) (`spell_price`).
        (
            "BG28_HERO_800p",
            passive(Script {
                on_event: Some(|c, me, e| {
                    if let Event::Bought { side, card, .. } = e
                        && *side == me.side
                        && is_tavern_spell(*card)
                    {
                        tick(c, me, 1, 3);
                    }
                }),
                listens: ev::BOUGHT,
                // "Every third Tavern spell you buy costs (0). (2 left!)"
                shown: Some(|c, sh| vec![0, 2 - c.power_counter(sh.side, sh.card).rem_euclid(3)]),
                ..E
            }),
        ),
        // Exarch Othaar: the next Tavern spell you buy costs (1) less (unlocks on Turn 3).
        (
            "BG31_HERO_006p",
            active(TargetSpec::None, |c, me, _| {
                if let Some(p) = c.player(me.side) {
                    p.stats.spell_discount_next += 1;
                }
            }),
        ),
        // Ragnaros: after you buy 12 cards, your Hero Power becomes Sulfuras.
        (
            "TB_BaconShop_HP_087",
            passive(Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Bought { .. }) && mine(me, e) && tick(c, me, 1, 12) > 0 {
                        let slot = c
                            .player_ref(me.side)
                            .and_then(|p| p.powers.iter().position(|h| h.card == me.card))
                            .unwrap_or(0);
                        c.set_hero_power(me.side, slot, cid("TB_BaconShop_HP_087t"));
                    }
                }),
                listens: ev::BOUGHT,
                // "After you buy 12 cards, get Sulfuras. (12 left!)"
                shown: Some(|c, sh| vec![12, shown_left(c, sh, 12)]),
                ..E
            }),
        ),
        // Sulfuras: end of turn, your left- and right-most minions +8/+8.
        (
            "TB_BaconShop_HP_087t",
            passive(Script {
                end_of_turn: Some(|c, me| {
                    let (l, r) = (c.leftmost(me.side), c.rightmost(me.side));
                    if let Some(l) = l {
                        c.buff(l, 8, 8);
                    }
                    if let Some(r) = r
                        && Some(r) != l
                    {
                        c.buff(r, 8, 8);
                    }
                }),
                ..E
            }),
        ),
        // Queen Wagtoggle: Start of Combat: a friendly minion of each type +1/+1; improves (by
        // the base) after you spend 10 Gold.
        (
            "TB_BaconShop_HP_037a",
            passive(Script {
                start_of_combat: Some(|c, me| {
                    let k = 1 + c
                        .player_ref(me.side)
                        .and_then(|p| p.power_of(me.card))
                        .map_or(0, |h| h.uses_game as i32);
                    for (t, _) in one_minion_per_type(c, me.side, |_| true) {
                        c.buff(t, k, k);
                    }
                }),
                on_event: Some(|c, me, e| {
                    if let Event::GoldSpent { side, amount } = e
                        && *side == me.side
                    {
                        let n = tick(c, me, *amount, 10);
                        if let Some(h) = c.power_mut(me.side, me.card) {
                            h.uses_game += n as u32; // improvements
                        }
                    }
                }),
                listens: ev::GOLD_SPENT,
                // "Start of Combat: Give a friendly minion of each type +1/+1. (Improves after
                // you spend 10 Gold!)"
                shown: Some(|c, sh| {
                    let k = 1 + c
                        .player_ref(sh.side)
                        .and_then(|p| p.power_of(sh.card))
                        .map_or(0, |h| h.uses_game as i32);
                    vec![k, k, 10]
                }),
                ..E
            }),
        ),
        // King Mukla: at the start of your turn, get 2 Bananas and give everyone else one.
        (
            "TB_BaconShop_HP_038",
            passive(Script {
                start_of_turn: Some(|c, me| {
                    // Official 26.6.2: each recipient's banana independently has a 33%
                    // chance to be a Big Banana; both spells may target any minion.
                    for _ in 0..2 {
                        give_banana(c, me.side);
                    }
                    let Some(p) = c.owner[me.side] else { return };
                    for q in 0..c.g.players.len() {
                        if q != p && c.g.players[q].alive {
                            c.g.with_recruit(q, |other| give_banana(other, 0));
                        }
                    }
                }),
                ..E
            }),
        ),
        // Wild Banana: give a friendly Beast +1/+1.
        (
            "TB_BaconShop_HP_038t",
            Script {
                spell: Some(|c, me, t| spell_buff(c, me, t, 1, 1)),
                spell_target: TargetSpec::FriendlyRace(Races::BEAST),
                ..E
            },
        ),
        // Chenvaala: after you play 3 Elementals, reduce the cost of upgrading by (3).
        (
            "TB_BaconShop_HP_088",
            passive(Script {
                on_event: Some(|c, me, e| {
                    if let Event::Played { side, played, .. } = e
                        && *side == me.side
                        && played.races.intersects(Races::ELEMENTAL)
                        && tick(c, me, 1, 3) > 0
                        && let Some(p) = c.player(me.side)
                    {
                        p.upgrade_cost = (p.upgrade_cost - 3).max(0);
                    }
                }),
                listens: ev::PLAYED,
                ..E
            }),
        ),
        // Aranna Starseeker: after 14 friendly minions attack, the first minion you buy each
        // turn is free (`minion_price`; counter stays at 14 once reached, uses_turn = used).
        (
            "TB_BaconShop_HP_065",
            passive(Script {
                on_event: Some(|c, me, e| {
                    let Some(h) = c.power_mut(me.side, me.card) else {
                        return;
                    };
                    match e {
                        Event::Attacks { side, .. } if *side == me.side && h.counter < 14 => {
                            h.counter += 1
                        }
                        Event::Bought { side, card, .. }
                            if *side == me.side
                                && h.counter >= 14
                                && db().get(*card).is_minion() =>
                        {
                            h.uses_turn = 1
                        }
                        _ => {}
                    }
                }),
                listens: ev::ATTACKS | ev::BOUGHT,
                // "After 14 friendly minions attack, the first minion you buy each turn is
                // free. (14 left!)"
                shown: Some(|c, sh| vec![14, shown_left(c, sh, 14)]),
                ..E
            }),
        ),
        (
            "TB_BaconShop_HP_065t2",
            passive(Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Bought { side, card, .. } if *side == me.side && db().get(*card).is_minion())
                        && let Some(h) = c.power_mut(me.side, me.card)
                    {
                        h.uses_turn = 1;
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            }),
        ),
        // Skycap'n Kragg: gain 2 Gold, +1 each turn (once per game).
        (
            "TB_BaconShop_HP_076",
            Script {
                // "Gain 2 Gold. Increases by 1 each turn. (Once per game.)"
                shown: Some(|c, _| vec![1 + c.g.turn.max(1) as i32, 1]),
                ..active(TargetSpec::None, |c, me, _| {
                    let n = 1 + c.g.turn as i32;
                    c.gain_gold(me.side, n);
                })
            },
        ),
        // Rakanishu: Tavern spells give an extra +1/+1; every 3 turns, improve this.
        (
            "TB_BaconShop_HP_085t",
            passive(Script {
                on_attach: Some(|c, me| {
                    note_owned_bonus(c, me.side, me.card);
                    if let Some(p) = c.player(me.side) {
                        p.stats.spell_atk = p.stats.spell_atk.saturating_add(1);
                        p.stats.spell_hp = p.stats.spell_hp.saturating_add(1);
                    }
                }),
                start_of_turn: Some(|c, me| {
                    if c.g.turn > 1 && tick(c, me, 1, 3) > 0 {
                        note_owned_bonus(c, me.side, me.card);
                        if let Some(p) = c.player(me.side) {
                            p.stats.spell_atk = p.stats.spell_atk.saturating_add(1);
                            p.stats.spell_hp = p.stats.spell_hp.saturating_add(1);
                            // The level is only kept for the card text (`shown`).
                            if let Some(h) = p.power_of_mut(me.card) {
                                h.level = h.level.saturating_add(1);
                            }
                        }
                    }
                }),
                // "Your Tavern spells give an extra +1/+1. At the start of every 3 turns,
                // improve this. (3 turns left!)"
                shown: Some(shown_improving),
                ..E
            }),
        ),
        // C'Thun: at end of turn, give a friendly minion +1/+1, repeated once more for each
        // turn so far (improves each turn).
        (
            "TB_BaconShop_HP_104",
            Script {
                hero_power: Some(|_, _, _| {}),
                end_of_turn: Some(|c, me| {
                    let used = c
                        .player_ref(me.side)
                        .and_then(|p| p.power_of(me.card))
                        .is_some_and(|h| h.uses_turn > 0);
                    if !used {
                        return;
                    }
                    for _ in 0..c.g.turn {
                        if let Some(t) = c.random_friendly(me.side, |_| true) {
                            c.buff(t, 1, 1);
                        }
                    }
                }),
                // "At end of turn, give a friendly minion +1/+1. Repeat 0 times. (Improves each
                // turn!)": one buff per turn so far, the first is not a repeat.
                shown: Some(|c, _| vec![1, 1, c.g.turn.max(1) as i32 - 1]),
                ..E
            },
        ),
        // Greybough: minions you summon during combat get +1/+2 and Taunt.
        (
            "TB_BaconShop_HP_107",
            passive(Script {
                on_event: Some(|c, me, e| {
                    if let Event::Summoned { side, eid } = e
                        && *side == me.side
                        && c.combat
                    {
                        c.buff(*eid, 1, 2);
                        c.give_kw(*eid, Kw::TAUNT);
                    }
                }),
                listens: ev::SUMMONED,
                ..E
            }),
        ),
        // Fungalmancer Flurgl: after you sell 5 minions, get a random Murloc.
        (
            "TB_BaconShop_HP_056",
            passive(Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Sold { .. }) && mine(me, e) && tick(c, me, 1, 5) > 0 {
                        let tier = c.tier(me.side);
                        if let Some(m) =
                            c.random_minion(|d| d.races.intersects(Races::MURLOC) && d.tier <= tier)
                        {
                            c.give_card(me.side, m);
                        }
                    }
                }),
                listens: ev::SOLD,
                // "After you sell 5 minions, get a random Murloc. (5 left!)"
                shown: Some(|c, sh| vec![5, shown_left(c, sh, 5)]),
                ..E
            }),
        ),
        // Lich Baz'hial: steal a card (minion or spell) from the Tavern; take 2 damage.
        (
            "TB_BaconShop_HP_049",
            active(TargetSpec::TavernAny, |c, me, t| {
                if let Some(item) = c.take_card_from_shop(me.side, t) {
                    c.add_to_hand(me.side, item);
                }
                c.hero_damage(me.side, 2);
            }),
        ),
        // Pyramad: steal a random minion from the Tavern; double its Health.
        (
            "TB_BaconShop_HP_040",
            active(TargetSpec::None, |c, me, _| {
                let t = c.tavern_minions(me.side);
                let Some(&sid) = c.g.rng.pick(&t) else { return };
                if let Some(mut m) = c.take_from_shop(me.side, sid) {
                    let h = m.hp;
                    m.add_stats(0, h);
                    c.add_to_hand(me.side, Item::Minion(m));
                }
            }),
        ),
        // Dinotamer Brann: after you buy 4 Battlecry minions, get a Brann Bronzebeard (once).
        (
            "TB_BaconShop_HP_048",
            passive(Script {
                on_event: Some(|c, me, e| {
                    let Event::Bought { side, card, .. } = e else {
                        return;
                    };
                    if *side != me.side || !db().get(*card).mech.has(Mech::BATTLECRY) {
                        return;
                    }
                    let Some(h) = c.power_mut(me.side, me.card) else {
                        return;
                    };
                    if h.uses_game > 0 {
                        return;
                    }
                    h.counter += 1;
                    if h.counter >= 4 {
                        h.uses_game = 1;
                        c.give_card(me.side, cid("BG_LOE_077"));
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            }),
        ),
        // Captain Eudora: dig; every 4th dig gets a random Golden minion, Tier <= yours (lower
        // Tiers can be dug up); it holds one pool copy.
        (
            "TB_BaconShop_HP_074",
            Script {
                // "Dig for a Golden minion! (4 Digs left.)"
                shown: Some(|c, sh| vec![shown_left(c, sh, 4)]),
                ..active(TargetSpec::None, |c, me, _| {
                    if tick(c, me, 1, 4) > 0 {
                        let tier = c.tier(me.side);
                        if let Some(m) = c.random_minion(|d| d.tier <= tier && d.golden.is_some()) {
                            let mut g = c.new_minion(me.side, db().golden_of(m));
                            c.take_pool_copy(&mut g);
                            c.add_to_hand(me.side, Item::Minion(g));
                        }
                    }
                })
            },
        ),
    ]
}
