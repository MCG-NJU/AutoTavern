//! The Wheel of Yogg-Saron: six outcomes. Weights are the community rules digest maintained to Aug
//! 2026 (not an official probability table): five at 19%, Rod of Roasting at 5%. "Doubled" (golden
//! Acolyte of Yogg-Saron) spins once and doubles the outcome.

use super::*;

/// (outcome, weight in percent) -- unofficial, see the module doc.
const WHEEL: [(Outcome, u32); 6] = [
    (Outcome::Mysterybox, 19),
    (Outcome::HandOfFate, 19),
    (Outcome::CurseOfFlesh, 19),
    (Outcome::MindflayerGoggles, 19),
    (Outcome::DevouringHunger, 19),
    (Outcome::RodOfRoasting, 5),
];

#[derive(Clone, Copy)]
enum Outcome {
    /// A random Tavern minion becomes Golden.
    Mysterybox,
    /// Get 2 random Darkmoon Prizes.
    HandOfFate,
    /// A random friendly minion gains the stats of another friendly minion.
    CurseOfFlesh,
    /// Cast 4 random Tavern spells.
    MindflayerGoggles,
    /// Consume the Tavern's minions, give their stats to friendly minions, then Refresh.
    DevouringHunger,
    /// Cast "+10/+10" on random minions until it lands on the hero or the bartender.
    RodOfRoasting,
}

pub fn wheel_of_yogg(c: &mut Ctx, side: Side, doubled: bool) {
    let total: u32 = WHEEL.iter().map(|x| x.1).sum();
    let mut r = c.g.rng.below(total as usize) as u32;
    let mut outcome = WHEEL[0].0;
    for (o, w) in WHEEL {
        if r < w {
            outcome = o;
            break;
        }
        r -= w;
    }
    let k = if doubled { 2 } else { 1 };
    match outcome {
        Outcome::Mysterybox => {
            for _ in 0..k {
                let cands: Vec<EntityId> = c
                    .tavern_minions(side)
                    .into_iter()
                    .filter(|e| {
                        c.find_any(*e)
                            .is_some_and(|m| !m.golden && db().get(m.card).golden.is_some())
                    })
                    .collect();
                if let Some(&t) = c.g.rng.pick(&cands)
                    && let Some(m) = c.find_any_mut(t)
                {
                    super::spells::make_golden(m);
                }
            }
        }
        Outcome::HandOfFate => {
            let tier = super::darkmoon::prize_tier_for_turn(c.g.turn);
            let prizes: Vec<CardId> = db()
                .iter()
                .filter(|d| d.prize_tier == tier && d.normal.is_none())
                .map(|d| d.cid)
                .collect();
            for _ in 0..2 * k {
                if let Some(&x) = c.g.rng.pick(&prizes) {
                    c.give_card(side, x);
                }
            }
        }
        Outcome::CurseOfFlesh => {
            let f = c.friendly(side);
            if f.len() >= 2 {
                let picks = c.g.rng.sample_indices(f.len(), 2);
                let (to, from) = (f[picks[0]], f[picks[1]]);
                let (a, h) = c.minion(from).map_or((0, 0), |m| (m.atk, m.hp));
                c.buff(to, a * k, h * k);
            }
        }
        Outcome::MindflayerGoggles => {
            for _ in 0..4 * k {
                cast_random_spell(c, side);
            }
        }
        Outcome::DevouringHunger => {
            let Some(p) = c.owner[side] else { return };
            let tav = c.tavern_minions(side);
            for sid in tav {
                let Some(m) = c.take_from_shop(side, sid) else {
                    continue;
                };
                c.g.pool.return_minion(&m);
                let f = c.friendly(side);
                if let Some(&t) = c.g.rng.pick(&f) {
                    c.buff(t, m.atk.saturating_mul(k), m.hp.saturating_mul(k));
                }
            }
            let _ = p;
            c.refresh_shop(side);
        }
        Outcome::RodOfRoasting => {
            for _ in 0..100 {
                let mut cands = c.friendly(side);
                cands.extend(c.tavern_minions(side));
                // Two more "slots": your hero and the bartender stop the Rod.
                let n = cands.len() + 2;
                let i = c.g.rng.below(n);
                if i >= cands.len() {
                    break;
                }
                c.buff(cands[i], 10 * k, 10 * k);
            }
        }
    }
}

/// Cast a random Tavern spell (Tier <= yours) on a random legal target.
pub fn cast_random_spell(c: &mut Ctx, side: Side) {
    let tier = c.tier(side);
    let Some(s) = c.random_spell(|d| d.tier <= tier) else {
        return;
    };
    let spec = effect_target_spec(s).unwrap_or(TargetSpec::None);
    let t = if spec == TargetSpec::None {
        Some(0)
    } else {
        let opts = c.legal_targets(side, spec, 0);
        c.g.rng.pick(&opts).copied()
    };
    if let Some(t) = t {
        c.you_cast(side, s, 0, t, 0);
    }
}
