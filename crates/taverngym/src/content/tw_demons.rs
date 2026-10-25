//! Timewarp cards (demons): the Timewarped Tavern's own pool (docs/TIMEWARP.md).

use super::*;

const IMP: &str = "BG_BRM_006t"; // 1/1 Imp
const STAFF: &str = "BG28_886"; // Staff of Enrichment
const MYTHRAX: &str = "BG34_Giant_684";

fn hero_hit(me: Me, e: &Event) -> bool {
    matches!(e, Event::HeroDamaged { side, amount, .. } if *side == me.side && *amount > 0)
}

/// "Give your Demons +a/+h."
fn buff_demons(c: &mut Ctx, side: Side, a: i32, h: i32) {
    for t in c.friendly_where(side, |m| m.is(Races::DEMON)) {
        c.buff(t, a, h);
    }
}

/// "Get 1 (2) random Tavern spells" (up to the player's Tier).
fn get_spells(c: &mut Ctx, me: Me) {
    let tier = c.tier(me.side);
    for _ in 0..me.g() {
        if let Some(s) = c.random_spell(|d| d.tier <= tier) {
            c.give_card(me.side, s);
        }
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Timewarped Rewinder: after your hero takes damage, rewind it and give your Demons
        // +1 (+2) Health.
        (
            "BG34_Giant_300",
            Script {
                on_event: Some(|c, me, e| {
                    if hero_hit(me, e) {
                        c.rewind_hero_damage(me.side);
                        buff_demons(c, me.side, 0, me.g());
                    }
                }),
                listens: ev::HERO_DAMAGED,
                ..E
            },
        ),
        // Timewarped Kil'jaeden: the Tavern offers two extra Demons with +7/+7 (+14/+14) whenever
        // it is Refreshed. Upgrades each turn: by the base amount (counter = upgrades), while on
        // the board only. The turn-start refill comes before the minions' start of turn, so it
        // still uses the previous value.
        (
            "BG34_Giant_313",
            Script {
                on_fill_shop: Some(|c, me| {
                    let n = c.minion(me.eid).map_or(0, |m| m.counter);
                    let v = 7i32.saturating_mul(me.g().saturating_add(n));
                    let tier = c.tier(me.side);
                    for _ in 0..2 {
                        if let Some(sid) = c.add_shop_minion(me.side, 1, tier, Races::DEMON) {
                            c.buff(sid, v, v);
                        }
                    }
                }),
                start_of_turn: Some(|c, me| {
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = m.counter.saturating_add(me.g());
                    }
                }),
                // "The Tavern offers two extra Demons with +7/+7 whenever it is Refreshed.
                // (Upgrades each turn!)"
                shown: Some(|_, sh| {
                    let v = 7i32.saturating_mul(sh.g().saturating_add(sh.counter()));
                    vec![v, v]
                }),
                flags: flag::IMPROVES,
                ..E
            },
        ),
        // Timewarped Shadowdancer: Taunt. End of turn: cast Staff of Enrichment (twice).
        (
            "BG34_Giant_360",
            Script {
                end_of_turn: Some(|c, me| {
                    for _ in 0..me.g() {
                        c.you_cast(me.side, cid(STAFF), 0, 0, 0);
                    }
                }),
                ..E
            },
        ),
        // Timewarped Kil'rek: Taunt. Deathrattle: get 1 (2) random Demons.
        (
            "BG34_Giant_584",
            Script {
                deathrattle: Some(|c, me| {
                    let tier = c.tier(me.side);
                    for _ in 0..me.g() {
                        if let Some(d) =
                            c.random_minion(|d| d.races.intersects(Races::DEMON) && d.tier <= tier)
                        {
                            c.give_card(me.side, d);
                        }
                    }
                }),
                ..E
            },
        ),
        // Timewarped Archimonde: after your hero takes damage, rewind it and reduce the Cost
        // of your next Tavern spell by (1) ((2)).
        (
            "BG34_Giant_596",
            Script {
                on_event: Some(|c, me, e| {
                    if hero_hit(me, e) {
                        c.rewind_hero_damage(me.side);
                        if let Some(p) = c.player(me.side) {
                            p.stats.spell_discount_next =
                                p.stats.spell_discount_next.saturating_add(me.g());
                        }
                    }
                }),
                listens: ev::HERO_DAMAGED,
                ..E
            },
        ),
        // Timewarped Icky Imp: Deathrattle: summon 2 (4) Imps with this minion's maximum
        // stats (its Attack and maximum Health when it died; plain Imps for the golden too).
        (
            "BG34_Giant_674",
            Script {
                deathrattle: Some(|c, me| {
                    let stats = match c.minion(me.eid) {
                        Some(m) => Some((m.atk, m.max_hp)),
                        None => c.dead_record(me.side, me.eid).map(|d| (d.atk, d.max_hp)),
                    };
                    let Some((a, h)) = stats else { return };
                    for _ in 0..2 * me.g() {
                        let mut m = c.new_minion(me.side, cid(IMP));
                        m.set_stats(a, h);
                        c.summon(me.side, me.pos, m);
                    }
                }),
                ..E
            },
        ),
        // Timewarped Chimera: whenever this takes damage, give a friendly minion of each type +2/+1
        // permanently (golden: twice -- one pick, +4/+2).
        (
            "BG34_Giant_679",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Damaged { eid, amount, .. } = e else {
                        return;
                    };
                    if *eid != me.eid || *amount <= 0 {
                        return;
                    }
                    for (t, _) in one_minion_per_type(c, me.side, |_| true) {
                        c.buff_perm(me.side, t, 2 * me.g(), me.g());
                    }
                }),
                listens: ev::DAMAGED,
                ..E
            },
        ),
        // Timewarped Mythrax: Start of Combat: gain the stats (twice) of 3 friendly minions of
        // different types, except Timewarped Mythraxes. The 3 are random among a one-per-type pick;
        // with fewer types it gains what there is.
        (
            "BG34_Giant_684",
            Script {
                start_of_combat: Some(|c, me| {
                    let myth = cid(MYTHRAX);
                    let picks = one_minion_per_type(c, me.side, |m| {
                        m.eid != me.eid && db().normal_of(m.card) != myth
                    });
                    let (mut a, mut h) = (0, 0);
                    for (t, _) in picks.into_iter().take(3) {
                        if let Some(m) = c.minion(t) {
                            a += m.atk;
                            h += m.hp;
                        }
                    }
                    c.buff(me.eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                }),
                ..E
            },
        ),
        // Timewarped Overfiend: after you buy a card, give your Demons +4/+4 (+8/+8).
        (
            "BG34_PreMadeChamp_011",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Bought { side, .. } if *side == me.side) {
                        buff_demons(c, me.side, 4 * me.g(), 4 * me.g());
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Timewarped Imp-filtrator: after you spend 8 Gold, give minions in the Tavern +3/+3
        // (+6/+6) this game (counter = Gold spent toward the next trigger).
        (
            "BG34_PreMadeChamp_013",
            Script {
                on_event: Some(|c, me, e| {
                    gold_counter(c, me, e, 8, |c, me| {
                        let v = 3 * me.g();
                        if let Some(p) = c.player(me.side) {
                            p.stats.tavern_atk = p.stats.tavern_atk.saturating_add(v);
                            p.stats.tavern_hp = p.stats.tavern_hp.saturating_add(v);
                        }
                        c.buff_tavern(me.side, v, v, Races::NONE);
                    });
                }),
                listens: ev::GOLD_SPENT,
                // "After you spend 8 Gold, give minions in the Tavern +3/+3 this game.
                // (8 Gold left!)"
                shown: Some(|_, sh| vec![8, 3 * sh.g(), 3 * sh.g(), (8 - sh.counter()).max(0)]),
                ..E
            },
        ),
        // Timewarped Calligrapher: Battlecry, Deathrattle and Rally: get 1 (2) random Tavern
        // spells.
        (
            "BG34_PreMadeChamp_091",
            Script {
                battlecry: Some(|c, me, _| get_spells(c, me)),
                deathrattle: Some(get_spells),
                rally: Some(|c, me, _| get_spells(c, me)),
                ..E
            },
        ),
    ]
}
