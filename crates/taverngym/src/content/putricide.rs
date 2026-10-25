//! Professor Putricide's Build-An-Undead: choose a base Undead from pool 1, then a part from pool
//! 2; the Creation (`BG25_HERO_100pt`, can't triple: no golden form) gets both cards' stats added,
//! their keywords, and their effects (both cards are attached to it). Pools: Firestone reference
//! data (`battlegroundsPutridicePool1/2`) minus the five removed in 34.6.2 (Skeletal Sidekick,
//! Incorporeal Corporal, Festeroot Hulk, Acherus Veteran, Ticking Abomination). Each step offers 3
//! options: one of Tier T or T-1, one of Tier T-2..T, one of any Tier up to T (wiki description; T
//! = Tavern Tier). Also used by Putricide Sticker and Festergut.

use super::*;

pub const CREATION: &str = "BG25_HERO_100pt";
/// Discover sources of the two steps (`after_discover`).
pub const STEP1: &str = "BG25_HERO_100p";
pub const STEP2: &str = "BG25_HERO_100pt";

const POOL1: [&str; 14] = [
    "BG25_005",      // Jelly Belly
    "BG25_009",      // Eternal Summoner
    "BG25_010",      // Handless Forsaken
    "BG25_014",      // Hungering Abomination
    "BG_GIL_681",    // Nightmare Amalgam
    "BG26_RLK_833",  // Foul Egg
    "BG_RLK_957",    // Wailing Banshee
    "BG26_RLK_824",  // Arms Dealer
    "BG26_GIL_513",  // Lost Spirit
    "BG_LOE_012",    // Tomb Pillager
    "BG26_tt_004",   // Flesheating Ghoul
    "BG26_LOOT_534", // Gilded Gargoyle
    "BG26_ICC_065",  // Bone Baron
    "BG26_ICC_027",  // Bone Drake (Dragon lobbies only)
];

const POOL2: [&str; 14] = [
    "BG25_001",     // Risen Rider
    "BG25_050",     // Colossus of the Sun
    "BG26_RLK_119", // Drakkari Embalmer
    "BG26_OG_248",  // Am'gam Rager
    "BG26_FP1_005", // Shade of Naxxramas
    "BG26_ICC_028", // Sunborne Val'kyr
    "BG26_CFM_636", // Shadow Rager
    "BG26_ICC_026", // Grim Necromancer
    "BG_ULD_275",   // Bone Wraith
    "BG_ICC_032",   // Venomancer
    "BG_ULD_205",   // Candletaker
    "BG_ICC_094",   // Fallen Sun Cleric
    "BG_FP1_014t",  // Thaddius
    "BG_BT_703t",   // Cursed Shadow
];

/// Parts not offered after a Reborn base (Firestone `battlegroundsPutridiceSummonerExclusion`).
const REBORN_EXCLUDED: [&str; 5] = [
    "BG25_001",
    "BG25_050",
    "BG_ULD_275",
    "BG_ICC_032",
    "BG_ULD_205",
];

const SKELETON: &str = "BG_ICC_026t";
const COIN: &str = "BG28_810";

/// 3 options: Tier T or T-1, Tier T-2..T, any Tier up to T (distinct; falls back to any).
fn options(c: &mut Ctx, side: Side, cands: &[CardId]) -> Vec<CardId> {
    let t = c.tier(side).min(6);
    let tier = |x: &CardId| db().get(*x).tier;
    let bands: [Box<dyn Fn(u8) -> bool>; 3] = [
        Box::new(move |k| k <= t && k + 1 >= t),
        Box::new(move |k| k <= t && k + 2 >= t),
        Box::new(move |k| k <= t),
    ];
    let mut out: Vec<CardId> = vec![];
    for band in &bands {
        let mut ok: Vec<CardId> = cands
            .iter()
            .copied()
            .filter(|x| band(tier(x)) && !out.contains(x))
            .collect();
        if ok.is_empty() {
            ok = cands.iter().copied().filter(|x| !out.contains(x)).collect();
        }
        if let Some(&x) = c.g.rng.pick(&ok) {
            out.push(x);
        }
    }
    out
}

fn offer(c: &mut Ctx, side: Side, source: &str, cands: Vec<CardId>) {
    let picks = options(c, side, &cands);
    let items = picks
        .into_iter()
        .map(|x| Item::Minion(c.new_minion(side, x)))
        .collect();
    c.offer_discover(side, cid(source), items);
}

/// Start a Creation: offer the base (step 1).
pub fn craft(c: &mut Ctx, side: Side) {
    // The two steps are Discovers of the Hero Power's cards. A Creation a trinket asked for
    // (Putricide Sticker) keeps the trinket's hand protection through them: with a full
    // hand it waits for space (official trinket rules 2026-04-06). `STEP1`'s card data = the
    // asking card + 1 (0 = none with protection).
    let giver = c
        .src
        .filter(|s| crate::engine::hand_protected(*s))
        .map_or(0, |s| i32::from(s.base().0) + 1);
    c.set_card_data(side, cid(STEP1), giver);
    let dragons = c.g.lobby_races.intersects(Races::DRAGON);
    let cands: Vec<CardId> = POOL1
        .iter()
        .filter(|id| dragons || **id != "BG26_ICC_027")
        .map(|id| cid(id))
        .collect();
    offer(c, side, STEP1, cands);
}

