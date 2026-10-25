//! Hero powers (active, passive, start-of-combat, economy overrides, trinket timing).
//! Heroes whose power is implemented here are the ones offered in `ContentMode::Implemented`.
//! Price/shop overrides for Millhouse, Sindragosa, Ysera and Clocksworth live in
//! `recruit.rs` / `engine.rs` because they change base rules rather than trigger.

use super::*;

/// The visit is on the printed game turn even if the power is acquired later.
/// `counter` holds that fixed turn, never a delay from acquisition.
fn extra_timewarp(major: bool) -> Script {
    Script {
        on_attach: Some(if major { tw_arm::<8> } else { tw_arm::<5> }),
        start_of_turn: Some(if major {
            tw_visit::<true>
        } else {
            tw_visit::<false>
        }),
        shown: Some(if major { tw_shown::<8> } else { tw_shown::<5> }),
        flags: flag::PASSIVE,
        hero_power_uses: 0,
        ..E
    }
}

fn tw_shown<const TURN: u32>(c: &Ctx, _sh: &crate::script::Shown) -> Vec<i32> {
    vec![TURN as i32, shown_turns_left(c, TURN)]
}

fn tw_arm<const TURN: u32>(c: &mut Ctx, me: Me) {
    let at = TURN;
    if let Some(h) = c.power_mut(me.side, me.card) {
        h.counter = at as i32;
    }
}

fn tw_visit<const MAJOR: bool>(c: &mut Ctx, me: Me) {
    let turn = c.g.turn as i32;
    if c.power_mut(me.side, me.card)
        .is_some_and(|h| h.counter == turn)
    {
        timewarp::open(c, me.side, MAJOR);
    }
}

/// `Script::shown` helper: what is left of a goal of `n` counted in the Hero Power's
/// `counter` ("(N left!)"); never below 0.
pub fn shown_left(c: &Ctx, sh: &crate::script::Shown, n: i32) -> i32 {
    (n - c.power_counter(sh.side, sh.card)).max(0)
}

