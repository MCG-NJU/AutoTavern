//! Card database: static definitions generated from the resource bundle
//! (`tools/gen_carddb.py` -> `data/cards.json`, embedded at compile time).

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::types::{Kw, Mech, Races};

/// Small non-cryptographic string hasher (FxHash) for the id -> CardId map: the ids are
/// looked up by content scripts on hot paths, and SipHash was measurable in profiles.
#[derive(Default, Clone, Copy)]
pub struct FxHasher(u64);

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        const K: u64 = 0x517c_c1b7_2722_0a95;
        let mut h = self.0;
        let mut b = bytes;
        while b.len() >= 8 {
            let w = u64::from_le_bytes(b[..8].try_into().unwrap());
            h = (h.rotate_left(5) ^ w).wrapping_mul(K);
            b = &b[8..];
        }
        if b.len() >= 4 {
            let w = u32::from_le_bytes(b[..4].try_into().unwrap()) as u64;
            h = (h.rotate_left(5) ^ w).wrapping_mul(K);
            b = &b[4..];
        }
        for &x in b {
            h = (h.rotate_left(5) ^ x as u64).wrapping_mul(K);
        }
        self.0 = h;
    }
    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.0 = (self.0.rotate_left(5) ^ i as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

/// A card id resolved once and cached in a static: `cardid!("BG25_001")`. Use it instead of
/// `cid` / `db().id` wherever the lookup would otherwise run on a hot path.
#[macro_export]
macro_rules! cardid {
    ($id:expr) => {{
        static C: ::std::sync::OnceLock<$crate::cards::CardId> = ::std::sync::OnceLock::new();
        *C.get_or_init(|| $crate::cards::db().id($id))
    }};
}

/// Dense index into the card database.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct CardId(pub u16);

impl CardId {
    /// Marks a second instance of a card that keeps its own state: Souvenir Stand's copy of
    /// a trinket the player already owns. It has the same card data and script (`db().get`
    /// and `script` ignore the mark) but is a different key for per-card counters.
    pub const COPY: u16 = 0x8000;
    pub fn copy(self) -> CardId {
        CardId(self.0 | Self::COPY)
    }
    /// The card itself, without the copy mark.
    pub fn base(self) -> CardId {
        CardId(self.0 & !Self::COPY)
    }
    /// Index into per-card tables.
    #[inline]
    pub fn idx(self) -> usize {
        (self.0 & !Self::COPY) as usize
    }
}

impl std::fmt::Debug for CardId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match db().try_get(*self) {
            Some(c) => write!(f, "{}", c.id),
            None => write!(f, "CardId({})", self.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardKind {
    Minion,
    TavernSpell,
    Spell,
    Hero,
    HeroPower,
    TrinketLesser,
    TrinketGreater,
    DarkGift,
    Quest,
    QuestReward,
}

impl CardKind {
    fn parse(s: &str) -> CardKind {
        match s {
            "minion" => CardKind::Minion,
            "tavern_spell" => CardKind::TavernSpell,
            "spell" => CardKind::Spell,
            "hero" => CardKind::Hero,
            "hero_power" => CardKind::HeroPower,
            "trinket_lesser" => CardKind::TrinketLesser,
            "trinket_greater" => CardKind::TrinketGreater,
            "dark_gift" => CardKind::DarkGift,
            "quest" => CardKind::Quest,
            "quest_reward" => CardKind::QuestReward,
            other => panic!("unknown card kind {other}"),
        }
    }
    pub fn is_spell(self) -> bool {
        matches!(self, CardKind::TavernSpell | CardKind::Spell)
    }
    pub fn is_trinket(self) -> bool {
        matches!(self, CardKind::TrinketLesser | CardKind::TrinketGreater)
    }
}

/// Pool membership flags from rules/current_ruleset.json.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pools {
    pub minion: bool,
    pub tier7: bool,
    pub hero: bool,
    pub spell: bool,
    pub trinket: bool,
    /// Timewarp minions reachable by effects (Minor = Tier 3, Major = Tier 5).
    pub timewarp: bool,
}

#[derive(Clone, Debug)]
pub struct CardDef {
    pub cid: CardId,
    pub id: String,
    pub dbf: u32,
    pub kind: CardKind,
    pub name: String,
    pub name_zh: String,
    pub tier: u8,
    pub atk: i32,
    pub hp: i32,
    pub armor: i32,
    pub cost: i32,
    pub races: Races,
    /// Associated minion types (trinket offers; bundle `lobby_or_offer_association`).
    pub assoc: Races,
    /// Only in lobbies with one of these types (hsdata tag 4757: the Aberration subset of
    /// spells, trinkets and heroes). NONE = no requirement.
    pub lobby_req: Races,
    pub kw: Kw,
    pub mech: Mech,
    pub golden: Option<CardId>,
    pub normal: Option<CardId>,
    pub pools: Pools,
    pub text: String,
    pub text_zh: String,
    /// Raw client IS_BACON_POOL_MINION value (0 = absent).
    pub pool_tag: u8,
    /// Explicitly reachable from current pool roots (tokens, generated cards...).
    pub reachable: bool,
    pub hero_power: Option<CardId>,
    /// Spellcraft minion: the temporary spell it gives (BACON_SPELLCRAFT_ID).
    pub spellcraft: Option<CardId>,
    /// A Spellcraft spell (spell school SPELLCRAFT): temporary, discarded at end of turn.
    pub spellcraft_spell: bool,
    /// Quest progress goal (QUEST_PROGRESS_TOTAL); also counters of some rewards/minions.
    pub quest_goal: i32,
    /// Quest Reward: the card it refers to ("Get a copy of X"; BACON_CARD_DBID_REWARD).
    pub reward_card: Option<CardId>,
    /// Hero: its Buddy minion.
    pub buddy: Option<CardId>,
    /// Darkmoon Prize tier (1-4; 0 = not a prize).
    pub prize_tier: u8,
    /// Timewarp offers: the types this card is tied to (hsdata BACON_SUBSET_* tags; not
    /// always the minion's own types), its offer weight in percent, and the warband size
    /// it needs.
    pub tw_types: Races,
    pub tw_weight: u32,
    pub tw_min_minions: u8,
    /// Dark Gift offer window (first turn, last turn inclusive; None = open ended).
    pub window: Option<(u8, Option<u8>)>,
}

impl CardDef {
    pub fn is_golden(&self) -> bool {
        self.normal.is_some()
    }
    pub fn is_minion(&self) -> bool {
        self.kind == CardKind::Minion
    }
}

#[derive(Deserialize)]
struct RawDb {
    meta: RawMeta,
    cards: Vec<RawCard>,
}

#[derive(Deserialize)]
pub struct RawMeta {
    pub client_version: String,
    pub server_patch: String,
}

#[derive(Deserialize)]
struct RawCard {
    id: String,
    dbf: u32,
    kind: String,
    name: String,
    name_zh: String,
    tier: u8,
    atk: i32,
    hp: i32,
    armor: i32,
    cost: i32,
    races: Vec<String>,
    #[serde(default)]
    assoc: Vec<String>,
    #[serde(default)]
    lobby_req: Vec<String>,
    keywords: Vec<String>,
    golden: String,
    normal: String,
    pools: Vec<String>,
    text: String,
    #[serde(default)]
    text_zh: String,
    #[serde(default)]
    pool_tag: u8,
    #[serde(default)]
    reachable: bool,
    #[serde(default)]
    hero_power: String,
    #[serde(default)]
    spellcraft: String,
    #[serde(default)]
    spellcraft_spell: bool,
    #[serde(default)]
    quest_goal: i32,
    #[serde(default)]
    reward_card: String,
    #[serde(default)]
    buddy: String,
    #[serde(default)]
    prize_tier: u8,
    #[serde(default)]
    tw_types: Vec<String>,
    #[serde(default)]
    tw_weight: Option<u32>,
    #[serde(default)]
    tw_min_minions: u8,
    #[serde(default)]
    window: Option<(Option<u8>, Option<u8>)>,
}

pub struct CardDb {
    pub client_version: String,
    pub server_patch: String,
    /// A hash of the card ids in table order. A `CardId` is a position in this table, so a
    /// saved game is only good for the table it was written with (`Game::check_loaded`).
    pub fingerprint: u64,
    cards: Vec<CardDef>,
    by_id: FxMap<String, CardId>,
}

const CARDS_JSON: &str = include_str!("../data/cards.json");

static DB: OnceLock<CardDb> = OnceLock::new();

/// Global card database (parsed once on first use).
pub fn db() -> &'static CardDb {
    DB.get_or_init(|| CardDb::parse(CARDS_JSON))
}

impl CardDb {
    fn parse(json: &str) -> CardDb {
        let raw: RawDb = serde_json::from_str(json).expect("cards.json parses");
        let mut by_id = FxMap::with_capacity_and_hasher(raw.cards.len(), Default::default());
        for (i, c) in raw.cards.iter().enumerate() {
            by_id.insert(c.id.clone(), CardId(i as u16));
        }
        let lookup = |s: &str| {
            if s.is_empty() {
                None
            } else {
                by_id.get(s).copied()
            }
        };
        let cards = raw
            .cards
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let mut kw = Kw::NONE;
                let mut mech = Mech(0);
                for k in &c.keywords {
                    kw.set(Kw::from_name(k));
                    mech.0 |= Mech::from_name(k).0;
                }
                if c.text.contains("Activate (") {
                    mech.0 |= Mech::ACTIVATE.0;
                }
                let mut races = Races::NONE;
                for r in &c.races {
                    races = races.union(Races::from_name(r));
                }
                let mut assoc = Races::NONE;
                for r in &c.assoc {
                    assoc = assoc.union(Races::from_name(r));
                }
                CardDef {
                    cid: CardId(i as u16),
                    id: c.id.clone(),
                    dbf: c.dbf,
                    kind: CardKind::parse(&c.kind),
                    name: c.name.clone(),
                    name_zh: c.name_zh.clone(),
                    tier: c.tier,
                    atk: c.atk,
                    hp: c.hp,
                    armor: c.armor,
                    cost: c.cost,
                    races,
                    assoc,
                    lobby_req: c
                        .lobby_req
                        .iter()
                        .fold(Races::NONE, |a, r| a.union(Races::from_name(r))),
                    kw,
                    mech,
                    golden: lookup(&c.golden),
                    normal: lookup(&c.normal),
                    pools: Pools {
                        minion: c.pools.iter().any(|p| p == "minion"),
                        tier7: c.pools.iter().any(|p| p == "tier7"),
                        hero: c.pools.iter().any(|p| p == "hero"),
                        spell: c.pools.iter().any(|p| p == "spell"),
                        trinket: c.pools.iter().any(|p| p == "trinket"),
                        timewarp: c.pools.iter().any(|p| p == "timewarp"),
                    },
                    text: c.text.clone(),
                    text_zh: c.text_zh.clone(),
                    pool_tag: c.pool_tag,
                    reachable: c.reachable,
                    hero_power: lookup(&c.hero_power),
                    spellcraft: lookup(&c.spellcraft),
                    spellcraft_spell: c.spellcraft_spell,
                    quest_goal: c.quest_goal,
                    reward_card: lookup(&c.reward_card),
                    buddy: lookup(&c.buddy),
                    prize_tier: c.prize_tier,
                    tw_types: c
                        .tw_types
                        .iter()
                        .fold(Races::NONE, |a, r| a.union(Races::from_name(r))),
                    tw_weight: c.tw_weight.unwrap_or(100),
                    tw_min_minions: c.tw_min_minions,
                    window: c.window.map(|(a, b)| (a.unwrap_or(0), b)),
                }
            })
            .collect();
        // FNV-1a over the ids, each followed by a separator.
        let mut fingerprint = 0xcbf2_9ce4_8422_2325u64;
        for c in &raw.cards {
            for b in c.id.bytes().chain(std::iter::once(0)) {
                fingerprint = (fingerprint ^ b as u64).wrapping_mul(0x0100_0000_01b3);
            }
        }
        CardDb {
            client_version: raw.meta.client_version,
            server_patch: raw.meta.server_patch,
            fingerprint,
            cards,
            by_id,
        }
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
    pub fn get(&self, c: CardId) -> &CardDef {
        &self.cards[c.idx()]
    }
    pub fn try_get(&self, c: CardId) -> Option<&CardDef> {
        self.cards.get(c.idx())
    }
    pub fn find(&self, id: &str) -> Option<CardId> {
        self.by_id.get(id).copied()
    }
    /// Lookup by string id; panics with the id on a typo (scripts use this).
    pub fn id(&self, id: &str) -> CardId {
        self.find(id)
            .unwrap_or_else(|| panic!("card id not in database: {id}"))
    }
    pub fn iter(&self) -> impl Iterator<Item = &CardDef> {
        self.cards.iter()
    }
    /// Golden form of a card if it has one (identity otherwise).
    pub fn golden_of(&self, c: CardId) -> CardId {
        self.get(c).golden.unwrap_or(c)
    }
    /// Normal form of a card (identity for non-golden cards).
    pub fn normal_of(&self, c: CardId) -> CardId {
        self.get(c).normal.unwrap_or(c)
    }
}

