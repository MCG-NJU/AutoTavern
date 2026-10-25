//! Card content: scripts for minions, spells, hero powers, trinkets and Dark Gifts, plus
//! system hooks (deity awakening, discard triggers, scheduled effects).
//!
//! Content is intentionally a subset (see docs/CONTENT.md); the engine treats unscripted
//! cards as vanilla bodies and `ContentMode::Implemented` keeps them out of pools.

use crate::cards::{CardId, cid, db};
use crate::engine::{Ctx, Game};
use crate::event::{Event, ev};
use crate::script::{Me, Script, TargetSpec, flag, script};
use crate::state::{Item, Minion, Scheduled};
use crate::types::{EntityId, Kw, Mech, PlayerId, Races, Side};

mod aberrations;
mod beasts;
mod buddies_a;
mod buddies_b;
mod buddies_c;
pub use buddies_c::{makeshift_marked, makeshift_resynced, on_stat_gain, on_temp_stat_gain};
mod darkmoon;
mod demons;
mod demons_p1;
pub use demons::bonus_keywords;
mod dragons;
mod elementals;
mod gifts;
pub use gifts::{
    TRASTATH_GIFTS as TRASTATH_GIFT_IDS, gift_fits, gift_init, gifted_options, gifted_options_with,
    slotted_gift_options, window_gifts,
};
mod hero_powers;
mod heroes;
mod heroes_active;
mod heroes_combat;
mod heroes_eco;
mod heroes_gen;
mod heroes_special;
pub use heroes_special::gift_discover_source;
mod mechs;
mod mechs_p1;
mod minions;
mod murlocs;
mod murlocs_p1;
mod naga;
mod neutral;
mod pirates;
mod pirates_p1;
mod quests;
mod secrets;
pub mod starcraft;
mod trinkets_p1;
mod trinkets_p2;
mod trinkets_p3;
mod trinkets_p4;
mod trinkets_p5;
mod trinkets_p6;
mod trinkets_p7;
mod trinkets_p8;
mod yogg;
pub use heroes_eco::hero_power_removed;
pub use quests::{
    ENDLESS_REWARDS, buddy_pool, check_total_attack, discover_buddy, draw_quest_types,
    on_stats_gained, quest_goal, quest_offerable, reward_weight,
};
pub use trinkets_p1::trinket_on_create;
pub use trinkets_p2::aurum_gold_check;
pub use trinkets_p3::{player_refresh, price_rule_used, sargeras_ready, tapestry_armed};
pub use trinkets_p6::{cube_turn_start, grant_trinket, replace_trinket};
pub use trinkets_p8::writhing_tentacles;
pub use yogg::wheel_of_yogg;
pub mod helpful;
mod putricide;
mod quilboar;
mod quilboar_p1;
mod spells;
mod spells_p2;
mod tier7;
pub mod timewarp;
pub mod trinket_offers;
mod trinkets;
mod tw_beasts;
mod tw_demons;
mod tw_dragons_pirates;
mod tw_elementals;
mod tw_murlocs_mechs;
mod tw_quilboar_neutral;
mod tw_spells;
mod tw_undead;
mod undead;

pub use spells::make_golden;

pub(crate) const E: Script = Script::EMPTY;

// Token / generated card ids (resolved via explicit relations in the bundle).
pub const SKELETON: &str = "BG_ICC_026t";
pub const MICROBOT: &str = "BG_BOT_312t";
pub const BEETLE: &str = "BG28_603t";
pub const TENTACLE: &str = "BGFYM_002t";
pub const HELPING_HAND: &str = "BG25_010t";
pub const WATER_DROPLET: &str = "BGS_115t";
pub const FORAGING_BAT: &str = "BG36_200t";
pub const ETERNAL_KNIGHT: &str = "BG25_008";
/// "Golem" token: the bundle has no stand-alone Golem for Dark Paradox / Golemancy, so the Blood
/// Golem body (no type, 1/1) is used with stats set by the effect.
pub const GOLEM: &str = "BG30_MagicItem_442t";
pub const BLOOD_GEM: &str = "BG20_GEM";

/// Client TavernSpell also includes generated special spells and StarCraft Upgrades.
/// Real Tavern spells have a Tavern Tier; a tier-zero Goldenizer/exit token does not.
pub fn is_tavern_spell(card: CardId) -> bool {
    let def = db().get(card);
    def.kind == crate::cards::CardKind::TavernSpell
        && def.tier > 0
        && !def.id.starts_with("BG31_HERO_801pt")
}

/// Bloodbinder's special Blood Gems explicitly also count as Tavern spells.
pub fn is_tavern_spell_instance(card: CardId, data: i32) -> bool {
    is_tavern_spell(card) || (card == crate::cardid!("BG20_GEM") && data == 4)
}

/// Copy the last actual Tavern spell with its instance parameter. Ignore an
/// ineligible entry from an older snapshot.
pub fn give_last_tavern_spell(c: &mut Ctx, side: Side, n: i32) {
    let Some((card, data)) = c.player_ref(side).and_then(|p| {
        let card = p.stats.last_spell_cast?;
        Some((card, p.stats.last_spell_cast_data))
    }) else {
        return;
    };
    if is_tavern_spell_instance(card, data) {
        for _ in 0..n {
            c.add_to_hand(side, Item::Spell { card, data });
        }
    }
}

pub const FODDER: &str = "BG35_150t";
pub const TAVERN_COIN: &str = "BG28_810";

/// Every script, keyed by card id (normal form; golden forms resolve automatically).
pub fn all_scripts() -> Vec<(&'static str, Script)> {
    let mut v = minions::scripts();
    v.extend(murlocs::scripts());
    v.extend(murlocs_p1::scripts());
    v.extend(pirates::scripts());
    v.extend(pirates_p1::scripts());
    v.extend(beasts::scripts());
    v.extend(aberrations::scripts());
    v.extend(quilboar::scripts());
    v.extend(quilboar_p1::scripts());
    v.extend(demons::scripts());
    v.extend(demons_p1::scripts());
    v.extend(dragons::scripts());
    v.extend(dragons::scripts_p1());
    v.extend(elementals::scripts());
    v.extend(mechs::scripts());
    v.extend(mechs_p1::scripts());
    v.extend(naga::scripts());
    v.extend(hero_powers::scripts());
    v.extend(quests::scripts());
    v.extend(undead::scripts());
    v.extend(neutral::scripts());
    v.extend(tier7::scripts());
    v.extend(trinkets_p1::scripts());
    v.extend(trinkets_p2::scripts());
    v.extend(trinkets_p3::scripts());
    v.extend(trinkets_p4::scripts());
    v.extend(trinkets_p5::scripts());
    v.extend(trinkets_p6::scripts());
    v.extend(trinkets_p7::scripts());
    v.extend(trinkets_p8::scripts());
    v.extend(darkmoon::scripts());
    v.extend(secrets::scripts());
    v.extend(starcraft::scripts());
    v.extend(buddies_a::scripts());
    v.extend(buddies_b::scripts());
    v.extend(buddies_c::scripts());
    v.extend(spells::scripts());
    v.extend(spells_p2::scripts());
    v.extend(heroes::scripts());
    v.extend(heroes_eco::scripts());
    v.extend(heroes_active::scripts());
    v.extend(heroes_combat::scripts());
    v.extend(heroes_gen::scripts());
    v.extend(heroes_special::scripts());
    v.extend(putricide::scripts());
    v.extend(trinkets::scripts());
    v.extend(gifts::scripts());
    v.extend(tw_beasts::scripts());
    v.extend(tw_elementals::scripts());
    v.extend(tw_undead::scripts());
    v.extend(tw_demons::scripts());
    v.extend(tw_dragons_pirates::scripts());
    v.extend(tw_murlocs_mechs::scripts());
    v.extend(tw_quilboar_neutral::scripts());
    v.extend(tw_spells::scripts());
    v
}

// ---------------------------------------------------------------------------- helpers

