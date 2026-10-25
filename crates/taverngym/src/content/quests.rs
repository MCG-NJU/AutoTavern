//! Quests and Quest Rewards (framework: `Ctx::offer_quests` / `quest_progress`).
//! Quest scripts listen to events while active and advance their own progress. Rewards act
//! like trinkets (events, turn hooks, Start of Combat); a few are engine checks
//! (`has_reward`: trigger multipliers, prices, triples, spell casts).

use super::trinkets_p6::shown_left;
use super::*;

/// Quests and Rewards in the current pool (hsdata 253216 tags 2466 / 2641; matches the hsbg.cards
/// 36.6.2 pool lists).
const POOL_QUESTS: [&str; 11] = [
    "BG24_Quest_114",
    "BG24_Quest_123",
    "BG24_Quest_124",
    "BG24_Quest_126",
    "BG24_Quest_151",
    "BG24_Quest_313",
    "BG24_Quest_314",
    "BG24_Quest_352",
    "BG27_Quest_801",
    "BG27_Quest_802",
    "BG28_Quest_500",
];
const POOL_REWARDS: [&str; 38] = [
    "BG24_Reward_107",
    "BG24_Reward_109",
    "BG24_Reward_111",
    "BG24_Reward_129",
    "BG24_Reward_130",
    "BG24_Reward_134",
    "BG24_Reward_135",
    "BG24_Reward_138",
    "BG24_Reward_306",
    "BG24_Reward_313",
    "BG24_Reward_350",
    "BG24_Reward_361",
    "BG24_Reward_363",
    "BG24_Reward_718",
    "BG24_Reward_719",
    "BG27_Reward_504",
    "BG27_Reward_802",
    "BG27_Reward_803",
    "BG27_Reward_806",
    "BG27_Reward_811",
    "BG27_Reward_812",
    "BG28_Reward_500",
    "BG28_Reward_501",
    "BG28_Reward_504",
    "BG28_Reward_506",
    "BG28_Reward_508",
    "BG28_Reward_509",
    "BG28_Reward_510",
    "BG28_Reward_513",
    "BG28_Reward_514",
    "BG28_Reward_515",
    "BG33_Reward_010",
    "BG33_Reward_011",
    "BG33_Reward_012",
    "BG33_Reward_013",
    "BG33_Reward_014",
    "BG33_Reward_015",
    "BG33_Reward_020",
];

/// Client tags of the pool Rewards (hsdata 253932): tag 2467, the percentage by which the Reward
/// scales the base goal of the Quest it is paired with (a stronger Reward makes a longer Quest),
/// and tag 2653, its offer weight (a Reward without the tag has 100).
const REWARD_TAGS: [(&str, i32, u64); 38] = [
    ("BG24_Reward_107", 70, 100),
    ("BG24_Reward_109", 170, 33),
    ("BG24_Reward_111", 110, 100),
    ("BG24_Reward_129", 220, 100),
    ("BG24_Reward_130", 210, 100),
    ("BG24_Reward_134", 130, 100),
    ("BG24_Reward_135", 135, 33),
    ("BG24_Reward_138", 110, 100),
    ("BG24_Reward_306", 115, 100),
    ("BG24_Reward_313", 100, 33),
    ("BG24_Reward_350", 250, 33),
    ("BG24_Reward_361", 120, 100),
    ("BG24_Reward_363", 120, 100),
    ("BG24_Reward_718", 110, 100),
    ("BG24_Reward_719", 120, 100),
    ("BG27_Reward_504", 80, 100),
    ("BG27_Reward_802", 175, 100),
    ("BG27_Reward_803", 175, 100),
    ("BG27_Reward_806", 130, 33),
    ("BG27_Reward_811", 175, 33),
    ("BG27_Reward_812", 70, 100),
    ("BG28_Reward_500", 120, 100),
    ("BG28_Reward_501", 250, 100),
    ("BG28_Reward_504", 90, 100),
    ("BG28_Reward_506", 140, 100),
    ("BG28_Reward_508", 75, 100),
    ("BG28_Reward_509", 75, 100),
    ("BG28_Reward_510", 180, 100),
    ("BG28_Reward_513", 115, 100),
    ("BG28_Reward_514", 175, 100),
    ("BG28_Reward_515", 170, 100),
    ("BG33_Reward_010", 170, 33),
    ("BG33_Reward_011", 115, 100),
    ("BG33_Reward_012", 190, 100),
    ("BG33_Reward_013", 190, 100),
    ("BG33_Reward_014", 100, 100),
    ("BG33_Reward_015", 170, 100),
    ("BG33_Reward_020", 85, 100),
];

fn reward_tags(reward: CardId) -> (i32, u64) {
    let id = db().get(reward).id.as_str();
    REWARD_TAGS
        .iter()
        .find(|x| x.0 == id)
        .map_or((100, 100), |x| (x.1, x.2))
}

/// The offer weight of a Reward (Quest offers, Ethereal Evidence).
pub fn reward_weight(reward: CardId) -> u64 {
    reward_tags(reward).1
}

/// Client tags 2643..2652 of the pool Quests and Rewards (hsdata 253932): per minion type,
/// the percentage applied to the goal in a game WITHOUT that type - a Quest that leans on a
/// type is shorter when the type is missing (Summon minions without Beasts), and so is the
/// Quest of a Reward that is weaker without it (Evil Twin, 120, is the one that gets longer:
/// no Murlocs, no Venomous). The official notes say "certain Quests are adjusted based on
/// the minion types in the pool"; the reading of the ten tags - Beast, Demon, Dragon, Mech,
/// Murloc, Pirate, Elemental, Quilboar, Naga, Undead, applied when the type is absent - is
/// the simulator's. Naga are in no lobby of this patch, so
/// their rows always apply.
const TYPE_GOAL_MODS: [(&str, &[(Races, i128)]); 11] = [
    // Assemble a Lineup: Summon N minions.
    (
        "BG24_Quest_114",
        &[
            (Races::BEAST, 90),
            (Races::DEMON, 90),
            (Races::MECH, 90),
            (Races::UNDEAD, 80),
        ],
    ),
    // Reenact the Murder: Have N friendly minions die.
    (
        "BG24_Quest_124",
        &[
            (Races::BEAST, 80),
            (Races::DEMON, 90),
            (Races::MECH, 90),
            (Races::UNDEAD, 80),
        ],
    ),
    // Round Up the Suspects: Destroy N enemy minions.
    (
        "BG27_Quest_802",
        &[
            (Races::BEAST, 80),
            (Races::DEMON, 90),
            (Races::MECH, 90),
            (Races::UNDEAD, 80),
        ],
    ),
    // Invite the Guests: Buy N minions.
    (
        "BG24_Quest_313",
        &[(Races::MURLOC, 90), (Races::PIRATE, 90)],
    ),
    // Dust for Prints: Add N cards to your hand.
    (
        "BG24_Quest_314",
        &[(Races::QUILBOAR, 70), (Races::NAGA, 70)],
    ),
    // Fill the Cauldron: Cast N spells.
    (
        "BG28_Quest_500",
        &[(Races::QUILBOAR, 85), (Races::NAGA, 85)],
    ),
    // Stolen Gold.
    (
        "BG24_Reward_109",
        &[(Races::BEAST, 90), (Races::DRAGON, 90), (Races::MECH, 90)],
    ),
    // Evil Twin.
    ("BG24_Reward_111", &[(Races::MURLOC, 120)]),
    // Bloodsoaked Tome.
    ("BG27_Reward_811", &[(Races::PIRATE, 80)]),
    // Beyond the Mirage.
    ("BG28_Reward_500", &[(Races::PIRATE, 85), (Races::NAGA, 85)]),
    // Temporal Tampering.
    ("BG28_Reward_501", &[(Races::PIRATE, 85)]),
];

