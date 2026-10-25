//! Game lifecycle: lobby setup, hero selection, turn start/end, combat rounds, pairing, elimination
//! and placement.

use std::sync::Arc;

use crate::cards::{CardId, CardKind, db};
use crate::config::{ContentMode, RulesConfig};
use crate::engine::{Game, Phase};
use crate::rng::Rng;
use crate::script::{implemented, script};
use crate::state::{DeityState, Item, Minion, Pending, Player, Pool, Scouted};
use crate::types::{EntityId, Kw, PlayerId, Races};

/// Lobby-level hero bans: a hero that is out of every lobby that has a minion type (the type is
/// client tag 2703 on the hero, 253932 data; Gallywix is out of lobbies with Elementals, an
/// official limit on his strength).
/// The opposite case, a hero that needs a type, is `CardDef::assoc`.
///
/// The first `HERO_BANS_BEFORE_SHUFFLE` entries are taken out before the hero list is shuffled,
/// the others after it (`Game::new`): taking them out late leaves the other heroes in the same
/// order and keeps the lobby's random stream.
const HERO_BANS_BEFORE_SHUFFLE: usize = 4;
const HERO_BANNED_WITH: [(&str, Races); 10] = [
    ("BG34_HERO_000", Races::ABERRATION), // Murozond, Unbounded
    ("BG34_HERO_004", Races::ABERRATION), // Morchie
    ("BG23_HERO_306", Races::ABERRATION), // Sylvanas Windrunner
    ("TB_BaconShop_HERO_45", Races::ABERRATION), // Arch-Villain Rafaam (36.6.3)
    ("TB_BaconShop_HERO_10", Races::ELEMENTAL), // Trade Prince Gallywix
    ("TB_BaconShop_HERO_91", Races::ELEMENTAL), // Zephrys, the Great
    ("TB_BaconShop_HERO_95", Races::MECH), // Greybough
    ("BG20_HERO_282", Races::DRAGON),     // Tamsin Roame
    ("TB_BaconShop_HERO_08", Races::DRAGON), // Illidan Stormrage
    ("BG20_HERO_301", Races::NAGA),       // Mutanus the Devourer
];
/// Y'Shaarj lobbies additionally ban these.
const YSHAARJ_HERO_BANS: [&str; 4] = [
    "TB_BaconShop_HERO_33",
    "TB_BaconShop_HERO_22",
    "BG25_HERO_103",
    "TB_BaconShop_HERO_93",
];
/// Whether a hero belongs in a lobby with these types: not banned there, its minion types
/// (`assoc`) and its subset (`lobby_req`) are present.
fn hero_fits(cfg: &RulesConfig, lobby: Races, yshaarj: bool, c: &crate::cards::CardDef) -> bool {
    hero_fits_from(cfg, lobby, yshaarj, c, HERO_BANNED_WITH.len())
}

/// `hero_fits` with only the first `bans` entries of `HERO_BANNED_WITH`.
fn hero_fits_from(
    cfg: &RulesConfig,
    lobby: Races,
    yshaarj: bool,
    c: &crate::cards::CardDef,
    bans: usize,
) -> bool {
    let aberrations = lobby.intersects(Races::ABERRATION);
    let id = c.id.as_str();
    !HERO_BANNED_WITH[..bans]
        .iter()
        .any(|(h, r)| *h == id && lobby.intersects(*r))
        && !(aberrations && yshaarj && YSHAARJ_HERO_BANS.contains(&id))
        && !cfg.hero_bans.iter().any(|b| b == id)
        && (!cfg.hero_type_filter || c.assoc.is_none() || c.assoc.intersects(lobby))
        && (c.lobby_req.is_none() || c.lobby_req.intersects(lobby))
}

/// Dark Paradox variants (one per lobby).
pub const PARADOX_VARIANTS: [&str; 5] = [
    "BG36_360t3",
    "BG36_360t4",
    "BG36_360t5",
    "BG36_360t6",
    "BG36_360t9",
];
pub const DEITIES: [&str; 2] = ["BGFYM_000", "BGFYM_011"];

impl Game {
    /// The heroes of the hero pool that belong in this lobby (`hero_fits`): the same set
    /// the hero offers were dealt from. Hero Power and Buddy Discovers draw from them.
    pub fn lobby_heroes(&self) -> impl Iterator<Item = &'static crate::cards::CardDef> + '_ {
        let yshaarj = self.deity.is_some_and(|d| db().get(d).id == "BGFYM_011");
        db().iter()
            .filter(|c| c.pools.hero && c.kind == CardKind::Hero)
            .filter(|c| !self.cfg.experimental_mode || crate::experimental::hero_allowed(c))
            .filter(move |c| hero_fits(&self.cfg, self.lobby_races, yshaarj, c))
    }

