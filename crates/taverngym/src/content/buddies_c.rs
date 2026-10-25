//! Buddies (part c).

use super::*;
use std::cell::Cell;

/// Darkmoon Prizes (bundle view `darkmoon_prizes_all_assets`) with their Prize Tier.
const PRIZES: [(&str, u8); 32] = [
    ("BG33_300", 4),
    ("BGS_Treasures_004", 1),
    ("BGS_Treasures_006", 2),
    ("BGS_Treasures_009", 2),
    ("BGS_Treasures_010", 2),
    ("BGS_Treasures_011", 3),
    ("BGS_Treasures_012", 2),
    ("BGS_Treasures_013", 1),
    ("BGS_Treasures_014", 2),
    ("BGS_Treasures_015", 3),
    ("BGS_Treasures_016", 4),
    ("BGS_Treasures_018", 2),
    ("BGS_Treasures_019", 3),
    ("BGS_Treasures_020", 3),
    ("BGS_Treasures_022", 4),
    ("BGS_Treasures_023", 4),
    ("BGS_Treasures_025", 4),
    ("BGS_Treasures_026", 2),
    ("BGS_Treasures_028", 4),
    ("BGS_Treasures_029", 1),
    ("BGS_Treasures_030", 2),
    ("BGS_Treasures_032", 4),
    ("BGS_Treasures_033", 1),
    ("BGS_Treasures_034", 3),
    ("BGS_Treasures_037", 3),
    ("BGS_Treasures_039", 3),
    ("BGS_Treasures_040", 1),
    ("BGS_Treasures_100", 1),
    ("BGS_Treasures_101", 2),
    ("BGS_Treasures_104", 3),
    ("BGS_Treasures_106", 4),
    ("BGS_Treasures_110", 1),
];

const TITANIC_GUARDIAN: &str = "TB_BaconShop_HERO_39_Buddy";
const SINESTRA: &str = "TB_BaconShop_HERO_52_Buddy";

thread_local! {
    /// Re-entrancy guard for stat-gain reactions (two Titanic Guardians would loop).
    static IN_GAIN: Cell<bool> = const { Cell::new(false) };
}

struct GainGuard;
impl Drop for GainGuard {
    fn drop(&mut self) {
        IN_GAIN.with(|f| f.set(false));
    }
}

/// Engine hook: a minion gained stats (`Ctx::buff`). Titanic Guardian and Sinestra react.
pub fn on_stat_gain(c: &mut Ctx, eid: EntityId, atk: i32, hp: i32) {
    stat_gain(c, eid, atk, hp, false);
}

/// Engine hook: a minion gained stats until next turn (`Ctx::buff_temp`). The two Buddies react as
/// to any gain and keep what they get; Makeshift Master's Deity does not (an assumption).
pub fn on_temp_stat_gain(c: &mut Ctx, eid: EntityId, atk: i32, hp: i32) {
    stat_gain(c, eid, atk, hp, true);
}

