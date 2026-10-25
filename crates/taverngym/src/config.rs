//! Rules configuration.
//!
//! Every value that is not a located server value (an unpublished rule or a "common baseline")
//! lives here, with its evidence status in the doc comment, so that it can be changed per game.
//! Defaults follow the base rules of the card data.

use serde::{Deserialize, Serialize};

/// Which content may appear in pools.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentMode {
    /// Only cards whose behaviour is implemented by a script (plus pure-keyword cards).
    Implemented,
    /// Every current-pool card; unimplemented text is ignored (stats + keywords only).
    AllVanilla,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesConfig {
    pub num_players: usize,
    pub content: ContentMode,

    // --- Economy (the client's recruit tips; base rules) -------------------------------------
    pub start_gold: i32,
    pub gold_increment: i32,
    /// Natural refill cap; max-Gold effects may exceed it.
    pub gold_refill_cap: i32,
    pub buy_cost: i32,
    pub sell_value: i32,
    pub refresh_cost: i32,
    /// Base upgrade cost to reach tier index+2 (tier 2..=7).
    pub upgrade_costs: [i32; 6],
    pub upgrade_discount_per_turn: i32,
    pub max_regular_tier: u8,

    // --- Shop (the 3-slot start is a baseline, not a located server value) --------------
    /// Minion slots by tavern tier 1..=7.
    pub shop_minion_slots: [usize; 7],
    /// Tavern-spell slots in every refresh.
    pub shop_spell_slots: usize,
    /// Most cards a Tavern can hold: 7, like the warband (neither may ever exceed 7). Cards over
    /// the limit are dropped; nothing is added to a full Tavern. Must not exceed the action space's
    /// Tavern slots (`N_SHOP`).
    #[serde(default = "shop_max_cards")]
    pub shop_max_cards: usize,

    // --- Zones (baseline) -------------------------------------------------------------
    pub board_size: usize,
    pub hand_size: usize,

    // --- Shared pool (15/15/13/11/9/7/5, wiki.gg data as of 36.6.0) ---------
    pub pool_copies_by_tier: [u32; 7],
    /// Tavern spell copies per tier 1-6. A spell's copies left = these minus the copies currently
    /// in any Tavern or hand (`Game::spell_copies_left`).
    #[serde(default = "spell_copies")]
    pub spell_copies_by_tier: [u32; 6],
    /// Exclude candidates whose client tag IS_BACON_POOL_MINION has a non-standard value (>= 2;
    /// absent = 0 and 1 are normal). Only Rot Hide Gnoll (BG25_013) has value 2 (meaning
    /// unpublished) and it was reported absent from 36.6 by manual testing.
    pub strict_pool_tag: bool,
    /// Extra card ids removed from every pool (manual overrides of the bundle's candidates).
    /// Empty by default (Flourishing Frostling is out of the pool in the 36.6.3 data).
    pub excluded_cards: Vec<String>,
    /// Hero Powers never offered by "Discover a Hero Power" effects: Edwin VanCleef and Cariel
    /// Roame (official 27.0 notes), Fungalmancer Flurgl (28.6.2).
    #[serde(default = "hero_power_discover_excluded")]
    pub hero_power_discover_excluded: Vec<String>,
    /// Silas Darkmoon: chance (%) that each new Tavern card carries a Darkmoon Ticket (30; on top
    /// of it one Refresh in twenty tickets every new card, `content::heroes_gen`).
    #[serde(default = "thirty")]
    pub silas_ticket_pct: u64,

    // --- Lobby ----------------------------------------------------------------------------
    /// Types per lobby (baseline 5; with `aberration_forced` one of them is Aberration; the
    /// selection policy is an assumption).
    pub lobby_races: usize,
    /// Aberrations are in every lobby (the launch guarantee, Sep 22 - Oct 1 2026). Off since
    /// 36.6.3: `lobby_races` of the 10 types are drawn uniformly, the "standard 50%".
    #[serde(default)]
    pub aberration_forced: bool,
    /// Types that are always in the lobby (names as `Races::NAMES`, e.g. "MURLOC"); the
    /// other slots are drawn at random. Empty = all random. For debugging and curricula.
    #[serde(default)]
    pub lobby_types: Vec<String>,
    pub hero_choices: usize,
    /// Heroes associated with minion types (Patches, Ysera...) are only offered when one of their
    /// types is in the lobby.
    #[serde(default = "yes")]
    pub hero_type_filter: bool,
    /// New heroes guaranteed to be offered in every lobby (each to one random player). Drest'agath
    /// and Kith'ix, for two weeks from Sep 22 (empty = off). `hero_choices = 4` models the
    /// season-pass offer.
    #[serde(default = "guaranteed_heroes")]
    pub guaranteed_heroes: Vec<String>,
    /// Only these heroes (card ids) are offered; empty = the whole hero pool. The lobby
    /// filters (types, bans) don't apply to a listed hero, and when the list is shorter than
    /// the seats' offers heroes repeat across players. For experiments (a fixed hero for
    /// every seat) and debugging; not a game rule.
    #[serde(default)]
    pub hero_pool: Vec<String>,
    /// Heroes (card ids) that are not in this lobby: never offered, and absent from what is
    /// drawn from the lobby's heroes (Hero Power and Buddy Discovers). For experiments
    /// without some heroes; not a game rule. Not written while it is empty. A hero named in
    /// `hero_pool` is offered all the same.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hero_bans: Vec<String>,

    // --- Combat / damage (inherited schedule) --------------------------------------
    /// (first_turn, cap) steps; cap applies while more than `damage_cap_until_alive` remain.
    pub damage_caps: Vec<(u32, i32)>,
    pub damage_cap_until_alive: usize,
    /// Safety cutoff for combat length. Hitting it is reported as a simulator truncation (tie),
    /// never as an official result.
    pub max_combat_attacks: u32,
    /// Hard limit on nested effect resolution depth.
    pub max_effect_depth: u32,

    // --- Trinkets (client tips GLUE_TIP_BACON_28/29) ------------------------------------------------
    pub lesser_trinket_turn: u32,
    pub greater_trinket_turn: u32,
    pub trinket_choices: usize,
    /// Type trinkets need this many minions of the type on board + in hand: [Lesser, Greater] (2025
    /// Developer Insight).
    #[serde(default = "trinket_type_min")]
    pub trinket_type_min: [usize; 2],
    /// Price cut of the one "pivot" trinket (a type you are not in) an offer may hold.
    #[serde(default = "two")]
    pub trinket_pivot_discount: i32,
    /// Weight multiplier (percent) of the "more likely" trinkets (client flag 4692,
    /// `trinket_offers::LIKELY`); 100 = none. The boost is unpublished; 200 is what four days of
    /// client offers show.
    #[serde(default = "trinket_likely_pct")]
    pub trinket_likely_pct: u64,

    // --- Dark Gifts (weights unknown -> uniform) ---------------------------------------------
    /// Spells cast by effects count as "you cast a spell". Off by default: only
    /// casts from hand trigger "after you cast" and count.
    #[serde(default)]
    pub effect_casts_count: bool,
    pub dark_gift_unlock_turn: u32,
    /// Offer weight (percent of a normal gift's) of the 4 rare gifts: Gilding, Persisting Horror,
    /// Titanic Strength, Invulnerability (50 is a guess).
    #[serde(default = "fifty")]
    pub dark_gift_rare_pct: u64,
    /// Harpy's Talons weight multiplier on Rally minions.
    #[serde(default = "two_u64")]
    pub harpy_rally_mult: u64,
    pub dark_gift_cost: i32,
    pub dark_gift_uses: u32,
    pub dark_gift_choices: usize,
    /// Dark Gift button, turn 9: the chance (percent) that the first option is Tier 5 rather than 4
    /// (`dark_gift_slots`). The client gave 4/5/6 and 5/5/6 twice each: an assumption, not a
    /// measured probability.
    #[serde(default = "fifty")]
    pub dark_gift_t9_first_t5_pct: u64,
    /// Dark Gift button, turns 10 on: the chance (percent) that each of the second and third
    /// options is Tier 6 rather than 5. The client gave 5/5/5, 5/5/6, 5/6/5 and 5/6/6, and
    /// there is no Tier 6 guarantee; the same kind of assumption.
    #[serde(default = "fifty")]
    pub dark_gift_late_t6_pct: u64,

    // --- Card assumptions ---------------------------------------------------------------------
    /// A Volumizer Magnetized to a host keeps receiving later Volumizer bonuses: the host
    /// gains every later bonus once per attached Volumizer. Off (the default): the attachment is
    /// a fixed enchantment worth the Volumizer's stats at that moment (base + the bonus so far),
    /// and later bonuses only reach Volumizer cards (as a player report, Firestone's simulator
    /// and real 36.6.3 boards show; there is no official statement).
    #[serde(default)]
    pub volumizer_attached_grow: bool,

    // --- Timewarped Tavern (34.2 system; off in 36.6.3) -------------------------------------
    /// Every player visits the Timewarped Tavern on `timewarp_turns` (Minor, Major). Off:
    /// the system is not active in the current patch; Morchie and Murozond still visit on
    /// their own turns, and effects can still Discover Timewarp cards.
    #[serde(default)]
    pub timewarp_enabled: bool,
    /// Experimental mode: a global turn-four Quest, Buddies and Timewarp, with the extra actions
    /// of `Game::extra_action`. Off by default.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub experimental_mode: bool,
    #[serde(default = "timewarp_turns")]
    pub timewarp_turns: [u32; 2],
    /// Chronum received at each visit (official 34.2 notes: 2).
    #[serde(default = "two")]
    pub timewarp_chronum: i32,
    /// Minions of a type needed to be "in" it for the Minor / Major Timewarp (official: 2/3).
    #[serde(default = "timewarp_type_min")]
    pub timewarp_type_min: [usize; 2],

    // --- Triples ----------------------------------------------------------------------------
    pub triple_copies: usize,

    // --- Deity ------------------------------------------------------------------------
    pub deity_threshold: u32,

    /// Turn cap (simulator safety, not a game rule; 0 = none): after this turn's combat the
    /// survivors are ranked by Health + Armor. Real games end long before; without it a
    /// lobby whose survivors can't field a minion never ends (found by fuzzing).
    #[serde(default = "max_turns")]
    pub max_turns: u32,
    /// The upper bound of the entries of `turn_action_caps`. On its own it caps nothing
    /// (the game has no limit on the number of operations).
    #[serde(default = "max_actions")]
    pub max_actions_per_turn: u32,
    /// The cap by turn: the most actions in turn t (index t - 1; the last entry holds for
    /// every later turn) before only End Turn (and pending choices) stay legal. Empty (the
    /// default): no cap. A real turn lasts a number of seconds and every operation takes
    /// time: set these from the seconds of each turn and of one operation. Moving minions
    /// and freezing never advance the game, so a policy without a cap can act forever:
    /// whoever drives one sets this (bots stop at 60 actions by themselves).
    pub turn_action_caps: Vec<u32>,
    /// The Freeze button answers this many times a turn - freezing and unfreezing the Tavern
    /// each count - and is not available after that (0 = no limit). 10 in the game
    /// (client 36.6.3). Not written while it has its default.
    #[serde(
        default = "freeze_presses",
        skip_serializing_if = "is_default_freeze_presses"
    )]
    pub freeze_presses_per_turn: u32,

    /// Record a structured event log (debugging; costs speed).
    pub record_log: bool,
}

