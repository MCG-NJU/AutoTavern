//! Game state data: minion instances, hand/shop items, players, pool, pending choices.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::cards::{CardId, CardKind, db};
use crate::types::{EntityId, Kw, PlayerId, Races};

/// One recorded change to a minion's stats (only kept while `cfg.record_log`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StatMod {
    /// Card whose effect caused it (None = system: triple merge, tavern bonus...).
    pub src: Option<CardId>,
    pub kind: ModKind,
    pub atk: i32,
    pub hp: i32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModKind {
    /// +atk/+hp.
    Add,
    /// Stats set to atk/hp.
    Set,
    /// Stats carried over from the copies merged by a triple.
    Merge,
    /// Tavern-wide "minions in the Tavern have +X/+Y" when the minion appeared.
    Tavern,
    /// Magnetized minion's stats.
    Magnetic,
}

/// A minion instance (on a board, in a hand, or in the tavern).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Minion {
    pub eid: EntityId,
    pub card: CardId,
    pub atk: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub kw: Kw,
    /// Permanently granted keywords, including grants matching the printed keyword.
    #[serde(default)]
    pub bonus_kw: Kw,
    /// Remaining hits the Divine Shield absorbs (meaningful while `kw` has DIVINE_SHIELD).
    pub shield_hits: u8,
    /// Hits a fresh Divine Shield grants (3 for Dark Paradox tier 5 / Toreth's Blessing).
    pub shield_max: u8,
    pub races: Races,
    pub tier: u8,
    pub golden: bool,
    /// Scripts attached to this minion: magnetized cards, Dark Gifts, granted effects.
    pub attached: Vec<CardId>,
    /// Per-attachment effect state, aligned with `attached`.
    #[serde(default)]
    pub attachment_data: Vec<i32>,
    /// Number of shared-pool copies this instance represents (returned on sell/elimination).
    pub pool_copies: u8,
    /// Original pool species for heterogeneous triples (Elemental of Surprise). Empty
    /// means all `pool_copies` belong to this minion's normal card. Not enchantments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pool_parts: Vec<(CardId, u8)>,
    /// Playing this (golden) minion from hand grants a Triple Reward.
    pub triple_reward: bool,
    /// Generic per-instance script counter ("improve this", countdowns).
    pub counter: i32,
    /// State of this minion's Dark Gift (Steady Growth stage, Affinity / Replication turns).
    #[serde(default)]
    pub gift_data: i32,
    /// A minion's lifetime "first played or Magnetized" reward has been claimed.
    #[serde(default)]
    pub first_play_reward_used: bool,
    /// Accumulated player-level "this game, wherever they are" bonuses already baked in.
    pub pbonus_atk: i32,
    pub pbonus_hp: i32,
    /// Buffs that expire at the start of the owner's next turn.
    pub temp_atk: i32,
    pub temp_hp: i32,
    /// Portion granted while in the Tavern. Acquisition fixes these stats; temporary
    /// enchantments brought into the Tavern keep their original expiry.
    #[serde(default)]
    pub tavern_temp_atk: i32,
    #[serde(default)]
    pub tavern_temp_hp: i32,
    pub temp_kw: Kw,
    /// Activate uses this turn.
    pub activations: u8,
    // ---- combat transient -------------------------------------------------------------
    pub destroyed: bool,
    pub attacked: bool,
    pub killed_by: EntityId,
    pub peak_atk: i32,
    pub peak_hp: i32,
    /// Summoned "for this combat only" from the hand (keeps the hand card).
    pub combat_only: bool,
    /// Blood Gems played on this minion and the stats they gave (for steal/transfer effects).
    #[serde(default)]
    pub gems: u16,
    #[serde(default)]
    pub gem_atk: i32,
    #[serde(default)]
    pub gem_hp: i32,
    /// Avenge progress (friendly deaths seen this combat).
    pub avenge: u16,
    /// Attacks made during the current attack turn (Windfury).
    pub attacks: u8,
    /// Dies if played on this turn number (Tomb Turning); 0 = no.
    #[serde(default)]
    pub doomed_turn: u32,
    /// "Every N turns" effects of this minion: the turns counted so far and the last turn
    /// counted (`Ctx::minion_turn_cycle`).
    #[serde(default)]
    pub cycle: u32,
    #[serde(default)]
    pub cycle_turn: u32,
    /// Deathrattles gained from other cards (Fish of N'Zoth); only their Deathrattle runs.
    #[serde(default)]
    pub gained_dr: Vec<CardId>,
    /// Stat change history for debugging / the UI (empty unless logging).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<StatMod>,
}

impl Minion {
    pub fn new(eid: EntityId, card: CardId) -> Minion {
        let d = db().get(card);
        debug_assert!(d.kind == CardKind::Minion, "{} is not a minion", d.id);
        let ds = d.kw.has(Kw::DIVINE_SHIELD);
        Minion {
            eid,
            card,
            atk: d.atk,
            hp: d.hp,
            max_hp: d.hp,
            kw: d.kw,
            bonus_kw: Kw::NONE,
            shield_hits: if ds { 1 } else { 0 },
            shield_max: 1,
            races: d.races,
            tier: d.tier,
            golden: d.is_golden(),
            attached: Vec::new(),
            attachment_data: Vec::new(),
            pool_copies: 0,
            pool_parts: Vec::new(),
            triple_reward: d.is_golden(),
            counter: 0,
            gift_data: 0,
            first_play_reward_used: false,
            pbonus_atk: 0,
            pbonus_hp: 0,
            temp_atk: 0,
            temp_hp: 0,
            tavern_temp_atk: 0,
            tavern_temp_hp: 0,
            temp_kw: Kw::NONE,
            activations: 0,
            destroyed: false,
            attacked: false,
            killed_by: 0,
            peak_atk: d.atk,
            peak_hp: d.hp,
            combat_only: false,
            gems: 0,
            gem_atk: 0,
            gem_hp: 0,
            avenge: 0,
            attacks: 0,
            doomed_turn: 0,
            cycle: 0,
            cycle_turn: 0,
            gained_dr: Vec::new(),
            mods: Vec::new(),
        }
    }

