//! Pirates (phase 1).

use super::*;

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Aureate Laureate: always Golden, but no Triple Reward.
        (
            "BG32_236",
            Script {
                on_create: Some(|m| {
                    make_golden(m);
                    m.triple_reward = false;
                }),
                ..E
            },
        ),
        // Clever Castaway: Activate (2): Discover 1 (2) Tavern spells.
        (
            "BG36_342",
            Script {
                activate: Some((2, TargetSpec::None, |c, me, _| {
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        c.discover_spell(me.side, me.card, 3, |d| d.tier <= tier);
                    }
                })),
                ..E
            },
        ),
        // Azsharan Cutlassier: Battlecry: Tavern spells give an extra +1 (+2) Attack this game.
        (
            "BG33_830",
            Script {
                battlecry: Some(|c, me, _| {
                    if let Some(p) = c.player(me.side) {
                        p.stats.spell_atk = p.stats.spell_atk.saturating_add(me.g());
                    }
                }),
                ..E
            },
        ),
        // Greedy Conniver: if this is Golden when sold, Discover a Tier 7 minion.
        (
            "BG36_369",
            Script {
                on_sell: Some(|c, me| {
                    if me.golden {
                        c.discover_tier7(me.side, me.card, 3);
                    }
                }),
                ..E
            },
        ),
        // Maritime Extortionist: +7/+7 (+14/+14) per Golden minion played (`apply_bonuses`).
        ("BG36_524", E),
        // Elite Navigator: Battlecry: make a friendly Pirate from Tier 4 or below Golden. A
        // targeted Battlecry (hsdata TARGETING_ARROW_TEXT); the golden's second Pirate, and a
        // target-less trigger, are random.
        (
            "BG32_231",
            Script {
                battlecry: Some(|c, me, t| {
                    let eligible = |m: &Minion| {
                        m.eid != me.eid
                            && m.is(Races::PIRATE)
                            && !m.golden
                            && m.tier <= 4
                            && db().get(m.card).golden.is_some()
                    };
                    // The chosen target takes its place also when it is no longer eligible: Brann's
                    // repeat of the Battlecry finds it Golden already and makes no other minion
                    // Golden (Elite Navigator and Captain Sanders with Brann); only the Golden
                    // card's second minion is random.
                    let mut picks = vec![];
                    if t != 0 && c.minion(t).is_some_and(eligible) {
                        picks.push(t);
                    }
                    let mut rest = c.friendly_where(me.side, |m| eligible(m) && m.eid != t);
                    c.g.rng.shuffle(&mut rest);
                    let n = (me.g() as usize).saturating_sub(usize::from(t != 0));
                    picks.extend(rest.into_iter().take(n));
                    for t in picks {
                        if let Some(m) = c.minion_mut(t) {
                            make_golden(m);
                        }
                    }
                }),
                battlecry_target: TargetSpec::FriendlyEligiblePirate,
                ..E
            },
        ),
        // Silent Deliverer: Battlecry: get 1 (2) random Golden Tier 4 minions (no Triple
        // Reward).
        (
            "BG36_343",
            Script {
                battlecry: Some(|c, me, _| {
                    for _ in 0..me.g() {
                        if let Some(card) = c.random_minion(|d| d.tier == 4 && d.golden.is_some()) {
                            let mut m = c.new_minion(me.side, db().golden_of(card));
                            // "It doesn't give a Triple Reward" (a Golden minion made by
                            // an effect gives one unless its card says otherwise).
                            m.triple_reward = false;
                            // A generated minion holds one pool copy when one is left.
                            c.take_pool_copy(&mut m);
                            c.add_to_hand(me.side, Item::Minion(m));
                        }
                    }
                }),
                ..E
            },
        ),
        // Hooktusk, Master Marauder: after you Discover a card, give your other Pirates +1/+1
        // (+2/+2), improved (by the base) by Golden minions you played this game.
        (
            "BG36_344",
            Script {
                on_event: Some(|c, me, e| {
                    if !matches!(e, Event::Discovered { side, .. } if *side == me.side) {
                        return;
                    }
                    let n = c.player_ref(me.side).map_or(0, |p| p.stats.golden_played);
                    let k = me.g() * (1 + n);
                    for t in c.friendly_where(me.side, |m| m.eid != me.eid && m.is(Races::PIRATE)) {
                        c.buff(t, k, k);
                    }
                }),
                listens: ev::DISCOVERED,
                // "After you Discover a card, give your other Pirates +1/+1. (Improved by
                // Golden minions you played this game!)"
                shown: Some(|c, sh| {
                    let n = c.player_ref(sh.side).map_or(0, |p| p.stats.golden_played);
                    let k = sh.g() * (1 + n);
                    vec![k, k]
                }),
                ..E
            },
        ),
    ]
}