/// Summon `n` copies of a token at the deathrattle/summon position (golden token if asked).
/// The fresh copy a Reborn brings back when the card alone does not describe the minion:
/// Putricide's Creation is assembled from its two cards again (`putricide::fresh_copy`).
pub fn fresh_copy(c: &mut Ctx, side: Side, dead: &Minion) -> Option<Minion> {
    putricide::fresh_copy(c, side, dead)
}

pub fn summon_n(c: &mut Ctx, me: Me, token: &str, n: usize, golden: bool) {
    let card = if golden {
        db().golden_of(cid(token))
    } else {
        cid(token)
    };
    let pos = if me.pos == 0 && c.on_board(me.eid) {
        c.pos_right_of(me.eid)
    } else {
        me.pos
    };
    c.summon_card(me.side, pos, card, n);
}

/// Add `n` copies of a card to hand.
pub fn give_n(c: &mut Ctx, side: Side, card: &str, n: usize) {
    let id = cid(card);
    for _ in 0..n {
        c.give_card(side, id);
    }
}

pub fn buff_deity(c: &mut Ctx, side: Side, atk: i32, hp: i32) {
    if let Some(p) = c.player(side)
        && let Some(d) = p.deity.as_mut()
    {
        d.atk = d.atk.saturating_add(atk);
        d.hp = d.hp.saturating_add(hp);
    }
    // An awakened Deity on the board this combat gets the stats right away too.
    let e = c.cs[side].deity_eid;
    if c.combat && e != 0 && c.on_board(e) {
        c.buff(e, atk, hp);
    }
}

/// Tavern-spell style buff applied to a target (adds the owner's extra spell stats).
pub fn spell_buff(c: &mut Ctx, me: Me, t: EntityId, a: i32, h: i32) {
    c.spell_buff(me, t, a, h);
}

// ---------------------------------------------------------------------------- system hooks

/// Called after a deity is summoned (C'Thun splits its stats among other minions).
pub fn deity_awakened(c: &mut Ctx, me: Me) {
    // Evil Experiment: after your Deity awakens, give it Reborn.
    if has_trinket(c, me.side, "BG36_MagicItem_416") {
        c.give_kw(me.eid, Kw::REBORN);
    }
    if db().normal_of(me.card) != cid("BGFYM_000") {
        return;
    }
    let Some(m) = c.minion(me.eid) else { return };
    let mult = if me.golden { 2 } else { 1 };
    let (mut a, mut h) = (i64::from(m.atk) * mult, i64::from(m.hp) * mult);
    let others = c.friendly_where(me.side, |x| x.eid != me.eid);
    if others.is_empty() {
        return;
    }
    // Split evenly; the remainder goes one point each to random different minions.
    let n = others.len() as i64;
    let mut gains = vec![
        (
            (a / n).clamp(0, i32::MAX as i64) as i32,
            (h / n).clamp(0, i32::MAX as i64) as i32
        );
        others.len()
    ];
    a %= n;
    h %= n;
    for i in c.g.rng.sample_indices(others.len(), a as usize) {
        gains[i].0 = gains[i].0.saturating_add(1);
    }
    for i in c.g.rng.sample_indices(others.len(), h as usize) {
        gains[i].1 = gains[i].1.saturating_add(1);
    }
    for (e, (ga, gh)) in others.into_iter().zip(gains) {
        c.buff(e, ga, gh);
    }
}

/// Keep each transforming Discover's target with its menu, not in a single source counter.
/// Both Pending::Discover and its target context are LIFO; combat auto-picks immediately.
pub fn offer_targeted_transform(
    c: &mut Ctx,
    side: Side,
    source: CardId,
    target: EntityId,
    n: usize,
    filter: impl Fn(&crate::cards::CardDef) -> bool,
) {
    let options = c.minion_options(side, n, filter);
    if options.is_empty() {
        return;
    }
    let Some(p) = c.player(side) else { return };
    p.discover_targets.push((source, target));
    c.offer_discover(side, source, options);
}

/// Consume the target belonging to the top menu from this exact source.
pub fn take_transform_target(c: &mut Ctx, side: Side, source: CardId) -> Option<EntityId> {
    if let Some(p) = c.player(side)
        && let Some(i) = p.discover_targets.iter().rposition(|x| x.0 == source)
    {
        return Some(p.discover_targets.remove(i).1);
    }
    None
}

/// Discard-specific effects ("if you discard this...").
pub fn on_discard(c: &mut Ctx, side: Side, card: CardId) {
    let id = db().get(card).id.as_str();
    match id {
        // Sludge Corrosion / Energizing Chamber: cast twice when discarded.
        "BG36_301t" | "BG36_371" => {
            for _ in 0..2 {
                c.you_cast(side, card, 0, 0, 0);
            }
        }
        // Corrupted Coin: increase max Gold by 2.
        "BG36_303" => c.add_max_gold(side, 2),
        _ => {}
    }
}

/// Run and expire scheduled start-of-turn effects.
pub fn run_scheduled(c: &mut Ctx, side: Side) {
    let Some(p) = c.owner[side] else { return };
    let list = std::mem::take(&mut c.g.players[p].scheduled);
    let mut keep = vec![];
    for mut s in list {
        s.turns_left = s.turns_left.saturating_sub(1);
        if s.turns_left > 0 {
            keep.push(s);
            continue;
        }
        let won = c.g.players[p].stats.won_last;
        if s.if_won && !won {
            continue;
        }
        if let Some(fx) = script(s.script).and_then(|x| x.scheduled) {
            let me = Me {
                side,
                eid: s.target,
                card: s.script,
                golden: false,
                opt: 0,
                pos: 0,
                data: s.a,
            };
            let who = c.who(side);
            let name = &db().get(s.script).name;
            c.g.log(|| format!("{who} scheduled effect fires: {name}"));
            let prev_src = c.src.replace(me.card);
            fx(c, me, &s);
            c.src = prev_src;
        }
    }
    c.g.players[p].scheduled.extend(keep);
}

pub fn schedule(c: &mut Ctx, side: Side, s: Scheduled) {
    if let Some(p) = c.player(side) {
        p.scheduled.push(s);
    }
}

/// Start-of-turn processing for cards in hand: Lockbox countdowns and minions whose
/// `start_of_turn` also runs in hand (flag TURN_IN_HAND).
pub fn hand_start_of_turn(c: &mut Ctx, side: Side) {
    let Some(p) = c.owner[side] else { return };
    let lb = cid(LOCKBOX);
    // Lockboxes: count down, open at 0.
    let mut opened = 0;
    let hand = &mut c.g.players[p].hand;
    let mut i = 0;
    while i < hand.len() {
        let open = match &mut hand[i].item {
            Item::Spell { card, data } if *card == lb => {
                *data -= 1;
                *data <= 0
            }
            _ => false,
        };
        if open {
            hand.remove(i);
            opened += 1;
        } else {
            i += 1;
        }
    }
    for _ in 0..opened {
        open_lockbox(c, side);
    }
    // In-hand turn hooks.
    let cards: Vec<(EntityId, CardId, bool)> = c.g.players[p]
        .hand
        .iter()
        .filter(|h| h.locked == 0)
        .filter_map(|h| h.item.as_minion().map(|m| (h.hid, m.card, m.golden)))
        .filter(|(_, card, _)| {
            script(*card)
                .is_some_and(|s| s.has_flag(flag::TURN_IN_HAND) && s.start_of_turn.is_some())
        })
        .collect();
    for (hid, card, golden) in cards {
        let fx = script(card).and_then(|s| s.start_of_turn).unwrap();
        let prev_src = c.src.replace(card);
        fx(
            c,
            Me {
                side,
                eid: hid,
                card,
                golden,
                opt: 0,
                pos: 0,
                data: 0,
            },
        );
        c.src = prev_src;
    }
}

/// `Script::shown` of a Tavern spell that gives "+atk/+hp" through `Ctx::spell_buff`: the
/// two numbers with the "your Tavern spells give an extra" bonus.
pub fn shown_spell_stats(c: &Ctx, sh: &crate::script::Shown, atk: i32, hp: i32) -> Vec<i32> {
    let (a, h) = c.spell_extra(sh.side);
    vec![atk + a, hp + h]
}

