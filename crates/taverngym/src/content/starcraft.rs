//! StarCraft heroes: Jim Raynor (Battlecruiser + Upgrades), Artanis (Protoss), Kerrigan (Zerg:
//! Larva and morphing units).

use super::*;

pub const BATTLECRUISER: &str = "BG31_HERO_801pt";
const PROTOSS: [&str; 5] = [
    "BG31_HERO_802pt",
    "BG31_HERO_802pt1",
    "BG31_HERO_802pt4",
    "BG31_HERO_802pt5",
    "BG31_HERO_802pt7",
];
/// Zerg units and their Zerg tier (1-3).
const ZERG: [(&str, u8); 9] = [
    ("BG31_HERO_811t2", 1),
    ("BG31_HERO_811t3", 1),
    ("BG31_HERO_811t4", 1),
    ("BG31_HERO_811t5", 2),
    ("BG31_HERO_811t6", 2),
    ("BG31_HERO_811t7", 2),
    ("BG31_HERO_811t8", 3),
    ("BG31_HERO_811t9", 3),
    ("BG31_HERO_811t10", 3),
];
const LARVA: &str = "BG31_HERO_811t";

/// Battlecruiser Upgrade values by family and variant (1-based tier index).
fn upgrade_value(id: &str) -> i32 {
    let fam = id.trim_start_matches("BG31_HERO_801pt");
    let letter = fam.chars().next().unwrap_or(' ');
    let k: usize = fam[1..].parse().unwrap_or(1);
    let table: &[i32] = match letter {
        'a' | 'b' => &[6, 8, 11, 15, 20, 28, 40],
        'c' => &[3, 6, 9, 18, 24, 30, 40],
        'd' => &[2, 3, 4, 5, 6, 8, 10],
        'e' => &[1, 2, 4, 7, 10, 15, 25],
        _ => &[0],
    };
    table.get(k - 1).copied().unwrap_or(0)
}

/// Your Battlecruiser (board first, then hand).
fn battlecruiser(c: &Ctx, side: Side) -> Option<EntityId> {
    let bc = cid(BATTLECRUISER);
    if let Some(m) = c.boards[side].iter().find(|m| db().normal_of(m.card) == bc) {
        return Some(m.eid);
    }
    c.player_ref(side)?
        .hand
        .iter()
        .find(|h| {
            h.item
                .as_minion()
                .is_some_and(|m| db().normal_of(m.card) == bc)
        })
        .map(|h| h.hid)
}

