//! Trinkets (lesser and greater). Immediate effects run in `on_attach` (on purchase);
//! persistent ones use start/end-of-turn, Start of Combat or event hooks.

use super::*;
use crate::script::Shown;

/// `Script::shown` of a player-level card: what is left of its "after N... (N left!)" count, which
/// `Ctx::tick_counter` (or the script itself) keeps in the card data.
pub(super) fn shown_left(c: &Ctx, sh: &Shown, every: i32) -> i32 {
    (every - c.card_data(sh.side, sh.card)).max(0)
}

/// `Script::shown` of a player-level card: what is left of its "N times each turn. (N left!)"
/// uses, counted in the turn data.
pub(super) fn shown_turn_left(c: &Ctx, sh: &Shown, uses: i32) -> i32 {
    (uses - c.turn_data(sh.side, sh.card)).max(0)
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // ------------------------------------------------------------------ lesser
        // Holy Mallet: SoC give your left- and right-most minions Divine Shield. Innkeeper's Stein:
        // the Tavern offers an extra minion of a higher Tier whenever it is Refreshed. At the top
        // Tier it is a minion of that Tier: Tier 6, or Tier 7 once the Tier 7 Tavern is open.
        (
            "BG30_MagicItem_423",
            Script {
                on_fill_shop: Some(|c, me| {
                    let top = if c.player_ref(me.side).is_some_and(|p| p.tier7) {
                        7
                    } else {
                        6
                    };
                    let t = (c.tier(me.side) + 1).min(top);
                    c.add_shop_minion(me.side, t, t, Races::NONE);
                }),
                ..E
            },
        ),
        // Cursed Crystal: after the Tavern is Refreshed, give its minions +3/+3 this turn.
        (
            "BG35_MagicItem_150",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Refreshed { side, .. } if *side == me.side) {
                        for t in c.tavern_minions(me.side) {
                            c.buff_temp(t, 3, 3);
                        }
                    }
                }),
                listens: ev::REFRESHED,
                ..E
            },
        ),
        // Herald Sticker: Start of Combat: trigger all friendly Deathrattles.
        (
            "BG32_MagicItem_306",
            Script {
                start_of_combat: Some(|c, me| {
                    let ids = c.friendly_where(me.side, |m| {
                        crate::script::minion_has(m, crate::script::Hook::Deathrattle)
                    });
                    for e in ids {
                        c.trigger_deathrattles(me.side, e);
                    }
                }),
                ..E
            },
        ),
        (
            "BG30_MagicItem_902",
            Script {
                start_of_combat: Some(|c, me| {
                    for e in [c.leftmost(me.side), c.rightmost(me.side)]
                        .into_iter()
                        .flatten()
                    {
                        c.give_kw(e, Kw::DIVINE_SHIELD);
                    }
                }),
                ..E
            },
        ),
        // Goblin Wallet: at the end of each turn, increase your maximum Gold by 1.
        (
            "BG30_MagicItem_847",
            Script {
                end_of_turn: Some(|c, me| c.add_max_gold(me.side, 1)),
                ..E
            },
        ),
        // Replica Cathedral: first spell each turn casts an extra time (recruit.rs).
        ("BG30_MagicItem_434", E),
        // Ancient Wishbone: your Hero Power triggers twice (recruit.rs). Only useful -- and only
        // offered -- with an active (non-passive) hero power; never to Drest'agath, whose Incubate
        // the client does not offer it with.
        (
            "BG30_MagicItem_804",
            Script {
                offer_ok: Some(|g, p| {
                    g.players[p]
                        .hero_power()
                        .and_then(script)
                        .is_some_and(|s| s.hero_power.is_some() && !s.has_flag(flag::PASSIVE))
                        && !g.players[p].has_power_id(crate::cardid!("BG36_HERO_000p"))
                }),
                ..E
            },
        ),
        // Defensive Shell: the first minion you play each turn gains Divine Shield.
        (
            "BG36_MagicItem_811",
            Script {
                on_event: Some(defensive_shell),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Worn Treasure Map: in two turns, gain 10 Gold.
        (
            "BG32_MagicItem_428",
            Script {
                on_attach: Some(|c, me| {
                    schedule(
                        c,
                        me.side,
                        Scheduled {
                            turns_left: 2,
                            script: me.card,
                            target: 0,
                            a: 10,
                            b: 0,
                            if_won: false,
                        },
                    )
                }),
                scheduled: Some(|c, me, s| c.gain_gold(me.side, s.a)),
                ..E
            },
        ),
        // Bob-blehead: gain 2 Gold; the Tavern no longer offers Tier 1-2 cards (recruit.rs).
        (
            "BG30_MagicItem_998",
            Script {
                on_attach: Some(|c, me| c.gain_gold(me.side, 2)),
                ..E
            },
        ),
        // Magician's Top Hat: get two minions each from Tiers 1, 2 and 3.
        (
            "BG35_MagicItem_815",
            Script {
                on_attach: Some(|c, me| {
                    for t in 1..=3u8 {
                        for _ in 0..2 {
                            if let Some(card) = c.random_minion(|d| d.tier == t) {
                                c.give_card(me.side, card);
                            }
                        }
                    }
                }),
                ..E
            },
        ),
        // Explorer's Binoculars: get three random Tier 4 minions.
        (
            "BG32_MagicItem_858",
            Script {
                on_attach: Some(|c, me| {
                    for _ in 0..3 {
                        if let Some(card) = c.random_minion(|d| d.tier == 4) {
                            c.give_card(me.side, card);
                        }
                    }
                }),
                ..E
            },
        ),
        // Glowing Gauntlet: Tavern minions have +3/+3; the Tavern always has 7 cards.
        (
            "BG30_MagicItem_841",
            Script {
                on_attach: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.stats.tavern_atk = p.stats.tavern_atk.saturating_add(3);
                        p.stats.tavern_hp = p.stats.tavern_hp.saturating_add(3);
                    }
                    c.buff_tavern(me.side, 3, 3, Races::NONE);
                }),
                ..E
            },
        ),
        // Stuffed Coin Purse: when you reach Tier 6, gain 12 Gold.
        (
            "BG35_MagicItem_814",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Upgraded { side, tier: 6 } = e
                        && *side == me.side
                    {
                        c.gain_gold(me.side, 12);
                    }
                }),
                listens: ev::UPGRADED,
                // Pointless once the player already reached Tier 6.
                offer_ok: Some(|g, p| g.players[p].tier < 6),
                ..E
            },
        ),
        // ------------------------------------------------------------------ greater
        // Reinforced Shield: whenever you summon a minion, give it Divine Shield (5/combat).
        (
            "BG30_MagicItem_886",
            Script {
                on_event: Some(reinforced_shield),
                listens: ev::SUMMONED,
                ..E
            },
        ),
        // Ironforge Anvil: SoC triple the stats of your minions with no type.
        (
            "BG30_MagicItem_403",
            Script {
                start_of_combat: Some(|c, me| {
                    for e in c.friendly_where(me.side, |m| m.races.is_none()) {
                        let (a, h) = c.minion(e).map(|m| (m.atk, m.hp)).unwrap_or((0, 0));
                        c.buff(e, a.saturating_mul(2), h.saturating_mul(2));
                    }
                }),
                ..E
            },
        ),
        // Bob's Tip Jar: gain 4 Gold; increase your maximum Gold by 4.
        (
            "BG30_MagicItem_996",
            Script {
                on_attach: Some(|c, me| {
                    c.gain_gold(me.side, 4);
                    c.add_max_gold(me.side, 4);
                }),
                ..E
            },
        ),
        // Ghastly Sticker: end of turn effects trigger an extra time (engine.rs).
        ("BG32_MagicItem_367", E),
        // Slamma Sticker: after you summon a Beast in combat, double its Attack.
        (
            "BG30_MagicItem_540",
            Script {
                on_event: Some(slamma_sticker),
                listens: ev::SUMMONED,
                ..E
            },
        ),
        // Orb of the Unknown (greater): get a random Greater Trinket and 4 Gold.
        (
            "BG35_MagicItem_816t",
            Script {
                on_attach: Some(|c, me| {
                    c.gain_gold(me.side, 4);
                    let Some(p) = c.owner[me.side] else { return };
                    let cands: Vec<CardId> = c.board_home(me.side, |c| {
                        c.g.trinket_universe
                            .iter()
                            .copied()
                            .filter(|t| {
                                db().get(*t).kind == crate::cards::CardKind::TrinketGreater
                                    && c.g.trinket_eligible(p, *t)
                            })
                            .collect()
                    });
                    if let Some(&t) = c.g.rng.pick(&cands) {
                        grant_trinket(c, me.side, t);
                    }
                }),
                ..E
            },
        ),
        // Innkeeper's Hearth: Discover two Tier 6 minions; set their stats to 30/30. The options
        // come from the pool like any minion Discover (by copies left, none of a card that has
        // none).
        (
            "BG32_MagicItem_362t",
            Script {
                on_attach: Some(|c, me| {
                    for _ in 0..2 {
                        let mut options = c.minion_options(me.side, 3, |d| d.tier == 6);
                        for o in options.iter_mut() {
                            if let Item::Minion(m) = o {
                                m.set_base_stats(30, 30);
                            }
                        }
                        c.offer_discover(me.side, me.card, options);
                    }
                }),
                ..E
            },
        ),
    ]
}

