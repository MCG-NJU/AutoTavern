//! Debug / UI support: a human-friendly JSON view of the game, the card catalog, debug
//! commands for manual testing (edit gold, boards, hands...) and combat previews.
//! Used by debugging front ends and `Game.view` / `Game.debug`; not part of the RL hot path.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::actions::*;
use crate::cards::{CardId, CardKind, db};
use crate::engine::{CombatRecord, Ctx, Game, Phase};
use crate::script::{Shown, flag, implemented, script};
use crate::state::{Item, Minion, Pending};
use crate::types::{Kw, PlayerId, Races, Side};

fn kind_name(k: CardKind) -> &'static str {
    match k {
        CardKind::Minion => "minion",
        CardKind::TavernSpell => "tavern_spell",
        CardKind::Spell => "spell",
        CardKind::Hero => "hero",
        CardKind::HeroPower => "hero_power",
        CardKind::TrinketLesser => "trinket_lesser",
        CardKind::TrinketGreater => "trinket_greater",
        CardKind::DarkGift => "dark_gift",
        CardKind::Quest => "quest",
        CardKind::QuestReward => "quest_reward",
    }
}

fn kw_names(k: Kw) -> Vec<&'static str> {
    (0..Kw::NAMES.len())
        .filter(|i| k.0 >> i & 1 == 1)
        .map(|i| Kw::NAMES[i])
        .collect()
}

fn race_names(r: Races) -> Vec<&'static str> {
    if r == Races::ALL {
        return vec!["ALL"];
    }
    r.iter().map(|x| Races::NAMES[x.index().unwrap()]).collect()
}

/// Public scoreboard information from the last completed combat, never the live board.
/// Dual-type minions count in both tribes; All-type minions count in each lobby tribe.
fn warband_summary(board: &[Minion], lobby_races: Races, fought: bool) -> Value {
    if board.is_empty() && !fought {
        return Value::Null;
    }
    let mut counts = [0usize; Races::NAMES.len()];
    for m in board {
        let races = if m.races == Races::ALL {
            lobby_races
        } else {
            m.races
        };
        for race in races.iter() {
            counts[race.index().unwrap()] += 1;
        }
    }
    let count = counts.iter().copied().max().unwrap_or(0);
    let leaders: Vec<_> = counts
        .iter()
        .enumerate()
        .filter(|(_, n)| count > 0 && **n == count)
        .map(|(i, _)| i)
        .collect();
    json!({
        "race": if leaders.len() == 1 { Some(Races::NAMES[leaders[0]]) } else { None },
        "count": count,
        "mixed": leaders.len() > 1,
        "size": board.len(),
        "counts": counts.iter().enumerate().filter(|(_, n)| **n > 0)
            .map(|(i, n)| json!({"race": Races::NAMES[i], "count": n})).collect::<Vec<_>>(),
    })
}

/// Static info about a card.
pub fn card_json(c: CardId) -> Value {
    let d = db().get(c);
    json!({
        "cid": c.0,
        "id": d.id,
        "name": d.name,
        "name_zh": d.name_zh,
        "text": d.text,
        "text_zh": d.text_zh,
        "kind": kind_name(d.kind),
        "tier": d.tier,
        "cost": d.cost,
        "atk": d.atk,
        "hp": d.hp,
        "races": race_names(d.races),
        "implemented": implemented(c),
        "in_pool": d.pools.minion || d.pools.spell || d.pools.hero || d.pools.trinket,
        "pool_tag": d.pool_tag,
        "reachable": d.reachable,
        "golden_id": d.golden.map(|g| db().get(g).id.clone()),
        "normal_id": d.normal.map(|g| db().get(g).id.clone()),
        "scripted": script(c).is_some(),
        "tier7": d.pools.tier7,
        "hero_power_id": d.hero_power.map(|h| db().get(h).id.clone()),
        "hero_power_scripted": d.hero_power.is_some_and(|h| script(h).is_some()),
        "gift_window": d.window.map(|(a, b)| json!([a, b])),
    })
}

fn mod_kind(k: crate::state::ModKind) -> &'static str {
    use crate::state::ModKind::*;
    match k {
        Add => "add",
        Set => "set",
        Merge => "merge",
        Tavern => "tavern",
        Magnetic => "magnetic",
    }
}

/// Stat breakdown: base card stats, recorded changes (with sources), player-level "this game"
/// bonuses, temporary buffs, and whatever is left unexplained.
fn breakdown(m: &Minion) -> Value {
    let d = db().get(m.card);
    let (mut a, mut h) = (d.atk, d.hp);
    let mut rows = vec![];
    for x in &m.mods {
        match x.kind {
            crate::state::ModKind::Set => {
                a = x.atk;
                h = x.hp;
            }
            _ => {
                a += x.atk;
                h += x.hp;
            }
        }
        rows.push(json!({
            "kind": mod_kind(x.kind),
            "src": x.src.map(|c| json!({"id": db().get(c).id, "name": db().get(c).name, "name_zh": db().get(c).name_zh})),
            "atk": x.atk,
            "hp": x.hp,
        }));
    }
    a += m.pbonus_atk + m.temp_atk;
    h += m.pbonus_hp + m.temp_hp;
    json!({
        "base": [d.atk, d.hp],
        "mods": rows,
        "this_game_bonus": [m.pbonus_atk, m.pbonus_hp],
        "temporary": [m.temp_atk, m.temp_hp],
        "other": [m.atk - a, m.max_hp - h],
        "damage": m.max_hp - m.hp,
    })
}

/// The runs of ASCII digits in `text`: (byte start, byte end, value).
fn numbers(text: &str) -> Vec<(usize, usize, i32)> {
    let b = text.as_bytes();
    let (mut out, mut i) = (vec![], 0);
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let s = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            out.push((s, i, text[s..i].parse().unwrap_or(i32::MAX)));
        } else {
            i += 1;
        }
    }
    out
}

/// The numbers printed in a card text, in order (what a `Script::shown` hook returns for a
/// fresh instance).
pub fn text_numbers(text: &str) -> Vec<i32> {
    numbers(text).iter().map(|x| x.2).collect()
}

/// `text` with its k-th number replaced by `vals[k]`.
fn with_numbers(text: &str, vals: &[i32]) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut at = 0;
    for ((s, e, _), v) in numbers(text).into_iter().zip(vals) {
        out.push_str(&text[at..s]);
        out.push_str(&v.to_string());
        at = e;
    }
    out.push_str(&text[at..]);
    // "(1 turns left!)" reads wrong; the texts use these plurals after a count.
    for word in ["turns", "Wishes", "Digs", "Creations", "times"] {
        let singular = &word[..word.len() - if word == "Wishes" { 2 } else { 1 }];
        out = out.replace(&format!("(1 {word} "), &format!("(1 {singular} "));
    }
    out
}

/// Display names of minion types: (English, Chinese), several joined with "or".
fn type_names(r: Races) -> (String, String) {
    const EN: [&str; 11] = [
        "Beast",
        "Demon",
        "Dragon",
        "Elemental",
        "Mech",
        "Murloc",
        "Pirate",
        "Quilboar",
        "Undead",
        "Aberration",
        "Naga",
    ];
    const ZH: [&str; 11] = [
        "野兽",
        "恶魔",
        "龙",
        "元素",
        "机械",
        "鱼人",
        "海盗",
        "野猪人",
        "亡灵",
        "畸变怪",
        "纳迦",
    ];
    let idx: Vec<usize> = r.iter().filter_map(|x| x.index()).collect();
    (
        idx.iter().map(|i| EN[*i]).collect::<Vec<_>>().join(" or "),
        idx.iter().map(|i| ZH[*i]).collect::<Vec<_>>().join("或"),
    )
}

