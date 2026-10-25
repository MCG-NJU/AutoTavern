//! Card behaviour scripts.
//!
//! A `Script` is a table of optional hook functions keyed by card id. The same script is
//! registered for a card's normal and golden ids; hooks read `me.golden` for golden numbers
//! (golden text is taken from the card data per card, never by blind doubling).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::cards::{CardId, db};
use crate::engine::Ctx;
use crate::event::Event;
use crate::types::{EntityId, Races, Side};

/// Identity of the thing whose hook is running.
#[derive(Clone, Copy, Debug)]
pub struct Me {
    pub side: Side,
    /// Minion entity carrying the script (0 for hero powers, trinkets, spells).
    pub eid: EntityId,
    /// Card whose script this is (may be an attached card, not the host minion).
    pub card: CardId,
    pub golden: bool,
    /// Choose One mode: 0, 1, or 2 = both.
    pub opt: u8,
    /// Board position (deathrattles: where the minion died).
    pub pos: usize,
    /// Numeric parameter: card data (Triple Reward tier...); for deathrattles the eid of
    /// the minion that killed this one (0 if none).
    pub data: i32,
}

impl Me {
    /// 1 for normal, 2 for golden: for effects whose golden text doubles the number.
    pub fn g(&self) -> i32 {
        if self.golden { 2 } else { 1 }
    }
    pub fn opt_a(&self) -> bool {
        self.opt == 0 || self.opt == 2
    }
    pub fn opt_b(&self) -> bool {
        self.opt == 1 || self.opt == 2
    }
}

/// Which entities a targeted effect may choose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetSpec {
    None,
    /// Any friendly board minion.
    Friendly,
    /// Friendly board minion other than the source.
    FriendlyOther,
    /// Friendly board minion with a type.
    FriendlyRace(Races),
    FriendlyRaceOther(Races),
    /// Friendly board minion of tier <= n, non-golden.
    FriendlyNonGoldenMaxTier(u8),
    /// Non-Golden Pirate from Tier 4 or below, excluding the source.
    FriendlyEligiblePirate,
    /// A minion in the tavern.
    Tavern,
    /// A friendly minion whose Rally an effect can trigger (Sky-hatch Runaway).
    FriendlyRallyTriggerable,
    /// A non-Golden Tavern minion or a Tavern spell (Kidnap Sack).
    TavernCard,
    /// Any card in the Tavern, minion or spell ("a card in the Tavern").
    TavernAny,
    /// A friendly minion or any card in the Tavern (Arcane Alteration).
    FriendlyOrTavernAny,
    /// Friendly or tavern minion.
    FriendlyOrTavern,
    /// A friendly or Tavern minion that is not Golden and has a Golden form (Timewarped
    /// Goldenizer).
    AnyNonGolden,
    /// A card in the owner's hand (discard costs).
    Hand,
    /// Friendly board minion that has the given hook (Battlecry / Rally / Deathrattle).
    FriendlyHas(Hook),
    /// Friendly or tavern minion of tier <= n (Duplicating Lens).
    AnyMaxTier(u8),
    /// Friendly or tavern minion other than the source.
    AnyOther,
    /// Friendly or tavern minion with a type (and not the source).
    AnyRace(Races),
    AnyRaceOther(Races),
    /// Friendly or tavern minion that has a type (Lost Staff of Hamuul).
    AnyTyped,
    /// Friendly or tavern minion without Divine Shield (Boon of Light: the client offers none that
    /// has one).
    AnyNoDivineShield,
    /// A Buddy, friendly or in the Tavern, that is not Golden and has a Golden form, other
    /// than the source (Talent Scout: "Make a Buddy Golden").
    BuddyNonGolden,
}

impl TargetSpec {
    /// When the card text doesn't limit the target to friendly minions ("a minion", "another
    /// minion", "a Beast"), Tavern minions are legal targets too.
    pub fn widened(self) -> TargetSpec {
        match self {
            TargetSpec::Friendly => TargetSpec::FriendlyOrTavern,
            TargetSpec::FriendlyOther => TargetSpec::AnyOther,
            TargetSpec::FriendlyRace(r) => TargetSpec::AnyRace(r),
            TargetSpec::FriendlyRaceOther(r) => TargetSpec::AnyRaceOther(r),
            t => t,
        }
    }
}

