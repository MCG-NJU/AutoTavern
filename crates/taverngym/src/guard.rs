//! Stack guard for the engine's re-entrant paths.
//!
//! A game worker that runs out of stack dies of a bare SIGSEGV inside the Python extension: a
//! cdylib installs no overflow handler, so not even Rust's "has overflowed its stack" appears.
//! The engine's re-entrant entry points (events, summons, deaths,
//! Battlecries, casts, cards added to the hand, Discovers, attacks) call [`Ctx::guard`]: it notes
//! the entry point in the context's trail and checks the stack against a floor. Below the floor it
//! panics with the trail - in a runaway recursion, the cycle - and writes the game to a JSON file
//! (directory `TAVERNGYM_GUARD_DIR`, else the current one).

use crate::cards::{CardId, db};
use crate::engine::Ctx;
use std::cell::Cell;

/// Entry points a context remembers (the last ones entered).
pub const TRAIL: usize = 64;
/// Stack kept free under the floor on a thread of known size (`workers::spawn`).
const MARGIN: usize = 8 << 20;
/// Budget of a thread of unknown size, from its shallowest engine context (the main and
/// Python threads have 8 MiB on Linux).
const UNKNOWN: usize = 6 << 20;

thread_local! {
    /// (top of this thread's stack as far as known, budget below it); top 0 = not known.
    static STACK: Cell<(usize, usize)> = const { Cell::new((0, UNKNOWN)) };
}

/// The current stack address (the stack grows down on every target we build for).
#[inline(always)]
fn here() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

/// Call first thing on a thread spawned with a stack of `size` bytes.
pub fn thread_started(size: usize) {
    STACK.with(|s| s.set((here(), size.saturating_sub(MARGIN))));
}

/// The lowest stack address engine calls on this thread may reach (read by `Ctx::new`).
pub fn floor() -> usize {
    STACK.with(|s| {
        let (mut top, budget) = s.get();
        let now = here();
        if top < now {
            // A thread of unknown size: measure from its shallowest context.
            top = now;
            s.set((top, budget));
        }
        top.saturating_sub(budget)
    })
}

/// A guarded entry point; `arg` is a card id, an event kind bit or a side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Site {
    Event,
    Summon,
    Deathrattles,
    Deaths,
    Damage,
    Battlecry,
    TriggeredBattlecry,
    AddToHand,
    GiveCard,
    Triples,
    Cast,
    Discover,
    Magnetize,
    Attack,
    AttackNow,
}

const SITES: [(Site, &str); 15] = [
    (Site::Event, "event"),
    (Site::Summon, "summon"),
    (Site::Deathrattles, "deathrattles"),
    (Site::Deaths, "deaths"),
    (Site::Damage, "damage"),
    (Site::Battlecry, "battlecry"),
    (Site::TriggeredBattlecry, "triggered battlecry"),
    (Site::AddToHand, "add to hand"),
    (Site::GiveCard, "give card"),
    (Site::Triples, "triples"),
    (Site::Cast, "cast"),
    (Site::Discover, "discover"),
    (Site::Magnetize, "magnetize"),
    (Site::Attack, "attack"),
    (Site::AttackNow, "attack now"),
];

/// `event::ev` bits in order.
const EVENTS: [&str; 25] = [
    "Played",
    "Summoned",
    "Bought",
    "Sold",
    "SpellCast",
    "Refreshed",
    "GoldSpent",
    "Attacks",
    "Damaged",
    "ShieldLost",
    "Died",
    "Killed",
    "Reborn",
    "HeroDamaged",
    "CardAdded",
    "Discarded",
    "Magnetized",
    "Upgraded",
    "Battlecry",
    "Deathrattle",
    "CardPlayed",
    "GemPlayed",
    "Consumed",
    "Discovered",
    "AttackDone",
];