impl Default for RulesConfig {
    fn default() -> Self {
        RulesConfig {
            num_players: 8,
            content: ContentMode::Implemented,
            start_gold: 3,
            gold_increment: 1,
            gold_refill_cap: 10,
            buy_cost: 3,
            sell_value: 1,
            refresh_cost: 1,
            upgrade_costs: [5, 7, 8, 11, 11, 10],
            upgrade_discount_per_turn: 1,
            max_regular_tier: 6,
            shop_minion_slots: [3, 4, 4, 5, 5, 6, 6],
            shop_spell_slots: 1,
            shop_max_cards: shop_max_cards(),
            board_size: 7,
            hand_size: 10,
            pool_copies_by_tier: [15, 15, 13, 11, 9, 7, 5],
            spell_copies_by_tier: spell_copies(),
            strict_pool_tag: true,
            excluded_cards: vec![],
            hero_power_discover_excluded: hero_power_discover_excluded(),
            silas_ticket_pct: 30,
            lobby_races: 5,
            aberration_forced: false,
            lobby_types: vec![],
            hero_choices: 2,
            hero_type_filter: true,
            guaranteed_heroes: guaranteed_heroes(),
            hero_pool: vec![],
            hero_bans: vec![],
            damage_caps: vec![(1, 5), (4, 10), (8, 15)],
            damage_cap_until_alive: 4,
            max_combat_attacks: 400,
            max_effect_depth: 64,
            lesser_trinket_turn: 6,
            greater_trinket_turn: 9,
            trinket_choices: 4,
            trinket_type_min: trinket_type_min(),
            trinket_pivot_discount: 2,
            trinket_likely_pct: trinket_likely_pct(),
            effect_casts_count: false,
            dark_gift_unlock_turn: 3,
            dark_gift_rare_pct: 50,
            harpy_rally_mult: 2,
            dark_gift_cost: 3,
            dark_gift_uses: 3,
            dark_gift_choices: 3,
            dark_gift_t9_first_t5_pct: 50,
            dark_gift_late_t6_pct: 50,
            volumizer_attached_grow: false,
            timewarp_enabled: false,
            experimental_mode: false,
            timewarp_turns: timewarp_turns(),
            timewarp_chronum: 2,
            timewarp_type_min: timewarp_type_min(),
            triple_copies: 3,
            deity_threshold: 4,
            max_actions_per_turn: max_actions(),
            turn_action_caps: Vec::new(),
            freeze_presses_per_turn: freeze_presses(),
            max_turns: max_turns(),
            record_log: false,
        }
    }
}