    /// Create a lobby: pick types, deity and Dark Paradox variant, build pools, deal hero
    /// offers. The game starts in `Phase::HeroSelect`.
    pub fn new(seed: u64, mut cfg: RulesConfig) -> Game {
        if cfg.experimental_mode {
            cfg.timewarp_enabled = true;
        }
        let mut rng = Rng::new(seed);
        let d = db();
        let n = cfg.num_players;

        // Lobby types: Aberration is mandatory while `aberration_forced`; the rest are drawn
        // uniformly (weights unknown).
        let optional = [
            Races::BEAST,
            Races::DEMON,
            Races::DRAGON,
            Races::ELEMENTAL,
            Races::MECH,
            Races::MURLOC,
            Races::PIRATE,
            Races::QUILBOAR,
            Races::UNDEAD,
        ];
        let mut lobby = Races::NONE;
        let mut optional = optional.to_vec();
        let mut k = cfg.lobby_races;
        optional.push(Races::ABERRATION);
        let forced = cfg.lobby_types.iter().filter_map(|t| {
            let i = Races::NAMES.iter().position(|n| n == t)?;
            Some(Races(1 << i))
        });
        let aberration = cfg.aberration_forced.then_some(Races::ABERRATION);
        for r in aberration.into_iter().chain(forced) {
            if !lobby.intersects(r) && k > 0 {
                lobby = lobby.union(r);
                k -= 1;
            }
        }
        optional.retain(|r| !lobby.intersects(*r));
        for i in rng.sample_indices(optional.len(), k) {
            lobby = lobby.union(optional[i]);
        }
        let aberrations = lobby.intersects(Races::ABERRATION);

        let deity = d.id(DEITIES[rng.below(DEITIES.len())]);
        let yshaarj = d.get(deity).id == "BGFYM_011";

        // Dark Paradox: one form per lobby, in every lobby (its base card is not an
        // Aberration-subset card). The tier-5 form is banned with Undead; tiers 5/6 are banned with
        // Y'Shaarj (only a Deity in Aberration lobbies). The bans go by Tavern Tier, not by the id
        // suffix: the Tier 5 form is BG36_360t4 and the Tier 6 form is BG36_360t9.
        let mut variants: Vec<&str> = PARADOX_VARIANTS.to_vec();
        let form_tier = |v: &str| d.get(d.id(v)).tier;
        if lobby.intersects(Races::UNDEAD) {
            variants.retain(|v| form_tier(v) != 5);
        }
        if yshaarj && aberrations {
            variants.retain(|v| !matches!(form_tier(v), 5 | 6));
        }
        let paradox = d.id(variants[rng.below(variants.len())]);

        let content_ok = |c: CardId| cfg.content == ContentMode::AllVanilla || implemented(c);
        let universe: Vec<CardId> = d
            .iter()
            // Tier 7 minions share the pool too (5 copies each; only Tier 7 Taverns draw them).
            .filter(|c| {
                let t7 = c.pools.tier7 && c.normal.is_none();
                (c.pools.minion || t7) && c.kind == CardKind::Minion
            })
            .filter(|c| c.tier >= 1 && (c.tier <= cfg.max_regular_tier || c.pools.tier7))
            .filter(|c| {
                if c.id.starts_with("BG36_360t") {
                    return c.cid == paradox;
                }
                c.races.is_none() || c.races == Races::ALL || c.races.intersects(lobby)
            })
            // Typeless minions tied to a type (Nadina the Red: Dragons, Disguised
            // Graverobber: Undead; client subset tag) need that type in the lobby.
            .filter(|c| c.assoc.is_none() || c.assoc.intersects(lobby))
            .filter(|c| content_ok(c.cid))
            .filter(|c| !cfg.strict_pool_tag || c.pool_tag <= 1)
            .filter(|c| !cfg.excluded_cards.contains(&c.id))
            .map(|c| c.cid)
            .collect();
        // Sorted by tier (then id) so pool draws walk only the tiers they may offer.
        let mut universe = universe;
        universe.sort_by_key(|c| (d.get(*c).tier, *c));
        let mut pool = Pool::empty();
        for &c in &universe {
            let t = d.get(c).tier as usize;
            pool.put(c, cfg.pool_copies_by_tier[(t - 1).min(6)] as u16);
        }
        let spell_universe: Vec<CardId> = d
            .iter()
            // Tier 7 spells are in the pool too; only a Tier 7 Tavern draws them.
            .filter(|c| c.pools.spell && c.kind == CardKind::TavernSpell && c.tier <= 7)
            .filter(|c| content_ok(c.cid) && script(c.cid).is_some())
            // Spells tied to a minion type (Bounties: Pirates, Cloning Conch: Murlocs...)
            // need that type in the lobby (client subset tags, `CardDef::lobby_req`).
            .filter(|c| c.lobby_req.is_none() || c.lobby_req.intersects(lobby))
            // (`excluded_cards` is for every pool: a counterfactual lobby without a spell.)
            .filter(|c| !cfg.excluded_cards.contains(&c.id))
            .map(|c| c.cid)
            .collect();
        let trinket_universe: Vec<CardId> = d
            .iter()
            .filter(|c| c.pools.trinket && c.kind.is_trinket())
            // Bundle condition `lobby_or_offer_association`: a trinket associated with minion types
            // needs at least one of them in the lobby (exact weights unknown).
            .filter(|c| c.assoc.is_none() || c.assoc.intersects(lobby))
            // Aberration-subset trinkets (Deity, discard...) need Aberrations in the lobby.
            .filter(|c| c.lobby_req.is_none() || c.lobby_req.intersects(lobby))
            .filter(|c| script(c.cid).is_some() || cfg.content == ContentMode::AllVanilla)
            .map(|c| c.cid)
            .collect();
        let gift_universe: Vec<CardId> = d
            .iter()
            .filter(|c| c.kind == CardKind::DarkGift && c.window.is_some())
            .filter(|c| script(c.cid).is_some())
            .map(|c| c.cid)
            .collect();

        let mut heroes: Vec<CardId> = d
            .iter()
            .filter(|c| c.kind == CardKind::Hero)
            .filter(|c| !cfg.experimental_mode || crate::experimental::hero_allowed(c))
            .filter(|c| {
                if cfg.hero_pool.is_empty() {
                    c.pools.hero
                        && hero_fits_from(&cfg, lobby, yshaarj, c, HERO_BANS_BEFORE_SHUFFLE)
                } else {
                    cfg.hero_pool.contains(&c.id)
                }
            })
            .filter(|c| {
                c.hero_power.is_some_and(|hp| {
                    script(hp).is_some() || cfg.content == ContentMode::AllVanilla
                })
            })
            .map(|c| c.cid)
            .collect();
        rng.shuffle(&mut heroes);
        if cfg.hero_pool.is_empty() {
            heroes.retain(|h| hero_fits(&cfg, lobby, yshaarj, d.get(*h)));
        }
        // New-hero guarantee: move each guaranteed hero into a distinct random player's
        // offer (swapping with whatever was dealt there).
        let per_seat = cfg.hero_choices.max(1);
        let dealt = (n * per_seat).min(heroes.len());
        let mut seats: Vec<usize> = (0..n).collect();
        rng.shuffle(&mut seats);
        let guaranteed = |h: CardId| cfg.guaranteed_heroes.contains(&d.get(h).id);
        for id in &cfg.guaranteed_heroes {
            let Some(pos) = heroes.iter().position(|h| d.get(*h).id == *id) else {
                continue;
            };
            // (Already dealt to someone: keep it where it is.)
            if pos < dealt {
                continue;
            }
            // Swap it into the first option of a seat that has no guaranteed hero yet.
            let slot = seats.iter().map(|seat| seat * per_seat).find(|slot| {
                *slot < dealt
                    && !heroes[*slot..(*slot + per_seat).min(heroes.len())]
                        .iter()
                        .any(|h| guaranteed(*h))
            });
            if let Some(slot) = slot {
                heroes.swap(pos, slot);
            }
        }

        let mut players: Vec<Player> = (0..n).map(Player::new).collect();
        let per = cfg.hero_choices.max(1);
        for (i, p) in players.iter_mut().enumerate() {
            let mut options: Vec<CardId> = heroes.iter().skip(i * per).take(per).copied().collect();
            if options.is_empty() {
                // Fewer heroes than seats: reuse (duplicates are a simulator fallback).
                options = heroes
                    .iter()
                    .cycle()
                    .skip(i * per)
                    .take(per)
                    .copied()
                    .collect();
            }
            p.pending.push(Pending::HeroSelect { options });
            // No Deity without Aberrations in the lobby (36.6.3: they are in half of them).
            p.deity = aberrations.then(|| DeityState {
                card: deity,
                atk: d.get(deity).atk,
                hp: d.get(deity).hp,
                golden: false,
                reborn: false,
            });
            p.upgrade_cost = cfg.upgrade_costs[0] + cfg.upgrade_discount_per_turn;
        }

        let mut g = Game {
            cfg,
            seed,
            rng,
            turn: 0,
            phase: Phase::HeroSelect,
            players,
            pool,
            next_eid: 0,
            lobby_races: lobby,
            deity: Some(deity),
            paradox: Some(paradox),
            universe,
            uidx: Default::default(),
            spell_universe,
            trinket_universe,
            gift_universe,
            eliminated: vec![],
            ghost_player: None,
            ending_turn: false,
            last_combats: vec![],
            truncated_combats: 0,
            truncated_recruit_contexts: 0,
            stopped_reason: None,
            turn_limit_end: false,
            card_table: d.fingerprint,
            pairs: vec![],
            turn_alive: 0,
            log: vec![],
            last_frames: vec![],
        };
        g.log(|| {
            format!(
                "lobby types {lobby:?}, deity {}, paradox {}",
                d.get(deity).name,
                d.get(paradox).id
            )
        });
        g
    }