/// Which announcement of a cast (`bit` = SPELL_CAST) or of a minion play (PLAYED / CARD_PLAYED) a
/// listener of `card` hears: true for "Whenever you cast / play..." cards, which act before the
/// spell's effect / the Battlecry; false for "After you..." and all other listeners, which act
/// after it. Read from the card's English text (Vicious Mindslasher and Hooktail act before the
/// spell, Corrupted Baton after it; Dragonwing Glider before the played minion's Battlecry).
pub fn acts_whenever(bit: u32, card: CardId) -> bool {
    static T: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| {
        db().iter()
            .map(|c| {
                let text = &db().get(db().normal_of(c.cid)).text;
                let cast =
                    text.contains("Whenever you cast") || text.contains("Whenever a spell is cast");
                let play = text.contains("Whenever you play") || text.contains("Whenever you use");
                u8::from(cast) | (u8::from(play) << 1)
            })
            .collect()
    });
    let flags = t.get(card.idx()).copied().unwrap_or(0);
    if bit == crate::event::ev::SPELL_CAST {
        flags & 1 != 0
    } else {
        flags & 2 != 0
    }
}

/// The targets an effect may pick when it casts `spell` for you: the spell's own, except that
/// Chef's Choice -- which by hand can only be aimed at a minion with a type -- can land on a
/// typeless minion when an effect casts it (it then gives a Tavern Coin).
pub fn effect_target_spec(spell: CardId) -> Option<TargetSpec> {
    let s = script(spell)?;
    Some(if db().get(spell).id == "BG28_518" {
        TargetSpec::FriendlyOrTavern
    } else {
        s.spell_target
    })
}

/// Whether a minion counts as "of your type `t`" for offers that follow the warband (the Dark Gift
/// button): it has the type, or it is a typeless minion tied to the type in the client data
/// (`assoc`: Nadina the Red for Dragons, Disguised Graverobber for Undead). General neutral minions
/// that merely work well with a type (Brann Bronzebeard, Balinda) do not count.
pub fn goes_with_type(d: &crate::cards::CardDef, t: Races) -> bool {
    d.races.intersects(t) || (d.races.is_none() && d.assoc.intersects(t))
}

/// The Hero Powers "Discover a Hero Power" draws from: those of the hero pool's heroes
/// that belong in this lobby (no power of a banned hero or of a hero whose minion type is
/// absent: "Get a Pirate" without Pirates).
pub fn hero_power_pool(g: &Game) -> Vec<CardId> {
    let mut v: Vec<CardId> = g.lobby_heroes().filter_map(|c| c.hero_power).collect();
    v.sort();
    v.dedup();
    v
}

/// Hero Power usage limits parsed from its text: "(Unlocks at Tier N)", "(Unlocks on Turn
/// N)", "(Once per game)".
#[derive(Clone, Copy, Default)]
struct HpLimits {
    tier: u8,
    turn: u32,
    once: bool,
    /// Card-specific availability rule (see `hero_power_available`); 0 = none.
    special: u8,
    /// The power only does something at the start of the game ("Start the game with",
    /// "At the start of the game", "Skip your first"): pointless to gain later.
    start_only: bool,
    /// "On Turn N,...": pointless to gain on Turn N or later (0: none).
    on_turn: u32,
}

/// Per-card rule ids for `HpLimits::special`, resolved from the card ids once.
const HP_SPECIAL: [(&str, u8); 18] = [
    ("TB_BaconShop_HP_040", 7), // Pyramad: needs a Tavern minion (official 36.6 fix)
    ("TB_BaconShop_HP_022", 1), // Shudderwock: unlocks on Turn 3 (not in its text)
    ("BG20_HERO_283p", 2),      // Galewing: only with no flightpath in progress
    ("BG25_HERO_100p", 3),      // Professor Putricide: 3 Creations per game
    ("TB_BaconShop_HP_102", 9), // Zephrys: 3 Wishes per game; needs a pair
    ("TB_BaconShop_HP_077", 8), // Tess: the last opponent had minions (official 27.0)
    // Card-taking powers need room in the hand.
    ("BG20_HERO_101p", 6),      // Xyrella
    ("TB_BaconShop_HP_068", 6), // Maiev
    ("TB_BaconShop_HP_049", 6), // Lich Baz'hial
    // Unavailable with nothing to act on (simulator assumption).
    ("TB_BaconShop_HP_081", 10), // Lord Barov: a combat to bet on
    ("TB_BaconShop_HP_020", 11), // Akazamzarak: a Secret not in play
    ("TB_BaconShop_HP_084", 12), // Jandice: a non-Golden minion and a Tavern minion
    ("BG20_HERO_201p", 13),      // Vol'jin: two minions
    ("BG23_HERO_306p", 14),      // Sylvanas: a minion died last combat
    ("BG31_HERO_005p", 4),       // Clone Exhibit: needs board space
    ("BG21_HERO_010p", 5),       // Spy Scouting: needs an opponent warband to copy
    // Murloc Holmes: the next opponent must have had a warband last combat (not on Turn 1).
    ("BG23_HERO_303p2", 15),
    ("TB_BaconShop_HP_011", 0), // Galakrond replaces a Tavern minion; no hand slot is used
];

fn hp_limits(hp: CardId) -> HpLimits {
    // Indexed by CardId: this runs for every hero power slot of every action-mask build.
    static T: std::sync::OnceLock<Vec<HpLimits>> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| {
        let num_after = |text: &str, key: &str| -> Option<u32> {
            let i = text.find(key)? + key.len();
            text[i..]
                .trim_start()
                .split(|ch: char| !ch.is_ascii_digit())
                .next()?
                .parse()
                .ok()
        };
        db().iter()
            .map(|c| {
                if c.kind != crate::cards::CardKind::HeroPower {
                    return HpLimits::default();
                }
                HpLimits {
                    tier: num_after(&c.text, "Unlocks at Tier").unwrap_or(0) as u8,
                    turn: num_after(&c.text, "Unlocks on Turn").unwrap_or(0),
                    once: c.text.contains("Once per game"),
                    start_only: [
                        "Start the game",
                        "At the start of the game",
                        "Skip your first",
                    ]
                    .iter()
                    .any(|k| c.text.contains(k)),
                    on_turn: num_after(&c.text, "On Turn").unwrap_or(0),
                    special: HP_SPECIAL
                        .iter()
                        .find(|(id, _)| *id == c.id)
                        .map_or(0, |x| x.1),
                }
            })
            .collect()
    });
    t.get(hp.idx()).copied().unwrap_or_default()
}

/// The Tavern Tier a hero power unlocks at (0: none).
pub fn hero_power_unlock_tier(hp: CardId) -> u8 {
    hp_limits(hp).tier
}

/// Whether "Discover a Hero Power" can still offer this power on `turn`: not when its moment has
/// passed. Start-of-game powers (A. F. Kay, N'Zoth, Sneed, The Curator, Patchwerk...) are only
/// offered before the first turn; "On Turn N" powers (Marin, Buttons, Genn, Morchie, Murozond) only
/// before that turn.
pub fn hero_power_in_time(hp: CardId, turn: u32) -> bool {
    let l = hp_limits(hp);
    !(l.start_only && turn >= 1) && !(l.on_turn > 0 && turn >= l.on_turn)
}