/// Texts whose placeholder is a name, not a number: the client prints "92" for a minion
/// type chosen at run time and "0" for a card or type name.
fn named_text(c: &Ctx, sh: &Shown) -> Option<(String, String)> {
    let d = db().get(sh.card);
    match d.id.as_str() {
        // Magicfin Apprentice ("This casts 0."): the spell it was taught.
        "BG33_890t" => {
            let (en, zh) = match sh.counter() {
                n if n > 0 => {
                    let spell = db().get(CardId((n - 1) as u16));
                    (spell.name.as_str(), spell.name_zh.as_str())
                }
                _ => ("nothing yet", "（尚未学会法术）"),
            };
            Some((
                d.text.replacen(" 0.", &format!(" {en}."), 1),
                d.text_zh.replacen("施放0", &format!("施放{zh}"), 1),
            ))
        }
        // "a random 92": the type kept in the card data once the card is owned.
        "BG30_MagicItem_426"
        | "BG30_MagicItem_426t"
        | "BG32_MagicItem_901"
        | "BG24_Reward_134"
        | "BG27_Reward_812" => {
            let data =
                if sh.data != 0 && matches!(d.id.as_str(), "BG24_Reward_134" | "BG27_Reward_812") {
                    sh.data
                } else {
                    c.card_data(sh.side, sh.card)
                };
            let r = Races(data as u16);
            let (en, zh) = if !r.is_none() {
                type_names(r)
            } else if d.id.starts_with("BG24") || d.id.starts_with("BG27") {
                (
                    "minion of a random type".into(),
                    "随机一个种族的随从".into(),
                )
            } else if d.id == "BG32_MagicItem_901" && c.player_ref(sh.side)?.has_trinket_id(sh.card)
            {
                ("minion (already used)".into(), "随从（已使用）".into())
            } else {
                (
                    "minion of your most common type".into(),
                    "你数量最多的种族的随从".into(),
                )
            };
            Some((d.text.replace("92", &en), d.text_zh.replace("92", &zh)))
        }
        // Unlikely Duo: "Buy 14 0 or 0" = buy <goal> minions of either of two types.
        "BG24_Quest_151" => {
            let q = c
                .player_ref(sh.side)?
                .quests
                .iter()
                .find(|q| q.card == sh.card);
            let goal = q.map_or(d.quest_goal, |q| q.goal);
            let (en, zh) = match q {
                Some(q) if q.data != 0 => {
                    let (a, b) = type_names(Races(q.data as u16));
                    (a, b.replace('或', "牌或") + "牌")
                }
                _ => (
                    "minions of two random types".into(),
                    "两个随机种族的随从牌".into(),
                ),
            };
            Some((
                format!("Quest: Buy {goal} {en}."),
                format!("任务：购买{goal}张{zh}。"),
            ))
        }
        // "Get a copy of '0'": the card the Reward names.
        _ => {
            let card = db().get(d.reward_card?);
            (d.text.contains("'0'") && d.text_zh.contains("0的")).then(|| {
                (
                    d.text.replace("'0'", &card.name),
                    d.text_zh.replacen("0的", &format!("{}的", card.name_zh), 1),
                )
            })
        }
    }
}

/// The text of a card as it should read now: (English, Chinese). None = as printed.
/// The script's `shown` hook supplies the current numbers; the Chinese text takes them
/// when its numbers come in the same order as the English ones.
pub fn live_text(c: &Ctx, sh: &Shown) -> Option<(String, String)> {
    let d = db().get(sh.card);
    if let Some(t) = named_text(c, sh) {
        return Some(t);
    }
    let f = script(sh.card).and_then(|s| s.shown)?;
    let vals = f(c, sh);
    let printed = text_numbers(&d.text);
    if vals.len() != printed.len() || vals == printed {
        return None;
    }
    let zh = if text_numbers(&d.text_zh) == printed {
        with_numbers(&d.text_zh, &vals)
    } else {
        d.text_zh.clone()
    };
    Some((with_numbers(&d.text, &vals), zh))
}

fn shown_minion<'a>(side: usize, m: &'a Minion) -> Shown<'a> {
    Shown {
        side,
        card: m.card,
        golden: m.golden,
        minion: Some(m),
        data: 0,
    }
}

fn shown_card(card: CardId) -> Shown<'static> {
    Shown {
        side: 0,
        card,
        golden: db().get(card).normal.is_some(),
        minion: None,
        data: 0,
    }
}

fn shown_item(it: &Item) -> Shown<'_> {
    match it {
        Item::Minion(m) => shown_minion(0, m),
        Item::Spell { card, data } => Shown {
            data: *data,
            ..shown_card(*card)
        },
    }
}

type LiveMap = HashMap<String, (String, String)>;

/// A minion and the cards attached to it (Dark Gifts: `<key>a<index>`).
fn put_minion(c: &Ctx, out: &mut LiveMap, key: String, m: &Minion) {
    for (i, a) in m.attached.iter().enumerate() {
        let sh = Shown {
            card: *a,
            golden: false,
            data: m.attachment_data_at(i),
            ..shown_minion(0, m)
        };
        if let Some(t) = live_text(c, &sh) {
            out.insert(format!("{key}a{i}"), t);
        }
    }
    if let Some(t) = live_text(c, &shown_minion(0, m)) {
        out.insert(key, t);
    }
}

fn put_item(c: &Ctx, out: &mut LiveMap, key: String, it: &Item) {
    match it {
        Item::Minion(m) => put_minion(c, out, key, m),
        Item::Spell { .. } => {
            if let Some(t) = live_text(c, &shown_item(it)) {
                out.insert(key, t);
            }
        }
    }
}

fn put_card(c: &Ctx, out: &mut LiveMap, key: String, card: CardId) {
    if let Some(t) = live_text(c, &shown_card(card)) {
        out.insert(key, t);
    }
}