fn upgrade_script(family: Option<char>) -> Script {
    // Only the family's own hook is registered: a hook that merely returns early would still
    // count as a Deathrattle / Rally on the Battlecruiser it is attached to.
    let mut s = Script {
        spell: Some(|c, me, _| {
            let Some(bc) = battlecruiser(c, me.side) else {
                return;
            };
            let id = db().get(me.card).id.clone();
            let v = upgrade_value(&id);
            match id.trim_start_matches("BG31_HERO_801pt").chars().next() {
                Some('a') => c.buff(bc, v, 0),
                Some('b') => c.buff(bc, 0, v),
                Some('j') => {
                    c.give_kw(bc, Kw::REBORN);
                    if let Some(m) = c.find_any_mut(bc) {
                        m.attached.push(me.card);
                    }
                }
                _ => {
                    if let Some(m) = c.find_any_mut(bc) {
                        m.attached.push(me.card);
                    }
                }
            }
        }),
        spell_target: TargetSpec::None,
        ..E
    };
    match family {
        Some('c') => {
            s.start_of_combat = Some(|c, me| {
                let id = db().get(me.card).id.clone();
                if !id.starts_with("BG31_HERO_801ptc") {
                    return;
                }
                let twice = c.minion(me.eid).is_some_and(|m| {
                    m.attached
                        .iter()
                        .any(|a| db().get(*a).id.starts_with("BG31_HERO_801pti"))
                });
                for _ in 0..if twice { 2 } else { 1 } {
                    let keyed: Vec<(EntityId, i64)> = c.boards[1 - me.side]
                        .iter()
                        .filter(|m| m.alive())
                        .map(|m| (m.eid, m.hp as i64))
                        .collect();
                    let t = c.pick_max(keyed);
                    if let Some(t) = t {
                        c.damage(me.eid, t, upgrade_value(&id));
                        c.resolve_deaths();
                    }
                }
            });
        }
        Some('d') => {
            s.rally = Some(|c, me, _| {
                let id = db().get(me.card).id.clone();
                if id.starts_with("BG31_HERO_801ptd") {
                    for t in c.friendly_where(me.side, |m| m.eid != me.eid) {
                        c.buff(t, upgrade_value(&id), 0);
                    }
                }
            });
        }
        Some('e') => {
            s.deathrattle = Some(|c, me| {
                let id = db().get(me.card).id.clone();
                if id.starts_with("BG31_HERO_801pte")
                    && let Some(t) = c.leftmost(me.side)
                {
                    let v = upgrade_value(&id);
                    c.buff(t, v, v);
                }
            });
        }
        Some('h') => {
            s.end_of_turn = Some(|c, me| {
                if db().get(me.card).id.starts_with("BG31_HERO_801pth") {
                    let tier = c.tier(me.side);
                    if let Some(m) = c.random_minion(|d| {
                        d.kw.has(Kw::MAGNETIC) && d.races.intersects(Races::MECH) && d.tier <= tier
                    }) {
                        c.give_card(me.side, m);
                    }
                }
            });
        }
        _ => {}
    }
    s
}

/// Upgrade families that can be bought only once per game: Advanced Construction, Fortified Bunker,
/// Missile Pod, Ultra-Capacitor (official 31.6 notes). Purchase history belongs to the player, not
/// the current Hero Power.
const ONCE_FAMILIES: [char; 4] = ['f', 'h', 'i', 'j'];

fn family(id: &str) -> Option<char> {
    id.strip_prefix("BG31_HERO_801pt")?.chars().next()
}

/// Single-purchase families stay unavailable after Raynor's power is replaced.
pub fn upgrade_available(c: &Ctx, side: Side, card: CardId) -> bool {
    let Some(f) = family(&db().get(card).id) else {
        return true;
    };
    let Some(bit) = ONCE_FAMILIES.iter().position(|x| *x == f) else {
        return true;
    };
    // Also accept memory from snapshots saved before history moved to player card_data.
    let legacy_bought = c
        .player_ref(side)
        .and_then(|p| p.power_of(cid("BG31_HERO_801p")))
        .is_some_and(|h| h.memory.iter().any(|b| family(&db().get(*b).id) == Some(f)));
    c.card_data(side, cid(BATTLECRUISER)) & (1 << bit) == 0 && !legacy_bought
}

/// Record every real Upgrade purchase, regardless of the current Hero Power.
/// Called by the shared Bought path before listeners see the purchase.
pub fn note_upgrade_bought(c: &mut Ctx, side: Side, card: CardId, price: i32) {
    let Some(f) = family(&db().get(card).id) else {
        return;
    };
    if db().get(card).kind != crate::cards::CardKind::TavernSpell {
        return;
    }
    if let Some(bit) = ONCE_FAMILIES.iter().position(|x| *x == f) {
        let history = c.card_data(side, cid(BATTLECRUISER));
        c.set_card_data(side, cid(BATTLECRUISER), history | (1 << bit));
    }
    if price == 0
        && f != 'f'
        && battlecruiser(c, side).is_some_and(|eid| {
            c.find_any(eid).is_some_and(|m| {
                m.attached
                    .iter()
                    .any(|a| family(&db().get(*a).id) == Some('f'))
            })
        })
    {
        c.set_card_data(side, cid("BG31_HERO_801ptf"), c.g.turn as i32);
    }
}