/// Extra availability rules for hero powers (unlocks, once per game...).
pub fn hero_power_available(g: &Game, p: PlayerId, hp: CardId) -> bool {
    let pl = &g.players[p];
    let l = hp_limits(hp);
    if pl.tier < l.tier || g.turn < l.turn {
        return false;
    }
    if l.once && pl.power_of(hp).is_some_and(|h| h.uses_game > 0) {
        return false;
    }
    match l.special {
        1 => g.turn >= 3,
        2 => pl.power_of(hp).is_some_and(|h| h.counter <= 0),
        3 => pl.power_of(hp).is_some_and(|h| h.uses_game < 3),
        4 => pl.board.len() < g.cfg.board_size,
        6 => pl.hand.len() < g.cfg.hand_size,
        7 => pl.shop.iter().any(|s| s.item.as_minion().is_some()),
        8 => g
            .last_fought(p)
            .is_some_and(|o| !g.fought_board(o).is_empty()),
        9 => {
            pl.power_of(hp).is_some_and(|h| h.uses_game < 3)
                && !heroes_gen::pairs_of(&pl.board, &pl.hand).is_empty()
        }
        // A combat to bet on: a ghost pairing only counts once someone was eliminated.
        10 => g
            .pairs
            .iter()
            .any(|(_, b)| b.is_some() || !g.eliminated.is_empty()),
        11 => secrets::secret_left(pl),
        12 => {
            pl.board.iter().any(|m| !m.golden)
                && pl.shop.iter().any(|s| s.item.as_minion().is_some())
        }
        14 => pl.power_of(hp).is_some_and(|h| !h.memory.is_empty()),
        13 => {
            let tavern = pl.shop.iter().filter(|s| s.item.as_minion().is_some());
            pl.board.len() + tavern.count() >= 2
        }
        // The next opponent's warband as it last fought (`Game::fought_board`); a ghost
        // opponent shows the last eliminated player's warband.
        5 | 15 => match pl.next_opponent {
            Some(o) => !g.fought_board(o).is_empty(),
            None => {
                pl.next_is_ghost
                    && g.eliminated
                        .last()
                        .is_some_and(|e| !g.fought_board(*e).is_empty())
            }
        },
        _ => true,
    }
}

/// Remove a hand card that only stood for a choice; its pool copies go back. "Whenever you cast a
/// spell on this": `e` is a spell that `me`'s side played from hand with `me` as the target it
/// chose - and each time that play is cast (Balinda, Temporal Tampering). A cast an effect makes
/// for you with a target of its own (Victorious Geomant's Blood Gems that do not fit, a random
/// spell's random target, a trinket's Repair Job) is your cast for the spell counters, but you did
/// not aim it.
pub fn aimed_at_me(c: &Ctx, me: Me, e: &Event) -> bool {
    c.cast_from_hand
        && matches!(e, Event::SpellCast { side, target, .. }
            if *side == me.side && *target == me.eid)
}

/// Hold the hand card `hid`: locked until its own condition lets go of it
/// (`HandCard::held`, `release_ready_held`) - `gold` more Gold spent, or with 0 the Tavern
/// reaching its Tier.
fn hold_in_hand(c: &mut Ctx, side: Side, hid: EntityId, gold: i32) -> bool {
    let card = c
        .player(side)
        .and_then(|p| p.hand.iter_mut().find(|h| h.hid == hid));
    match card {
        Some(h) => {
            h.locked = 1;
            h.held = true;
            h.held_gold = gold.max(0);
            true
        }
        None => false,
    }
}

/// Let go of every held hand card whose condition is met: its Gold spent (`spent` = the Gold just
/// spent), or its Tavern Tier reached. Called when Gold is spent, when the Tavern is upgraded and
/// at the start of a turn - whatever Hero Power the player has by then (Faelin's Tier 6 minion and
/// Thorim's Tier 7 one stayed locked for good after a power swap).
pub fn release_ready_held(c: &mut Ctx, side: Side, spent: i32) {
    let Some(p) = c.owner[side] else { return };
    let pl = &mut c.g.players[p];
    if !pl.hand.iter().any(|h| h.held) {
        return;
    }
    let tier = pl.tier;
    let mut ready: Vec<CardId> = vec![];
    for h in pl.hand.iter_mut().filter(|h| h.held) {
        let free = if h.held_gold > 0 {
            h.held_gold = (h.held_gold - spent).max(0);
            h.held_gold == 0
        } else {
            db().get(h.item.card()).tier <= tier
        };
        if free {
            h.held = false;
            h.locked = 0;
            ready.push(h.item.card());
        }
    }
    if ready.is_empty() {
        return;
    }
    // The power that gave them, if the player still has it, stops waiting for them.
    for card in ready {
        for hp in pl.powers.iter_mut().filter(|h| {
            matches!(
                db().get(h.card).id.as_str(),
                "BG22_HERO_201p" | "BG27_HERO_801p2"
            )
        }) {
            if let Some(i) = hp.memory.iter().position(|x| *x == card) {
                hp.memory.remove(i);
                break;
            }
        }
    }
    if !c.combat {
        c.check_triples(side);
    }
}

/// Let go of the held hand card `card`: it can be played, and take part in a triple, from
/// now on. When no such card is held (it never reached the hand) the card is given now
/// instead.
pub fn release_held(c: &mut Ctx, side: Side, card: CardId) {
    let held = c
        .player(side)
        .and_then(|p| p.hand.iter_mut().find(|h| h.held && h.item.card() == card));
    match held {
        Some(h) => {
            h.held = false;
            h.locked = 0;
            if !c.combat {
                c.check_triples(side);
            }
        }
        None => {
            c.give_card(side, card);
        }
    }
}

fn drop_from_hand(c: &mut Ctx, side: Side, hid: EntityId) {
    let Some(pl) = c.player(side) else { return };
    let Some(i) = pl.hand.iter().position(|x| x.hid == hid) else {
        return;
    };
    let h = pl.hand.remove(i);
    if let Item::Minion(m) = h.item {
        c.g.pool.return_minion(&m);
    }
}

/// Resolve menu descriptors directly, without pretending they entered the hand.
pub fn consume_discover_choice(
    c: &mut Ctx,
    side: Side,
    source: CardId,
    mut item: Item,
) -> Option<Item> {
    let card = item.card();
    let id = db().get(db().normal_of(source)).id.as_str();
    if id == "TB_BaconShop_HP_011" {
        if let Item::Minion(m) = &mut item {
            c.take_pool_copy(m);
            c.apply_tavern_buffs(side, m);
        }
        let pos = c
            .power_mut(side, source)
            .map_or(0, |h| h.counter.max(0) as usize);
        let sid = item.as_minion().map_or_else(|| c.g.alloc_eid(), |m| m.eid);
        if let Some(p) = c.player(side) {
            p.shop.insert(
                pos.min(p.shop.len()),
                crate::state::ShopSlot::new(sid, item),
            );
        }
        return None;
    }
    let virtual_pick = matches!(id, putricide::STEP1 | putricide::STEP2 | "BG23_HERO_303p2")
        || (id == "BG20_HERO_283p"
            && matches!(
                db().get(card).id.as_str(),
                "BG20_HERO_283p_t1" | "BG20_HERO_283p_t2" | "BG20_HERO_283p_t3"
            ));
    if virtual_pick {
        let data = if let Item::Spell { data, .. } = item {
            data
        } else {
            0
        };
        after_discover(c, side, source, card, None, data);
        None
    } else {
        Some(item)
    }
}