    /// Apply a hero choice; starts turn 1 once everyone has chosen.
    pub fn select_hero(&mut self, p: PlayerId, hero: CardId) {
        let d = db().get(hero);
        let pl = &mut self.players[p];
        pl.hero = Some(hero);
        if self.cfg.experimental_mode {
            pl.buddy_button = d
                .buddy
                .filter(|b| crate::experimental::buddy_allowed(*b))
                .map(crate::experimental::BuddyButton::new);
        }
        pl.powers.clear();
        pl.health = 30;
        pl.armor = d.armor;
        let name = d.name.clone();
        self.log(|| format!("p{p} picks {name}"));
        if let Some(hp) = d.hero_power {
            self.set_hero_power(p, 0, hp);
        }
        if self.phase == Phase::HeroSelect && self.players.iter().all(|p| p.hero.is_some()) {
            self.phase = Phase::Recruit;
            self.start_turn();
        }
    }

    /// Put Hero Power `card` in `slot` (replacing it, or adding a new one past the end) and
    /// run its `on_attach` ("At the start of the game..." / on gaining it).
    pub fn set_hero_power(&mut self, p: PlayerId, slot: usize, card: CardId) {
        self.with_recruit(p, |ctx| ctx.set_hero_power(0, slot, card));
    }

    /// Begin a new turn for every living player.
    pub fn start_turn(&mut self) {
        self.turn += 1;
        let t = self.turn;
        self.log(|| format!("=== turn {t} ==="));
        self.turn_alive = self.alive_count();
        self.decide_pairs();
        for p in 0..self.players.len() {
            if self.players[p].alive {
                self.start_turn_player(p);
                self.flush_hand_queue(p);
            }
        }
    }

