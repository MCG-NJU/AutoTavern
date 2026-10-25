//! Game events dispatched to card scripts.

use crate::cards::CardId;
use crate::types::{EntityId, Kw, Races, Side};

/// Identity of the card actually played, captured before effects change its host.
/// A Magnetic play delivers buffs to `Played::eid` but checks this card's types.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlayInfo {
    pub source: EntityId,
    pub card: CardId,
    pub races: Races,
    pub atk: i32,
    pub tier: u8,
    pub golden: bool,
    pub battlecry: bool,
    pub repeated: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A minion was played from hand (after its Battlecry resolved).
    Played {
        side: Side,
        eid: EntityId,
        played: PlayInfo,
    },
    /// A minion entered the board (played or summoned).
    Summoned {
        side: Side,
        eid: EntityId,
    },
    /// A card was bought from the tavern.
    Bought {
        side: Side,
        card: CardId,
        eid: EntityId,
        /// The id the card had in the Tavern (a bought spell gets a new hand id).
        sid: EntityId,
        /// Gold actually paid (after discounts).
        price: i32,
    },
    /// A minion was sold (with its stats when sold).
    Sold {
        side: Side,
        card: CardId,
        eid: EntityId,
        atk: i32,
        hp: i32,
    },
    /// A spell was cast. `tavern` = a Tavern spell (not Blood Gems, rewards...).
    SpellCast {
        side: Side,
        card: CardId,
        target: EntityId,
        tavern: bool,
        /// The cast's data and Choose One option (for recasts: Lovely Locket).
        data: i32,
        opt: u8,
    },
    /// The tavern was refreshed (button, effect or turn start).
    /// `auto`: not your Refresh -- the turn-start refill or an effect's Refresh. It triggers "after
    /// the Tavern is Refreshed" effects but doesn't count toward "you Refresh (N times)".
    Refreshed {
        side: Side,
        auto: bool,
    },
    GoldSpent {
        side: Side,
        amount: i32,
    },
    /// A minion attacks (fires before damage).
    Attacks {
        side: Side,
        attacker: EntityId,
        defender: EntityId,
    },
    Damaged {
        side: Side,
        eid: EntityId,
        amount: i32,
        source: EntityId,
    },
    ShieldLost {
        side: Side,
        eid: EntityId,
    },
    Died {
        side: Side,
        eid: EntityId,
        card: CardId,
        races: Races,
    },
    /// `side` is the killer's side. Emitted whether or not the killer survived (mutual kills and
    /// minion-sourced effect damage count).
    Killed {
        side: Side,
        killer: EntityId,
        victim: EntityId,
    },
    Reborn {
        side: Side,
        eid: EntityId,
    },
    HeroDamaged {
        side: Side,
        amount: i32,
        in_combat: bool,
    },
    CardAdded {
        side: Side,
        hid: EntityId,
    },
    Discarded {
        side: Side,
        card: CardId,
    },
    Magnetized {
        side: Side,
        host: EntityId,
        card: CardId,
    },
    Upgraded {
        side: Side,
        tier: u8,
    },
    BattlecryTriggered {
        side: Side,
        eid: EntityId,
    },
    DeathrattleTriggered {
        side: Side,
        eid: EntityId,
    },
    /// A Blood Gem was played on a minion.
    GemPlayed {
        side: Side,
        target: EntityId,
    },
    /// A minion was consumed (from the tavern or the warband). `eater` gained its stats;
    /// `atk`/`hp`/`kw` describe the consumed minion.
    Consumed {
        side: Side,
        eater: EntityId,
        /// Consumed from the tavern (false: a friendly minion).
        tavern: bool,
        card: CardId,
        atk: i32,
        hp: i32,
        kw: Kw,
    },
    /// An attack finished (after its damage).
    AttackDone {
        side: Side,
        attacker: EntityId,
    },
    /// A Discover choice resolved (the card went to hand, or waits for hand space). `hid` is
    /// the hand id it has or was promised (0: none).
    Discovered {
        side: Side,
        card: CardId,
        hid: EntityId,
    },
    /// Any card played from hand (minion, spell, magnetize).
    CardPlayed {
        side: Side,
        card: CardId,
        eid: EntityId,
    },
}