/// After a Discover from `source` resolved with `card` (Imposing Percussionist).
/// `hid` is the hand id the pick got (None: it never entered the hand, e.g. a full hand).
pub fn after_discover(
    c: &mut Ctx,
    side: Side,
    source: CardId,
    card: CardId,
    hid: Option<EntityId>,
    data: i32,
) {
    let turn = c.g.turn;
    // The Discovered card: exactly the one that was added (not another copy in hand). It may
    // have been merged away by a triple, in which case it is no longer there.
    let last = |c: &mut Ctx| -> Option<EntityId> {
        let hid = hid?;
        c.player_ref(side)
            .and_then(|p| {
                p.hand
                    .iter()
                    .find(|h| h.hid == hid && h.item.card() == card)
            })
            .map(|h| h.hid)
    };
    // Take the pick back: it is not a card to keep. With a full hand the pick of a trinket's
    // or Quest Reward's Discover waits in the hand queue instead of being in the hand.
    let protected = crate::engine::hand_protected(source);
    let take = |c: &mut Ctx| {
        if let Some(h) = last(c) {
            drop_from_hand(c, side, h);
        } else if hid.is_none() && protected {
            let queued = c.player(side).and_then(|pl| {
                let i = pl.hand_queue.iter().rposition(|x| x.card() == card)?;
                Some(pl.queue_take(i).0)
            });
            if let Some(Item::Minion(m)) = queued {
                c.g.pool.return_minion(&m);
            }
        }
    };
    match db().get(db().normal_of(source)).id.as_str() {
        // Imposing Percussionist: deal damage to your hero equal to its Tier (the minion
        // deals it: Jaraxxus counts it).
        "BG26_525" => {
            let prev = c.src.replace(source);
            c.hero_damage(side, db().get(card).tier as i32);
            c.src = prev;
        }
        // Search Through Time / Timewarped Beanstalk: lock it in your hand for 1 turn.
        "BG34_330" | "BG34_Treasure_301" => {
            if let Some(h) = last(c)
                && let Some(x) = c
                    .player(side)
                    .and_then(|p| p.hand.iter_mut().find(|x| x.hid == h))
            {
                x.locked = 1;
            }
        }
        // Token of the Old Gods: the chosen minion transforms into the pick.
        "BG30_MagicItem_416t" => {
            take(c);
            if let Some(target) = take_transform_target(c, side, source) {
                trinkets_p8::transform_any(c, side, target, card);
            }
        }
        // Portable Factory: remember the pick (a copy of it each turn).
        "BG32_MagicItem_361" | "BG32_MagicItem_361t" => {
            c.set_card_data(side, source, card.0 as i32);
        }
        // Galakrond: the Discovered minion replaces the chosen one in the Tavern.
        "TB_BaconShop_HP_011" => {
            let pos = c
                .power_mut(side, source)
                .map_or(0, |h| h.counter.max(0) as usize);
            if let Some(h) = last(c)
                && let Some(pl) = c.player(side)
                && let Some(i) = pl.hand.iter().position(|x| x.hid == h)
            {
                let mut x = pl.hand.remove(i);
                // It arrives as a Tavern minion: Tavern buffs apply (like Malygos / Toki).
                if let Item::Minion(m) = &mut x.item {
                    c.apply_tavern_buffs(side, m);
                }
                if let Some(pl) = c.player(side) {
                    let pos = pos.min(pl.shop.len());
                    pl.shop
                        .insert(pos, crate::state::ShopSlot::new(x.hid, x.item));
                }
            }
        }
        // Bru'kan: the chosen Element is remembered (not kept in hand).
        "BG22_HERO_001p" => {
            take(c);
            let k = [
                "BG22_HERO_001p_t1_s",
                "BG22_HERO_001p_t2_s",
                "BG22_HERO_001p_t3_s",
                "BG22_HERO_001p_t4_s",
            ]
            .iter()
            .position(|e| cid(e) == card)
            .map_or(0, |i| i as i32 + 1);
            if let Some(hp) = c.power_mut(side, source) {
                hp.counter = k;
            }
        }
        // Murloc Holmes: the pick is a guess, not a card; right = a Tavern Coin.
        "BG23_HERO_303p2" => {
            take(c);
            let right = c.power_mut(side, source).and_then(|h| h.memory.pop());
            if right == Some(card) {
                give_n(c, side, TAVERN_COIN, 1);
                // Watfin: get a plain copy (golden: 2) of the minion.
                if db().get(card).id != "BG23_HERO_303pt" {
                    for _ in 0..buddy_power(c, side, "BG23_HERO_303_Buddy") {
                        c.give_card(side, db().normal_of(card));
                    }
                }
            }
        }
        // Lord Barov: the pick is the guessed winner (a hero card).
        "TB_BaconShop_HP_081" => {
            take(c);
            let guessed = c.g.players.iter().position(|pl| pl.hero == Some(card));
            let alive = guessed.is_some_and(|q| c.g.players[q].alive);
            if let Some(hp) = c.power_mut(side, source) {
                // Picking the ghost (a dead hero) = guessing that its opponent loses.
                hp.counter = match guessed {
                    Some(q) if alive => q as i32 + 1,
                    Some(_) => -(hp.values.first().copied().unwrap_or(0) + 1),
                    None => 0,
                };
            }
        }
        // Ambassador Faelin: the picks are held until you reach their Tier - in the hand, locked,
        // from the start.
        "BG22_HERO_201p" => {
            if !last(c).is_some_and(|h| hold_in_hand(c, side, h, 0)) {
                take(c);
            }
            if let Some(hp) = c.power_mut(side, source) {
                hp.memory.push(card);
            }
        }
        // Ethereal Evidence: the pick becomes your current extra Reward.
        "BG24_Reward_363" => {
            take(c);
            let prev = c.card_data(side, source);
            let Some(p) = c.player(side) else { return };
            if prev > 0 {
                let old = CardId((prev - 1) as u16);
                if let Some(i) = p.rewards.iter().position(|r| *r == old) {
                    p.rewards.remove(i);
                }
            }
            p.rewards.push(card);
            c.set_card_data(side, card, data);
            c.set_card_data(side, source, card.0 as i32 + 1);
            if let Some(f) = script(card).and_then(|s| s.on_attach) {
                let me = Me {
                    side,
                    eid: 0,
                    card,
                    golden: false,
                    opt: 0,
                    pos: 0,
                    data,
                };
                // As when a Quest gives it: its cards have the Reward's hand protection.
                let prev_src = c.src.replace(card);
                f(c, me);
                c.src = prev_src;
            }
        }
        // Magicfin Relic: the Discovered spell is taught to the newest untaught Apprentice.
        "BG33_Reward_011" => {
            let apprentice = cid("BG33_890t");
            let target = c.player_ref(side).and_then(|p| {
                p.hand
                    .iter()
                    .rev()
                    .find(|h| {
                        h.item
                            .as_minion()
                            .is_some_and(|m| m.card == apprentice && m.counter == 0)
                    })
                    .map(|h| h.hid)
            });
            take(c);
            if let Some(t) = target
                && let Some(m) = c.find_any_mut(t)
            {
                m.counter = card.0 as i32 + 1;
            } else if let Some(m) = c.player(side).and_then(|pl| {
                // A full hand: the Apprentice is waiting for space too.
                pl.hand_queue
                    .iter_mut()
                    .rev()
                    .filter_map(|x| x.as_minion_mut())
                    .find(|m| m.card == apprentice && m.counter == 0)
            }) {
                m.counter = card.0 as i32 + 1;
            }
        }
        // The Great Akazamzarak: the chosen Secret goes into play, not to hand.
        "TB_BaconShop_HP_020" => {
            take(c);
            if let Some(p) = c.player(side) {
                if p.secrets.len() < crate::state::MAX_SECRETS
                    && !p.secrets.iter().any(|s| secrets::same_secret(*s, card))
                {
                    p.secrets.push(card);
                }
                // Golden Street Magician: the queued second pick loses this Secret.
                for q in p.pending.iter_mut() {
                    if let crate::state::Pending::Discover { source: s, options } = q
                        && *s == source
                    {
                        options.retain(|o| !secrets::same_secret(o.card(), card));
                    }
                }
                p.pending.retain(|q| {
                    !matches!(q, crate::state::Pending::Discover { source: s, options }
                        if *s == source && options.is_empty())
                });
            }
        }
        // Thorim: the Tier 7 pick waits until 60 Gold are spent.
        // Putricide: the two Build-An-Undead steps (the pick isn't kept as a card).
        putricide::STEP1 | putricide::STEP2 => {
            take(c);
            let step1 = db().get(db().normal_of(source)).id == putricide::STEP1;
            putricide::picked(c, side, step1, card);
        }
        // Thorim: the Tier 7 minion is in the hand from the start, locked until 60 Gold is spent.
        "BG27_HERO_801p2" => {
            // 60 Gold from when the power started counting.
            let left = 60 - c.power_mut(side, source).map_or(0, |h| h.counter);
            if !last(c).is_some_and(|h| hold_in_hand(c, side, h, left.max(1))) {
                take(c);
            }
            if let Some(hp) = c.power_mut(side, source) {
                hp.memory.push(card);
            }
        }
        // Tras'tath: the pick stays locked in hand until Turn 7.
        "BG36_HERO_101p" => {
            if let Some(h) = last(c)
                && let Some(x) = c
                    .player(side)
                    .and_then(|p| p.hand.iter_mut().find(|x| x.hid == h))
            {
                x.locked = 7u32.saturating_sub(turn) as u8;
            }
        }
        // Galewing: the chosen flightpath starts (turns = its number).
        "BG20_HERO_283p" => {
            let turns = match db().get(card).id.as_str() {
                "BG20_HERO_283p_t1" => 1,
                "BG20_HERO_283p_t2" => 2,
                "BG20_HERO_283p_t3" => 3,
                // The minion Discovered at the end of Eastern Plaguelands comes from the
                // same Hero Power: it is the reward, not a flightpath, and stays in hand.
                _ => 0,
            };
            if turns > 0 {
                take(c);
                if let Some(hp) = c.power_mut(side, source) {
                    hp.memory = vec![card];
                    hp.counter = turns;
                }
            }
        }
        // Cariel Roame: the chosen improvement applies to Conviction.
        "BG21_HERO_000p" => {
            take(c);
            heroes_special::improve_conviction(c, side, card);
        }
        // Artanis: the Protoss pick waits until 14 cards are bought.
        "BG31_HERO_802p" => {
            take(c);
            if let Some(hp) = c.power_mut(side, source) {
                hp.memory.push(card);
            }
        }
        // Kerrigan: the pick is the form a Zerg on the board morphs into.
        "BG31_HERO_811p" | "BG31_HERO_811p2" | "BG31_HERO_811p3" => {
            take(c);
            let eid = c.power_mut(side, source).and_then(|h| h.values.pop());
            if let Some(eid) = eid {
                starcraft::morph(c, eid as EntityId, db().normal_of(card));
            }
        }
        // Tomb Turning: it dies if you play it this turn.
        "BG34_888" => {
            if let Some(h) = last(c)
                && let Some(m) = c.find_any_mut(h)
            {
                m.doomed_turn = turn;
            }
        }
        _ => {}
    }
}