    pub fn attachment_data_at(&self, index: usize) -> i32 {
        self.attachment_data
            .get(index)
            .copied()
            .unwrap_or(self.gift_data)
    }

    pub fn attach(&mut self, card: CardId, data: i32) {
        while self.attachment_data.len() < self.attached.len() {
            self.attachment_data.push(self.gift_data);
        }
        self.attached.push(card);
        self.attachment_data.push(data);
    }

    pub fn extend_attachments_from(&mut self, other: &Minion) {
        for (i, &card) in other.attached.iter().enumerate() {
            self.attach(card, other.attachment_data_at(i));
        }
    }

    pub fn def(&self) -> &'static crate::cards::CardDef {
        db().get(self.card)
    }
    pub fn name(&self) -> &'static str {
        &db().get(self.card).name
    }
    pub fn normal_card(&self) -> CardId {
        db().normal_of(self.card)
    }

    pub fn visit_pool_parts(&self, mut f: impl FnMut(CardId, u16)) {
        if self.pool_copies == 0 {
            return;
        }
        if self.pool_parts.is_empty() {
            f(self.normal_card(), self.pool_copies as u16);
        } else {
            for &(card, n) in &self.pool_parts {
                f(card, n as u16);
            }
        }
    }
    pub fn pool_entries(&self) -> Vec<(CardId, u16)> {
        let mut entries = vec![];
        self.visit_pool_parts(|c, n| entries.push((c, n)));
        entries
    }
    pub fn has(&self, k: Kw) -> bool {
        self.kw.has(k)
    }
    pub fn is(&self, r: Races) -> bool {
        self.races.intersects(r)
    }
    pub fn alive(&self) -> bool {
        self.hp > 0 && !self.destroyed
    }
    pub fn is_card(&self, id: &str) -> bool {
        let c = db().get(self.card);
        c.id == id || c.normal.map(|n| db().get(n).id == id).unwrap_or(false)
    }
    /// Same as `is_card` with a resolved (normal-form) id: no string comparison.
    #[inline]
    pub fn is_card_id(&self, normal: CardId) -> bool {
        self.card == normal || db().get(self.card).normal == Some(normal)
    }

    /// Add stats (health also raises max health). Tracks peaks for death memory.
    pub fn add_stats(&mut self, atk: i32, hp: i32) {
        if crate::script::script(self.card)
            .is_some_and(|s| s.has_flag(crate::script::flag::NO_STAT_GAIN))
        {
            return;
        }
        // Saturating: runaway doubling effects must not wrap around.
        self.atk = self.atk.saturating_add(atk).max(0);
        self.max_hp = self.max_hp.saturating_add(hp);
        self.hp = self.hp.saturating_add(hp);
        if self.max_hp < 1 {
            self.max_hp = 1;
        }
        self.peak_atk = self.peak_atk.max(self.atk);
        self.peak_hp = self.peak_hp.max(self.hp);
    }

    pub(crate) fn add_temp_stats(&mut self, atk: i32, hp: i32, in_tavern: bool) {
        let (old_a, old_h) = (self.atk, self.max_hp);
        self.add_stats(atk, hp);
        let (a, h) = (
            self.atk.saturating_sub(old_a),
            self.max_hp.saturating_sub(old_h),
        );
        self.temp_atk = self.temp_atk.saturating_add(a);
        self.temp_hp = self.temp_hp.saturating_add(h);
        if in_tavern {
            self.tavern_temp_atk = self.tavern_temp_atk.saturating_add(a);
            self.tavern_temp_hp = self.tavern_temp_hp.saturating_add(h);
        }
    }

    pub(crate) fn keep_tavern_buffs(&mut self) {
        self.temp_atk = self.temp_atk.saturating_sub(self.tavern_temp_atk);
        self.temp_hp = self.temp_hp.saturating_sub(self.tavern_temp_hp);
        self.tavern_temp_atk = 0;
        self.tavern_temp_hp = 0;
    }

    pub fn set_stats(&mut self, atk: i32, hp: i32) {
        if crate::script::script(self.card)
            .is_some_and(|s| s.has_flag(crate::script::flag::NO_STAT_GAIN))
        {
            return;
        }
        self.atk = atk.max(0);
        self.hp = hp;
        self.max_hp = hp;
        self.peak_atk = self.peak_atk.max(self.atk);
        self.peak_hp = self.peak_hp.max(self.hp);
    }

    /// Fixed stats ("a 3/3 copy", "set its stats to 2") under the player's continuous bonuses
    /// already on the minion (`pbonus_*`: Eternal Knight, the Undead army...), which stay marked as
    /// applied. Raw `set_stats` stays for exact snapshots (copies, Robust Evolution...).
    pub fn set_base_stats(&mut self, atk: i32, hp: i32) {
        self.set_stats(
            atk.saturating_add(self.pbonus_atk),
            hp.saturating_add(self.pbonus_hp),
        );
    }

    /// Give keywords for good. One the minion only had "until next turn" stays with it: it
    /// is no longer temporary (`Ctx::give_kw_temp` keeps its own marks).
    pub fn give(&mut self, k: Kw) {
        if k.has(Kw::DIVINE_SHIELD) && !self.kw.has(Kw::DIVINE_SHIELD) {
            self.shield_hits = self.shield_max.max(1);
        }
        self.kw.set(k);
        self.bonus_kw.set(k);
        self.temp_kw.clear(k);
    }

    pub fn remove(&mut self, k: Kw) {
        self.kw.clear(k);
        self.bonus_kw.clear(k);
        self.temp_kw.clear(k);
        if k.has(Kw::DIVINE_SHIELD) {
            self.shield_hits = 0;
        }
    }

    /// Record a stat change (merging consecutive adds from the same source).
    pub fn record(&mut self, src: Option<CardId>, kind: ModKind, atk: i32, hp: i32) {
        if kind == ModKind::Add
            && let Some(last) = self.mods.last_mut()
            && last.kind == ModKind::Add
            && last.src == src
        {
            last.atk = last.atk.saturating_add(atk);
            last.hp = last.hp.saturating_add(hp);
            return;
        }
        self.mods.push(StatMod { src, kind, atk, hp });
    }

    pub fn has_attached(&self, id: &str) -> bool {
        self.attached.iter().any(|c| db().get(*c).id == id)
    }
}