    fn start_turn_player(&mut self, p: PlayerId) {
        let cfg = &self.cfg;
        let turn = self.turn as i32;
        let base = (cfg.start_gold + cfg.gold_increment * (turn - 1)).min(cfg.gold_refill_cap);
        let discount = cfg.upgrade_discount_per_turn;
        let pl = &mut self.players[p];
        pl.ended_turn = false;
        pl.actions_this_turn = 0;
        pl.freezes_this_turn = 0;
        pl.skipping = false;
        pl.turn_data.clear();
        // Refill: the natural amount plus max-Gold bonuses, capped at 99; Gold gained on top at the
        // start of the turn has no cap.
        pl.gold = (base + pl.max_gold_bonus).min(99) + pl.gold_next_turn;
        pl.gold_next_turn = 0;
        pl.upgrade_cost = (pl.upgrade_cost - discount).max(0);
        for h in pl.powers.iter_mut() {
            h.uses_turn = 0;
            h.cooldown = h.cooldown.saturating_sub(1);
        }
        pl.dark_gift_used_turn = false;
        pl.stats.gold_spent_turn = 0;
        pl.stats.spells_cast_turn = 0;
        pl.stats.tavern_spells_cast_turn = 0;
        pl.stats.cards_played_turn = 0;
        pl.stats.minions_played_turn = 0;
        pl.stats.spellcraft_cast_turn = 0;
        pl.stats.elementals_played_turn = 0;
        pl.stats.battlecries_turn = 0;
        // The Golden Hammer: "Golden until next turn", wherever the minion is by then.
        for (eid, card, a, h) in std::mem::take(&mut pl.revert_golden) {
            let m = pl.board.iter_mut().find(|m| m.eid == eid).or_else(|| {
                pl.hand
                    .iter_mut()
                    .filter_map(|x| x.item.as_minion_mut())
                    .find(|m| m.eid == eid)
            });
            if let Some(m) = m {
                m.card = card;
                m.golden = false;
                m.triple_reward = false;
                m.atk = (m.atk - a).max(0);
                m.max_hp = (m.max_hp - h).max(1);
                m.hp = m.hp.min(m.max_hp);
            }
        }
        for m in pl.board.iter_mut() {
            expire_temp(m);
            m.activations = 0;
        }
        for h in pl.hand.iter_mut() {
            if let Item::Minion(m) = &mut h.item {
                expire_temp(m);
            }
            if !h.held {
                h.locked = h.locked.saturating_sub(1);
            }
        }
        for s in pl.shop.iter_mut() {
            if let Item::Minion(m) = &mut s.item {
                expire_temp(m);
            }
        }
        let log_start = self.log.len();
        let turn = self.turn;
        let lesser = self.cfg.lesser_trinket_turn;
        let greater = self.cfg.greater_trinket_turn;
        self.with_recruit(p, |ctx| {
            if ctx.player_ref(0).unwrap().is_frozen() {
                // Frozen cards stay (one turn); the rest of the Tavern is refilled. When new cards
                // come in, "after the Tavern is Refreshed" effects trigger; a fully frozen Tavern
                // is not refreshed.
                let unfrozen: Vec<EntityId> = ctx
                    .player_ref(0)
                    .unwrap()
                    .shop
                    .iter()
                    .filter(|s| !s.frozen)
                    .map(|s| s.sid)
                    .collect();
                for sid in unfrozen {
                    if let Some(m) = ctx.take_from_shop(0, sid) {
                        ctx.g.pool.return_minion(&m);
                    } else if let Some(pl) = ctx.player(0) {
                        pl.shop.retain(|s| s.sid != sid);
                    }
                }
                let pl = ctx.player(0).unwrap();
                pl.set_frozen(false);
                for s in pl.shop.iter_mut() {
                    s.kept = true;
                    s.filled = false;
                }
                let before = ctx.player_ref(0).unwrap().shop.len();
                ctx.fill_shop(0);
                if ctx.player_ref(0).unwrap().shop.len() > before {
                    // A real refill: extra offers, Fodder and the event, like any Refresh.
                    ctx.finish_refresh(0, false);
                } else {
                    ctx.feed_fodder(0);
                }
            } else {
                ctx.refresh_shop_ex(0, false);
            }
        });
        // Turn start order: Gold and upgrade discount -> temporary effects expire -> Tavern ->
        // trinket offer -> trinkets, then Hero Powers -> minions left to right (each gives its
        // Spellcraft spell as it comes up) -> scheduled -> hand -> triples.
        let pl = &self.players[p];
        let greater = if pl.greater_turn > 0 {
            pl.greater_turn
        } else {
            greater
        };
        // Two separate tests: a Greater offer moved onto the Lesser turn (Marin with Ornate
        // Clock) comes that turn too, on top of the Lesser one.
        let greater_due = turn >= greater && !pl.greater_offered;
        if turn == lesser {
            self.offer_trinkets(p, CardKind::TrinketLesser);
        }
        if greater_due {
            self.players[p].greater_offered = true;
            self.offer_trinkets(p, CardKind::TrinketGreater);
        }
        // The Timewarped Tavern system (off in 36.6.3): everyone visits on the two turns.
        if self.cfg.timewarp_enabled && !self.cfg.experimental_mode {
            let [minor, major] = self.cfg.timewarp_turns;
            if turn == minor || turn == major {
                self.with_recruit(p, |ctx| {
                    crate::content::timewarp::open(ctx, 0, turn == major)
                });
            }
        }
        self.with_recruit(p, |ctx| {
            crate::content::release_ready_held(ctx, 0, 0);
            ctx.run_player_hook(0, |s| s.start_of_turn, "start of turn");
            crate::content::cube_turn_start(ctx, 0);
            crate::content::hero_start_of_turn(ctx, 0);
            for eid in ctx.friendly(0) {
                ctx.give_spellcraft(0, eid);
                ctx.run_minion_hook(0, eid, |s| s.start_of_turn, 1, "start of turn");
            }
            crate::content::run_scheduled(ctx, 0);
            crate::content::hand_start_of_turn(ctx, 0);
            ctx.check_triples(0);
            ctx.resolve_deaths();
        });
        if self.cfg.experimental_mode {
            self.with_recruit(p, |ctx| {
                if ctx.g.turn == 4 {
                    ctx.offer_quests(0, 3);
                }
                let [minor, major] = ctx.g.cfg.timewarp_turns;
                if turn == minor || turn == major {
                    crate::content::timewarp::open(ctx, 0, turn == major);
                }
            });
        }
        if self.cfg.record_log {
            self.players[p].turn_events = self.log[log_start..].to_vec();
        }
    }

    /// Offer trinkets of a size as a blocking choice (pick one of 4 and pay; eligibility weights
    /// unknown).
    pub fn offer_trinkets(&mut self, p: PlayerId, kind: CardKind) {
        // Mysterious Orb: your next Trinket is a Lesser Trinket.
        let kind = if std::mem::take(&mut self.players[p].next_trinket_lesser) {
            CardKind::TrinketLesser
        } else {
            kind
        };
        // A paid offer that is still open keeps its own discounts while this one is on top.
        let below = self.players[p]
            .pending
            .iter()
            .any(Pending::paid_trinket_offer);
        let kept = std::mem::take(&mut self.players[p].trinket_pivots);
        let offer = self.trinket_offer(p, kind);
        let gold = self.players[p].gold;
        // Nothing affordable: the offer is skipped (an assumption).
        if offer.is_empty() || offer.iter().all(|t| self.trinket_price(p, *t) > gold) {
            self.players[p].trinket_pivots = if below { kept } else { Vec::new() };
            self.log(|| format!("p{p}: no affordable trinket, offer skipped"));
            return;
        }
        if below {
            self.players[p].trinket_pivot_stack.push(kept);
        }
        self.players[p].pending.push(Pending::Trinket {
            options: offer,
            free: false,
            replace: None,
        });
    }

    /// Price of trinket `t` in player `p`'s current offer (pivot options are cheaper).
    pub fn trinket_price(&self, p: PlayerId, t: CardId) -> i32 {
        let cost = db().get(t).cost;
        if self.players[p].trinket_pivots.contains(&t) {
            (cost - self.cfg.trinket_pivot_discount).max(0)
        } else {
            cost
        }
    }

    /// The types of the minions that count for being "in" a type for the trinket offer rules: those
    /// on board (not in hand).
    pub(crate) fn offer_minion_types(&self, p: PlayerId) -> Vec<Races> {
        self.players[p].board.iter().map(|m| m.races).collect()
    }