fn stat_gain(c: &mut Ctx, eid: EntityId, atk: i32, hp: i32, temp: bool) {
    if IN_GAIN.with(|f| f.get()) {
        return;
    }
    // The side whose board or hand holds `eid`; or whose Tavern does (Makeshift Master can mark a
    // Tavern minion, "Choose a minion"; the Buddies only watch your own minions).
    let in_hand =
        |s: Side| c.owner[s].is_some_and(|p| c.g.players[p].hand.iter().any(|h| h.hid == eid));
    let in_tavern = |s: Side| {
        !c.combat && c.owner[s].is_some_and(|p| c.g.players[p].shop.iter().any(|x| x.sid == eid))
    };
    let (side, tavern) = match c.locate(eid) {
        Some((s, _)) => (Some(s), false),
        None => match (0..2).find(|&s| in_hand(s)) {
            Some(s) => (Some(s), false),
            None => ((0..2).find(|&s| in_tavern(s)), true),
        },
    };
    let Some(side) = side else { return };
    if c.timewarp_suspended(side) {
        return;
    }
    static IDS: std::sync::OnceLock<(CardId, CardId)> = std::sync::OnceLock::new();
    let (guardian, sinestra) = *IDS.get_or_init(|| (cid(TITANIC_GUARDIAN), cid(SINESTRA)));
    let mut guardians = vec![];
    let mut sinestras = 0i32;
    for m in c.boards[side].iter().filter(|m| m.alive()) {
        let n = db().normal_of(m.card);
        if n == guardian && m.eid != eid {
            guardians.push((m.eid, if m.golden { 2 } else { 1 }));
        }
        if n == sinestra && c.combat && atk > 0 {
            sinestras += if m.golden { 2 } else { 1 };
        }
    }
    // Makeshift Master: the chosen minion's recruit-phase gains also go to your Deity, once
    // for each spell that chose it (`makeshift_marks`).
    if !c.combat && !temp && (atk > 0 || hp > 0) {
        let k = makeshift_marks(c, side, eid);
        if k > 0 {
            buff_deity(
                c,
                side,
                atk.max(0).saturating_mul(k),
                hp.max(0).saturating_mul(k),
            );
        }
    }
    if tavern || (guardians.is_empty() && sinestras == 0) {
        return;
    }
    IN_GAIN.with(|f| f.set(true));
    let _guard = GainGuard;
    // Titanic Guardian: whenever a different minion in your hand or board gains Health, this
    // also gains it (golden: twice that amount).
    let extra_hp = if c.on_board(eid) { sinestras } else { 0 };
    for (g, k) in guardians {
        if hp > 0 {
            c.buff(g, 0, hp.saturating_mul(k));
        }
        if extra_hp > 0 {
            c.buff(g, 0, extra_hp.saturating_mul(k));
        }
    }
    // Sinestra: whenever a friendly minion gains Attack during combat, give it +1 (+2) Health
    // permanently.
    if sinestras > 0 && c.on_board(eid) {
        c.buff_perm(side, eid, 0, sinestras);
    }
}

/// How many Makeshift Master spells chose `eid` this turn. Each cast is its own effect (a
/// Souvenir Stand copy's spell, a repeated cast): the spell adds one turn-data entry
/// `(BG36_MagicItem_417t, eid + 1)` per cast.
fn makeshift_marks(c: &Ctx, side: Side, eid: EntityId) -> i32 {
    let Some(p) = c.player_ref(side) else {
        return 0;
    };
    if p.turn_data.is_empty() {
        return 0;
    }
    let mm = crate::cardid!("BG36_MagicItem_417t");
    let mark = eid as i32 + 1;
    p.turn_data
        .iter()
        .filter(|x| x.0 == mm && x.1 == mark)
        .count() as i32
}

/// Makeshift Master's marked minions with their (Attack, maximum Health), before a "this
/// game" bonus re-syncs them (`Ctx::bump_player_stat`). Empty in combat and without a mark.
pub fn makeshift_marked(c: &Ctx, side: Side) -> Vec<(EntityId, i32, i32)> {
    let mut out: Vec<(EntityId, i32, i32)> = Vec::new();
    if c.combat {
        return out;
    }
    let Some(p) = c.player_ref(side) else {
        return out;
    };
    let mm = crate::cardid!("BG36_MagicItem_417t");
    for x in &p.turn_data {
        if x.0 != mm || x.1 <= 0 {
            continue;
        }
        let eid = (x.1 - 1) as EntityId;
        if !out.iter().any(|o| o.0 == eid)
            && let Some(m) = c.find_any(eid)
        {
            out.push((eid, m.atk, m.max_hp));
        }
    }
    out
}

/// After a "this game" bonus re-synced the minions (Undead Attack from Butchering...): what it
/// added to a marked minion goes to the Deity as well, as any other rise (every way of raising
/// stats is "stats gained").
pub fn makeshift_resynced(c: &mut Ctx, side: Side, before: &[(EntityId, i32, i32)]) {
    for &(eid, atk, hp) in before {
        let Some(m) = c.find_any(eid) else { continue };
        let (da, dh) = (m.atk.saturating_sub(atk), m.max_hp.saturating_sub(hp));
        let k = makeshift_marks(c, side, eid);
        if k > 0 && (da > 0 || dh > 0) {
            buff_deity(
                c,
                side,
                da.max(0).saturating_mul(k),
                dh.max(0).saturating_mul(k),
            );
        }
    }
}

fn random_minions(c: &mut Ctx, side: Side, n: i32, f: impl Fn(&crate::cards::CardDef) -> bool) {
    for _ in 0..n {
        if let Some(m) = c.random_minion(&f) {
            c.give_card(side, m);
        }
    }
}