/// A card in hand, in the tavern, or offered by a Discover.
/// Keep minions inline: boxing every hand/shop item would add an allocation to ordinary
/// draws and clones just to shrink the spell variant.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Item {
    Minion(Minion),
    /// A spell with an optional numeric parameter (Triple Reward tier, Lockbox turns...).
    Spell {
        card: CardId,
        data: i32,
    },
}

impl Item {
    pub fn card(&self) -> CardId {
        match self {
            Item::Minion(m) => m.card,
            Item::Spell { card, .. } => *card,
        }
    }
    pub fn as_minion(&self) -> Option<&Minion> {
        match self {
            Item::Minion(m) => Some(m),
            _ => None,
        }
    }
    pub fn as_minion_mut(&mut self) -> Option<&mut Minion> {
        match self {
            Item::Minion(m) => Some(m),
            _ => None,
        }
    }
    pub fn spell(card: CardId) -> Item {
        Item::Spell { card, data: 0 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HandCard {
    pub hid: EntityId,
    pub item: Item,
    /// Turns until this card can be played (0 = playable).
    pub locked: u8,
    /// Paired card: when one is played the other is discarded (Kith'ix, Willbreaker).
    pub link: EntityId,
    /// Linked group (golden Wandering Willbreaker / Faceless Operative: 4 cards, play 2, the rest
    /// are discarded): group id (0 = none) and plays left.
    #[serde(default)]
    pub group: EntityId,
    #[serde(default)]
    pub group_left: u8,
    /// Removed from hand at the end of the turn (Spellcraft spells).
    #[serde(default)]
    pub temporary: bool,
    /// Spellcraft spell: the minion that gave it (0 = none).
    #[serde(default)]
    pub src: EntityId,
    /// Choose One card with both effects combined (Fandral's Fortune).
    #[serde(default)]
    pub combined: bool,
    /// Locked until the effect that gave it lets go, not for a number of turns: Thorim's Tier 7
    /// minion (60 Gold spent), Faelin's three (their Tavern Tier reached). The card is in the hand
    /// from the start like any locked card; `locked` stays 1 while it is held.
    #[serde(default)]
    pub held: bool,
    /// What lets a held card go belongs to the card, not to the Hero Power that gave it, so it also
    /// unlocks after that power is replaced: the Gold the player still has to spend before it
    /// unlocks (Thorim: 60 at the pick), or 0 for a card that unlocks when the Tavern reaches its
    /// Tier (Faelin).
    #[serde(default)]
    pub held_gold: i32,
}

/// The most Secrets a player can have in play (a rule of the game; the observation has a
/// slot for each, `obs::N_SECRETS`).
pub const MAX_SECRETS: usize = 5;

/// What a card waiting in `Player::hand_queue` keeps until it reaches the hand (its link and its
/// Dark Gifts).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct QueueMark {
    /// The hand id reserved for the card (a linked partner names it).
    #[serde(default)]
    pub hid: EntityId,
    /// Its linked partner (`HandCard::link`).
    #[serde(default)]
    pub link: EntityId,
    /// Dark Gifts picked with it whose "when attached" effect is still to run.
    #[serde(default)]
    pub gifts: Vec<CardId>,
    /// A temporary card (a trinket's Spellcraft spell, `Ctx::give_temp_spell`): it leaves at
    /// the end of the turn while it waits, and is temporary in the hand (`HandCard::temporary`,
    /// with `src`).
    #[serde(default)]
    pub temporary: bool,
    #[serde(default)]
    pub src: EntityId,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ShopSlot {
    pub sid: EntityId,
    pub item: Item,
    /// Frozen: kept at the next turn start (the Freeze button freezes the Tavern's current
    /// contents only; later additions are not frozen).
    #[serde(default)]
    pub frozen: bool,
    /// Buy price override for this card (Chillmere Mosaic: "They cost (1)").
    #[serde(default)]
    pub price: Option<i32>,
    /// Kept from last turn by a Freeze: not a new card of the turn-start refill (effects
    /// that act on the cards a Refresh brought in skip it).
    #[serde(default)]
    pub kept: bool,
    /// Put there by the default Refresh fill. These have the lowest priority for the Tavern's 7
    /// slots: a card an effect adds (an extra offer, Fodder...) takes the place of one of them when
    /// the Tavern is full.
    #[serde(default)]
    pub filled: bool,
}

impl ShopSlot {
    pub fn new(sid: EntityId, item: Item) -> ShopSlot {
        ShopSlot {
            sid,
            item,
            frozen: false,
            price: None,
            kept: false,
            filled: false,
        }
    }
}

/// Per-player counters for "this game" effects and triggers.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PlayerStats {
    /// You buffed a Tavern minion this game (trinket offer conditions).
    #[serde(default)]
    pub tavern_buffed: bool,
    /// Ballers sold this game (Baller Portrait offer condition).
    #[serde(default)]
    pub ballers_sold: i32,
    pub spells_cast: i32,
    pub tavern_spells_cast: i32,
    pub battlecries_triggered: i32,
    pub deathrattles_triggered: i32,
    pub elementals_played: i32,
    pub cards_played: i32,
    pub cards_discarded: i32,
    pub golden_played: i32,
    pub minions_bought: i32,
    pub cards_bought: i32,
    pub minions_sold: i32,
    pub gold_spent_total: i32,
    pub gold_spent_turn: i32,
    pub refreshes: i32,
    // "this game" type bonuses (applied to instances as `pbonus_*`)
    pub undead_atk: i32,
    /// Health your Undead have this game: only Butchering cast with a Tavern-spell bonus gives any
    /// (the bonus's Health part).
    #[serde(default)]
    pub undead_hp: i32,
    pub beetle_atk: i32,
    pub beetle_hp: i32,
    pub eternal_knight_deaths: i32,
    /// Eternal Portrait (greater): +4/+2 for Eternal Knights per friendly Undead death.
    #[serde(default)]
    pub knight_bonus: i32,
    /// Extra stats Blood Gems give on top of the base +1/+1.
    pub gem_atk: i32,
    pub gem_hp: i32,
    #[serde(default)]
    pub elementals_played_turn: i32,
    #[serde(default)]
    pub battlecries_turn: i32,
    /// Spellcraft spells cast this turn / this game.
    #[serde(default)]
    pub spellcraft_cast_turn: i32,
    #[serde(default)]
    pub spellcraft_cast: i32,
    /// "Your Volumizers have +X this game (wherever they are)", incl. magnetized ones.
    #[serde(default)]
    pub volumizer_atk: i32,
    #[serde(default)]
    pub volumizer_hp: i32,
    /// Magnetizations this game, from hand or by effects (Dr. Boom's Monster).
    #[serde(default)]
    pub magnetized: i32,
    /// "Your Elementals give an extra +N/+N this game" (Timewarped Swirler).
    #[serde(default)]
    pub elem_give_atk: i32,
    #[serde(default)]
    pub elem_give_hp: i32,
    /// Extra stats Tavern spells give.
    pub spell_atk: i32,
    pub spell_hp: i32,
    /// Stats for minions in the Tavern (all / Elementals).
    pub tavern_atk: i32,
    pub tavern_hp: i32,
    pub tavern_elemental_atk: i32,
    pub tavern_elemental_hp: i32,
    /// "Minions of <type> in the Tavern have +a/+h this game": (types mask, a, h).
    #[serde(default)]
    pub tavern_race: Vec<(u16, i32, i32)>,
    /// "Improve your future X" counters.
    pub baller_bonus: i32,
    pub lobster_bonus: i32,
    pub last_spell_cast: Option<CardId>,
    /// Instance parameter of the last Tavern spell (e.g. Bloodbinder's Blood Gem).
    #[serde(default)]
    pub last_spell_cast_data: i32,
    pub combats_won: i32,
    pub combats_lost: i32,
    pub won_last: bool,
    pub lost_last: bool,
    pub tied_last: bool,
    /// The last combat: damage dealt to the opponent's hero (> 0: won) or taken (< 0: lost),
    /// after the cap; 0 for a tie. The leaderboard shows everybody's recent results.
    #[serde(default)]
    pub last_damage: i32,
    /// Combats won in a row (> 0) or lost in a row (< 0); a tie starts over at 0.
    #[serde(default)]
    pub streak: i32,
    /// The last combat (diagnostics): the Deity awakened; the largest minion Y'Shaarj
    /// summoned back (Attack + Health, 0 = none).
    #[serde(default)]
    pub last_deity: bool,
    #[serde(default)]
    pub last_returned_max: i32,
    pub damage_taken_outside_combat: i32,
    pub spell_discount_next: i32,
    /// Spells cast this turn (reset at turn start).
    pub spells_cast_turn: i32,
    /// Tavern spells you cast this turn (Inductive Gyroblade).
    #[serde(default)]
    pub tavern_spells_cast_turn: i32,
    /// Cards played this turn (reset at turn start).
    pub cards_played_turn: i32,
    pub minions_played_turn: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeityState {
    pub card: CardId,
    pub atk: i32,
    pub hp: i32,
    pub golden: bool,
    pub reborn: bool,
}

/// A persistent "this game" effect owned by a player (e.g. "After the Tavern is Refreshed this
/// game..."). Its behaviour is the source card's `Script::persistent_event`; each stack fires once.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlayerEffect {
    pub card: CardId,
    pub stacks: i32,
}

/// Something scheduled for the start of a later turn.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Scheduled {
    pub turns_left: u32,
    pub script: CardId,
    pub target: EntityId,
    pub a: i32,
    pub b: i32,
    /// Only fires if the owner won the combat in between.
    pub if_won: bool,
}

/// A decision the player must make before doing anything else.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Pending {
    HeroSelect {
        options: Vec<CardId>,
    },
    /// Pick one item to add to hand.
    Discover {
        source: CardId,
        options: Vec<Item>,
    },
    /// Pick a Choose One mode (0/1) for `cont`.
    ChooseOne {
        card: CardId,
        cont: Continuation,
    },
    /// Pick a target entity for `cont`.
    Target {
        options: Vec<EntityId>,
        cont: Continuation,
    },
    /// Pick one of several Hero Powers for power slot `slot` (a slot past the last one adds
    /// a new Hero Power).
    HeroPowerChoice {
        options: Vec<CardId>,
        #[serde(default)]
        slot: u8,
    },
    /// Trinket offer: must pick one and pay its cost before anything else. `free`: no cost;
    /// `replace`: the pick replaces that trinket (such offers can't be declined either).
    Trinket {
        options: Vec<CardId>,
        #[serde(default)]
        free: bool,
        #[serde(default)]
        replace: Option<CardId>,
    },
    /// Choose a Quest (with its paired Reward). `types` holds, per option, the minion types
    /// drawn for it when it was offered (`content::draw_quest_types`): of the Quest and of
    /// the Reward, 0 = none.
    Quest {
        options: Vec<(CardId, CardId)>,
        #[serde(default)]
        types: Vec<(u16, u16)>,
    },
    /// The Timewarped Tavern: `options` are (card, Chronum cost); choosing one buys it and
    /// the visit goes on while something is affordable. The index past the last option
    /// leaves the Tavern.
    Timewarp {
        options: Vec<(Item, i32)>,
        chronum: i32,
        major: bool,
    },
}

