//! Automatic combat.
//!
//! Flow: snapshot both warbands -> first attacker = side with more minions, then the higher Tavern
//! Tier, then random -> Start of Combat (trinkets, Rewards and Quests, Hero Powers, Tavern-spell
//! effects, minions in hand, minions on the board) -> alternate attacks, each attacker chosen left
//! to right, target random among legal (Taunt first, Stealth excluded) -> deaths, Deathrattles,
//! Reborn, Avenge after every attack -> end when a side is empty or nobody can attack. Survivors'
//! tiers + winner tier = hero damage.

use crate::engine::{CombatRecord, Ctx, Game};
use crate::event::Event;
use crate::script::{Me, flag, script};
use crate::state::Minion;
use crate::types::{EntityId, Kw, PlayerId, Side};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CombatOutcome {
    /// 0 = side 0 won, 1 = side 1 won, 2 = tie.
    pub winner: u8,
    /// Uncapped damage the winner deals (tier + surviving minion tiers).
    pub damage: i32,
    /// The simulator's attack cutoff ended the fight (not an official result).
    pub truncated: bool,
    /// What each side did in the fight.
    pub stats: [CombatStats; 2],
}

/// Counters of one side in one combat (diagnostics: how a warband fights).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CombatStats {
    /// Attacks made.
    pub attacks: u16,
    /// Enemy minions destroyed by this side's Venomous / Poisonous hits.
    pub venom_kills: u16,
    /// Own Divine Shields lost (each one absorbed a hit).
    pub shields_lost: u16,
    /// Own minions that died.
    pub deaths: u16,
    /// Own Deathrattles triggered, minions summoned, Reborn triggers.
    pub deathrattles: u16,
    pub summons: u16,
    pub reborns: u16,
    /// The side's Deity awakened.
    pub deity_wakes: u16,
    /// Y'Shaarj's Deathrattle: the largest minion it summoned back (Attack + Health).
    pub returned_max: u32,
}

fn prepare(mut board: Vec<Minion>) -> Vec<Minion> {
    for m in board.iter_mut() {
        m.attacked = false;
        m.avenge = 0;
        m.destroyed = false;
        m.killed_by = 0;
        m.attacks = 0;
        m.peak_atk = m.atk;
        m.peak_hp = m.hp;
    }
    board
}

impl Game {
    /// Simulate one combat. `b = None` means a ghost with the given board and tier.
    /// Persistent effects (hand cards, permanent buffs, counters) are applied to the real
    /// players; board changes stay in the combat copies.
    pub fn simulate_combat(
        &mut self,
        a: PlayerId,
        b: Option<PlayerId>,
        ghost: Option<(Vec<Minion>, u8)>,
    ) -> CombatOutcome {
        self.simulate_combat_mode(a, b, ghost, None)
    }

    /// Continue a visible snapshot immediately before the first normal attack.
    /// Start-of-Combat changes are already in the imported stats and must not run
    /// twice. `first` is the observed attack side, not a fresh random decision.
    /// Hidden opening-death/queued-effect memories are not reconstructed here.
    pub fn simulate_combat_after_start(
        &mut self,
        a: PlayerId,
        b: Option<PlayerId>,
        ghost: Option<(Vec<Minion>, u8)>,
        first: Side,
    ) -> CombatOutcome {
        assert!(first < 2, "combat side must be 0 or 1");
        self.simulate_combat_mode(a, b, ghost, Some(first))
    }

    fn simulate_combat_mode(
        &mut self,
        a: PlayerId,
        b: Option<PlayerId>,
        ghost: Option<(Vec<Minion>, u8)>,
        observed_first: Option<Side>,
    ) -> CombatOutcome {
        let board_a = prepare(self.players[a].board.clone());
        let (board_b, ghost_tier) = match (b, ghost) {
            (Some(pb), _) => (prepare(self.players[pb].board.clone()), 0),
            (None, Some((bd, t))) => (prepare(bd), t),
            (None, None) => (vec![], 1),
        };
        let mut ctx = Ctx::new(self, [board_a, board_b], [Some(a), b], true);
        ctx.frame();
        ctx.cs[0].tier = ctx.g.players[a].tier;
        ctx.cs[1].tier = match b {
            Some(pb) => ctx.g.players[pb].tier,
            None => ghost_tier,
        };
        if b.is_none() {
            ctx.ghost_of[1] = ctx.g.ghost_player.take();
        }
        ctx.first = match observed_first {
            Some(side) => side,
            None => first_attacker(&mut ctx),
        };
        if observed_first.is_some() {
            restore_visible_combat_memory(&mut ctx);
        }
        let out = run(&mut ctx, observed_first.is_none());
        let frames = std::mem::take(&mut ctx.frames);
        let log = format!(
            "combat p{a} vs {:?}: winner {} dmg {}{}",
            b,
            out.winner,
            out.damage,
            if out.truncated { " (truncated)" } else { "" }
        );
        drop(ctx);
        self.last_frames = frames;
        self.log(|| log);
        out
    }