/// A step's pick resolved (`after_discover`; the picked card was already removed from hand).
pub fn picked(c: &mut Ctx, side: Side, step1: bool, card: CardId) {
    let key = cid(CREATION);
    if step1 {
        c.set_card_data(side, key, card.0 as i32);
        let reborn = c.new_minion(side, card).kw.has(Kw::REBORN);
        let cands: Vec<CardId> = POOL2
            .iter()
            .filter(|id| !reborn || !REBORN_EXCLUDED.contains(id))
            .map(|id| cid(id))
            .collect();
        offer(c, side, STEP2, cands);
        return;
    }
    let base = CardId(c.card_data(side, key) as u16);
    c.set_card_data(side, key, 0);
    if base.0 == 0 {
        return;
    }
    let m = assemble(c, side, base, card);
    let giver = c.card_data(side, cid(STEP1));
    c.set_card_data(side, cid(STEP1), 0);
    let prev = c.src;
    if giver > 0 {
        c.src = Some(CardId((giver - 1) as u16));
    }
    c.add_to_hand(side, Item::Minion(m));
    c.src = prev;
}

fn assemble(c: &mut Ctx, side: Side, base: CardId, part: CardId) -> Minion {
    let (a, b) = (c.new_minion(side, base), c.new_minion(side, part));
    let mut m = c.new_minion(side, cid(CREATION));
    m.set_stats(
        a.atk.saturating_add(b.atk),
        a.max_hp.saturating_add(b.max_hp),
    );
    m.give(a.kw);
    m.give(b.kw);
    if a.races == Races::ALL || b.races == Races::ALL {
        m.races = Races::ALL; // Nightmare Amalgam
    }
    m.attached.push(base);
    m.attached.push(part);
    m
}

/// A fresh copy of a Creation (Reborn): its two cards assembled again, without what the dead
/// one had gained since (buffs, Magnetic parts, Dark Gifts). None for any other minion.
/// The Creation is those two cards: a plain `BG25_HERO_100pt` would be a 1/1 with no text.
pub fn fresh_copy(c: &mut Ctx, side: Side, dead: &Minion) -> Option<Minion> {
    if !dead.is_card_id(cid(CREATION)) {
        return None;
    }
    let (base, part) = (*dead.attached.first()?, *dead.attached.get(1)?);
    Some(assemble(c, side, base, part))
}

/// A random Creation (Festergut): one option of each step, as `craft` would offer them.
pub fn random_creation(c: &mut Ctx, side: Side) -> Option<Minion> {
    let dragons = c.g.lobby_races.intersects(Races::DRAGON);
    let p1: Vec<CardId> = POOL1
        .iter()
        .filter(|id| dragons || **id != "BG26_ICC_027")
        .map(|id| cid(id))
        .collect();
    let base = *options(c, side, &p1).first()?;
    let reborn = c.new_minion(side, base).kw.has(Kw::REBORN);
    let p2: Vec<CardId> = POOL2
        .iter()
        .filter(|id| !reborn || !REBORN_EXCLUDED.contains(id))
        .map(|id| cid(id))
        .collect();
    let part = *options(c, side, &p2).first()?;
    Some(assemble(c, side, base, part))
}

fn dr_coin(c: &mut Ctx, me: Me) {
    for _ in 0..me.g() {
        c.give_card(me.side, cid(COIN));
    }
}