/// Apply the target-widening rule (`TargetSpec::widened`) to a script from its card text.
fn widen_targets(mut s: Script, text: &str) -> Script {
    if text.to_lowercase().contains("friendly") {
        return s;
    }
    s.battlecry_target = s.battlecry_target.widened();
    s.spell_target = s.spell_target.widened();
    s.hero_power_target = s.hero_power_target.widened();
    if let Some((cost, spec, fx)) = s.activate {
        s.activate = Some((cost, spec.widened(), fx));
    }
    s
}

/// A triggerable ability kind (for "trigger a friendly minion's...").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hook {
    Battlecry,
    Rally,
    Deathrattle,
}

impl Hook {
    pub fn in_script(self, s: &Script) -> bool {
        match self {
            // A Choose One card has no Battlecry to trigger.
            Hook::Battlecry => s.battlecry.is_some() && !s.choose_one,
            Hook::Rally => s.rally.is_some(),
            Hook::Deathrattle => s.deathrattle.is_some(),
        }
    }
}

/// Rallies that effects can't trigger (hsdata 253216 tag
/// BACON_RALLY_CANT_BE_ARTIFICIALLY_TRIGGERED, normal forms).
const RALLY_UNTRIGGERABLE: [&str; 13] = [
    "BG25_016",        // Sin'dorei Straight Shot
    "BG27_013",        // Transmuted Bramblewitch
    "BG27_017",        // Obsidian Ravager
    "BG27_822",        // Niuzao
    "BG31_928",        // Arcane Cannoneer
    "BG31_HERO_802pt", // Colossus
    "BG33_113",        // Campfire Shadow
    "BG34_140",        // Expert Aviator
    "BG34_604",        // Heroic Underdog
    "BG34_631",        // Whelp Watcher
    "BG34_Giant_110",  // Timewarped Sea Glass
    "BG35_700",        // Ship Jumper
    "BG36_333",        // Jailbird Juggernaut
];

pub fn rally_untriggerable(card: CardId) -> bool {
    RALLY_UNTRIGGERABLE.contains(&db().get(db().normal_of(card)).id.as_str())
}

/// A Buddy that can still be made Golden (Talent Scout's targets).
pub fn gildable_buddy(m: &crate::state::Minion) -> bool {
    let d = db().get(m.card);
    !m.golden && d.golden.is_some() && d.id.contains("_Buddy")
}

/// Whether a minion (host card or any attached card) has a hook.
pub fn minion_has(m: &crate::state::Minion, h: Hook) -> bool {
    std::iter::once(m.card)
        .chain(m.attached.iter().copied())
        .chain(
            m.gained_dr
                .iter()
                .copied()
                .filter(|_| matches!(h, Hook::Deathrattle)),
        )
        .any(|c| script(c).is_some_and(|s| h.in_script(s)))
}

pub type Fx = fn(&mut Ctx, Me);
pub type FxT = fn(&mut Ctx, Me, EntityId);
pub type FxE = fn(&mut Ctx, Me, &Event);
/// `Script::discover_pick`: (ctx, side, source card, pick) -> the pick, unless consumed.
pub type FxPick = fn(&mut Ctx, Side, CardId, crate::state::Item) -> Option<crate::state::Item>;

/// A card whose text is being displayed (see `Script::shown`).
pub struct Shown<'a> {
    pub side: Side,
    /// The card shown (the golden card for a golden instance).
    pub card: CardId,
    pub golden: bool,
    /// The instance, for a minion on the board, in the hand, in the Tavern or in a choice.
    pub minion: Option<&'a crate::state::Minion>,
    /// Spell data of a hand or Tavern spell (0 otherwise).
    pub data: i32,
}

impl Shown<'_> {
    /// 1 for normal, 2 for golden.
    pub fn g(&self) -> i32 {
        if self.golden { 2 } else { 1 }
    }
    /// The instance's script counter (0 without an instance).
    pub fn counter(&self) -> i32 {
        self.minion.map_or(0, |m| m.counter)
    }
}