impl Pending {
    pub fn len(&self) -> usize {
        match self {
            Pending::HeroSelect { options } => options.len(),
            Pending::Discover { options, .. } => options.len(),
            Pending::ChooseOne { .. } => 2,
            Pending::Target { options, .. } => options.len(),
            Pending::HeroPowerChoice { options, .. } => options.len(),
            Pending::Trinket { options, .. } => options.len(),
            Pending::Quest { options, .. } => options.len(),
            Pending::Timewarp { options, .. } => options.len() + 1,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// A regular trinket offer: paid, replacing nothing (the one that has pivot discounts).
    pub fn paid_trinket_offer(&self) -> bool {
        matches!(
            self,
            Pending::Trinket {
                free: false,
                replace: None,
                ..
            }
        )
    }
    /// Whether the player may back out of this choice (nothing has been paid or moved yet).
    pub fn cancellable(&self) -> bool {
        match self {
            Pending::ChooseOne { cont, .. } | Pending::Target { cont, .. } => cont.cancellable(),
            // A replacing trinket (Mystery Cube, Trip Vouchers) can't be declined.
            _ => false,
        }
    }
}

/// What to do once a pending target/mode is chosen.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Continuation {
    /// A played minion's Battlecry (the minion is already on the board).
    Battlecry {
        eid: EntityId,
        opt: u8,
        #[serde(default)]
        played: Option<crate::event::PlayInfo>,
    },
    /// One Battlecry source of a played composite minion. Targets are chosen one source
    /// at a time; the multiplier is fixed once for the whole play.
    AttachedBattlecry {
        eid: EntityId,
        card: CardId,
        remaining: Vec<CardId>,
        mult: u32,
        #[serde(default)]
        opt: u8,
        #[serde(default)]
        played: Option<crate::event::PlayInfo>,
    },
    /// Cast a spell that is no longer in hand (already paid).
    Spell { card: CardId, data: i32, opt: u8 },
    /// Play the spell still in hand with id `hid`: pay, remove and cast once the target is
    /// known (cancellable until then).
    HandSpell { hid: EntityId, opt: u8 },
    /// Use the hero power in `slot`: pays and counts the use when executed (cancellable
    /// until then).
    HeroPower {
        opt: u8,
        #[serde(default)]
        slot: u8,
    },
    /// Activate a minion: pays and counts the use when executed (cancellable until then).
    Activate { eid: EntityId },
    /// Second target of an effect already paid for (Vol'jin, Mutanus): runs `card`'s
    /// `followup` script with `first` / `data`.
    Followup {
        card: CardId,
        first: EntityId,
        data: i32,
    },
}

impl Continuation {
    pub fn cancellable(&self) -> bool {
        !matches!(
            self,
            Continuation::Battlecry { .. }
                | Continuation::AttachedBattlecry { .. }
                | Continuation::Spell { .. }
                | Continuation::Followup { .. }
        )
    }
}

/// An active Quest: complete `goal` progress to get `reward`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QuestState {
    pub card: CardId,
    pub reward: CardId,
    pub progress: i32,
    pub goal: i32,
    /// Quest parameter (Unlikely Duo: the two minion types).
    #[serde(default)]
    pub data: i32,
    /// The minion type shown on the Reward when the pair was offered (Scepter of Guidance,
    /// The Friends Along the Way); 0 = none.
    #[serde(default)]
    pub reward_data: i32,
}