/// After player `dead` is eliminated (their warband before it returned to the pool).
pub fn on_eliminated(g: &mut Game, dead: PlayerId, board: Vec<Minion>) {
    let kitty = cid("TB_BaconShop_HP_080");
    for q in 0..g.players.len() {
        if q == dead || !g.players[q].alive || g.players[q].power_of(kitty).is_none() {
            continue;
        }
        let board = board.clone();
        g.with_recruit(q, |c| {
            // Mr. Bigglesworth: Discover a minion from their warband, enchantments kept.
            if board.is_empty() {
                give_n(c, 0, "TB_BaconShop_HP_080t", 1);
                return;
            }
            let picks = c.g.rng.sample_indices(board.len(), 3);
            let options = picks
                .into_iter()
                .map(|i| {
                    let mut m = board[i].clone();
                    m.eid = c.g.alloc_eid();
                    m.pool_copies = 0;
                    Item::Minion(m)
                })
                .collect();
            c.offer_discover(0, kitty, options);
        });
    }
}

/// Quest-like Hero Powers: a long count (attacks, Tiers or cards bought, damage, turns) and
/// then the payoff. Master Nguyen holds a power for one turn and is never offered them.
const QUEST_POWERS: [&str; 9] = [
    "TB_BaconShop_HP_065", // Aranna: after 14 friendly minions attack
    "BG20_HERO_242p",      // Guff: after you buy 20 Tiers' worth of cards
    "BG33_HERO_001p_ALT",  // Loh: after 15 friendly minions attack
    "TB_BaconShop_HP_087", // Ragnaros: after you buy 12 cards
    "TB_BaconShop_HP_036", // Jaraxxus: after friendly minions deal 150 damage
    "TB_BaconShop_HP_048", // Dinotamer Brann: after you buy 4 Battlecry minions
    "BG26_HERO_104p",      // Voone: at the end of every 3 turns
    "BG36_HERO_105p",      // Xavius: every 4 turns
    "TB_BaconShop_HP_106", // Tickatus: every 4 turns
];

/// Hero Powers Master Nguyen is not offered on `turn`: the quest-like ones and those still locked
/// on that turn ("Unlocks on Turn N": no Turn 3 power on Turn 2).
pub fn nguyen_excluded(turn: u32) -> Vec<CardId> {
    db().iter()
        .filter(|c| c.kind == crate::cards::CardKind::HeroPower)
        .filter(|c| {
            let l = hp_limits(c.cid);
            l.turn > turn || (l.special == 1 && turn < 3) || QUEST_POWERS.contains(&c.id.as_str())
        })
        .map(|c| c.cid)
        .collect()
}

/// Start of turn effects that survive Hero Power swaps (Power of the Storm).
pub fn hero_start_of_turn(c: &mut Ctx, side: Side) {
    power_of_the_storm(c, side);
}

/// Power of the Storm: "At the start of every turn, choose from 2 new Hero Powers." The
/// choice sits on top of the Storm for the turn, in the slot the Storm was put in; the Storm
/// itself stays underneath (`counters`: slot + 1), so the next turn offers again - as the
/// Mystery Cube stays under the trinket of the turn. It ends when a power swap replaces
/// that slot (`storm_replaced`; `counters`: -1). Once a turn.
pub fn power_of_the_storm(c: &mut Ctx, side: Side) {
    let storm = cid("BG20_HERO_202p");
    let Some(pl) = c.player_ref(side) else { return };
    let slot = match c.card_data(side, storm) {
        // A state from before the slot was recorded: Master Nguyen himself, slot 0.
        0 if pl.hero_is("BG20_HERO_202") => 0,
        n if n <= 0 => return,
        n => (n - 1) as usize,
    };
    if c.turn_data(side, storm) > 0 {
        return;
    }
    c.add_turn_data(side, storm, 1);
    let mut out = nguyen_excluded(c.g.turn);
    out.push(storm);
    let prev = std::mem::replace(&mut c.storm_offer, true);
    c.offer_hero_powers(side, 2, slot, &out);
    c.storm_offer = prev;
}

/// Another effect puts a Hero Power into `slot` (a "Discover a Hero Power" swap, a Power of X
/// spell): if Power of the Storm sat there it is gone for good - what the player has is no longer
/// Master Nguyen's power.
pub fn storm_replaced(c: &mut Ctx, side: Side, slot: usize) {
    let storm = cid("BG20_HERO_202p");
    let at = c.card_data(side, storm);
    let nguyen = c
        .player_ref(side)
        .is_some_and(|p| p.hero_is("BG20_HERO_202"));
    if at == slot as i32 + 1 || (at == 0 && slot == 0 && nguyen) {
        c.set_card_data(side, storm, -1);
    }
}

/// The Hero Power named `name` (case-insensitive), e.g. for "Power of X" spells.
pub fn hero_power_named(name: &str) -> Option<CardId> {
    // Several powers can share a name ("Reclaimed Souls", "Tavern Lighting"): prefer one that
    // is implemented, so a Power of X spell never hands out a dead button.
    let mut named = db().iter().filter(|c| {
        c.kind == crate::cards::CardKind::HeroPower && c.name.eq_ignore_ascii_case(name)
    });
    let first = named.next()?;
    std::iter::once(first)
        .chain(named)
        .find(|c| script(c.cid).is_some())
        .map(|c| c.cid)
        .or(Some(first.cid))
}

// ---------------------------------------------------------------------------- persistent

/// "After the Tavern is Refreshed this game, give a random minion in it +X/+X" (per stack). One
/// minion per Refresh takes all of it: every stack of the effect and every effect of this kind -
/// Easterly Winds, Waveling, En-Djinn Blazer - add up on the same random minion.
pub fn refresh_random_tavern_minion(c: &mut Ctx, me: Me, e: &Event, a: i32, h: i32) {
    if !matches!(e, Event::Refreshed { side, .. } if *side == me.side) {
        return;
    }
    let seq = c.emit_seq;
    c.event_picks.retain(|x| x.0 == seq);
    let t = match c.event_picks.first() {
        Some(x) => x.2,
        None => {
            let cands = c.tavern_minions(me.side);
            let Some(&t) = c.g.rng.pick(&cands) else {
                return;
            };
            c.event_picks.push((seq, me.card, t));
            t
        }
    };
    // A Tavern spell's effect (Easterly Winds) gets the Tavern-spell extra.
    let (ea, eh) = if is_tavern_spell(me.card) {
        c.spell_extra(me.side)
    } else {
        (0, 0)
    };
    c.buff(t, a.saturating_add(ea), h.saturating_add(eh));
}