/// Bit per event kind; scripts declare which kinds they listen to.
pub mod ev {
    pub const PLAYED: u32 = 1 << 0;
    pub const SUMMONED: u32 = 1 << 1;
    pub const BOUGHT: u32 = 1 << 2;
    pub const SOLD: u32 = 1 << 3;
    pub const SPELL_CAST: u32 = 1 << 4;
    pub const REFRESHED: u32 = 1 << 5;
    pub const GOLD_SPENT: u32 = 1 << 6;
    pub const ATTACKS: u32 = 1 << 7;
    pub const DAMAGED: u32 = 1 << 8;
    pub const SHIELD_LOST: u32 = 1 << 9;
    pub const DIED: u32 = 1 << 10;
    pub const KILLED: u32 = 1 << 11;
    pub const REBORN: u32 = 1 << 12;
    pub const HERO_DAMAGED: u32 = 1 << 13;
    pub const CARD_ADDED: u32 = 1 << 14;
    pub const DISCARDED: u32 = 1 << 15;
    pub const MAGNETIZED: u32 = 1 << 16;
    pub const UPGRADED: u32 = 1 << 17;
    pub const BATTLECRY: u32 = 1 << 18;
    pub const DEATHRATTLE: u32 = 1 << 19;
    pub const CARD_PLAYED: u32 = 1 << 20;
    pub const GEM_PLAYED: u32 = 1 << 21;
    pub const CONSUMED: u32 = 1 << 22;
    pub const DISCOVERED: u32 = 1 << 23;
    pub const ATTACK_DONE: u32 = 1 << 24;
}

impl Event {
    pub fn kind(&self) -> u32 {
        match self {
            Event::Played { .. } => ev::PLAYED,
            Event::Summoned { .. } => ev::SUMMONED,
            Event::Bought { .. } => ev::BOUGHT,
            Event::Sold { .. } => ev::SOLD,
            Event::SpellCast { .. } => ev::SPELL_CAST,
            Event::Refreshed { .. } => ev::REFRESHED,
            Event::GoldSpent { .. } => ev::GOLD_SPENT,
            Event::Attacks { .. } => ev::ATTACKS,
            Event::Damaged { .. } => ev::DAMAGED,
            Event::ShieldLost { .. } => ev::SHIELD_LOST,
            Event::Died { .. } => ev::DIED,
            Event::Killed { .. } => ev::KILLED,
            Event::Reborn { .. } => ev::REBORN,
            Event::HeroDamaged { .. } => ev::HERO_DAMAGED,
            Event::CardAdded { .. } => ev::CARD_ADDED,
            Event::Discarded { .. } => ev::DISCARDED,
            Event::Magnetized { .. } => ev::MAGNETIZED,
            Event::Upgraded { .. } => ev::UPGRADED,
            Event::BattlecryTriggered { .. } => ev::BATTLECRY,
            Event::DeathrattleTriggered { .. } => ev::DEATHRATTLE,
            Event::CardPlayed { .. } => ev::CARD_PLAYED,
            Event::GemPlayed { .. } => ev::GEM_PLAYED,
            Event::Consumed { .. } => ev::CONSUMED,
            Event::Discovered { .. } => ev::DISCOVERED,
            Event::AttackDone { .. } => ev::ATTACK_DONE,
        }
    }

    /// The side the event belongs to ("friendly" for listeners on that side).
    pub fn side(&self) -> Side {
        match *self {
            Event::Played { side, .. }
            | Event::Summoned { side, .. }
            | Event::Bought { side, .. }
            | Event::Sold { side, .. }
            | Event::SpellCast { side, .. }
            | Event::Refreshed { side, .. }
            | Event::GoldSpent { side, .. }
            | Event::Attacks { side, .. }
            | Event::Damaged { side, .. }
            | Event::ShieldLost { side, .. }
            | Event::Died { side, .. }
            | Event::Killed { side, .. }
            | Event::Reborn { side, .. }
            | Event::HeroDamaged { side, .. }
            | Event::CardAdded { side, .. }
            | Event::Discarded { side, .. }
            | Event::Magnetized { side, .. }
            | Event::Upgraded { side, .. }
            | Event::BattlecryTriggered { side, .. }
            | Event::DeathrattleTriggered { side, .. }
            | Event::CardPlayed { side, .. }
            | Event::GemPlayed { side, .. }
            | Event::Consumed { side, .. }
            | Event::Discovered { side, .. }
            | Event::AttackDone { side, .. } => side,
        }
    }
}