/// `Script::shown` helper: the turns left until Turn `turn` ("(N turns left!)").
pub fn shown_turns_left(c: &Ctx, turn: u32) -> i32 {
    turn.saturating_sub(c.g.turn.max(1)) as i32
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        // Patchwerk: start the game with 30 extra Health.
        (
            "TB_BaconShop_HP_035",
            passive(Script {
                on_attach: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.health += 30;
                    }
                }),
                ..E
            }),
        ),
        // George the Fallen: give a minion Divine Shield.
        // Shudderwock: trigger a friendly minion's Battlecry (unlocks on Turn 3).
        (
            "TB_BaconShop_HP_022",
            active(
                TargetSpec::FriendlyHas(crate::script::Hook::Battlecry),
                |c, me, t| c.trigger_battlecry_ext(me.side, t),
            ),
        ),
        (
            // Boon of Light: give a minion Divine Shield - not one that has it (the client's
            // targets leave them out).
            "TB_BaconShop_HP_010",
            active(TargetSpec::AnyNoDivineShield, |c, _me, t| {
                c.give_kw(t, Kw::DIVINE_SHIELD)
            }),
        ),
        // Illidan: SoC left- and right-most gain +2/+1 and attack immediately.
        (
            "TB_BaconShop_HP_069",
            passive(Script {
                start_of_combat: Some(illidan),
                ..E
            }),
        ),
        // Deathwing: SoC give ALL minions +2 Attack permanently.
        (
            "TB_BaconShop_HP_061",
            passive(Script {
                start_of_combat: Some(|c, _me| {
                    for s in 0..2 {
                        for e in c.friendly(s) {
                            c.buff_perm(s, e, 2, 0);
                        }
                    }
                }),
                ..E
            }),
        ),
        ("TB_BaconShop_HP_054", passive(E)), // Millhouse: prices in recruit.rs
        ("TB_BaconShop_HP_014", passive(E)), // Sindragosa: prices/slots/freeze
        // Ysera: the Tavern offers an extra Dragon whenever it is Refreshed.
        (
            "TB_BaconShop_HP_062",
            passive(Script {
                on_fill_shop: Some(|c, me| {
                    let t = c.tier(me.side);
                    c.add_shop_minion(me.side, 1, t, Races::DRAGON);
                }),
                ..E
            }),
        ),
        // Nozdormu: at the start of your turn, gain a free Refresh.
        (
            "TB_BaconShop_HP_063",
            passive(Script {
                start_of_turn: Some(|c, me| {
                    if let Some(p) = c.player(me.side) {
                        p.free_refreshes += 1;
                    }
                }),
                ..E
            }),
        ),
        // The Lich King: give a minion Reborn until next turn.
        (
            "TB_BaconShop_HP_024",
            active(TargetSpec::Friendly, |c, _me, t| {
                c.give_kw_temp(t, Kw::REBORN)
            }),
        ),
        // Reno Jackson: once per game, make a friendly minion Golden.
        (
            "TB_BaconShop_HP_046",
            active(TargetSpec::FriendlyNonGoldenMaxTier(7), |c, _me, t| {
                if let Some(m) = c.minion_mut(t) {
                    super::spells::make_golden(m);
                }
            }),
        ),
        // Kael'thas: after you buy 3 minions, get a Tavern Coin.
        (
            "TB_BaconShop_HP_066",
            passive(Script {
                on_event: Some(kaelthas),
                listens: ev::BOUGHT,
                ..E
            }),
        ),
        // Forest Warden Omu: after you upgrade the Tavern, gain 2 Gold.
        (
            "TB_BaconShop_HP_082",
            passive(Script {
                on_event: Some(|c, me, _e| c.gain_gold(me.side, 2)),
                listens: ev::UPGRADED,
                ..E
            }),
        ),
        // Death Speaker Blackthorn: get 2 Blood Gems (twice per turn).
        (
            "BG20_HERO_103p",
            Script {
                hero_power: Some(|c, me, _| give_n(c, me.side, BLOOD_GEM, 2)),
                hero_power_uses: 2,
                ..E
            },
        ),
        // Forest Lord Cenarius: increase your maximum Gold by 1.
        (
            "BG32_HERO_001p",
            active(TargetSpec::None, |c, me, _| c.add_max_gold(me.side, 1)),
        ),
        // Doctor Holli'dae: get a random Tavern spell.
        (
            "BG28_HERO_801p",
            active(TargetSpec::None, |c, me, _| {
                let tier = c.tier(me.side);
                if let Some(s) = c.random_spell(|d| d.tier <= tier) {
                    c.give_card(me.side, s);
                }
            }),
        ),
        // Al'Akir: SoC give your left-most minion Windfury, Divine Shield and Taunt.
        (
            "TB_BaconShop_HP_086",
            passive(Script {
                start_of_combat: Some(|c, me| {
                    if let Some(e) = c.leftmost(me.side) {
                        c.give_kw(e, Kw::WINDFURY.union(Kw::DIVINE_SHIELD).union(Kw::TAUNT));
                    }
                }),
                ..E
            }),
        ),
        // Trade Prince Gallywix: after you sell a minion, gain 1 Gold next turn.
        (
            "TB_BaconShop_HP_008",
            passive(Script {
                on_event: Some(|c, me, _e| c.gold_next_turn(me.side, 1)),
                listens: ev::SOLD,
                ..E
            }),
        ),
        // Cap'n Hoggarr: after you buy a Pirate, gain 1 Gold.
        (
            "BG26_HERO_101p",
            passive(Script {
                on_event: Some(hoggarr),
                listens: ev::BOUGHT,
                ..E
            }),
        ),
        // Edwin VanCleef: give a minion +2/+2; improves after you buy 4 cards.
        (
            "TB_BaconShop_HP_001",
            Script {
                hero_power: Some(|c, me, t| {
                    // Each improvement adds the base +2/+2 again ("improve").
                    let k = 2
                        * (1 + c
                            .power_mut(me.side, me.card)
                            .map(|h| h.counter / 4)
                            .unwrap_or(0));
                    c.buff(t, k, k);
                }),
                hero_power_target: TargetSpec::Friendly,
                on_event: Some(|c, me, _e| {
                    if let Some(h) = c.power_mut(me.side, me.card) {
                        h.counter += 1;
                    }
                }),
                listens: ev::BOUGHT,
                // "Give a minion +2/+2. Improves after you buy 4 cards. (4 left!)"
                shown: Some(|c, sh| {
                    let n = c.power_counter(sh.side, sh.card);
                    let k = 2 * (1 + n / 4);
                    vec![k, k, 4, 4 - n % 4]
                }),
                ..E
            },
        ),
        // Mister Clocksworth: triples need 2 copies; Tavern Coins instead of rewards.
        ("BG34_HERO_002p", passive(E)),
        // Morchie (Warped Conflux): "On Turn 5, visit the Minor Timewarp. (4 turns left!)"
        ("BG34_HERO_004p", extra_timewarp(false)),
        // Murozond, Unbounded (Alternate Timeline): "On Turn 8, visit the Major Timewarp.
        // (7 turns left!)"
        ("BG34_HERO_000p", extra_timewarp(true)),
        // Marin the Manager: on Turn 5, choose a Lesser Trinket to buy.
        (
            "BG30_HERO_304p",
            passive(Script {
                start_of_turn: Some(|c, me| {
                    trinket_turn(c, me, 5, crate::cards::CardKind::TrinketLesser)
                }),
                // "On Turn 5, choose a Lesser Trinket to buy. (4 turns left!)"
                shown: Some(|c, _| vec![5, shown_turns_left(c, 5)]),
                ..E
            }),
        ),
        // Buttons: on Turn 8, choose a Greater Trinket to buy.
        (
            "BG32_HERO_002p",
            passive(Script {
                start_of_turn: Some(|c, me| {
                    trinket_turn(c, me, 8, crate::cards::CardKind::TrinketGreater)
                }),
                // "On Turn 8, choose a Greater Trinket to buy. (7 turns left!)"
                shown: Some(|c, _| vec![8, shown_turns_left(c, 8)]),
                ..E
            }),
        ),
        // Drest'agath: discard a card to get a random Aberration.
        (
            "BG36_HERO_000p",
            active(TargetSpec::Hand, |c, me, t| {
                if crate::recruit::discard_card(c, me.side, t).is_some() {
                    let tier = c.tier(me.side);
                    if let Some(a) =
                        c.random_minion(|d| d.races.intersects(Races::ABERRATION) && d.tier <= tier)
                    {
                        c.give_card(me.side, a);
                    }
                }
            }),
        ),
        // Kith'ix: get 2 random minions; when you play one, discard the other.
        (
            "BG36_HERO_002p",
            active(TargetSpec::None, |c, me, _| {
                let tier = c.tier(me.side);
                if let (Some(a), Some(b)) = (
                    c.random_minion(|d| d.tier <= tier),
                    c.random_minion(|d| d.tier <= tier),
                ) {
                    crate::recruit::give_linked_pair(c, me.side, a, b);
                }
            }),
        ),
        // Drek'Thar / Vanndar: when you have space in combat, summon a copy of your
        // highest-Attack / highest-Health minion (unlocks on Turn 7).
        (
            "BG22_HERO_002p",
            passive(Script {
                start_of_combat: Some(|c, me| copy_highest(c, me, false)),
                ..E
            }),
        ),
        (
            "BG22_HERO_003p",
            passive(Script {
                start_of_combat: Some(|c, me| copy_highest(c, me, true)),
                ..E
            }),
        ),
        // Ozumat: when you have space in combat, summon a 2/2 Tentacle with Taunt; it gains
        // +1/+1 after you sell a minion (stored in hp_counter).
        (
            "BG23_HERO_201p",
            passive(Script {
                start_of_combat: Some(|c, me| {
                    let k = c
                        .power_mut(me.side, me.card)
                        .map(|h| h.counter)
                        .unwrap_or(0);
                    let mut m = c.new_minion(me.side, cid("BG23_HERO_201pt"));
                    m.add_stats(k, k);
                    c.when_space(me.side, crate::engine::SpaceReq::Minion(m));
                }),
                on_event: Some(|c, me, _e| {
                    if let Some(h) = c.power_mut(me.side, me.card) {
                        h.counter += 1;
                    }
                }),
                listens: ev::SOLD,
                // "When you have space in combat, summon a 2/2 Tentacle with Taunt. (Gains
                // +1/+1 after you sell a minion!)"
                shown: Some(|c, sh| {
                    let k = c.power_counter(sh.side, sh.card);
                    vec![2 + k, 2 + k, 1, 1]
                }),
                ..E
            }),
        ),
        // Yogg-Saron: at the start of your turn, cast a random Tavern spell (Turn 3+).
        (
            "TB_BaconShop_HP_039t",
            passive(Script {
                start_of_turn: Some(yogg),
                ..E
            }),
        ),
    ]
}