    /// Minions of each type on board (all-type minions count for every type).
    fn type_counts(&self, p: PlayerId) -> Vec<(Races, usize)> {
        // Only the types of this lobby: an all-type minion does not make "your most common type" a
        // type that is not in the game.
        let races = self.offer_minion_types(p);
        Races::ALL
            .iter()
            .filter(|t| self.lobby_races.intersects(*t))
            .map(|t| (t, races.iter().filter(|r| r.intersects(t)).count()))
            .collect()
    }

    /// Distinct minion types on board, each minion standing for one type at most
    /// (an all-type minion alone is one type, three of them are three; two Beasts and an
    /// all-type minion are two): the matching of `content::distinct_types_of`.
    fn distinct_types(&self, p: PlayerId) -> usize {
        let races = self.offer_minion_types(p);
        crate::content::distinct_types_of(&races).max(0) as usize
    }

    /// Build a trinket offer (official "Updated Trinket Rules" 2026-04-06), from all eligible
    /// trinkets -- affordable or not:
    /// - "in" a type = >= 2 (Lesser) / >= 3 (Greater) minions of it on board; "in" menagerie
    ///   = 3 or more distinct types among them. A typed trinket you are not in is offered
    ///   only if it can be discounted, and then costs 2 Gold less (a "pivot" option);
    /// - every offer includes a no-type trinket, one of the most common type you are in and
    ///   one whose price is 2 or less (no menagerie trinket is guaranteed: client offers);
    /// - at most one trinket for each type that isn't your most common one (so several
    ///   pivot options of different types can come together), at most one menagerie
    ///   trinket, and the exclusion groups (`trinket_offers::EXCLUSIVE_GROUPS`);
    /// - picks are weighted by the client's offer weights (`trinket_offers::weight`).
    ///   Sets `Player::trinket_pivots`.
    pub fn trinket_offer(&mut self, p: PlayerId, kind: CardKind) -> Vec<CardId> {
        use crate::content::trinket_offers::{DISCOUNTABLE, EXCLUSIVE_GROUPS, MENAGERIE, types};
        let d = db();
        let greater = kind == CardKind::TrinketGreater;
        let need = self.cfg.trinket_type_min[usize::from(greater)];
        let counts = self.type_counts(p);
        let in_types = counts
            .iter()
            .filter(|x| x.1 >= need)
            .fold(Races::NONE, |a, x| a.union(x.0));
        let in_menagerie = self.distinct_types(p) >= 3;
        // Your most common type among those you're in (ties random).
        let top = counts
            .iter()
            .filter(|x| x.1 >= need)
            .map(|x| x.1)
            .max()
            .unwrap_or(0);
        let most: Vec<Races> = counts
            .iter()
            .filter(|x| top > 0 && x.1 == top)
            .map(|x| x.0)
            .collect();
        let most = self.rng.pick(&most).copied().unwrap_or(Races::NONE);
        let menagerie = |t: CardId| MENAGERIE.contains(&d.get(t).id.as_str());
        let typed = |t: CardId| !types(t).is_none() || menagerie(t);
        let is_in = |t: CardId| types(t).intersects(in_types) || (menagerie(t) && in_menagerie);
        // (card, pivot?)
        let cands: Vec<(CardId, bool)> = self
            .trinket_universe
            .iter()
            .copied()
            .filter(|t| d.get(*t).kind == kind && self.trinket_eligible(p, *t))
            .map(|t| (t, typed(t) && !is_in(t)))
            // A typed trinket you're not in is only offered if it can be discounted.
            .filter(|(t, pivot)| !pivot || DISCOUNTABLE.contains(&d.get(*t).id.as_str()))
            .collect();
        let neutral = |t: CardId| !typed(t);
        let discount = self.cfg.trinket_pivot_discount;
        // The price the option is offered at.
        let cheap = |c: &(CardId, bool)| {
            let cost = d.get(c.0).cost;
            (if c.1 { (cost - discount).max(0) } else { cost }) <= 2
        };
        // A typed trinket of a type other than your most common one.
        let other = |t: CardId| !types(t).is_none() && !types(t).intersects(most);
        let group = |t: CardId| {
            let id = d.get(t).id.as_str();
            EXCLUSIVE_GROUPS.iter().position(|g| g.contains(&id))
        };
        let mut picked: Vec<(CardId, bool)> = vec![];
        let fits = |picked: &[(CardId, bool)], c: &(CardId, bool)| {
            !picked.iter().any(|x| x.0 == c.0)
                && !(menagerie(c.0) && picked.iter().any(|x| menagerie(x.0)))
                && !(group(c.0).is_some() && picked.iter().any(|x| group(x.0) == group(c.0)))
                && !(other(c.0)
                    && picked
                        .iter()
                        .any(|x| other(x.0) && types(x.0).intersects(types(c.0))))
        };
        type Req<'a> = Box<dyn Fn(&(CardId, bool)) -> bool + 'a>;
        let mut reqs: Vec<Req> = vec![Box::new(|c| neutral(c.0))];
        if !most.is_none() {
            reqs.push(Box::new(|c| !c.1 && types(c.0).intersects(most)));
        }
        reqs.push(Box::new(cheap));
        let n = self.cfg.trinket_choices;
        for r in &reqs {
            if picked.len() >= n || picked.iter().any(r) {
                continue;
            }
            let ok: Vec<(CardId, bool)> = cands
                .iter()
                .copied()
                .filter(|c| r(c) && fits(&picked, c))
                .collect();
            if let Some(c) = self.pick_trinket(&ok) {
                picked.push(c);
            }
        }
        while picked.len() < n {
            let ok: Vec<(CardId, bool)> =
                cands.iter().copied().filter(|c| fits(&picked, c)).collect();
            let Some(c) = self.pick_trinket(&ok) else {
                break;
            };
            picked.push(c);
        }
        self.rng.shuffle(&mut picked);
        self.players[p].trinket_pivots = picked.iter().filter(|x| x.1).map(|x| x.0).collect();
        picked.into_iter().map(|x| x.0).collect()
    }