/// Live texts of everything player `p` looks at, by view key: `e<eid>` warband, `h<hid>`
/// hand, `s<sid>` Tavern, `p<slot>` Hero Powers, `t<i>` trinkets, `r<i>` rewards, `q<i>`
/// quests, `o<i>` options of the pending choice. Computed on a copy of the game inside a
/// recruit context, so the hooks can use the engine's own helpers.
fn live_texts(g: &Game, p: PlayerId) -> LiveMap {
    let mut tmp = g.clone();
    tmp.cfg.record_log = false;
    tmp.with_recruit(p, |c| {
        let c: &Ctx = c;
        let mut out = HashMap::new();
        for m in &c.boards[0] {
            put_minion(c, &mut out, format!("e{}", m.eid), m);
        }
        let pl = &c.g.players[p];
        for h in &pl.hand {
            put_item(c, &mut out, format!("h{}", h.hid), &h.item);
        }
        for s in &pl.shop {
            put_item(c, &mut out, format!("s{}", s.sid), &s.item);
        }
        match pl.pending.last() {
            Some(Pending::Discover { options, .. }) => {
                for (i, o) in options.iter().enumerate() {
                    put_item(c, &mut out, format!("o{i}"), o);
                }
            }
            Some(Pending::Timewarp { options, .. }) => {
                for (i, (o, _)) in options.iter().enumerate() {
                    put_item(c, &mut out, format!("o{i}"), o);
                }
            }
            Some(Pending::Trinket { options, .. }) => {
                for (i, t) in options.iter().enumerate() {
                    put_card(c, &mut out, format!("o{i}"), *t);
                }
            }
            // A Quest offer prints its goal as "0": show the goal, and the minion types
            // drawn for the pair.
            Some(Pending::Quest { options, types }) => {
                let lobby = c.g.lobby_races;
                for (i, (q, r)) in options.iter().enumerate() {
                    put_card(c, &mut out, format!("o{i}"), *q);
                    put_card(c, &mut out, format!("o{i}r"), *r);
                    let (qt, rt) = types.get(i).copied().unwrap_or((0, 0));
                    let d = db().get(*q);
                    // The text prints the goal as "0", or as the base number (Pressure the
                    // Authorities): show the goal of this pair.
                    let prints_goal = |t: &str| {
                        let n = text_numbers(t);
                        n == [0] || n == [d.quest_goal]
                    };
                    if prints_goal(&d.text) && d.quest_goal > 0 {
                        let goal = [crate::content::quest_goal(lobby, *q, *r)];
                        let zh = if prints_goal(&d.text_zh) {
                            with_numbers(&d.text_zh, &goal)
                        } else {
                            d.text_zh.clone()
                        };
                        out.insert(format!("o{i}"), (with_numbers(&d.text, &goal), zh));
                    }
                    // Unlikely Duo prints type placeholders: its text is written here, with
                    // the goal of this pair and the two types drawn for it.
                    if d.id == "BG24_Quest_151" {
                        let goal = crate::content::quest_goal(lobby, *q, *r);
                        let (en, zh) = if qt != 0 {
                            let (a, b) = type_names(Races(qt));
                            (a, b.replace('或', "牌或") + "牌")
                        } else {
                            (
                                "minions of two random types".into(),
                                "两个随机种族的随从牌".into(),
                            )
                        };
                        out.insert(
                            format!("o{i}"),
                            (
                                format!("Quest: Buy {goal} {en}."),
                                format!("任务：购买{goal}张{zh}。"),
                            ),
                        );
                    }
                    // A Reward about a minion type prints "92": the type of this pair.
                    if rt != 0 {
                        let rd = db().get(*r);
                        let (en, zh) = type_names(Races(rt));
                        out.insert(
                            format!("o{i}r"),
                            (rd.text.replace("92", &en), rd.text_zh.replace("92", &zh)),
                        );
                    }
                }
            }
            _ => {}
        }
        for (i, h) in pl.powers.iter().enumerate() {
            put_card(c, &mut out, format!("p{i}"), h.card);
        }
        for (i, t) in pl.trinkets.iter().enumerate() {
            put_card(c, &mut out, format!("t{i}"), *t);
        }
        for (i, t) in pl.rewards.iter().enumerate() {
            put_card(c, &mut out, format!("r{i}"), *t);
        }
        for (i, q) in pl.quests.iter().enumerate() {
            put_card(c, &mut out, format!("q{i}"), q.card);
            put_card(c, &mut out, format!("q{i}r"), q.reward);
        }
        out
    })
}

/// Overwrite `text` / `text_zh` of a card's JSON with its live text, if it has one.
fn set_live(v: &mut Value, live: &LiveMap, key: String) {
    if let Some((en, zh)) = live.get(&key) {
        v["text"] = json!(en);
        v["text_zh"] = json!(zh);
        v["live_text"] = json!(true);
    }
}

pub fn minion_json(m: &Minion) -> Value {
    let d = db().get(m.card);
    let activate = script(m.card)
        .and_then(|s| s.activate)
        .map(|(cost, _, _)| cost);
    json!({
        "eid": m.eid,
        "cid": m.card.0,
        "id": d.id,
        "name": d.name,
        "name_zh": d.name_zh,
        "text": d.text,
        "text_zh": d.text_zh,
        "base_atk": d.atk,
        "base_hp": d.hp,
        "counter": m.counter,
        "tier": m.tier,
        "atk": m.atk,
        "hp": m.hp,
        "max_hp": m.max_hp,
        "golden": m.golden,
        "keywords": kw_names(m.kw),
        "races": race_names(m.races),
        "shield_hits": m.shield_hits,
        "attached": m.attached.iter().map(|a| card_json(*a)).collect::<Vec<_>>(),
        "breakdown": breakdown(m),
        "shield_max": m.shield_max,
        "pool_copies": m.pool_copies,
        "golden_text": d.golden.map(|g| json!({"text": db().get(g).text, "text_zh": db().get(g).text_zh})),
        "temp_atk": m.temp_atk,
        "temp_hp": m.temp_hp,
        "triple_reward": m.triple_reward,
        "activate_cost": activate,
        "activations": m.activations,
        "implemented": implemented(m.card),
    })
}

fn item_json(it: &Item) -> Value {
    match it {
        Item::Minion(m) => {
            let mut v = minion_json(m);
            v["type"] = json!("minion");
            v
        }
        Item::Spell { card, data } => {
            let mut v = card_json(*card);
            v["type"] = json!("spell");
            v["data"] = json!(data);
            v["unplayable"] = json!(script(*card).is_some_and(|s| s.has_flag(flag::UNPLAYABLE)));
            v
        }
    }
}

fn combat_json(r: &CombatRecord) -> Value {
    json!({
        "turn": r.turn,
        "a": r.a,
        "b": r.b,
        "winner": r.winner,
        "damage": r.damage,
        "truncated": r.truncated,
        "frames": r.frames.iter().map(|f| json!({
            "log": f.log,
            "attacks": f.attacks,
            "boards": [
                f.boards[0].iter().map(minion_json).collect::<Vec<_>>(),
                f.boards[1].iter().map(minion_json).collect::<Vec<_>>(),
            ],
        })).collect::<Vec<_>>(),
    })
}

/// Everything the UI needs from player `p`'s seat (debug: opponents' current boards too).
pub fn view(g: &Game, p: PlayerId) -> Value {
    view_for(g, p, false, true)
}

/// Skip materialising other players' private boards and unrelated combat replays.
pub fn public_view(g: &Game, p: PlayerId) -> Value {
    view_for(g, p, true, true)
}

/// A public view without the combat replays (for a front end that already has them).
pub fn public_view_without_combats(g: &Game, p: PlayerId) -> Value {
    view_for(g, p, true, false)
}