/// The goal of `quest` when it is paired with `reward` in a game with the minion types
/// `lobby`: the Quest's base number (`QUEST_PROGRESS_TOTAL`) times the Reward's percentage,
/// times the percentages of the Quest and of the Reward for every type that is not in the
/// game (`TYPE_GOAL_MODS`), rounded once to the nearest number (halves up; the rounding is
/// an assumption).
pub fn quest_goal(lobby: Races, quest: CardId, reward: CardId) -> i32 {
    let base = db().get(quest).quest_goal.max(1) as i128;
    let (mut num, mut den) = (base * reward_tags(reward).0 as i128, 100i128);
    for card in [quest, reward] {
        let id = db().get(card).id.as_str();
        let mods = TYPE_GOAL_MODS.iter().find(|x| x.0 == id).map(|x| x.1);
        for (r, pct) in mods.into_iter().flatten() {
            if !lobby.intersects(*r) {
                num *= pct;
                den *= 100;
            }
        }
    }
    (((2 * num + den) / (2 * den)) as i32).max(1)
}

/// The minion types a Quest or a Reward is about, drawn from the game's types: two different ones
/// for Unlikely Duo, one for Scepter of Guidance and The Friends Along the Way, none (0) for any
/// other card. They are drawn when the pair is offered - the client shows them on the offered cards
/// - or when the card is gained another way.
pub fn draw_quest_types(c: &mut Ctx, card: CardId) -> u16 {
    match db().get(card).id.as_str() {
        "BG24_Quest_151" => {
            let a = random_lobby_type(c);
            let others: Vec<Races> = (0..Races::COUNT)
                .map(|i| Races(1 << i))
                .filter(|r| c.g.lobby_races.intersects(*r) && *r != a)
                .collect();
            let b = c.g.rng.pick(&others).copied().unwrap_or(a);
            a.0 | b.0
        }
        "BG24_Reward_134" | "BG27_Reward_812" => random_lobby_type(c).0,
        _ => 0,
    }
}

/// Rewards that count something or grow over time: Ethereal Evidence, which swaps its
/// Reward every turn, does not offer them (Cooked Book, Hidden Treasure Vault, Cycle of
/// Energy, Gift of the Golden Kobold, Smelting Chamber).
const COUNTING_REWARDS: [&str; 5] = [
    "BG24_Reward_306",
    "BG24_Reward_361",
    "BG28_Reward_504",
    "BG28_Reward_508",
    "BG28_Reward_509",
];

/// Rewards that can be earned endlessly: their Quest starts over when it is completed and gives the
/// Reward again each time (`Ctx::quest_progress`), and owning one doesn't stop it being offered
/// again (card text).
pub const ENDLESS_REWARDS: [&str; 2] = ["BG24_Reward_311", "BG33_Reward_020"];

/// Quests / Rewards that Quest offers may pick: the current pool. Victim's Specter is not
/// offered in Aberration lobbies (36.6.3 notes; no client tag, so the rule is written here).
pub fn quest_offerable(lobby: Races, id: &str) -> bool {
    if id == "BG24_Reward_138" && lobby.intersects(Races::ABERRATION) {
        return false;
    }
    // A Reward tied to a minion type (Magicfin Relic: Murlocs) needs it in the lobby.
    let assoc = db().find(id).map_or(Races::NONE, |c| db().get(c).assoc);
    if !assoc.is_none() && !assoc.intersects(lobby) {
        return false;
    }
    POOL_QUESTS.contains(&id) || POOL_REWARDS.contains(&id)
}

/// "Increase a friendly minion's stats N times" (Find the Murder Weapon).
pub fn on_stats_gained(c: &mut Ctx, side: Side, _eid: EntityId) {
    let q = cid("BG24_Quest_123");
    if c.player_ref(side)
        .is_some_and(|p| p.quests.iter().any(|x| x.card == q))
    {
        c.quest_progress(side, q, 1);
    }
    check_total_attack(c, side);
}

/// Pressure the Authorities: complete once the warband reaches the goal total Attack.
pub fn check_total_attack(c: &mut Ctx, side: Side) {
    let q = cid("BG27_Quest_801");
    let Some((progress, goal)) = c
        .player_ref(side)
        .and_then(|p| p.quests.iter().find(|x| x.card == q))
        .map(|x| (x.progress, x.goal))
    else {
        return;
    };
    let total: i32 = c.boards[side]
        .iter()
        .filter(|m| m.alive())
        .fold(0i32, |total, m| total.saturating_add(m.atk));
    if total >= goal {
        c.quest_progress(side, q, goal - progress);
    } else if total != progress
        && let Some(x) = c
            .player(side)
            .and_then(|p| p.quests.iter_mut().find(|x| x.card == q))
    {
        // The progress shown is the warband's total Attack at the last check.
        x.progress = total;
    }
}

fn quest(listens: u32, f: crate::script::FxE) -> Script {
    Script {
        on_event: Some(f),
        listens,
        ..E
    }
}

/// `Script::shown` of a Quest whose text prints its goal as 0 ("Quest: Summon 0 minions."):
/// the goal of the active Quest (`QuestState::goal`); as printed while it isn't active.
fn shown_goal(c: &Ctx, sh: &crate::script::Shown) -> Vec<i32> {
    let goal = c
        .player_ref(sh.side)
        .and_then(|p| p.quests.iter().find(|q| q.card == sh.card))
        .map_or(0, |q| q.goal);
    vec![goal]
}

/// A `quest` whose text prints its goal.
fn counted(listens: u32, f: crate::script::FxE) -> Script {
    Script {
        shown: Some(shown_goal),
        ..quest(listens, f)
    }
}

fn mine(me: Me, e: &Event) -> bool {
    e.side() == me.side
}

/// A Refresh that counts toward "Refresh N times" (not the turn-start refill).
fn counted_refresh(me: Me, e: &Event) -> bool {
    matches!(e, Event::Refreshed { side, auto: false } if *side == me.side)
}

fn progress(c: &mut Ctx, me: Me, n: i32) {
    c.quest_progress(me.side, me.card, n);
}

/// Reward-level Avenge (N): friendly deaths this combat (reset at Start of Combat).
fn avenge(c: &mut Ctx, me: Me, e: &Event, n: i32) -> bool {
    if !c.combat || !matches!(e, Event::Died { side, .. } if *side == me.side) {
        return false;
    }
    let k = c.card_data(me.side, me.card) + 1;
    c.set_card_data(me.side, me.card, k);
    k % n == 0
}

fn reset_avenge(c: &mut Ctx, me: Me) {
    c.set_card_data(me.side, me.card, 0);
}

fn random_lobby_type(c: &mut Ctx) -> Races {
    let types: Vec<Races> = (0..Races::COUNT)
        .map(|i| Races(1 << i))
        .filter(|r| c.g.lobby_races.intersects(*r))
        .collect();
    c.g.rng.pick(&types).copied().unwrap_or(Races::NONE)
}