fn describe((site, arg): (u8, u32)) -> String {
    let name = SITES.get(site as usize).map_or("?", |s| s.1);
    let what = match SITES.get(site as usize).map(|s| s.0) {
        Some(Site::Event) => EVENTS
            .get(arg.trailing_zeros() as usize)
            .map_or_else(|| format!("bit {arg:#x}"), |e| e.to_string()),
        Some(Site::Deaths) => format!("depth {arg}"),
        Some(Site::Triples) => format!("side {arg}"),
        _ if arg == u32::MAX => return name.to_string(),
        _ => match u16::try_from(arg)
            .ok()
            .and_then(|c| db().try_get(CardId(c)))
        {
            Some(c) => format!("{} {}", c.id, c.name),
            None => format!("{arg}"),
        },
    };
    format!("{name} {what}")
}

impl Ctx<'_> {
    /// Note an engine entry point; below the stack floor, panic with the trail (module
    /// docs).
    #[inline(always)]
    pub fn guard(&mut self, site: Site, arg: u32) {
        self.trail[self.trail_n as usize % TRAIL] = (site as u8, arg);
        self.trail_n = self.trail_n.wrapping_add(1);
        if here() < self.stack_floor {
            self.guard_tripped();
        }
    }

    #[cold]
    #[inline(never)]
    fn guard_tripped(&self) -> ! {
        let (top, budget) = STACK.with(Cell::get);
        let used = top.saturating_sub(here()) as f64 / (1 << 20) as f64;
        let n = self.trail_n as usize;
        let steps: Vec<String> = (n.saturating_sub(TRAIL)..n)
            .map(|i| describe(self.trail[i % TRAIL]))
            .collect();
        let mut counts: Vec<(&String, usize)> = vec![];
        for s in &steps {
            match counts.iter_mut().find(|(k, _)| *k == s) {
                Some((_, c)) => *c += 1,
                None => counts.push((s, 1)),
            }
        }
        counts.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        let most: Vec<String> = counts
            .iter()
            .take(8)
            .map(|(s, c)| format!("{c}x {s}"))
            .collect();
        let sides: Vec<String> = self
            .owner
            .iter()
            .map(
                |o| match o.and_then(|p| self.g.players.get(p).map(|pl| (p, pl.hero))) {
                    Some((p, Some(h))) => format!("p{p} {}", db().get(h).name),
                    Some((p, None)) => format!("p{p}"),
                    None => "ghost".to_string(),
                },
            )
            .collect();
        let phase = if self.combat { "combat" } else { "recruit" };
        let file = self.guard_dump(&steps);
        panic!(
            "engine recursion guard: {used:.1} MiB of stack in use (budget {} MiB), {n} \
             entries in this context, turn {} {phase}, sides {}; most frequent of the last {}: \
             {}; oldest first: {}; game written to {file}",
            budget >> 20,
            self.g.turn,
            sides.join(" / "),
            steps.len(),
            most.join(", "),
            steps.join(" > "),
        );
    }

    /// The game, the context's boards and the trail as JSON; the file name, or why not.
    fn guard_dump(&self, steps: &[String]) -> String {
        fn json<T: serde::Serialize + ?Sized>(x: &T) -> String {
            serde_json::to_string(x).unwrap_or_else(|e| format!("{:?}", e.to_string()))
        }
        let dir = std::env::var_os("TAVERNGYM_GUARD_DIR").unwrap_or_else(|| ".".into());
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = std::path::Path::new(&dir).join(format!(
            "taverngym-guard-{}-{nanos}.json",
            std::process::id()
        ));
        let body = format!(
            "{{\"combat\":{},\"turn\":{},\"owner\":{},\"trail\":{},\"boards\":{},\"game\":{}}}",
            self.combat,
            self.g.turn,
            json(&self.owner),
            json(steps),
            json(&self.boards),
            json(&*self.g),
        );
        match std::fs::write(&path, body) {
            Ok(()) => path.display().to_string(),
            Err(e) => format!("nothing ({}: {e})", path.display()),
        }
    }
}