/// A random Battlecruiser Upgrade: the variant of exactly your Tavern Tier of a random family that
/// has one.
pub fn random_upgrade(c: &mut Ctx, side: Side) -> Option<CardId> {
    let tier = c.tier(side);
    let ups: Vec<&crate::cards::CardDef> = db()
        .iter()
        .filter(|d| {
            d.id.starts_with("BG31_HERO_801pt")
                && d.kind == crate::cards::CardKind::TavernSpell
                && d.tier == tier
                && upgrade_available(c, side, d.cid)
        })
        .collect();
    let mut families: Vec<char> = ups
        .iter()
        .filter_map(|d| d.id.trim_start_matches("BG31_HERO_801pt").chars().next())
        .collect();
    families.sort();
    families.dedup();
    let &f = c.g.rng.pick(&families)?;
    ups.into_iter()
        .filter(|d| d.id.trim_start_matches("BG31_HERO_801pt").starts_with(f))
        .max_by_key(|d| d.tier)
        .map(|d| d.cid)
}

fn zerg_level(c: &Ctx, side: Side) -> u8 {
    let Some(p) = c.player_ref(side) else {
        return 0;
    };
    if p.has_power("BG31_HERO_811p3") {
        3
    } else if p.has_power("BG31_HERO_811p2") {
        2
    } else if p.has_power("BG31_HERO_811p") {
        1
    } else {
        0
    }
}

pub fn is_zerg(card: CardId) -> bool {
    let n = db().get(db().normal_of(card)).id.as_str();
    n == LARVA || ZERG.iter().any(|z| z.0 == n)
}