/// The numbers of the card's English text as they stand now, in the order they appear in
/// the text (every number, the fixed ones too).
pub type FxShown = fn(&Ctx, &Shown) -> Vec<i32>;

#[derive(Clone, Copy)]
pub struct Script {
    // --- minion hooks ---
    pub battlecry: Option<FxT>,
    pub battlecry_target: TargetSpec,
    /// Card has "Choose One": the player picks a mode before targeting.
    pub choose_one: bool,
    pub deathrattle: Option<Fx>,
    pub start_of_combat: Option<Fx>,
    /// Rally: fires when this minion attacks; gets the defender.
    pub rally: Option<FxT>,
    /// Avenge (threshold normal, threshold golden, effect).
    pub avenge: Option<(u16, u16, Fx)>,
    pub on_sell: Option<Fx>,
    /// Start of Combat while this card is in hand (`me.eid` = hand id).
    pub hand_start_of_combat: Option<Fx>,
    pub start_of_turn: Option<Fx>,
    pub end_of_turn: Option<Fx>,
    /// Activate (gold cost, target, effect).
    pub activate: Option<(i32, TargetSpec, FxT)>,
    /// Behaviour of this card's persistent player effect (`Player::effects`), with its mask.
    pub persistent_event: Option<FxE>,
    pub persistent_listens: u32,
    /// Add extra tavern offers when the tavern is filled (hero power / trinket / board).
    pub on_fill_shop: Option<Fx>,
    /// Generic event listener + mask of `event::ev` bits.
    pub on_event: Option<FxE>,
    pub listens: u32,
    /// Listener also fires while the card is in hand.
    pub listens_in_hand: bool,
    /// Called when attached to / gained by an owner (Dark Gifts, magnetize, trinket buy,
    /// hero selection).
    pub on_attach: Option<Fx>,
    // --- spells / hero powers ---
    pub spell: Option<FxT>,
    pub spell_target: TargetSpec,
    pub hero_power: Option<FxT>,
    pub hero_power_target: TargetSpec,
    /// Hero power uses per turn (0 = passive).
    pub hero_power_uses: u32,
    /// Scheduled effect (see `Player::scheduled`).
    pub scheduled: Option<fn(&mut Ctx, Me, &crate::state::Scheduled)>,
    // --- static properties ---
    pub flags: u32,
    /// Keywords every instance starts with (e.g. Cleave, which has no data tag).
    pub innate: crate::types::Kw,
    /// Adjust a freshly created instance (e.g. multi-hit Divine Shield).
    pub on_create: Option<fn(&mut crate::state::Minion)>,
    /// Second-target step (see `Ctx::ask_followup`): `me.eid` = first target, `me.data` = data.
    pub followup: Option<FxT>,
    /// "After this attacks" (after the attack's damage; runs even if it is dying).
    pub after_attack: Option<Fx>,
    /// After this board minion's stats changed (Scarlet Survivor).
    pub on_stats: Option<Fx>,
    /// Carry state into the golden made from `parts` (e.g. counter progress).
    pub on_merge: Option<fn(&mut crate::state::Minion, &[crate::state::Minion])>,
    /// The pick of a Discover this card offered, before it goes to the hand:
    /// `(ctx, side, source card, pick)`. Returning None consumes the pick ("choose a minion
    /// to transform into", a Pirate boarding Nellie's Ship): it never enters the hand, so a
    /// full hand doesn't matter and no "after you Discover" effect fires.
    pub discover_pick: Option<FxPick>,
    /// Extra eligibility for being offered to a player (trinkets); None = always.
    pub offer_ok: Option<fn(&crate::engine::Game, crate::types::PlayerId) -> bool>,
    /// Display only: the current values of the numbers in the card's text --
    /// improved amounts, "(N left!)" counters, spell bonuses. Never used by game logic.
    /// Cards whose numbers never change don't need it. For an instance in its initial
    /// state it must return exactly the printed numbers (`tests/shown.rs` checks that).
    pub shown: Option<FxShown>,
}