impl RulesConfig {
    /// The most actions a seat may take in `turn`: its entry of `turn_action_caps` (at most
    /// `max_actions_per_turn`); `u32::MAX`, no cap, when the table is empty.
    pub fn action_cap(&self, turn: u32) -> u32 {
        match self.turn_action_caps.as_slice() {
            [] => u32::MAX,
            caps => {
                caps[(turn.max(1) as usize - 1).min(caps.len() - 1)].min(self.max_actions_per_turn)
            }
        }
    }

    /// Reject values the fixed-size action space and observation cannot represent.
    /// A configuration written by hand: a JSON object of the fields to change. An unknown
    /// key is an error, and the result is validated. (A saved game's own configuration is read
    /// with serde: it may carry fields of an older build.)
    pub fn from_json(s: &str) -> Result<RulesConfig, String> {
        let v: serde_json::Value =
            serde_json::from_str(s).map_err(|e| format!("bad config: {e}"))?;
        let serde_json::Value::Object(map) = &v else {
            return Err("bad config: a JSON object is expected".into());
        };
        let known = serde_json::to_value(RulesConfig::default()).map_err(|e| e.to_string())?;
        for k in map.keys() {
            if known.get(k).is_none() && !Self::UNWRITTEN_AT_DEFAULT.contains(&k.as_str()) {
                return Err(format!("bad config: unknown field {k:?}"));
            }
        }
        let cfg: RulesConfig = serde_json::from_value(v).map_err(|e| format!("bad config: {e}"))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// The fields that are not written while they have their default (the default config
    /// stays the same): `from_json` knows them by name.
    const UNWRITTEN_AT_DEFAULT: [&'static str; 3] =
        ["hero_bans", "freeze_presses_per_turn", "experimental_mode"];

    pub fn validate(&self) -> Result<(), String> {
        let (b, h, s) = (self.board_size, self.hand_size, 7 + self.shop_spell_slots);
        if !(1..=7).contains(&b) {
            return Err(format!("board_size {b} must be 1..=7"));
        }
        if !(1..=10).contains(&h) {
            return Err(format!("hand_size {h} must be 1..=10"));
        }
        if s > 8 {
            return Err(format!(
                "shop_spell_slots {} exceeds the 8 shop slots",
                self.shop_spell_slots
            ));
        }
        if !(2..=8).contains(&self.num_players) {
            return Err(format!("num_players {} must be 2..=8", self.num_players));
        }
        // A blocking menu must fit the flat action space and the observation's choice slots.
        let choices = crate::actions::N_CHOICE;
        for (name, n) in [
            ("hero_choices", self.hero_choices),
            ("trinket_choices", self.trinket_choices),
            ("dark_gift_choices", self.dark_gift_choices),
        ] {
            if n > choices {
                return Err(format!("{name} {n} exceeds the {choices} choice slots"));
            }
        }
        // Taverns, pools and upgrade costs are defined up to Tier 7, and Tier 7 is the one
        // above `max_regular_tier` that `Player::tier7` unlocks.
        if !(1..=6).contains(&self.max_regular_tier) {
            return Err(format!(
                "max_regular_tier {} must be 1..=6",
                self.max_regular_tier
            ));
        }
        for t in &self.lobby_types {
            if !crate::types::Races::NAMES.contains(&t.as_str()) {
                return Err(format!("lobby_types: unknown type {t}"));
            }
        }
        // Values that look legal and break the engine.
        if !(1..=crate::actions::N_SHOP).contains(&self.shop_max_cards) {
            return Err(format!(
                "shop_max_cards {} must be 1..={}",
                self.shop_max_cards,
                crate::actions::N_SHOP
            ));
        }
        if self.max_combat_attacks == 0 || self.max_effect_depth == 0 {
            return Err(
                "max_combat_attacks and max_effect_depth must be at least 1 \
                        (0 would cut every combat short, it does not mean no limit)"
                    .into(),
            );
        }
        if self.triple_copies < 2 {
            return Err(format!(
                "triple_copies {} must be at least 2",
                self.triple_copies
            ));
        }
        if self.damage_caps.windows(2).any(|w| w[0].0 >= w[1].0) {
            return Err("damage_caps must be in ascending order of their first turn".into());
        }
        let copies = self
            .pool_copies_by_tier
            .iter()
            .chain(&self.spell_copies_by_tier);
        if let Some(n) = copies.into_iter().find(|n| **n > u16::MAX as u32) {
            return Err(format!("{n} copies of a card exceed {}", u16::MAX));
        }
        if self.gold_refill_cap < 0 {
            return Err(format!(
                "gold_refill_cap {} must not be negative",
                self.gold_refill_cap
            ));
        }
        let d = crate::cards::db();
        for (name, ids) in [
            ("excluded_cards", &self.excluded_cards),
            ("guaranteed_heroes", &self.guaranteed_heroes),
            (
                "hero_power_discover_excluded",
                &self.hero_power_discover_excluded,
            ),
        ] {
            if let Some(id) = ids.iter().find(|id| d.find(id).is_none()) {
                return Err(format!("{name}: {id} is not a card"));
            }
        }
        for h in &self.hero_bans {
            let d = crate::cards::db();
            if !d
                .find(h)
                .is_some_and(|c| d.get(c).kind == crate::cards::CardKind::Hero)
            {
                return Err(format!("hero_bans: {h} is not a hero"));
            }
        }
        for h in &self.hero_pool {
            let d = crate::cards::db();
            let ok = d.find(h).is_some_and(|c| {
                let c = d.get(c);
                c.kind == crate::cards::CardKind::Hero && c.hero_power.is_some()
            });
            if self.experimental_mode
                && ok
                && !crate::experimental::hero_allowed(d.get(d.find(h).unwrap()))
            {
                return Err(format!(
                    "hero_pool: {h} has no enabled Buddy in the experimental mode"
                ));
            }
            if !ok {
                return Err(format!("hero_pool: {h} is not a hero with a Hero Power"));
            }
            // A hero whose power has no script is never offered: a pool of such heroes
            // gives every seat an empty hero choice and the game cannot start.
            let scripted = d
                .find(h)
                .and_then(|c| d.get(c).hero_power)
                .is_some_and(|hp| crate::script::script(hp).is_some());
            if !scripted && self.content != ContentMode::AllVanilla {
                return Err(format!(
                    "hero_pool: the Hero Power of {h} is not implemented"
                ));
            }
        }
        // Aberrations forced into every lobby take one of its places.
        let forced = self.lobby_types.len()
            + usize::from(
                self.aberration_forced && !self.lobby_types.iter().any(|t| t == "ABERRATION"),
            );
        if forced > self.lobby_races {
            return Err(format!(
                "{forced} types are forced into the lobby (lobby_types, aberration_forced) \
                 but a lobby has {}",
                self.lobby_races
            ));
        }
        Ok(())
    }

    pub fn damage_cap(&self, turn: u32, alive: usize) -> Option<i32> {
        if alive <= self.damage_cap_until_alive {
            return None;
        }
        self.damage_caps
            .iter()
            .rev()
            .find(|(t, _)| turn >= *t)
            .map(|(_, c)| *c)
    }

    /// The Dark Gift button's options by place, as the client offers them on turns 3-8: the Tiers
    /// option k may have (one is chosen when there are two: `dark_gift_t9_first_t5_pct`,
    /// `dark_gift_late_t6_pct`) and whether it is of your most common type on the board. The
    /// options keep these places. Turns 9 and later follow the same frame with fewer
    /// observations.
    pub fn dark_gift_slots(&self, turn: u32) -> [(&'static [u8], bool); 3] {
        match turn {
            0..=2 => [(&[], false), (&[], false), (&[], false)],
            3 => [(&[2], false), (&[2], false), (&[2], false)],
            4 => [(&[2], false), (&[2], false), (&[3], false)],
            5 => [(&[3], false), (&[3], false), (&[3], false)],
            6 => [(&[3], true), (&[4], false), (&[4], false)],
            7 => [(&[4], true), (&[4], false), (&[4], false)],
            8 => [(&[4], true), (&[5], false), (&[5], false)],
            9 => [(&[4, 5], true), (&[5], false), (&[6], false)],
            _ => [(&[5], true), (&[5, 6], true), (&[5, 6], false)],
        }
    }

    /// Dark Gift minion tiers offered on a given turn: the Tiers of `dark_gift_slots` (Tiers 5
    /// and 6 from turn 10 on).
    pub fn dark_gift_tiers(&self, turn: u32) -> &'static [u8] {
        match turn {
            0..=2 => &[],
            3 => &[2],
            4 => &[2, 3],
            5 => &[3],
            6 => &[3, 4],
            7 => &[4],
            8 => &[4, 5],
            9 => &[4, 5, 6],
            _ => &[5, 6],
        }
    }
}

/// A field-level serde default replaces the struct default, so a partial config keeps these.
fn guaranteed_heroes() -> Vec<String> {
    vec!["BG36_HERO_000".into(), "BG36_HERO_002".into()]
}

fn timewarp_turns() -> [u32; 2] {
    [6, 9]
}

fn timewarp_type_min() -> [usize; 2] {
    [2, 3]
}

fn shop_max_cards() -> usize {
    7
}

fn max_turns() -> u32 {
    100
}

fn freeze_presses() -> u32 {
    10
}

fn is_default_freeze_presses(n: &u32) -> bool {
    *n == freeze_presses()
}

fn max_actions() -> u32 {
    200
}

fn yes() -> bool {
    true
}

fn trinket_likely_pct() -> u64 {
    200
}

fn trinket_type_min() -> [usize; 2] {
    [2, 3]
}

fn two() -> i32 {
    2
}

fn thirty() -> u64 {
    30
}

fn hero_power_discover_excluded() -> Vec<String> {
    [
        "TB_BaconShop_HP_001",
        "BG21_HERO_000p",
        "TB_BaconShop_HP_056",
    ]
    .map(String::from)
    .to_vec()
}

fn two_u64() -> u64 {
    2
}

fn fifty() -> u64 {
    50
}

fn spell_copies() -> [u32; 6] {
    [5, 7, 9, 11, 9, 7]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_cap_schedule() {
        let c = RulesConfig::default();
        assert_eq!(c.damage_cap(1, 8), Some(5));
        assert_eq!(c.damage_cap(3, 8), Some(5));
        assert_eq!(c.damage_cap(4, 8), Some(10));
        assert_eq!(c.damage_cap(9, 5), Some(15));
        assert_eq!(c.damage_cap(9, 4), None);
    }
}