pub(super) fn hero_buddy(c: &Ctx, player: PlayerId) -> Option<CardId> {
    c.g.players[player]
        .hero
        .and_then(|h| db().get(h).buddy)
        .filter(|b| db().get(*b).id != "TB_BaconShop_HERO_56_Buddy")
        .filter(|b| script(*b).is_some())
}

pub(super) fn power_buddy(c: &Ctx, player: PlayerId) -> Option<CardId> {
    let power = db().normal_of(c.g.players[player].powers.first()?.card);
    // Legal transformations of the same power retain its Buddy association. A truly
    // different power still uses its own Hero; the player's starting Hero is irrelevant.
    let power = match db().get(power).id.as_str() {
        "TB_BaconShop_HP_087t" => cid("TB_BaconShop_HP_087"),
        "TB_BaconShop_HP_036t" => cid("TB_BaconShop_HP_036"),
        "BG26_HERO_102p2" => cid("BG26_HERO_102p"),
        _ => power,
    };
    db().iter()
        .find(|d| d.hero_power == Some(power) && d.buddy.is_some())?
        .buddy
        .filter(|b| db().get(*b).id != "TB_BaconShop_HERO_56_Buddy")
        .filter(|b| script(*b).is_some())
}

/// Current Darkmoon Prize Tier (Tickatus: a Tier every 4 turns).
fn prize_tier(turn: u32) -> u8 {
    (turn / 4).clamp(1, 4) as u8
}