/// One Hero Power with its own use counts and progress counter.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HeroPowerState {
    pub card: CardId,
    #[serde(default)]
    pub uses_turn: u32,
    #[serde(default)]
    pub uses_game: u32,
    /// Script progress counter ("(3 left!)", improvements...).
    #[serde(default)]
    pub counter: i32,
    /// Cost adjustment (discounts, "costs (1) more after each use").
    #[serde(default)]
    pub cost_mod: i32,
    /// Turns before it can be used again (Snake Eyes).
    #[serde(default)]
    pub cooldown: u8,
    /// Cards remembered by the power (Kurtrus: minions bought).
    #[serde(default)]
    pub memory: Vec<CardId>,
    /// Numbers remembered by the power (Tavish: Attack of loaded minions).
    #[serde(default)]
    pub values: Vec<i32>,
    /// Improvement level ("Improve this").
    #[serde(default)]
    pub level: i32,
}

impl HeroPowerState {
    pub fn new(card: CardId) -> Self {
        HeroPowerState {
            card,
            uses_turn: 0,
            uses_game: 0,
            counter: 0,
            cost_mod: 0,
            cooldown: 0,
            memory: Vec::new(),
            values: Vec::new(),
            level: 0,
        }
    }
}

/// What a player has seen of another player's warband: the one the other took into the last
/// combat between the two. The game shows no warband a player did not fight (the leaderboard
/// has an opponent's Health, Tavern Tier, Trinkets, main minion type and recent results).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Scouted {
    pub board: Arc<Vec<Minion>>,
    /// Turn of that combat (0 = the two have not met).
    pub turn: u32,
}

