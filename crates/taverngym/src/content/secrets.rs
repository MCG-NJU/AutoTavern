//! Secrets: The Great Akazamzarak's Secret zone. Secrets are one-shot triggers that are used up
//! when they fire (`Ctx::use_secret`); Ice Block is checked in `hero_damage_ex`. Also E.T.C.'s
//! Buddy Discover.

use super::*;

/// Secrets Akazamzarak offers in 36.6 (choose 1 of these 4): Venomstrike Trap, Autodefense Matrix,
/// Redemption, Pack Tactics. The other Secrets stay scripted for other sources.
pub const SECRETS: [&str; 4] = [
    "TB_Bacon_Secrets_01",
    "TB_Bacon_Secrets_07",
    "TB_Bacon_Secrets_10",
    "TB_Bacon_Secrets_15",
];

/// Better Secrets (Street Magician).
const BETTER: [&str; 4] = [
    "TB_Bacon_Secrets_01b",
    "TB_Bacon_Secrets_07b",
    "TB_Bacon_Secrets_10b",
    "TB_Bacon_Secrets_15b",
];

/// A Secret and its Better version (`..._07` / `..._07b`) are the same Secret.
/// Akazamzarak has a Secret left to offer (one not in play).
pub fn secret_left(pl: &crate::state::Player) -> bool {
    SECRETS
        .iter()
        .any(|s| !pl.secrets.iter().any(|o| same_secret(*o, cid(s))))
}

pub fn same_secret(a: CardId, b: CardId) -> bool {
    let base = |x: CardId| {
        let id = db().get(x).id.as_str();
        id.strip_suffix('b').unwrap_or(id).to_string()
    };
    base(a) == base(b)
}

/// The friendly minion being attacked, if `e` is an enemy attack on `me`'s side.
fn attacked(c: &Ctx, me: Me, e: &Event) -> Option<EntityId> {
    match e {
        Event::Attacks { side, defender, .. }
            if *side != me.side && c.friendly(me.side).contains(defender) =>
        {
            Some(*defender)
        }
        _ => None,
    }
}

fn friendly_died(me: Me, e: &Event) -> Option<(EntityId, CardId)> {
    match e {
        Event::Died {
            side, eid, card, ..
        } if *side == me.side => Some((*eid, *card)),
        _ => None,
    }
}

fn summon_right_of(c: &mut Ctx, side: Side, of: EntityId, m: Minion) {
    let pos = if c.on_board(of) {
        c.pos_right_of(of)
    } else {
        c.boards[side].len()
    };
    c.summon(side, pos, m);
}

fn copy_of(c: &mut Ctx, t: EntityId) -> Option<Minion> {
    let mut m = c.minion(t)?.clone();
    m.eid = c.g.alloc_eid();
    m.combat_only = true;
    m.pool_copies = 0;
    m.attacked = false;
    Some(m)
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // The Great Akazamzarak: choose one of the 4 Secrets (those not in play) and put it
        // into the battlefield (`after_discover`).
        (
            "TB_BaconShop_HP_020",
            Script {
                hero_power: Some(|c, me, _| {
                    let owned: Vec<CardId> = c
                        .player_ref(me.side)
                        .map(|p| p.secrets.clone())
                        .unwrap_or_default();
                    // Street Magician: choose from Better Secrets (golden: twice).
                    let magic = buddy_power(c, me.side, "TB_BaconShop_HERO_21_Buddy");
                    let pool: Vec<&str> = if magic > 0 {
                        BETTER.to_vec()
                    } else {
                        SECRETS.to_vec()
                    };
                    // A Secret and its Better version count as the same Secret; the golden
                    // second pick drops the first choice (`after_discover`).
                    for _ in 0..magic.max(1) {
                        let cands: Vec<CardId> = pool
                            .iter()
                            .map(|s| cid(s))
                            .filter(|s| !owned.iter().any(|o| same_secret(*o, *s)))
                            .collect();
                        let options = cands.into_iter().map(Item::spell).collect();
                        c.offer_discover(me.side, me.card, options);
                    }
                }),
                ..E
            },
        ),
        // E.T.C., Band Manager: Discover a Buddy (unlocks at Tier 2).
        (
            "BG25_HERO_105p",
            Script {
                hero_power: Some(|c, me, _| quests::discover_buddy(c, me.side, me.card)),
                ..E
            },
        ),
        // Venomstrike Trap (Better: also Reborn): summon a 2/3 Poisonous Cobra.
        ("TB_Bacon_Secrets_01", secret_on_attack()),
        ("TB_Bacon_Secrets_01b", secret_on_attack()),
        // Snake Trap: summon three 1/1 Snakes.
        ("TB_Bacon_Secrets_02", secret_on_attack()),
        // Splitting Image: summon a copy of the attacked minion.
        ("TB_Bacon_Secrets_04", secret_on_attack()),
        // Autodefense Matrix (Better: shield takes 2 hits).
        ("TB_Bacon_Secrets_07", secret_on_attack()),
        ("TB_Bacon_Secrets_07b", secret_on_attack()),
        // Pack Tactics: a 3/3 copy (Better: a full copy).
        ("TB_Bacon_Secrets_15", secret_on_attack()),
        ("TB_Bacon_Secrets_15b", secret_on_attack()),
        // Effigy: when a friendly minion dies, summon a random minion of the same Tier.
        ("TB_Bacon_Secrets_05", secret_on_death()),
        // Avenge: when a friendly minion dies, give a random friendly minion +3/+2.
        ("TB_Bacon_Secrets_08", secret_on_death()),
        // Redemption: return it to life with 1 Health (Better: full stats).
        ("TB_Bacon_Secrets_10", secret_on_death()),
        ("TB_Bacon_Secrets_10b", secret_on_death()),
        // Hand of Salvation: when your second minion dies (this combat), return it to life.
        ("TB_Bacon_Secrets_11", secret_on_death()),
        // Ice Block: `hero_damage_ex`. Cast (Safety Patch), it goes into play unless it is
        // there already.
        (
            "TB_Bacon_Secrets_12",
            Script {
                spell: Some(|c, me, _| {
                    if let Some(p) = c.player(me.side)
                        && !p.secrets.iter().any(|s| same_secret(*s, me.card))
                        && p.secrets.len() < crate::state::MAX_SECRETS
                    {
                        p.secrets.push(me.card);
                    }
                }),
                ..E
            },
        ),
        // Competitive Spirit: when your turn starts, give your minions +1/+1.
        (
            "TB_Bacon_Secrets_13",
            Script {
                start_of_turn: Some(|c, me| {
                    if c.use_secret(me.side, me.card) {
                        for t in c.friendly(me.side) {
                            c.buff(t, 1, 1);
                        }
                    }
                }),
                ..E
            },
        ),
        // Reckoning: after an enemy minion deals 3 or more damage, destroy it.
        (
            "TB_Bacon_Secrets_14",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Damaged {
                        side,
                        amount,
                        source,
                        ..
                    } = e
                    else {
                        return;
                    };
                    if *side != me.side
                        || *amount < 3
                        || !c.boards[1 - me.side].iter().any(|m| m.eid == *source)
                    {
                        return;
                    }
                    if c.use_secret(me.side, me.card) {
                        c.destroy(*source);
                    }
                }),
                listens: ev::DAMAGED,
                ..E
            },
        ),
    ]
}