fn view_for(g: &Game, p: PlayerId, public: bool, combats: bool) -> Value {
    let pl = &g.players[p];
    let power_json = |slot: usize| {
        pl.powers.get(slot).map(|h| {
            let s = script(h.card);
            let mut v = card_json(h.card);
            v["slot"] = json!(slot);
            v["action"] = json!(A_HERO_POWER + slot);
            v["price"] = json!(g.hero_power_price(p, slot));
            v["usable"] = json!(g.hero_power_usable(p, slot));
            v["passive"] =
                json!(s.is_none_or(|s| s.hero_power.is_none() || s.has_flag(flag::PASSIVE)));
            v["uses_turn"] = json!(h.uses_turn);
            v["uses_game"] = json!(h.uses_game);
            v["counter"] = json!(h.counter);
            v
        })
    };
    let pending = pl.pending.last().map(|top| match top {
        Pending::HeroSelect { options } => json!({
            "kind": "hero",
            "options": options.iter().map(|h| {
                let mut v = card_json(*h);
                let d = db().get(*h);
                v["armor"] = json!(d.armor);
                v["hero_power"] = d.hero_power.map(card_json).unwrap_or(Value::Null);
                v
            }).collect::<Vec<_>>(),
        }),
        Pending::Discover { source, options } => json!({
            "kind": "discover",
            "source": card_json(*source),
            "options": options.iter().map(item_json).collect::<Vec<_>>(),
        }),
        Pending::ChooseOne { card, .. } => json!({
            "kind": "choose_one",
            "source": card_json(*card),
            "options": [card_json(*card), card_json(*card)],
        }),
        Pending::Target { options, .. } => json!({
            "kind": "target",
            "targets": options,
        }),
        Pending::HeroPowerChoice { options, slot } => json!({
            "kind": "hero_power",
            "slot": slot,
            "options": options.iter().map(|c| card_json(*c)).collect::<Vec<_>>(),
        }),
        Pending::Trinket {
            options,
            free,
            replace,
        } => json!({
            "kind": "trinket",
            "free": free,
            "replace": replace.map(card_json),
            "options": options.iter().map(|c| {
                let mut v = card_json(*c);
                // What it costs this player now, its base cost and its offer weight (%).
                v["base_cost"] = v["cost"].clone();
                v["cost"] = json!(g.trinket_price(p, *c));
                v["weight"] = json!(crate::content::trinket_offers::weight(g, &db().get(*c).id));
                v
            }).collect::<Vec<_>>(),
        }),
        Pending::Timewarp {
            options,
            chronum,
            major,
        } => json!({
            "kind": "timewarp",
            "major": major,
            "chronum": chronum,
            // Choosing index `leave` exits the Timewarped Tavern.
            "leave": options.len(),
            "options": options.iter().map(|(it, cost)| {
                let mut v = item_json(it);
                v["chronum_cost"] = json!(cost);
                v
            }).collect::<Vec<_>>(),
        }),
        Pending::Quest { options, .. } => json!({
            "kind": "quest",
            "options": options.iter().map(|(q, r)| {
                let mut v = card_json(*q);
                v["goal"] = json!(crate::content::quest_goal(g.lobby_races, *q, *r));
                v["reward"] = card_json(*r);
                v
            }).collect::<Vec<_>>(),
        }),
    });
    // Which hand card is being cast while its target/mode is pending (for the UI).
    let casting = pl.pending.last().and_then(|top| match top {
        Pending::Target {
            cont: crate::state::Continuation::HandSpell { hid, .. },
            ..
        }
        | Pending::ChooseOne {
            cont: crate::state::Continuation::HandSpell { hid, .. },
            ..
        } => Some(*hid),
        _ => None,
    });
    let cancellable = pl.pending.last().is_some_and(|t| t.cancellable());
    let base_gold = (g.cfg.start_gold + g.cfg.gold_increment * (g.turn as i32 - 1).max(0))
        .min(g.cfg.gold_refill_cap);
    let opponents: Vec<Value> = g
        .players
        .iter()
        .filter(|o| o.id != p)
        .map(|o| {
            json!({
                "id": o.id,
                "hero": o.hero.map(card_json),
                "health": o.health,
                "armor": o.armor,
                "tier": o.tier,
                "alive": o.alive,
                "placement": o.placement,
                "win_streak": o.stats.streak.max(0),
                "last_damage": o.stats.last_damage,
                "last_warband": warband_summary(&o.seen_board, g.lobby_races, o.stats.won_last || o.stats.lost_last || o.stats.tied_last),
                "last_board": if public { Vec::new() } else { o.last_board.iter().map(minion_json).collect::<Vec<_>>() },
                // What the viewer may know: the warband of its last combat against this
                // opponent (`last_board` can already hold this turn's finished board).
                "seen_board": g.known_board(p, o.id).0.iter().map(minion_json).collect::<Vec<_>>(),
                "seen_turn": g.known_board(p, o.id).1,
                "board": if public { Vec::new() } else { o.board.iter().map(minion_json).collect::<Vec<_>>() },
                "hand_size": o.hand.len(),
                "trinkets": o.trinkets.iter().map(|t| card_json(*t)).collect::<Vec<_>>(),
                "quests": o.quests.iter().map(|q| json!({ "quest": card_json(q.card), "reward": card_json(q.reward), "progress": q.progress, "goal": q.goal })).collect::<Vec<_>>(),
                "rewards": o.rewards.iter().map(|r| card_json(*r)).collect::<Vec<_>>(),
                "buddy_claims": o.buddy_button.as_ref().map(|b| b.claims),
                "was_last_opponent": pl.last_opponent == Some(o.id),
                "is_next_opponent": pl.next_opponent == Some(o.id),
            })
        })
        .collect();
    let mask = g.legal_mask(p);
    // Silas Darkmoon keeps his tickets (Tavern ids) on the power.
    let silas = crate::cardid!("TB_BaconShop_HP_101");
    let tickets: &[i32] = pl.power_of(silas).map_or(&[], |h| &h.values);
    let mut view = json!({
        "seat": p,
        "seed": g.seed,
        "turn": g.turn,
        "phase": match g.phase { Phase::HeroSelect => "hero_select", Phase::Recruit => "recruit", Phase::GameOver => "game_over" },
        "version": crate::GAME_VERSION,
        "lobby_races": race_names(g.lobby_races),
        "has_aberrations": g.lobby_races.intersects(crate::types::Races::ABERRATION),
        "deity": g.deity.map(card_json),
        "paradox": g.paradox.map(card_json),
        "alive": g.alive_count(),
        "me": {
            "hero": pl.hero.map(card_json),
            "hero_power": power_json(0),
            "hero_powers": (0..pl.powers.len()).filter_map(power_json).collect::<Vec<_>>(),
            "health": pl.health,
            "armor": pl.armor,
            "alive": pl.alive,
            "placement": pl.placement,
            "win_streak": pl.stats.streak.max(0),
            "last_damage": pl.stats.last_damage,
            "last_warband": warband_summary(&pl.seen_board, g.lobby_races, pl.stats.won_last || pl.stats.lost_last || pl.stats.tied_last),
            "tier": pl.tier,
            "gold": pl.gold,
            "max_gold": base_gold + pl.max_gold_bonus,
            "max_gold_bonus": pl.max_gold_bonus,
            "gold_next_turn": pl.gold_next_turn,
            "upgrade_price": g.upgrade_price(p),
            "can_upgrade_tier": pl.tier < g.cfg.max_regular_tier,
            "refresh_price": g.refresh_price(p),
            "minion_price": g.minion_price(p),
            "free_refreshes": pl.free_refreshes,
            "frozen": pl.is_frozen(),
            "ended_turn": pl.ended_turn,
            "board": pl.board.iter().map(minion_json).collect::<Vec<_>>(),
            "hand": pl.hand.iter().enumerate().map(|(i, h)| {
                let mut v = item_json(&h.item);
                v["hid"] = json!(h.hid);
                v["locked"] = json!(h.locked);
                v["held"] = json!(h.held);
                v["link"] = json!(h.link);
                v["group"] = json!(h.group);
                v["group_left"] = json!(h.group_left);
                // Playing a hand card costs nothing; the key stays for readers of the view.
                v["play_cost"] = json!(0);
                v["temporary"] = json!(h.temporary);
                v["combined"] = json!(h.combined);
                if g.cfg.experimental_mode && i >= crate::actions::N_HAND {
                    let mut probe = g.clone();
                    probe.players[p].hand.swap(0, i);
                    let mask = probe.legal_mask(p);
                    v["extra_play_positions"] = json!((0..crate::actions::N_POS).filter(|j| mask[crate::actions::A_PLAY + j]).collect::<Vec<_>>());
                    v["extra_magnetize"] = json!((0..pl.board.len()).filter(|j| mask[crate::actions::A_MAGNETIZE + j]).collect::<Vec<_>>());
                }
                v
            }).collect::<Vec<_>>(),
            "shop": pl.shop.iter().enumerate().map(|(i, s)| {
                let mut v = item_json(&s.item);
                v["sid"] = json!(s.sid);
                v["frozen"] = json!(s.frozen);
                v["kept"] = json!(s.kept);
                v["ticket"] = json!(tickets.contains(&(s.sid as i32)));
                // The price this card costs now (per-card overrides and discounts).
                let price = g.slot_price(p, i);
                v["price"] = json!(price.map(|x| x.0));
                v["costs_health"] = json!(price.is_some_and(|x| x.1));
                // Triple hint: copies the player already holds and how many make a Golden.
                if let Item::Minion(m) = &s.item
                    && !m.golden
                    && db().get(m.card).golden.is_some()
                {
                    let same = |x: &Minion| !x.golden && x.card == m.card;
                    let owned = pl.board.iter().filter(|x| same(x)).count()
                        + pl.hand
                            .iter()
                            .filter(|h| h.locked == 0 && h.item.as_minion().is_some_and(same))
                            .count();
                    v["owned"] = json!(owned);
                    v["triple_need"] =
                        json!(crate::engine::triple_need_of(pl, &g.cfg, m.card));
                }
                v
            }).collect::<Vec<_>>(),
            "trinkets": pl.trinkets.iter().map(|t| card_json(*t)).collect::<Vec<_>>(),
            "quests": pl.quests.iter().map(|q| json!({
                "quest": card_json(q.card), "reward": card_json(q.reward),
                "progress": q.progress, "goal": q.goal,
            })).collect::<Vec<_>>(),
            "rewards": pl.rewards.iter().map(|t| card_json(*t)).collect::<Vec<_>>(),
            "secrets": pl.secrets.iter().map(|t| card_json(*t)).collect::<Vec<_>>(),
            "hand_queue": pl.hand_queue.iter().map(item_json).collect::<Vec<_>>(),
            "recent_opponents": pl.recent_opponents,
            "buddy_button": pl.buddy_button.as_ref().map(|b| json!({
                "card": card_json(b.card), "claims": b.claims, "price": b.price(),
                "discount": b.discount, "available": g.buddy_available(p),
                "quantity": if b.claims == 0 { 1 } else if b.claims == 1 { 2 } else { 0 },
            })),
            "chronum_balance": pl.chronum_balance,
            "hand_limit": g.cfg.hand_size.max(pl.timewarp_hand_limit),
            "buddy": pl.hero.and_then(|h| db().get(h).buddy).map(card_json),
            "casting": casting,
            "turn_events": pl.turn_events,
            "cancellable": cancellable,
            "dark_gift": {
                "available": g.can_dark_gift(p),
                "uses": pl.dark_gift_uses,
                "max_uses": g.cfg.dark_gift_uses,
                "cost": g.cfg.dark_gift_cost,
                "unlock_turn": g.cfg.dark_gift_unlock_turn,
                // Minion Tiers the button offers this turn and the next one.
                "tiers": g.cfg.dark_gift_tiers(g.turn),
                "next_tiers": g.cfg.dark_gift_tiers(g.turn + 1),
                // The options' Tiers by place, and whether the place is of the common type.
                "slots": g.cfg.dark_gift_slots(g.turn).iter()
                    .map(|(t, typed)| json!({"tiers": t, "typed": typed}))
                    .collect::<Vec<_>>(),
                "used_this_turn": pl.dark_gift_used_turn,
            },
            "deity": pl.deity.as_ref().map(|d| json!({"card": card_json(d.card), "atk": d.atk, "hp": d.hp, "golden": d.golden, "reborn": d.reborn})),
            "effects": pl.effects.iter().map(|x| json!({"card": card_json(x.card), "stacks": x.stacks})).collect::<Vec<_>>(),
            "scheduled": pl.scheduled.iter().map(|s| json!({"card": card_json(s.script), "turns_left": s.turns_left, "if_won": s.if_won})).collect::<Vec<_>>(),
            "stats": serde_json::to_value(&pl.stats).unwrap_or(Value::Null),
            "pending": pending,
            "last_opponent": pl.last_opponent,
            "next_opponent": pl.next_opponent,
            "next_is_ghost": pl.next_is_ghost,
        },
        "opponents": opponents,
        "mask": mask.iter().map(|b| *b as u8).collect::<Vec<_>>(),
        "actions": {
            "END": A_END, "REFRESH": A_REFRESH, "FREEZE": A_FREEZE, "UPGRADE": A_UPGRADE,
            "HERO_POWER": A_HERO_POWER, "DARK_GIFT": A_DARK_GIFT, "CANCEL": A_CANCEL, "BUY": A_BUY, "SELL": A_SELL,
            "ACTIVATE": A_ACTIVATE, "CHOOSE": A_CHOOSE, "PLAY": A_PLAY,
            "MAGNETIZE": A_MAGNETIZE, "MOVE": A_MOVE, "N_POS": N_POS, "N_BOARD": N_BOARD,
        },
        "combats": g.last_combats.iter().filter(|c| combats && (!public || c.a == p || c.b == Some(p))).map(combat_json).collect::<Vec<_>>(),
        "config": serde_json::to_value(&g.cfg).unwrap_or(Value::Null),
    });
    // Card texts with their current numbers.
    let live = live_texts(g, p);
    if !live.is_empty() {
        let me = &mut view["me"];
        let each = |list: &mut Value, f: &dyn Fn(usize, &Value) -> String| {
            for (i, v) in list.as_array_mut().into_iter().flatten().enumerate() {
                let key = f(i, v);
                for (j, a) in v["attached"]
                    .as_array_mut()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    set_live(a, &live, format!("{key}a{j}"));
                }
                set_live(v, &live, key);
            }
        };
        each(&mut me["board"], &|_, v| format!("e{}", v["eid"]));
        each(&mut me["hand"], &|_, v| format!("h{}", v["hid"]));
        each(&mut me["shop"], &|_, v| format!("s{}", v["sid"]));
        each(&mut me["hero_powers"], &|i, _| format!("p{i}"));
        each(&mut me["trinkets"], &|i, _| format!("t{i}"));
        each(&mut me["rewards"], &|i, _| format!("r{i}"));
        for (i, q) in me["quests"]
            .as_array_mut()
            .into_iter()
            .flatten()
            .enumerate()
        {
            set_live(&mut q["quest"], &live, format!("q{i}"));
        }
        set_live(&mut me["hero_power"], &live, "p0".into());
        if me["pending"].is_object() {
            each(&mut me["pending"]["options"], &|i, _| format!("o{i}"));
            let options = me["pending"]["options"].as_array_mut();
            for (i, o) in options.into_iter().flatten().enumerate() {
                if o["reward"].is_object() {
                    set_live(&mut o["reward"], &live, format!("o{i}r"));
                }
            }
        }
        for (i, q) in me["quests"]
            .as_array_mut()
            .into_iter()
            .flatten()
            .enumerate()
        {
            set_live(&mut q["reward"], &live, format!("q{i}r"));
        }
    }
    view
}