/// (minion, Attack, maximum Health) as last looked at (`Ctx::mishmash_sync`).
pub type SeenStats = Vec<(EntityId, i32, i32)>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Player {
    pub id: PlayerId,
    pub hero: Option<CardId>,
    /// Hero Powers: the hero's own first, then any extra ones (Genn, Reinvigorating Light...).
    #[serde(default)]
    pub powers: Vec<HeroPowerState>,
    pub health: i32,
    pub armor: i32,
    pub alive: bool,
    /// Final placement (1..=8), 0 while alive.
    pub placement: u8,
    pub tier: u8,
    pub gold: i32,
    pub max_gold_bonus: i32,
    pub gold_next_turn: i32,
    pub upgrade_cost: i32,
    pub board: Vec<Minion>,
    pub hand: Vec<HandCard>,
    pub shop: Vec<ShopSlot>,
    pub free_refreshes: i32,
    pub trinkets: Vec<CardId>,
    pub dark_gift_uses: u32,
    pub dark_gift_used_turn: bool,
    pub deity: Option<DeityState>,
    pub stats: PlayerStats,
    pub pending: Vec<Pending>,
    pub scheduled: Vec<Scheduled>,
    /// Persistent "this game" effects (see `PlayerEffect`).
    #[serde(default)]
    pub effects: Vec<PlayerEffect>,
    /// Fodders to add to the next Refreshes: `fodder_queue[i]` for the i-th next one.
    #[serde(default)]
    pub fodder_queue: Vec<u8>,
    /// "Golden until next turn" minions to revert at the next turn start: (eid, card, atk,
    /// hp) = the original card and the stats the golden form added.
    #[serde(default)]
    pub revert_golden: Vec<(EntityId, CardId, i32, i32)>,
    /// Minions owed to a combat "when you have space" until actually summoned (Tavish's Lock
    /// and Load): (source card, minion, attacks a random enemy when summoned).
    #[serde(default)]
    pub owed: Vec<(CardId, Minion, bool)>,
    /// Cards from protected sources (Trinkets, Quest Rewards) waiting for hand space (official
    /// trinket rules 2026-04-06).
    #[serde(default)]
    pub hand_queue: Vec<Item>,
    /// What each waiting card keeps for when it reaches the hand, in step with
    /// `hand_queue` (a list of another length is stale and ignored).
    #[serde(default)]
    pub hand_queue_marks: Vec<QueueMark>,
    /// Extra Chronum for the next Timewarp visit (Timewarped Investment).
    #[serde(default)]
    pub chronum_saved: i32,
    /// Spendable until this recruit turn ends, including after leaving Timewarp.
    #[serde(default)]
    pub chronum_balance: i32,
    #[serde(default)]
    pub chronum_granted_turn: u32,
    #[serde(default)]
    pub buddy_button: Option<crate::experimental::BuddyButton>,
    /// Only direct Timewarp purchases may extend the hand.
    #[serde(default)]
    pub timewarp_hand_limit: usize,
    /// Improvement levels of non-minion cards ("improve this" on Rewards...).
    #[serde(default)]
    pub levels: Vec<(CardId, i32)>,
    /// Secrets in play (Akazamzarak): one-shot combat triggers, removed when they fire.
    #[serde(default)]
    pub secrets: Vec<CardId>,
    /// Tier 7 is unlocked for this player (Norgannon's Reward).
    #[serde(default)]
    pub tier7: bool,
    /// Active Quests; completed ones become `rewards`.
    #[serde(default)]
    pub quests: Vec<QuestState>,
    /// Quest Rewards owned (passive effects, dispatched like trinkets).
    #[serde(default)]
    pub rewards: Vec<CardId>,
    /// Progress counters of cards that are not minions (trinkets, hero powers...).
    #[serde(default)]
    pub counters: Vec<(CardId, i32)>,
    /// Active provenance of Cube-generated lesser trinkets.
    #[serde(default)]
    pub cube_sources: Vec<CardId>,
    /// Targets of unresolved transforming Discovers, in pending-menu stack order.
    #[serde(default)]
    pub discover_targets: Vec<(CardId, EntityId)>,
    /// The minions wearing hats of Asher the Haberdasher, one entry per +1/+1 hat: a hat
    /// passes to a friendly minion when its wearer is sold (`Ctx::pass_hats`).
    #[serde(default)]
    pub hats: Vec<EntityId>,
    /// A triple check was held back by an open target / Choose One menu
    /// (`Ctx::check_triples`): it runs when the menu is answered or cancelled.
    #[serde(default)]
    pub triples_held: bool,
    pub ended_turn: bool,
    pub last_opponent: Option<PlayerId>,
    /// Opponents of the most recent combats, newest last (None = the ghost); matchmaking avoids
    /// them.
    #[serde(default)]
    pub recent_opponents: Vec<Option<PlayerId>>,
    /// Opponent for this turn's combat, known during recruit (None with `next_is_ghost`).
    #[serde(default)]
    pub next_opponent: Option<PlayerId>,
    #[serde(default)]
    pub next_is_ghost: bool,
    /// Board at the end of the last recruit phase (ghost snapshot / scouting).
    pub last_board: Vec<Minion>,
    /// The warband this player took into the most recent *completed* combat round.
    /// `last_board` already holds this turn's finished board during the recruit phase. Only
    /// the player it fought has seen it: observations go through `Game::known_board`.
    #[serde(default)]
    pub seen_board: Vec<Minion>,
    /// What this player has seen of the others, by player id (`Game::known_board`).
    #[serde(default)]
    pub scouted: Vec<Scouted>,
    /// Diagnostics only: this seat is shown every warband of the last completed round (more than
    /// the game shows).
    #[serde(default)]
    pub sees_all_boards: bool,
    pub actions_this_turn: u32,
    /// Presses of the Freeze button this turn (`RulesConfig::freeze_presses_per_turn`).
    #[serde(default)]
    pub freezes_this_turn: u32,
    /// This seat is exempt from `RulesConfig::turn_action_caps` (e.g. a person's seat in a
    /// game whose other seats are capped). Set by the host (`DebugCmd::ActionCap`); without
    /// a cap table nobody is capped anyway.
    #[serde(default)]
    pub uncapped: bool,
    /// This turn is skipped (Faelin / A. F. Kay): no Gold, only Freeze and End Turn.
    #[serde(default)]
    pub skipping: bool,
    /// Per-turn progress of non-minion cards ("the first Pirate you buy each turn..."),
    /// cleared at the start of each turn.
    #[serde(default)]
    pub turn_data: Vec<(CardId, i32)>,
    /// Turn of the Greater Trinket offer when moved (Ornate Clock; 0 = rules default), and
    /// whether it happened.
    #[serde(default)]
    pub greater_turn: u32,
    #[serde(default)]
    pub greater_offered: bool,
    /// The next trinket offer is Lesser (Mysterious Orb).
    #[serde(default)]
    pub next_trinket_lesser: bool,
    /// The discounted "pivot" options of the current regular trinket offer: of the top-most paid
    /// offer when two are open at once.
    #[serde(default)]
    pub trinket_pivots: Vec<CardId>,
    /// The discounts of the paid trinket offers that wait under the top-most one (a Reward's offer
    /// opened on a trinket turn): each offer keeps its own and gets them back when the offer above
    /// it is settled.
    #[serde(default)]
    pub trinket_pivot_stack: Vec<Vec<CardId>>,
    /// Mishmash bookkeeping in the Tavern (`Ctx::mishmash_sync`): the Attack and maximum
    /// Health each Amalgam on the board had when a Mishmash last looked. `None` while no
    /// Mishmash is watching.
    #[serde(default)]
    pub mishmash_seen: Option<SeenStats>,
    /// Log lines of this player's start-of-turn effects (only when logging; for the UI).
    #[serde(default)]
    pub turn_events: Vec<String>,
}