/// Persistent refresh effect script: fires `f` per stack after each refresh.
pub const fn refresh_effect(f: crate::script::FxE) -> Script {
    Script {
        persistent_event: Some(f),
        persistent_listens: ev::REFRESHED,
        ..E
    }
}

// ---------------------------------------------------------------------------- counters

/// "Whenever / After you spend N Gold": accumulate on the minion's counter and fire once per
/// completed N (a single large spend can fire several times).
pub fn gold_counter(c: &mut Ctx, me: Me, e: &Event, every: i32, fx: fn(&mut Ctx, Me)) {
    let Event::GoldSpent { side, amount } = e else {
        return;
    };
    if *side != me.side {
        return;
    }
    // Player-level cards (trinkets, rewards) keep the progress in their card data.
    let total = if me.eid == 0 {
        let total = c.card_data(me.side, me.card) + amount;
        c.set_card_data(me.side, me.card, total % every);
        total
    } else {
        let Some(m) = c.minion_mut(me.eid) else {
            return;
        };
        let total = m.counter + amount;
        m.counter = total % every;
        total
    };
    for _ in 0..(total / every) {
        fx(c, me);
    }
}

// ---------------------------------------------------------------------------- Lockbox

pub const LOCKBOX: &str = "BG36_520t";
pub const LOCKBOX_TURNS: i32 = 5;
pub const ESCAPEE_PORTRAIT: &str = "BG36_MagicItem_363";

/// "Its type" of a chosen minion: an Amalgam (all types) counts as one random type of the lobby;
/// other minions keep their types.
pub fn target_type(c: &mut Ctx, races: Races) -> Races {
    if races != Races::ALL {
        return races;
    }
    let lobby: Vec<Races> = c.g.lobby_races.iter().collect();
    c.g.rng.pick(&lobby).copied().unwrap_or(races)
}

/// Whether `side`'s player owns trinket `id`.
pub fn has_trinket(c: &Ctx, side: Side, id: &str) -> bool {
    c.player_ref(side).is_some_and(|p| p.has_trinket(id))
}

/// "Get a Lockbox. If you already have one, it opens N turns sooner instead."
pub fn lockbox_or_hasten(c: &mut Ctx, side: Side, sooner: i32) {
    let Some(p) = c.owner[side] else { return };
    let lb = cid(LOCKBOX);
    let existing = c.g.players[p].hand.iter().position(|h| h.item.card() == lb);
    match existing {
        Some(i) => {
            if let Item::Spell { data, .. } = &mut c.g.players[p].hand[i].item {
                *data -= sooner;
            }
            let left = match &c.g.players[p].hand[i].item {
                Item::Spell { data, .. } => *data,
                _ => 1,
            };
            if left <= 0 {
                let h = c.g.players[p].hand.remove(i);
                let _ = h;
                open_lockbox(c, side);
            }
        }
        None => {
            // Escapee Portrait: your Lockboxes open 1 turn sooner, for each copy (a Souvenir
            // Stand copy is a second Portrait, as each shortens the Lockboxes you hold).
            let portraits = c
                .player_ref(side)
                .map_or(0, |p| p.trinket_count(cid(ESCAPEE_PORTRAIT)) as i32);
            c.add_to_hand(
                side,
                Item::Spell {
                    card: lb,
                    data: LOCKBOX_TURNS - portraits,
                },
            );
        }
    }
}

/// Break a Lockbox: a random Golden minion with a type (tier <= yours; it gives its Triple
/// Reward).
pub fn open_lockbox(c: &mut Ctx, side: Side) {
    let tier = c.tier(side);
    // Never Aureate Laureate, the Pirate that is "always Golden".
    let Some(card) =
        c.random_minion(|d| !d.races.is_none() && d.tier <= tier && d.id != "BG32_236")
    else {
        return;
    };
    // A Golden minion gives its Triple Reward when played unless a text says otherwise.
    let mut m = c.new_minion(side, db().golden_of(card));
    m.triple_reward = true;
    c.take_pool_copy(&mut m);
    let who = c.who(side);
    let name = m.name();
    c.g.log(|| format!("{who} opens a Lockbox: golden {name}"));
    c.add_to_hand(side, Item::Minion(m));
}

// ---------------------------------------------------------------------------- Blood Gems

pub const VOLUMIZERS: [&str; 3] = ["BG34_170t", "BG34_170t2", "BG34_170t3"];
pub const CHROMADRAKES: [&str; 5] = [
    "BG34_634t",
    "BG34_635t",
    "BG34_636t",
    "BG34_637t",
    "BG34_638t",
];

/// The Bounty Tavern spells (bundle view `bounty_spell_candidates`).
pub const BOUNTIES: [&str; 5] = ["BG33_811", "BG33_812", "BG33_813", "BG33_814", "BG33_815"];

pub fn is_bounty(card: CardId) -> bool {
    BOUNTIES.contains(&db().get(card).id.as_str())
}

// ---------------------------------------------------------------------------- Spellcraft

/// 2 for golden Spellcraft spells (`..._Gt`, `..._t_G`), else 1.
pub fn sc_g(me: Me) -> i32 {
    let id = db().get(me.card).id.as_str();
    if id.contains("_Gt") || id.ends_with("_G") {
        2
    } else {
        1
    }
}

/// A random (normal) Spellcraft spell; spells from any source are temporary.
pub fn give_random_spellcraft(c: &mut Ctx, side: Side, n: i32) {
    static IDS: std::sync::OnceLock<Vec<CardId>> = std::sync::OnceLock::new();
    let ids = IDS.get_or_init(|| {
        // The Spellcraft spells of pool minions (normal forms): no reward / trinket / Duos /
        // Timewarp / rotated spells, no golden variants.
        let mut v: Vec<CardId> = db()
            .iter()
            .filter(|d| d.pools.minion || d.pools.tier7)
            .filter_map(|d| d.spellcraft)
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    });
    for _ in 0..n {
        if let Some(&s) = c.g.rng.pick(ids) {
            c.give_temp_spell(side, s, 0);
        }
    }
}

/// Whether the Spellcraft spell being cast on `target` is permanent instead of "until next
/// turn": Sunken Persistence / Thaumaturgist Portrait / Weary Portrait on the source minion,
/// or a Lava Lurker use (spells played from hand only).
fn sc_permanent(c: &mut Ctx, side: Side, target: EntityId) -> bool {
    let src = c.cast_src;
    if let Some(m) = c.minion(src) {
        let nid = db().get(db().normal_of(m.card)).id.as_str();
        let sunken = m.has_attached("BG36_MidGameEffect_000t62");
        let pl = c.player_ref(side);
        let portrait = |t: &str| pl.is_some_and(|p| p.has_trinket(t));
        if sunken
            || (nid == "BG31_924" && portrait("BG32_MagicItem_920"))
            || (nid == "BG31_830" && portrait("BG32_MagicItem_933"))
        {
            return true;
        }
    }
    if c.cast_from_hand
        && let Some(m) = c.minion_mut(target)
        && m.is_card("BG23_009")
        && m.counter > 0
    {
        m.counter -= 1;
        return true;
    }
    false
}

/// Spellcraft "Give a minion +a/+h (and keywords) until next turn".
pub fn sc_buff(c: &mut Ctx, me: Me, target: EntityId, a: i32, h: i32, kw: Kw) {
    if sc_permanent(c, me.side, target) {
        c.buff(target, a, h);
        c.give_kw(target, kw);
    } else {
        c.buff_temp(target, a, h);
        if kw != Kw::NONE {
            c.give_kw_temp(target, kw);
        }
    }
}