fn defensive_shell(c: &mut Ctx, me: Me, e: &Event) {
    let Event::Played { side, eid, .. } = e else {
        return;
    };
    if *side == me.side
        && c.player_ref(me.side)
            .is_some_and(|p| p.stats.minions_played_turn == 1)
    {
        c.give_kw(*eid, Kw::DIVINE_SHIELD);
    }
}

fn reinforced_shield(c: &mut Ctx, me: Me, e: &Event) {
    let Event::Summoned { side, eid } = e else {
        return;
    };
    if *side != me.side || !c.combat || c.combat_data(me.side, me.card) >= 5 {
        return;
    }
    // A minion that already has Divine Shield doesn't use a charge.
    if c.minion(*eid).is_some_and(|m| m.has(Kw::DIVINE_SHIELD)) {
        return;
    }
    c.add_combat_data(me.side, me.card, 1);
    c.give_kw(*eid, Kw::DIVINE_SHIELD);
}

fn slamma_sticker(c: &mut Ctx, me: Me, e: &Event) {
    let Event::Summoned { side, eid } = e else {
        return;
    };
    if *side != me.side || !c.combat {
        return;
    }
    if let Some(m) = c.minion(*eid)
        && m.is(Races::BEAST)
    {
        let a = m.atk;
        c.buff(*eid, a, 0);
    }
}