fn make_golden_eid(c: &mut Ctx, t: EntityId) {
    if let Some(m) = c.find_any_mut(t) {
        make_golden(m);
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Sire Denathrius: at the start of the game, choose one of two Quests.
        (
            "BG24_HERO_100p",
            Script {
                on_attach: Some(|c, me| c.offer_quests(me.side, 2)),
                flags: flag::PASSIVE,
                ..E
            },
        ),
        // ------------------------------------------------------------------ quests
        (
            "BG24_Quest_126",
            counted(ev::GOLD_SPENT, |c, me, e| {
                if let Event::GoldSpent { side, amount } = e
                    && *side == me.side
                {
                    progress(c, me, *amount);
                }
            }),
        ),
        (
            "BG24_Quest_112",
            quest(ev::REFRESHED, |c, me, e| {
                if counted_refresh(me, e) {
                    progress(c, me, 1);
                }
            }),
        ),
        (
            "BG24_Quest_114",
            counted(ev::SUMMONED, |c, me, e| {
                if mine(me, e) {
                    progress(c, me, 1);
                }
            }),
        ),
        // Unmask the Culprit: lose or tie N combats (checked at the next turn start).
        (
            "BG24_Quest_120",
            Script {
                start_of_turn: Some(|c, me| {
                    let lost = c
                        .player_ref(me.side)
                        .is_some_and(|p| p.stats.lost_last || p.stats.tied_last);
                    if c.g.turn > 1 && lost {
                        progress(c, me, 1);
                    }
                }),
                ..E
            },
        ),
        // Find the Murder Weapon: `on_stats_gained`.
        (
            "BG24_Quest_123",
            Script {
                shown: Some(shown_goal),
                ..E
            },
        ),
        (
            "BG24_Quest_124",
            counted(ev::DIED, |c, me, e| {
                if mine(me, e) {
                    progress(c, me, 1);
                }
            }),
        ),
        // Sort It All Out: minions ordered from lowest to highest Attack at Start of Combat.
        (
            "BG24_Quest_125",
            Script {
                start_of_combat: Some(|c, me| {
                    let atk: Vec<i32> = c.boards[me.side].iter().map(|m| m.atk).collect();
                    if atk.len() > 1 && atk.windows(2).all(|w| w[0] <= w[1]) {
                        progress(c, me, 1);
                    }
                }),
                ..E
            },
        ),
        // Unlikely Duo: buy N minions of either of two types (data = the two types).
        (
            "BG24_Quest_151",
            Script {
                on_attach: Some(|c, me| {
                    // Two different types ("Buy 4 X or Y"): the ones shown at the offer
                    // (`me.data`), drawn now when the Quest came another way.
                    let types = match me.data {
                        0 => draw_quest_types(c, me.card) as i32,
                        t => t,
                    };
                    if let Some(q) = c
                        .player(me.side)
                        .and_then(|p| p.quests.iter_mut().find(|q| q.card == me.card))
                    {
                        q.data = types;
                    }
                }),
                on_event: Some(|c, me, e| {
                    let Event::Bought { side, card, .. } = e else {
                        return;
                    };
                    let types = c
                        .player_ref(me.side)
                        .and_then(|p| p.quests.iter().find(|q| q.card == me.card))
                        .map_or(0, |q| q.data);
                    if *side == me.side && db().get(*card).races.intersects(Races(types as u16)) {
                        progress(c, me, 1);
                    }
                }),
                listens: ev::BOUGHT,
                ..E
            },
        ),
        (
            "BG24_Quest_311",
            quest(ev::PLAYED, |c, me, e| {
                if let Event::Played { side, played, .. } = e
                    && *side == me.side
                    && played.battlecry
                {
                    progress(c, me, 1);
                }
            }),
        ),
        (
            "BG24_Quest_313",
            counted(ev::BOUGHT, |c, me, e| {
                if let Event::Bought { side, card, .. } = e
                    && *side == me.side
                    && db().get(*card).is_minion()
                {
                    progress(c, me, 1);
                }
            }),
        ),
        (
            "BG24_Quest_314",
            counted(ev::CARD_ADDED, |c, me, e| {
                if mine(me, e) {
                    progress(c, me, 1);
                }
            }),
        ),
        (
            "BG24_Quest_318",
            quest(ev::ATTACKS, |c, me, e| {
                if let Event::Attacks { side, defender, .. } = e
                    && *side != me.side
                    && c.minion(*defender).is_some_and(|m| m.has(Kw::TAUNT))
                {
                    progress(c, me, 1);
                }
            }),
        ),
        (
            "BG24_Quest_320",
            quest(ev::DEATHRATTLE, |c, me, e| {
                if mine(me, e) {
                    progress(c, me, 1);
                }
            }),
        ),
        // Hire an Investigator: end your turn with unspent Gold N times.
        (
            "BG24_Quest_351",
            Script {
                end_of_turn: Some(|c, me| {
                    if c.player_ref(me.side).is_some_and(|p| p.gold > 0) {
                        progress(c, me, 1);
                    }
                }),
                ..E
            },
        ),
        (
            "BG24_Quest_352",
            counted(ev::ATTACKS, |c, me, e| {
                if mine(me, e) {
                    progress(c, me, 1);
                }
            }),
        ),
        (
            "BG27_Quest_800",
            quest(ev::SOLD, |c, me, e| {
                if mine(me, e) {
                    progress(c, me, 1);
                }
            }),
        ),
        // Pressure the Authorities: warband total Attack (checked on stat gains, plays, end of
        // turn).
        (
            "BG27_Quest_801",
            Script {
                on_attach: Some(|c, me| check_total_attack(c, me.side)),
                on_event: Some(|c, me, e| {
                    if mine(me, e) {
                        check_total_attack(c, me.side);
                    }
                }),
                listens: ev::PLAYED,
                end_of_turn: Some(|c, me| check_total_attack(c, me.side)),
                // "Quest: Get your warband to 28 total Attack.": the goal of the active
                // Quest, the printed base number while it is not active.
                shown: Some(|c, sh| {
                    let goal = c
                        .player_ref(sh.side)
                        .and_then(|p| p.quests.iter().find(|q| q.card == sh.card))
                        .map_or(db().get(sh.card).quest_goal, |q| q.goal);
                    vec![goal]
                }),
                ..E
            },
        ),
        (
            "BG27_Quest_802",
            // "Destroy N enemy minions": every enemy minion that dies in your combat, also
            // one destroyed by an effect without a killer (Leeroy, poison from a spell).
            counted(ev::DIED, |c, me, e| {
                if let Event::Died { side, .. } = e
                    && *side != me.side
                    && c.combat
                {
                    progress(c, me, 1);
                }
            }),
        ),
        (
            "BG28_Quest_500",
            counted(ev::SPELL_CAST, |c, me, e| {
                if mine(me, e) {
                    progress(c, me, 1);
                }
            }),
        ),
        // An Investigation!: in 3 turns, choose one of three Quests.
        (
            "BG24_Quest_Bob",
            Script {
                start_of_turn: Some(|c, me| {
                    if c.tick_counter(me.side, me.card, 3) {
                        if let Some(p) = c.player(me.side) {
                            p.quests.retain(|q| q.card != me.card);
                        }
                        c.offer_quests(me.side, 3);
                    }
                }),
                ..E
            },
        ),
        // ------------------------------------------------------------------ rewards
        (
            "BG24_Reward_107",
            Script {
                end_of_turn: Some(|c, me| {
                    let mut bc = c.friendly_where(me.side, |m| {
                        crate::script::minion_has(m, crate::script::Hook::Battlecry)
                    });
                    c.g.rng.shuffle(&mut bc);
                    for e in bc.into_iter().take(2) {
                        c.trigger_battlecry_ext(me.side, e);
                    }
                }),
                ..E
            },
        ),
        // Stolen Gold: Start of Combat: make your left- and right-most minions Golden.
        (
            "BG24_Reward_109",
            Script {
                start_of_combat: Some(|c, me| {
                    let (l, r) = (c.leftmost(me.side), c.rightmost(me.side));
                    for t in [l, r].into_iter().flatten() {
                        make_golden_eid(c, t);
                    }
                }),
                ..E
            },
        ),
        // Evil Twin: Start of Combat: summon a copy of your highest-Health minion.
        (
            "BG24_Reward_111",
            Script {
                start_of_combat: Some(|c, me| {
                    let keyed: Vec<(usize, i64)> = c.boards[me.side]
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| m.alive())
                        .map(|(i, m)| (i, m.hp as i64))
                        .collect();
                    let best = c.pick_max(keyed).map(|i| (i, c.boards[me.side][i].clone()));
                    if let Some((i, mut copy)) = best {
                        copy.eid = c.g.alloc_eid();
                        copy.combat_only = true;
                        copy.pool_copies = 0;
                        copy.attacked = false;
                        c.summon(me.side, i + 1, copy);
                    }
                }),
                ..E
            },
        ),
        // Ritual Dagger: after a friendly Deathrattle minion dies, give it +5/+5 permanently.
        (
            "BG24_Reward_113",
            quest(ev::DIED, |c, me, e| {
                let Event::Died {
                    side, eid, card, ..
                } = e
                else {
                    return;
                };
                let _ = card;
                if *side != me.side
                    || !c
                        .dead_record(me.side, *eid)
                        .is_some_and(|d| d.had_deathrattle)
                {
                    return;
                }
                if let Some(p) = c.owner[me.side]
                    && let Some(m) = c.g.players[p].board.iter_mut().find(|m| m.eid == *eid)
                {
                    m.add_stats(5, 5);
                }
            }),
        ),
        // Theotar's Parasol: end of turn, right-most minion Stealth until next turn and +8
        // Health.
        (
            "BG24_Reward_115",
            Script {
                end_of_turn: Some(|c, me| {
                    if let Some(t) = c.rightmost(me.side) {
                        c.give_kw_temp(t, Kw::STEALTH);
                        c.buff(t, 0, 8);
                    }
                }),
                ..E
            },
        ),
        // Exquisite Conch: `battlecry_mult`.
        ("BG24_Reward_123", E),
        // The Smoking Gun: your minions have +4 Attack (applied at Start of Combat).
        (
            "BG24_Reward_125",
            Script {
                start_of_combat: Some(|c, me| {
                    for t in c.friendly(me.side) {
                        c.buff(t, 4, 0);
                    }
                }),
                ..E
            },
        ),
        // Mirror Shield: after the Tavern is Refreshed, a random minion in it +6/+6 and
        // Divine Shield.
        (
            "BG24_Reward_128",
            quest(ev::REFRESHED, |c, me, e| {
                if mine(me, e) {
                    let t = c.tavern_minions(me.side);
                    if let Some(&sid) = c.g.rng.pick(&t) {
                        c.buff(sid, 6, 6);
                        c.give_kw(sid, Kw::DIVINE_SHIELD);
                    }
                }
            }),
        ),
        // Secret Sinstone: after you Discover a card, get an extra copy of it.
        (
            "BG24_Reward_129",
            quest(ev::DISCOVERED, |c, me, e| {
                // The card Discovered, in hand or waiting for hand space.
                if let Event::Discovered { side, hid, .. } = e
                    && *side == me.side
                {
                    c.give_copy_of_hand(me.side, *hid);
                }
            }),
        ),
        // Ghastly Mask / Gilnean War Horn / Turbulent Tombs / Rallying Cry / Secret Culprit:
        // get a copy of the reward's card; the rest is an engine check.
        ("BG24_Reward_130", get_reward_card()),
        ("BG27_Reward_802", get_reward_card()),
        ("BG27_Reward_803", get_reward_card()),
        ("BG33_Reward_021", get_reward_card()),
        ("BG28_Reward_510", get_reward_card()),
        // Red Hand: at the start of each turn, a minion in your hand +12/+12.
        (
            "BG24_Reward_131",
            Script {
                start_of_turn: Some(|c, me| {
                    let hand: Vec<EntityId> = c
                        .player_ref(me.side)
                        .map(|p| {
                            p.hand
                                .iter()
                                .filter(|h| h.item.as_minion().is_some())
                                .map(|h| h.hid)
                                .collect()
                        })
                        .unwrap_or_default();
                    if let Some(&h) = c.g.rng.pick(&hand) {
                        c.buff(h, 12, 12);
                    }
                }),
                ..E
            },
        ),
        // The Friends Along the Way: at the start of each turn, get 2 random minions of a
        // type (chosen when earned).
        (
            "BG24_Reward_134",
            Script {
                on_attach: Some(|c, me| {
                    // The type shown when its Quest was offered (`me.data`), drawn now when
                    // the Reward came another way.
                    let r = match me.data {
                        0 => draw_quest_types(c, me.card) as i32,
                        t => t,
                    };
                    c.set_card_data(me.side, me.card, r);
                }),
                start_of_turn: Some(|c, me| {
                    let r = Races(c.card_data(me.side, me.card) as u16);
                    let tier = c.tier(me.side);
                    for _ in 0..2 {
                        if let Some(m) =
                            c.random_minion(|d| d.races.intersects(r) && d.tier <= tier)
                        {
                            c.give_card(me.side, m);
                        }
                    }
                }),
                ..E
            },
        ),
        // Yogg-tastic Tasties: at the start of each turn, spin the Wheel of Yogg-Saron.
        (
            "BG24_Reward_135",
            Script {
                start_of_turn: Some(|c, me| super::yogg::wheel_of_yogg(c, me.side, false)),
                ..E
            },
        ),
        // Tiny Henchmen: end of turn, +3/+3 to 3 friendly minions of Tier 3 or lower.
        (
            "BG24_Reward_136",
            Script {
                end_of_turn: Some(|c, me| {
                    let mut f = c.friendly_where(me.side, |m| m.tier <= 3);
                    c.g.rng.shuffle(&mut f);
                    for t in f.into_iter().take(3) {
                        c.buff(t, 3, 3);
                    }
                }),
                ..E
            },
        ),
        // Victim's Specter: after each combat, get a plain copy of the last friendly minion that
        // died -- the most recent death, also when nothing died in the last combat (the record
        // carries over).
        (
            "BG24_Reward_138",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Died { side, card, .. } = e
                        && *side == me.side
                        && c.combat
                    {
                        c.set_card_data(me.side, me.card, db().normal_of(*card).0 as i32 + 1);
                    }
                }),
                listens: ev::DIED,
                start_of_turn: Some(|c, me| {
                    let v = c.card_data(me.side, me.card);
                    if v > 0 {
                        c.give_card(me.side, CardId((v - 1) as u16));
                    }
                }),
                ..E
            },
        ),
        // Anima Bribe: after you sell a minion, give its stats to a minion in the Tavern.
        (
            "BG24_Reward_305",
            quest(ev::SOLD, |c, me, e| {
                if let Event::Sold { side, atk, hp, .. } = e
                    && *side == me.side
                {
                    let t = c.tavern_minions(me.side);
                    if let Some(&sid) = c.g.rng.pick(&t) {
                        c.buff(sid, *atk, *hp);
                    }
                }
            }),
        ),
        // Cooked Book: after you buy a minion, give it +2/+2 and improve this.
        (
            "BG24_Reward_306",
            Script {
                // "After you buy a minion, give it +2/+2 and improve this."
                shown: Some(|c, sh| {
                    let k = c
                        .card_data(sh.side, sh.card)
                        .saturating_add(1)
                        .saturating_mul(2);
                    vec![k, k]
                }),
                ..quest(ev::BOUGHT, |c, me, e| {
                    if let Event::Bought {
                        side, card, eid, ..
                    } = e
                        && *side == me.side
                        && db().get(*card).is_minion()
                    {
                        let k = c
                            .card_data(me.side, me.card)
                            .saturating_add(1)
                            .saturating_mul(2);
                        c.buff(*eid, k, k);
                        let n = c.card_data(me.side, me.card).saturating_add(1);
                        c.set_card_data(me.side, me.card, n);
                    }
                })
            },
        ),
        // Teal Tiger Sapphire: Tavern minions +1/+1 per Refresh this turn.
        (
            "BG24_Reward_308",
            Script {
                on_event: Some(|c, me, e| {
                    if !counted_refresh(me, e) {
                        return;
                    }
                    let k = c.card_data(me.side, me.card) + 1;
                    c.set_card_data(me.side, me.card, k);
                    c.buff_tavern_temp(me.side, k, k);
                }),
                listens: ev::REFRESHED,
                start_of_turn: Some(|c, me| c.set_card_data(me.side, me.card, 0)),
                ..E
            },
        ),
        // Devils in the Details: end of turn, your left- and right-most minions each consume a
        // Tavern minion.
        (
            "BG24_Reward_309",
            Script {
                end_of_turn: Some(|c, me| {
                    let (l, r) = (c.leftmost(me.side), c.rightmost(me.side));
                    for t in [l, r].into_iter().flatten() {
                        c.consume_random(me.side, t, 1);
                        if l == r {
                            break;
                        }
                    }
                }),
                ..E
            },
        ),
        // Partner in Crime: get your Golden Buddy.
        (
            "BG24_Reward_310",
            Script {
                on_attach: Some(|c, me| {
                    let hero = c.player_ref(me.side).and_then(|p| p.hero);
                    if let Some(b) = hero.and_then(|h| db().get(h).buddy) {
                        c.give_card(me.side, db().golden_of(b));
                    }
                }),
                ..E
            },
        ),
        // Another Hidden Body: Discover a minion of your Tier.
        (
            "BG24_Reward_311",
            Script {
                on_attach: Some(|c, me| {
                    let tier = c.tier(me.side);
                    c.discover_minion(me.side, me.card, 3, |d| d.tier == tier);
                }),
                ..E
            },
        ),
        // Staff of Origination: Start of Combat: your minions +12/+12.
        (
            "BG24_Reward_312",
            Script {
                start_of_combat: Some(|c, me| {
                    for t in c.friendly(me.side) {
                        c.buff(t, 12, 12);
                    }
                }),
                ..E
            },
        ),
        // Wondrous Wisdomball: occasionally a helpful Refresh -- the first Refresh after you
        // get it, then each Refresh with 50% (`content::helpful`).
        ("BG24_Reward_313", E),
        // Alter Ego: even- (odd-) Tier Tavern minions have +7/+7, swapping each turn.
        (
            "BG24_Reward_321",
            Script {
                on_event: Some(|c, me, e| {
                    if !mine(me, e) {
                        return;
                    }
                    let odd = c.card_data(me.side, me.card) == 1;
                    for sid in c.tavern_minions(me.side) {
                        let even_tier = c.find_any(sid).is_some_and(|m| m.tier % 2 == 0);
                        if even_tier != odd {
                            c.buff(sid, 7, 7);
                        }
                    }
                }),
                listens: ev::REFRESHED,
                end_of_turn: Some(|c, me| {
                    let v = 1 - c.card_data(me.side, me.card);
                    c.set_card_data(me.side, me.card, v);
                }),
                ..E
            },
        ),
        // Menagerie Mayhem: end of turn, your minions +1/+1 per different friendly type.
        (
            "BG24_Reward_331",
            Script {
                end_of_turn: Some(|c, me| {
                    let k = distinct_type_count(c, me.side);
                    if k > 0 {
                        for t in c.friendly(me.side) {
                            c.buff(t, k, k);
                        }
                    }
                }),
                ..E
            },
        ),
        // Pilfered Lamps: `triple_need`.
        ("BG24_Reward_350", E),
        // Hidden Treasure Vault: at the start of each turn, gain 1 Gold (improves each turn).
        (
            "BG24_Reward_361",
            Script {
                start_of_turn: Some(|c, me| {
                    let k = c.card_data(me.side, me.card) + 1;
                    c.set_card_data(me.side, me.card, k);
                    c.gain_gold(me.side, k);
                }),
                // "At the start of each turn, gain 1 Gold. (Improves each turn!)"
                shown: Some(|c, sh| vec![c.card_data(sh.side, sh.card) + 1]),
                ..E
            },
        ),
        // Essence of Zerus: end of turn, get a Shifter Zerus.
        (
            "BG24_Reward_362",
            Script {
                end_of_turn: Some(|c, me| give_n(c, me.side, "BGS_029", 1)),
                ..E
            },
        ),
        // Ethereal Evidence: at the start of every turn, choose from 2 new Rewards (the pick
        // replaces the previous one).
        (
            "BG24_Reward_363",
            Script {
                start_of_turn: Some(|c, me| {
                    let owned: Vec<CardId> = c
                        .player_ref(me.side)
                        .map(|p| p.rewards.clone())
                        .unwrap_or_default();
                    let pool: Vec<CardId> = db()
                        .iter()
                        .filter(|d| {
                            d.kind == crate::cards::CardKind::QuestReward
                                && quest_offerable(c.g.lobby_races, &d.id)
                        })
                        // No Reward that counts or grows over time.
                        .filter(|d| !COUNTING_REWARDS.contains(&d.id.as_str()))
                        .filter(|d| {
                            script(d.cid).is_some()
                                && (!owned.contains(&d.cid)
                                    || ENDLESS_REWARDS.contains(&d.id.as_str()))
                        })
                        .map(|d| d.cid)
                        .collect();
                    let pool = pool.into_iter().map(|r| (r, reward_weight(r))).collect();
                    let options = c
                        .weighted_distinct(pool, 2)
                        .into_iter()
                        .map(|card| Item::Spell {
                            card,
                            data: draw_quest_types(c, card) as i32,
                        })
                        .collect();
                    c.offer_discover(me.side, me.card, options);
                }),
                ..E
            },
        ),
        // Volatile Venom: your minions have +7/+7 (Start of Combat); after they attack, they
        // die.
        (
            "BG24_Reward_364",
            Script {
                start_of_combat: Some(|c, me| {
                    for t in c.friendly(me.side) {
                        c.buff(t, 7, 7);
                    }
                }),
                on_event: Some(|c, me, e| {
                    if let Event::AttackDone { side, attacker } = e
                        && *side == me.side
                    {
                        c.destroy(*attacker);
                    }
                }),
                listens: ev::ATTACK_DONE,
                ..E
            },
        ),
        // Blood Goblet: end of turn, right-most minion +Attack equal to your missing Health.
        (
            "BG24_Reward_708",
            Script {
                end_of_turn: Some(|c, me| {
                    let missing = c.player_ref(me.side).map_or(0, |p| (30 - p.health).max(0));
                    if let Some(t) = c.rightmost(me.side)
                        && missing > 0
                    {
                        c.buff(t, missing, 0);
                    }
                }),
                ..E
            },
        ),
        // Sinfall Medallion: after you play a minion, 2 other friendly minions of its Tier
        // +4/+4.
        (
            "BG24_Reward_712",
            quest(ev::PLAYED, |c, me, e| {
                let Event::Played { side, eid, played } = e else {
                    return;
                };
                if *side != me.side {
                    return;
                }
                let Some(tier) = Some(played.tier) else {
                    return;
                };
                let mut f = c.friendly_where(me.side, |m| m.eid != *eid && m.tier == tier);
                c.g.rng.shuffle(&mut f);
                for t in f.into_iter().take(2) {
                    c.buff(t, 4, 4);
                }
            }),
        ),
        // Enhance-a-matic: at the start of each turn, get an Enhanced Part.
        (
            "BG24_Reward_715",
            Script {
                start_of_turn: Some(|c, me| {
                    let parts = [
                        "BG24_Reward_715t",
                        "BG24_Reward_715t2",
                        "BG24_Reward_715t3",
                        "BG24_Reward_715t4",
                    ];
                    let i = c.g.rng.below(parts.len());
                    give_n(c, me.side, parts[i], 1);
                }),
                ..E
            },
        ),
        ("BG24_Reward_715t", part()),
        ("BG24_Reward_715t2", part()),
        ("BG24_Reward_715t3", part()),
        ("BG24_Reward_715t4", part()),
        // Kidnap Sack / The Golden Hammer / Rushing Winds: Spellcraft rewards (a temporary
        // spell each turn).
        ("BG24_Reward_718", spellcraft_reward()),
        ("BG24_Reward_719", spellcraft_reward()),
        ("BG33_Reward_006", spellcraft_reward()),
        // Kidnap Sack: move a non-Golden Tavern card (minion or spell) to your hand.
        (
            "BG24_Reward_718t",
            Script {
                spell: Some(|c, me, t| {
                    let Some(p) = c.owner[me.side] else { return };
                    let shop = &mut c.g.players[p].shop;
                    if let Some(i) = shop.iter().position(|s| s.sid == t) {
                        let item = shop.remove(i).item;
                        c.add_to_hand(me.side, item);
                    }
                }),
                spell_target: TargetSpec::TavernCard,
                ..E
            },
        ),
        // The Golden Hammer: make a friendly minion Golden until next turn.
        (
            "BG24_Reward_719t",
            Script {
                spell: Some(|c, me, t| {
                    let Some((card, golden, a, h)) =
                        c.minion(t).map(|m| (m.card, m.golden, m.atk, m.max_hp))
                    else {
                        return;
                    };
                    if golden || db().get(card).golden.is_none() {
                        return;
                    }
                    make_golden_eid(c, t);
                    let Some((a2, h2)) = c.minion(t).map(|m| (m.atk, m.max_hp)) else {
                        return;
                    };
                    if let Some(p) = c.player(me.side) {
                        p.revert_golden.push((t, card, a2 - a, h2 - h));
                    }
                }),
                spell_target: TargetSpec::Friendly,
                ..E
            },
        ),
        // Rushing Winds: give a minion Windfury and Divine Shield.
        (
            "BG33_Reward_006t",
            Script {
                spell: Some(|c, _, t| {
                    c.give_kw(t, Kw::WINDFURY);
                    c.give_kw(t, Kw::DIVINE_SHIELD);
                }),
                spell_target: TargetSpec::Friendly,
                ..E
            },
        ),
        // Boom Squad: Avenge (3): 10 damage to the highest-Health enemy minion.
        (
            "BG27_Reward_502",
            Script {
                start_of_combat: Some(reset_avenge),
                on_event: Some(|c, me, e| {
                    if avenge(c, me, e, 3) {
                        let keyed: Vec<(EntityId, i64)> = c.boards[1 - me.side]
                            .iter()
                            .filter(|m| m.alive())
                            .map(|m| (m.eid, m.hp as i64))
                            .collect();
                        let t = c.pick_max(keyed);
                        if let Some(t) = t {
                            c.damage(0, t, 10);
                        }
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Invigorating Conch: when you buy a minion, give its stats to a random friendly
        // minion.
        (
            "BG27_Reward_503",
            quest(ev::BOUGHT, |c, me, e| {
                if let Event::Bought {
                    side, eid, card, ..
                } = e
                    && *side == me.side
                    && db().get(*card).is_minion()
                    && let Some((a, h)) = c.find_any(*eid).map(|m| (m.atk, m.hp))
                    && let Some(t) = c.random_friendly(me.side, |_| true)
                {
                    c.buff(t, a, h);
                }
            }),
        ),
        // Timeline Acceleration: at the start of each turn, get two Accelerators.
        (
            "BG27_Reward_504",
            Script {
                start_of_turn: Some(|c, me| give_n(c, me.side, "BG27_Reward_504t", 2)),
                ..E
            },
        ),
        // Accelerator: transform a minion into one from a Tier higher.
        (
            "BG27_Reward_504t",
            Script {
                spell: Some(|c, me, t| {
                    let Some((tier, pool)) = c.find_any(t).map(|m| (m.tier, m.pool_entries()))
                    else {
                        return;
                    };
                    let Some(new) = c.random_minion(|d| d.tier == (tier + 1).min(6)) else {
                        return;
                    };
                    for (card, copies) in pool {
                        c.g.pool.put(card, copies);
                    }
                    let mut m = c.new_minion(me.side, new);
                    c.take_pool_copy(&mut m);
                    if let Some(slot) = c.find_any_mut(t) {
                        m.eid = slot.eid;
                        *slot = m;
                    }
                    c.unfreeze_shop(t); // a transformed Tavern card isn't frozen
                }),
                spell_target: TargetSpec::FriendlyOrTavern,
                ..E
            },
        ),
        // Sturdy Shard: end of turn, minions without Taunt +1/+2 per friendly Taunt minion.
        (
            "BG27_Reward_804",
            Script {
                end_of_turn: Some(|c, me| {
                    let n = c.boards[me.side]
                        .iter()
                        .filter(|m| m.has(Kw::TAUNT))
                        .count() as i32;
                    if n > 0 {
                        for t in c.friendly_where(me.side, |m| !m.has(Kw::TAUNT)) {
                            c.buff(t, n, 2 * n);
                        }
                    }
                }),
                ..E
            },
        ),
        // Doppelganger's Locket: after each combat, Discover a non-Golden minion from your last
        // opponent's warband (enchantments kept).
        (
            "BG27_Reward_806",
            Script {
                start_of_turn: Some(|c, me| {
                    let Some(p) = c.owner[me.side] else { return };
                    let Some(o) = c.g.last_fought(p) else {
                        return;
                    };
                    let board: Vec<Minion> =
                        c.g.fought_board(o)
                            .iter()
                            .filter(|m| !m.golden)
                            .cloned()
                            .collect();
                    let picks = c.g.rng.sample_indices(board.len(), 3);
                    let options = picks
                        .into_iter()
                        .map(|i| {
                            let mut m = board[i].clone();
                            m.eid = c.g.alloc_eid();
                            m.pool_copies = 0;
                            Item::Minion(m)
                        })
                        .collect();
                    c.offer_discover(me.side, me.card, options);
                }),
                ..E
            },
        ),
        // Map of the Unknown: after you play a minion of a type you don't control, give a
        // friendly minion of each type +2/+2.
        (
            "BG27_Reward_810",
            quest(ev::PLAYED, |c, me, e| {
                let Event::Played { side, eid, played } = e else {
                    return;
                };
                if *side != me.side {
                    return;
                }
                let Some(r) = Some(played.races) else {
                    return;
                };
                let others = c.boards[me.side]
                    .iter()
                    .filter(|m| m.eid != *eid)
                    .fold(Races::NONE, |a, m| Races(a.0 | m.races.0));
                if r.is_none() || (r.0 & !others.0) == 0 {
                    return;
                }
                for (t, _) in one_minion_per_type(c, me.side, |_| true) {
                    c.buff(t, 2, 2);
                }
            }),
        ),
        // Bloodsoaked Tome: `minion_price`.
        ("BG27_Reward_811", E),
        // Scepter of Guidance: the Tavern always offers 2 minions of a type (chosen when
        // its Quest is offered) when Refreshed.
        (
            "BG27_Reward_812",
            Script {
                on_attach: Some(|c, me| {
                    // The type shown when its Quest was offered (`me.data`), drawn now when
                    // the Reward came another way.
                    let r = match me.data {
                        0 => draw_quest_types(c, me.card) as i32,
                        t => t,
                    };
                    c.set_card_data(me.side, me.card, r);
                }),
                on_fill_shop: Some(|c, me| {
                    let r = Races(c.card_data(me.side, me.card) as u16);
                    let tier = c.tier(me.side);
                    for _ in 0..2 {
                        c.add_shop_minion(me.side, 1, tier, r);
                    }
                }),
                ..E
            },
        ),
        // Endless Blood Moon: Blood Gems +1/+1; get two Blood Gems each turn.
        (
            "BG27_Reward_815",
            Script {
                on_attach: Some(|c, me| gem_bonus(c, me.side, 1, 1)),
                start_of_turn: Some(|c, me| give_n(c, me.side, BLOOD_GEM, 2)),
                ..E
            },
        ),
        // Beyond the Mirage / Temporal Tampering: engine checks.
        ("BG28_Reward_500", E),
        ("BG28_Reward_501", E),
        // Splitting Scroll: whenever you buy a Tavern spell costing (3)+, get an extra copy.
        (
            "BG28_Reward_502",
            quest(ev::BOUGHT, |c, me, e| {
                if let Event::Bought { side, card, .. } = e
                    && *side == me.side
                    && is_tavern_spell(*card)
                    && db().get(*card).cost >= 3
                {
                    c.give_card(me.side, *card);
                }
            }),
        ),
        // Cycle of Energy: Avenge (3): get a random Tavern spell.
        (
            "BG28_Reward_504",
            Script {
                start_of_combat: Some(reset_avenge),
                on_event: Some(|c, me, e| {
                    if avenge(c, me, e, 3) {
                        let tier = c.tier(me.side);
                        if let Some(s) = c.random_spell(|d| d.tier <= tier) {
                            c.give_card(me.side, s);
                        }
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Tumbling Disaster: minions summoned in combat +1/+1; Avenge (4): improve permanently.
        (
            "BG28_Reward_505",
            Script {
                start_of_combat: Some(reset_avenge),
                on_event: Some(|c, me, e| {
                    if let Event::Summoned { side, eid } = e
                        && *side == me.side
                        && c.combat
                    {
                        let k = 1 + improvements(c, me);
                        c.buff(*eid, k, k);
                    }
                    if avenge(c, me, e, 4) {
                        add_improvement(c, me);
                    }
                }),
                listens: ev::SUMMONED | ev::DIED,
                ..E
            },
        ),
        // Double-Headed Reward: the first card you buy each turn, get an extra copy.
        (
            "BG28_Reward_506",
            Script {
                on_event: Some(|c, me, e| {
                    if let Event::Bought {
                        side, card, eid, ..
                    } = e
                        && *side == me.side
                        && c.card_data(me.side, me.card) == 0
                    {
                        c.set_card_data(me.side, me.card, 1);
                        // A full copy of the bought card; a plain one when the card is not in hand
                        // (cast when bought). With a full hand the copy waits for space: that is
                        // still the one extra copy.
                        let in_hand = c
                            .player_ref(me.side)
                            .is_some_and(|p| p.hand.iter().any(|h| h.hid == *eid));
                        if in_hand {
                            c.give_copy_of_hand(me.side, *eid);
                        } else {
                            c.give_card(me.side, *card);
                        }
                    }
                }),
                listens: ev::BOUGHT,
                start_of_turn: Some(|c, me| c.set_card_data(me.side, me.card, 0)),
                ..E
            },
        ),
        // Gift of the Golden Kobold: after you Refresh 5 times, make the highest-Tier Tavern
        // minion Golden.
        (
            "BG28_Reward_508",
            Script {
                // "After you Refresh 5 times, make the highest-Tier minion in the Tavern
                // Golden. (5 left!)"
                shown: Some(|c, sh| shown_left(c, sh, 5)),
                ..quest(ev::REFRESHED, |c, me, e| {
                    if counted_refresh(me, e) && c.tick_counter(me.side, me.card, 5) {
                        golden_forge(c, me.side);
                    }
                })
            },
        ),
        // Smelting Chamber: at the start of each turn, make a friendly minion of Tier (1 +
        // improvements) Golden and improve this.
        (
            "BG28_Reward_509",
            Script {
                start_of_turn: Some(|c, me| {
                    // It improves every turn, target or not, and stays at Tier 6 once there.
                    let tier = (1 + improvements(c, me)).min(6) as u8;
                    let cands = c.friendly_where(me.side, |m| {
                        !m.golden && m.tier == tier && db().get(m.card).golden.is_some()
                    });
                    if let Some(&t) = c.g.rng.pick(&cands) {
                        make_golden_eid(c, t);
                    }
                    if tier < 6 {
                        add_improvement(c, me);
                    }
                }),
                // "At the start of each turn, make a friendly Tier 1 minion Golden and improve
                // this."
                shown: Some(|c, sh| vec![(1 + c.card_level(sh.side, sh.card)).min(6)]),
                ..E
            },
        ),
        // Open Auditions: at the start of each turn, Discover a Buddy.
        (
            "BG28_Reward_513",
            Script {
                start_of_turn: Some(|c, me| discover_buddy(c, me.side, me.card)),
                ..E
            },
        ),
        // Untamed Sorcery: at the start of each turn, cast 5 random Tavern spells. A Choose One
        // spell takes a random option (as Yogg-Saron's Puzzle Box).
        (
            "BG28_Reward_514",
            Script {
                start_of_turn: Some(|c, me| {
                    let tier = c.tier(me.side);
                    for _ in 0..5 {
                        let Some(s) = c.random_spell(|d| d.tier <= tier) else {
                            return;
                        };
                        let spec = effect_target_spec(s).unwrap_or(TargetSpec::None);
                        let targets = c.legal_targets(me.side, spec, 0);
                        let t = if spec == TargetSpec::None {
                            Some(0)
                        } else {
                            c.g.rng.pick(&targets).copied()
                        };
                        if let Some(t) = t {
                            let choose = script(s).is_some_and(|x| x.choose_one);
                            let opt = if choose { c.g.rng.below(2) as u8 } else { 0 };
                            c.you_cast(me.side, s, 0, t, opt);
                        }
                    }
                }),
                ..E
            },
        ),
        // Stash of the Scribe: at the start of each turn, get 3 random Tavern spells.
        (
            "BG28_Reward_515",
            Script {
                start_of_turn: Some(|c, me| {
                    let tier = c.tier(me.side);
                    for _ in 0..3 {
                        if let Some(s) = c.random_spell(|d| d.tier <= tier) {
                            c.give_card(me.side, s);
                        }
                    }
                }),
                ..E
            },
        ),
        // Stable Amalgamation: Avenge (7): when you have space, summon a 50/50 Amalgam.
        (
            "BG28_Reward_518",
            Script {
                start_of_combat: Some(reset_avenge),
                on_event: Some(|c, me, e| {
                    if avenge(c, me, e, 7) {
                        let m = c.new_minion(me.side, cid("BG28_Reward_518t"));
                        c.when_space(me.side, crate::engine::SpaceReq::Minion(m));
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        (
            "BG28_Reward_518t",
            Script {
                on_create: Some(|m| m.races = Races::ALL),
                ..E
            },
        ),
        // Righteous Charge: Start of Combat: your left-most minion gains Divine Shield and
        // attacks immediately.
        (
            "BG33_Reward_003",
            Script {
                start_of_combat: Some(|c, me| {
                    if let Some(t) = c.leftmost(me.side) {
                        c.give_kw(t, Kw::DIVINE_SHIELD);
                        crate::combat::attack_now(c, me.side, t);
                    }
                }),
                ..E
            },
        ),
        // Grim Freshener: Avenge (2): gain a free Refresh.
        (
            "BG33_Reward_004",
            Script {
                start_of_combat: Some(reset_avenge),
                on_event: Some(|c, me, e| {
                    if avenge(c, me, e, 2)
                        && let Some(p) = c.player(me.side)
                    {
                        p.free_refreshes += 1;
                    }
                }),
                listens: ev::DIED,
                ..E
            },
        ),
        // Norgannon's Reward: unlock Tier 7; at the start of your next turn only, upgrade the
        // Tavern (free).
        (
            "BG33_Reward_010",
            Script {
                on_attach: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.tier7 = true;
                    }
                    c.set_card_data(me.side, me.card, 1);
                }),
                start_of_turn: Some(|c, me| {
                    if c.card_data(me.side, me.card) == 1 {
                        c.set_card_data(me.side, me.card, 0);
                        let (max, costs) = (c.cfg().max_regular_tier, c.cfg().upgrade_costs);
                        if let Some(p) = c.player(me.side)
                            && p.tier < 7
                        {
                            p.tier += 1;
                            let tier = p.tier;
                            // Like a bought upgrade: the next one costs its own tier's price.
                            p.upgrade_cost = if tier < max + u8::from(p.tier7) {
                                costs[(tier as usize - 1).min(5)]
                            } else {
                                0
                            };
                            release_ready_held(c, me.side, 0);
                            c.emit(Event::Upgraded {
                                side: me.side,
                                tier,
                            });
                        }
                    }
                }),
                ..E
            },
        ),
        // Magicfin Relic: at the start of each turn, get a 1/1 Murloc; Discover a Tavern spell
        // to teach it (`after_discover`).
        (
            "BG33_Reward_011",
            Script {
                start_of_turn: Some(|c, me| {
                    give_n(c, me.side, "BG33_890t", 1);
                    let tier = c.tier(me.side);
                    c.discover_spell(me.side, me.card, 3, |d| d.tier <= tier);
                }),
                ..E
            },
        ),
        // Untold Riches: get a 5-Gold Coin Pouch; +5 maximum Gold.
        (
            "BG33_Reward_012",
            Script {
                on_attach: Some(|c, me| {
                    give_n(c, me.side, "BG33_Reward_012t", 1);
                    c.add_max_gold(me.side, 5);
                }),
                ..E
            },
        ),
        (
            "BG33_Reward_012t",
            Script {
                spell: Some(|c, me, _| c.gain_gold(me.side, 5)),
                ..E
            },
        ),
        // Golden Forge: at the start of each turn, make the highest-Tier Tavern minion Golden.
        (
            "BG33_Reward_013",
            Script {
                start_of_turn: Some(|c, me| golden_forge(c, me.side)),
                ..E
            },
        ),
        // Quaint Boutique / Jumbo Warehouse: next turn, gain 4 Gold and choose a Lesser /
        // Greater Trinket to buy.
        ("BG33_Reward_014", boutique()),
        ("BG33_Reward_015", boutique()),
        // Cosmic Reward: Discover a second Hero Power.
        (
            "BG33_Reward_017",
            Script {
                on_attach: Some(|c, me| drop(c.offer_hero_powers(me.side, 3, 1, &[]))),
                ..E
            },
        ),
        // Perpetual Incantation: Tavern spells give an extra +2/+1; "+2/+1 can be earned
        // endlessly": its Quest starts over and gives another +2/+1 at every completion
        // (`Ctx::quest_progress`; the client text shows the total, "+{3}/+{1}").
        (
            "BG33_Reward_020",
            Script {
                on_attach: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.stats.spell_atk = p.stats.spell_atk.saturating_add(2);
                        p.stats.spell_hp = p.stats.spell_hp.saturating_add(1);
                    }
                    let n = c.card_data(me.side, me.card);
                    c.set_card_data(me.side, me.card, n.saturating_add(1));
                }),
                // "Your Tavern spells give an extra +2/+1. (+2/+1 can be earned endlessly!)"
                shown: Some(|c, sh| {
                    let n = c.card_data(sh.side, sh.card).max(1);
                    vec![n.saturating_mul(2), n, 2, 1]
                }),
                ..E
            },
        ),
    ]
}

fn get_reward_card() -> Script {
    Script {
        on_attach: Some(|c, me| {
            if let Some(x) = db().get(me.card).reward_card {
                c.give_card(me.side, x);
            }
        }),
        ..E
    }
}

/// Enhanced Part: +5/+5 and the part's keyword.
fn part() -> Script {
    Script {
        spell: Some(|c, me, t| {
            spell_buff(c, me, t, 5, 5);
            let k = match db().get(me.card).id.as_str() {
                "BG24_Reward_715t" => Kw::TAUNT,
                "BG24_Reward_715t2" => Kw::WINDFURY,
                "BG24_Reward_715t3" => Kw::DIVINE_SHIELD,
                _ => Kw::REBORN,
            };
            c.give_kw(t, k);
        }),
        spell_target: TargetSpec::Friendly,
        ..E
    }
}

/// A Reward with a Spellcraft spell: get it (temporary) at the start of each turn and when
/// earned.
fn spellcraft_reward() -> Script {
    Script {
        on_attach: Some(|c, me| {
            // Earned during combat: the next turn's start gives the spell (a temporary card
            // given now would still be in hand then, next to that turn's one).
            if !c.combat
                && let Some(s) = db().get(me.card).spellcraft
            {
                c.give_temp_spell(me.side, s, 0);
            }
        }),
        start_of_turn: Some(|c, me| {
            if let Some(s) = db().get(me.card).spellcraft {
                c.give_temp_spell(me.side, s, 0);
            }
        }),
        ..E
    }
}

fn boutique() -> Script {
    Script {
        on_attach: Some(|c, me| c.set_card_data(me.side, me.card, 1)),
        start_of_turn: Some(|c, me| {
            if c.card_data(me.side, me.card) != 1 {
                return;
            }
            c.set_card_data(me.side, me.card, 2);
            c.gain_gold(me.side, 4);
            let kind = if db().get(me.card).id == "BG33_Reward_014" {
                crate::cards::CardKind::TrinketLesser
            } else {
                crate::cards::CardKind::TrinketGreater
            };
            if let Some(p) = c.owner[me.side] {
                c.board_home(me.side, |c| c.g.offer_trinkets(p, kind));
            }
        }),
        ..E
    }
}

fn improvements(c: &Ctx, me: Me) -> i32 {
    c.card_level(me.side, me.card)
}

fn add_improvement(c: &mut Ctx, me: Me) {
    c.improve_card(me.side, me.card);
}

fn golden_forge(c: &mut Ctx, side: Side) {
    let Some(p) = c.owner[side] else { return };
    let keyed: Vec<(EntityId, i64)> = c.g.players[p]
        .shop
        .iter()
        .filter_map(|s| {
            s.item
                .as_minion()
                .map(|m| (m.tier, m.golden, s.sid, db().get(m.card).golden.is_some()))
        })
        .filter(|x| !x.1 && x.3)
        .map(|x| (x.2, x.0 as i64))
        .collect();
    let best = c.pick_max(keyed);
    if let Some(sid) = best {
        make_golden_eid(c, sid);
    }
}

/// Buddies that only work with one hero's Hero Power: generic Buddy sources (Open Auditions,
/// E.T.C....) never offer them. Getting a hero's own Buddy, or a named one (Finley, Tess...), is
/// not limited.
const POWER_BOUND_BUDDIES: [&str; 14] = [
    "BG20_HERO_283_Buddy",        // Flight Trainer: Galewing's flightpaths
    "BG20_HERO_301_Buddy",        // Nightmare Ectoplasm: Mutanus's Devour
    "BG21_HERO_000_Buddy",        // Captain Fairmount: Conviction
    "BG22_HERO_000_Buddy",        // Crabby: Lock and Load
    "BG22_HERO_001_Buddy",        // Spirit Raptor: Bru'kan's Elements
    "BG22_HERO_004_Buddy",        // Varden's Aquarrior: Twice as Nice
    "BG23_HERO_303_Buddy",        // Watfin: Detective for Hire
    "TB_BaconShop_HERO_12_Buddy", // Pigeon Lord: the Rat King's type
    "TB_BaconShop_HERO_21_Buddy", // Street Magician: Better Secrets
    "TB_BaconShop_HERO_42_Buddy", // Jr. Navigator: Lead Explorer's cost
    "TB_BaconShop_HERO_58_Buddy", // Nexus Lord: Arcane Alteration
    "TB_BaconShop_HERO_60_Buddy", // Crimson Hand Centurion: Verdant Spheres
    "TB_BaconShop_HERO_62_Buddy", // Shadow Warden: Maiev's Imprison
    "TB_BaconShop_HERO_67_Buddy", // Raging Contender: Trash for Treasure
];

/// The Buddies of the lobby's heroes (`Game::lobby_heroes`) that have a script, minus temporarily
/// banned ones (resource pack `temporary_bans`): Vaelastrasz; and minus the Hero-Power-bound ones.
pub fn buddy_pool(g: &Game) -> Vec<CardId> {
    let all = g.cfg.content == crate::config::ContentMode::AllVanilla;
    g.lobby_heroes()
        .filter_map(|d| d.buddy)
        .filter(|b| {
            let id = db().get(*b).id.as_str();
            id != "TB_BaconShop_HERO_56_Buddy" && !POWER_BOUND_BUDDIES.contains(&id)
        })
        .filter(|b| all || script(*b).is_some())
        .collect()
}

/// `buddy_pool` up to `side`'s Tavern Tier: a Discover or a random card that names no Tier never
/// reaches above yours (E.T.C. offered Buddies of any Tier).
pub fn buddies_up_to_tier(c: &Ctx, side: Side) -> Vec<CardId> {
    let tier = c.tier(side);
    buddy_pool(c.g)
        .into_iter()
        .filter(|b| db().get(*b).tier <= tier)
        .collect()
}

/// Discover a Buddy (from the lobby heroes' Buddies, up to your Tier).
pub fn discover_buddy(c: &mut Ctx, side: Side, source: CardId) {
    let buddies = buddies_up_to_tier(c, side);
    let picks = c.g.rng.sample_indices(buddies.len(), 3);
    let options = picks
        .into_iter()
        .map(|i| Item::Minion(c.new_minion(side, buddies[i])))
        .collect();
    c.offer_discover(side, source, options);
}