/// Morph a minion into Zerg unit `card` (golden if it is golden), keeping stats and
/// enchantments; base keywords follow the new form.
pub fn morph(c: &mut Ctx, eid: EntityId, card: CardId) {
    let Some(m) = c.find_any_mut(eid) else { return };
    // Keywords of a form are its database ones plus the script's innate ones (Viper's
    // Immune while attacking, Ultralisk's Cleave exist only as `innate`).
    let forms = |c: CardId| Kw(db().get(c).kw.0 | script(c).map_or(0, |s| s.innate.0));
    let old = forms(m.card);
    let target = if m.golden { db().golden_of(card) } else { card };
    m.kw = Kw((m.kw.0 & !old.0) | m.bonus_kw.0);
    m.card = target;
    m.tier = db().get(target).tier;
    let innate = forms(target);
    m.kw = m.kw.union(innate);
    m.temp_kw = Kw(m.temp_kw.0 & !innate.0);
    if m.has(Kw::DIVINE_SHIELD) && m.shield_hits == 0 {
        m.shield_hits = m.shield_max.max(1);
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    let mut v = vec![
        // Jim Raynor: start with a 2/2 Battlecruiser; each Refresh adds an Upgrade to the
        // Tavern. Advanced Construction: 1 Upgrade bought each turn is free (`spell_price`).
        (
            "BG31_HERO_801p",
            Script {
                on_attach: Some(|c, me| {
                    let m = c.new_minion(me.side, cid(BATTLECRUISER));
                    let pos = c.boards[me.side].len();
                    c.summon(me.side, pos, m);
                }),
                on_event: Some(|c, me, e| match e {
                    Event::Refreshed { side, .. } if *side == me.side => {
                        if let Some(u) = random_upgrade(c, me.side)
                            && let Some(p) = c.owner[me.side]
                        {
                            let sid = c.g.alloc_eid();
                            c.g.players[p]
                                .shop
                                .push(crate::state::ShopSlot::new(sid, Item::spell(u)));
                        }
                    }
                    _ => {}
                }),
                listens: ev::REFRESHED,
                flags: flag::PASSIVE,
                ..E
            },
        ),
        (BATTLECRUISER, E),
        // Artanis: at the start of the game, choose from 2 Protoss minions to get after you buy
        // 14 cards (memory; `after_discover`).
        (
            "BG31_HERO_802p",
            Script {
                on_attach: Some(|c, me| {
                    let picks = c.g.rng.sample_indices(PROTOSS.len(), 2);
                    let options = picks
                        .into_iter()
                        .map(|i| Item::Minion(c.new_minion(me.side, cid(PROTOSS[i]))))
                        .collect();
                    c.offer_discover(me.side, me.card, options);
                }),
                on_event: Some(|c, me, e| {
                    if !matches!(e, Event::Bought { side, .. } if *side == me.side) {
                        return;
                    }
                    let Some(h) = c.power_mut(me.side, me.card) else {
                        return;
                    };
                    if h.memory.is_empty() {
                        return;
                    }
                    h.counter += 1;
                    if h.counter >= 14 {
                        let got = std::mem::take(&mut h.memory);
                        for x in got {
                            c.give_card(me.side, x);
                        }
                    }
                }),
                listens: ev::BOUGHT,
                // "At the start of the game, choose from 2 Protoss minions to get after you buy
                // 14 cards. (14 left!)"
                shown: Some(|c, sh| vec![2, 14, super::heroes::shown_left(c, sh, 14)]),
                flags: flag::PASSIVE,
                ..E
            },
        ),
        // Colossus: Rally: deal 2 (4) damage to the target's neighbours, +1 (+2) per Gold spent
        // while it is on the board (counter).
        (
            "BG31_HERO_802pt",
            Script {
                rally: Some(|c, me, t| {
                    let k = 2i32
                        .saturating_mul(me.g())
                        .saturating_add(c.minion(me.eid).map_or(0, |m| m.counter));
                    for n in c.neighbors(t) {
                        c.damage(me.eid, n, k);
                    }
                }),
                on_event: Some(|c, me, e| {
                    if let Event::GoldSpent { side, amount } = e
                        && *side == me.side
                    {
                        c.counter_perm(me.side, me.eid, amount.saturating_mul(me.g()));
                    }
                }),
                listens: ev::GOLD_SPENT,
                // "Rally: Deal 2 damage to the target's neighbors. (Improves when you spend
                // Gold!)"
                shown: Some(|_, sh| vec![2i32.saturating_mul(sh.g()).saturating_add(sh.counter())]),
                flags: flag::IMPROVES,
                ..E
            },
        ),
        // Carrier: Avenge (4): summon a 7/7 (14/14) Interceptor, then improve permanently.
        (
            "BG31_HERO_802pt1",
            Script {
                avenge: Some((4, 4, |c, me| {
                    let k = 7i32.saturating_mul(
                        me.g()
                            .saturating_add(c.minion(me.eid).map_or(0, |m| m.counter)),
                    );
                    let mut m = c.new_minion(me.side, cid("BG31_HERO_802pt1t"));
                    m.set_stats(k, k);
                    let pos = c.pos_right_of(me.eid);
                    c.summon(me.side, pos, m);
                    c.counter_perm(me.side, me.eid, me.g());
                })),
                // "Avenge (4): Summon a 7/7 Interceptor. Then improve this permanently."
                shown: Some(|_, sh| {
                    let k = 7i32.saturating_mul(sh.g().saturating_add(sh.counter()));
                    vec![4, k, k]
                }),
                flags: flag::IMPROVES,
                ..E
            },
        ),
        // Immortal: Start of Combat: gain the stats (double) of adjacent minions.
        (
            "BG31_HERO_802pt4",
            Script {
                start_of_combat: Some(|c, me| {
                    let (mut a, mut h) = (0i32, 0i32);
                    for n in c.neighbors(me.eid) {
                        if let Some(m) = c.minion(n) {
                            a = a.saturating_add(m.atk);
                            h = h.saturating_add(m.hp);
                        }
                    }
                    c.buff(me.eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                }),
                ..E
            },
        ),
        // Void Ray: whenever a friendly minion attacks, it and this get +5 (+10) Attack
        // permanently.
        (
            "BG31_HERO_802pt5",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Attacks { side, attacker, .. } = e
                        && *side == me.side
                    {
                        c.buff_perm(me.side, *attacker, 5 * me.g(), 0);
                        if *attacker != me.eid {
                            c.buff_perm(me.side, me.eid, 5 * me.g(), 0);
                        }
                    }
                }),
                listens: ev::ATTACKS,
                ..E
            },
        ),
        // Mothership: Avenge (4): get 1 (2) random Protoss minions.
        (
            "BG31_HERO_802pt7",
            Script {
                avenge: Some((4, 4, |c, me| {
                    // The other Protoss minions, not another Mothership.
                    let others: Vec<&str> = PROTOSS
                        .iter()
                        .copied()
                        .filter(|p| *p != "BG31_HERO_802pt7")
                        .collect();
                    for _ in 0..me.g() {
                        if let Some(p) = c.g.rng.pick(&others) {
                            c.give_card(me.side, cid(p));
                        }
                    }
                })),
                ..E
            },
        ),
        // Kerrigan: Spawning Pool -> Evolution Chamber -> Ultralisk Cavern (each use unlocks
        // the next Zerg tier; costs (1) less each turn). Start with a 2/2 Larva.
        (
            "BG31_HERO_811p",
            Script {
                on_attach: Some(|c, me| {
                    let m = c.new_minion(me.side, cid(LARVA));
                    let pos = c.boards[me.side].len();
                    c.summon(me.side, pos, m);
                }),
                hero_power: Some(|c, me, _| next_pool(c, me, "BG31_HERO_811p2")),
                start_of_turn: Some(zerg_turn),
                ..E
            },
        ),
        (
            "BG31_HERO_811p2",
            Script {
                hero_power: Some(|c, me, _| next_pool(c, me, "BG31_HERO_811p3")),
                start_of_turn: Some(zerg_turn),
                ..E
            },
        ),
        (
            "BG31_HERO_811p3",
            Script {
                start_of_turn: Some(zerg_turn),
                flags: flag::PASSIVE,
                ..E
            },
        ),
        (LARVA, E),
        // Zergling: Start of Combat: summon 1 (2) copies of this.
        (
            "BG31_HERO_811t2",
            Script {
                start_of_combat: Some(|c, me| {
                    for _ in 0..me.g() {
                        let Some(mut m) = c.minion(me.eid).cloned() else {
                            return;
                        };
                        m.eid = c.g.alloc_eid();
                        m.combat_only = true;
                        m.pool_copies = 0;
                        let pos = c.pos_right_of(me.eid);
                        c.summon(me.side, pos, m);
                    }
                }),
                ..E
            },
        ),
        // Roach: end of turn, gain Health equal to your Tier (twice).
        (
            "BG31_HERO_811t3",
            Script {
                end_of_turn: Some(|c, me| {
                    let t = c.tier(me.side) as i32;
                    c.buff(me.eid, 0, t * me.g());
                }),
                ..E
            },
        ),
        // Hydralisk: Rally: gain Attack equal to your Tier (twice) permanently.
        (
            "BG31_HERO_811t4",
            Script {
                rally: Some(|c, me, _| {
                    let t = c.tier(me.side) as i32;
                    c.buff_perm(me.side, me.eid, t * me.g(), 0);
                }),
                ..E
            },
        ),
        // Baneling: Deathrattle: damage equal to its Attack to a random enemy minion (twice).
        (
            "BG31_HERO_811t5",
            Script {
                deathrattle: Some(|c, me| {
                    let a = c.deathrattle_stats(me.side, me.eid).map_or(0, |d| d.0);
                    for _ in 0..me.g() {
                        let foes: Vec<EntityId> = c.boards[1 - me.side]
                            .iter()
                            .filter(|m| m.alive())
                            .map(|m| m.eid)
                            .collect();
                        if let Some(&t) = c.g.rng.pick(&foes) {
                            c.damage(me.eid, t, a);
                        }
                    }
                }),
                ..E
            },
        ),
        // Mutalisk: after a friendly minion kills an enemy, gain +3 (+6) Attack permanently.
        (
            "BG31_HERO_811t6",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Killed { side, .. } = e
                        && *side == me.side
                    {
                        c.buff_perm(me.side, me.eid, 3 * me.g(), 0);
                    }
                }),
                listens: ev::KILLED,
                ..E
            },
        ),
        // Lurker: Avenge (1): gain +1/+1 (+2/+2) permanently.
        (
            "BG31_HERO_811t7",
            Script {
                avenge: Some((1, 1, |c, me| c.buff_perm(me.side, me.eid, me.g(), me.g()))),
                ..E
            },
        ),
        // Viper: Venomous, Immune while attacking (keywords).
        (
            "BG31_HERO_811t8",
            Script {
                innate: Kw(Kw::VENOMOUS.0 | Kw::IMMUNE_ATTACKING.0),
                ..E
            },
        ),
        // Infestor: whenever you play a card, give your minions +1/+1 (+2/+2).
        (
            "BG31_HERO_811t9",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::CardPlayed { side, .. } if *side == me.side) {
                        for t in c.friendly(me.side) {
                            c.buff(t, me.g(), me.g());
                        }
                    }
                }),
                listens: ev::CARD_PLAYED,
                ..E
            },
        ),
        // Ultralisk: also damages adjacent minions; Start of Combat: double (triple) its stats.
        (
            "BG31_HERO_811t10",
            Script {
                innate: Kw::CLEAVE,
                start_of_combat: Some(|c, me| {
                    if let Some((a, h)) = c.minion(me.eid).map(|m| (m.atk, m.hp)) {
                        c.buff(me.eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                    }
                }),
                ..E
            },
        ),
    ];
    for d in db().iter() {
        if d.id.starts_with("BG31_HERO_801pt") && d.kind == crate::cards::CardKind::TavernSpell {
            v.push((
                leak(&d.id),
                upgrade_script(d.id.trim_start_matches("BG31_HERO_801pt").chars().next()),
            ));
        }
    }
    v
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn next_pool(c: &mut Ctx, me: Me, next: &str) {
    let slot = c
        .player_ref(me.side)
        .and_then(|p| p.powers.iter().position(|h| h.card == me.card))
        .unwrap_or(0);
    c.set_hero_power(me.side, slot, cid(next));
}