    /// One of `ok` by offer weight (`trinket_offers::weight`).
    fn pick_trinket(&mut self, ok: &[(CardId, bool)]) -> Option<(CardId, bool)> {
        let ws: Vec<u64> = ok
            .iter()
            .map(|c| crate::content::trinket_offers::weight(self, &db().get(c.0).id))
            .collect();
        let total: u64 = ws.iter().sum();
        if total == 0 {
            return None;
        }
        let mut r = self.rng.next_u64() % total;
        for (c, w) in ok.iter().zip(ws) {
            if r < w {
                return Some(*c);
            }
            r -= w;
        }
        None
    }

    /// Whether trinket `t` may be offered to player `p`: not already owned, and the card's own
    /// condition (e.g. Ancient Wishbone needs an active hero power). Lobby-type association is
    /// applied when the lobby's trinket universe is built.
    pub fn trinket_eligible(&self, p: PlayerId, t: CardId) -> bool {
        if self.players[p].has_trinket_id(t)
            || !crate::content::trinket_offers::offer_rule(self, p, &db().get(t).id)
        {
            return false;
        }
        script(t)
            .and_then(|s| s.offer_ok)
            .is_none_or(|ok| ok(self, p))
    }

    /// End a player's recruit phase; when all living players are done, fight and advance.
    pub fn end_turn(&mut self, p: PlayerId) -> crate::recruit::ActResult {
        let pl = &self.players[p];
        if !pl.alive || pl.ended_turn {
            return Err("can't end turn");
        }
        if !pl.pending.is_empty() {
            return Err("a choice is pending");
        }
        // Cards gained at the end of the turn triple at the next turn start.
        self.ending_turn = true;
        self.with_recruit(p, |ctx| {
            // Sindragosa: the Tavern freezes at the end of each turn - before the end of turn
            // effects, so that her Thawed Champion finds Frozen minions to make Golden (it found
            // none), and again after them for what they added to the Tavern.
            let sindragosa = ctx.player_ref(0).unwrap().has_power("TB_BaconShop_HP_014");
            if sindragosa {
                ctx.player(0).unwrap().set_frozen(true);
            }
            let mult = ctx.end_of_turn_mult(0);
            ctx.run_player_hook_times(0, |s| s.end_of_turn, mult, "end of turn");
            ctx.run_board_hook(0, |s| s.end_of_turn, mult, "end of turn");
            ctx.resolve_deaths();
            if sindragosa {
                ctx.player(0).unwrap().set_frozen(true);
            }
        });
        // Pending choices created by end-of-turn effects are resolved randomly.
        let mut guard = 0;
        while let Some(pend) = self.players[p].pending.last() {
            let legal: Vec<usize> = (0..pend.len())
                .filter(|i| self.option_legal(p, *i))
                .collect();
            guard += 1;
            if legal.is_empty() || guard > 256 {
                self.players[p].pending.clear();
                break;
            }
            let i = legal[self.rng.below(legal.len())];
            let _ = self.choose(p, i);
        }
        self.ending_turn = false;
        let pl = &mut self.players[p];
        pl.ended_turn = true;
        pl.chronum_balance = 0;
        // Temporary cards (Spellcraft spells) leave the hand, and the queue of cards waiting
        // for space; this is not a discard.
        pl.drop_temporary_cards();
        if self.cfg.experimental_mode {
            pl.normalize_timewarp_hand_limit(self.cfg.hand_size);
        }
        pl.last_board = pl.board.clone();
        self.fight_when_all_ended();
        Ok(())
    }

    /// After a seat ends its turn: fight once every living seat has ended. A seat its own
    /// end-of-turn effects killed is eliminated first: it never fights as a living seat, whatever
    /// order the seats ended in. `recruit_deaths` starts the round itself when everyone is done.
    fn fight_when_all_ended(&mut self) {
        if self.players.iter().any(|p| p.alive && p.health <= 0) {
            self.recruit_deaths();
        } else if self.players.iter().all(|p| !p.alive || p.ended_turn) {
            self.combat_round();
        }
    }

    /// End a seat's recruit phase without running end-of-turn effects or resolving choices
    /// (for a warband set up from outside, after those effects). Fights when everyone is
    /// done, like `end_turn`.
    pub fn end_turn_silent(&mut self, p: PlayerId) {
        let pl = &mut self.players[p];
        if !pl.alive || pl.ended_turn || self.phase != Phase::Recruit {
            return;
        }
        pl.pending.clear();
        pl.ended_turn = true;
        pl.chronum_balance = 0;
        pl.drop_temporary_cards();
        if self.cfg.experimental_mode {
            pl.normalize_timewarp_hand_limit(self.cfg.hand_size);
        }
        pl.last_board = pl.board.clone();
        self.fight_when_all_ended();
    }

    /// Pair living players (avoiding last opponents when possible), fight, apply damage,
    /// eliminate, and start the next turn.
    /// Pair living players for this turn's combat (avoiding last opponents when possible)
    /// and publish each player's next opponent, so it is known during the recruit phase.
    pub fn decide_pairs(&mut self) {
        let alive: Vec<PlayerId> = (0..self.players.len())
            .filter(|p| self.players[*p].alive)
            .collect();
        let pairs = self.make_pairs(&alive);
        for p in self.players.iter_mut() {
            p.next_opponent = None;
            p.next_is_ghost = false;
        }
        for &(a, b) in &pairs {
            self.players[a].next_opponent = b;
            self.players[a].next_is_ghost = b.is_none();
            if let Some(b) = b {
                self.players[b].next_opponent = Some(a);
            }
        }
        let s = pairs
            .iter()
            .map(|(a, b)| match b {
                Some(b) => format!("p{a}-p{b}"),
                None => format!("p{a}-ghost"),
            })
            .collect::<Vec<_>>()
            .join(" ");
        self.log(|| format!("pairings: {s}"));
        self.pairs = pairs;
    }

    /// `viewer` has fought `other` (or its ghost) this turn: it remembers what it saw.
    fn note_scouted(&mut self, viewer: PlayerId, other: PlayerId) {
        let n = self.players.len();
        let o = &self.players[other];
        let seen = Scouted {
            board: Arc::new(o.seen_board.clone()),
            turn: self.turn,
        };
        let known = &mut self.players[viewer].scouted;
        if known.len() < n {
            known.resize_with(n, Scouted::default);
        }
        known[other] = seen;
    }

