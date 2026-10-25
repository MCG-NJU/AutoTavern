//! Game container and the effect-resolution context shared by recruit and combat.
//!
//! `Ctx` owns up to two "arena" boards. In the recruit phase the acting player's board is
//! moved into side 0 (and moved back afterwards); in combat both warbands are cloned into
//! sides 0/1. All card scripts act through `Ctx` primitives, so the same deathrattle,
//! summon or buff code works in both phases (e.g. a Deathrattle triggered in the tavern).

use serde::{Deserialize, Serialize};

use crate::cards::{CardDef, CardId, CardKind, db};
use crate::config::RulesConfig;
use crate::event::Event;
use crate::rng::Rng;
use crate::script::{Me, Script, TargetSpec, flag, script};
use crate::state::{HandCard, Item, Minion, Pending, Player, Pool};
use crate::types::{EntityId, Kw, PlayerId, Races, Side};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    HeroSelect,
    Recruit,
    GameOver,
}

/// A simulator driver stopped without a game-rule terminal result. This never gives
/// the survivors invented placements or changes Phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoppedReason {
    NoProgress,
    DriverGuard,
}

impl StoppedReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoProgress => "no_progress",
            Self::DriverGuard => "driver_guard",
        }
    }
}

fn diagnostic_is_zero(n: &u32) -> bool {
    *n == 0
}

/// Summary of one combat for logs/observations.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CombatRecord {
    pub turn: u32,
    pub a: PlayerId,
    /// None = ghost.
    pub b: Option<PlayerId>,
    /// 0 = a won, 1 = b won, 2 = tie.
    pub winner: u8,
    pub damage: i32,
    pub truncated: bool,
    /// Replay frames (only recorded when `cfg.record_log`).
    #[serde(default)]
    pub frames: Vec<CombatFrame>,
}

/// One replay step: log lines since the previous frame and both boards afterwards.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CombatFrame {
    pub log: Vec<String>,
    pub boards: [Vec<Minion>; 2],
    /// Attacks made since the previous frame: (attacker, defender), for the replay.
    #[serde(default)]
    pub attacks: Vec<(EntityId, EntityId)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub cfg: RulesConfig,
    pub seed: u64,
    pub rng: Rng,
    pub turn: u32,
    pub phase: Phase,
    pub players: Vec<Player>,
    pub pool: Pool,
    pub next_eid: EntityId,
    /// Minion types in this lobby.
    pub lobby_races: Races,
    /// Lobby deity (Aberration system).
    pub deity: Option<CardId>,
    /// The Dark Paradox variant enabled in this lobby.
    pub paradox: Option<CardId>,
    /// Normal-form minions that can be offered/generated in this lobby.
    pub universe: Vec<CardId>,
    /// Compact index over `universe` for pool draws; rebuilt on demand (`universe_index`),
    /// never serialized.
    #[serde(skip)]
    pub uidx: UniverseIndex,
    /// Tavern spells that can be offered in this lobby.
    pub spell_universe: Vec<CardId>,
    pub trinket_universe: Vec<CardId>,
    pub gift_universe: Vec<CardId>,
    /// Player ids in elimination order.
    pub eliminated: Vec<PlayerId>,
    /// Set by the round for its next ghost combat: whose warband the ghost is.
    #[serde(default)]
    pub ghost_player: Option<PlayerId>,
    /// A player's end of turn is resolving: no triples until the next turn start.
    #[serde(default)]
    pub ending_turn: bool,
    pub last_combats: Vec<CombatRecord>,
    /// Combats of this game that a simulator guard cut short (attack, effect-depth or
    /// death-loop limit): they end as ties, which is not a game rule. Counted so that
    /// callers can report how often it happens.
    #[serde(default)]
    pub truncated_combats: u32,
    /// Non-combat effect contexts cut short by a safety guard. At most one count per
    /// context, regardless of how many effects subsequently hit that same guard.
    #[serde(default, skip_serializing_if = "diagnostic_is_zero")]
    pub truncated_recruit_contexts: u32,
    /// A driver failure, distinct from normal game over / max-turn rankings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_reason: Option<StoppedReason>,
    /// The game was ended by `max_turns` (survivors ranked by Health), not by the rules.
    #[serde(default)]
    pub turn_limit_end: bool,
    /// `CardDb::fingerprint` of the card table this game was created with (0 = a state
    /// saved before the field existed). Card ids are positions in that table.
    #[serde(default)]
    pub card_table: u64,
    /// This turn's pairings, decided at turn start (None = ghost opponent).
    #[serde(default)]
    pub pairs: Vec<(PlayerId, Option<PlayerId>)>,
    /// Players alive at the start of this turn's recruit phase: the damage cap is lifted from that
    /// point, not mid-round. 0 = unknown (old snapshots).
    #[serde(default)]
    pub turn_alive: usize,
    pub log: Vec<String>,
    /// Frames of the most recent simulated combat (moved into its `CombatRecord`).
    #[serde(skip)]
    pub last_frames: Vec<CombatFrame>,
}

impl Game {
    /// Whether this build can play a game that was just read from a saved state: its
    /// configuration is valid and it was written with this card table. A state from another
    /// table would be read as other cards, or panic at the first pool access.
    pub fn check_loaded(&self) -> Result<(), String> {
        self.cfg.validate()?;
        let d = db();
        if self.card_table != 0 && self.card_table != d.fingerprint {
            return Err(format!(
                "the state was saved with another card table ({:016x}; this build has {:016x})",
                self.card_table, d.fingerprint
            ));
        }
        if self.pool.counts.len() != d.len() {
            return Err(format!(
                "the state's pool table has {} entries, the card table {}",
                self.pool.counts.len(),
                d.len()
            ));
        }
        Ok(())
    }

    pub fn alloc_eid(&mut self) -> EntityId {
        self.next_eid += 1;
        self.next_eid
    }

    /// The universe index (built lazily; a snapshot loses it).
    pub fn universe_index(&mut self) -> &UniverseIndex {
        if self.uidx.meta.len() != self.universe.len() {
            self.uidx = UniverseIndex::build(&self.universe);
        }
        &self.uidx
    }

    pub fn log(&mut self, f: impl FnOnce() -> String) {
        if self.cfg.record_log {
            let s = f();
            self.log.push(format!("[t{}] {}", self.turn, s));
        }
    }

    /// Deliver queued protected cards while the hand has space.
    pub fn flush_hand_queue(&mut self, p: PlayerId) {
        while !self.players[p].hand_queue.is_empty()
            && self.players[p].hand.len() < self.cfg.hand_size
        {
            let (item, mark) = self.players[p].queue_take(0);
            self.with_recruit(p, |c| c.deliver_queued(0, item, mark));
        }
    }

    pub fn alive_count(&self) -> usize {
        self.players.iter().filter(|p| p.alive).count()
    }

    /// Tavern spell copies left in the pool: the tier's copies minus those in any Tavern or hand.
    /// Cast, discarded or refreshed-away spells return by themselves; extra copies made with none
    /// left keep the count at 0 until enough are gone.
    pub fn spell_copies_left(&self, card: CardId) -> u64 {
        let t = db().get(card).tier as usize;
        if t == 0 || t > 7 {
            return 0;
        }
        let total = self.cfg.spell_copies_by_tier[t.min(6) - 1] as usize;
        let used: usize = self
            .players
            .iter()
            .map(|p| {
                p.shop.iter().filter(|s| s.item.card() == card).count()
                    + p.hand.iter().filter(|h| h.item.card() == card).count()
            })
            .sum();
        total.saturating_sub(used) as u64
    }

    /// Run `f` with player `p`'s board moved into a recruit-phase context.
    pub fn with_recruit<R>(&mut self, p: PlayerId, f: impl FnOnce(&mut Ctx) -> R) -> R {
        let board = std::mem::take(&mut self.players[p].board);
        let mut ctx = Ctx::new(self, [board, Vec::new()], [Some(p), None], false);
        ctx.bump_player_stat(0, |_| {});
        let r = f(&mut ctx);
        ctx.bump_player_stat(0, |_| {});
        ctx.keep_tavern_full(0);
        let board = std::mem::take(&mut ctx.boards[0]);
        drop(ctx);
        self.players[p].board = board;
        self.trim_shop(p);
        r
    }

    /// Remove one card of the default Refresh fill from `p`'s Tavern (the newest minion,
    /// else the newest spell) and return its pool copy. None when there is no such card.
    pub fn drop_filled_card(&mut self, p: PlayerId) -> Option<EntityId> {
        let shop = &self.players[p].shop;
        let pick = |minion: bool| {
            shop.iter()
                .rposition(|s| s.filled && s.item.as_minion().is_some() == minion)
        };
        let i = pick(true).or_else(|| pick(false))?;
        let s = self.players[p].shop.remove(i);
        if let Item::Minion(m) = s.item {
            self.pool.return_minion(&m);
        }
        Some(s.sid)
    }

    /// The Tavern holds at most `shop_max_cards` (7) cards, whatever happens. Over the
    /// limit, cards of the default fill go first (they have the lowest priority), then the
    /// newest of the others; pool copies go back.
    fn trim_shop(&mut self, p: PlayerId) {
        let cap = self.cfg.shop_max_cards.min(crate::actions::N_SHOP);
        while self.players[p].shop.len() > cap {
            if self.drop_filled_card(p).is_some() {
                continue;
            }
            if let Some(s) = self.players[p].shop.pop()
                && let Item::Minion(m) = s.item
            {
                self.pool.return_minion(&m);
            }
        }
    }

    pub fn player_tier(&self, p: PlayerId) -> u8 {
        self.players[p].tier
    }
}

/// Compact per-entry data of `Game::universe` for pool draws: they run for every Tavern
/// slot of every Refresh and must not walk the full card definitions.
#[derive(Clone, Debug, Default)]
pub struct UniverseIndex {
    /// (tier, races) of each `universe` entry.
    pub meta: Vec<(u8, u16)>,
    /// `universe` index range `[start, end)` of tier `t` (index 1..=7) when the universe is
    /// sorted by tier (as `Game::new` builds it); an unsorted universe (an old snapshot)
    /// makes every range the whole universe.
    pub tier_range: [(usize, usize); 8],
}

impl UniverseIndex {
    pub fn build(universe: &[CardId]) -> UniverseIndex {
        let d = db();
        let meta: Vec<(u8, u16)> = universe
            .iter()
            .map(|&c| {
                let def = d.get(c);
                (def.tier, def.races.0)
            })
            .collect();
        let sorted = meta.windows(2).all(|w| w[0].0 <= w[1].0);
        let mut tier_range = [(0, meta.len()); 8];
        if sorted {
            for (t, r) in tier_range.iter_mut().enumerate().skip(1) {
                let start = meta.partition_point(|m| (m.0 as usize) < t);
                let end = meta.partition_point(|m| (m.0 as usize) <= t);
                *r = (start, end);
            }
        }
        UniverseIndex { meta, tier_range }
    }

    /// `universe` index range covering tiers `min..=max`.
    pub fn range(&self, min: u8, max: u8) -> std::ops::Range<usize> {
        if min > max || max == 0 {
            return 0..0;
        }
        let lo = self.tier_range[min.clamp(1, 7) as usize].0;
        let hi = self.tier_range[max.clamp(1, 7) as usize].1;
        lo..hi.max(lo)
    }
}

/// Copies of `card` the player needs for a Golden one (Mister Clocksworth and two other
/// effects lower it to 2).
pub fn triple_need_of(p: &Player, cfg: &RulesConfig, card: CardId) -> usize {
    if p.has_power_id(crate::cardid!("BG34_HERO_002p"))
        || p.has_reward_id(crate::cardid!("BG24_Reward_350"))
    {
        return 2;
    }
    if p.has_trinket_id(crate::cardid!("BG30_MagicItem_439"))
        && db().get(card).races.intersects(Races::PIRATE)
    {
        return 2;
    }
    cfg.triple_copies
}

/// A friendly minion that died (for "died this combat" memory, e.g. Y'Shaarj).
#[derive(Clone, Debug)]
pub struct DeadRecord {
    pub eid: EntityId,
    pub card: CardId,
    pub atk: i32,
    pub max_hp: i32,
    pub peak_atk: i32,
    pub peak_hp: i32,
    /// Stats it had from Blood Gems.
    pub gem_atk: i32,
    pub gem_hp: i32,
    /// Its script counter when it died ("improve this" on a Deathrattle).
    pub counter: i32,
    /// It had a Deathrattle (its own, attached or gained) when it died.
    pub had_deathrattle: bool,
    /// All actual Deathrattles carried by the dead entity, preserving repeated sources.
    pub deathrattle_cards: Vec<CardId>,
    /// Its minion types when it died (may differ from the card's printed types).
    pub races: Races,
    /// Board position it died at (among the survivors).
    pub pos: usize,
}

impl Ctx<'_> {
    /// Death record of a minion that died this combat (most recent first).
    pub fn dead_record(&self, side: Side, eid: EntityId) -> Option<&DeadRecord> {
        self.cs[side].dead.iter().rev().find(|d| d.eid == eid)
    }

    /// Attack and maximum Health that "this minion's stats" means in a Deathrattle of `eid`:
    /// the living minion's when the Deathrattle is triggered without a death (Deathstrider,
    /// Wolfhead Flail..., in the Tavern as well as in combat), else its death record.
    pub fn deathrattle_stats(&self, side: Side, eid: EntityId) -> Option<(i32, i32)> {
        match self.minion(eid) {
            Some(m) => Some((m.atk, m.max_hp)),
            None => self.dead_record(side, eid).map(|d| (d.atk, d.max_hp)),
        }
    }
}

/// A summon that waits until the board has space ("When you have space...").
#[derive(Clone, Debug)]
pub enum SpaceReq {
    /// A prebuilt minion (tokens...).
    Minion(Minion),
    /// A minion that comes back where one stood (Teron's copy): left of the minion that was
    /// to its right (so right of what its own Deathrattle summoned there), at the far right
    /// if it had none, else at the index it had.
    InPlace(Minion, EntityId, usize),
    /// An awakening deity (runs its "after this awakens" effect once summoned).
    Deity(Minion),
    /// A hand card summoned "for this combat only".
    FromHand(EntityId),
    /// The highest-Attack hand minion of a type, chosen when space appears (Diremuck Forager).
    FromHandBest(Races),
    /// A copy of your highest-Attack (or highest-Health) minion, chosen when space appears.
    CopyHighest { by_health: bool },
    /// A minion that attacks immediately once summoned.
    AttackNow(Minion),
    /// A minion owed by a player effect `card` ("(2 left!)"): each one summoned uses up one
    /// stack of that effect.
    Tracked(Minion, CardId),
    /// An owed minion (`Player::owed`, matched by eid): removed from the list once summoned;
    /// `fire` = it then attacks a random enemy minion.
    Owed(Minion, bool),
}

/// Per-side combat bookkeeping.
#[derive(Clone, Debug, Default)]
pub struct SideState {
    pub deaths: u32,
    pub aberration_deaths: u32,
    pub deity_awakened: bool,
    /// The minion whose attack turn is (or was last) running: summons left of it wait for the next
    /// round.
    pub last_attacker: EntityId,
    /// The awakened Deity on the board this combat (0 = none).
    pub deity_eid: EntityId,
    pub deity_waiting: bool,
    /// Friendly minions that died this combat, in order.
    pub dead: Vec<DeadRecord>,
    pub tier: u8,
    pub attacks_made: u32,
    /// "When you have space" requests, served first-in first-out as the board frees up.
    pub waiting: Vec<SpaceReq>,
    /// Trinket per-combat counters (e.g. Reinforced Shield uses).
    pub trinket_uses: u32,
    /// Per-combat progress of player-level cards (trinket Avenge, "first... each combat").
    pub card_data: Vec<(CardId, i32)>,
    /// Hand cards already summoned "for this combat only" (can't be summoned twice).
    pub from_hand: Vec<EntityId>,
    /// Mishmash bookkeeping for this combat (`Ctx::mishmash_sync`; `Player::mishmash_seen`
    /// is the Tavern's).
    pub mishmash_seen: Option<crate::state::SeenStats>,
    /// Deathly Striker: hand ids gained by each Striker this combat (summoned on death).
    pub striker_refs: std::collections::HashMap<EntityId, Vec<EntityId>>,
    /// Exact copies stored by a minion this combat (Stitched Salvager).
    pub stored: std::collections::HashMap<EntityId, Vec<Minion>>,
    /// Full death snapshots only when Timewarped Radio Star can copy a dead killer.
    pub dead_copies: std::collections::HashMap<EntityId, Minion>,
    /// Humming Bird / Goldrinn: stats your Beasts have for the rest of this combat (also
    /// later summons, even after the source dies).
    pub beast_atk_aura: i32,
    pub beast_hp_aura: i32,
}

/// Log label of a side's owner: prints as "p3" or "ghost". It is `Copy` and formats lazily,
/// so building one costs nothing when logging is off.
#[derive(Clone, Copy, Debug)]
pub struct Who(pub Option<PlayerId>);

impl std::fmt::Display for Who {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(p) => write!(f, "p{p}"),
            None => f.write_str("ghost"),
        }
    }
}

/// One event listener: (eid, card, golden, where: 0 = player-level, 1 = board, 2 = hand,
/// 3 = persistent effect).
type Listener = (EntityId, CardId, bool, u8);

/// Listeners of one event side, kept on the stack (a board of 7 with attachments and a
/// few player-level cards fits; anything beyond spills to the heap).
struct Listeners {
    inline: [Listener; 24],
    len: usize,
    spill: Vec<Listener>,
}

impl Listeners {
    fn new() -> Self {
        Listeners {
            inline: [(0, CardId(0), false, 0); 24],
            len: 0,
            spill: Vec::new(),
        }
    }
    #[inline]
    fn push(&mut self, l: Listener) {
        if self.len < self.inline.len() {
            self.inline[self.len] = l;
            self.len += 1;
        } else {
            self.spill.push(l);
        }
    }
    fn clear(&mut self) {
        self.len = 0;
        self.spill.clear();
    }
    fn iter(&self) -> impl Iterator<Item = &Listener> {
        self.inline[..self.len].iter().chain(self.spill.iter())
    }
}

pub struct Ctx<'g> {
    pub g: &'g mut Game,
    pub boards: [Vec<Minion>; 2],
    pub owner: [Option<PlayerId>; 2],
    pub combat: bool,
    pub cs: [SideState; 2],
    /// Per-side combat counters (`CombatOutcome::stats`).
    pub cstats: [crate::combat::CombatStats; 2],
    pub depth: u32,
    /// Replay frames being recorded (combat with logging only).
    pub frames: Vec<CombatFrame>,
    /// Card whose effect is currently resolving (for log attribution).
    pub src: Option<CardId>,
    /// Spell being cast from hand: the Spellcraft minion that gave it (0 = none).
    pub cast_src: EntityId,
    /// The card the player is casting from hand right now (None otherwise).
    pub cast_card: Option<CardId>,
    /// The spell being cast was played from hand (not a copy / effect cast).
    pub cast_from_hand: bool,
    /// Physical hand card behind this cast, shared by its extra casts.
    pub cast_hid: EntityId,
    /// Includes the final Timewarp purchase, after its menu has already closed.
    pub timewarp_purchase: [bool; 2],
    physical_spell_uses: Vec<(EntityId, EntityId, bool)>,
    /// A spell is resolving (casts started inside it are effect casts).
    pub in_cast: bool,
    /// Which half of a cast / of playing a minion is being announced: 1 = "whenever you cast /
    /// play" (before the spell's effect / the Battlecry), 2 = "after you..."; 0 = no split (every
    /// listener hears the event). See `content::acts_whenever`.
    pub spell_phase: u8,
    pub play_phase: u8,
    /// The next `cast_spell` is a cast the card text has you make (set by `you_cast`).
    pub cast_by_you: bool,
    /// The spell being cast was aimed at a living friendly minion when the cast began: "after
    /// you cast a spell on a friendly minion" still knows it when the spell destroyed or moved
    /// its target (Lovely Locket after Butchering).
    pub cast_on_friendly: bool,
    /// The hand id reserved by the last `add_to_hand` that put its card in the waiting
    /// queue (None: the card is in the hand, or was lost). `None` from `add_to_hand` alone
    /// does not tell the two apart.
    pub queued: Option<EntityId>,
    /// The hand id a waiting card was promised (`Player::hand_queue_marks`), used by the
    /// `add_to_hand` that delivers it.
    deliver_hid: Option<EntityId>,
    /// Hero damage being resolved that can still be rewound: (side, armor, health) lost.
    pub rewind: Option<(Side, i32, i32)>,
    /// A Polarizing Beatboxer echo Magnetization is resolving (no further echoes).
    pub echo_magnetize: bool,
    /// The side that attacks first this combat: it also resolves first at Start of Combat, for
    /// simultaneous deaths and for queued summons.
    pub first: Side,
    /// A combat exchange is dealing its damage: Damaged / ShieldLost wait until both sides dealt
    /// theirs.
    pub defer_hits: bool,
    deferred: Vec<Event>,
    /// Cleave hits keep the attacker's Venomous for every target.
    pub keep_venom: bool,
    /// Triples wait during a summon effect batch, or until next turn after turn end.
    pub no_triples: bool,
    /// Depth of `resolve_deaths` calls in progress: a "when you have space" summon asked for
    /// inside a death batch only joins the queue and is served after the batch.
    pub death_depth: u32,
    /// Depth of Battlecries triggered by another effect (`trigger_battlecry_ext`): a Discover
    /// offered meanwhile is picked at random, in the recruit phase too.
    pub triggered_battlecries: u32,
    /// A safety limit (death loop / effect depth) cut resolution short.
    pub truncated: bool,
    /// The eliminated player a ghost side stands for: it has no player-level effects, but permanent
    /// gains stay on its warband.
    pub ghost_of: [Option<PlayerId>; 2],
    /// The Played event being dispatched is a Magnetization from hand: listeners that also react to
    /// Magnetized skip it (one trigger per action).
    pub magnet_play: bool,
    /// Replaces cards of the Refresh in progress before "after Refresh" effects see the
    /// Tavern (Knockoff Wisdomball, Warband Whistle); taken by the next Refresh.
    pub shop_fixup: Option<fn(&mut Ctx, Side)>,
    /// Put into the Tavern by a Refresh before its default fill (Warband Whistle's copies).
    pub shop_prefill: Option<fn(&mut Ctx, Side)>,
    /// The default fill of this Refresh may use every Tavern slot (Warband Whistle: its
    /// copies come first, then the usual cards while there is room).
    pub shop_fill_cap: Option<usize>,
    /// Cards of this Refresh that extra offers may not displace (Warband Whistle's copies).
    pub shop_priority: Vec<EntityId>,
    /// A gift offer that ignores the game state's gating (Tras'tath's start-of-game list: no
    /// Tavern Tier, trigger-count or turn conditions).
    pub gift_start: bool,
    /// Number of the event being dispatched, and the random pick shared within that event by the
    /// stacks and the cards of one kind of effect (every "after the Tavern is Refreshed, give a
    /// random minion" buff lands on the same minion).
    pub emit_seq: u32,
    next_seq: u32,
    pub event_picks: Vec<(u32, CardId, EntityId)>,
    /// While a hero damage event is dispatched: the card whose effect dealt it (None: a
    /// cost paid with Health, combat damage). Listeners run with `src` set to themselves.
    pub hero_hit_by: Option<CardId>,
    /// The Hero Power offer being made is Power of the Storm's own choice of the turn (any
    /// other offer for its slot replaces the Storm itself).
    pub storm_offer: bool,
    frame_log_start: usize,
    /// Attacks since the last replay frame (combat with logging only).
    pub frame_attacks: Vec<(EntityId, EntityId)>,
    /// Lowest stack address this context's calls may reach, and the last entry points
    /// entered with the number entered (`guard`).
    pub(crate) stack_floor: usize,
    pub(crate) trail: [(u8, u32); crate::guard::TRAIL],
    pub(crate) trail_n: u32,
}