fn bc_random_friendly(c: &mut Ctx, me: Me, t: EntityId, undead: bool) -> Option<EntityId> {
    if t != 0 {
        return Some(t);
    }
    c.random_friendly(me.side, |m| {
        m.eid != me.eid && (!undead || m.is(Races::UNDEAD))
    })
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // The Creation itself: everything it does comes from its two attached cards (an entry
        // here marks the card as implemented).
        (CREATION, E),
        // --- pool 1 (bases) ---
        // Jelly Belly: after a friendly minion is Reborn, gain +2/+3 permanently.
        (
            "BG25_005",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Reborn { side, .. } if *side == me.side) {
                        c.buff_perm(me.side, me.eid, 2 * me.g(), 3 * me.g());
                    }
                }),
                listens: ev::REBORN,
                ..E
            },
        ),
        // Hungering Abomination: Avenge (1): gain +1/+2 permanently.
        (
            "BG25_014",
            Script {
                avenge: Some((1, 1, |c, me| {
                    c.buff_perm(me.side, me.eid, me.g(), 2 * me.g())
                })),
                ..E
            },
        ),
        // Nightmare Amalgam: all minion types (`picked`).
        ("BG_GIL_681", E),
        // Foul Egg: Deathrattle: summon a 3/3 Undead Chicken (the token isn't in the card
        // data: a 3/3 Skeleton stands in).
        (
            "BG26_RLK_833",
            Script {
                deathrattle: Some(|c, me| {
                    for i in 0..me.g() as usize {
                        let mut m = c.new_minion(me.side, cid(SKELETON));
                        m.set_base_stats(3, 3);
                        c.summon(me.side, me.pos + i, m);
                    }
                }),
                ..E
            },
        ),
        // Wailing Banshee: Deathrattle: give a random friendly Undead +2/+1.
        (
            "BG_RLK_957",
            Script {
                deathrattle: Some(|c, me| {
                    if let Some(t) = c.random_friendly(me.side, |m| m.is(Races::UNDEAD)) {
                        c.buff(t, 2 * me.g(), me.g());
                    }
                }),
                ..E
            },
        ),
        // Arms Dealer: after you summon an Undead, give it +1 Attack.
        (
            "BG26_RLK_824",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Summoned { side, eid } = e
                        && *side == me.side
                        && *eid != me.eid
                        && c.minion(*eid).is_some_and(|m| m.is(Races::UNDEAD))
                    {
                        c.buff(*eid, me.g(), 0);
                    }
                }),
                listens: ev::SUMMONED,
                ..E
            },
        ),
        // Lost Spirit: Deathrattle: your minions +1 Attack.
        (
            "BG26_GIL_513",
            Script {
                deathrattle: Some(|c, me| {
                    for t in c.friendly(me.side) {
                        c.buff(t, me.g(), 0);
                    }
                }),
                ..E
            },
        ),
        // Tomb Pillager / Gilded Gargoyle: Deathrattle: add a Coin to your hand.
        (
            "BG_LOE_012",
            Script {
                deathrattle: Some(dr_coin),
                ..E
            },
        ),
        (
            "BG26_LOOT_534",
            Script {
                deathrattle: Some(dr_coin),
                ..E
            },
        ),
        // Flesheating Ghoul: whenever a minion dies, gain +1 Attack.
        (
            "BG26_tt_004",
            Script {
                on_event: Some(|c, me, e| {
                    if matches!(e, Event::Died { eid, .. } if *eid != me.eid) {
                        c.buff(me.eid, me.g(), 0);
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Bone Baron: Deathrattle: add two 1/1 Skeletons to your hand.
        (
            "BG26_ICC_065",
            Script {
                deathrattle: Some(|c, me| {
                    for _ in 0..2 * me.g() {
                        c.give_card(me.side, cid(SKELETON));
                    }
                }),
                ..E
            },
        ),
        // Bone Drake: Deathrattle: add a random Dragon to your hand.
        (
            "BG26_ICC_027",
            Script {
                deathrattle: Some(|c, me| {
                    for _ in 0..me.g() {
                        let tier = c.tier(me.side);
                        if let Some(d) =
                            c.random_minion(|d| d.races.intersects(Races::DRAGON) && d.tier <= tier)
                        {
                            c.give_card(me.side, d);
                        }
                    }
                }),
                ..E
            },
        ),
        // --- pool 2 (parts) ---
        // Drakkari Embalmer: Battlecry: give a friendly Undead Reborn.
        (
            "BG26_RLK_119",
            Script {
                battlecry: Some(|c, me, t| {
                    if let Some(t) = bc_random_friendly(c, me, t, true) {
                        c.give_kw(t, Kw::REBORN);
                    }
                }),
                ..E
            },
        ),
        // Shade of Naxxramas: at the start of your turn, gain +1/+1.
        (
            "BG26_FP1_005",
            Script {
                start_of_turn: Some(|c, me| c.buff(me.eid, me.g(), me.g())),
                ..E
            },
        ),
        // Sunborne Val'kyr: Battlecry: give adjacent minions +2 Health.
        (
            "BG26_ICC_028",
            Script {
                battlecry: Some(|c, me, _| {
                    let Some((s, i)) = c.locate(me.eid) else {
                        return;
                    };
                    let n: Vec<EntityId> = [i.checked_sub(1), Some(i + 1)]
                        .into_iter()
                        .flatten()
                        .filter_map(|j| c.boards[s].get(j).map(|m| m.eid))
                        .collect();
                    for t in n {
                        c.buff(t, 0, 2 * me.g());
                    }
                }),
                ..E
            },
        ),
        // Grim Necromancer: Battlecry: summon two 1/1 Skeletons.
        (
            "BG26_ICC_026",
            Script {
                battlecry: Some(|c, me, _| {
                    let pos = c.pos_right_of(me.eid);
                    c.summon_card(me.side, pos, cid(SKELETON), 2 * me.g() as usize);
                }),
                ..E
            },
        ),
        // Fallen Sun Cleric: Battlecry: give a friendly minion +1/+1.
        (
            "BG_ICC_094",
            Script {
                battlecry: Some(|c, me, t| {
                    if let Some(t) = bc_random_friendly(c, me, t, false) {
                        c.buff(t, me.g(), me.g());
                    }
                }),
                ..E
            },
        ),
        // Keyword / vanilla parts: stats and keywords only.
        ("BG25_050", E),
        ("BG26_OG_248", E),
        ("BG26_CFM_636", E),
        ("BG_ULD_275", E),
        ("BG_ICC_032", E),
        ("BG_ULD_205", E),
        ("BG_FP1_014t", E),
        ("BG_BT_703t", E),
    ]
}