/// Tavern spells that give stats (hsdata 253216 tag 4254, pool spells).
const STAT_SPELLS: [&str; 29] = [
    "BG28_168",
    "BG28_169",
    "BG28_503",
    "BG28_520",
    "BG28_825",
    "BG28_838",
    "BG28_845",
    "BG28_886",
    "BG28_888",
    "BG28_897",
    "BG28_966",
    "BG31_880",
    "BG31_881",
    "BG31_886",
    "BG32_337",
    "BG33_811",
    "BG33_812",
    "BG33_813",
    "BG34_272",
    "BG34_444",
    "BG34_990",
    "BG35_912",
    "BG35_951",
    "BG36_246",
    "BG36_301t",
    "BG36_371",
    "BG36_624",
    "BG36_883",
    "EBG_Spell_032",
];

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Titanic Guardian / Sinestra: `on_stat_gain`.
        (TITANIC_GUARDIAN, E),
        (SINESTRA, E),
        // Maxwell, Mighty Steed: when sold, get 1 (2) Buddies of your Hero Power.
        (
            "TB_BaconShop_HERO_40_Buddy",
            Script {
                on_sell: Some(|c, me| {
                    let Some(p) = c.owner[me.side] else { return };
                    if let Some(b) = power_buddy(c, p) {
                        for _ in 0..me.g() {
                            c.give_card(me.side, b);
                        }
                    }
                }),
                ..E
            },
        ),
        // Sr. Tomb Diver: Deathrattle: make your right-most (two right-most) minions Golden.
        (
            "TB_BaconShop_HERO_41_Buddy",
            Script {
                deathrattle: Some(|c, me| {
                    // The right-most minion(s) by position: an already-Golden one uses up its
                    // slot instead of the effect moving further left.
                    let targets: Vec<EntityId> = c.boards[me.side]
                        .iter()
                        .rev()
                        .filter(|m| m.alive() && m.eid != me.eid)
                        .take(me.g() as usize)
                        .filter(|m| !m.golden && db().get(m.card).golden.is_some())
                        .map(|m| m.eid)
                        .collect();
                    for t in targets {
                        if let Some(m) = c.minion_mut(t) {
                            make_golden(m);
                        }
                    }
                }),
                ..E
            },
        ),
        // Jr. Navigator: Battlecry: reduce the Cost of 'Lead Explorer' by (2) ((4)).
        (
            "TB_BaconShop_HERO_42_Buddy",
            Script {
                battlecry: Some(|c, me, _| {
                    if let Some(h) = c.power_mut(me.side, cid("TB_BaconShop_HP_047")) {
                        h.cost_mod -= 2 * me.g();
                    }
                }),
                ..E
            },
        ),
        // Brann's Epic Egg: Deathrattle: summon and get 1 (2) random Battlecry minions.
        (
            "TB_BaconShop_HERO_43_Buddy",
            Script {
                deathrattle: Some(|c, me| {
                    let tier = c.tier(me.side);
                    let mut pos = me.pos;
                    for _ in 0..me.g() {
                        let Some(card) =
                            c.random_minion(|d| d.mech.has(Mech::BATTLECRY) && d.tier <= tier)
                        else {
                            return;
                        };
                        let m = c.new_minion(me.side, card);
                        if c.summon(me.side, pos, m).is_some() {
                            pos += 1;
                        }
                        c.give_card(me.side, card);
                    }
                }),
                ..E
            },
        ),
        // Loyal Henchman: after you kill a second minion each combat, get 1 (2) plain copies of
        // it (counter on the combat copy = kills this combat).
        (
            "TB_BaconShop_HERO_45_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Killed { side, victim, .. } = e else {
                        return;
                    };
                    // A friendly minion killed it (it may have died too).
                    if !c.combat || *side != me.side {
                        return;
                    }
                    let Some(m) = c.minion_mut(me.eid) else {
                        return;
                    };
                    m.counter += 1;
                    if m.counter != 2 {
                        return;
                    }
                    let card = c
                        .dead_record(1 - me.side, *victim)
                        .map(|d| d.card)
                        .or_else(|| c.minion(*victim).map(|m| m.card));
                    if let Some(card) = card {
                        for _ in 0..me.g() {
                            c.give_card(me.side, db().normal_of(card));
                        }
                    }
                }),
                listens: ev::KILLED,
                ..E
            },
        ),
        // Magnus Manastorm: two (four) Refreshes each turn are free, from the turn it is bought.
        (
            "TB_BaconShop_HERO_49_Buddy",
            // A per-turn allowance: counter = free Refreshes left this turn, refilled each turn and
            // not carried over (`Game::refresh_price` / `refresh`).
            Script {
                on_create: Some(|m| m.counter = if m.golden { 4 } else { 2 }),
                start_of_turn: Some(|c, me| {
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = 2 * me.g();
                    }
                }),
                ..E
            },
        ),
        // Hunter of Old: at the start of your turn, get 1 (2) of your last opponent's Buddy.
        (
            "TB_BaconShop_HERO_50_Buddy",
            Script {
                start_of_turn: Some(|c, me| {
                    let Some(p) = c.owner[me.side] else { return };
                    let Some(o) = c.g.last_fought(p) else {
                        return;
                    };
                    if let Some(b) = hero_buddy(c, o) {
                        for _ in 0..me.g() {
                            c.give_card(me.side, b);
                        }
                    }
                }),
                ..E
            },
        ),
        // Valithria Dreamwalker: whenever a Dragon enters your warband or the Tavern, gain +1/+1
        // (+2/+2).
        (
            "TB_BaconShop_HERO_53_Buddy",
            Script {
                // `counter` = the highest Tavern id already seen (see the Refreshed arm).
                on_create: Some(|m| m.counter = m.eid as i32),
                on_event: Some(|c, me, e| {
                    let n = match e {
                        Event::Summoned { side, eid }
                            if *side == me.side
                                && *eid != me.eid
                                && c.minion(*eid).is_some_and(|m| m.is(Races::DRAGON)) =>
                        {
                            1
                        }
                        // It just entered the warband: what the Tavern holds now was there
                        // before it and doesn't count later (e.g. when kept frozen).
                        Event::Summoned { eid, .. } if *eid == me.eid => {
                            let top = c.g.next_eid as i32;
                            if let Some(m) = c.minion_mut(me.eid) {
                                m.counter = top;
                            }
                            0
                        }
                        Event::Refreshed { side, .. } if *side == me.side => {
                            // Only Dragons that entered the Tavern since the last look: entity
                            // ids grow, so `counter` is the highest shop id already seen
                            // (frozen Dragons kept over a refill are not counted again).
                            let Some(p) = c.owner[me.side] else { return };
                            let seen = c.minion(me.eid).map_or(0, |m| m.counter);
                            let shop = &c.g.players[p].shop;
                            let n = shop
                                .iter()
                                .filter(|s| s.sid as i32 > seen)
                                .filter_map(|s| s.item.as_minion())
                                .filter(|m| m.is(Races::DRAGON))
                                .count() as i32;
                            let top = shop.iter().map(|s| s.sid as i32).max().unwrap_or(seen);
                            if let Some(m) = c.minion_mut(me.eid) {
                                m.counter = m.counter.max(top);
                            }
                            n
                        }
                        _ => 0,
                    };
                    if n > 0 {
                        c.buff(me.eid, n * me.g(), n * me.g());
                    }
                }),
                listens: ev::SUMMONED | ev::REFRESHED,
                ..E
            },
        ),
        // Sparkfin Soothsayer: Battlecry: transform Tavern minions into Murlocs of the same Tier
        // (golden: a Tier higher).
        (
            "TB_BaconShop_HERO_55_Buddy",
            Script {
                battlecry: Some(|c, me, _| {
                    let Some(p) = c.owner[me.side] else { return };
                    let up = u8::from(me.golden);
                    for sid in c.tavern_minions(me.side) {
                        let Some(i) = c.g.players[p].shop.iter().position(|s| s.sid == sid) else {
                            continue;
                        };
                        let Some(tier) = c.g.players[p].shop[i].item.as_minion().map(|m| m.tier)
                        else {
                            continue;
                        };
                        let t = (tier + up).min(6);
                        let Some(new) = c.draw_pool_minion(me.side, t, t, Races::MURLOC) else {
                            continue;
                        };
                        let new_sid = new.eid;
                        let old = std::mem::replace(
                            &mut c.g.players[p].shop[i],
                            crate::state::ShopSlot::new(new_sid, Item::Minion(new)),
                        );
                        if let Item::Minion(m) = old.item {
                            c.g.pool.return_minion(&m);
                        }
                    }
                }),
                ..E
            },
        ),
        // Vaelastrasz: Rally: get 1 (2) random Dragons.
        (
            "TB_BaconShop_HERO_56_Buddy",
            Script {
                rally: Some(|c, me, _| {
                    let tier = c.tier(me.side);
                    random_minions(c, me.side, me.g(), |d| {
                        d.races.intersects(Races::DRAGON) && d.tier <= tier
                    });
                }),
                ..E
            },
        ),
        // Chromie: one (two) Refreshes each turn are helpful (`content::helpful`; counter = helpful
        // Refreshes left this turn, a per-turn allowance).
        (
            "TB_BaconShop_HERO_57_Buddy",
            Script {
                on_create: Some(|m| m.counter = if m.golden { 2 } else { 1 }),
                start_of_turn: Some(|c, me| {
                    if let Some(m) = c.minion_mut(me.eid) {
                        m.counter = me.g();
                    }
                }),
                ..E
            },
        ),
        // Nexus Lord: 'Arcane Alteration' replaces with a card from one (two) Tiers higher
        // (heroes_active.rs Malygos).
        ("TB_BaconShop_HERO_58_Buddy", E),
        // Sklibb, Demon Hunter: the Tavern offers 1 (2) extra minions of a higher Tier whenever
        // it is Refreshed.
        (
            "TB_BaconShop_HERO_59_Buddy",
            Script {
                on_fill_shop: Some(|c, me| {
                    let t = c.tier(me.side);
                    if t < 6 {
                        for _ in 0..me.g() {
                            c.add_shop_minion(me.side, t + 1, t + 1, Races::NONE);
                        }
                    }
                }),
                ..E
            },
        ),
        // Crimson Hand Centurion: after 'Verdant Spheres' triggers, gain the stats (double) of
        // the last minion you bought.
        (
            "TB_BaconShop_HERO_60_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Bought {
                        side, card, eid, ..
                    } = e
                    else {
                        return;
                    };
                    if *side != me.side || !db().get(*card).is_minion() {
                        return;
                    }
                    let triggered = c
                        .player_ref(me.side)
                        .and_then(|p| p.power_of(cid("TB_BaconShop_HP_066")))
                        .is_some_and(|h| h.counter == 0);
                    if !triggered {
                        return;
                    }
                    if let Some((a, h)) = c.find_any(*eid).map(|m| (m.atk, m.hp)) {
                        c.buff(me.eid, a.saturating_mul(me.g()), h.saturating_mul(me.g()));
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        // Shadow Warden: Battlecry: your next 1 (2) Hero Powers make the target Golden
        // (heroes_active.rs Maiev; stored as card data on this Buddy's id).
        (
            "TB_BaconShop_HERO_62_Buddy",
            Script {
                battlecry: Some(|c, me, _| {
                    let key = cid("TB_BaconShop_HERO_62_Buddy");
                    let n = c.card_data(me.side, key) + me.g();
                    c.set_card_data(me.side, key, n);
                }),
                ..E
            },
        ),
        // Dagwik Stickytoe: end of turn, a random friendly Golden minion +5/+5 (+10/+10).
        (
            "TB_BaconShop_HERO_64_Buddy",
            Script {
                end_of_turn: Some(|c, me| {
                    if let Some(t) = c.random_friendly(me.side, |m| m.golden) {
                        c.buff(t, 5 * me.g(), 5 * me.g());
                    }
                }),
                ..E
            },
        ),
        // Raging Contender: after 'Trash for Treasure', gain Gold equal to the removed minion's
        // Tier (twice) (heroes_active.rs Hooktusk).
        ("TB_BaconShop_HERO_67_Buddy", E),
        // Sharkbait: when sold, refresh your Hero Power: it can be used again this turn, a
        // "once per game" button (Kragg's own Piggy Bank) once more this game. A passive
        // power has nothing to refresh.
        (
            "TB_BaconShop_HERO_68_Buddy",
            Script {
                on_sell: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        for h in p.powers.iter_mut() {
                            h.uses_turn = 0;
                            let button = script(h.card).is_some_and(|s| !s.has_flag(flag::PASSIVE));
                            if button && hp_limits(h.card).once {
                                h.uses_game = 0;
                            }
                        }
                    }
                }),
                ..E
            },
        ),
        // Lil' K.T.: at the start of your turn, get 1 (2) plain minions from your
        // lowest-Health opponent's warband.
        (
            "TB_BaconShop_HERO_70_Buddy",
            Script {
                start_of_turn: Some(|c, me| {
                    let Some(p) = c.owner[me.side] else { return };
                    let keyed: Vec<(usize, i64)> = (0..c.g.players.len())
                        .filter(|&q| {
                            q != p && c.g.players[q].alive && !c.g.fought_board(q).is_empty()
                        })
                        .map(|q| (q, -((c.g.players[q].health + c.g.players[q].armor) as i64)))
                        .collect();
                    let target = c.pick_max(keyed);
                    let Some(q) = target else { return };
                    let cards: Vec<CardId> =
                        c.g.fought_board(q)
                            .iter()
                            .map(|m| db().normal_of(m.card))
                            .collect();
                    for _ in 0..me.g() {
                        if let Some(&card) = c.g.rng.pick(&cards) {
                            c.give_card(me.side, card);
                        }
                    }
                }),
                ..E
            },
        ),
        // Mawsworn Soulkeeper: Deathrattle: summon 2 (4) random Undead.
        (
            "TB_BaconShop_HERO_702_Buddy",
            Script {
                deathrattle: Some(|c, me| {
                    let tier = c.tier(me.side);
                    let mut pos = me.pos;
                    for _ in 0..2 * me.g() {
                        let Some(card) = c
                            .random_minion(|d| d.races.intersects(Races::UNDEAD) && d.tier <= tier)
                        else {
                            return;
                        };
                        let m = c.new_minion(me.side, card);
                        if c.summon(me.side, pos, m).is_some() {
                            pos += 1;
                        }
                    }
                }),
                ..E
            },
        ),
        // Jandice's Apprentice: after you play a minion you've already played this turn, give
        // your minions stats equal to your Tier (twice). Plays are remembered per card as the
        // turn number (card data under the card id with the top bit set).
        (
            "TB_BaconShop_HERO_71_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Played { side, played, .. } = e else {
                        return;
                    };
                    if *side != me.side {
                        return;
                    }
                    if played.repeated {
                        let k = c.tier(me.side) as i32 * me.g();
                        for t in c.friendly(me.side) {
                            c.buff(t, k, k);
                        }
                    }
                }),
                listens: ev::PLAYED,
                ..E
            },
        ),
        // Barov's Apprentice: after you play a Coin, gain 1 (2) Gold.
        (
            "TB_BaconShop_HERO_72_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::CardPlayed { side, card, .. } = e
                        && *side == me.side
                        && (*card == crate::cardid!(TAVERN_COIN)
                            || *card == crate::cardid!("TB_BaconShop_HP_008a"))
                    {
                        c.gain_gold(me.side, me.g());
                    }
                }),
                listens: ev::CARD_PLAYED,
                ..E
            },
        ),
        // Evergreen Botani: end of turn, get 1 (2) random minions of your Tier.
        (
            "TB_BaconShop_HERO_74_Buddy",
            Script {
                end_of_turn: Some(|c, me| {
                    let tier = c.tier(me.side);
                    random_minions(c, me.side, me.g(), |d| d.tier == tier);
                }),
                ..E
            },
        ),
        // Lantern Tender: end of turn, get 2 (4) random Tavern spells that give stats (spells
        // targeting a friendly minion).
        (
            "TB_BaconShop_HERO_75_Buddy",
            Script {
                end_of_turn: Some(|c, me| {
                    let tier = c.tier(me.side);
                    for _ in 0..2 * me.g() {
                        let s = c.random_spell(|d| {
                            d.tier <= tier && STAT_SPELLS.contains(&d.id.as_str())
                        });
                        if let Some(s) = s {
                            c.give_card(me.side, s);
                        }
                    }
                }),
                ..E
            },
        ),
        // Spirit of Air: Deathrattle: give 1 (2) random friendly minions Windfury, Divine
        // Shield and Taunt.
        (
            "TB_BaconShop_HERO_76_Buddy",
            Script {
                deathrattle: Some(|c, me| {
                    let mut f = c.friendly_where(me.side, |m| m.eid != me.eid);
                    c.g.rng.shuffle(&mut f);
                    for t in f.into_iter().take(me.g() as usize) {
                        c.give_kw(t, Kw::WINDFURY);
                        c.give_kw(t, Kw::DIVINE_SHIELD);
                        c.give_kw(t, Kw::TAUNT);
                    }
                }),
                ..E
            },
        ),
        // Snow Elemental: the Tavern offers 1 (2) extra Elementals whenever it is Refreshed
        // ("Frozen": per-minion freezing is not modelled).
        (
            "TB_BaconShop_HERO_78_Buddy",
            Script {
                // The extra Elementals come Frozen (card text).
                on_fill_shop: Some(|c, me| {
                    let t = c.tier(me.side);
                    for _ in 0..me.g() {
                        if let Some(sid) = c.add_shop_minion(me.side, 1, t, Races::ELEMENTAL)
                            && let Some(p) = c.owner[me.side]
                            && let Some(s) = c.g.players[p].shop.iter_mut().find(|s| s.sid == sid)
                        {
                            s.frozen = true;
                        }
                    }
                }),
                ..E
            },
        ),
        // Burth: after you Discover a minion, give it +2/+2 (+4/+4) and improve this (by the base
        // amount; the counter is in steps of the plain card and kept permanently, `flag::IMPROVES`:
        // a triple keeps the growth).
        (
            "TB_BaconShop_HERO_90_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Discovered { side, card, hid } = e else {
                        return;
                    };
                    if *side != me.side || !db().get(*card).is_minion() {
                        return;
                    }
                    let k = me
                        .g()
                        .saturating_add(c.minion(me.eid).map_or(0, |m| m.counter))
                        .saturating_mul(2);
                    // The Discovered minion itself: in hand, or waiting for hand space.
                    let in_hand = c
                        .player_ref(me.side)
                        .is_some_and(|p| p.hand.iter().any(|h| h.hid == *hid));
                    if in_hand {
                        c.buff(*hid, k, k);
                    } else if let Some(m) = c.player(me.side).and_then(|p| {
                        let i = p.queue_marks().iter().position(|m| m.hid == *hid)?;
                        p.hand_queue[i].as_minion_mut()
                    }) {
                        m.add_stats(k, k);
                    } else {
                        return;
                    }
                    c.counter_perm(me.side, me.eid, me.g());
                }),
                listens: ev::DISCOVERED,
                flags: flag::IMPROVES,
                ..E
            },
        ),
        // Phyresz: when sold, Discover a plain copy of a different minion you have exactly one
        // of (golden: twice).
        (
            "TB_BaconShop_HERO_91_Buddy",
            Script {
                on_sell: Some(|c, me| {
                    let Some(p) = c.owner[me.side] else { return };
                    let mut counts: Vec<(CardId, u32)> = vec![];
                    let cards = c.boards[me.side].iter().map(|m| m.card).chain(
                        c.g.players[p]
                            .hand
                            .iter()
                            .filter_map(|h| h.item.as_minion())
                            .map(|m| m.card),
                    );
                    for card in cards {
                        let n = db().normal_of(card);
                        match counts.iter_mut().find(|x| x.0 == n) {
                            Some(x) => x.1 += 1,
                            None => counts.push((n, 1)),
                        }
                    }
                    let me_card = db().normal_of(me.card);
                    let singles: Vec<CardId> = counts
                        .into_iter()
                        .filter(|x| x.1 == 1 && x.0 != me_card)
                        .map(|x| x.0)
                        .collect();
                    for _ in 0..me.g() {
                        let picks = c.g.rng.sample_indices(singles.len(), 3);
                        let options: Vec<Item> = picks
                            .into_iter()
                            .map(|i| Item::Minion(c.new_minion(me.side, singles[i])))
                            .collect();
                        c.offer_discover(me.side, me.card, options);
                    }
                }),
                ..E
            },
        ),
        // Baby Y'Shaarj: after you summon a minion of your Tier, give your minions +1/+1
        // (+2/+2).
        (
            "TB_BaconShop_HERO_92_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Summoned { side, eid } = e else {
                        return;
                    };
                    if *side != me.side || *eid == me.eid {
                        return;
                    }
                    let tier = c.tier(me.side);
                    if c.minion(*eid).is_some_and(|m| m.tier == tier) {
                        for t in c.friendly(me.side) {
                            c.buff(t, me.g(), me.g());
                        }
                    }
                }),
                listens: ev::SUMMONED,
                ..E
            },
        ),
        // Baby N'Zoth: Battlecry: make a (golden: all) friendly Deathrattle minion Golden.
        // The normal one is a targeted Battlecry (client TARGETING_ARROW_TEXT "Make a
        // Deathrattle minion Golden."; the golden card has no arrow): the player chooses.
        // A chosen minion that is already Golden wastes the Battlecry.
        (
            "TB_BaconShop_HERO_93_Buddy",
            Script {
                battlecry: Some(|c, me, t| {
                    let eligible = |m: &Minion| {
                        m.eid != me.eid
                            && !m.golden
                            && db().get(m.card).golden.is_some()
                            && crate::script::minion_has(m, crate::script::Hook::Deathrattle)
                    };
                    let picks = if me.golden {
                        c.friendly_where(me.side, eligible)
                    } else if c.minion(t).is_some_and(eligible) {
                        vec![t]
                    } else {
                        vec![]
                    };
                    for t in picks {
                        if let Some(m) = c.minion_mut(t) {
                            make_golden(m);
                        }
                    }
                }),
                battlecry_target: TargetSpec::FriendlyHas(crate::script::Hook::Deathrattle),
                ..E
            },
        ),
        // Ticket Collector: when sold, Discover 1 (2) Darkmoon Prizes from the next Prize Tier.
        (
            "TB_BaconShop_HERO_94_Buddy",
            Script {
                on_sell: Some(|c, me| {
                    let tier = (prize_tier(c.g.turn) + 1).min(4);
                    let cands: Vec<CardId> = PRIZES
                        .iter()
                        .filter(|x| x.1 == tier)
                        .map(|x| cid(x.0))
                        .collect();
                    for _ in 0..me.g() {
                        let picks = c.g.rng.sample_indices(cands.len(), 3);
                        let options: Vec<Item> = picks
                            .into_iter()
                            .map(|i| {
                                let x = cands[i];
                                if db().get(x).is_minion() {
                                    Item::Minion(c.new_minion(me.side, x))
                                } else {
                                    Item::spell(x)
                                }
                            })
                            .collect();
                        c.offer_discover(me.side, me.card, options);
                    }
                }),
                ..E
            },
        ),
        // Wandering Treant: whenever a friendly Taunt minion is attacked, give your minions
        // +1/+1 (+2/+2) permanently.
        (
            "TB_BaconShop_HERO_95_Buddy",
            Script {
                on_event: Some(|c, me, e| {
                    let Event::Attacks { side, defender, .. } = e else {
                        return;
                    };
                    if *side == me.side
                        || !c.friendly(me.side).contains(defender)
                        || !c.minion(*defender).is_some_and(|m| m.has(Kw::TAUNT))
                    {
                        return;
                    }
                    for t in c.friendly(me.side) {
                        c.buff_perm(me.side, t, me.g(), me.g());
                    }
                }),
                listens: ev::ATTACKS,
                ..E
            },
        ),
    ]
}