    /// Apply a combat result to heroes and record it.
    pub fn apply_combat_result(&mut self, a: PlayerId, b: Option<PlayerId>, out: CombatOutcome) {
        let alive = match self.turn_alive {
            0 => self.alive_count(),
            n => n,
        };
        let cap = self.cfg.damage_cap(self.turn, alive);
        let dmg = match cap {
            Some(c) => out.damage.min(c),
            None => out.damage,
        };
        for (side, p) in [(0, Some(a)), (1, b)] {
            if let Some(p) = p {
                let st = &mut self.players[p].stats;
                st.last_deity = out.stats[side].deity_wakes > 0;
                st.last_returned_max = out.stats[side].returned_max as i32;
            }
        }
        let set_result = |p: &mut crate::state::Player, won: bool, lost: bool| {
            p.stats.won_last = won;
            p.stats.lost_last = lost;
            p.stats.tied_last = !won && !lost;
            p.stats.last_damage = (won as i32 - lost as i32) * dmg;
            p.stats.streak = match (won, lost) {
                (true, _) => p.stats.streak.max(0) + 1,
                (_, true) => p.stats.streak.min(0) - 1,
                _ => 0,
            };
            if won {
                p.stats.combats_won += 1;
            }
            if lost {
                p.stats.combats_lost += 1;
            }
        };
        match out.winner {
            0 => {
                set_result(&mut self.players[a], true, false);
                if let Some(pb) = b {
                    set_result(&mut self.players[pb], false, true);
                    self.damage_hero_after_combat(pb, dmg);
                }
            }
            1 => {
                set_result(&mut self.players[a], false, true);
                if let Some(pb) = b {
                    set_result(&mut self.players[pb], true, false);
                }
                self.damage_hero_after_combat(a, dmg);
            }
            _ => {
                set_result(&mut self.players[a], false, false);
                if let Some(pb) = b {
                    set_result(&mut self.players[pb], false, false);
                }
            }
        }
        if self.cfg.experimental_mode {
            self.discount_buddy(a, out.winner == 1);
            if let Some(b) = b {
                self.discount_buddy(b, out.winner == 0);
            }
        }
        self.truncated_combats += out.truncated as u32;
        self.last_combats.push(CombatRecord {
            turn: self.turn,
            a,
            b,
            winner: out.winner,
            damage: if out.winner == 2 { 0 } else { dmg },
            truncated: out.truncated,
            frames: std::mem::take(&mut self.last_frames),
        });
    }

    /// The loser's minions are all dead at this point: only player-level effects (hero
    /// power, trinkets, quests...) can react, not the warband.
    fn damage_hero_after_combat(&mut self, p: PlayerId, dmg: i32) {
        let mut ctx = Ctx::new(self, [vec![], vec![]], [Some(p), None], false);
        ctx.hero_damage_ex(0, dmg, true);
    }
}