    /// What `viewer` may know of the warband of `other`: the one `other` took into the last combat
    /// between the two, and the turn of that combat (0 = they have not met). The game shows nothing
    /// of the warbands a player did not fight. The warband of player `o` as the game last showed
    /// it: what it fought with in the last finished combat round (`seen_board`), or a dead player's
    /// final warband (`last_board`, the ghost's). Every effect that reads another player's warband
    /// during the recruit phase goes through this: a living player's `last_board` and `board` are
    /// already this turn's once that seat has ended its turn (or is buying), which nobody has seen
    /// and which would make the result depend on the order of the seats.
    pub fn fought_board(&self, o: PlayerId) -> &Vec<Minion> {
        let pl = &self.players[o];
        if pl.alive {
            &pl.seen_board
        } else {
            &pl.last_board
        }
    }

    pub fn known_board(&self, viewer: PlayerId, other: PlayerId) -> (&[Minion], u32) {
        self.board_shown(viewer, other, self.players[viewer].sees_all_boards)
    }

    /// `known_board` in a given view: `all` = every warband of the last completed round (more
    /// than the game shows; for diagnostics).
    pub fn board_shown(&self, viewer: PlayerId, other: PlayerId, all: bool) -> (&[Minion], u32) {
        if all {
            let board = &self.players[other].seen_board;
            let turn = if board.is_empty() {
                0
            } else {
                self.turn.saturating_sub(1)
            };
            return (board, turn);
        }
        match self.players[viewer].scouted.get(other) {
            Some(s) => (&s.board, s.turn),
            None => (&[], 0),
        }
    }

    pub fn combat_round(&mut self) {
        // Records (and replay frames) describe only the most recent round.
        self.last_combats.clear();
        let alive: Vec<PlayerId> = (0..self.players.len())
            .filter(|p| self.players[*p].alive)
            .collect();
        // Use the pairings announced at turn start (recompute only if they no longer match,
        // e.g. an old snapshot without pairings).
        let valid = !self.pairs.is_empty()
            && self
                .pairs
                .iter()
                .all(|(a, b)| self.players[*a].alive && b.is_none_or(|b| self.players[b].alive))
            && self
                .pairs
                .iter()
                .map(|(_, b)| 1 + b.is_some() as usize)
                .sum::<usize>()
                == alive.len();
        let pairs = if valid {
            std::mem::take(&mut self.pairs)
        } else {
            self.make_pairs(&alive)
        };
        for &(a, b) in &pairs {
            match b {
                Some(b) => {
                    let out = self.simulate_combat(a, Some(b), None);
                    self.apply_combat_result(a, Some(b), out);
                    self.players[a].last_opponent = Some(b);
                    self.players[b].last_opponent = Some(a);
                    self.players[a].note_opponent(Some(b));
                    self.players[b].note_opponent(Some(a));
                }
                None => {
                    // Ghost: the most recently eliminated player's final warband (the exact
                    // ghost policy is unpublished).
                    let ghost = self
                        .eliminated
                        .last()
                        .map(|g| (self.players[*g].last_board.clone(), self.players[*g].tier))
                        .unwrap_or((vec![], 1));
                    let ghost_id = self.eliminated.last().copied();
                    self.ghost_player = ghost_id;
                    let out = self.simulate_combat(a, None, Some(ghost));
                    // The combat takes the ghost's identity for itself; afterwards the ghost is
                    // still this player's last opponent (`last_fought`) and the warband this player
                    // saw.
                    self.ghost_player = ghost_id;
                    self.apply_combat_result(a, None, out);
                    self.players[a].last_opponent = None;
                    self.players[a].note_opponent(None);
                }
            }
        }
        // The warband each player showed this round (the ghost still uses `last_board`)...
        for p in self.players.iter_mut() {
            p.seen_board.clone_from(&p.last_board);
        }
        // ... and who saw it: the two sides of a combat see each other, nobody else does.
        for &(a, b) in &pairs {
            match b {
                Some(b) => {
                    self.note_scouted(a, b);
                    self.note_scouted(b, a);
                }
                None => {
                    if let Some(ghost) = self.ghost_player {
                        self.note_scouted(a, ghost);
                    }
                }
            }
        }
        self.eliminate_dead();
        if self.alive_count() <= 1 {
            for p in self.players.iter_mut() {
                if p.alive {
                    p.placement = 1;
                }
            }
            self.phase = Phase::GameOver;
            self.log(|| "game over".into());
            return;
        }
        // Simulator safety (not a game rule): a lobby whose survivors can never hurt each
        // other (empty warbands and hands full of unplayable cards) would go on forever.
        if self.cfg.max_turns > 0 && self.turn >= self.cfg.max_turns {
            self.turn_limit_end = true;
            self.finish_by_health();
            return;
        }
        self.start_turn();
    }

    /// End the game at the turn cap: the survivors are ranked by Health + Armor (ties by
    /// seat); only the first stays alive.
    fn finish_by_health(&mut self) {
        let mut alive: Vec<PlayerId> = (0..self.players.len())
            .filter(|p| self.players[*p].alive)
            .collect();
        alive.sort_by_key(|p| {
            let pl = &self.players[*p];
            (std::cmp::Reverse(pl.health + pl.armor), *p)
        });
        for (rank, p) in alive.into_iter().enumerate() {
            self.players[p].placement = rank as u8 + 1;
            self.players[p].alive = rank == 0;
        }
        self.phase = Phase::GameOver;
        let t = self.turn;
        self.log(|| format!("game over: turn cap {t} reached, ranked by Health"));
    }

    /// Pairings: nobody meets the same opponent -- the ghost included -- twice within 3 combats
    /// (not in their last 2), except with 2 players left. The search runs over randomly ordered
    /// players; with no valid pairing the window shrinks to the last combat, then to none (fewest
    /// repeats is not needed: 0 windows always succeed).
    fn make_pairs(&mut self, alive: &[PlayerId]) -> Vec<(PlayerId, Option<PlayerId>)> {
        let mut order = alive.to_vec();
        self.rng.shuffle(&mut order);
        let windows: &[usize] = if alive.len() <= 2 { &[0] } else { &[2, 1, 0] };
        for &w in windows {
            let recent: Vec<Vec<Option<PlayerId>>> = self
                .players
                .iter()
                .map(|p| p.recent_opponents.iter().rev().take(w).copied().collect())
                .collect();
            let mut out = vec![];
            if pair_search(&order, &recent, &mut out) {
                return out;
            }
        }
        unreachable!("an unconstrained pairing always exists")
    }