/// Kerrigan's turn start: the unlock power gets cheaper; every Zerg on the board may morph
/// (one choice per Zerg; `after_discover` morphs the eid on top of `values`).
fn zerg_turn(c: &mut Ctx, me: Me) {
    let later = c.g.turn > 1;
    if let Some(h) = c.power_mut(me.side, me.card)
        && later
    {
        h.cost_mod -= 1;
    }
    let level = zerg_level(c, me.side).max(1);
    let zergs: Vec<(EntityId, bool)> = c.boards[me.side]
        .iter()
        .filter(|m| is_zerg(m.card) && m.counter != -1)
        .map(|m| (m.eid, m.golden))
        .collect();
    for (eid, golden) in zergs {
        // 3 options: one from your current Zerg Tier (official 31.6 note), two others from
        // everything unlocked (the count is an assumption).
        let unlocked: Vec<&(&str, u8)> = ZERG.iter().filter(|z| z.1 <= level).collect();
        let current: Vec<&(&str, u8)> = unlocked.iter().copied().filter(|z| z.1 == level).collect();
        let mut picks: Vec<&(&str, u8)> = c.g.rng.pick(&current).copied().into_iter().collect();
        let rest: Vec<&(&str, u8)> = unlocked
            .iter()
            .copied()
            .filter(|z| !picks.iter().any(|p| p.0 == z.0))
            .collect();
        for i in c.g.rng.sample_indices(rest.len(), 3 - picks.len()) {
            picks.push(rest[i]);
        }
        let options: Vec<Item> = picks
            .into_iter()
            .map(|z| {
                let card = if golden {
                    db().golden_of(cid(z.0))
                } else {
                    cid(z.0)
                };
                Item::Minion(c.new_minion(me.side, card))
            })
            .collect();
        if let Some(h) = c.power_mut(me.side, me.card) {
            h.values.push(eid as i32);
        }
        c.offer_discover(me.side, me.card, options);
    }
}