/// All cards (for search / pickers in the UI).
pub fn catalog() -> Value {
    Value::Array(db().iter().map(|c| card_json(c.cid)).collect())
}

// ---------------------------------------------------------------------------- commands

#[derive(Deserialize, Debug)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum DebugCmd {
    /// Change player fields (only the given ones).
    SetPlayer {
        p: PlayerId,
        health: Option<i32>,
        armor: Option<i32>,
        tier: Option<u8>,
        gold: Option<i32>,
        max_gold_bonus: Option<i32>,
        gold_next_turn: Option<i32>,
        free_refreshes: Option<i32>,
        upgrade_cost: Option<i32>,
    },
    SetTurn {
        turn: u32,
    },
    /// Create a minion (by card id string) in board/hand/shop.
    AddMinion {
        p: PlayerId,
        zone: String,
        card: String,
        pos: Option<usize>,
        #[serde(default)]
        golden: bool,
        atk: Option<i32>,
        hp: Option<i32>,
    },
    /// Add a spell to hand or shop.
    AddSpell {
        p: PlayerId,
        zone: String,
        card: String,
        #[serde(default)]
        data: i32,
    },
    Remove {
        p: PlayerId,
        zone: String,
        index: usize,
    },
    /// Edit a minion in board/hand/shop.
    EditMinion {
        p: PlayerId,
        zone: String,
        index: usize,
        atk: Option<i32>,
        hp: Option<i32>,
        keywords: Option<Vec<String>>,
        golden: Option<bool>,
        attach: Option<String>,
    },
    ClearZone {
        p: PlayerId,
        zone: String,
    },
    /// Free reroll of the tavern.
    RefreshShop {
        p: PlayerId,
    },
    GiveTrinket {
        p: PlayerId,
        card: String,
    },
    OfferTrinkets {
        p: PlayerId,
        #[serde(default)]
        greater: bool,
    },
    SetDeity {
        p: PlayerId,
        atk: i32,
        hp: i32,
        #[serde(default)]
        golden: bool,
    },
    /// Make every countdown fire at the next turn start (scheduled effects, hand locks).
    AdvanceCountdowns {
        p: PlayerId,
    },
    /// Reset per-turn limits (hero power, Dark Gift, Activate).
    ResetLimits {
        p: PlayerId,
    },
    SetHero {
        p: PlayerId,
        hero: String,
    },
    /// During hero selection: put `hero` among the player's choices (replaces the first).
    OfferHero {
        p: PlayerId,
        hero: String,
    },
    /// Replace one hero offer while retaining lobby and occupied-seat filters.
    RerollHero {
        p: PlayerId,
        index: usize,
        #[serde(default)]
        excluded: Vec<String>,
    },
    /// `on = false`: the seat is exempt from `RulesConfig::turn_action_caps`
    /// (`Player::uncapped`, e.g. a person's seat); `on = true` puts the caps back.
    ActionCap {
        p: PlayerId,
        on: bool,
    },
    /// Put a Hero Power in `slot` (0 or 1; past the end adds one).
    SetHeroPower {
        p: PlayerId,
        slot: usize,
        card: String,
    },
    /// Open a Timewarped Tavern visit (Minor, or Major).
    OpenTimewarp {
        p: PlayerId,
        #[serde(default)]
        major: bool,
    },
    /// Start a Quest (`reward`: its Reward; omitted = a random one of the pool).
    GiveQuest {
        p: PlayerId,
        quest: String,
        reward: Option<String>,
    },
    /// Give a Quest Reward as if its Quest was completed.
    GiveReward {
        p: PlayerId,
        card: String,
    },
    /// Put a Secret into play.
    AddSecret {
        p: PlayerId,
        card: String,
    },
    /// Simulate `n` combats a vs b on copies (no state change).
    CombatPreview {
        a: PlayerId,
        b: PlayerId,
        #[serde(default = "default_n")]
        n: u32,
        /// Scalar analytics without replay frames.
        #[serde(default)]
        details: bool,
        /// Fight b's remembered board as a ghost; it cannot take hero damage.
        #[serde(default)]
        ghost_b: bool,
        /// The board is observed after opening effects, just before a normal attack.
        #[serde(default)]
        after_start: bool,
        #[serde(default)]
        first_attacker: Option<Side>,
    },
}