    /// Eliminate players at 0 health. Several deaths in one round are ranked by remaining health
    /// (lower = worse); exact server tie-break is unpublished. A hero reduced to 0 Health during
    /// the recruit phase (Lich Baz'hial's own power...) is eliminated at once, not after the next
    /// combat. The round goes on for the others; pairings are redone at combat time.
    pub fn recruit_deaths(&mut self) {
        if self.phase != Phase::Recruit || !self.players.iter().any(|p| p.alive && p.health <= 0) {
            return;
        }
        self.eliminate_dead();
        if self.alive_count() <= 1 {
            for p in self.players.iter_mut() {
                if p.alive {
                    p.placement = 1;
                }
            }
            self.phase = Phase::GameOver;
            self.log(|| "game over".into());
        } else {
            self.repair_pairs();
            if self.players.iter().all(|p| !p.alive || p.ended_turn) {
                self.combat_round();
            }
        }
    }

    /// A player died in the recruit phase: the pairings announced for this turn stand for
    /// everyone else. Whoever was to meet a dead player meets the ghost instead; two players
    /// left without an opponent (one of them may have had the ghost) meet each other.
    fn repair_pairs(&mut self) {
        if self.pairs.is_empty() {
            return;
        }
        let alive = |g: &Game, p: PlayerId| g.players[p].alive;
        let mut pairs = vec![];
        let mut free: Vec<PlayerId> = vec![];
        for (a, b) in std::mem::take(&mut self.pairs) {
            match (alive(self, a), b.map(|b| (b, alive(self, b)))) {
                (true, Some((_, true))) => pairs.push((a, b)),
                (true, _) => free.push(a),
                (false, Some((b, true))) => free.push(b),
                (false, _) => {}
            }
        }
        for two in free.chunks(2) {
            pairs.push((two[0], two.get(1).copied()));
        }
        for &(a, b) in &pairs {
            self.players[a].next_opponent = b;
            self.players[a].next_is_ghost = b.is_none();
            if let Some(b) = b {
                self.players[b].next_opponent = Some(a);
                self.players[b].next_is_ghost = false;
            }
        }
        self.pairs = pairs;
    }

    fn eliminate_dead(&mut self) {
        let mut dead: Vec<PlayerId> = (0..self.players.len())
            .filter(|p| self.players[*p].alive && self.players[*p].health <= 0)
            .collect();
        // Lower Health + Armor places lower; with the same total the lower seat places better, as
        // at the turn cap (`finish_by_health`).
        dead.sort_by_key(|p| {
            (
                self.players[*p].health + self.players[*p].armor,
                std::cmp::Reverse(*p),
            )
        });
        let recruit = self.phase == Phase::Recruit;
        let mut place = self.alive_count() as u8;
        for p in dead {
            let pl = &mut self.players[p];
            pl.alive = false;
            pl.placement = place;
            place -= 1;
            // A player who dies in the recruit phase leaves this turn's warband as the
            // ghost's (`last_board` otherwise still held the previous turn's).
            if recruit {
                pl.last_board = pl.board.clone();
            }
            let last_board = pl.board.clone();
            // Return the warband, hand and tavern to the shared pool.
            let mut returned: Vec<(CardId, u16)> = vec![];
            for m in pl.board.iter() {
                m.visit_pool_parts(|c, n| returned.push((c, n)));
            }
            for h in pl.hand.iter() {
                if let Item::Minion(m) = &h.item {
                    m.visit_pool_parts(|c, n| returned.push((c, n)));
                }
            }
            // Spells held leave the game too: their copies return to the spell pool (copies
            // left are counted from live Taverns and hands).
            pl.hand.clear();
            for it in pl.hand_queue.drain(..) {
                if let Item::Minion(m) = &it {
                    m.visit_pool_parts(|c, n| returned.push((c, n)));
                }
            }
            for s in pl.shop.drain(..) {
                if let Item::Minion(m) = &s.item {
                    m.visit_pool_parts(|c, n| returned.push((c, n)));
                }
            }
            pl.pending.clear();
            for (c, n) in returned {
                self.pool.put(c, n);
            }
            self.eliminated.push(p);
            let pl_place = self.players[p].placement;
            self.log(|| format!("p{p} eliminated (place {pl_place})"));
            crate::content::on_eliminated(self, p, last_board);
        }
    }

    pub fn is_over(&self) -> bool {
        self.phase == Phase::GameOver
    }
}

fn expire_temp(m: &mut Minion) {
    if m.temp_atk != 0 || m.temp_hp != 0 {
        m.atk = m.atk.saturating_sub(m.temp_atk).max(0);
        m.max_hp = m.max_hp.saturating_sub(m.temp_hp).max(1);
        m.hp = m.hp.min(m.max_hp).max(1);
        m.temp_atk = 0;
        m.temp_hp = 0;
    }
    m.tavern_temp_atk = 0;
    m.tavern_temp_hp = 0;
    if m.temp_kw != Kw::NONE {
        m.kw.clear(m.temp_kw);
        if m.temp_kw.has(Kw::DIVINE_SHIELD) {
            m.shield_hits = 0;
        }
        m.temp_kw = Kw::NONE;
    }
}

/// Backtracking over the remaining players: the first one is paired with a later one (or,
/// with an odd count, with the ghost) that neither met within the window.
fn pair_search(
    rest: &[PlayerId],
    recent: &[Vec<Option<PlayerId>>],
    out: &mut Vec<(PlayerId, Option<PlayerId>)>,
) -> bool {
    let Some((&a, tail)) = rest.split_first() else {
        return true;
    };
    let met = |x: PlayerId, y: Option<PlayerId>| recent[x].contains(&y);
    if rest.len() % 2 == 1 && !met(a, None) {
        out.push((a, None));
        if pair_search(tail, recent, out) {
            return true;
        }
        out.pop();
    }
    for (k, &b) in tail.iter().enumerate() {
        if met(a, Some(b)) || met(b, Some(a)) {
            continue;
        }
        let mut next = tail.to_vec();
        next.remove(k);
        out.push((a, Some(b)));
        if pair_search(&next, recent, out) {
            return true;
        }
        out.pop();
    }
    false
}