/// Magnetic Volumizer (normal or golden).
pub fn is_volumizer(card: CardId) -> bool {
    static IDS: std::sync::OnceLock<Vec<CardId>> = std::sync::OnceLock::new();
    let ids = IDS.get_or_init(|| {
        VOLUMIZERS
            .iter()
            .flat_map(|v| [cid(v), db().golden_of(cid(v))])
            .collect()
    });
    ids.contains(&card)
}

/// Give `n` random cards from `set`.
pub fn give_random_of(c: &mut Ctx, side: Side, set: &[&str], n: i32) {
    for _ in 0..n {
        let i = c.g.rng.below(set.len());
        c.give_card(side, cid(set[i]));
    }
}

/// Choose One cards available to `side`: lobby minions in the pool and lobby Tavern spells
/// with Choose One, up to the current Tier.
pub fn choose_one_cards(c: &Ctx, side: Side) -> Vec<CardId> {
    let tier = c.tier(side);
    let d = db();
    let pool = &c.g.pool;
    let mut v: Vec<CardId> =
        c.g.universe
            .iter()
            .copied()
            .filter(|x| pool.get(*x) > 0)
            .chain(c.g.spell_universe.iter().copied())
            .filter(|x| {
                let x = d.get(*x);
                x.mech.has(Mech::CHOOSE_ONE) && x.tier <= tier
            })
            .collect();
    v.dedup();
    v
}

/// A fresh hand item for `card` (minion or spell).
pub fn choose_one_item(c: &mut Ctx, side: Side, card: CardId) -> Item {
    if db().get(card).kind == crate::cards::CardKind::Minion {
        Item::Minion(c.new_minion(side, card))
    } else {
        Item::spell(card)
    }
}

// ---------------------------------------------------------------------------- minion types

/// "A friendly minion of each type": one distinct minion per type, choosing the assignment
/// that covers the most types (dual-type / all-type minions fill whichever type helps most;
/// ties broken at random). Returns (minion, type) pairs.
pub fn one_minion_per_type(
    c: &mut Ctx,
    side: Side,
    f: impl Fn(&Minion) -> bool,
) -> Vec<(EntityId, Races)> {
    let mut minions: Vec<(EntityId, Races)> = c.boards[side]
        .iter()
        .filter(|m| m.alive() && f(m))
        .map(|m| (m.eid, m.races))
        .collect();
    let mut types: Vec<Races> = (0..Races::COUNT).map(|i| Races(1 << i)).collect();
    c.g.rng.shuffle(&mut minions);
    c.g.rng.shuffle(&mut types);
    // Kuhn's augmenting paths: owner[j] = type index matched to minion j.
    let mut owner: Vec<Option<usize>> = vec![None; minions.len()];
    fn augment(
        t: usize,
        types: &[Races],
        minions: &[(EntityId, Races)],
        owner: &mut [Option<usize>],
        seen: &mut [bool],
    ) -> bool {
        for j in 0..minions.len() {
            if seen[j] || !minions[j].1.intersects(types[t]) {
                continue;
            }
            seen[j] = true;
            if owner[j].is_none_or(|u| augment(u, types, minions, owner, seen)) {
                owner[j] = Some(t);
                return true;
            }
        }
        false
    }
    for t in 0..types.len() {
        let mut seen = vec![false; minions.len()];
        augment(t, &types, &minions, &mut owner, &mut seen);
    }
    owner
        .iter()
        .enumerate()
        .filter_map(|(j, t)| t.map(|t| (minions[j].0, types[t])))
        .collect()
}

/// Number of different minion types among `side`'s minions, each minion counted at most
/// once (7 Amalgams = 7 types).
/// Number of different types among minions with `races` (each minion counts once; maximal
/// matching).
pub fn distinct_types_of(races: &[Races]) -> i32 {
    fn augment(t: usize, races: &[Races], owner: &mut [Option<usize>], seen: &mut [bool]) -> bool {
        for j in 0..races.len() {
            if seen[j] || !races[j].intersects(Races(1 << t)) {
                continue;
            }
            seen[j] = true;
            if owner[j].is_none_or(|u| augment(u, races, owner, seen)) {
                owner[j] = Some(t);
                return true;
            }
        }
        false
    }
    let mut owner = vec![None; races.len()];
    let mut n = 0;
    for t in 0..Races::COUNT {
        let mut seen = vec![false; races.len()];
        if augment(t, races, &mut owner, &mut seen) {
            n += 1;
        }
    }
    n
}

pub fn distinct_type_count(c: &mut Ctx, side: Side) -> i32 {
    one_minion_per_type(c, side, |_| true).len() as i32
}

/// Get a Triple Reward (Discover a minion from one Tier higher than yours).
pub fn give_triple_reward(c: &mut Ctx, side: Side) {
    let tier = c.tier(side);
    let item = triple_reward_item(c, side, tier);
    c.add_to_hand(side, item);
}

/// The card a Triple Reward gives for a golden of `tier`: normally a Triple Reward (Discover
/// from tier + 1); Mister Clocksworth: a Tavern Coin; Corrupted Tome: a Triple Prize.
pub fn triple_reward_item(c: &Ctx, side: Side, tier: u8) -> Item {
    let p = c.player_ref(side);
    if p.is_some_and(|pl| pl.has_power("BG34_HERO_002p")) {
        return Item::spell(cid(TAVERN_COIN));
    }
    if p.is_some_and(|pl| pl.has_trinket("BG35_MagicItem_812")) {
        return Item::spell(cid("BG35_MagicItem_812t"));
    }
    // Tier 7 rewards only in a Tier 7 Tavern.
    let cap = if p.is_some_and(|pl| pl.tier >= 7) {
        7
    } else {
        6
    };
    Item::Spell {
        card: cid(crate::recruit::TRIPLE_REWARD),
        data: (tier + 1).min(cap) as i32,
    }
}

/// Copies of Buddy `id` on `side`'s board: sum of (1 normal / 2 golden).
pub fn buddy_power(c: &Ctx, side: Side, id: &str) -> i32 {
    c.boards[side]
        .iter()
        .filter(|m| m.alive() && m.is_card(id))
        .map(|m| if m.golden { 2 } else { 1 })
        .sum()
}

/// The Bonus Keywords.
pub const BONUS_KWS: [Kw; 6] = [
    Kw::TAUNT,
    Kw::DIVINE_SHIELD,
    Kw::WINDFURY,
    Kw::REBORN,
    Kw::VENOMOUS,
    Kw::STEALTH,
];

/// Number of different Bonus Keywords in `side`'s warband.
pub fn distinct_bonus_keywords(c: &Ctx, side: Side) -> i32 {
    BONUS_KWS
        .iter()
        .filter(|k| c.boards[side].iter().any(|m| m.alive() && m.kw.has(**k)))
        .count() as i32
}

/// Give a random Bonus Keyword the minion doesn't have yet (any one if it has all).
pub fn give_random_bonus_keyword(c: &mut Ctx, eid: EntityId) {
    let have = c.find_any(eid).map(|m| m.kw).unwrap_or(Kw::NONE);
    let missing: Vec<Kw> = BONUS_KWS
        .iter()
        .copied()
        .filter(|k| !have.has(*k))
        .collect();
    let pool: Vec<Kw> = if missing.is_empty() {
        BONUS_KWS.to_vec()
    } else {
        missing
    };
    if let Some(&k) = c.g.rng.pick(&pool) {
        c.give_kw(eid, k);
    }
}

pub fn is_blood_gem(card: CardId) -> bool {
    db().get(card).id.starts_with("BG20_GEM")
}

/// Play `n` Blood Gems on a minion.
pub fn gems_on(c: &mut Ctx, side: Side, target: EntityId, n: i32) {
    for _ in 0..n {
        c.play_gem(side, target);
    }
}

/// "Your Blood Gems give an extra +a/+h this game."
pub fn gem_bonus(c: &mut Ctx, side: Side, a: i32, h: i32) {
    if let Some(p) = c.player(side) {
        p.stats.gem_atk = p.stats.gem_atk.saturating_add(a);
        p.stats.gem_hp = p.stats.gem_hp.saturating_add(h);
    }
}