fn default_n() -> u32 {
    200
}

fn card(id: &str) -> Result<CardId, String> {
    db().find(id).ok_or_else(|| format!("unknown card id {id}"))
}

pub fn run_debug(g: &mut Game, cmd: DebugCmd) -> Result<Value, String> {
    let np = g.players.len();
    let check = |p: PlayerId| {
        if p < np {
            Ok(())
        } else {
            Err(format!("bad player {p}"))
        }
    };
    match cmd {
        DebugCmd::SetPlayer {
            p,
            health,
            armor,
            tier,
            gold,
            max_gold_bonus,
            gold_next_turn,
            free_refreshes,
            upgrade_cost,
        } => {
            check(p)?;
            let pl = &mut g.players[p];
            if let Some(v) = health {
                pl.health = v;
            }
            if let Some(v) = armor {
                pl.armor = v;
            }
            if let Some(v) = tier {
                pl.tier = v.clamp(1, 7);
            }
            if let Some(v) = gold {
                pl.gold = v;
            }
            if let Some(v) = max_gold_bonus {
                pl.max_gold_bonus = v;
            }
            if let Some(v) = gold_next_turn {
                pl.gold_next_turn = v;
            }
            if let Some(v) = free_refreshes {
                pl.free_refreshes = v;
            }
            if let Some(v) = upgrade_cost {
                pl.upgrade_cost = v;
            }
        }
        DebugCmd::SetTurn { turn } => g.turn = turn.max(1),
        DebugCmd::AddMinion {
            p,
            zone,
            card: id,
            pos,
            golden,
            atk,
            hp,
        } => {
            check(p)?;
            let mut c = card(&id)?;
            if db().get(c).kind != CardKind::Minion {
                return Err(format!("{id} is not a minion"));
            }
            if golden {
                c = db().golden_of(c);
            }
            let mut m = g.with_recruit(p, |ctx| ctx.new_minion(0, c));
            if atk.is_some() || hp.is_some() {
                m.set_stats(atk.unwrap_or(m.atk), hp.unwrap_or(m.hp));
            }
            let pl = &mut g.players[p];
            match zone.as_str() {
                "board" => {
                    if pl.board.len() >= g.cfg.board_size {
                        return Err("board is full".into());
                    }
                    let i = pos.unwrap_or(pl.board.len()).min(pl.board.len());
                    pl.board.insert(i, m);
                }
                "hand" => {
                    if pl.hand.len() >= g.cfg.hand_size {
                        return Err("hand is full".into());
                    }
                    g.with_recruit(p, |ctx| ctx.add_to_hand(0, Item::Minion(m)));
                }
                "shop" => pl
                    .shop
                    .push(crate::state::ShopSlot::new(m.eid, Item::Minion(m))),
                _ => return Err("zone must be board/hand/shop".into()),
            }
        }
        DebugCmd::AddSpell {
            p,
            zone,
            card: id,
            data,
        } => {
            check(p)?;
            let c = card(&id)?;
            if !db().get(c).kind.is_spell() {
                return Err(format!("{id} is not a spell"));
            }
            match zone.as_str() {
                "hand" => {
                    g.with_recruit(p, |ctx| ctx.add_to_hand(0, Item::Spell { card: c, data }))
                        .ok_or("hand is full")?;
                }
                "shop" => {
                    let sid = g.alloc_eid();
                    g.players[p].shop.push(crate::state::ShopSlot::new(
                        sid,
                        Item::Spell { card: c, data },
                    ));
                }
                _ => return Err("zone must be hand/shop".into()),
            }
        }
        DebugCmd::Remove { p, zone, index } => {
            check(p)?;
            // A removed minion's pool copies go back, as when it leaves the game by the
            // rules.
            let pl = &mut g.players[p];
            let ok = match zone.as_str() {
                "board" if index < pl.board.len() => {
                    let m = pl.board.remove(index);
                    g.pool.return_minion(&m);
                    true
                }
                "hand" if index < pl.hand.len() => {
                    if let Item::Minion(m) = pl.hand.remove(index).item {
                        g.pool.return_minion(&m);
                    }
                    true
                }
                "shop" if index < pl.shop.len() => {
                    if let Item::Minion(m) = pl.shop.remove(index).item {
                        g.pool.return_minion(&m);
                    }
                    true
                }
                _ => false,
            };
            if !ok {
                return Err("nothing at that index".into());
            }
        }
        DebugCmd::EditMinion {
            p,
            zone,
            index,
            atk,
            hp,
            keywords,
            golden,
            attach,
        } => {
            check(p)?;
            let attach = match attach {
                Some(a) if !a.is_empty() => Some(card(&a)?),
                _ => None,
            };
            let record = g.cfg.record_log;
            let pl = &mut g.players[p];
            let m: &mut Minion = match zone.as_str() {
                "board" => pl.board.get_mut(index),
                "hand" => pl.hand.get_mut(index).and_then(|h| h.item.as_minion_mut()),
                "shop" => pl.shop.get_mut(index).and_then(|s| s.item.as_minion_mut()),
                _ => None,
            }
            .ok_or("no minion there")?;
            if golden == Some(true) && !m.golden {
                crate::content::make_golden(m);
            }
            if atk.is_some() || hp.is_some() {
                let (a, h) = (atk.unwrap_or(m.atk), hp.unwrap_or(m.hp));
                m.atk = a.max(0);
                m.max_hp = h.max(1);
                m.hp = h.max(1);
                if record {
                    m.record(None, crate::state::ModKind::Set, m.atk, m.max_hp);
                }
            }
            if let Some(ks) = keywords {
                let mut k = Kw::NONE;
                for s in &ks {
                    k.set(Kw::from_name(s));
                }
                m.kw = k;
                m.shield_hits = if k.has(Kw::DIVINE_SHIELD) {
                    m.shield_max.max(1)
                } else {
                    0
                };
            }
            if let Some(a) = attach {
                m.attached.push(a);
                // Apply the attached card's static part (e.g. a Dark Gift's stats/keywords).
                let eid = m.eid;
                if let Some(fx) = script(a).and_then(|s| s.on_attach) {
                    let zone_is_board = zone == "board";
                    let p2 = p;
                    g.with_recruit(p2, |ctx| {
                        let me = crate::script::Me {
                            side: 0,
                            eid,
                            card: a,
                            golden: false,
                            opt: 0,
                            pos: 0,
                            data: 0,
                        };
                        if zone_is_board || ctx.find_any_mut(eid).is_some() {
                            let prev_src = ctx.src.replace(me.card);
                            fx(ctx, me);
                            ctx.src = prev_src;
                        }
                    });
                }
            }
        }
        DebugCmd::ClearZone { p, zone } => {
            check(p)?;
            let pl = &mut g.players[p];
            let gone: Vec<Minion> = match zone.as_str() {
                "board" => std::mem::take(&mut pl.board),
                "hand" => std::mem::take(&mut pl.hand)
                    .into_iter()
                    .filter_map(|h| h.item.as_minion().cloned())
                    .collect(),
                "shop" => std::mem::take(&mut pl.shop)
                    .into_iter()
                    .filter_map(|s| s.item.as_minion().cloned())
                    .collect(),
                _ => return Err("zone must be board/hand/shop".into()),
            };
            for m in &gone {
                g.pool.return_minion(m);
            }
        }
        DebugCmd::RefreshShop { p } => {
            check(p)?;
            g.with_recruit(p, |ctx| ctx.refresh_shop(0));
        }
        DebugCmd::GiveTrinket { p, card: id } => {
            check(p)?;
            let c = card(&id)?;
            if !db().get(c).kind.is_trinket() {
                return Err(format!("{id} is not a trinket"));
            }
            g.players[p].trinkets.push(c);
            if let Some(fx) = script(c).and_then(|s| s.on_attach) {
                g.with_recruit(p, |ctx| {
                    let me = crate::script::Me {
                        side: 0,
                        eid: 0,
                        card: c,
                        golden: false,
                        opt: 0,
                        pos: 0,
                        data: 0,
                    };
                    let prev_src = ctx.src.replace(me.card);
                    fx(ctx, me);
                    ctx.src = prev_src;
                    ctx.resolve_deaths();
                });
            }
        }
        DebugCmd::OfferTrinkets { p, greater } => {
            check(p)?;
            let kind = if greater {
                CardKind::TrinketGreater
            } else {
                CardKind::TrinketLesser
            };
            g.offer_trinkets(p, kind);
        }
        DebugCmd::SetDeity { p, atk, hp, golden } => {
            check(p)?;
            if let Some(d) = g.players[p].deity.as_mut() {
                d.atk = atk;
                d.hp = hp;
                d.golden = golden;
            }
        }
        DebugCmd::AdvanceCountdowns { p } => {
            check(p)?;
            let pl = &mut g.players[p];
            for s in pl.scheduled.iter_mut() {
                s.turns_left = s.turns_left.min(1);
            }
            for h in pl.hand.iter_mut() {
                h.locked = 0;
                h.held = false;
            }
        }
        DebugCmd::ResetLimits { p } => {
            check(p)?;
            let pl = &mut g.players[p];
            for h in pl.powers.iter_mut() {
                h.uses_turn = 0;
            }
            pl.dark_gift_used_turn = false;
            for m in pl.board.iter_mut() {
                m.activations = 0;
            }
        }
        DebugCmd::SetHero { p, hero } => {
            check(p)?;
            let h = card(&hero)?;
            let d = db().get(h);
            if d.kind != CardKind::Hero {
                return Err(format!("{hero} is not a hero"));
            }
            if g.cfg.experimental_mode && !crate::experimental::hero_allowed(d) {
                return Err("hero has no enabled Buddy in the experimental mode".into());
            }
            g.players[p].hero = Some(h);
            if g.cfg.experimental_mode {
                g.players[p].buddy_button = d.buddy.map(crate::experimental::BuddyButton::new);
            }
            if let Some(hp) = d.hero_power {
                g.players[p].set_hero_power(0, hp);
            }
        }
        DebugCmd::RerollHero { p, index, excluded } => {
            check(p)?;
            let Some(Pending::HeroSelect { options }) = g.players[p].pending.last() else {
                return Err("the player is not choosing a hero".into());
            };
            if index >= options.len() {
                return Err("invalid hero offer index".into());
            }
            let mut occupied = std::collections::HashSet::new();
            for player in &g.players {
                if let Some(hero) = player.hero {
                    occupied.insert(hero);
                }
                for pending in &player.pending {
                    if let Pending::HeroSelect { options } = pending {
                        occupied.extend(options.iter().copied());
                    }
                }
            }
            let candidates: Vec<CardId> = g
                .lobby_heroes()
                .filter(|c| !occupied.contains(&c.cid) && !excluded.contains(&c.id))
                .filter(|c| {
                    c.hero_power.is_some_and(|hp| {
                        script(hp).is_some()
                            || g.cfg.content == crate::config::ContentMode::AllVanilla
                    })
                })
                .map(|c| c.cid)
                .collect();
            if candidates.is_empty() {
                return Err("no eligible replacement hero remains".into());
            }
            let hero = candidates[g.rng.below(candidates.len())];
            if let Some(Pending::HeroSelect { options }) = g.players[p].pending.last_mut() {
                options[index] = hero;
            }
        }
        DebugCmd::ActionCap { p, on } => {
            check(p)?;
            g.players[p].uncapped = !on;
        }
        DebugCmd::OfferHero { p, hero } => {
            check(p)?;
            let h = card(&hero)?;
            if g.cfg.experimental_mode && !crate::experimental::hero_allowed(db().get(h)) {
                return Err("hero has no enabled Buddy in the experimental mode".into());
            }
            if db().get(h).kind != CardKind::Hero {
                return Err(format!("{hero} is not a hero"));
            }
            let Some(Pending::HeroSelect { options }) = g.players[p].pending.last_mut() else {
                return Err("the player is not choosing a hero".into());
            };
            if !options.contains(&h)
                && let Some(first) = options.first_mut()
            {
                *first = h;
            }
        }
        DebugCmd::SetHeroPower { p, slot, card: id } => {
            check(p)?;
            let c = card(&id)?;
            if db().get(c).kind != CardKind::HeroPower {
                return Err(format!("{id} is not a hero power"));
            }
            g.set_hero_power(p, slot.min(crate::actions::N_POWERS - 1), c);
        }
        DebugCmd::OpenTimewarp { p, major } => {
            check(p)?;
            g.with_recruit(p, |ctx| crate::content::timewarp::open(ctx, 0, major));
            if !matches!(g.players[p].pending.last(), Some(Pending::Timewarp { .. })) {
                return Err("no Timewarp card can be offered".into());
            }
        }
        DebugCmd::GiveQuest { p, quest, reward } => {
            check(p)?;
            let q = card(&quest)?;
            if db().get(q).kind != CardKind::Quest {
                return Err(format!("{quest} is not a quest"));
            }
            let r = match reward {
                Some(r) => card(&r)?,
                None => {
                    let lobby = g.lobby_races;
                    let pool: Vec<CardId> = db()
                        .iter()
                        .filter(|d| d.kind == CardKind::QuestReward && d.normal.is_none())
                        .filter(|d| crate::content::quest_offerable(lobby, &d.id))
                        .filter(|d| script(d.cid).is_some())
                        .map(|d| d.cid)
                        .collect();
                    *g.rng.pick(&pool).ok_or("no reward available")?
                }
            };
            if db().get(r).kind != CardKind::QuestReward {
                return Err("the reward is not a quest reward".into());
            }
            if g.players[p].quests.len() >= crate::obs::N_QUESTS {
                return Err("too many active quests".into());
            }
            // Through the engine's own path: the Quest's start effect runs and its minion
            // types are drawn (an Unlikely Duo given bare could never progress).
            g.with_recruit(p, |ctx| ctx.start_quest(0, q, r));
        }
        DebugCmd::GiveReward { p, card: id } => {
            check(p)?;
            let c = card(&id)?;
            if db().get(c).kind != CardKind::QuestReward {
                return Err(format!("{id} is not a quest reward"));
            }
            g.players[p].rewards.push(c);
            if let Some(fx) = script(c).and_then(|s| s.on_attach) {
                g.with_recruit(p, |ctx| {
                    let me = crate::script::Me {
                        side: 0,
                        eid: 0,
                        card: c,
                        golden: false,
                        opt: 0,
                        pos: 0,
                        data: 0,
                    };
                    fx(ctx, me);
                    ctx.resolve_deaths();
                });
            }
        }
        DebugCmd::AddSecret { p, card: id } => {
            check(p)?;
            let c = card(&id)?;
            if !db().get(c).id.starts_with("TB_Bacon_Secrets_") {
                return Err(format!("{id} is not a secret"));
            }
            if g.players[p].secrets.len() >= crate::state::MAX_SECRETS {
                return Err("too many secrets".into());
            }
            g.players[p].secrets.push(c);
        }
        DebugCmd::CombatPreview {
            a,
            b,
            n,
            details,
            ghost_b,
            after_start,
            first_attacker,
        } => {
            check(a)?;
            check(b)?;
            if a == b {
                return Err("pick two different players".into());
            }
            if after_start && !matches!(first_attacker, Some(0 | 1)) {
                return Err("post-start preview requires observed first attack side".into());
            }
            if !after_start && first_attacker.is_some() {
                return Err("observed first attack side requires post-start snapshot".into());
            }
            // Every iteration clones the whole game: cap it (the server holds its lock).
            let n = n.clamp(1, 5000);
            let mut wins = [0u32; 3];
            let mut dmg = [0i64; 2];
            let mut sample = Value::Null;
            let mut trials = Vec::new();
            for i in 0..n {
                let mut c = g.clone();
                c.cfg.record_log = !details && i == 0;
                c.log.clear();
                c.rng = crate::rng::Rng::new(g.seed ^ (0x9E37_79B9 * (i as u64 + 1)));
                let target = if ghost_b { None } else { Some(b) };
                let ghost = if ghost_b {
                    c.ghost_player = Some(b);
                    Some((c.players[b].board.clone(), c.players[b].tier))
                } else {
                    None
                };
                let out = if after_start {
                    c.simulate_combat_after_start(a, target, ghost, first_attacker.unwrap())
                } else {
                    c.simulate_combat(a, target, ghost)
                };
                wins[out.winner as usize] += 1;
                if out.winner < 2 {
                    dmg[out.winner as usize] += out.damage as i64;
                }
                if details {
                    // Apply the actual native loss-damage path, including the cap,
                    // armor, known Ice Block and other player-level prevention.
                    // The cloned game is discarded; preview never mutates g.
                    let before_a = c.players[a].health + c.players[a].armor;
                    let before_b = c.players[b].health + c.players[b].armor;
                    if !out.truncated {
                        c.apply_combat_result(a, target, out);
                    }
                    let taken = if out.winner == 1 {
                        (before_a - c.players[a].health - c.players[a].armor).max(0)
                    } else {
                        0
                    };
                    let dealt = if out.winner == 0 && !ghost_b {
                        (before_b - c.players[b].health - c.players[b].armor).max(0)
                    } else {
                        0
                    };
                    trials.push(json!({
                        "winner": out.winner,
                        "truncated": out.truncated,
                        "damage_dealt": dealt,
                        "damage_taken": taken,
                        "lethal_win": out.winner == 0 && !ghost_b && c.players[b].health <= 0,
                        "lethal_loss": out.winner == 1 && c.players[a].health <= 0,
                    }));
                } else if i == 0 {
                    let rec = CombatRecord {
                        turn: c.turn,
                        a,
                        b: Some(b),
                        winner: out.winner,
                        damage: out.damage,
                        truncated: out.truncated,
                        frames: std::mem::take(&mut c.last_frames),
                    };
                    sample = combat_json(&rec);
                }
            }
            let n = n as f64;
            let mut result = json!({
                "n": n,
                "win_a": wins[0] as f64 / n,
                "win_b": wins[1] as f64 / n,
                "tie": wins[2] as f64 / n,
                "avg_damage_by_a": if wins[0] > 0 { dmg[0] as f64 / wins[0] as f64 } else { 0.0 },
                "avg_damage_by_b": if wins[1] > 0 { dmg[1] as f64 / wins[1] as f64 } else { 0.0 },
                "sample": sample,
            });
            if details {
                result["trials"] = json!(trials);
                result["after_start"] = json!(after_start);
            }
            return Ok(result);
        }
    }
    Ok(json!({"ok": true}))
}