fn active(spec: TargetSpec, f: crate::script::FxT) -> Script {
    Script {
        hero_power: Some(f),
        hero_power_target: spec,
        ..E
    }
}

fn passive(s: Script) -> Script {
    Script {
        flags: s.flags | flag::PASSIVE,
        hero_power_uses: 0,
        ..s
    }
}

fn illidan(c: &mut Ctx, me: Me) {
    let (Some(l), Some(r)) = (c.leftmost(me.side), c.rightmost(me.side)) else {
        return;
    };
    c.buff(l, 2, 1);
    if r != l {
        c.buff(r, 2, 1);
    }
    crate::combat::attack_now(c, me.side, l);
    if r != l && c.on_board(r) {
        crate::combat::attack_now(c, me.side, r);
    }
}

fn kaelthas(c: &mut Ctx, me: Me, e: &Event) {
    let Event::Bought { card, .. } = e else {
        return;
    };
    if !db().get(*card).is_minion() {
        return;
    }
    let Some(h) = c.power_mut(me.side, me.card) else {
        return;
    };
    h.counter += 1;
    if h.counter >= 3 {
        h.counter = 0;
        c.give_card(me.side, cid(TAVERN_COIN));
    }
}

fn hoggarr(c: &mut Ctx, me: Me, e: &Event) {
    if let Event::Bought { card, .. } = e
        && db().get(*card).races.intersects(Races::PIRATE)
    {
        c.gain_gold(me.side, 1);
    }
}