pub mod flag {
    /// Deathrattles trigger an extra time (Titus Rivendare; golden: two extra).
    pub const EXTRA_DEATHRATTLE: u32 = 1 << 0;
    /// Battlecries trigger twice (Brann; golden: three times).
    pub const EXTRA_BATTLECRY: u32 = 1 << 1;
    /// End of turn effects trigger twice (Drakkari Enchanter).
    pub const EXTRA_END_OF_TURN: u32 = 1 << 2;
    /// Has Taunt while it has Divine Shield (Relentless Deflector).
    pub const TAUNT_WHILE_SHIELDED: u32 = 1 << 3;
    /// Can't gain stats (Fishbait).
    pub const NO_STAT_GAIN: u32 = 1 << 4;
    /// A spell that cannot be played from hand (Lockbox).
    pub const UNPLAYABLE: u32 = 1 << 5;
    /// Hero power that is passive (no button).
    pub const PASSIVE: u32 = 1 << 6;
    /// `start_of_turn` also runs while this card is in hand ("Improves each turn!").
    pub const TURN_IN_HAND: u32 = 1 << 7;
    /// Hero Power discounts ("your next Hero Power costs (1) less") end when it is used.
    pub const HP_DISCOUNT_RESETS: u32 = 1 << 8;
    /// "Casts When Bought": the spell is cast on purchase instead of going to hand.
    pub const CAST_ON_BUY: u32 = 1 << 9;
    /// Costs Health to buy instead of Gold (Hasty Excavation).
    pub const HEALTH_COST: u32 = 1 << 10;
    /// A "Start of Combat:" Tavern spell effect for the next combat only (player effect,
    /// runs once per stack, then removed).
    pub const NEXT_COMBAT: u32 = 1 << 11;
    /// Its effect is a player effect: can't be cast again while that effect is pending
    /// (next-combat spells, Boon of Beetles).
    pub const ONE_PENDING: u32 = 1 << 12;
    /// The minion's `counter` holds the uses of a "... per turn" effect made this turn
    /// (cleared at the start of the turn by its script, and when it turns Golden).
    pub const TURN_USES: u32 = 1 << 13;
    /// "... and improve this": the minion's `counter` is how far its effect has grown, in steps of
    /// the normal card's base amount. The effect is base x (1 + counter) on the normal card and
    /// base x (2 + counter) on the Golden one, whose own improvements count two steps each. Kept in
    /// the normal card's steps so that a triple can add up the growth of its parts exactly
    /// (`Ctx::merge_golden`).
    pub const IMPROVES: u32 = 1 << 14;
}

impl Script {
    pub const EMPTY: Script = Script {
        battlecry: None,
        battlecry_target: TargetSpec::None,
        choose_one: false,
        deathrattle: None,
        start_of_combat: None,
        rally: None,
        avenge: None,
        on_sell: None,
        hand_start_of_combat: None,
        start_of_turn: None,
        end_of_turn: None,
        activate: None,
        persistent_event: None,
        persistent_listens: 0,
        on_fill_shop: None,
        on_event: None,
        listens: 0,
        listens_in_hand: false,
        on_attach: None,
        spell: None,
        spell_target: TargetSpec::None,
        hero_power: None,
        hero_power_target: TargetSpec::None,
        hero_power_uses: 1,
        scheduled: None,
        flags: 0,
        innate: crate::types::Kw::NONE,
        on_create: None,
        on_merge: None,
        discover_pick: None,
        on_stats: None,
        after_attack: None,
        followup: None,
        offer_ok: None,
        shown: None,
    };

    pub fn has_flag(&self, f: u32) -> bool {
        self.flags & f != 0
    }
}

pub struct Registry {
    by_card: Vec<Option<&'static Script>>,
    /// `Script::listens` per card (0 without a script): one load per card on event dispatch.
    listens: Vec<u32>,
    /// `listens` for cards that also listen while in hand (`listens_in_hand`), else 0.
    hand_listens: Vec<u32>,
    /// `Script::persistent_listens` per card.
    persistent_listens: Vec<u32>,
    /// Union of every script's `listens` / `persistent_listens`: events outside it have no
    /// listener anywhere and are not dispatched at all.
    pub any_listens: u32,
    /// Union of `hand_listens`: outside it, hands need no scan.
    pub any_hand_listens: u32,
    /// Union of `persistent_listens`.
    pub any_persistent_listens: u32,
    /// Union of `listens` over non-minion cards (Hero Powers, trinkets, Rewards, Quests,
    /// Secrets): outside it, the player-level cards need no scan.
    pub any_player_listens: u32,
}