fn run(ctx: &mut Ctx, initialize: bool) -> CombatOutcome {
    if initialize {
        start_of_combat(ctx);
        ctx.flush_waiting();
    }
    ctx.frame();
    // Decided once, before Start of Combat.
    let mut side = ctx.first;
    let mut attacks = 0u32;
    let mut truncated = false;
    // Consecutive attack turns in which nothing attacked.
    let mut stalls = 0u32;
    loop {
        if ctx.boards[0].is_empty() || ctx.boards[1].is_empty() {
            break;
        }
        // A safety limit already cut death resolution short: the boards may hold minions
        // that can neither attack nor be attacked, so stop here (the result is a truncated
        // tie either way) instead of spinning.
        if ctx.truncated {
            break;
        }
        let can0 = ctx.boards[0].iter().any(|m| m.alive() && m.atk > 0);
        let can1 = ctx.boards[1].iter().any(|m| m.alive() && m.atk > 0);
        if !can0 && !can1 {
            break;
        }
        if attacks >= ctx.g.cfg.max_combat_attacks {
            truncated = true;
            break;
        }
        if let Some(att) = next_attacker(ctx, side)
            && attack_turn(ctx, side, att) > 0
        {
            attacks += 1;
            stalls = 0;
            ctx.frame();
        } else {
            stalls += 1;
        }
        // A full cycle without an attack (an attacker exists but nothing can be attacked,
        // e.g. an injected board holding a minion at 0 Health): nothing can change any more.
        if stalls >= 2 {
            // Stealth-only warbands can't be attacked: when that is why nobody attacked, the tie is
            // the rule's result, not a cutoff.
            let hidden = |s: usize| {
                ctx.boards[s]
                    .iter()
                    .all(|m| !m.alive() || m.has(Kw::STEALTH))
            };
            let armed = |s: usize| ctx.boards[s].iter().any(|m| m.alive() && m.atk > 0);
            let by_rule = (0..2).all(|s| !armed(s) || hidden(1 - s));
            truncated = !by_rule;
            break;
        }
        side = 1 - side;
    }
    // A safety limit cutting resolution short makes the result unreliable.
    truncated |= ctx.truncated;
    let alive = [!ctx.boards[0].is_empty(), !ctx.boards[1].is_empty()];
    let (winner, damage) = if truncated || alive[0] == alive[1] {
        (2, 0)
    } else {
        let w = if alive[0] { 0 } else { 1 };
        let dmg = ctx.tier(w) as i32 + ctx.boards[w].iter().map(|m| m.tier as i32).sum::<i32>();
        (w as u8, dmg)
    };
    CombatOutcome {
        winner,
        damage,
        truncated,
        stats: ctx.cstats,
    }
}

/// Only memory implied directly by the still-visible board can be restored.
/// Do not rerun buffs or invent minions that died during the opening effects.
fn restore_visible_combat_memory(ctx: &mut Ctx) {
    let bird = crate::cardid!("BG26_805");
    for side in 0..2 {
        ctx.cs[side].beast_atk_aura = ctx.boards[side]
            .iter()
            .filter(|m| crate::cards::db().normal_of(m.card) == bird)
            .map(|m| if m.golden { 2 } else { 1 })
            .sum();
        if let Some(deity) = ctx.player_ref(side).and_then(|p| p.deity.as_ref()) {
            let normal = crate::cards::db().normal_of(deity.card);
            if let Some(m) = ctx.boards[side]
                .iter()
                .find(|m| crate::cards::db().normal_of(m.card) == normal)
            {
                ctx.cs[side].deity_awakened = true;
                ctx.cs[side].deity_eid = m.eid;
            }
        }
    }
}

/// More minions attacks first; on a tie the higher Tavern Tier; then random.
fn first_attacker(ctx: &mut Ctx) -> Side {
    let (a, b) = (ctx.boards[0].len(), ctx.boards[1].len());
    let (ta, tb) = (ctx.cs[0].tier, ctx.cs[1].tier);
    if a != b {
        usize::from(b > a)
    } else if ta != tb {
        usize::from(tb > ta)
    } else {
        ctx.g.rng.below(2)
    }
}