/// Convenience: `cid("BG25_001")`.
pub fn cid(id: &str) -> CardId {
    db().id(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_and_links_golden() {
        let d = db();
        assert!(d.len() > 3000);
        assert_eq!(d.client_version, crate::GAME_VERSION);
        let rider = d.id("BG25_001");
        let def = d.get(rider);
        assert_eq!((def.tier, def.atk, def.hp), (1, 2, 1));
        assert!(def.kw.has(Kw::TAUNT) && def.kw.has(Kw::REBORN));
        assert!(def.races.contains(Races::UNDEAD));
        assert!(def.pools.minion);
        let g = d.golden_of(rider);
        assert_ne!(g, rider);
        assert_eq!(d.normal_of(g), rider);
        assert_eq!(d.get(g).atk, 4);
    }

    #[test]
    fn pool_counts_match_bundle() {
        let d = db();
        assert_eq!(d.iter().filter(|c| c.pools.minion).count(), 245);
        assert_eq!(d.iter().filter(|c| c.pools.hero).count(), 116);
        assert_eq!(d.iter().filter(|c| c.pools.spell).count(), 70);
        assert_eq!(d.iter().filter(|c| c.pools.trinket).count(), 236);
    }

    #[test]
    fn all_races_sentinel() {
        let paradox = db().get(db().id("BG36_360t3"));
        assert_eq!(paradox.races, Races::ALL);
    }
}
