//! Small shared value types: races, keywords, mechanics, identifiers.

use serde::{Deserialize, Serialize};

/// Index of a player in the lobby (0..8).
pub type PlayerId = usize;
/// Unique id of a minion/card instance within one game. 0 = none.
pub type EntityId = u32;

/// Minion types as a bitmask. `ALL` sets every lobby type bit.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Races(pub u16);

impl Races {
    pub const NONE: Races = Races(0);
    pub const BEAST: Races = Races(1 << 0);
    pub const DEMON: Races = Races(1 << 1);
    pub const DRAGON: Races = Races(1 << 2);
    pub const ELEMENTAL: Races = Races(1 << 3);
    pub const MECH: Races = Races(1 << 4);
    pub const MURLOC: Races = Races(1 << 5);
    pub const PIRATE: Races = Races(1 << 6);
    pub const QUILBOAR: Races = Races(1 << 7);
    pub const UNDEAD: Races = Races(1 << 8);
    pub const ABERRATION: Races = Races(1 << 9);
    pub const NAGA: Races = Races(1 << 10);
    pub const ALL: Races = Races((1 << 11) - 1);
    pub const COUNT: usize = 11;
    pub const NAMES: [&'static str; 11] = [
        "BEAST",
        "DEMON",
        "DRAGON",
        "ELEMENTAL",
        "MECHANICAL",
        "MURLOC",
        "PIRATE",
        "QUILBOAR",
        "UNDEAD",
        "ABERRATION",
        "NAGA",
    ];

    pub fn from_name(s: &str) -> Races {
        if s == "ALL" {
            return Races::ALL;
        }
        Self::NAMES
            .iter()
            .position(|n| *n == s)
            .map(|i| Races(1 << i))
            .unwrap_or(Races::NONE)
    }
    pub fn contains(self, other: Races) -> bool {
        other.0 != 0 && self.0 & other.0 == other.0
    }
    pub fn intersects(self, other: Races) -> bool {
        self.0 & other.0 != 0
    }
    pub fn is_none(self) -> bool {
        self.0 == 0
    }
    pub fn union(self, other: Races) -> Races {
        Races(self.0 | other.0)
    }
    /// Iterate single-race bits.
    pub fn iter(self) -> impl Iterator<Item = Races> {
        (0..Self::COUNT)
            .map(|i| Races(1 << i))
            .filter(move |r| self.intersects(*r))
    }
    pub fn index(self) -> Option<usize> {
        (self.0.count_ones() == 1).then(|| self.0.trailing_zeros() as usize)
    }
}

impl std::fmt::Debug for Races {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if *self == Races::ALL {
            return write!(f, "ALL");
        }
        let names: Vec<_> = self
            .iter()
            .map(|r| Races::NAMES[r.index().unwrap()])
            .collect();
        write!(f, "{}", names.join("|"))
    }
}

/// Combat keywords carried by a minion instance.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Kw(pub u32);

impl Kw {
    pub const NONE: Kw = Kw(0);
    pub const TAUNT: Kw = Kw(1 << 0);
    pub const DIVINE_SHIELD: Kw = Kw(1 << 1);
    pub const WINDFURY: Kw = Kw(1 << 2);
    pub const REBORN: Kw = Kw(1 << 3);
    pub const VENOMOUS: Kw = Kw(1 << 4);
    pub const POISONOUS: Kw = Kw(1 << 5);
    pub const STEALTH: Kw = Kw(1 << 6);
    pub const MAGNETIC: Kw = Kw(1 << 7);
    /// Attacks four times per turn (golden Windfury effects).
    pub const MEGA_WINDFURY: Kw = Kw(1 << 8);
    /// Damages the minions next to its target too.
    pub const CLEAVE: Kw = Kw(1 << 9);
    /// Takes no damage.
    pub const IMMUNE: Kw = Kw(1 << 10);
    /// Takes no damage while attacking (Invulnerability gift).
    pub const IMMUNE_ATTACKING: Kw = Kw(1 << 11);

    /// The "Bonus Keywords" set: Taunt, Divine Shield, Reborn, Venomous, Windfury, Stealth.
    pub const BONUS: [Kw; 6] = [
        Kw::TAUNT,
        Kw::DIVINE_SHIELD,
        Kw::REBORN,
        Kw::VENOMOUS,
        Kw::WINDFURY,
        Kw::STEALTH,
    ];
    pub const NAMES: [&'static str; 12] = [
        "TAUNT",
        "DIVINE_SHIELD",
        "WINDFURY",
        "REBORN",
        "VENOMOUS",
        "POISONOUS",
        "STEALTH",
        "MAGNETIC",
        "MEGA_WINDFURY",
        "CLEAVE",
        "IMMUNE",
        "IMMUNE_ATTACKING",
    ];

    pub fn from_name(s: &str) -> Kw {
        Self::NAMES
            .iter()
            .position(|n| *n == s)
            .map(|i| Kw(1 << i))
            .unwrap_or(Kw::NONE)
    }
    pub fn has(self, k: Kw) -> bool {
        self.0 & k.0 == k.0 && k.0 != 0
    }
    pub fn set(&mut self, k: Kw) {
        self.0 |= k.0;
    }
    pub fn clear(&mut self, k: Kw) {
        self.0 &= !k.0;
    }
    pub fn union(self, k: Kw) -> Kw {
        Kw(self.0 | k.0)
    }
    pub fn count_bonus(self) -> usize {
        Kw::BONUS.iter().filter(|k| self.has(**k)).count()
    }
}

impl std::fmt::Debug for Kw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<_> = (0..Kw::NAMES.len())
            .filter(|i| self.0 & (1 << i) != 0)
            .map(|i| Kw::NAMES[i])
            .collect();
        write!(f, "{}", names.join("|"))
    }
}

/// Printed mechanics (text features) of a card definition; informational for the engine
/// (behaviour lives in scripts) but used for pool filters such as "a Battlecry minion".
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Debug)]
pub struct Mech(pub u32);

impl Mech {
    pub const BATTLECRY: Mech = Mech(1 << 0);
    pub const DEATHRATTLE: Mech = Mech(1 << 1);
    pub const START_OF_COMBAT: Mech = Mech(1 << 2);
    pub const END_OF_TURN: Mech = Mech(1 << 3);
    pub const AVENGE: Mech = Mech(1 << 4);
    pub const RALLY: Mech = Mech(1 << 5);
    pub const CHOOSE_ONE: Mech = Mech(1 << 6);
    pub const DISCOVER: Mech = Mech(1 << 7);
    pub const AURA: Mech = Mech(1 << 8);
    pub const MAGNETIC: Mech = Mech(1 << 9);
    pub const ACTIVATE: Mech = Mech(1 << 10);

    pub fn from_name(s: &str) -> Mech {
        match s {
            "BATTLECRY" => Mech::BATTLECRY,
            "DEATHRATTLE" => Mech::DEATHRATTLE,
            "START_OF_COMBAT" => Mech::START_OF_COMBAT,
            "END_OF_TURN_TRIGGER" => Mech::END_OF_TURN,
            "AVENGE" => Mech::AVENGE,
            "BACON_RALLY" => Mech::RALLY,
            "CHOOSE_ONE" => Mech::CHOOSE_ONE,
            "DISCOVER" => Mech::DISCOVER,
            "AURA" => Mech::AURA,
            "MAGNETIC" => Mech::MAGNETIC,
            _ => Mech(0),
        }
    }
    pub fn has(self, m: Mech) -> bool {
        self.0 & m.0 != 0
    }
}

/// Which side of an arena. In the recruit phase only side 0 is populated.
pub type Side = usize;