impl Player {
    pub fn normalize_timewarp_hand_limit(&mut self, base: usize) {
        self.timewarp_hand_limit = if self.hand.len() > base {
            self.hand.len()
        } else {
            0
        };
    }

    /// The marks of the waiting cards, one per card (rebuilt empty when something else
    /// changed the queue without them).
    pub fn queue_marks(&mut self) -> &mut Vec<QueueMark> {
        if self.hand_queue_marks.len() != self.hand_queue.len() {
            self.hand_queue_marks = vec![QueueMark::default(); self.hand_queue.len()];
        }
        &mut self.hand_queue_marks
    }

    /// Take waiting card `i` out of the queue with its mark.
    pub fn queue_take(&mut self, i: usize) -> (Item, QueueMark) {
        let mark = self.queue_marks().remove(i);
        (self.hand_queue.remove(i), mark)
    }

    /// End of the turn: temporary cards (Spellcraft spells) leave the hand, and the queue
    /// of cards waiting for hand space; this is not a discard.
    pub fn drop_temporary_cards(&mut self) {
        self.hand.retain(|h| !h.temporary);
        let mut i = 0;
        while i < self.hand_queue.len() {
            if self.queue_marks()[i].temporary {
                self.queue_take(i);
            } else {
                i += 1;
            }
        }
    }

