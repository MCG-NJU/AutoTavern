//! Pirate minions (Gold-spend counters, Lockboxes, Bounties).

use super::*;

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Gunpowder Courier: whenever you spend 5 Gold, give your Pirates +3/+1 (golden 2x).
        // The client counts 5, as the text says; only the printed "(6 Gold left!)" still has
        // the old 6.
        (
            "BG26_810",
            Script {
                on_event: Some(|c, me, e| {
                    gold_counter(c, me, e, 5, |c, me| {
                        for _ in 0..me.g() {
                            for p in c.friendly_where(me.side, |m| m.is(Races::PIRATE)) {
                                c.buff(p, 3, 1);
                            }
                        }
                    })
                }),
                listens: ev::GOLD_SPENT,
                // "Whenever you spend 5 Gold, give your Pirates +3/+1. (6 Gold left!)"
                shown: Some(|_, sh| vec![5, 3, 1, (5 - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Ship Master Eudora (back in 36.6.3): Deathrattle: give your minions +6/+6 (twice);
        // Golden minions keep it permanently.
        (
            "BG33_828",
            Script {
                deathrattle: Some(|c, me| {
                    for _ in 0..me.g() {
                        for t in c.friendly(me.side) {
                            if c.minion(t).is_some_and(|m| m.golden) {
                                c.buff_perm(me.side, t, 6, 6);
                            } else {
                                c.buff(t, 6, 6);
                            }
                        }
                    }
                }),
                ..E
            },
        ),
        // Enterprising Escapee: after you spend 6 Gold, get a Lockbox or open yours 1 (2) turn(s)
        // sooner.
        (
            "BG36_523",
            Script {
                on_event: Some(|c, me, e| {
                    gold_counter(c, me, e, 6, |c, me| lockbox_or_hasten(c, me.side, me.g()))
                }),
                listens: ev::GOLD_SPENT,
                // "After you spend 6 Gold, get a Lockbox. If you already have one, it opens 1
                // turns sooner instead. (6 Gold left!)"
                shown: Some(|_, sh| vec![6, sh.g(), (6 - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Sky Admiral Rogers: after you spend 9 Gold, get a random Bounty (golden: two).
        (
            "BG33_823",
            Script {
                on_event: Some(|c, me, e| {
                    gold_counter(c, me, e, 9, |c, me| {
                        give_random_of(c, me.side, &BOUNTIES, me.g())
                    })
                }),
                listens: ev::GOLD_SPENT,
                // "After you spend 9 Gold, get a random Bounty. (9 Gold left!)"
                shown: Some(|_, sh| vec![9, (9 - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Lovesick Balladist: Battlecry: give a Pirate +2 Health, improved (by the base, as every
        // "improve") by each Gold spent this turn (golden: twice).
        (
            "BG26_814",
            Script {
                battlecry: Some(|c, me, t| {
                    if t == 0 {
                        return;
                    }
                    let spent = c
                        .player_ref(me.side)
                        .map(|p| p.stats.gold_spent_turn)
                        .unwrap_or(0);
                    for _ in 0..me.g() {
                        c.buff(t, 0, 2 * (1 + spent));
                    }
                }),
                battlecry_target: TargetSpec::FriendlyRaceOther(Races::PIRATE),
                // "Battlecry: Give a Pirate +2 Health. (Improved by each Gold you spent this
                // turn!)"; golden: "+2 Health twice."
                shown: Some(|c, sh| {
                    let spent = c.player_ref(sh.side).map_or(0, |p| p.stats.gold_spent_turn);
                    vec![2 * (1 + spent)]
                }),
                ..E
            },
        ),
        // Bilgewater Breakout: Battlecry: get a Lockbox, or open yours 1 (2) turn(s) sooner.
        (
            "BG36_520",
            Script {
                battlecry: Some(|c, me, _| lockbox_or_hasten(c, me.side, me.g())),
                ..E
            },
        ),
        // Locked-up Mutineer: Deathrattle: same.
        (
            "BG36_521",
            Script {
                deathrattle: Some(|c, me| lockbox_or_hasten(c, me.side, me.g())),
                ..E
            },
        ),
        // Lockbox: unplayable; counts down in hand (content::hand_start_of_turn).
        (
            LOCKBOX,
            Script {
                flags: flag::UNPLAYABLE,
                // "Unplayable. In 5 turns, break this open and get a random Golden minion
                // with a type! (5 turns left!)": a Lockbox in hand keeps its turns left in
                // its spell data.
                shown: Some(|_, sh| {
                    let left = if sh.data > 0 { sh.data } else { LOCKBOX_TURNS };
                    vec![LOCKBOX_TURNS, left]
                }),
                ..E
            },
        ),
        // Doubloon Grifter (only from Grifter Portrait): whenever you get 4 Pirates, increase
        // your maximum Gold by 1 (2). "Get" = a Pirate card added to your hand, however it
        // got there (counter = Pirates so far).
        (
            "BG31_826",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::CardAdded { side, hid } = e else {
                        return;
                    };
                    let pirate = *side == me.side
                        && c.player_ref(me.side).is_some_and(|p| {
                            p.hand
                                .iter()
                                .find(|h| h.hid == *hid)
                                .and_then(|h| h.item.as_minion())
                                .is_some_and(|m| m.is(Races::PIRATE))
                        });
                    if pirate && c.counter_perm(me.side, me.eid, 1) >= 4 {
                        c.counter_perm(me.side, me.eid, -4);
                        c.add_max_gold(me.side, me.g());
                    }
                }),
                listens: ev::CARD_ADDED,
                // "Whenever you get 4 Pirates, increase your maximum Gold by 1. (4 left!)"
                shown: Some(|_, sh| vec![4, sh.g(), (4 - sh.counter()).max(0)]),
                ..E
            },
        ),
    ]
}