impl Registry {
    #[inline]
    pub fn listens(&self, c: CardId) -> u32 {
        self.listens[c.idx()]
    }
    #[inline]
    pub fn hand_listens(&self, c: CardId) -> u32 {
        self.hand_listens[c.idx()]
    }
    #[inline]
    pub fn persistent_listens(&self, c: CardId) -> u32 {
        self.persistent_listens[c.idx()]
    }
    #[inline]
    pub fn script(&self, c: CardId) -> Option<&'static Script> {
        self.by_card.get(c.idx()).copied().flatten()
    }
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| {
        let d = db();
        let mut by_card: Vec<Option<&'static Script>> = vec![None; d.len()];
        let mut seen: HashMap<&'static str, ()> = HashMap::new();
        for (id, s) in crate::content::all_scripts() {
            assert!(seen.insert(id, ()).is_none(), "duplicate script for {id}");
            let c = d.id(id);
            let s = widen_targets(s, &d.get(c).text);
            let leaked: &'static Script = Box::leak(Box::new(s));
            by_card[c.0 as usize] = Some(leaked);
        }
        // Golden forms resolve to their normal form's script unless they have their own, so
        // that a lookup is a single index (this runs on every event dispatch).
        for c in d.iter() {
            if let Some(n) = c.normal
                && by_card[c.cid.0 as usize].is_none()
            {
                by_card[c.cid.0 as usize] = by_card[n.0 as usize];
            }
        }
        let listens: Vec<u32> = by_card.iter().map(|s| s.map_or(0, |s| s.listens)).collect();
        let hand_listens: Vec<u32> = by_card
            .iter()
            .map(|s| s.map_or(0, |s| if s.listens_in_hand { s.listens } else { 0 }))
            .collect();
        let persistent_listens: Vec<u32> = by_card
            .iter()
            .map(|s| s.map_or(0, |s| s.persistent_listens))
            .collect();
        let any_listens = listens
            .iter()
            .chain(&persistent_listens)
            .fold(0, |a, b| a | b);
        let any_hand_listens = hand_listens.iter().fold(0, |a, b| a | b);
        let any_persistent_listens = persistent_listens.iter().fold(0, |a, b| a | b);
        let any_player_listens = d
            .iter()
            .filter(|c| c.kind != crate::cards::CardKind::Minion)
            .fold(0, |a, c| a | listens[c.cid.0 as usize]);
        Registry {
            by_card,
            listens,
            hand_listens,
            persistent_listens,
            any_listens,
            any_hand_listens,
            any_persistent_listens,
            any_player_listens,
        }
    })
}

/// Script for a card, if implemented (golden cards use their normal form's script).
#[inline]
pub fn script(c: CardId) -> Option<&'static Script> {
    registry().by_card.get(c.idx()).copied().flatten()
}

/// Whether a card's behaviour is fully supported by the engine: it has a script, or its
/// text is fully expressed by keywords (vanilla / keyword-only bodies). Cached per card:
/// lobby setup asks this for every card of the database.
pub fn implemented(c: CardId) -> bool {
    static CACHE: OnceLock<Vec<bool>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| {
        (0..db().len())
            .map(|i| implemented_uncached(CardId(i as u16)))
            .collect()
    });
    cache.get(c.idx()).copied().unwrap_or(false)
}

fn implemented_uncached(c: CardId) -> bool {
    if script(c).is_some() {
        return true;
    }
    let d = db().get(c);
    let text = d.text.trim();
    if text.is_empty() {
        return true;
    }
    // Keyword-only texts such as "Taunt Reborn" or "Divine Shield, Windfury".
    let stripped = text
        .replace("Divine Shield", "")
        .replace("Taunt", "")
        .replace("Reborn", "")
        .replace("Windfury", "")
        .replace("Venomous", "")
        .replace("Stealth", "")
        .replace("Magnetic", "")
        .replace([',', '.', ' '], "");
    stripped.is_empty()
}