    pub fn new(id: PlayerId) -> Player {
        Player {
            id,
            hero: None,
            powers: Vec::new(),
            health: 30,
            armor: 0,
            alive: true,
            placement: 0,
            tier: 1,
            gold: 0,
            max_gold_bonus: 0,
            gold_next_turn: 0,
            upgrade_cost: 0,
            board: Vec::new(),
            hand: Vec::new(),
            shop: Vec::new(),
            free_refreshes: 0,
            trinkets: Vec::new(),
            dark_gift_uses: 0,
            dark_gift_used_turn: false,
            deity: None,
            stats: PlayerStats::default(),
            pending: Vec::new(),
            scheduled: Vec::new(),
            effects: Vec::new(),
            fodder_queue: Vec::new(),
            counters: Vec::new(),
            cube_sources: Vec::new(),
            discover_targets: Vec::new(),
            hats: Vec::new(),
            quests: Vec::new(),
            rewards: Vec::new(),
            tier7: false,
            revert_golden: Vec::new(),
            secrets: Vec::new(),
            levels: Vec::new(),
            owed: Vec::new(),
            hand_queue: Vec::new(),
            hand_queue_marks: Vec::new(),
            chronum_saved: 0,
            chronum_balance: 0,
            chronum_granted_turn: 0,
            buddy_button: None,
            timewarp_hand_limit: 0,
            triples_held: false,
            ended_turn: false,
            last_opponent: None,
            recent_opponents: Vec::new(),
            next_opponent: None,
            next_is_ghost: false,
            last_board: Vec::new(),
            seen_board: Vec::new(),
            scouted: Vec::new(),
            sees_all_boards: false,
            uncapped: false,
            actions_this_turn: 0,
            freezes_this_turn: 0,
            skipping: false,
            turn_data: Vec::new(),
            greater_turn: 0,
            greater_offered: false,
            next_trinket_lesser: false,
            trinket_pivots: Vec::new(),
            trinket_pivot_stack: Vec::new(),
            mishmash_seen: None,
            turn_events: Vec::new(),
        }
    }

    /// The primary (hero's own) Hero Power.
    pub fn hero_power(&self) -> Option<CardId> {
        self.powers.first().map(|h| h.card)
    }
    pub fn power_cards(&self) -> impl Iterator<Item = CardId> + '_ {
        self.powers.iter().map(|h| h.card)
    }
    pub fn has_power(&self, id: &str) -> bool {
        self.powers.iter().any(|h| db().get(h.card).id == id)
    }
    /// State of the Hero Power `card` (first match).
    pub fn power_of(&self, card: CardId) -> Option<&HeroPowerState> {
        self.powers.iter().find(|h| h.card == card)
    }
    pub fn power_of_mut(&mut self, card: CardId) -> Option<&mut HeroPowerState> {
        self.powers.iter_mut().find(|h| h.card == card)
    }
    /// Put `card` in power slot `slot` with fresh counters (a slot past the end appends).
    pub fn set_hero_power(&mut self, slot: usize, card: CardId) {
        let st = HeroPowerState::new(card);
        if slot < self.powers.len() {
            self.powers[slot] = st;
        } else {
            self.powers.push(st);
        }
    }
    /// Record a combat opponent (None = the ghost) for matchmaking (keeps the last 2).
    pub fn note_opponent(&mut self, o: Option<PlayerId>) {
        self.recent_opponents.push(o);
        if self.recent_opponents.len() > 2 {
            self.recent_opponents.remove(0);
        }
    }
    pub fn hero_is(&self, id: &str) -> bool {
        self.hero.map(|h| db().get(h).id == id).unwrap_or(false)
    }
    /// Any Tavern card is frozen.
    pub fn is_frozen(&self) -> bool {
        self.shop.iter().any(|s| s.frozen)
    }
    /// Freeze (or unfreeze) everything currently in the Tavern.
    pub fn set_frozen(&mut self, on: bool) {
        for s in self.shop.iter_mut() {
            s.frozen = on;
        }
    }
    pub fn has_reward(&self, id: &str) -> bool {
        self.rewards.iter().any(|t| db().get(*t).id == id)
    }
    pub fn has_trinket(&self, id: &str) -> bool {
        self.trinkets.iter().any(|t| db().get(*t).id == id)
    }
    /// Resolved-id variants for hot paths (`cardid!`).
    #[inline]
    pub fn has_trinket_id(&self, t: CardId) -> bool {
        // (`base`: Souvenir Stand's copy counts as the trinket.)
        self.trinkets.iter().any(|x| x.base() == t)
    }
    #[inline]
    pub fn trinket_count(&self, t: CardId) -> u32 {
        self.trinkets.iter().filter(|x| x.base() == t).count() as u32
    }
    #[inline]
    pub fn has_reward_id(&self, t: CardId) -> bool {
        self.rewards.contains(&t)
    }
    #[inline]
    pub fn has_power_id(&self, hp: CardId) -> bool {
        self.powers.iter().any(|h| h.card == hp)
    }
    pub fn count_minions_on_board(&self, id: &str) -> (usize, usize) {
        let mut n = 0;
        let mut g = 0;
        for m in &self.board {
            if m.is_card(id) {
                if m.golden { g += 1 } else { n += 1 }
            }
        }
        (n, g)
    }
}

/// Shared finite minion pool: remaining copies per card (indexed by CardId).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pool {
    pub counts: Vec<u16>,
}

impl Pool {
    pub fn empty() -> Pool {
        Pool {
            counts: vec![0; db().len()],
        }
    }
    pub fn get(&self, c: CardId) -> u16 {
        self.counts[c.0 as usize]
    }
    pub fn take(&mut self, c: CardId) -> bool {
        let n = &mut self.counts[c.0 as usize];
        if *n > 0 {
            *n -= 1;
            true
        } else {
            false
        }
    }
    pub fn put(&mut self, c: CardId, copies: u16) {
        self.counts[c.0 as usize] += copies;
    }
    pub fn return_minion(&mut self, m: &Minion) {
        m.visit_pool_parts(|c, n| self.put(c, n));
    }
}