fn trinket_turn(c: &mut Ctx, me: Me, turn: u32, kind: crate::cards::CardKind) {
    if c.g.turn == turn
        && let Some(p) = c.owner[me.side]
    {
        c.board_home(me.side, |c| c.g.offer_trinkets(p, kind));
    }
}

fn yogg(c: &mut Ctx, me: Me) {
    if c.g.turn < 3 {
        return;
    }
    let tier = c.tier(me.side);
    let Some(s) = c.random_spell(|d| d.tier <= tier) else {
        return;
    };
    let spec = effect_target_spec(s).unwrap_or(TargetSpec::None);
    let targets = c.legal_targets(me.side, spec, 0);
    if spec != TargetSpec::None && targets.is_empty() {
        return;
    }
    let t = c.g.rng.pick(&targets).copied().unwrap_or(0);
    let opt = c.g.rng.below(2) as u8;
    let who = c.who(me.side);
    let name = &db().get(s).name;
    c.g.log(|| {
        format!("{who} hero power Puzzle Box (Yogg-Saron): casts a random Tavern spell: {name}")
    });
    c.you_cast(me.side, s, 0, t, opt);
}

fn copy_highest(c: &mut Ctx, me: Me, by_health: bool) {
    if c.g.turn >= 7 {
        c.when_space(me.side, crate::engine::SpaceReq::CopyHighest { by_health });
    }
}