/// Start of Combat, stage by stage (see the order below); the side that attacks first resolves
/// first within each stage.
fn start_of_combat(ctx: &mut Ctx) {
    let first = ctx.first;
    let order = [first, 1 - first];
    // Order (official 30.6 notes): Trinkets, Quest Rewards and Quests first, then Hero Powers,
    // Tavern-spell effects, minions in hand, minions on the board. Within each group the first
    // attacker's side goes first.
    for s in order {
        let trinkets: Vec<_> = ctx
            .player_ref(s)
            .map(|p| {
                p.trinkets
                    .iter()
                    .chain(&p.rewards)
                    .copied()
                    .chain(p.quests.iter().map(|q| q.card))
                    .collect()
            })
            .unwrap_or_default();
        for t in trinkets {
            if let Some(fx) = script(t).and_then(|sc| sc.start_of_combat) {
                let who = ctx.who(s);
                let name = &crate::cards::db().get(t).name;
                ctx.g.log(|| format!("{who} Start of Combat: {name}"));
                let prev_src = ctx.src.replace(t);
                fx(ctx, me0(s, t));
                ctx.src = prev_src;
                ctx.resolve_deaths();
            }
        }
    }
    for s in order {
        let powers: Vec<_> = ctx
            .player_ref(s)
            .map(|p| p.power_cards().collect())
            .unwrap_or_default();
        for hp in powers {
            let Some(fx) = script(hp).and_then(|sc| sc.start_of_combat) else {
                continue;
            };
            let who = ctx.who(s);
            let name = &crate::cards::db().get(hp).name;
            ctx.g.log(|| format!("{who} Start of Combat: {name}"));
            let prev_src = ctx.src.replace(hp);
            fx(ctx, me0(s, hp));
            ctx.src = prev_src;
            ctx.resolve_deaths();
        }
    }
    // Player effects with Start of Combat ("Start of Combat:" Tavern spells). NEXT_COMBAT
    // effects run once per stack and are used up; others run once with `data` = stacks.
    for s in order {
        let Some(p) = ctx.owner[s] else { continue };
        let effects: Vec<(crate::cards::CardId, i32)> = ctx.g.players[p]
            .effects
            .iter()
            .map(|x| (x.card, x.stacks))
            .collect();
        for (card, stacks) in effects {
            let Some(sc) = script(card) else { continue };
            let Some(fx) = sc.start_of_combat else {
                continue;
            };
            let once = sc.has_flag(flag::NEXT_COMBAT);
            if once {
                ctx.g.players[p].effects.retain(|x| x.card != card);
            }
            for _ in 0..if once { stacks.max(0) } else { 1 } {
                let mut me = me0(s, card);
                me.data = stacks;
                let prev_src = ctx.src.replace(card);
                fx(ctx, me);
                ctx.src = prev_src;
                ctx.resolve_deaths();
            }
        }
    }
    // Cards in hand with "Start of Combat: if this is in your hand..." (left to right).
    for s in order {
        let hand: Vec<(crate::types::EntityId, crate::cards::CardId, bool)> = ctx
            .player_ref(s)
            .map(|p| {
                p.hand
                    .iter()
                    // A locked or held card takes part in nothing.
                    .filter(|h| h.locked == 0)
                    .filter_map(|h| h.item.as_minion().map(|m| (h.hid, m.card, m.golden)))
                    .filter(|(_, c, _)| {
                        script(*c).is_some_and(|sc| sc.hand_start_of_combat.is_some())
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (hid, card, golden) in hand {
            // A card summoned from hand "for this combat only" is not in hand any more.
            if ctx.cs[s].from_hand.contains(&hid) {
                continue;
            }
            let fx = script(card).and_then(|sc| sc.hand_start_of_combat).unwrap();
            let who = ctx.who(s);
            let name = &crate::cards::db().get(card).name;
            ctx.g
                .log(|| format!("{who} Start of Combat (in hand): {name}"));
            let me = Me {
                side: s,
                eid: hid,
                card,
                golden,
                opt: 0,
                pos: 0,
                data: 0,
            };
            let prev_src = ctx.src.replace(card);
            fx(ctx, me);
            ctx.src = prev_src;
            ctx.resolve_deaths();
        }
    }
    // The minions on the board when their stage begins. One that an earlier stage summoned (Evil
    // Twin's copy) is among them and runs its own Start of Combat; one summoned during this stage
    // is not.
    // Both sides' minions are taken before either side's stage runs: a minion that comes
    // onto the second side's board during the first side's stage (a Reborn after Start of
    // Combat damage) is "summoned during the minions' stage" too, whichever side it is on.
    let stage = order.map(|s| ctx.board_hook_snapshot(s));
    for (s, minions) in order.into_iter().zip(stage) {
        ctx.run_board_hook_on(s, minions, |sc| sc.start_of_combat, 1, "Start of Combat");
        ctx.resolve_deaths();
    }
}

fn me0(side: Side, card: crate::cards::CardId) -> Me {
    Me {
        side,
        eid: 0,
        card,
        golden: false,
        opt: 0,
        pos: 0,
        data: 0,
    }
}

/// Leftmost minion that hasn't attacked this cycle and has Attack > 0; when everyone has
/// attacked, the cycle restarts from the left.
fn next_attacker(ctx: &mut Ctx, side: Side) -> Option<EntityId> {
    let b = &mut ctx.boards[side];
    if let Some(m) = b.iter().find(|m| !m.attacked && m.atk > 0 && m.alive()) {
        return Some(m.eid);
    }
    if !b.iter().any(|m| m.atk > 0 && m.alive()) {
        return None;
    }
    for m in b.iter_mut() {
        m.attacked = false;
    }
    b.iter().find(|m| m.atk > 0 && m.alive()).map(|m| m.eid)
}

fn has_taunt(m: &Minion) -> bool {
    m.has(Kw::TAUNT)
        || (m.has(Kw::DIVINE_SHIELD)
            && script(m.card).is_some_and(|s| s.has_flag(flag::TAUNT_WHILE_SHIELDED)))
}

/// Random legal target on `side`: Taunt minions first; Stealth minions can't be attacked. If only
/// Stealth minions remain there is no target and the attacker doesn't attack; a combat where nobody
/// can attack any more ends as a tie (`run`'s stall guard).
pub fn pick_target(ctx: &mut Ctx, side: Side) -> Option<EntityId> {
    let b = &ctx.boards[side];
    // Count first, then draw one index and take the k-th candidate: no allocation, and the
    // same random draw as picking from a collected list.
    let visible = |m: &Minion| m.alive() && !m.has(Kw::STEALTH);
    let n_taunt = b.iter().filter(|m| visible(m) && has_taunt(m)).count();
    let n_visible = b.iter().filter(|m| visible(m)).count();
    let (n, f): (usize, fn(&Minion) -> bool) = if n_taunt > 0 {
        (n_taunt, |m| {
            m.alive() && !m.has(Kw::STEALTH) && has_taunt(m)
        })
    } else if n_visible > 0 {
        (n_visible, |m| m.alive() && !m.has(Kw::STEALTH))
    } else {
        return None;
    };
    let k = ctx.g.rng.below(n);
    ctx.boards[side]
        .iter()
        .filter(|m| f(m))
        .nth(k)
        .map(|m| m.eid)
}

fn attack_count(m: &Minion) -> u32 {
    if m.has(Kw::MEGA_WINDFURY) {
        4
    } else if m.has(Kw::WINDFURY) {
        2
    } else {
        1
    }
}

/// One attacker's turn: 1 attack, 2 with Windfury, 4 with Mega-Windfury (Windfury gained during the
/// turn adds its attacks; losing it doesn't remove any; each attack picks a new target). Returns
/// the number of attacks made.
fn attack_turn(ctx: &mut Ctx, side: Side, attacker: EntityId) -> u32 {
    let Some(mut n) = ctx.minion(attacker).map(attack_count) else {
        return 0;
    };
    ctx.cs[side].last_attacker = attacker;
    let mut k = 0;
    let mut made = 0;
    while k < n {
        k += 1;
        if !ctx.minion(attacker).is_some_and(|m| m.alive() && m.atk > 0) {
            break;
        }
        let Some(target) = pick_target(ctx, 1 - side) else {
            break;
        };
        do_attack(ctx, side, attacker, target);
        made += 1;
        ctx.resolve_deaths();
        if ctx.boards[1 - side].is_empty() {
            break;
        }
        if let Some(m) = ctx.minion(attacker) {
            n = n.max(attack_count(m));
        }
    }
    if let Some(m) = ctx.minion_mut(attacker) {
        m.attacked = true;
    }
    made
}

/// "Attack immediately" (Start of Combat effects): a single attack outside the rotation.
pub fn attack_now(ctx: &mut Ctx, side: Side, attacker: EntityId) {
    let Some(card) = ctx
        .minion(attacker)
        .filter(|m| m.alive() && m.atk > 0)
        .map(|m| m.card)
    else {
        return;
    };
    ctx.guard(crate::guard::Site::AttackNow, u32::from(card.0));
    if let Some(target) = pick_target(ctx, 1 - side) {
        do_attack(ctx, side, attacker, target);
        ctx.resolve_deaths();
    }
}

/// Resolve one attack: Stealth drops, Rally and "whenever attacks" triggers fire, then both
/// minions deal damage simultaneously (Cleave also hits the defender's neighbours).
pub fn do_attack(ctx: &mut Ctx, side: Side, attacker: EntityId, defender: EntityId) {
    let card = ctx
        .minion(attacker)
        .map_or(u32::MAX, |m| u32::from(m.card.0));
    ctx.guard(crate::guard::Site::Attack, card);
    ctx.cs[side].attacks_made += 1;
    if let Some(m) = ctx.minion_mut(attacker) {
        m.remove(Kw::STEALTH);
    }
    {
        let a = ctx.minion(attacker).map(|m| m.name()).unwrap_or("?");
        let d = ctx.minion(defender).map(|m| m.name()).unwrap_or("?");
        let who = ctx.who(side);
        ctx.g.log(|| format!("{who}'s {a} attacks {d}"));
        if ctx.g.cfg.record_log {
            ctx.frame_attacks.push((attacker, defender));
        }
    }
    run_rally(ctx, side, attacker, defender);
    ctx.emit(Event::Attacks {
        side,
        attacker,
        defender,
    });
    let (Some(a), Some(d)) = (ctx.minion(attacker), ctx.minion(defender)) else {
        return;
    };
    if !a.alive() || !d.alive() {
        return;
    }
    let atk_a = a.atk;
    let atk_d = d.atk;
    // Wildfire Elemental: excess damage when it kills the defender (golden: both neighbours).
    static WILDFIRE: std::sync::OnceLock<crate::cards::CardId> = std::sync::OnceLock::new();
    let wf = *WILDFIRE.get_or_init(|| crate::cards::db().id("BGS_126"));
    let wildfire = (crate::cards::db().normal_of(a.card) == wf).then(|| {
        let shielded = d.has(Kw::DIVINE_SHIELD) && d.shield_hits > 0;
        (
            a.golden,
            if shielded { 0 } else { atk_a - d.hp },
            ctx.neighbors(defender),
        )
    });
    let cleave = a.has(Kw::CLEAVE);
    let immune_attacking = a.has(Kw::IMMUNE_ATTACKING);
    let splash = if cleave {
        ctx.neighbors(defender)
    } else {
        vec![]
    };
    // The exchange deals all its damage first, then the damage triggers fire; Cleave carries
    // Venomous to every target it hits.
    ctx.defer_hits = true;
    let venom = ctx.minion(attacker).is_some_and(|m| m.has(Kw::VENOMOUS));
    ctx.keep_venom = cleave && venom;
    let mut dealt = ctx.damage(attacker, defender, atk_a);
    for n in splash {
        dealt = dealt.saturating_add(ctx.damage(attacker, n, atk_a));
    }
    if ctx.keep_venom {
        ctx.keep_venom = false;
        if dealt > 0
            && let Some(m) = ctx.minion_mut(attacker)
        {
            m.remove(Kw::VENOMOUS);
        }
    }
    if !immune_attacking {
        ctx.damage(defender, attacker, atk_d);
    }
    ctx.defer_hits = false;
    ctx.flush_deferred();
    ctx.emit(Event::AttackDone { side, attacker });
    if let Some(m) = ctx.minion(attacker)
        && let Some(fx) = script(m.card).and_then(|s| s.after_attack)
    {
        let me = Me {
            side,
            eid: attacker,
            card: m.card,
            golden: m.golden,
            opt: 0,
            pos: 0,
            data: 0,
        };
        let prev_src = ctx.src.replace(me.card);
        fx(ctx, me);
        ctx.src = prev_src;
    }
    if let Some((golden, excess, mut adj)) = wildfire
        && excess > 0
        && ctx.minion(defender).is_some_and(|m| !m.alive())
    {
        if !golden && adj.len() > 1 {
            let i = ctx.g.rng.below(adj.len());
            adj = vec![adj[i]];
        }
        for n in adj {
            ctx.damage(attacker, n, excess);
        }
    }
}

pub fn run_rally(ctx: &mut Ctx, side: Side, attacker: EntityId, defender: EntityId) {
    let Some(m) = ctx.minion(attacker) else {
        return;
    };
    let host = m.card;
    let golden = m.golden;
    let cards: Vec<_> = std::iter::once(m.card)
        .chain(m.attached.iter().copied())
        .collect();
    for c in cards {
        let Some(fx) = script(c).and_then(|s| s.rally) else {
            continue;
        };
        if !ctx.on_board(attacker) {
            break;
        }
        let g = if c == host {
            golden
        } else {
            crate::cards::db().get(c).is_golden()
        };
        let me = Me {
            side,
            eid: attacker,
            card: c,
            golden: g,
            opt: 0,
            pos: 0,
            data: 0,
        };
        for _ in 0..ctx.rally_mult(side) {
            let prev_src = ctx.src.replace(me.card);
            fx(ctx, me, defender);
            ctx.src = prev_src;
        }
    }
}