fn secret_on_attack() -> Script {
    Script {
        on_event: Some(|c, me, e| {
            let Some(t) = attacked(c, me, e) else { return };
            if !c.use_secret(me.side, me.card) {
                return;
            }
            match db().get(me.card).id.as_str() {
                "TB_Bacon_Secrets_01" | "TB_Bacon_Secrets_01b" => {
                    let mut m = c.new_minion(me.side, cid("BG_EX1_170"));
                    if db().get(me.card).id.ends_with('b') {
                        m.give(Kw::REBORN);
                    }
                    // The Cobra joins at the far right.
                    let _ = t;
                    let pos = c.boards[me.side].len();
                    c.summon(me.side, pos, m);
                }
                "TB_Bacon_Secrets_02" => {
                    for _ in 0..3 {
                        let m = c.new_minion(me.side, cid("BG_EX1_554t"));
                        summon_right_of(c, me.side, t, m);
                    }
                }
                "TB_Bacon_Secrets_04" | "TB_Bacon_Secrets_15b" => {
                    if let Some(m) = copy_of(c, t) {
                        summon_right_of(c, me.side, t, m);
                    }
                }
                "TB_Bacon_Secrets_15" => {
                    if let Some(mut m) = copy_of(c, t) {
                        m.set_base_stats(3, 3);
                        summon_right_of(c, me.side, t, m);
                    }
                }
                "TB_Bacon_Secrets_07" | "TB_Bacon_Secrets_07b" => {
                    c.give_kw(t, Kw::DIVINE_SHIELD);
                    if db().get(me.card).id.ends_with('b')
                        && let Some(m) = c.minion_mut(t)
                    {
                        m.shield_max = m.shield_max.max(2);
                        m.shield_hits = m.shield_hits.max(m.shield_max);
                    }
                }
                _ => {}
            }
        }),
        listens: ev::ATTACKS,
        ..E
    }
}

fn secret_on_death() -> Script {
    Script {
        on_event: Some(|c, me, e| {
            let Some((eid, card)) = friendly_died(me, e) else {
                return;
            };
            let id = db().get(me.card).id.as_str();
            if id == "TB_Bacon_Secrets_11" && c.cs[me.side].dead.len() != 2 {
                return;
            }
            if !c.use_secret(me.side, me.card) {
                return;
            }
            let rec = c.dead_record(me.side, eid).map(|d| (d.atk, d.max_hp));
            // Redemption revives in the dead minion's slot.
            let slot = c
                .dead_record(me.side, eid)
                .map_or(usize::MAX, |d| d.pos)
                .min(c.boards[me.side].len());
            match id {
                "TB_Bacon_Secrets_05" => {
                    let tier = db().get(card).tier;
                    if let Some(x) = c.random_minion(|d| d.tier == tier) {
                        let m = c.new_minion(me.side, x);
                        let pos = c.boards[me.side].len();
                        c.summon(me.side, pos, m);
                    }
                }
                "TB_Bacon_Secrets_08" => {
                    if let Some(t) = c.random_friendly(me.side, |_| true) {
                        c.buff(t, 3, 2);
                    }
                }
                "TB_Bacon_Secrets_10" | "TB_Bacon_Secrets_11" => {
                    let mut m = c.new_minion(me.side, card);
                    if id == "TB_Bacon_Secrets_10" {
                        // 1 Health under the continuous Health bonuses.
                        m.hp = 1i32.saturating_add(m.pbonus_hp.max(0)).min(m.max_hp);
                    }
                    m.combat_only = true;
                    c.summon(me.side, slot, m);
                }
                "TB_Bacon_Secrets_10b" => {
                    let mut m = c.new_minion(me.side, card);
                    if let Some((a, h)) = rec {
                        m.set_stats(a, h);
                    }
                    m.combat_only = true;
                    c.summon(me.side, slot, m);
                }
                _ => {}
            }
        }),
        listens: ev::DIED,
        ..E
    }
}