impl<'g> Ctx<'g> {
    pub fn new(
        g: &'g mut Game,
        boards: [Vec<Minion>; 2],
        owner: [Option<PlayerId>; 2],
        combat: bool,
    ) -> Ctx<'g> {
        let frame_log_start = g.log.len();
        let mut ctx = Ctx {
            g,
            boards,
            owner,
            combat,
            cs: [SideState::default(), SideState::default()],
            depth: 0,
            frames: Vec::new(),
            frame_log_start,
            frame_attacks: Vec::new(),
            src: None,
            cast_src: 0,
            cast_card: None,
            cast_from_hand: false,
            cast_hid: 0,
            timewarp_purchase: [false; 2],
            physical_spell_uses: Vec::new(),
            in_cast: false,
            spell_phase: 0,
            play_phase: 0,
            cast_by_you: false,
            cast_on_friendly: false,
            queued: None,
            deliver_hid: None,
            rewind: None,
            echo_magnetize: false,
            magnet_play: false,
            shop_fixup: None,
            shop_prefill: None,
            shop_fill_cap: None,
            shop_priority: Vec::new(),
            first: 0,
            defer_hits: false,
            deferred: Vec::new(),
            keep_venom: false,
            truncated: false,
            cstats: Default::default(),
            no_triples: false,
            death_depth: 0,
            triggered_battlecries: 0,
            ghost_of: [None, None],
            gift_start: false,
            emit_seq: 0,
            next_seq: 0,
            event_picks: Vec::new(),
            hero_hit_by: None,
            storm_offer: false,
            stack_floor: crate::guard::floor(),
            trail: [(0, 0); crate::guard::TRAIL],
            trail_n: 0,
        };
        if combat {
            // A Mishmash watches its Amalgam from the start of the combat. The one in the
            // Tavern stops here and looks afresh next turn: permanent changes made during
            // the combat reach the warband copies directly, with no Mishmash to see them.
            for p in owner.into_iter().flatten() {
                ctx.g.players[p].mishmash_seen = None;
            }
            ctx.mishmash_sync(0);
            ctx.mishmash_sync(1);
        }
        ctx
    }

    /// Record a replay frame (no-op unless logging).
    pub fn frame(&mut self) {
        if !self.g.cfg.record_log {
            return;
        }
        let start = self.frame_log_start.min(self.g.log.len());
        let log = self.g.log[start..].to_vec();
        self.frame_log_start = self.g.log.len();
        let attacks = std::mem::take(&mut self.frame_attacks);
        self.frames.push(CombatFrame {
            log,
            attacks,
            boards: [self.boards[0].clone(), self.boards[1].clone()],
        });
    }

    // ------------------------------------------------------------------ accessors

    pub fn rng(&mut self) -> &mut Rng {
        &mut self.g.rng
    }
    pub fn cfg(&self) -> &RulesConfig {
        &self.g.cfg
    }
    pub fn player(&mut self, side: Side) -> Option<&mut Player> {
        self.owner[side].map(|p| &mut self.g.players[p])
    }
    pub fn player_ref(&self, side: Side) -> Option<&Player> {
        self.owner[side].map(|p| &self.g.players[p])
    }
    /// Log label of a side's owner ("p3", or "ghost"); formats lazily.
    #[inline]
    pub fn who(&self, side: Side) -> Who {
        Who(self.owner[side])
    }
    pub fn other(side: Side) -> Side {
        1 - side
    }
    pub fn tier(&self, side: Side) -> u8 {
        match self.player_ref(side) {
            Some(p) => p.tier,
            None => self.cs[side].tier.max(1),
        }
    }
    pub fn board_full(&self, side: Side) -> bool {
        self.boards[side].len() >= self.g.cfg.board_size
    }
    pub fn locate(&self, eid: EntityId) -> Option<(Side, usize)> {
        if eid == 0 {
            return None;
        }
        for s in 0..2 {
            if let Some(i) = self.boards[s].iter().position(|m| m.eid == eid) {
                return Some((s, i));
            }
        }
        None
    }
    pub fn minion(&self, eid: EntityId) -> Option<&Minion> {
        self.locate(eid).map(|(s, i)| &self.boards[s][i])
    }
    pub fn minion_mut(&mut self, eid: EntityId) -> Option<&mut Minion> {
        self.locate(eid).map(|(s, i)| &mut self.boards[s][i])
    }
    pub fn on_board(&self, eid: EntityId) -> bool {
        self.locate(eid).is_some()
    }
    /// Run `f` with `side`'s arena board back on its player. Game-level code (trinket offers
    /// and their eligibility rules) reads `Player::board`, which is empty while the recruit
    /// context owns the board.
    pub fn board_home<R>(&mut self, side: Side, f: impl FnOnce(&mut Ctx) -> R) -> R {
        let home = match self.owner[side] {
            Some(p) if !self.combat && self.g.players[p].board.is_empty() => Some(p),
            _ => None,
        };
        let Some(p) = home else { return f(self) };
        self.g.players[p].board = std::mem::take(&mut self.boards[side]);
        let r = f(self);
        self.boards[side] = std::mem::take(&mut self.g.players[p].board);
        r
    }

    /// Live friendly minions (eids) on `side`, left to right.
    pub fn friendly(&self, side: Side) -> Vec<EntityId> {
        self.boards[side]
            .iter()
            .filter(|m| m.alive())
            .map(|m| m.eid)
            .collect()
    }
    pub fn friendly_where(&self, side: Side, f: impl Fn(&Minion) -> bool) -> Vec<EntityId> {
        self.boards[side]
            .iter()
            .filter(|m| m.alive() && f(m))
            .map(|m| m.eid)
            .collect()
    }
    pub fn random_friendly(&mut self, side: Side, f: impl Fn(&Minion) -> bool) -> Option<EntityId> {
        let c = self.friendly_where(side, f);
        self.g.rng.pick(&c).copied()
    }
    pub fn random_friendly_other(
        &mut self,
        me: Me,
        f: impl Fn(&Minion) -> bool,
    ) -> Option<EntityId> {
        let c = self.friendly_where(me.side, |m| m.eid != me.eid && f(m));
        self.g.rng.pick(&c).copied()
    }
    pub fn leftmost(&self, side: Side) -> Option<EntityId> {
        self.boards[side].iter().find(|m| m.alive()).map(|m| m.eid)
    }
    pub fn rightmost(&self, side: Side) -> Option<EntityId> {
        self.boards[side]
            .iter()
            .rev()
            .find(|m| m.alive())
            .map(|m| m.eid)
    }
    pub fn neighbors(&self, eid: EntityId) -> Vec<EntityId> {
        let Some((s, i)) = self.locate(eid) else {
            return vec![];
        };
        let b = &self.boards[s];
        let mut v = vec![];
        if i > 0 {
            v.push(b[i - 1].eid);
        }
        if i + 1 < b.len() {
            v.push(b[i + 1].eid);
        }
        v
    }
    /// Most common minion type on the side's board (ties broken randomly; ALL counts for
    /// every type).
    /// "Your most common type": a type counts only if some minion that is not an Amalgam has it
    /// (single or dual type); then every minion having it, Amalgams and multi-type minions
    /// included, counts toward it. Ties are random. `with_hand` also counts hand minions
    /// (trinkets).
    pub fn most_common_race(&mut self, side: Side) -> Option<Races> {
        self.most_common_race_ex(side, false)
    }

    pub fn most_common_race_ex(&mut self, side: Side, with_hand: bool) -> Option<Races> {
        let mut races: Vec<Races> = self.boards[side]
            .iter()
            .filter(|m| m.alive())
            .map(|m| m.races)
            .collect();
        if with_hand {
            races.extend(self.hand_minions(side).into_iter().map(|x| x.2));
        }
        // Only the lobby's types count: a dual-type minion's other type (Ominous Seer is
        // also a Naga) can't be "your most common type" when it isn't in the game.
        let lobby = self.g.lobby_races;
        let mut present = Races::NONE;
        for r in &races {
            if *r != Races::ALL {
                present = Races(present.0 | (r.0 & lobby.0));
            }
        }
        let keyed: Vec<(Races, i64)> = present
            .iter()
            .map(|t| (t, races.iter().filter(|r| r.intersects(t)).count() as i64))
            .collect();
        self.pick_max(keyed)
    }

    // ------------------------------------------------------------------ creation

    pub fn timewarp_suspended(&self, side: Side) -> bool {
        !self.combat
            && (self.timewarp_purchase[side]
                || self.player_ref(side).is_some_and(|p| {
                    p.pending
                        .iter()
                        .any(|q| matches!(q, Pending::Timewarp { .. }))
                }))
    }

    /// New minion instance for `side` with the owner's "this game" bonuses applied.
    pub fn new_minion(&mut self, side: Side, card: CardId) -> Minion {
        let eid = self.g.alloc_eid();
        let mut m = Minion::new(eid, card);
        if let Some(s) = script(card) {
            m.give(s.innate);
            m.bonus_kw = Kw::NONE;
            if let Some(f) = s.on_create {
                f(&mut m);
            }
        }
        if let Some(p) = self.owner[side] {
            apply_bonuses(&self.g.cfg, &self.g.players[p], &mut m);
            crate::content::trinket_on_create(&self.g.players[p], &mut m);
        }
        m
    }

    /// Summon a minion at `pos` (clamped). Returns its eid, or None if the board is full.
    pub fn summon(&mut self, side: Side, pos: usize, mut m: Minion) -> Option<EntityId> {
        self.guard(crate::guard::Site::Summon, u32::from(m.card.0));
        if self.board_full(side) {
            // The minion is dropped: a pool copy it holds goes back (found by the fuzzer:
            // a copy summoned onto a full board leaked its pool copy).
            if !self.combat {
                self.g.pool.return_minion(&m);
            }
            return None;
        }
        let pos = pos.min(self.boards[side].len());
        // Attack order: a minion summoned left of the current attacker (or of where it died) waits
        // for the next round; at or right of it, it attacks in turn -- the attacker's own
        // Deathrattle summons (at its position) attack next.
        if self.combat {
            let la = self.cs[side].last_attacker;
            if la != 0 {
                let alive_at = self.boards[side].iter().position(|x| x.eid == la);
                let waits = match alive_at {
                    Some(i) => pos <= i,
                    None => self.cs[side]
                        .dead
                        .iter()
                        .rev()
                        .find(|d| d.eid == la)
                        .is_some_and(|d| pos < d.pos),
                };
                if waits {
                    m.attacked = true;
                }
            }
        }
        let eid = m.eid;
        let aura = if self.combat && m.is(Races::BEAST) {
            (self.cs[side].beast_atk_aura, self.cs[side].beast_hp_aura)
        } else {
            (0, 0)
        };
        // Entering the board is no stat gain: a Mishmash starts watching here, and an
        // Amalgam is noted with the stats it comes with (also when it was here before).
        let watched = {
            let (_, mishmash, amalgam) = stat_watcher_ids();
            let n = db().normal_of(m.card);
            n == mishmash || n == amalgam
        };
        self.boards[side].insert(pos, m);
        crate::content::check_total_attack(self, side);
        if watched {
            if let Some(Some(seen)) = self.mishmash_seen(side) {
                seen.retain(|x| x.0 != eid);
            }
            self.mishmash_sync(side);
        }
        if aura != (0, 0) {
            self.buff(eid, aura.0, aura.1);
        }
        let name = self.boards[side][pos].name();
        let who = self.who(side);
        self.g.log(|| format!("{who} summons {name} at {pos}"));
        self.emit(Event::Summoned { side, eid });
        if !self.combat {
            self.check_triples(side);
        }
        Some(eid)
    }

    /// Run one complete summon/effect batch before triples may free any board slots.
    /// Nested effects inherit the outer suppression; combat never checks triples.
    fn with_deferred_triples<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        if self.combat {
            return f(self);
        }
        let previous = std::mem::replace(&mut self.no_triples, true);
        let result = f(self);
        self.no_triples = previous;
        if !previous {
            for side in 0..2 {
                self.check_triples(side);
            }
        }
        result
    }

    /// Summon `n` new minions of `card` starting at `pos`, as one batch.
    pub fn summon_card(&mut self, side: Side, pos: usize, card: CardId, n: usize) -> Vec<EntityId> {
        self.with_deferred_triples(|ctx| {
            let mut out = vec![];
            for i in 0..n {
                if ctx.board_full(side) {
                    break;
                }
                let m = ctx.new_minion(side, card);
                if let Some(e) = ctx.summon(side, pos + i, m) {
                    out.push(e);
                }
            }
            out
        })
    }

    /// Minions in the side owner's hand, as (hand id, attack, types), skipping cards already
    /// summoned from hand this combat.
    pub fn hand_minions(&self, side: Side) -> Vec<(EntityId, i32, Races)> {
        let Some(p) = self.player_ref(side) else {
            return vec![];
        };
        // Read/count effects include locked cards. Summoning selectors separately exclude them, as
        // does summon_from_hand.
        p.hand
            .iter()
            .filter(|h| !self.cs[side].from_hand.contains(&h.hid))
            .filter_map(|h| h.item.as_minion().map(|m| (h.hid, m.atk, m.races)))
            .collect()
    }

    /// Highest-Attack hand minion(s) matching `f` (ties: random), up to `n`.
    pub fn highest_attack_in_hand(
        &mut self,
        side: Side,
        n: usize,
        f: impl Fn(Races) -> bool,
    ) -> Vec<EntityId> {
        let mut v: Vec<(usize, EntityId, i32)> = self
            .summonable_hand_minions(side)
            .into_iter()
            .enumerate()
            .filter(|(_, (_, _, r))| f(*r))
            .map(|(i, (h, a, _))| (i, h, a))
            .collect();
        // Highest Attack first; ties in random order.
        self.g.rng.shuffle(&mut v);
        v.sort_by_key(|(_, _, a)| -a);
        v.into_iter().take(n).map(|(_, h, _)| h).collect()
    }

    /// Hand minions eligible to be summoned: locked/held cards still cannot leave hand.
    fn summonable_hand_minions(&self, side: Side) -> Vec<(EntityId, i32, Races)> {
        let Some(p) = self.player_ref(side) else {
            return vec![];
        };
        p.hand
            .iter()
            .filter(|h| h.locked == 0 && !self.cs[side].from_hand.contains(&h.hid))
            .filter_map(|h| h.item.as_minion().map(|m| (h.hid, m.atk, m.races)))
            .collect()
    }

    /// Summon a copy of a hand minion "for this combat only": the card stays in hand and a
    /// combat-only copy (fresh entity) joins the board. Each hand card at most once per combat.
    pub fn summon_from_hand(&mut self, side: Side, hid: EntityId, pos: usize) -> Option<EntityId> {
        if self.cs[side].from_hand.contains(&hid) || self.board_full(side) {
            return None;
        }
        let p = self.owner[side]?;
        let m = self.g.players[p]
            .hand
            .iter()
            .find(|h| h.hid == hid && h.locked == 0)
            .and_then(|h| h.item.as_minion())?
            .clone();
        let mut copy = m;
        copy.eid = self.g.alloc_eid();
        copy.combat_only = true;
        copy.pool_copies = 0;
        copy.attacked = false;
        self.cs[side].from_hand.push(hid);
        self.summon(side, pos, copy)
    }

    /// Position just right of a minion (for "summon a copy of this").
    pub fn pos_right_of(&self, eid: EntityId) -> usize {
        self.locate(eid).map(|(_, i)| i + 1).unwrap_or(usize::MAX)
    }

    // ------------------------------------------------------------------ stats & keywords

    /// Give stats to a minion wherever it is (arena boards, owners' hands, tavern).
    pub fn buff(&mut self, eid: EntityId, atk: i32, hp: i32) {
        let (atk, hp) = self.elemental_extra(eid, atk, hp);
        self.buff_inner(eid, atk, hp, true);
    }

    fn buff_inner(&mut self, eid: EntityId, atk: i32, hp: i32, keep: bool) {
        let (atk, hp) = self.combat_gain(eid, atk, hp);
        let combat = self.combat;
        let record = self.g.cfg.record_log && (atk != 0 || hp != 0);
        let logging = record && !combat;
        let owner = if logging {
            self.owner_label(eid)
        } else {
            String::new()
        };
        let src = self.src;
        if atk > 0 || hp > 0 {
            self.mark_tavern_buff(eid);
        }
        let (got_atk, got_hp);
        if let Some(m) = self.find_any_mut(eid) {
            if script(m.card).is_some_and(|s| s.has_flag(flag::NO_STAT_GAIN)) {
                return;
            }
            let before = (m.atk, m.max_hp);
            m.add_stats(atk, hp);
            // What the minion really got (nothing past the i32 limit): the listeners hear that (a
            // capped Gem still fed Titanic Guardian).
            got_atk = m.atk.saturating_sub(before.0);
            got_hp = m.max_hp.saturating_sub(before.1);
            if record {
                m.record(src, crate::state::ModKind::Add, atk, hp);
            }
            if logging {
                // Recruit-phase stat changes are permanent: log them with their source.
                let from = src
                    .map(|c| format!(" (from {})", db().get(c).name))
                    .unwrap_or_default();
                let line = format!(
                    "    ↳ {owner} {} {:+}/{:+} → {}/{}{from}",
                    m.name(),
                    atk,
                    hp,
                    m.atk,
                    m.hp
                );
                self.g.log(|| line);
            }
        } else {
            return;
        }
        // The kept copy saturates on its own: it may still have room for the gain.
        if combat && keep && (atk > 0 || hp > 0) {
            self.keep_combat_gain(eid, atk.max(0), hp.max(0), Kw::NONE);
        }
        self.stats_changed(eid, got_atk, got_hp);
    }

    /// What follows a change of a minion's stats by `atk`/`hp`: the stat quests, the minions that
    /// watch stat gains, the minion's own `on_stats` and the buddies. Every way of raising stats
    /// ends here: a plain buff, stats set to a higher value and a Magnetization (a buff until next
    /// turn has its own short path).
    pub(crate) fn stats_changed(&mut self, eid: EntityId, atk: i32, hp: i32) {
        if (atk > 0 || hp > 0)
            && let Some((s, _)) = self.locate(eid)
            && self.player_ref(s).is_some_and(|p| !p.quests.is_empty())
        {
            crate::content::on_stats_gained(self, s, eid);
        }
        if let Some((side, _)) = self.locate(eid)
            && self.timewarp_suspended(side)
        {
            // Discard the paused gain while keeping the next active comparison current.
            self.mishmash_sync(side);
            return;
        }
        if atk > 0 || hp > 0 {
            self.stat_watchers(eid);
            self.timewarp_stat_watchers(eid, atk.max(0));
        }
        self.after_stats(eid);
        // Buddies reacting to stat gains (Titanic Guardian, Sinestra).
        crate::content::on_stat_gain(self, eid, atk.max(0), hp.max(0));
        if let Some((s, _)) = self.locate(eid) {
            self.mishmash_sync(s);
        }
    }

    /// Tentacle of C'Thun: after a different friendly minion gains stats, +1/+1 (+2/+2) until
    /// next turn. Its own gains and Mishmash's don't trigger it again.
    fn stat_watchers(&mut self, eid: EntityId) {
        let (tentacle, mishmash, _) = stat_watcher_ids();
        if self.src.is_some_and(|s| {
            let n = db().normal_of(s);
            n == tentacle || n == mishmash
        }) {
            return;
        }
        let Some((s, _)) = self.locate(eid) else {
            return;
        };
        if self.timewarp_suspended(s) {
            return;
        }
        let watchers: Vec<(EntityId, bool)> = self.boards[s]
            .iter()
            .filter(|m| m.eid != eid && m.alive() && db().normal_of(m.card) == tentacle)
            .map(|m| (m.eid, m.golden))
            .collect();
        for (w, golden) in watchers {
            let g = if golden { 2 } else { 1 };
            let prev = self.src.replace(tentacle);
            self.buff_temp(w, g, g);
            self.src = prev;
        }
    }

    /// Mishmash: "Whenever your Amalgam gains stats, this gains them too" (golden: twice). Mishmash
    /// follows the Amalgam's stats themselves, whatever raised them - a buff, stats until next
    /// turn, stats set to a higher value, a Magnetization, a "this game" bonus. It gains the rise
    /// in Attack and in maximum Health for good; a drop changes nothing (a later rise counts from
    /// the lower value).
    ///
    /// The stats each Amalgam had at the last look are kept per side (`mishmash_seen`); this
    /// compares and hands out the difference. It runs after every stat change made through
    /// the context, when a Mishmash or an Amalgam enters the board, and at the start and end
    /// of every recruit context (`bump_player_stat`), which catches a stat written directly.
    /// A Mishmash that just arrived only starts watching, and an Amalgam that just arrived
    /// is only noted: nothing from before counts.
    pub(crate) fn mishmash_sync(&mut self, side: Side) {
        let paused = self.timewarp_suspended(side);
        let (_, mishmash, amalgam) = stat_watcher_ids();
        let any = self.boards[side]
            .iter()
            .any(|m| m.alive() && db().normal_of(m.card) == mishmash);
        if !any {
            if let Some(seen) = self.mishmash_seen(side)
                && seen.is_some()
            {
                *seen = None;
            }
            return;
        }
        let now: Vec<(EntityId, i32, i32)> = self.boards[side]
            .iter()
            .filter(|m| m.alive() && db().normal_of(m.card) == amalgam)
            .map(|m| (m.eid, m.atk, m.max_hp))
            .collect();
        let Some(seen) = self.mishmash_seen(side) else {
            return;
        };
        let Some(old) = seen.replace(now.clone()) else {
            return;
        };
        if paused {
            return;
        }
        let (mut atk, mut hp) = (0i32, 0i32);
        for (eid, a, h) in now {
            if let Some((_, a0, h0)) = old.iter().find(|x| x.0 == eid) {
                atk = atk.saturating_add((a - a0).max(0));
                hp = hp.saturating_add((h - h0).max(0));
            }
        }
        if atk == 0 && hp == 0 {
            return;
        }
        let watchers: Vec<(EntityId, bool)> = self.boards[side]
            .iter()
            .filter(|m| m.alive() && db().normal_of(m.card) == mishmash)
            .map(|m| (m.eid, m.golden))
            .collect();
        for (w, golden) in watchers {
            let g = if golden { 2 } else { 1 };
            let prev = self.src.replace(mishmash);
            self.buff(w, atk.saturating_mul(g), hp.saturating_mul(g));
            self.src = prev;
        }
    }

    /// Where `mishmash_sync` keeps what it saw: the combat's side state, or the player in
    /// the Tavern.
    fn mishmash_seen(&mut self, side: Side) -> Option<&mut Option<crate::state::SeenStats>> {
        if self.combat {
            return Some(&mut self.cs[side].mishmash_seen);
        }
        let p = self.owner[side]?;
        Some(&mut self.g.players[p].mishmash_seen)
    }

    /// Timewarped Whelp Smuggler ("whenever a friendly minion gains Attack, give it +3 (+6)
    /// Health") and Timewarped Astrogill ("while this is in your hand, after a different
    /// friendly Murloc gains stats, gain +3/+2 (+6/+4)").
    fn timewarp_stat_watchers(&mut self, eid: EntityId, atk: i32) {
        static IDS: std::sync::OnceLock<(CardId, CardId)> = std::sync::OnceLock::new();
        let (smuggler, astrogill) =
            *IDS.get_or_init(|| (db().id("BG34_Giant_064"), db().id("BG34_Giant_801")));
        let Some((s, i)) = self.locate(eid) else {
            return;
        };
        if self.timewarp_suspended(s) {
            return;
        }
        let murloc = self.boards[s][i].is(Races::MURLOC);
        if atk > 0 {
            let hp: i32 = self.boards[s]
                .iter()
                .filter(|m| m.alive() && db().normal_of(m.card) == smuggler)
                .map(|m| if m.golden { 6 } else { 3 })
                .sum();
            if hp > 0 {
                let prev = self.src.replace(smuggler);
                self.buff(eid, 0, hp);
                self.src = prev;
            }
        }
        if murloc && let Some(p) = self.owner[s] {
            // Hand cards summoned for this combat are not in the hand any more.
            let away = self.cs[s].from_hand.clone();
            for h in self.g.players[p].hand.iter_mut() {
                if away.contains(&h.hid) {
                    continue;
                }
                if let Item::Minion(m) = &mut h.item
                    && db().normal_of(m.card) == astrogill
                {
                    let g = if m.golden { 2 } else { 1 };
                    m.add_stats(3 * g, 2 * g);
                }
            }
        }
    }

    /// "Your Elementals give an extra +N/+N this game" (Timewarped Swirler): added to every
    /// stat gain whose source card is an Elemental minion, except what a minion gives
    /// itself.
    fn elemental_extra(&self, eid: EntityId, atk: i32, hp: i32) -> (i32, i32) {
        if atk <= 0 && hp <= 0 {
            return (atk, hp);
        }
        let Some(src) = self.src.map(|c| db().get(db().normal_of(c))) else {
            return (atk, hp);
        };
        if src.kind != CardKind::Minion || !src.races.intersects(Races::ELEMENTAL) {
            return (atk, hp);
        }
        let side = self.locate(eid).map_or(0, |x| x.0);
        let Some(st) = self.player_ref(side).map(|p| &p.stats) else {
            return (atk, hp);
        };
        if st.elem_give_atk == 0 && st.elem_give_hp == 0 {
            return (atk, hp);
        }
        if self
            .find_any(eid)
            .is_some_and(|m| db().normal_of(m.card) == src.cid)
        {
            return (atk, hp);
        }
        (
            atk.saturating_add(st.elem_give_atk),
            hp.saturating_add(st.elem_give_hp),
        )
    }

    /// Run the `on_stats` hook of a board minion whose stats changed (Scarlet Survivor).
    fn after_stats(&mut self, eid: EntityId) {
        let Some((side, i)) = self.locate(eid) else {
            return;
        };
        if self.timewarp_suspended(side) {
            return;
        }
        let m = &self.boards[side][i];
        let Some(f) = script(m.card).and_then(|s| s.on_stats) else {
            return;
        };
        let me = Me {
            side,
            eid,
            card: m.card,
            golden: m.golden,
            opt: 0,
            pos: i,
            data: 0,
        };
        f(self, me);
    }

    /// Tarecgosa / Persistent Poet / Timewarped Poet: stats and Bonus Keywords gained in
    /// combat are kept on the warband copy (golden: double stats). Multiple sources take the
    /// largest multiplier.
    fn combat_retention_multiplier(&self, eid: EntityId) -> i32 {
        static IDS: std::sync::OnceLock<(CardId, CardId, CardId)> = std::sync::OnceLock::new();
        let (tarecgosa, poet, tw_poet) = *IDS.get_or_init(|| {
            (
                db().id("BG21_015"),
                db().id("BG29_813"),
                db().id("BG34_Giant_314"),
            )
        });
        let Some((s, i)) = self.locate(eid) else {
            return 0;
        };
        let b = &self.boards[s];
        let g = |m: &Minion| if m.golden { 2 } else { 1 };
        let mut k = 0;
        if db().normal_of(b[i].card) == tarecgosa {
            k = g(&b[i]);
        }
        // Tarecgosa's Blessing (Dark Gift): keeps double the stats.
        if b[i]
            .attached
            .contains(&crate::cardid!("BG36_MidGameEffect_000t50"))
        {
            k = k.max(2);
        }
        if b[i].is(Races::DRAGON) {
            for j in [i.wrapping_sub(1), i + 1] {
                if let Some(n) = b.get(j)
                    && n.alive()
                    && db().normal_of(n.card) == poet
                {
                    k = k.max(g(n));
                }
            }
            // Timewarped Poet: all your Dragons (itself included), wherever they stand.
            for n in b.iter() {
                if n.alive() && db().normal_of(n.card) == tw_poet {
                    k = k.max(g(n));
                }
            }
        }
        k
    }

    /// A combat gain as it happens: "double stats gained in combat" (golden Tarecgosa and Poets,
    /// Tarecgosa's Blessing) doubles the gain on the fighting minion at once, not only on the copy
    /// it keeps (a Blessed Runic Arcanist's two Shiny Rings made 4/5 8/9; Training Certificate on a
    /// golden Tarecgosa tripled it). Losses are unchanged.
    fn combat_gain(&self, eid: EntityId, atk: i32, hp: i32) -> (i32, i32) {
        if !self.combat || (atk <= 0 && hp <= 0) || self.combat_retention_multiplier(eid) < 2 {
            return (atk, hp);
        }
        let double = |x: i32| if x > 0 { x.saturating_mul(2) } else { x };
        (double(atk), double(hp))
    }

    /// The warband copy keeps the combat gain as it was made (`combat_gain` doubled it).
    fn keep_combat_gain(&mut self, eid: EntityId, atk: i32, hp: i32, kw: Kw) {
        if self.combat_retention_multiplier(eid) == 0 {
            return;
        }
        let Some((s, _)) = self.locate(eid) else {
            return;
        };
        let (src, record) = (self.src, self.g.cfg.record_log);
        if let Some(m) = self.warband_minion_mut(s, eid) {
            if atk > 0 || hp > 0 {
                m.add_stats(atk, hp);
                if record {
                    m.record(src, crate::state::ModKind::Add, atk, hp);
                }
            }
            m.give(crate::content::bonus_keywords(kw));
        }
    }

    /// The lasting copy of a minion that is fighting on `side`: on its owner's warband, or
    /// on the ghost's (`last_board` of the dead player: what a ghost gains for good
    /// stays on its warband). None outside combat and for a minion made in this combat.
    fn warband_minion_mut(&mut self, side: Side, eid: EntityId) -> Option<&mut Minion> {
        if !self.combat {
            return None;
        }
        let board = match (self.owner[side], self.ghost_of[side]) {
            (Some(p), _) => &mut self.g.players[p].board,
            (None, Some(g)) => &mut self.g.players[g].last_board,
            (None, None) => return None,
        };
        board.iter_mut().find(|m| m.eid == eid)
    }

    /// Owner label of an entity on a board / in a hand / in a tavern ("p3", "p3 hand"...).
    pub fn owner_label(&self, eid: EntityId) -> String {
        if let Some((s, _)) = self.locate(eid) {
            return self.who(s).to_string();
        }
        for s in 0..2 {
            let Some(p) = self.owner[s] else { continue };
            let pl = &self.g.players[p];
            if pl.hand.iter().any(|h| h.hid == eid) {
                return format!("p{p} hand:");
            }
            if pl.shop.iter().any(|x| x.sid == eid) {
                return format!("p{p} tavern:");
            }
        }
        "?".to_string()
    }

    /// " on <name>" for logs (empty when there is no target).
    pub fn target_name(&self, eid: EntityId) -> String {
        if eid == 0 {
            return String::new();
        }
        if let Some(m) = self.minion(eid) {
            return format!(" on {}", m.name());
        }
        for s in 0..2 {
            let Some(p) = self.owner[s] else { continue };
            let pl = &self.g.players[p];
            if let Some(x) = pl.shop.iter().find(|x| x.sid == eid) {
                return format!(" on {} (tavern)", db().get(x.item.card()).name);
            }
            if let Some(x) = pl.hand.iter().find(|x| x.hid == eid) {
                return format!(" on {} (hand)", db().get(x.item.card()).name);
            }
        }
        String::new()
    }

    /// Permanent buff: in combat also applies to the persistent warband copy.
    pub fn buff_perm(&mut self, side: Side, eid: EntityId, atk: i32, hp: i32) {
        let (atk, hp) = self.elemental_extra(eid, atk, hp);
        // "Can't gain stats": nothing is kept either.
        if self
            .find_any(eid)
            .is_some_and(|m| script(m.card).is_some_and(|s| s.has_flag(flag::NO_STAT_GAIN)))
        {
            return;
        }
        self.buff_inner(eid, atk, hp, false);
        if self.combat {
            let (a, h) = self.combat_gain(eid, atk, hp);
            let (src, record) = (self.src, self.g.cfg.record_log);
            if let Some(m) = self.warband_minion_mut(side, eid) {
                m.add_stats(a, h);
                if record {
                    m.record(src, crate::state::ModKind::Add, a, h);
                }
            }
        }
    }

    /// Buff until the start of the owner's next turn.
    /// A recruit-phase buff on a Tavern minion: its owner "buffed the Tavern" this game.
    fn mark_tavern_buff(&mut self, eid: EntityId) -> bool {
        if self.combat {
            return false;
        }
        for p in self.owner.into_iter().flatten() {
            if self.g.players[p].shop.iter().any(|s| s.sid == eid) {
                self.g.players[p].stats.tavern_buffed = true;
                return true;
            }
        }
        false
    }

    pub fn buff_temp(&mut self, eid: EntityId, atk: i32, hp: i32) {
        let (atk, hp) = self.elemental_extra(eid, atk, hp);
        let (atk, hp) = self.combat_gain(eid, atk, hp);
        let in_tavern = if atk > 0 || hp > 0 {
            self.mark_tavern_buff(eid)
        } else {
            !self.combat
                && self
                    .owner
                    .into_iter()
                    .flatten()
                    .any(|p| self.g.players[p].shop.iter().any(|s| s.sid == eid))
        };
        if let Some(m) = self.find_any_mut(eid) {
            // "Can't gain stats" (Fishbait) covers temporary stats too.
            if script(m.card).is_some_and(|s| s.has_flag(flag::NO_STAT_GAIN)) {
                return;
            }
            m.add_temp_stats(atk, hp, in_tavern);
        }
        if self.combat {
            self.keep_combat_gain(eid, atk.max(0), hp.max(0), Kw::NONE);
        }
        // A temporary gain is a stat gain too (Find the Murder Weapon, Pressure the
        // Authorities, Tentacle of C'Thun; Mishmash keeps what its Amalgam gained).
        if (atk > 0 || hp > 0)
            && let Some((s, _)) = self.locate(eid)
            && self.player_ref(s).is_some_and(|p| !p.quests.is_empty())
        {
            crate::content::on_stats_gained(self, s, eid);
        }
        if let Some((side, _)) = self.locate(eid)
            && self.timewarp_suspended(side)
        {
            // Discard the paused gain while keeping the next active comparison current.
            self.mishmash_sync(side);
            return;
        }
        if atk > 0 || hp > 0 {
            // A gain until next turn is a gain: the same watchers as `stats_changed`, in its order
            // - Tentacle, Whelp Smuggler, the minion's own "once this reaches..." hook, Titanic
            // Guardian and Sinestra. Makeshift Master's Deity is left to permanent gains.
            self.stat_watchers(eid);
            self.timewarp_stat_watchers(eid, atk.max(0));
            self.after_stats(eid);
            crate::content::on_temp_stat_gain(self, eid, atk.max(0), hp.max(0));
        }
        if let Some((s, _)) = self.locate(eid) {
            self.mishmash_sync(s);
        }
    }

    /// "Set its stats to X/Y" on a minion in play. What the set adds to its Attack and to its
    /// maximum Health counts as stats gained (Tyrael's 50/50 on the Amalgam makes Mishmash grow by
    /// the difference).
    pub fn set_stats(&mut self, eid: EntityId, atk: i32, hp: i32) {
        let (record, src) = (self.g.cfg.record_log, self.src);
        // In the Tavern the Tavern's "this game" buffs stay on top of a "set to": the client
        // recomputes them after it (Perfect Vision with +9/+5 on a Tavern minion with +5/+4: set
        // 29/25, shown 34/29).
        let (ta, th) = self.tavern_aura_of(eid);
        let Some(m) = self.find_any_mut(eid) else {
            return;
        };
        if script(m.card).is_some_and(|s| s.has_flag(flag::NO_STAT_GAIN)) {
            return;
        }
        let atk = atk.saturating_add(m.pbonus_atk).saturating_add(ta);
        let hp = hp.saturating_add(m.pbonus_hp).saturating_add(th);
        let (da, dh) = (
            atk.saturating_sub(m.atk).max(0),
            hp.saturating_sub(m.max_hp).max(0),
        );
        m.set_stats(atk, hp);
        if record {
            m.record(src, crate::state::ModKind::Set, atk, hp);
        }
        if da > 0 || dh > 0 {
            self.stats_changed(eid, da, dh);
        } else if let Some((s, _)) = self.locate(eid) {
            self.mishmash_sync(s);
        }
    }

    pub fn give_kw(&mut self, eid: EntityId, k: Kw) {
        if let Some(m) = self.find_any_mut(eid) {
            m.give(k);
        }
        if self.combat {
            self.keep_combat_gain(eid, 0, 0, k);
        }
    }

    pub fn give_kw_perm(&mut self, side: Side, eid: EntityId, k: Kw) {
        self.give_kw(eid, k);
        if self.combat
            && let Some(g) = self.ghost_of[side]
            && let Some(m) = self.g.players[g]
                .last_board
                .iter_mut()
                .find(|m| m.eid == eid)
        {
            m.give(k);
        }
        if self.combat
            && let Some(p) = self.owner[side]
            && let Some(m) = self.g.players[p].board.iter_mut().find(|m| m.eid == eid)
        {
            m.give(k);
        }
    }

    /// Give keywords until the start of the owner's next turn. Only the ones the minion
    /// does not have yet are temporary; what it has for good stays for good, and what it
    /// had until next turn still ends then.
    pub fn give_kw_temp(&mut self, eid: EntityId, k: Kw) {
        if let Some(m) = self.find_any_mut(eid) {
            let temp = Kw(m.temp_kw.0 | (k.0 & !m.kw.0));
            let bonus = m.bonus_kw;
            m.give(k);
            m.bonus_kw = bonus;
            m.temp_kw = temp;
        }
    }

    /// Find a minion instance in arena boards, owner hands or the recruit tavern.
    /// A minion anywhere the side owners can see: board, hand or tavern.
    pub fn find_any(&self, eid: EntityId) -> Option<&Minion> {
        if eid == 0 {
            return None;
        }
        if let Some(m) = self.minion(eid) {
            return Some(m);
        }
        for s in 0..2 {
            let Some(p) = self.owner[s] else { continue };
            let pl = &self.g.players[p];
            if let Some(m) = pl
                .hand
                .iter()
                .find(|h| h.hid == eid)
                .and_then(|h| h.item.as_minion())
            {
                return Some(m);
            }
            if !self.combat
                && let Some(m) = pl
                    .shop
                    .iter()
                    .find(|x| x.sid == eid)
                    .and_then(|x| x.item.as_minion())
            {
                return Some(m);
            }
        }
        None
    }

    pub fn find_any_mut(&mut self, eid: EntityId) -> Option<&mut Minion> {
        if eid == 0 {
            return None;
        }
        if let Some((s, i)) = self.locate(eid) {
            return Some(&mut self.boards[s][i]);
        }
        // Locate first, then borrow mutably once (keeps the borrow checker happy).
        let mut found: Option<(PlayerId, bool, usize)> = None;
        for s in 0..2 {
            let Some(p) = self.owner[s] else { continue };
            let pl = &self.g.players[p];
            if let Some(i) = pl.hand.iter().position(|h| h.hid == eid) {
                found = Some((p, true, i));
                break;
            }
            if !self.combat
                && let Some(i) = pl.shop.iter().position(|x| x.sid == eid)
            {
                found = Some((p, false, i));
                break;
            }
        }
        let (p, in_hand, i) = found?;
        let pl = &mut self.g.players[p];
        if in_hand {
            pl.hand[i].item.as_minion_mut()
        } else {
            pl.shop[i].item.as_minion_mut()
        }
    }

    /// Change a player-level "this game, wherever they are" counter and re-sync every
    /// instance it shows on (arena board, persistent board in combat, hand, and the minions
    /// in the player's Tavern, which carry these bonuses from the moment they are offered).
    pub fn bump_player_stat(&mut self, side: Side, f: impl FnOnce(&mut crate::state::PlayerStats)) {
        let Some(p) = self.owner[side] else { return };
        // Makeshift Master's marked minions, as they are before the re-sync.
        let marked = crate::content::makeshift_marked(self, side);
        let combat = self.combat;
        let grow = self.g.cfg.volumizer_attached_grow;
        let Player {
            stats,
            trinkets,
            board,
            hand,
            shop,
            health,
            ..
        } = &mut self.g.players[p];
        f(stats);
        for m in self.boards[side].iter_mut() {
            apply_bonuses_to(grow, stats, trinkets, *health, false, m);
        }
        if combat {
            for m in board.iter_mut() {
                apply_bonuses_to(grow, stats, trinkets, *health, false, m);
            }
        }
        for h in hand.iter_mut() {
            if let Item::Minion(m) = &mut h.item {
                apply_bonuses_to(grow, stats, trinkets, *health, false, m);
            }
        }
        // A minion that was in the Tavern before the counter changed (one offered later is
        // made with the bonus): what reads Tavern stats - consuming, Arcane Absorption -
        // sees the same numbers the player does. This also takes the own-minion auras off a
        // minion that came into the Tavern carrying them (`with_recruit` runs it after every
        // action).
        for slot in shop.iter_mut() {
            if let Item::Minion(m) = &mut slot.item {
                apply_bonuses_to(grow, stats, trinkets, *health, true, m);
            }
        }
        // A "this game" bonus that raised the Amalgam's stats is a rise like any other; so is
        // one that raised a minion Makeshift Master marked.
        if !marked.is_empty() {
            crate::content::makeshift_resynced(self, side, &marked);
        }
        self.mishmash_sync(side);
        crate::content::check_total_attack(self, side);
    }

    // ------------------------------------------------------------------ damage & death

    /// Deal damage from `source` (minion eid or 0). Returns damage actually dealt.
    pub fn damage(&mut self, source: EntityId, target: EntityId, amount: i32) -> i32 {
        if amount <= 0 {
            return 0;
        }
        self.guard(crate::guard::Site::Damage, u32::MAX);
        let (src_venom, src_poison) = match self.minion(source) {
            Some(m) => (m.has(Kw::VENOMOUS), m.has(Kw::POISONOUS)),
            None => (false, false),
        };
        let Some((side, i)) = self.locate(target) else {
            return 0;
        };
        let m = &mut self.boards[side][i];
        if !m.alive() || m.has(Kw::IMMUNE) {
            return 0;
        }
        if m.has(Kw::DIVINE_SHIELD) {
            m.shield_hits = m.shield_hits.saturating_sub(1);
            if m.shield_hits == 0 {
                m.remove(Kw::DIVINE_SHIELD);
                self.emit(Event::ShieldLost { side, eid: target });
            }
            return 0;
        }
        m.hp = m.hp.saturating_sub(amount);
        if m.hp <= 0 || src_poison || src_venom {
            m.killed_by = source;
        }
        if src_poison || src_venom {
            m.destroyed = true;
            if self.combat {
                let c = &mut self.cstats[1 - side].venom_kills;
                *c = c.saturating_add(1);
            }
        }
        if src_venom
            && !self.keep_venom
            && let Some(s) = self.minion_mut(source)
        {
            s.remove(Kw::VENOMOUS);
        }
        self.emit(Event::Damaged {
            side,
            eid: target,
            amount,
            source,
        });
        amount
    }

    pub fn destroy(&mut self, eid: EntityId) {
        if let Some(m) = self.minion_mut(eid) {
            m.destroyed = true;
        }
    }

    /// Deathrattle trigger count. Titus Rivendare says "trigger an extra time" (golden: two
    /// extra), so copies add up. Brann / Drakkari say "trigger twice" and take the maximum.
    pub fn deathrattle_mult(&self, side: Side) -> u32 {
        let mut n = 1;
        let mut multiplier = 1;
        for m in &self.boards[side] {
            if script(m.card).is_some_and(|s| s.has_flag(flag::EXTRA_DEATHRATTLE)) {
                if script(m.card).is_some_and(|s| s.has_flag(flag::EXTRA_BATTLECRY)) {
                    multiplier = multiplier.max(if m.golden { 3 } else { 2 });
                } else {
                    n += if m.golden { 2 } else { 1 };
                }
            }
        }
        n += multiplier - 1;
        // Turbulent Tombs: your Deathrattles trigger an extra time.
        if self
            .player_ref(side)
            .is_some_and(|p| p.has_reward_id(crate::cardid!("BG27_Reward_803")))
        {
            n += 1;
        }
        n
    }

    /// Rally trigger count (Rallying Cry: an extra time).
    pub fn rally_mult(&self, side: Side) -> u32 {
        1 + u32::from(
            self.player_ref(side)
                .is_some_and(|p| p.has_reward_id(crate::cardid!("BG33_Reward_021"))),
        )
    }

    pub fn battlecry_mult(&self, side: Side) -> u32 {
        let mut n = 1;
        for m in &self.boards[side] {
            if script(m.card).is_some_and(|s| s.has_flag(flag::EXTRA_BATTLECRY)) {
                n = n.max(if m.golden { 3 } else { 2 });
            }
        }
        if let Some(p) = self.player_ref(side) {
            // Gilnean War Horn: an extra time.
            if p.has_reward_id(crate::cardid!("BG27_Reward_802")) {
                n += 1;
            }
            // Big Brann Play: an extra time this turn, per copy cast.
            if !p.turn_data.is_empty() {
                let brann = crate::cardid!("BGS_Treasures_030");
                n += p
                    .turn_data
                    .iter()
                    .find(|x| x.0 == brann)
                    .map_or(0, |x| x.1.max(0) as u32);
            }
            // Exquisite Conch: your first Battlecry each turn triggers 2 extra times.
            if p.stats.battlecries_turn == 0 && p.has_reward_id(crate::cardid!("BG24_Reward_123")) {
                n += 2;
            }
        }
        n
    }

    pub fn end_of_turn_mult(&self, side: Side) -> u32 {
        let mut n = 1;
        for m in &self.boards[side] {
            if script(m.card).is_some_and(|s| s.has_flag(flag::EXTRA_END_OF_TURN)) {
                n = n.max(if m.golden { 3 } else { 2 });
            }
        }
        if let Some(p) = self.player_ref(side) {
            n += p
                .trinkets
                .iter()
                .filter(|t| t.base() == crate::cardid!("BG32_MagicItem_367"))
                .count() as u32;
            n += u32::from(p.has_reward_id(crate::cardid!("BG24_Reward_130")));
        }
        // Lucifron: an extra time (golden: 2 extra); copies add up.
        let lucifron = crate::cardid!("TB_BaconShop_HERO_11_Buddy");
        for m in &self.boards[side] {
            if m.is_card_id(lucifron) {
                n += if m.golden { 2 } else { 1 };
            }
        }
        n
    }

    /// Run all deathrattles of a (dead or "triggered") minion.
    pub fn run_deathrattles(&mut self, side: Side, m: &Minion, pos: usize) {
        self.guard(crate::guard::Site::Deathrattles, u32::from(m.card.0));
        self.with_deferred_triples(|ctx| ctx.run_deathrattles_inner(side, m, pos));
    }

    fn run_deathrattles_inner(&mut self, side: Side, m: &Minion, pos: usize) {
        let cards = || {
            std::iter::once(m.card)
                .chain(m.attached.iter().copied())
                .chain(m.gained_dr.iter().copied())
        };
        if !cards().any(|c| script(c).is_some_and(|s| s.deathrattle.is_some())) {
            return;
        }
        let mult = self.deathrattle_mult(side);
        // Deathly Phylactery: your first Deathrattle each combat triggers an extra time -- one
        // Deathrattle effect, not every Deathrattle of the first minion.
        let phylactery = crate::cardid!("BG30_MagicItem_700");
        let mut first = 0;
        if self.combat
            && self
                .player_ref(side)
                .is_some_and(|p| p.has_trinket_id(phylactery))
            && self.combat_data(side, phylactery) == 0
        {
            self.add_combat_data(side, phylactery, 1);
            first = self.player_ref(side).map_or(0, |p| {
                p.trinkets.iter().filter(|t| t.base() == phylactery).count() as u32
            });
        }
        for card in cards() {
            let Some(s) = script(card) else { continue };
            let Some(dr) = s.deathrattle else { continue };
            let golden = if card == m.card {
                m.golden
            } else {
                db().get(card).is_golden()
            };
            for _ in 0..mult + std::mem::take(&mut first) {
                let me = Me {
                    side,
                    eid: m.eid,
                    card,
                    golden,
                    opt: 0,
                    pos,
                    data: m.killed_by as i32,
                };
                let prev_src = self.src.replace(me.card);
                dr(self, me);
                self.src = prev_src;
                self.bump_player_stat(side, |st| st.deathrattles_triggered += 1);
                self.emit(Event::DeathrattleTriggered { side, eid: m.eid });
            }
        }
    }

    /// Remove dead minions, trigger death events, deathrattles, reborn and deity; repeat until the
    /// boards are stable. Order inside one batch of deaths: the deaths are announced, then
    /// Deathrattles, then Reborn, then Avenge. "When you have space" summons asked for meanwhile
    /// (an Avenge that summons, a Deity) wait in the queue until the outermost resolution has
    /// settled (after Deathrattles, then Reborn).
    pub fn resolve_deaths(&mut self) {
        // In the Tavern no triple frees a slot between the Deathrattles and the Reborn of a batch:
        // the whole resolution, nested effects and waiting summons included, is one batch. Calls
        // with nothing dead (most actions) skip the extra triple scan.
        if !self.combat && self.boards.iter().flatten().any(|m| !m.alive()) {
            self.with_deferred_triples(|ctx| ctx.resolve_deaths_inner());
        } else {
            self.resolve_deaths_inner();
        }
    }

    fn resolve_deaths_inner(&mut self) {
        self.guard(crate::guard::Site::Deaths, self.death_depth);
        let mut settled = false;
        self.death_depth += 1;
        for _ in 0..64 {
            let mut dead: Vec<(Side, usize, Minion)> = vec![];
            let radio = crate::cardid!("BG34_Giant_330");
            let needs_dead_copy = self.boards.iter().flatten().any(|m| {
                std::iter::once(m.card)
                    .chain(m.attached.iter().copied())
                    .chain(m.gained_dr.iter().copied())
                    .any(|card| db().normal_of(card) == radio)
            });
            // The first attacker's side resolves first.
            for s in [self.first, 1 - self.first] {
                let mut i = 0;
                while i < self.boards[s].len() {
                    if !self.boards[s][i].alive() {
                        let m = self.boards[s].remove(i);
                        dead.push((s, i, m));
                    } else {
                        i += 1;
                    }
                }
            }
            if dead.is_empty() {
                settled = true;
                break;
            }
            if needs_dead_copy {
                for (side, _, m) in &dead {
                    self.cs[*side].dead_copies.insert(m.eid, m.clone());
                }
            }
            // Avenge counts on the minions there at the deaths: ids up to the newest one now
            // (`alloc_eid` hands out `next_eid` itself). Bodies this batch makes, by its death
            // events, Deathrattles or Reborn, did not see them.
            let born = self.g.next_eid.saturating_add(1);
            for (s, pos, m) in &dead {
                let (s, pos) = (*s, *pos);
                self.cs[s].deaths += 1;
                let name = m.name();
                let who = self.who(s);
                self.g.log(|| format!("{who}'s {name} dies"));
                if m.races.intersects(Races::ABERRATION) {
                    self.cs[s].aberration_deaths += 1;
                }
                self.cs[s].dead.push(DeadRecord {
                    eid: m.eid,
                    card: m.card,
                    atk: m.atk,
                    max_hp: m.max_hp,
                    peak_atk: m.peak_atk,
                    peak_hp: m.peak_hp,
                    gem_atk: m.gem_atk,
                    gem_hp: m.gem_hp,
                    counter: m.counter,
                    had_deathrattle: crate::script::minion_has(m, crate::script::Hook::Deathrattle)
                        || !m.gained_dr.is_empty(),
                    deathrattle_cards: std::iter::once(m.card)
                        .chain(m.attached.iter().copied())
                        .chain(m.gained_dr.iter().copied())
                        .filter(|&card| script(card).is_some_and(|s| s.deathrattle.is_some()))
                        .collect(),
                    races: m.races,
                    pos,
                });
                if m.is_card_id(crate::cardid!("BG25_008")) {
                    self.bump_player_stat(s, |st| st.eternal_knight_deaths += 1);
                }
                self.emit(Event::Died {
                    side: s,
                    eid: m.eid,
                    card: m.card,
                    races: m.races,
                });
                if m.killed_by != 0 {
                    // `Killed.side` is the killer's side: a minion killed by a friendly
                    // effect (Boom-in-a-Box...) was killed by its own side. The killer may
                    // have died in the same exchange, so the death records are checked too.
                    let k = m.killed_by;
                    let killer_side = self
                        .locate(k)
                        .map(|(ks, _)| ks)
                        .or_else(|| dead.iter().find(|d| d.2.eid == k).map(|d| d.0))
                        .or_else(|| (0..2).find(|&ks| self.cs[ks].dead.iter().any(|d| d.eid == k)))
                        .unwrap_or(1 - s);
                    // "Kills an enemy minion" effects: a minion killed by its own side
                    // (Boom-in-a-Box...) is not a kill (card texts).
                    if killer_side != s {
                        let killed = Event::Killed {
                            side: killer_side,
                            killer: k,
                            victim: m.eid,
                        };
                        self.emit(killed.clone());
                        // A killer that died in the same exchange is off the board and hears no
                        // events: its own "when this kills" still counts.
                        if self.locate(k).is_none() {
                            let card = dead
                                .iter()
                                .find(|d| d.2.eid == k)
                                .map(|d| d.2.card)
                                .or_else(|| self.dead_record(killer_side, k).map(|d| d.card));
                            if let Some(card) = card
                                && let Some(sc) = script(card)
                                && sc.listens & crate::event::ev::KILLED != 0
                                && let Some(f) = sc.on_event
                            {
                                let me = Me {
                                    side: killer_side,
                                    eid: k,
                                    card,
                                    golden: db().get(card).is_golden(),
                                    opt: 0,
                                    pos: 0,
                                    data: 0,
                                };
                                let prev = self.src.replace(card);
                                f(self, me, &killed);
                                self.src = prev;
                            }
                        }
                    }
                }
            }
            // Outside combat a dead minion leaves the game: return its pool copies.
            if !self.combat {
                for (_, _, m) in &dead {
                    self.g.pool.return_minion(m);
                }
            }
            // Deathrattles resolve before Reborn (client tip GLUE_TIP_BACON_21). Each summons where its
            // minion died: `pos` counts the survivors left of it, shifted by what earlier (lefter)
            // deaths already summoned.
            let mut off = [0usize; 2];
            let mut made = Vec::with_capacity(dead.len());
            for (s, pos, m) in &dead {
                let p = pos + off[*s];
                let before = self.boards[*s].len();
                if !(m.combat_only && !self.combat) {
                    self.run_deathrattles(*s, m, p);
                }
                let own = self.boards[*s].len().saturating_sub(before);
                off[*s] += own;
                made.push(own);
            }
            // Reborn goes right of the Deathrattle summons at its place: its own and those of the
            // minions that died beside it with no survivor between them (Handless Forsaken's
            // Helping Hand stood left of the Reborn Mummifier that had died left of it).
            let at: Vec<usize> = dead
                .iter()
                .map(|(s, pos, _)| {
                    let summoned: usize = dead
                        .iter()
                        .zip(&made)
                        .filter(|((t, q, _), _)| t == s && q <= pos)
                        .map(|(_, n)| n)
                        .sum();
                    pos + summoned
                })
                .collect();
            let mut back = [0usize; 2];
            for ((s, _, m), p) in dead.iter().zip(at) {
                if m.has(Kw::REBORN) {
                    let before = self.boards[*s].len();
                    self.reborn(*s, p + back[*s], m);
                    back[*s] += self.boards[*s].len().saturating_sub(before);
                }
            }
            // Reborn precedes Avenge. New Deathrattle/Reborn bodies do not count the
            // deaths that created them; the existing birth cutoff still applies.
            for (s, _, _) in &dead {
                self.avenge_tick(*s, born);
            }
            if self.combat {
                for s in [self.first, 1 - self.first] {
                    self.check_deity(s);
                }
            }
        }
        if !settled {
            self.mark_truncated();
        }
        self.death_depth -= 1;
        if self.death_depth == 0 {
            self.flush_waiting();
        }
    }

    fn reborn(&mut self, side: Side, pos: usize, dead: &Minion) {
        // Persisting Horror / Ultra-Capacitor (Battlecruiser): full stats and enchantments.
        let persisting = dead.has_attached("BG36_MidGameEffect_000t12")
            || dead.has_attached("BG31_HERO_801ptj")
            || dead.has_attached("BG31_HERO_801ptj2");
        // Putricide's Creation is its two cards: the fresh copy has them too (a bare Creation
        // card would be a 1/1 with no text).
        let mut m = match crate::content::fresh_copy(self, side, dead) {
            Some(m) => m,
            None => self.new_minion(side, dead.card),
        };
        if persisting {
            // "Perfect Reborn" (Persisting Horror): full stats, keywords and attachments.
            m.set_stats(dead.atk, dead.max_hp);
            m.kw = dead.kw;
            m.attached = dead.attached.clone();
            m.attachment_data = dead.attachment_data.clone();
            m.gift_data = dead.gift_data;
            m.races = dead.races;
            m.bonus_kw = dead.bonus_kw;
            m.temp_atk = dead.temp_atk;
            m.temp_hp = dead.temp_hp;
            m.tavern_temp_atk = dead.tavern_temp_atk;
            m.tavern_temp_hp = dead.tavern_temp_hp;
            m.temp_kw = dead.temp_kw;
            m.gained_dr = dead.gained_dr.clone();
            m.shield_max = dead.shield_max;
        } else {
            // A normal Reborn is a fresh card with 1 Health: no enchantments, Magnetic parts or
            // Dark Gifts. (A Creation's two cards are the card itself.) A bonus it has "wherever it
            // is" is an aura and applies on top of that Health: a Reborn Eternal Knight has 1 + its
            // Health bonus.
            m.hp = 1i32.saturating_add(m.pbonus_hp.max(0)).min(m.max_hp);
        }
        if m.has(Kw::DIVINE_SHIELD) {
            m.shield_hits = m.shield_max;
        }
        m.remove(Kw::REBORN);
        m.combat_only = dead.combat_only;
        let is_deity = self.cs[side].deity_eid == dead.eid;
        if is_deity {
            // Summoned listeners must already see the replacement body.
            self.cs[side].deity_eid = m.eid;
        }
        if let Some(e) = self.summon(side, pos, m) {
            self.emit(Event::Reborn { side, eid: e });
        } else if is_deity {
            self.cs[side].deity_eid = 0;
        }
    }

    /// Damage events held back during a combat exchange.
    pub fn take_deferred(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.deferred)
    }

    /// Dispatch the held-back exchange events in order (the buffer is reused).
    pub fn flush_deferred(&mut self) {
        let mut evs = std::mem::take(&mut self.deferred);
        for e in evs.drain(..) {
            self.emit(e);
        }
        // Keep the buffer's capacity unless a nested exchange left its own behind.
        if self.deferred.capacity() < evs.capacity() {
            self.deferred = evs;
        }
    }

    /// Per-combat progress of a player-level card (`SideState::card_data`).
    pub fn combat_data(&self, side: Side, card: CardId) -> i32 {
        self.cs[side]
            .card_data
            .iter()
            .find(|x| x.0 == card)
            .map_or(0, |x| x.1)
    }

    pub fn add_combat_data(&mut self, side: Side, card: CardId, d: i32) -> i32 {
        let v = &mut self.cs[side].card_data;
        match v.iter_mut().find(|x| x.0 == card) {
            Some(x) => {
                x.1 += d;
                x.1
            }
            None => {
                v.push((card, d));
                d
            }
        }
    }

    /// One friendly death counted for Avenge on `side`: its trinkets, and its living minions
    /// that were on the board when the minion died (entity id below `born`).
    fn avenge_tick(&mut self, side: Side, born: EntityId) {
        // Trinket Avenge (combat only; counts reset each combat like minions').
        if self.combat
            && let Some(p) = self.player_ref(side)
        {
            let cards: Vec<CardId> = p
                .trinkets
                .iter()
                .copied()
                .filter(|c| script(*c).is_some_and(|s| s.avenge.is_some()))
                .collect();
            for card in cards {
                let (n, _, fx) = script(card).unwrap().avenge.unwrap();
                if self.add_combat_data(side, card, 1) >= n as i32 {
                    self.add_combat_data(side, card, -(n as i32));
                    let me = Me {
                        side,
                        eid: 0,
                        card,
                        golden: false,
                        opt: 0,
                        pos: 0,
                        data: 0,
                    };
                    let prev_src = self.src.replace(me.card);
                    fx(self, me);
                    self.src = prev_src;
                }
            }
        }
        // Avenge is a combat keyword: deaths in the Tavern don't count.
        if !self.combat {
            return;
        }
        let ids: Vec<(EntityId, CardId)> = self.boards[side]
            .iter()
            .filter(|m| m.alive() && m.eid < born)
            .flat_map(|m| {
                std::iter::once(m.card)
                    .chain(m.attached.iter().copied())
                    .filter(|c| script(*c).is_some_and(|s| s.avenge.is_some()))
                    .map(move |c| (m.eid, c))
            })
            .collect();
        for (eid, card) in ids {
            let s = script(card).unwrap();
            let (tn, tg, fx) = s.avenge.unwrap();
            let Some(m) = self.minion_mut(eid) else {
                continue;
            };
            let golden = m.golden;
            let threshold = if golden { tg } else { tn };
            m.avenge += 1;
            if m.avenge >= threshold {
                m.avenge = 0;
                let me = Me {
                    side,
                    eid,
                    card,
                    golden,
                    opt: 0,
                    pos: 0,
                    data: 0,
                };
                let prev_src = self.src.replace(me.card);
                fx(self, me);
                self.src = prev_src;
            }
        }
    }

    fn check_deity(&mut self, side: Side) {
        let thr = self.g.cfg.deity_threshold;
        if self.cs[side].deity_awakened || self.cs[side].aberration_deaths < thr {
            return;
        }
        let Some(d) = self.player_ref(side).and_then(|p| p.deity.clone()) else {
            return;
        };
        self.cs[side].deity_awakened = true;
        self.cstats[side].deity_wakes += 1;
        let card = if d.golden {
            db().golden_of(d.card)
        } else {
            d.card
        };
        let mut m = self.new_minion(side, card);
        m.set_stats(d.atk, d.hp);
        if d.reborn {
            m.give(Kw::REBORN);
        }
        let pos = self.boards[side].len();
        if self.board_full(side) {
            self.cs[side].waiting.push(SpaceReq::Deity(m));
            return;
        }
        if let Some(e) = self.summon(side, pos, m) {
            self.cs[side].deity_eid = e;
            let who = self.who(side);
            self.g.log(|| format!("{who}'s deity awakens"));
            let me = Me {
                side,
                eid: e,
                card,
                golden: d.golden,
                opt: 0,
                pos,
                data: 0,
            };
            crate::content::deity_awakened(self, me);
        }
    }

    /// A recruit-phase attack (e.g. a Beast attacking Fishbait in the Tavern): the tavern minion is
    /// placed on the empty enemy side of the recruit arena, the attack resolves like a combat
    /// attack (Rally and "whenever attacks" fire), deaths resolve, and any survivor goes back to
    /// the tavern. Venomous is not consumed by these attacks.
    pub fn tavern_attack(&mut self, side: Side, attacker: EntityId, sid: EntityId) {
        let Some(p) = self.owner[side] else { return };
        let Some(i) = self.g.players[p].shop.iter().position(|s| s.sid == sid) else {
            return;
        };
        let Item::Minion(target) = self.g.players[p].shop.remove(i).item else {
            return;
        };
        let venom = self.minion(attacker).is_some_and(|m| m.has(Kw::VENOMOUS));
        let who = self.who(side);
        let (a, d) = (
            self.minion(attacker).map(|m| m.name()).unwrap_or("?"),
            target.name(),
        );
        self.g
            .log(|| format!("{who}'s {a} attacks {d} in the Tavern"));
        let other = 1 - side;
        self.boards[other].push(target);
        crate::combat::do_attack(self, side, attacker, sid);
        self.resolve_deaths();
        if venom && let Some(m) = self.minion_mut(attacker) {
            m.give(Kw::VENOMOUS);
        }
        // Survivors return to the tavern.
        for m in std::mem::take(&mut self.boards[other]) {
            let sid = m.eid;
            let shop = &mut self.g.players[p].shop;
            let at = i.min(shop.len());
            shop.insert(at, crate::state::ShopSlot::new(sid, Item::Minion(m)));
        }
    }

    /// Trigger a minion's Rally without it attacking (target = defender if any, else 0).
    pub fn trigger_rally(&mut self, side: Side, eid: EntityId, target: EntityId) {
        // Rallies that need a real attack can't be triggered by effects (hsdata tag
        // BACON_RALLY_CANT_BE_ARTIFICIALLY_TRIGGERED).
        if self
            .minion(eid)
            .is_some_and(|m| crate::script::rally_untriggerable(m.card))
        {
            return;
        }
        // Triggered without an attack in combat: a random enemy minion is the target.
        let target = if target == 0 && self.combat {
            let foes: Vec<EntityId> = self.boards[1 - side]
                .iter()
                .filter(|m| m.alive())
                .map(|m| m.eid)
                .collect();
            self.g.rng.pick(&foes).copied().unwrap_or(0)
        } else {
            target
        };
        crate::combat::run_rally(self, side, eid, target);
        self.resolve_deaths();
    }

    /// Trigger a minion's Battlecry from another effect: a targeted Battlecry takes a random legal
    /// target (none: it does nothing); a Choose One card has no Battlecry to trigger. Its Discovers
    /// are picked at random (`offer_discover`).
    pub fn trigger_battlecry_ext(&mut self, side: Side, eid: EntityId) {
        let Some(card) = self.minion(eid).map(|m| m.card) else {
            return;
        };
        self.guard(crate::guard::Site::TriggeredBattlecry, u32::from(card.0));
        // Attached Battlecries only when the card has no script.
        let target = match script(card) {
            None => 0,
            Some(s) if s.choose_one => return,
            Some(s) if s.battlecry_target == TargetSpec::None => 0,
            Some(s) => {
                let opts = self.legal_targets(side, s.battlecry_target, eid);
                match self.g.rng.pick(&opts) {
                    Some(&t) => t,
                    None => return,
                }
            }
        };
        // Its random picks triple once every trigger of it is done, as a played Battlecry's picks
        // do (Brann's second Discover comes before a triple).
        let prev_no_triples = std::mem::replace(&mut self.no_triples, true);
        self.triggered_battlecries += 1;
        self.trigger_battlecry(side, eid, target, 0);
        self.triggered_battlecries -= 1;
        self.no_triples = prev_no_triples;
        self.check_triples(side);
    }

    /// Trigger a living minion's Deathrattles (it does not die; counts as triggered).
    pub fn trigger_deathrattles(&mut self, side: Side, eid: EntityId) {
        let Some((s, i)) = self.locate(eid) else {
            return;
        };
        let m = self.boards[s][i].clone();
        let _ = side;
        self.run_deathrattles(s, &m, i + 1);
        self.resolve_deaths();
    }

    /// Script counter of the Hero Power `card` owned by `side` (0 if it isn't there).
    pub fn power_counter(&self, side: Side, card: CardId) -> i32 {
        self.player_ref(side)
            .and_then(|p| p.power_of(card))
            .map_or(0, |h| h.counter)
    }

    /// State of the Hero Power `card` owned by `side` (for its script's counters).
    pub fn power_mut(
        &mut self,
        side: Side,
        card: CardId,
    ) -> Option<&mut crate::state::HeroPowerState> {
        self.player(side).and_then(|p| p.power_of_mut(card))
    }

    /// Put Hero Power `card` in `slot` (replacing it, or adding one past the end) and run its
    /// `on_attach` ("At the start of the game..." / on gaining it).
    pub fn set_hero_power(&mut self, side: Side, slot: usize, card: CardId) {
        let Some(p) = self.owner[side] else { return };
        let old = self.g.players[p].powers.get(slot).map(|h| h.card);
        self.g.players[p].set_hero_power(slot, card);
        if let Some(old) = old
            && old != card
            && !self.g.players[p].has_power_id(old)
        {
            crate::content::hero_power_removed(self, side, old);
        }
        let name = &db().get(card).name;
        let who = self.who(side);
        self.g.log(|| format!("{who} gets Hero Power {name}"));
        if let Some(fx) = script(card).and_then(|s| s.on_attach) {
            let me = Me {
                side,
                eid: 0,
                card,
                golden: false,
                opt: 0,
                pos: 0,
                data: 0,
            };
            let prev_src = self.src.replace(card);
            fx(self, me);
            self.src = prev_src;
        }
    }

    /// Offer a choice of `n` distinct Quests, each paired with a random Reward (by offer
    /// weight). The pair fixes the Quest's goal (`content::quest_goal`).
    pub fn offer_quests(&mut self, side: Side, n: usize) {
        let Some(p) = self.owner[side] else { return };
        let all = self.g.cfg.content == crate::config::ContentMode::AllVanilla;
        let lobby = self.g.lobby_races;
        let usable = |kind: CardKind| -> Vec<CardId> {
            db().iter()
                .filter(|c| c.kind == kind && c.normal.is_none())
                .filter(|c| crate::content::quest_offerable(lobby, &c.id))
                .filter(|c| all || script(c.cid).is_some())
                .map(|c| c.cid)
                .collect()
        };
        let (quests, rewards) = (usable(CardKind::Quest), usable(CardKind::QuestReward));
        // Not a Quest already active, nor a Reward already owned or already promised.
        let pl = &self.g.players[p];
        let active: Vec<CardId> = pl.quests.iter().map(|q| q.card).collect();
        let mut owned: Vec<CardId> = pl.rewards.clone();
        owned.extend(pl.quests.iter().map(|q| q.reward));
        let quests: Vec<CardId> = quests.into_iter().filter(|q| !active.contains(q)).collect();
        let endless =
            |r: &CardId| crate::content::ENDLESS_REWARDS.contains(&db().get(*r).id.as_str());
        let rewards: Vec<CardId> = rewards
            .into_iter()
            .filter(|r| !owned.contains(r) || endless(r))
            .collect();
        // Quests uniformly (no pool Quest has a weight); Rewards by their offer weight.
        let qi = self.g.rng.sample_indices(quests.len(), n);
        let rewards = rewards
            .into_iter()
            .map(|r| (r, crate::content::reward_weight(r)))
            .collect();
        let options: Vec<(CardId, CardId)> = qi
            .into_iter()
            .zip(self.weighted_distinct(rewards, n))
            .map(|(a, r)| (quests[a], r))
            .collect();
        if options.is_empty() {
            return;
        }
        if self.combat {
            let (q, r) = options[self.g.rng.below(options.len())];
            self.start_quest(side, q, r);
            return;
        }
        let choice = self.quest_choice(options);
        self.g.players[p].pending.push(choice);
    }

    /// The choice between the Quest / Reward pairs `options`, with the minion types of each pair
    /// drawn now: the offered cards show them.
    pub fn quest_choice(&mut self, options: Vec<(CardId, CardId)>) -> Pending {
        let types = options
            .iter()
            .map(|(q, r)| {
                (
                    crate::content::draw_quest_types(self, *q),
                    crate::content::draw_quest_types(self, *r),
                )
            })
            .collect();
        Pending::Quest { options, types }
    }

    /// Take Quest `quest` with reward `reward`.
    pub fn start_quest(&mut self, side: Side, quest: CardId, reward: CardId) {
        self.start_quest_typed(side, quest, reward, (0, 0));
    }

    /// `start_quest` with the minion types the offer showed for the Quest and for the
    /// Reward (0 = none, or to be drawn now).
    pub fn start_quest_typed(
        &mut self,
        side: Side,
        quest: CardId,
        reward: CardId,
        types: (u16, u16),
    ) {
        let goal = crate::content::quest_goal(self.g.lobby_races, quest, reward);
        let who = self.who(side);
        let (qn, rn) = (&db().get(quest).name, &db().get(reward).name);
        self.g
            .log(|| format!("{who} takes Quest {qn} ({goal}) for {rn}"));
        if let Some(pl) = self.player(side) {
            pl.quests.push(crate::state::QuestState {
                card: quest,
                reward,
                progress: 0,
                goal,
                data: types.0 as i32,
                reward_data: types.1 as i32,
            });
        }
        if let Some(f) = script(quest).and_then(|s| s.on_attach) {
            let me = Me {
                side,
                eid: 0,
                card: quest,
                golden: false,
                opt: 0,
                pos: 0,
                data: types.0 as i32,
            };
            f(self, me);
        }
    }

    /// Advance Quest `quest` by `n`; on completion, gain its Reward (runs `on_attach`).
    pub fn quest_progress(&mut self, side: Side, quest: CardId, n: i32) {
        let Some(pl) = self.player(side) else { return };
        let Some(i) = pl.quests.iter().position(|q| q.card == quest) else {
            return;
        };
        pl.quests[i].progress += n;
        if pl.quests[i].progress < pl.quests[i].goal {
            return;
        }
        // "Can be earned endlessly" (Perpetual Incantation, Another Hidden Body): the Quest
        // starts over and its Reward is earned again at every completion.
        let endless =
            crate::content::ENDLESS_REWARDS.contains(&db().get(pl.quests[i].reward).id.as_str());
        let q = if endless {
            pl.quests[i].progress = 0;
            pl.quests[i].clone()
        } else {
            pl.quests.remove(i)
        };
        if !(endless && pl.rewards.contains(&q.reward)) {
            pl.rewards.push(q.reward);
        }
        let who = self.who(side);
        let rn = &db().get(q.reward).name;
        self.g.log(|| format!("{who} completes a Quest: {rn}"));
        if let Some(fx) = script(q.reward).and_then(|s| s.on_attach) {
            let me = Me {
                side,
                eid: 0,
                card: q.reward,
                golden: false,
                opt: 0,
                pos: 0,
                data: q.reward_data,
            };
            let prev_src = self.src.replace(q.reward);
            fx(self, me);
            self.src = prev_src;
        }
    }

    /// Offer a choice of `n` distinct Hero Powers (from the heroes' pool, excluding the ones
    /// `side` already has and `exclude`) for power slot `slot` (past the end = add one).
    pub fn offer_hero_powers(
        &mut self,
        side: Side,
        n: usize,
        slot: usize,
        exclude: &[CardId],
    ) -> Vec<CardId> {
        let Some(p) = self.owner[side] else {
            return vec![];
        };
        let owned: Vec<CardId> = self.g.players[p].power_cards().collect();
        let content = self.g.cfg.content;
        // The configured exclusions and powers still locked behind a Tier are not offered; neither
        // are powers whose moment has passed (`hero_power_in_time`).
        let tier = self.g.players[p].tier;
        let turn = self.g.turn;
        let banned: Vec<CardId> = self
            .g
            .cfg
            .hero_power_discover_excluded
            .iter()
            .filter_map(|id| db().find(id))
            .collect();
        let cands: Vec<CardId> = crate::content::hero_power_pool(self.g)
            .into_iter()
            .filter(|c| !owned.contains(c) && !exclude.contains(c) && !banned.contains(c))
            .filter(|c| crate::content::hero_power_unlock_tier(*c) <= tier)
            .filter(|c| crate::content::hero_power_in_time(*c, turn))
            .filter(|c| script(*c).is_some() || content == crate::config::ContentMode::AllVanilla)
            .collect();
        let picks = self.g.rng.sample_indices(cands.len(), n);
        let options: Vec<CardId> = picks.into_iter().map(|i| cands[i]).collect();
        if options.is_empty() {
            return options;
        }
        // A power swap for the slot Power of the Storm sits in replaces the Storm: no more
        // choices at the turn start. Not its own choice of the turn, and not a Hero Power
        // that replaces itself (the Storm stays underneath it).
        if !self.storm_offer
            && !self
                .src
                .is_some_and(|s| db().get(s).kind == CardKind::HeroPower)
        {
            crate::content::storm_replaced(self, side, slot);
        }
        if self.combat {
            let c = options[self.g.rng.below(options.len())];
            self.set_hero_power(side, slot, c);
            return options;
        }
        self.g.players[p]
            .pending
            .push(crate::state::Pending::HeroPowerChoice {
                options: options.clone(),
                slot: slot as u8,
            });
        options
    }

    /// Improvement level of a non-minion card.
    pub fn card_level(&self, side: Side, card: CardId) -> i32 {
        self.player_ref(side)
            .and_then(|p| p.levels.iter().find(|x| x.0 == card))
            .map_or(0, |x| x.1)
    }

    pub fn improve_card(&mut self, side: Side, card: CardId) {
        let Some(p) = self.player(side) else { return };
        match p.levels.iter_mut().find(|x| x.0 == card) {
            Some(x) => x.1 += 1,
            None => p.levels.push((card, 1)),
        }
    }

    /// Use up Secret `card` of `side` (it fired). Returns false if it wasn't in play.
    pub fn use_secret(&mut self, side: Side, card: CardId) -> bool {
        let Some(p) = self.player(side) else {
            return false;
        };
        let Some(i) = p.secrets.iter().position(|s| *s == card) else {
            return false;
        };
        p.secrets.remove(i);
        let who = self.who(side);
        let name = &db().get(card).name;
        self.g.log(|| format!("{who}'s Secret is revealed: {name}"));
        true
    }

    /// Value stored for a non-minion card (reward parameters: chosen minion type...).
    /// Per-turn progress of a non-minion card (`Player::turn_data`, reset each turn).
    pub fn turn_data(&self, side: Side, card: CardId) -> i32 {
        self.player_ref(side)
            .and_then(|p| p.turn_data.iter().find(|x| x.0 == card))
            .map_or(0, |x| x.1)
    }

    pub fn add_turn_data(&mut self, side: Side, card: CardId, d: i32) -> i32 {
        let Some(p) = self.player(side) else { return 0 };
        match p.turn_data.iter_mut().find(|x| x.0 == card) {
            Some(x) => {
                x.1 += d;
                x.1
            }
            None => {
                p.turn_data.push((card, d));
                d
            }
        }
    }

    pub fn card_data(&self, side: Side, card: CardId) -> i32 {
        self.player_ref(side)
            .and_then(|p| p.counters.iter().find(|x| x.0 == card))
            .map_or(0, |x| x.1)
    }

    pub fn set_card_data(&mut self, side: Side, card: CardId, v: i32) {
        let Some(p) = self.player(side) else { return };
        match p.counters.iter_mut().find(|x| x.0 == card) {
            Some(x) => x.1 = v,
            None => p.counters.push((card, v)),
        }
    }

    /// "Every N turns" counts turns, not triggers (n = 2 goes no, yes, no, yes... from the turn the
    /// count starts). `tick` advances the count of a player-level card (hero power, trinket,
    /// Reward) and says whether it fired: it runs once a turn, and every further call in that turn
    /// repeats its answer -- an end of turn effect that triggers several times fires that many
    /// times in the firing turn, and the count does not move faster.
    pub fn turn_cycle(
        &mut self,
        side: Side,
        card: CardId,
        tick: impl FnOnce(&mut Self) -> bool,
    ) -> bool {
        match self.turn_data(side, card) {
            0 => {
                let fired = tick(self);
                self.add_turn_data(side, card, if fired { 2 } else { 1 });
                fired
            }
            x => x == 2,
        }
    }

    /// The same for a minion's "at the end of every `n` turns" (its own effect or a Dark
    /// Gift on it): the turn it starts counting is the first of the cycle, each turn counts
    /// once, and every call in a firing turn answers true (a second copy of the same gift,
    /// extra end of turn triggers).
    pub fn minion_turn_cycle(&mut self, eid: EntityId, n: u32) -> bool {
        let turn = self.g.turn;
        let Some(m) = self.minion_mut(eid) else {
            return false;
        };
        if m.cycle_turn != turn {
            m.cycle_turn = turn;
            m.cycle += 1;
        }
        m.cycle % n.max(1) == 0
    }

    /// Advance the progress counter of a non-minion card ("After N...: X. (N left!)"). Returns true
    /// (and resets) each time it reaches `every`.
    /// "Every N turns" on the game's turns (Xavius's and Tickatus's powers count the whole game,
    /// also when gained later - from Genn's offer, a swap): it fires on Turns N, 2N... The card
    /// data is the turns since the last one (card text).
    pub fn period_turn(&mut self, side: Side, card: CardId, every: i32) -> bool {
        let t = self.g.turn as i32;
        self.set_card_data(side, card, t % every);
        t > 0 && t % every == 0
    }

    pub fn tick_counter(&mut self, side: Side, card: CardId, every: i32) -> bool {
        let Some(p) = self.player(side) else {
            return false;
        };
        let i = match p.counters.iter().position(|x| x.0 == card) {
            Some(i) => i,
            None => {
                p.counters.push((card, 0));
                p.counters.len() - 1
            }
        };
        p.counters[i].1 += 1;
        if p.counters[i].1 >= every {
            p.counters[i].1 = 0;
            true
        } else {
            false
        }
    }

    /// Add stacks of a persistent "this game" effect (merged per source card, normal form).
    pub fn add_player_effect(&mut self, side: Side, card: CardId, stacks: i32) {
        let card = db().normal_of(card);
        let name = &db().get(card).name;
        let who = self.who(side);
        self.g
            .log(|| format!("{who} gains a 'this game' effect: {name} (x{stacks})"));
        let Some(p) = self.player(side) else { return };
        match p.effects.iter_mut().find(|x| x.card == card) {
            Some(x) => x.stacks += stacks,
            None => p.effects.push(crate::state::PlayerEffect { card, stacks }),
        }
    }

    /// Queue a "when you have space" summon and serve the queue right away if possible.
    /// Inside a death batch it only waits: the batch's Deathrattles and Reborn come first
    /// (`resolve_deaths` serves the queue when it has settled).
    pub fn when_space(&mut self, side: Side, req: SpaceReq) {
        self.cs[side].waiting.push(req);
        if self.death_depth == 0 {
            self.flush_waiting();
        }
    }

    /// Serve "when you have space" requests while there is room (first in, first out).
    pub fn flush_waiting(&mut self) {
        for s in [self.first, 1 - self.first] {
            while !self.cs[s].waiting.is_empty() && !self.board_full(s) {
                let req = self.cs[s].waiting.remove(0);
                let pos = self.boards[s].len();
                match req {
                    SpaceReq::Minion(m) => {
                        self.summon(s, pos, m);
                    }
                    SpaceReq::InPlace(m, right, index) => {
                        let at = match self.boards[s].iter().position(|x| x.eid == right) {
                            Some(i) => i,
                            None if right == 0 => pos,
                            None => index.min(pos),
                        };
                        self.summon(s, at, m);
                    }
                    SpaceReq::Owed(m, fire) => {
                        let eid = m.eid;
                        if let Some(e) = self.summon(s, pos, m) {
                            if let Some(p) = self.owner[s] {
                                self.g.players[p].owed.retain(|x| x.1.eid != eid);
                            }
                            if fire {
                                let foes: Vec<EntityId> = self.boards[1 - s]
                                    .iter()
                                    .filter(|m| m.alive())
                                    .map(|m| m.eid)
                                    .collect();
                                if let Some(&t) = self.g.rng.pick(&foes) {
                                    crate::combat::do_attack(self, s, e, t);
                                    // Like `attack_now`: its kills resolve right away.
                                    self.resolve_deaths();
                                }
                            }
                        }
                    }
                    SpaceReq::Tracked(m, card) => {
                        if self.summon(s, pos, m).is_some()
                            && let Some(p) = self.owner[s]
                        {
                            let fx = &mut self.g.players[p].effects;
                            if let Some(i) = fx.iter().position(|x| x.card == card) {
                                fx[i].stacks -= 1;
                                if fx[i].stacks <= 0 {
                                    fx.remove(i);
                                }
                            }
                        }
                    }
                    SpaceReq::Deity(m) => {
                        let (card, golden) = (m.card, m.golden);
                        if let Some(e) = self.summon(s, pos, m) {
                            self.cs[s].deity_eid = e;
                            let me = Me {
                                side: s,
                                eid: e,
                                card,
                                golden,
                                opt: 0,
                                pos,
                                data: 0,
                            };
                            crate::content::deity_awakened(self, me);
                        }
                    }
                    SpaceReq::FromHand(hid) => {
                        self.summon_from_hand(s, hid, pos);
                    }
                    SpaceReq::FromHandBest(race) => {
                        let pick = self.highest_attack_in_hand(s, 1, |r| r.intersects(race));
                        if let Some(&hid) = pick.first() {
                            self.summon_from_hand(s, hid, pos);
                        }
                    }
                    SpaceReq::CopyHighest { by_health } => {
                        let keyed: Vec<(usize, i64)> = self.boards[s]
                            .iter()
                            .enumerate()
                            .filter(|(_, m)| m.alive())
                            .map(|(i, m)| (i, if by_health { m.hp } else { m.atk } as i64))
                            .collect();
                        let best = self.pick_max(keyed).map(|i| self.boards[s][i].clone());
                        if let Some(mut copy) = best {
                            copy.eid = self.g.alloc_eid();
                            copy.combat_only = true;
                            copy.pool_copies = 0;
                            copy.attacked = false;
                            self.summon(s, pos, copy);
                        }
                    }
                    SpaceReq::AttackNow(m) => {
                        if let Some(e) = self.summon(s, pos, m) {
                            crate::combat::attack_now(self, s, e);
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------ hero

    pub fn hero_damage(&mut self, side: Side, amount: i32) {
        self.hero_damage_ex(side, amount, false);
    }

    /// Hero damage. `end_of_combat` = the damage for losing a combat (dealt after the fight,
    /// when the loser has no minions left to react). Every instance can be rewound.
    pub fn hero_damage_ex(&mut self, side: Side, amount: i32, end_of_combat: bool) {
        if amount <= 0 {
            return;
        }
        let combat = self.combat || end_of_combat;
        // Ice Block: prevent fatal damage (the Secret is used up).
        let ice = crate::cardid!("TB_Bacon_Secrets_12");
        if let Some(p) = self.player(side)
            && p.health + p.armor <= amount
            && let Some(i) = p.secrets.iter().position(|s| *s == ice)
        {
            p.secrets.remove(i);
            let who = self.who(side);
            self.g
                .log(|| format!("{who}'s Ice Block prevents fatal damage"));
            return;
        }
        let Some(p) = self.player(side) else { return };
        let absorbed = amount.min(p.armor);
        p.armor -= absorbed;
        p.health -= amount - absorbed;
        if !combat {
            p.stats.damage_taken_outside_combat += amount;
        }
        self.bump_player_stat(side, |_| {});
        let prev = self.rewind.replace((side, absorbed, amount - absorbed));
        let prev_by = std::mem::replace(&mut self.hero_hit_by, self.src);
        self.emit(Event::HeroDamaged {
            side,
            amount,
            in_combat: combat,
        });
        self.hero_hit_by = prev_by;
        self.rewind = prev;
    }

    /// "After your hero takes damage, rewind it": undo the damage being resolved (once).
    pub fn rewind_hero_damage(&mut self, side: Side) -> bool {
        let Some((s, armor, health)) = self.rewind else {
            return false;
        };
        if s != side {
            return false;
        }
        self.rewind = Some((s, 0, 0));
        if armor == 0 && health == 0 {
            return false;
        }
        let who = self.who(side);
        self.g
            .log(|| format!("{who} rewinds {} damage", armor + health));
        if let Some(p) = self.player(side) {
            p.armor += armor;
            p.health += health;
        }
        self.bump_player_stat(side, |_| {});
        true
    }

    // ------------------------------------------------------------------ hand & gold

    /// Add an item to the side owner's hand. Returns the hand id, or None if full/no owner.
    pub fn add_to_hand(&mut self, side: Side, item: Item) -> Option<EntityId> {
        self.add_to_hand_impl(side, item, false)
    }

    pub fn add_timewarp_purchase(&mut self, side: Side, item: Item) -> Option<EntityId> {
        self.add_to_hand_impl(side, item, true)
    }

    fn add_to_hand_impl(&mut self, side: Side, mut item: Item, expand: bool) -> Option<EntityId> {
        self.guard(crate::guard::Site::AddToHand, u32::from(item.card().0));
        self.queued = None;
        if let Item::Minion(m) = &mut item {
            m.keep_tavern_buffs();
        }
        let Some(p) = self.owner[side] else {
            // No hand (the ghost): the card is lost and its pool copies go back.
            if let Item::Minion(m) = &item {
                self.g.pool.return_minion(m);
            }
            return None;
        };
        // Trinkets and Quest Rewards have hand protection: with a full hand their cards wait for
        // space (`Player::hand_queue`, delivered by `Game::flush_hand_queue`; official trinket
        // rules 2026-04-06); everything else is lost.
        let expand = expand && self.g.cfg.experimental_mode;
        if expand {
            self.g.players[p].timewarp_hand_limit = self.g.players[p]
                .timewarp_hand_limit
                .max(self.g.players[p].hand.len() + 1)
                .max(self.g.cfg.hand_size);
        }
        let hand_limit = if self.g.cfg.experimental_mode {
            self.g
                .cfg
                .hand_size
                .max(self.g.players[p].timewarp_hand_limit)
        } else {
            self.g.cfg.hand_size
        };
        let protected = self.src.is_some_and(hand_protected);
        if !expand && self.g.players[p].hand.len() >= hand_limit && protected {
            let name = &db().get(item.card()).name;
            self.g
                .log(|| format!("p{p}: hand full, {name} waits for space"));
            // The card keeps the hand id it will have, so that a link can name it.
            let hid = match &mut item {
                Item::Minion(m) => {
                    if m.eid == 0 {
                        m.eid = self.g.alloc_eid();
                    }
                    m.eid
                }
                _ => self.g.alloc_eid(),
            };
            let pl = &mut self.g.players[p];
            pl.queue_marks().push(crate::state::QueueMark {
                hid,
                ..Default::default()
            });
            pl.hand_queue.push(item);
            self.queued = Some(hid);
            return None;
        }
        if !expand && self.g.players[p].hand.len() >= hand_limit {
            // The card is lost (hand overflow); its pool copies go back.
            if let Item::Minion(m) = &item {
                self.g.pool.return_minion(m);
            }
            return None;
        }
        let hid = match &mut item {
            Item::Minion(m) => {
                if m.eid == 0 {
                    m.eid = self.g.alloc_eid();
                }
                apply_bonuses(&self.g.cfg, &self.g.players[p], m);
                // A minion made before the player's Portrait came (a Brann bought from the
                // Tavern) is theirs now (Bronzebeard Portrait's types).
                crate::content::trinket_on_create(&self.g.players[p], m);
                m.eid
            }
            _ => match self.deliver_hid.take() {
                Some(hid) => hid,
                None => self.g.alloc_eid(),
            },
        };
        let name = &db().get(item.card()).name;
        self.g.log(|| format!("p{p} gets {name}"));
        self.g.players[p].hand.push(HandCard {
            hid,
            item,
            locked: 0,
            link: 0,
            group: 0,
            group_left: 0,
            temporary: false,
            src: 0,
            combined: false,
            held: false,
            held_gold: 0,
        });
        self.emit(Event::CardAdded { side, hid });
        if !self.combat {
            self.check_triples(side);
        }
        Some(hid)
    }

    /// Put a card that waited for hand space into the hand, with what it kept
    /// (`QueueMark`): its hand id, its link and the Dark Gifts still to attach.
    pub fn deliver_queued(&mut self, side: Side, item: Item, mark: crate::state::QueueMark) {
        // The card triples only once its marks are on it.
        let prev_no_triples = std::mem::replace(&mut self.no_triples, true);
        self.deliver_hid = (mark.hid != 0).then_some(mark.hid);
        let hid = self.add_to_hand(side, item);
        self.deliver_hid = None;
        if let Some(hid) = hid {
            self.set_hand_link(side, hid, mark.link);
            if mark.temporary
                && let Some(h) = self
                    .player(side)
                    .and_then(|p| p.hand.iter_mut().find(|h| h.hid == hid))
            {
                h.temporary = true;
                h.src = mark.src;
            }
            self.attach_gifts(side, hid, &mark.gifts);
        }
        self.no_triples = prev_no_triples;
        self.check_triples(side);
    }

    /// Run the "when attached" effects of Dark Gifts `gifts` on hand card `hid`: a gifted
    /// Discover's pick, or a card that waited for hand space with them.
    pub fn attach_gifts(&mut self, side: Side, hid: EntityId, gifts: &[CardId]) {
        for &g in gifts {
            if let Some(fx) = script(g).and_then(|s| s.on_attach) {
                let me = Me {
                    side,
                    eid: hid,
                    card: g,
                    golden: false,
                    opt: 0,
                    pos: 0,
                    data: 0,
                };
                let prev_src = self.src.replace(me.card);
                fx(self, me);
                self.src = prev_src;
            }
        }
    }

    /// Link hand card `hid` to its partner `link` (0 = none); a card still waiting for hand
    /// space keeps the link in its mark.
    pub fn set_hand_link(&mut self, side: Side, hid: EntityId, link: EntityId) {
        if link == 0 {
            return;
        }
        let Some(pl) = self.player(side) else { return };
        if let Some(h) = pl.hand.iter_mut().find(|h| h.hid == hid) {
            h.link = link;
        } else if let Some(m) = pl.queue_marks().iter_mut().find(|m| m.hid == hid) {
            m.link = link;
        }
    }

    /// Give a card that is removed from hand at the end of the turn (Spellcraft spells).
    /// `src` = the Spellcraft minion that gave it (0 = none).
    pub fn give_temp_spell(&mut self, side: Side, card: CardId, src: EntityId) -> Option<EntityId> {
        // With a full hand a temporary card waits for space only when its giver has hand
        // protection: a trinket's Spellcraft spell ("ALL cards generated by Trinkets will
        // wait", official trinket rules 2026-04-06). It stays temporary while it waits
        // (`QueueMark::temporary`) and leaves with the turn as it would from the hand. Other
        // temporary cards are lost.
        if !self.src.is_some_and(hand_protected)
            && self
                .player_ref(side)
                .is_some_and(|p| p.hand.len() >= self.g.cfg.hand_size)
        {
            return None;
        }
        let hid = self.give_card(side, card);
        let queued = self.queued;
        let pl = self.player(side)?;
        if let Some(hid) = hid {
            if let Some(h) = pl.hand.iter_mut().find(|h| h.hid == hid) {
                h.temporary = true;
                h.src = src;
            }
        } else if let Some(q) = queued
            && let Some(m) = pl.queue_marks().iter_mut().find(|m| m.hid == q)
        {
            m.temporary = true;
            m.src = src;
        }
        hid
    }

    /// Give a minion's Spellcraft spell (golden minions give the golden spell).
    pub fn give_spellcraft(&mut self, side: Side, eid: EntityId) {
        let Some(m) = self.minion(eid) else { return };
        if let Some(spell) = db().get(m.card).spellcraft {
            self.give_temp_spell(side, spell, eid);
        }
    }

    /// Add a fresh copy of `card` (minion or spell) to hand.
    pub fn give_card(&mut self, side: Side, card: CardId) -> Option<EntityId> {
        self.guard(crate::guard::Site::GiveCard, u32::from(card.0));
        let item = if db().get(card).kind == CardKind::Minion {
            let mut m = self.new_minion(side, card);
            self.take_pool_copy(&mut m);
            Item::Minion(m)
        } else if card == crate::cardid!("BG36_520t") {
            Item::Spell {
                card,
                data: crate::content::LOCKBOX_TURNS,
            }
        } else {
            Item::spell(card)
        };
        self.add_to_hand(side, item)
    }

    /// A full copy of `m` as a new card ("a copy" keeps stats, enchantments, golden and
    /// attachments; "a plain copy" is the base card): new id and one pool copy when one is left. A
    /// Golden copy gives its Triple Reward when played like any Golden minion, unless the
    /// original's text denies one (the flag is copied).
    pub fn full_copy(&mut self, m: &Minion) -> Minion {
        let mut x = m.clone();
        x.eid = self.g.alloc_eid();
        x.pool_copies = 0;
        x.attacked = false;
        x.combat_only = false;
        self.take_pool_copy(&mut x);
        x
    }

    /// Get a full copy of hand card `hid` (a spell: the same spell and data). A card still
    /// waiting for hand space (`hid` = the hand id it was promised) is copied too, with the
    /// Dark Gifts it has yet to get.
    pub fn give_copy_of_hand(&mut self, side: Side, hid: EntityId) -> Option<EntityId> {
        self.queued = None;
        let pl = self.player(side)?;
        let in_hand = pl
            .hand
            .iter()
            .find(|h| h.hid == hid)
            .map(|h| h.item.clone());
        let (item, gifts) = match in_hand {
            Some(item) => (item, Vec::new()),
            None => {
                let i = pl.queue_marks().iter().position(|m| m.hid == hid)?;
                (
                    pl.hand_queue[i].clone(),
                    pl.hand_queue_marks[i].gifts.clone(),
                )
            }
        };
        let item = match item {
            Item::Minion(m) => Item::Minion(self.full_copy(&m)),
            Item::Spell { card, .. } if card == crate::cardid!("BG36_520t") => Item::Spell {
                card,
                data: crate::content::LOCKBOX_TURNS,
            },
            spell => spell,
        };
        let new = self.add_to_hand(side, item);
        if gifts.is_empty() {
            return new;
        }
        match new {
            Some(h) => self.attach_gifts(side, h, &gifts),
            None => {
                if let Some(q) = self.queued
                    && let Some(m) = self
                        .player(side)
                        .and_then(|pl| pl.queue_marks().iter_mut().find(|m| m.hid == q))
                {
                    m.gifts = gifts;
                }
            }
        }
        new
    }

    /// The newest hand card of `card` (the one just gained).
    pub fn newest_in_hand(&self, side: Side, card: CardId) -> Option<EntityId> {
        self.player_ref(side)?
            .hand
            .iter()
            .rev()
            .find(|h| h.item.card() == card)
            .map(|h| h.hid)
    }

    /// A generated pool minion takes one copy from the shared pool when one is left; otherwise it
    /// is an extra copy (0 copies: selling it doesn't grow the pool). A golden instance holds one
    /// copy too.
    pub fn take_pool_copy(&mut self, m: &mut Minion) {
        // Cards outside the lobby universe never have copies, so the pool answers alone.
        let n = db().normal_of(m.card);
        if m.pool_copies == 0 && self.g.pool.take(n) {
            m.pool_copies = 1;
            m.pool_parts.clear();
        }
    }

    pub fn gain_gold(&mut self, side: Side, n: i32) {
        let Some(p) = self.player(side) else { return };
        p.gold += n;
        // Splinter of Aurum: "when you have 15 Gold" holds the moment the Gold comes, from
        // whatever gives it (a Hero Power, another trinket...).
        if n > 0 && p.gold >= 15 && p.has_trinket_id(crate::cardid!("BG32_MagicItem_350")) {
            crate::content::aurum_gold_check(self, side);
        }
    }
    pub fn gold_next_turn(&mut self, side: Side, n: i32) {
        if let Some(p) = self.player(side) {
            p.gold_next_turn += n;
        }
    }
    pub fn add_max_gold(&mut self, side: Side, n: i32) {
        if let Some(p) = self.player(side) {
            p.max_gold_bonus += n;
        }
    }
    pub fn spend_gold(&mut self, side: Side, n: i32) {
        if self.pay_gold(side, n) {
            self.emit(Event::GoldSpent { side, amount: n });
        }
    }

    /// `spend_gold` without its "after you spend Gold" event, which the caller announces
    /// itself (a purchase: after its "after you buy" effects). Whether anything was paid.
    pub fn pay_gold(&mut self, side: Side, n: i32) -> bool {
        if n <= 0 {
            return false;
        }
        if let Some(p) = self.player(side) {
            p.gold -= n;
        }
        // Via bump_player_stat: "for every N Gold you've spent" auras (Malorne).
        self.bump_player_stat(side, |st| {
            st.gold_spent_total += n;
            st.gold_spent_turn += n;
        });
        crate::content::release_ready_held(self, side, n);
        true
    }

    // ------------------------------------------------------------------ triples

    pub fn triple_need(&self, side: Side, card: CardId) -> usize {
        match self.player_ref(side) {
            Some(p) => triple_need_of(p, &self.g.cfg, card),
            None => usize::MAX,
        }
    }

    /// Merge copies of a non-golden minion (board side 0 of the recruit ctx + hand) into a
    /// golden one in hand.
    pub fn check_triples(&mut self, side: Side) {
        self.guard(crate::guard::Site::Triples, side as u32);
        let Some(p) = self.owner[side] else { return };
        // Only in the recruit phase, not during combat or the end of turn, nor once the player has
        // ended the turn: a card that reaches the hand then (a queued card finding space, a
        // Discover answered late) triples at the next turn start - the warband for this round's
        // combat is already set.
        if self.combat || self.no_triples || self.g.ending_turn || self.g.players[p].ended_turn {
            return;
        }
        // Triples wait until every open Discover is answered: Brann's second Discover follows the
        // first without a triple in between. An open target or Choose One menu holds them too: its
        // source and targets must not be tripled away by a card arriving meanwhile (`Game::choose`
        // checks again once the menu is answered or cancelled).
        let pending = &self.g.players[p].pending;
        if pending
            .iter()
            .any(|x| matches!(x, Pending::Target { .. } | Pending::ChooseOne { .. }))
        {
            self.g.players[p].triples_held = true;
            return;
        }
        if pending
            .iter()
            .any(|x| matches!(x, Pending::Discover { .. }))
        {
            return;
        }
        loop {
            let mut counts: Vec<(CardId, usize)> = vec![];
            let mut bump = |c: CardId| {
                if let Some(e) = counts.iter_mut().find(|(k, _)| *k == c) {
                    e.1 += 1;
                } else {
                    counts.push((c, 1));
                }
            };
            for m in &self.boards[side] {
                if !m.golden && db().get(m.card).golden.is_some() && !m.combat_only {
                    bump(m.card);
                }
            }
            // Locked hand cards don't take part.
            for h in &self.g.players[p].hand {
                if let Item::Minion(m) = &h.item
                    && h.locked == 0
                    && !m.golden
                    && db().get(m.card).golden.is_some()
                {
                    bump(m.card);
                }
            }
            // Elemental of Surprise (golden ones too) is a wildcard copy for any Elemental:
            // 2 x X + Surprise = golden X; Surprises (incl. a golden one) merge into a single
            // golden Surprise, so any number of them converges to one.
            let surprise = crate::cardid!("BG26_175");
            let gsurprise = db().golden_of(surprise);
            let golden_wild = self.boards[side]
                .iter()
                .filter(|m| m.card == gsurprise && !m.combat_only)
                .count()
                + self.g.players[p]
                    .hand
                    .iter()
                    .filter(|h| {
                        h.locked == 0 && h.item.as_minion().is_some_and(|m| m.card == gsurprise)
                    })
                    .count();
            let normal_wild = counts
                .iter()
                .find(|(c, _)| *c == surprise)
                .map_or(0, |x| x.1);
            let wild = normal_wild + golden_wild;
            let pick = counts
                .iter()
                .find(|(c, n)| *c != surprise && *n >= self.triple_need(side, *c))
                .map(|x| x.0)
                .or_else(|| {
                    (normal_wild > 0 && wild >= self.triple_need(side, surprise))
                        .then_some(surprise)
                })
                .or_else(|| {
                    counts
                        .iter()
                        .find(|(c, n)| {
                            *c != surprise
                                && db().get(*c).races.intersects(Races::ELEMENTAL)
                                && n + wild >= self.triple_need(side, *c)
                        })
                        .map(|x| x.0)
                });
            let Some(card) = pick else {
                return;
            };
            let need = self.triple_need(side, card);
            let mut parts: Vec<Minion> = vec![];
            for want in [card, surprise, gsurprise] {
                // The copies owned longest merge first, on the board or in hand: with one on the
                // board, one in hand and two more arriving together (a Discover and Sinstone's
                // extra copy), the last one stays in hand. Entity ids grow in the order the cards
                // were made.
                let mut have: Vec<EntityId> = self.g.players[p]
                    .hand
                    .iter()
                    .filter(|h| {
                        h.locked == 0 && matches!(&h.item, Item::Minion(m) if m.card == want)
                    })
                    .map(|h| h.hid)
                    .chain(
                        self.boards[side]
                            .iter()
                            .filter(|m| m.card == want && !m.combat_only)
                            .map(|m| m.eid),
                    )
                    .collect();
                have.sort_unstable();
                have.truncate(need - parts.len());
                for eid in have {
                    let hand = &mut self.g.players[p].hand;
                    if let Some(i) = hand.iter().position(|h| h.hid == eid) {
                        if let Item::Minion(m) = hand.remove(i).item {
                            parts.push(m);
                        }
                    } else if let Some(i) = self.boards[side].iter().position(|m| m.eid == eid) {
                        parts.push(self.boards[side].remove(i));
                    }
                }
            }
            let golden = self.merge_golden(side, card, &parts);
            // Effects scheduled against a merged part (Incubation's doubling) follow the
            // golden minion.
            for s in self.g.players[p].scheduled.iter_mut() {
                if parts.iter().any(|x| x.eid == s.target) {
                    s.target = golden.eid;
                }
            }
            // Asher the Haberdasher's hats and Dancin' Deryl's follow it too.
            for h in self.g.players[p].hats.iter_mut() {
                if parts.iter().any(|x| x.eid == *h) {
                    *h = golden.eid;
                }
            }
            let hats = crate::cardid!("TB_BaconShop_HP_042");
            for h in self.g.players[p]
                .powers
                .iter_mut()
                .filter(|h| h.card == hats)
            {
                for v in h.values.iter_mut() {
                    if parts.iter().any(|x| x.eid as i32 == *v) {
                        *v = golden.eid as i32;
                    }
                }
            }
            let name = golden.name();
            self.g.log(|| format!("p{p} triples into golden {name}"));
            // No hand-size protection: with a full hand the golden card is lost.
            self.add_to_hand(side, Item::Minion(golden));
        }
    }

    fn merge_golden(&mut self, side: Side, card: CardId, parts: &[Minion]) -> Minion {
        let d = db();
        let base = d.get(card);
        let gcard = d.golden_of(card);
        let mut g = self.new_minion(side, gcard);
        let (mut ea, mut eh) = (0i64, 0i64);
        // What the parts had "this turn" / "until next turn": the Golden inherits it too and loses
        // it when it would have ended (a triple drops nothing).
        let (mut ta, mut th, mut tkw) = (0i32, 0i32, Kw::NONE);
        let mut copies = 0u8;
        let mut origins: Vec<(CardId, u8)> = vec![];
        for m in parts {
            // A golden part (Elemental of Surprise merges) keeps only its gains above its own
            // golden base.
            let pb = d.get(m.card);
            // Golden base + every part's extra stats, losses included.
            ea += m.atk as i64 - pb.atk as i64 - m.pbonus_atk as i64 - m.temp_atk as i64;
            eh += m.max_hp as i64 - pb.hp as i64 - m.pbonus_hp as i64 - m.temp_hp as i64;
            copies += m.pool_copies;
            m.visit_pool_parts(|c, n| {
                if let Some(part) = origins.iter_mut().find(|part| part.0 == c) {
                    part.1 += n as u8;
                } else {
                    origins.push((c, n as u8));
                }
            });
            g.gems = g.gems.saturating_add(m.gems);
            g.gem_atk = g.gem_atk.saturating_add(m.gem_atk);
            g.gem_hp = g.gem_hp.saturating_add(m.gem_hp);
            g.kw.set(Kw(m.kw.0 & !m.temp_kw.0 & !base.kw.0));
            ta = ta.saturating_add(m.temp_atk);
            th = th.saturating_add(m.temp_hp);
            tkw.set(m.temp_kw);
            g.extend_attachments_from(m);
            g.bonus_kw.set(m.bonus_kw);
            g.gained_dr.extend(m.gained_dr.iter().copied());
            // Dark Gift state outside `attached` and the stat sums (all gifts of the parts stack):
            // Steady Growth stage / Replication counters, Toreth's 3-hit shield, Amalgamation's
            // all-types.
            g.gift_data = g.gift_data.max(m.gift_data);
            g.shield_max = g.shield_max.max(m.shield_max);
            if m.races == Races::ALL {
                g.races = Races::ALL;
            }
        }
        let ea = ea.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        let eh = eh.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        g.add_stats(ea, eh);
        if self.g.cfg.record_log && (ea != 0 || eh != 0) {
            g.record(None, crate::state::ModKind::Merge, ea, eh);
        }
        let (old_a, old_h) = (g.atk, g.max_hp);
        g.add_stats(ta, th);
        g.temp_atk = g.atk.saturating_sub(old_a);
        g.temp_hp = g.max_hp.saturating_sub(old_h);
        // Temporary only where no part (nor the card) has the keyword for good.
        let tkw = Kw(tkw.0 & !g.kw.0);
        g.kw.set(tkw);
        g.temp_kw = tkw;
        // A fresh golden has a full shield, including a multi-hit one carried over from a part
        // (the new instance starts at 1 hit when its card has Divine Shield).
        if g.has(Kw::DIVINE_SHIELD) {
            g.shield_hits = g.shield_max.max(1);
        }
        g.pool_copies = copies;
        if origins.iter().any(|part| part.0 != card) {
            g.pool_parts = origins;
        }
        g.triple_reward = true;
        // "Improve this" growth of the parts adds up on the Golden (`flag::IMPROVES`).
        if script(gcard).is_some_and(|s| s.has_flag(flag::IMPROVES)) {
            g.counter = parts.iter().fold(0i32, |n, m| n.saturating_add(m.counter));
        }
        if crate::content::starcraft::is_zerg(gcard) && parts.iter().any(|m| m.counter < 0) {
            g.counter = -1;
        }
        if let Some(f) = script(gcard).and_then(|s| s.on_merge) {
            f(&mut g, parts);
        }
        g
    }

    // ------------------------------------------------------------------ spells

    pub fn spell_buff(&mut self, me: Me, target: EntityId, atk: i32, hp: i32) {
        let (ea, eh) = if crate::content::is_tavern_spell_instance(me.card, me.data) {
            self.spell_extra(me.side)
        } else {
            (0, 0)
        };
        self.buff(target, atk.saturating_add(ea), hp.saturating_add(eh));
    }

    /// "Your Tavern spells give an extra +a/+h": this-game bonuses plus board auras
    /// (Humon'gozz, Enchanted Sentinel incl. magnetized).
    pub fn spell_extra(&self, side: Side) -> (i32, i32) {
        let (mut a, mut h) = self
            .player_ref(side)
            .map(|p| (p.stats.spell_atk, p.stats.spell_hp))
            .unwrap_or((0, 0));
        static IDS: std::sync::OnceLock<(CardId, CardId, CardId, CardId)> =
            std::sync::OnceLock::new();
        let (gozz, sentinel, sentinel_g, lubber) = *IDS.get_or_init(|| {
            let s = db().id("BG35_341");
            (
                db().id("BG32_341"),
                s,
                db().golden_of(s),
                db().id("BG34_Giant_066"),
            )
        });
        for m in self.boards[side].iter().filter(|m| m.alive()) {
            // Timewarped Lubber: your Tavern spells give an extra +1/+1 (+2/+2).
            if db().normal_of(m.card) == lubber {
                let g = if m.golden { 2 } else { 1 };
                a = a.saturating_add(g);
                h = h.saturating_add(g);
            }
            if db().normal_of(m.card) == gozz {
                let g = if m.golden { 2 } else { 1 };
                a = a.saturating_add(g);
                h = h.saturating_add(2 * g);
            }
            // Enchanted Sentinel: on the board or Magnetized to a minion.
            for c in std::iter::once(m.card).chain(m.attached.iter().copied()) {
                if c == sentinel {
                    a = a.saturating_add(1);
                    h = h.saturating_add(1);
                } else if c == sentinel_g {
                    a = a.saturating_add(2);
                    h = h.saturating_add(2);
                }
            }
        }
        // Trinkets: Heart of the Forest (+1/+1, improved), Honeycomb Ring (+1/+1, improved
        // for the turn), Multilayered Shield (+1/+2 per different type, plus the base).
        if let Some(p) = self.player_ref(side)
            && !p.trinkets.is_empty()
        {
            static T: std::sync::OnceLock<[CardId; 3]> = std::sync::OnceLock::new();
            let [heart, honey, shield] = *T.get_or_init(|| {
                [
                    db().id("BG32_MagicItem_801t"),
                    db().id("BG36_MagicItem_371"),
                    db().id("BG36_MagicItem_373"),
                ]
            });
            // A Souvenir Stand copy is a trinket of its own: it is matched by its card (`base`) and
            // keeps its own level / turn count under its own id.
            for &t in &p.trinkets {
                if t.base() == heart {
                    let k = self.card_level(side, t).saturating_add(1);
                    a = a.saturating_add(k);
                    h = h.saturating_add(k);
                } else if t.base() == honey {
                    let k = self.turn_data(side, t).saturating_add(1);
                    a = a.saturating_add(k);
                    h = h.saturating_add(k);
                } else if t.base() == shield {
                    let races: Vec<Races> = self.boards[side]
                        .iter()
                        .filter(|m| m.alive())
                        .map(|m| m.races)
                        .collect();
                    let k = 1 + crate::content::distinct_types_of(&races);
                    a = a.saturating_add(k);
                    h = h.saturating_add(2 * k);
                }
            }
        }
        (a, h)
    }

    /// Add `delta` to a minion's counter; in combat the warband copy keeps it too
    /// ("improve this permanently", damage dealt "(35 left!)"). Returns the new value.
    pub fn counter_perm(&mut self, side: Side, eid: EntityId, delta: i32) -> i32 {
        let combat = self.combat;
        let v = match self.minion_mut(eid) {
            Some(m) => {
                m.counter = m.counter.saturating_add(delta);
                m.counter
            }
            None => return 0,
        };
        if combat && let Some(m) = self.warband_minion_mut(side, eid) {
            m.counter = v;
        }
        v
    }

    /// Cast a spell a card's text has YOU cast ("Cast Shiny Ring twice", "cast it again": no "this
    /// casts"). It is your spell -- it counts toward your spell counters, triggers "after you cast"
    /// and is multiplied by "your spells cast an extra time" effects -- but it is not played from
    /// hand.
    pub fn you_cast(&mut self, side: Side, card: CardId, data: i32, target: EntityId, opt: u8) {
        let times = self.spell_cast_times(side, card, data, target, false);
        for _ in 0..times {
            self.cast_by_you = true;
            self.cast_spell(side, card, data, target, opt);
        }
    }

    /// Cast a spell for `side` (not paying; callers handle cost and hand removal).
    pub fn cast_spell(&mut self, side: Side, card: CardId, data: i32, target: EntityId, opt: u8) {
        self.guard(crate::guard::Site::Cast, u32::from(card.0));
        // A Boon of Beetles cast while its effect is pending doesn't happen, the effect's cast
        // included (the two a Magicfin Apprentice cast with Brann stayed set aside, 2 Beetles left
        // before and after) - as a hand play is refused (`Game::apply`).
        if script(card).is_some_and(|s| s.has_flag(flag::ONE_PENDING))
            && self
                .player_ref(side)
                .is_some_and(|p| p.effects.iter().any(|x| x.card == db().normal_of(card)))
        {
            let name = &db().get(card).name;
            let who = self.who(side);
            self.g
                .log(|| format!("{who}'s {name} isn't cast: its effect is already pending"));
            return;
        }
        let tavern = crate::content::is_tavern_spell_instance(card, data);
        if self.g.cfg.record_log {
            let name = &db().get(card).name;
            let who = self.who(side);
            let on = self.target_name(target);
            self.g.log(|| format!("{who} casts {name}{on}"));
        }
        // "You cast a spell" is a cast you make from hand, or one a card's text has you make ("cast
        // X": `you_cast`). A cast the text gives to the card itself ("this casts", "this plays") --
        // also when nested inside your cast -- resolves normally but neither counts toward spell
        // counters nor triggers "after you cast" (`effect_casts_count` makes every cast count).
        let hand = self.cast_from_hand && !self.in_cast;
        let yours = std::mem::take(&mut self.cast_by_you);
        let counts = hand || yours || self.g.cfg.effect_casts_count;
        let prev_cast = (self.in_cast, self.cast_from_hand, self.cast_on_friendly);
        self.in_cast = true;
        self.cast_from_hand = hand;
        self.cast_on_friendly = target != 0
            && self.boards[side]
                .iter()
                .any(|m| m.eid == target && m.alive());
        let cast = Event::SpellCast {
            side,
            card,
            target,
            tavern,
            data,
            opt,
        };
        // "Whenever you cast" acts before the spell's effect, "after you cast" after it
        // (Hooktail's +1 Attack is in before the spell reads stats).
        if counts {
            let prev = std::mem::replace(&mut self.spell_phase, 1);
            self.emit(cast.clone());
            self.spell_phase = prev;
        }
        if let Some(s) = script(card)
            && let Some(fx) = s.spell
        {
            let me = Me {
                side,
                eid: 0,
                card,
                golden: false,
                opt,
                pos: 0,
                data,
            };
            let prev_src = self.src.replace(me.card);
            fx(self, me, target);
            self.src = prev_src;
        }
        if counts {
            let spellcraft = db().get(card).spellcraft_spell;
            self.bump_player_stat(side, |st| {
                st.spells_cast += 1;
                st.spells_cast_turn += 1;
                if spellcraft {
                    st.spellcraft_cast += 1;
                }
                if tavern {
                    st.tavern_spells_cast += 1;
                    st.tavern_spells_cast_turn += 1;
                    st.last_spell_cast = Some(card);
                    st.last_spell_cast_data = data;
                }
            });
            let prev = std::mem::replace(&mut self.spell_phase, 2);
            self.emit(cast);
            self.spell_phase = prev;
        }
        (self.in_cast, self.cast_from_hand, self.cast_on_friendly) = prev_cast;
        self.resolve_deaths();
    }

    /// Once per physical card, while all extra casts share the same eligibility.
    pub fn use_physical_spell_allowance(&mut self, eid: EntityId) -> bool {
        if !self.cast_from_hand || self.cast_hid == 0 {
            return false;
        }
        if let Some(&(_, _, eligible)) = self
            .physical_spell_uses
            .iter()
            .find(|&&(source, hid, _)| source == eid && hid == self.cast_hid)
        {
            return eligible;
        }
        let eligible = self.minion_mut(eid).is_some_and(|m| {
            if m.counter > 0 {
                m.counter -= 1;
                true
            } else {
                false
            }
        });
        self.physical_spell_uses
            .push((eid, self.cast_hid, eligible));
        eligible
    }

    /// Current stats of one Blood Gem for `side` (base +1/+1 plus "this game" bonuses).
    pub fn gem_stats(&self, side: Side) -> (i32, i32) {
        let Some(p) = self.player_ref(side) else {
            return (1, 1);
        };
        let (mut a, mut h) = (
            p.stats.gem_atk.saturating_add(1),
            p.stats.gem_hp.saturating_add(1),
        );
        // Surveyor Portrait: Blood Gems played from your hand give an extra +6/+6. Only the
        // gem card itself: gems an effect plays while another hand spell resolves don't.
        let from_hand = self.cast_from_hand
            && self
                .cast_card
                .is_some_and(|c| db().get(c).id.starts_with("BG20_GEM"));
        if from_hand && p.has_trinket("BG30_MagicItem_943") {
            a = a.saturating_add(6);
            h = h.saturating_add(6);
        }
        (a, h)
    }

    /// "Play a Blood Gem on" a minion (in combat this lasts for the combat only).
    pub fn play_gem(&mut self, side: Side, target: EntityId) {
        self.play_gem_inner(side, target, false);
    }

    /// "Play a permanent Blood Gem on" a minion (combat gems also stick to the warband).
    pub fn play_gem_perm(&mut self, side: Side, target: EntityId) {
        self.play_gem_inner(side, target, true);
    }

    fn play_gem_inner(&mut self, side: Side, target: EntityId, permanent: bool) {
        let (a, h) = self.gem_stats(side);
        self.play_gem_sized(side, target, a, h, permanent);
    }

    /// Play one Blood Gem of the given size (Blood Gem Barrage's gem carries more than the
    /// usual stats).
    pub fn play_gem_sized(
        &mut self,
        side: Side,
        target: EntityId,
        a: i32,
        h: i32,
        permanent: bool,
    ) {
        if permanent {
            self.buff_perm(side, target, a, h);
        } else {
            self.buff(target, a, h);
        }
        let combat = self.combat;
        let track = |m: &mut Minion| {
            m.gems = m.gems.saturating_add(1);
            m.gem_atk = m.gem_atk.saturating_add(a);
            m.gem_hp = m.gem_hp.saturating_add(h);
        };
        if let Some(m) = self.find_any_mut(target) {
            track(m);
        }
        if combat
            && permanent
            && let Some(p) = self.owner[side]
            && let Some(m) = self.g.players[p].board.iter_mut().find(|m| m.eid == target)
        {
            track(m);
        }
        self.emit(Event::GemPlayed { side, target });
    }

    // ------------------------------------------------------------------ pools & discover

    /// The minion whose effect is resolving (its normal card), if any. A minion's random and
    /// Discover minion generation never yields the minion's own card (Primalfin Lookout Discovers
    /// no Primalfin Lookout, a sold Tad gives no Tad). Spells, Hero Powers and trinkets have no
    /// such limit (Boundless Potential can Discover itself).
    pub fn generating_minion(&self) -> Option<CardId> {
        let d = db();
        self.src
            .filter(|c| d.get(*c).kind == CardKind::Minion)
            .map(|c| d.normal_of(c))
    }

    /// Random lobby minion (normal form) matching `f`. Generation does not consume the
    /// shared pool (a simulator assumption).
    /// A random lobby minion matching `f`, drawn from the shared pool (weighted by the copies
    /// left). With no copy left it is picked uniformly among the matching cards (an extra copy).
    /// This only chooses the card: `give_card` takes the copy.
    pub fn random_minion(&mut self, f: impl Fn(&CardDef) -> bool) -> Option<CardId> {
        let d = db();
        let own = self.generating_minion();
        let f = |x: &CardDef| Some(x.cid) != own && f(x);
        // Two passes over the universe instead of collecting the candidates.
        let (mut total, mut count) = (0u64, 0usize);
        for &c in &self.g.universe {
            if f(d.get(c)) {
                total += self.g.pool.get(c) as u64;
                count += 1;
            }
        }
        if count == 0 {
            return None;
        }
        if total == 0 {
            let k = self.g.rng.below(count);
            return self
                .g
                .universe
                .iter()
                .copied()
                .filter(|c| f(d.get(*c)))
                .nth(k);
        }
        let mut r = self.g.rng.next_u64() % total;
        for &c in &self.g.universe {
            if !f(d.get(c)) {
                continue;
            }
            let n = self.g.pool.get(c) as u64;
            if r < n {
                return Some(c);
            }
            r -= n;
        }
        None
    }

    /// A random lobby Tavern spell matching `f`, drawn by copies left (effects take copies too).
    /// With no copies left, uniform: the result is an extra copy.
    pub fn random_spell(&mut self, f: impl Fn(&CardDef) -> bool) -> Option<CardId> {
        let cands = self.spell_cands(f);
        if let Some(&c) = self.weighted_distinct(cands.clone(), 1).first() {
            return Some(c);
        }
        let all: Vec<CardId> = cands.into_iter().map(|x| x.0).collect();
        self.g.rng.pick(&all).copied()
    }

    /// A Tavern spell for a Tavern slot: strictly by copies left (none left = no spell).
    pub fn draw_pool_spell(&mut self, f: impl Fn(&CardDef) -> bool) -> Option<CardId> {
        // Bob-blehead also keeps Tier 1-2 spells out of the Tavern.
        let floor = self.tavern_tier_floor(0);
        let cands = self.spell_cands(|d| d.tier >= floor && f(d));
        self.weighted_distinct(cands, 1).first().copied()
    }

    fn spell_cands(&self, f: impl Fn(&CardDef) -> bool) -> Vec<(CardId, u64)> {
        let d = db();
        let mut cands: Vec<(CardId, u64)> = self
            .g
            .spell_universe
            .iter()
            .copied()
            .filter(|c| f(d.get(*c)))
            .map(|c| {
                let t = d.get(c).tier as usize;
                // Tier 7 spells (Tier 7 Taverns only) have as many copies as Tier 6 ones (no count
                // is published).
                let n = if (1..=7).contains(&t) {
                    self.g.cfg.spell_copies_by_tier[t.min(6) - 1] as u64
                } else {
                    0
                };
                (c, n)
            })
            .collect();
        // One pass over Taverns and hands (as `Game::spell_copies_left`).
        for p in &self.g.players {
            let items = p
                .shop
                .iter()
                .map(|s| &s.item)
                .chain(p.hand.iter().map(|h| &h.item));
            for it in items {
                if let Item::Spell { card, .. } = it
                    && let Some(x) = cands.iter_mut().find(|x| x.0 == *card)
                {
                    x.1 = x.1.saturating_sub(1);
                }
            }
        }
        cands
    }

    /// Up to `n` distinct cards drawn by weight without replacement (zero weights never).
    pub fn weighted_distinct(&mut self, cands: Vec<(CardId, u64)>, n: usize) -> Vec<CardId> {
        let mut cands: Vec<(CardId, u64)> = cands.into_iter().filter(|x| x.1 > 0).collect();
        let mut out = vec![];
        while out.len() < n && !cands.is_empty() {
            let total: u64 = cands.iter().map(|x| x.1).sum();
            let mut r = self.g.rng.next_u64() % total;
            let mut i = 0;
            while r >= cands[i].1 {
                r -= cands[i].1;
                i += 1;
            }
            out.push(cands.swap_remove(i).0);
        }
        out
    }

    /// Offer a Discover of up to `n` distinct lobby minions matching `f` (taken from the
    /// pool when chosen).
    /// Discover minions matching `f` from the pool: options are drawn by copies left, distinct;
    /// fewer kinds left = fewer options; none = no Discover.
    pub fn discover_minion(
        &mut self,
        side: Side,
        source: CardId,
        n: usize,
        f: impl Fn(&CardDef) -> bool,
    ) {
        let options = self.minion_options(side, n, f);
        self.offer_discover(side, source, options);
    }

    pub fn minion_options(
        &mut self,
        side: Side,
        n: usize,
        f: impl Fn(&CardDef) -> bool,
    ) -> Vec<Item> {
        let d = db();
        let own = self.generating_minion();
        let f = |x: &CardDef| Some(x.cid) != own && f(x);
        let cands: Vec<(CardId, u64)> = self
            .g
            .universe
            .iter()
            .copied()
            .filter(|c| f(d.get(*c)))
            .map(|c| (c, self.g.pool.get(c) as u64))
            .collect();
        self.weighted_distinct(cands, n)
            .into_iter()
            .map(|card| Item::Minion(self.new_minion(side, card)))
            .collect()
    }

    /// Discover a Tier 7 minion (`n` options) from the pool.
    pub fn discover_tier7(&mut self, side: Side, source: CardId, n: usize) {
        self.discover_minion(side, source, n, |d| d.pools.tier7);
    }

    pub fn discover_spell(
        &mut self,
        side: Side,
        source: CardId,
        n: usize,
        f: impl Fn(&CardDef) -> bool,
    ) {
        // By copies left; spells with none left only fill missing options.
        let cands = self.spell_cands(f);
        let mut picks = self.weighted_distinct(cands.clone(), n);
        let rest: Vec<CardId> = cands
            .into_iter()
            .map(|x| x.0)
            .filter(|c| !picks.contains(c))
            .collect();
        for i in self.g.rng.sample_indices(rest.len(), n - picks.len()) {
            picks.push(rest[i]);
        }
        let options = picks.into_iter().map(Item::spell).collect();
        self.offer_discover(side, source, options);
    }

    /// Present a Discover choice (auto-picked at random in combat).
    pub fn offer_discover(&mut self, side: Side, source: CardId, options: Vec<Item>) {
        self.guard(crate::guard::Site::Discover, u32::from(source.0));
        if options.is_empty() {
            return;
        }
        // A Battlecry triggered by another effect (Kelp Keeper, Snicker-snack, Young Murk-Eye...)
        // can't ask, in the recruit phase either: the pick is random and taken like the player's
        // pick.
        if self.triggered_battlecries > 0 && !self.combat {
            let i = self.g.rng.below(options.len());
            let item = options.into_iter().nth(i).unwrap();
            self.take_discover_pick(side, source, item);
            return;
        }
        if self.combat {
            let i = self.g.rng.below(options.len());
            let item = options.into_iter().nth(i).unwrap();
            let item = match script(source).and_then(|s| s.discover_pick) {
                Some(f) => match f(self, side, source, item) {
                    Some(item) => item,
                    None => return,
                },
                None => item,
            };
            let Some(item) = crate::content::consume_discover_choice(self, side, source, item)
            else {
                return;
            };
            let mut item = item;
            // The pick takes a pool copy when one is left, like the recruit-phase pick.
            if let Item::Minion(m) = &mut item {
                self.take_pool_copy(m);
            }
            let card = item.card();
            let data = if let Item::Spell { data, .. } = &item {
                *data
            } else {
                0
            };
            let hid = self.add_to_hand(side, item);
            let promised = hid.or(self.queued).unwrap_or(0);
            self.emit(Event::Discovered {
                side,
                card,
                hid: promised,
            });
            crate::content::after_discover(self, side, source, card, hid, data);
            return;
        }
        if let Some(p) = self.player(side) {
            p.pending.push(Pending::Discover { source, options });
        }
    }

    // ------------------------------------------------------------------ targets

    pub fn legal_targets(&self, side: Side, spec: TargetSpec, source: EntityId) -> Vec<EntityId> {
        let b = &self.boards[side];
        match spec {
            TargetSpec::None => vec![],
            TargetSpec::Friendly => b.iter().map(|m| m.eid).collect(),
            TargetSpec::FriendlyOther => b
                .iter()
                .filter(|m| m.eid != source)
                .map(|m| m.eid)
                .collect(),
            TargetSpec::FriendlyRace(r) => b.iter().filter(|m| m.is(r)).map(|m| m.eid).collect(),
            TargetSpec::FriendlyRaceOther(r) => b
                .iter()
                .filter(|m| m.is(r) && m.eid != source)
                .map(|m| m.eid)
                .collect(),
            TargetSpec::FriendlyNonGoldenMaxTier(t) => b
                .iter()
                .filter(|m| !m.golden && m.tier <= t && db().get(m.card).golden.is_some())
                .map(|m| m.eid)
                .collect(),
            TargetSpec::FriendlyEligiblePirate => b
                .iter()
                .filter(|m| {
                    m.eid != source
                        && m.is(Races::PIRATE)
                        && !m.golden
                        && m.tier <= 4
                        && db().get(m.card).golden.is_some()
                })
                .map(|m| m.eid)
                .collect(),
            TargetSpec::Tavern => self.tavern_minions(side),
            TargetSpec::TavernCard => self
                .player_ref(side)
                .map(|p| {
                    p.shop
                        .iter()
                        .filter(|s| s.item.as_minion().is_none_or(|m| !m.golden))
                        .map(|s| s.sid)
                        .collect()
                })
                .unwrap_or_default(),
            TargetSpec::TavernAny => self
                .player_ref(side)
                .map(|p| p.shop.iter().map(|s| s.sid).collect())
                .unwrap_or_default(),
            TargetSpec::FriendlyOrTavernAny => {
                let mut v: Vec<EntityId> = b.iter().map(|m| m.eid).collect();
                if let Some(p) = self.player_ref(side) {
                    v.extend(p.shop.iter().map(|s| s.sid));
                }
                v
            }
            TargetSpec::FriendlyRallyTriggerable => b
                .iter()
                .filter(|m| {
                    crate::script::minion_has(m, crate::script::Hook::Rally)
                        && !crate::script::rally_untriggerable(m.card)
                        && m.eid != source
                })
                .map(|m| m.eid)
                .collect(),
            TargetSpec::FriendlyHas(h) => b
                .iter()
                .filter(|m| crate::script::minion_has(m, h) && m.eid != source)
                .map(|m| m.eid)
                .collect(),
            // Locked cards can't be chosen (to discard) either. The hand card to discard (the only
            // use of this spec).
            TargetSpec::Hand => self
                .player_ref(side)
                .map(|p| {
                    p.hand
                        .iter()
                        .filter(|h| crate::recruit::discardable(h))
                        .map(|h| h.hid)
                        .collect()
                })
                .unwrap_or_default(),
            TargetSpec::FriendlyOrTavern => {
                let mut v: Vec<EntityId> = b.iter().map(|m| m.eid).collect();
                v.extend(self.tavern_minions(side));
                v
            }
            TargetSpec::AnyNonGolden => {
                self.any_where(side, |m| !m.golden && db().get(m.card).golden.is_some())
            }
            TargetSpec::AnyMaxTier(t) => self.any_where(side, |m| m.tier <= t),
            TargetSpec::AnyOther => self.any_where(side, |m| m.eid != source),
            TargetSpec::AnyRace(r) => self.any_where(side, |m| m.is(r)),
            TargetSpec::AnyRaceOther(r) => self.any_where(side, |m| m.is(r) && m.eid != source),
            TargetSpec::AnyTyped => self.any_where(side, |m| !m.races.is_none()),
            TargetSpec::AnyNoDivineShield => self.any_where(side, |m| !m.has(Kw::DIVINE_SHIELD)),
            TargetSpec::BuddyNonGolden => self.any_where(side, |m| {
                m.eid != source && crate::script::gildable_buddy(m)
            }),
        }
    }

    /// The item with the largest key; ties are broken at random ("highest / lowest X" picks
    /// randomly among equals unless the text says left- / right-most).
    pub fn pick_max<T: Copy>(&mut self, items: impl IntoIterator<Item = (T, i64)>) -> Option<T> {
        let items: Vec<(T, i64)> = items.into_iter().collect();
        let best = items.iter().map(|x| x.1).max()?;
        let ties: Vec<T> = items.iter().filter(|x| x.1 == best).map(|x| x.0).collect();
        self.g.rng.pick(&ties).copied()
    }

    /// Friendly board and Tavern minions matching `f`.
    fn any_where(&self, side: Side, f: impl Fn(&Minion) -> bool) -> Vec<EntityId> {
        let mut v: Vec<EntityId> = self.boards[side]
            .iter()
            .filter(|m| f(m))
            .map(|m| m.eid)
            .collect();
        v.extend(
            self.tavern_minions(side)
                .into_iter()
                .filter(|e| self.find_any(*e).is_some_and(&f)),
        );
        v
    }

    pub fn tavern_minions(&self, side: Side) -> Vec<EntityId> {
        match self.player_ref(side) {
            Some(p) if !self.combat => p
                .shop
                .iter()
                .filter(|s| matches!(s.item, Item::Minion(_)))
                .map(|s| s.sid)
                .collect(),
            _ => vec![],
        }
    }

    // ------------------------------------------------------------------ battlecries

    /// Fix the multiplier once for a minion's complete Battlecry play/trigger. A composite
    /// minion's sources share War Drum's consumed allowance instead of spending it per part.
    pub fn battlecry_play_multiplier(&mut self, side: Side, eid: EntityId) -> u32 {
        let Some(m) = self.minion(eid) else { return 1 };
        let card = m.card;
        let dragon = m.is(Races::DRAGON);
        let attached = m.attached.clone();
        self.guard(crate::guard::Site::Battlecry, u32::from(card.0));
        let mut mult = self.battlecry_mult(side);
        let has_bc = std::iter::once(card)
            .chain(attached.iter().copied())
            .any(|c| script(c).is_some_and(|s| crate::script::Hook::Battlecry.in_script(s)));
        if has_bc
            && let Some(p) = self.player_ref(side)
            && !p.trinkets.is_empty()
        {
            if dragon {
                mult += p.trinket_count(crate::cardid!("BG36_MagicItem_215"));
            }
            let drum = crate::cardid!("BG32_MagicItem_416");
            let drums: Vec<CardId> = p
                .trinkets
                .iter()
                .copied()
                .filter(|t| t.base() == drum && self.turn_data(side, *t) == 0)
                .collect();
            for drum in drums {
                self.add_turn_data(side, drum, 1);
                mult += 2;
            }
        }
        mult
    }

    /// Execute just one source, without selecting targets or consuming multipliers again.
    pub fn trigger_battlecry_part(
        &mut self,
        side: Side,
        eid: EntityId,
        card: CardId,
        target: EntityId,
        opt: u8,
        mult: u32,
    ) {
        self.with_deferred_triples(|ctx| {
            ctx.trigger_battlecry_part_inner(side, eid, card, target, opt, mult)
        });
    }

    fn trigger_battlecry_part_inner(
        &mut self,
        side: Side,
        eid: EntityId,
        card: CardId,
        target: EntityId,
        opt: u8,
        mult: u32,
    ) {
        let Some(s) = script(card) else { return };
        let Some(bc) = s.battlecry else { return };
        let golden = self
            .minion(eid)
            .filter(|m| m.card == card)
            .map_or_else(|| db().get(card).is_golden(), |m| m.golden);
        let times = if s.choose_one { 1 } else { mult };
        for _ in 0..times {
            let me = Me {
                side,
                eid,
                card,
                golden,
                opt,
                pos: 0,
                data: 0,
            };
            let prev_src = self.src.replace(card);
            bc(self, me, target);
            self.src = prev_src;
            if s.choose_one {
                continue;
            }
            self.bump_player_stat(side, |st| {
                st.battlecries_triggered += 1;
                st.battlecries_turn += 1;
            });
            self.emit(Event::BattlecryTriggered { side, eid });
        }
        self.resolve_deaths();
    }

    /// Trigger all Battlecries from an effect. Attached targeted sources automatically
    /// select their own legal targets. Playing from hand uses the explicit source queue.
    pub fn trigger_battlecry(&mut self, side: Side, eid: EntityId, target: EntityId, opt: u8) {
        self.with_deferred_triples(|ctx| ctx.trigger_battlecry_inner(side, eid, target, opt));
    }

    fn trigger_battlecry_inner(&mut self, side: Side, eid: EntityId, target: EntityId, opt: u8) {
        let Some(m) = self.minion(eid) else { return };
        let native = m.card;
        let cards: Vec<CardId> = std::iter::once(native)
            .chain(m.attached.iter().copied())
            .collect();
        let mult = self.battlecry_play_multiplier(side, eid);
        for card in cards {
            let Some(s) = script(card) else { continue };
            if s.battlecry.is_none() {
                continue;
            }
            let target = if card != native && s.battlecry_target != TargetSpec::None {
                let options = self.legal_targets(side, s.battlecry_target, eid);
                self.g.rng.pick(&options).copied().unwrap_or(0)
            } else {
                target
            };
            self.trigger_battlecry_part(side, eid, card, target, opt, mult);
        }
    }

    // ------------------------------------------------------------------ event dispatch

    /// Keep guard diagnostics even with logging disabled. Combat truncation is counted
    /// by the combat result path; a recruit context is counted only on its first cutoff.
    fn mark_truncated(&mut self) {
        if !self.truncated && !self.combat {
            self.g.truncated_recruit_contexts = self.g.truncated_recruit_contexts.saturating_add(1);
        }
        self.truncated = true;
    }

    /// Dispatch an event to listening scripts: event side first, then the other side; per
    /// side hero power, trinkets, board minions left to right (host then attached), then
    /// hand cards that listen in hand. The cross-source order is a simulator convention
    /// (the server's full order is unpublished).
    pub fn emit(&mut self, e: Event) {
        if self.depth >= self.g.cfg.max_effect_depth {
            self.mark_truncated();
            return;
        }
        if self.defer_hits && matches!(e, Event::Damaged { .. } | Event::ShieldLost { .. }) {
            self.deferred.push(e);
            return;
        }
        if self.combat {
            let c = &mut self.cstats;
            let counter = match &e {
                Event::Attacks { side, .. } => Some(&mut c[*side].attacks),
                Event::ShieldLost { side, .. } => Some(&mut c[*side].shields_lost),
                Event::Died { side, .. } => Some(&mut c[*side].deaths),
                Event::DeathrattleTriggered { side, .. } => Some(&mut c[*side].deathrattles),
                Event::Summoned { side, .. } => Some(&mut c[*side].summons),
                Event::Reborn { side, .. } => Some(&mut c[*side].reborns),
                _ => None,
            };
            if let Some(x) = counter {
                *x = x.saturating_add(1);
            }
        }
        let reg = crate::script::registry();
        if reg.any_listens & e.kind() == 0 {
            // No card in the whole content set listens to this event kind.
            return;
        }
        self.guard(crate::guard::Site::Event, e.kind());
        self.depth += 1;
        self.next_seq = self.next_seq.wrapping_add(1);
        let seq = self.next_seq;
        let outer_seq = self.emit_seq;
        let bit = e.kind();
        let first = e.side();
        let mut listeners = Listeners::new();
        for s in [first, 1 - first] {
            listeners.clear();
            self.collect_listeners(reg, s, bit, &mut listeners);
            for &(eid, card, golden, where_) in listeners.iter() {
                if self.timewarp_suspended(s)
                    && (where_ == 1 || where_ == 2 || db().get(card).kind == CardKind::HeroPower)
                {
                    continue;
                }
                // Board listeners must still be on board.
                if where_ == 1 && !self.on_board(eid) {
                    continue;
                }
                let Some(sc) = script(card) else { continue };
                // A cast and a minion play are announced twice: "whenever" listeners hear
                // the first announcement, all others the second.
                let phase = if bit == crate::event::ev::SPELL_CAST {
                    self.spell_phase
                } else if bit & (crate::event::ev::PLAYED | crate::event::ev::CARD_PLAYED) != 0 {
                    self.play_phase
                } else {
                    0
                };
                if phase != 0 && crate::content::acts_whenever(bit, card) != (phase == 1) {
                    continue;
                }
                // where 3 = persistent player effect: fires once per stack.
                let (fx, times) = if where_ == 3 {
                    let stacks = self
                        .player_ref(s)
                        .and_then(|p| p.effects.iter().find(|x| x.card == card))
                        .map(|x| x.stacks)
                        .unwrap_or(0);
                    (sc.persistent_event, stacks.max(0) as u32)
                } else {
                    (sc.on_event, 1)
                };
                let Some(fx) = fx else { continue };
                let me = Me {
                    side: s,
                    eid,
                    card,
                    golden,
                    opt: 0,
                    pos: 0,
                    data: 0,
                };
                for _ in 0..times {
                    let prev_src = self.src.replace(me.card);
                    self.emit_seq = seq;
                    fx(self, me, &e);
                    self.src = prev_src;
                }
            }
        }
        self.emit_seq = outer_seq;
        self.depth -= 1;
    }

    /// Append the listeners of `side` for event kind `bit` to `out` (see `Listener`). This
    /// runs for every event, so it only touches the registry's flat per-card tables and skips
    /// whole zones no card of the content set listens from.
    fn collect_listeners(
        &self,
        reg: &'static crate::script::Registry,
        side: Side,
        bit: u32,
        out: &mut Listeners,
    ) {
        let p = self.player_ref(side);
        if let Some(p) = p {
            if reg.any_persistent_listens & bit != 0 {
                for x in &p.effects {
                    if reg.persistent_listens(x.card) & bit != 0 {
                        out.push((0, x.card, false, 3));
                    }
                }
            }
            if reg.any_player_listens & bit != 0 {
                for h in &p.powers {
                    if reg.listens(h.card) & bit != 0 {
                        out.push((0, h.card, false, 0));
                    }
                }
                for &t in &p.trinkets {
                    if reg.listens(t) & bit != 0 {
                        out.push((0, t, false, 0));
                    }
                }
                for &t in &p.rewards {
                    if reg.listens(t) & bit != 0 {
                        out.push((0, t, false, 0));
                    }
                }
                for &t in &p.secrets {
                    if reg.listens(t) & bit != 0 {
                        out.push((0, t, false, 0));
                    }
                }
                for q in &p.quests {
                    if reg.listens(q.card) & bit != 0 {
                        out.push((0, q.card, false, 0));
                    }
                }
            }
        }
        for m in &self.boards[side] {
            if reg.listens(m.card) & bit != 0 {
                out.push((m.eid, m.card, m.golden, 1));
            }
            for &a in &m.attached {
                if reg.listens(a) & bit != 0 {
                    out.push((m.eid, a, db().get(a).is_golden(), 1));
                }
            }
        }
        if reg.any_hand_listens & bit != 0
            && let Some(p) = p
        {
            for h in &p.hand {
                if let Item::Minion(m) = &h.item
                    && h.locked == 0
                    && reg.hand_listens(m.card) & bit != 0
                {
                    out.push((m.eid, m.card, m.golden, 2));
                }
            }
        }
    }

    /// Run a hook on every minion of `side` (host + attached) left to right.
    /// Run a hook on one board minion (its card, then attached cards).
    pub fn run_minion_hook(
        &mut self,
        side: Side,
        eid: EntityId,
        pick: fn(&Script) -> Option<crate::script::Fx>,
        times: u32,
        label: &str,
    ) {
        let Some(m) = self.minion(eid) else { return };
        let golden = m.golden;
        let cards: Vec<(CardId, bool, i32)> = std::iter::once((m.card, golden, 0))
            .chain(
                m.attached
                    .iter()
                    .enumerate()
                    .map(|(i, a)| (*a, db().get(*a).is_golden(), m.attachment_data_at(i))),
            )
            .collect();
        for (card, g, data) in cards {
            let Some(fx) = script(card).and_then(pick) else {
                continue;
            };
            for _ in 0..times {
                if !self.on_board(eid) {
                    return;
                }
                let me = Me {
                    side,
                    eid,
                    card,
                    golden: g,
                    opt: 0,
                    pos: 0,
                    data,
                };
                let who = self.who(side);
                let name = &db().get(card).name;
                self.g.log(|| format!("{who} {label}: {name}"));
                let prev_src = self.src.replace(me.card);
                fx(self, me);
                self.src = prev_src;
            }
        }
    }

    pub fn run_board_hook(
        &mut self,
        side: Side,
        pick: fn(&Script) -> Option<crate::script::Fx>,
        times: u32,
        label: &str,
    ) {
        let snapshot = self.board_hook_snapshot(side);
        self.run_board_hook_on(side, snapshot, pick, times, label);
    }

    /// The cards a board hook of `side` would run for now: each minion and its attachments,
    /// left to right.
    pub fn board_hook_snapshot(&self, side: Side) -> Vec<(EntityId, CardId, bool, i32)> {
        self.boards[side]
            .iter()
            .flat_map(|m| {
                std::iter::once((m.eid, m.card, m.golden, 0)).chain(
                    m.attached.iter().enumerate().map(move |(i, a)| {
                        (m.eid, *a, db().get(*a).is_golden(), m.attachment_data_at(i))
                    }),
                )
            })
            .collect()
    }

    /// `run_board_hook` for a snapshot taken earlier (`board_hook_snapshot`): a minion that
    /// came onto the board since is not in it.
    pub fn run_board_hook_on(
        &mut self,
        side: Side,
        snapshot: Vec<(EntityId, CardId, bool, i32)>,
        pick: fn(&Script) -> Option<crate::script::Fx>,
        times: u32,
        label: &str,
    ) {
        for (eid, card, golden, data) in snapshot {
            let Some(s) = script(card) else { continue };
            let Some(fx) = pick(s) else { continue };
            for _ in 0..times {
                if !self.on_board(eid) {
                    break;
                }
                let me = Me {
                    side,
                    eid,
                    card,
                    golden,
                    opt: 0,
                    pos: 0,
                    data,
                };
                let who = self.who(side);
                let name = &db().get(card).name;
                self.g.log(|| format!("{who} {label}: {name}"));
                let prev_src = self.src.replace(me.card);
                fx(self, me);
                self.src = prev_src;
            }
        }
    }

    /// Run a hook on the side owner's hero power and trinkets.
    pub fn run_player_hook(
        &mut self,
        side: Side,
        pick: fn(&Script) -> Option<crate::script::Fx>,
        label: &str,
    ) {
        self.run_player_hook_times(side, pick, 1, label);
    }

    /// Run a player-level hook `times` times per card (end-of-turn multipliers reach hero powers,
    /// trinkets, Rewards and effects too). Quests run once: their progress checks are not effects
    /// to repeat.
    pub fn run_player_hook_times(
        &mut self,
        side: Side,
        pick: fn(&Script) -> Option<crate::script::Fx>,
        times: u32,
        label: &str,
    ) {
        let Some(p) = self.player_ref(side) else {
            return;
        };
        let quests: Vec<CardId> = p.quests.iter().map(|q| q.card).collect();
        // Trinkets, then Rewards, then Hero Powers.
        let mut cards: Vec<CardId> = p.trinkets.to_vec();
        cards.extend(p.rewards.iter().copied());
        cards.extend(p.secrets.iter().copied());
        cards.extend(p.quests.iter().map(|q| q.card));
        cards.extend(p.power_cards());
        // Persistent effects run once per stack.
        for x in &p.effects {
            for _ in 0..x.stacks.max(0) {
                cards.push(x.card);
            }
        }
        for card in cards {
            let Some(s) = script(card) else { continue };
            let Some(fx) = pick(s) else { continue };
            let me = Me {
                side,
                eid: 0,
                card,
                golden: false,
                opt: 0,
                pos: 0,
                data: 0,
            };
            let who = self.who(side);
            let name = &db().get(card).name;
            self.g.log(|| format!("{who} {label}: {name}"));
            let n = if quests.contains(&card) { 1 } else { times };
            for _ in 0..n {
                let prev_src = self.src.replace(me.card);
                fx(self, me);
                self.src = prev_src;
            }
        }
    }
}

/// (Tentacle of C'Thun, Mishmash, Amalgam): the cards of the stat watchers, resolved once.
fn stat_watcher_ids() -> (CardId, CardId, CardId) {
    static IDS: std::sync::OnceLock<(CardId, CardId, CardId)> = std::sync::OnceLock::new();
    *IDS.get_or_init(|| {
        (
            db().id("TB_BaconShop_HERO_29_Buddy"),
            db().id("TB_BaconShop_HERO_33_Buddy"),
            db().id("TB_BaconShop_HP_033t"),
        )
    })
}

/// Card ids used by `apply_bonuses_to`, resolved once.
struct BonusIds {
    beetle: CardId,
    extortionist: CardId,
    malorne: CardId,
    golem: CardId,
    frostling: CardId,
    knight: CardId,
    paradox_t6: CardId,
    boom_monster: CardId,
    /// Dark Gifts scaling with a counter: (card, atk per, hp per, counter kind).
    gifts: [(CardId, i32, i32, u8); 7],
    hammer: CardId,
    hammer_g: CardId,
    urn: CardId,
}

fn bonus_ids() -> &'static BonusIds {
    static IDS: std::sync::OnceLock<BonusIds> = std::sync::OnceLock::new();
    IDS.get_or_init(|| {
        let d = db();
        BonusIds {
            beetle: d.id("BG28_603t"),
            extortionist: d.id("BG36_524"),
            malorne: d.id("BG32_HERO_001_Buddy"),
            golem: d.id("BG35_342"),
            frostling: d.id("BG26_537"),
            knight: d.id("BG25_008"),
            paradox_t6: d.id("BG36_360t6"),
            boom_monster: d.id("BG31_176"),
            gifts: [
                (d.id("BG36_MidGameEffect_000t28"), 3, 3, 0),
                (d.id("BG36_MidGameEffect_000t28t"), 2, 2, 0),
                (d.id("BG36_MidGameEffect_000t29"), 2, 2, 1),
                (d.id("BG36_MidGameEffect_000t29t"), 1, 1, 1),
                (d.id("BG36_MidGameEffect_000t30"), 3, 3, 2),
                (d.id("BG36_MidGameEffect_000t30t"), 2, 2, 2),
                (d.id("BG36_MidGameEffect_000t88"), 3, 3, 3),
            ],
            hammer: d.id("BG36_MagicItem_403"),
            hammer_g: d.id("BG36_MagicItem_403t"),
            urn: d.id("BG30_MagicItem_989t"),
        }
    })
}

/// Re-sync a minion's player-level "this game" bonuses (idempotent).
pub fn apply_bonuses(cfg: &RulesConfig, p: &Player, m: &mut Minion) {
    apply_bonuses_to(
        cfg.volumizer_attached_grow,
        &p.stats,
        &p.trinkets,
        p.health,
        false,
        m,
    );
}

/// `apply_bonuses` for a minion in `p`'s Tavern: the "this game" bonuses without the trinket
/// auras that only reach your own minions (Hammer of Twilight).
pub fn apply_tavern_bonuses(cfg: &RulesConfig, p: &Player, m: &mut Minion) {
    apply_bonuses_to(
        cfg.volumizer_attached_grow,
        &p.stats,
        &p.trinkets,
        p.health,
        true,
        m,
    );
}

/// `apply_bonuses` from the parts of a player that matter (lets callers hold other fields of
/// the same player mutably). `attached_grow` is `RulesConfig::volumizer_attached_grow`;
/// `tavern` is whether the minion is in the player's Tavern.
pub fn apply_bonuses_to(
    attached_grow: bool,
    st: &crate::state::PlayerStats,
    trinkets: &[CardId],
    health: i32,
    tavern: bool,
    m: &mut Minion,
) {
    let (atk, hp) = bonus_of(attached_grow, st, trinkets, health, tavern, m);
    let (da, dh) = (
        atk.saturating_sub(m.pbonus_atk),
        hp.saturating_sub(m.pbonus_hp),
    );
    if da != 0 || dh != 0 {
        m.add_stats(da, dh);
        m.pbonus_atk = atk;
        m.pbonus_hp = hp;
    }
}

/// The player-level bonus a minion carries (Attack, Health): what `apply_bonuses_to` keeps
/// it at.
/// Hammer of Twilight's Attack on a player's minions in play and in hand (`bonus_of`).
pub fn hammer_attack(st: &crate::state::PlayerStats, trinkets: &[CardId]) -> i32 {
    let ids = bonus_ids();
    let k = 1 + st.cards_discarded;
    trinkets
        .iter()
        .map(|t| t.base())
        .map(|t| {
            if t == ids.hammer {
                k
            } else if t == ids.hammer_g {
                2 * k
            } else {
                0
            }
        })
        .fold(0i32, |s, x| s.saturating_add(x))
}

fn bonus_of(
    attached_grow: bool,
    st: &crate::state::PlayerStats,
    trinkets: &[CardId],
    health: i32,
    tavern: bool,
    m: &Minion,
) -> (i32, i32) {
    if script(m.card).is_some_and(|s| s.has_flag(flag::NO_STAT_GAIN)) {
        return (0, 0);
    }
    let ids = bonus_ids();
    let mut atk = 0i64;
    let mut hp = 0i64;
    if m.races.intersects(Races::UNDEAD) {
        atk += st.undead_atk as i64;
        hp += st.undead_hp as i64;
    }
    let normal = db().normal_of(m.card);
    let g: i64 = if m.golden { 2 } else { 1 };
    if normal == crate::cardid!("BG36_372") && health <= 15 {
        atk += 30 * g;
        hp += 30 * g;
    }
    if normal == ids.beetle {
        atk += st.beetle_atk as i64;
        hp += st.beetle_hp as i64;
    } else if normal == ids.extortionist {
        // Maritime Extortionist: +7/+7 per Golden minion played this game.
        atk += 7 * g * (st.golden_played as i64);
        hp += 7 * g * (st.golden_played as i64);
    } else if normal == ids.malorne {
        // Malorne: +1/+1 (+2/+2) for every 3 Gold spent this game.
        let k = g * ((st.gold_spent_total as i64) / 3);
        atk += k;
        hp += k;
    } else if normal == ids.golem {
        // Falling Sky Golem: +4/+2 per Deathrattle triggered this game.
        atk += 4 * g * (st.deathrattles_triggered as i64);
        hp += 2 * g * (st.deathrattles_triggered as i64);
    } else if normal == ids.frostling {
        // Flourishing Frostling: +2/+1 per Elemental played this game.
        atk += 2 * g * (st.elementals_played as i64);
        hp += g * (st.elementals_played as i64);
    } else if normal == ids.knight {
        atk += 4 * g * (st.eternal_knight_deaths as i64) + 4 * (st.knight_bonus as i64);
        hp += 2 * g * (st.eternal_knight_deaths as i64) + 2 * (st.knight_bonus as i64);
    } else if normal == ids.paradox_t6 {
        atk += 3 * g * (st.battlecries_triggered as i64);
        hp += 3 * g * (st.battlecries_triggered as i64);
    } else if normal == ids.boom_monster {
        // Dr. Boom's Monster: +2/+2 (+4/+4) for each time you've Magnetized this game.
        atk += 2 * g * (st.magnetized as i64);
        hp += 2 * g * (st.magnetized as i64);
    }
    // Volumizers: the minion itself and, with `volumizer_attached_grow`, every Volumizer
    // magnetized to it (otherwise `magnetize_onto` adds the bonus of that moment for good).
    if (st.volumizer_atk as i64) != 0 || (st.volumizer_hp as i64) != 0 {
        let mut vols = i64::from(crate::content::is_volumizer(m.card));
        if attached_grow {
            vols += m
                .attached
                .iter()
                .filter(|a| crate::content::is_volumizer(**a))
                .count() as i64;
        }
        atk += vols * (st.volumizer_atk as i64);
        hp += vols * (st.volumizer_hp as i64);
    }
    for a in &m.attached {
        if let Some(&(_, pa, ph, kind)) = ids.gifts.iter().find(|x| x.0 == *a) {
            let n = match kind {
                0 => st.battlecries_triggered as i64,
                1 => st.deathrattles_triggered as i64,
                2 => st.tavern_spells_cast as i64,
                _ => st.cards_discarded as i64,
            };
            atk += pa as i64 * n;
            hp += ph as i64 * n;
        }
    }
    // Trinket auras: Hammer of Twilight (improved by each discarded card) on your minions in play
    // and in hand, not on those in the Tavern; Artisanal Urn (Undead +20 Attack) wherever they are.
    if !trinkets.is_empty() {
        let k = 1 + (st.cards_discarded as i64);
        for t in trinkets.iter().map(|t| t.base()) {
            if t == ids.hammer && !tavern {
                atk += k;
            } else if t == ids.hammer_g && !tavern {
                atk += 2 * k;
                hp += k;
            } else if t == ids.urn && m.races.intersects(Races::UNDEAD) {
                atk += 20;
            }
        }
    }
    (
        atk.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
        hp.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
    )
}

impl Game {
    /// For a state built from what a game client shows: the stats of `p`'s minions already
    /// include their player-level bonuses. This records the bonuses as applied without adding
    /// them, so that the next re-sync adds only what changes (a state imported with zero records
    /// would gain every "this game" bonus a second time).
    pub fn adopt_bonuses(&mut self, p: PlayerId) {
        let grow = self.cfg.volumizer_attached_grow;
        let Player {
            stats,
            trinkets,
            board,
            hand,
            shop,
            health,
            ..
        } = &mut self.players[p];
        let minions = board
            .iter_mut()
            .map(|m| (m, false))
            .chain(
                hand.iter_mut()
                    .filter_map(|h| h.item.as_minion_mut().map(|m| (m, false))),
            )
            .chain(
                shop.iter_mut()
                    .filter_map(|s| s.item.as_minion_mut().map(|m| (m, true))),
            );
        for (m, tavern) in minions {
            (m.pbonus_atk, m.pbonus_hp) = bonus_of(grow, stats, trinkets, *health, tavern, m);
        }
    }

    /// The player whose warband `p` fought last: the last opponent, or the eliminated player whose
    /// final warband was the ghost -- a ghost is "your last opponent" too (Doppelganger's Locket,
    /// Time Thief, Tess still work after a ghost fight).
    pub fn last_fought(&self, p: PlayerId) -> Option<PlayerId> {
        self.players[p].last_opponent.or(if self.turn > 1 {
            self.ghost_player
        } else {
            None
        })
    }
}

/// Whether cards generated by `source` wait for space when the hand is full instead of being lost:
/// trinkets and Quest Rewards (official trinket rules 2026-04-06), and Xavius's Hero Power (client
/// tag 4701).
pub fn hand_protected(source: CardId) -> bool {
    let d = db().get(source);
    matches!(
        d.kind,
        CardKind::TrinketLesser | CardKind::TrinketGreater | CardKind::QuestReward
    ) || matches!(
        d.id.as_str(),
        "BG36_HERO_105p" | "BG24_HERO_100_Buddyt" | "BG24_HERO_100_Buddy_Gt"
    )
}
