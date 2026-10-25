//! Per-trinket offer conditions and hero exclusions from the official "Battlegrounds Developer
//! Insights: Updated Trinket Rules". Checked by
//! `Game::trinket_eligible` on top of the cards' own `offer_ok`. Out-of-pool trinkets named in the
//! post are listed too, so they work if they return.

use crate::cards::db;
use crate::engine::Game;
use crate::script::{Hook, minion_has};
use crate::state::{Minion, Player};
use crate::types::{PlayerId, Races};

/// Trinkets of which an offer holds at most one (besides the menagerie cap).
pub const EXCLUSIVE_GROUPS: [&[&str]; 2] = [
    &[
        "BG30_MagicItem_888",
        "BG30_MagicItem_891",
        "BG32_MagicItem_271",
    ],
    &["BG32_MagicItem_400", "BG32_MagicItem_844"],
];

/// The menagerie trinkets (hsdata 253932 tag 3551, pool trinkets: Glowing Crystal, Aggem
/// Sticker, Multilayered Shield, Tableware Portrait). You are "in" menagerie with 3 or more
/// distinct minion types on board; an offer holds at most one of them, and they are not
/// no-type trinkets.
pub const MENAGERIE: [&str; 4] = [
    "BG36_MagicItem_220",
    "BG32_MagicItem_284",
    "BG36_MagicItem_373",
    "BG36_MagicItem_611",
];

/// Typed trinkets that may be offered at a discount when you are not "in" their type (hsdata 253216
/// tag 3443 `BACON_TRINKET_CAN_BE_DISCOUNTED`, pool trinkets). Every other typed trinket is only
/// offered when you are in its type.
pub const DISCOUNTABLE: [&str; 34] = [
    "BG30_MagicItem_700",
    "BG30_MagicItem_841",
    "BG32_MagicItem_284",
    "BG32_MagicItem_306",
    "BG32_MagicItem_350",
    "BG32_MagicItem_416",
    "BG32_MagicItem_926",
    "BG32_MagicItem_951",
    "BG35_MagicItem_712",
    "BG35_MagicItem_740",
    "BG35_MagicItem_750",
    "BG35_MagicItem_753",
    "BG35_MagicItem_755",
    "BG35_MagicItem_840",
    "BG35_MagicItem_890",
    "BG36_MagicItem_200",
    "BG36_MagicItem_202",
    "BG36_MagicItem_203",
    "BG36_MagicItem_212",
    "BG36_MagicItem_213",
    "BG36_MagicItem_214",
    "BG36_MagicItem_300",
    "BG36_MagicItem_301",
    "BG36_MagicItem_302",
    "BG36_MagicItem_302t",
    "BG36_MagicItem_303",
    "BG36_MagicItem_303t",
    "BG36_MagicItem_363",
    "BG36_MagicItem_390",
    "BG36_MagicItem_400",
    "BG36_MagicItem_403",
    "BG36_MagicItem_403t",
    "BG36_MagicItem_424",
    "BG36_MagicItem_852",
];

/// Offer weights in percent (hsdata 253932 tag 3524; unlisted = 100). 36.6.3 removed the boost of
/// the 36.6 trinkets.
const WEIGHTS: [(&str, u64); 9] = [
    ("BG30_MagicItem_435", 50),
    ("BG30_MagicItem_801", 200),
    ("BG30_MagicItem_888", 250),
    ("BG30_MagicItem_891", 250),
    ("BG31_MagicItem_903", 25),
    ("BG35_MagicItem_817", 150),
    ("BG35_MagicItem_842", 25),
    ("BG36_MagicItem_363", 50),
    ("BG36_MagicItem_372", 150),
];

/// The "more likely" no-type trinkets of the official post and the other cards carrying the unnamed
/// client flag 4692: weight times `RulesConfig::trinket_likely_pct` (the boost is unpublished; 200
/// from client offers).
const LIKELY: [&str; 14] = [
    "BG30_MagicItem_426",
    "BG30_MagicItem_426t",
    "BG30_MagicItem_435",
    "BG30_MagicItem_706",
    "BG30_MagicItem_876",
    "BG32_MagicItem_901",
    "BG35_MagicItem_930",
    "BG35_MagicItem_931",
    "BG35_MagicItem_931t",
    "BG36_MagicItem_206",
    "BG36_MagicItem_208",
    "BG36_MagicItem_309",
    "BG36_MagicItem_370",
    "BG36_MagicItem_610",
];

/// Offer weight of trinket `id`.
pub fn weight(g: &Game, id: &str) -> u64 {
    let w = WEIGHTS.iter().find(|x| x.0 == id).map_or(100, |x| x.1);
    if LIKELY.contains(&id) {
        w * g.cfg.trinket_likely_pct / 100
    } else {
        w
    }
}

/// The minion types a trinket belongs to for the offer rules: its associated types and, for the
/// Aberration subset (hsdata tag 4757, `lobby_req`), Aberration. Four trinkets of that subset carry
/// the "can be discounted" tag, which only means something for a typed trinket: the subset is read
/// as their type.
pub fn types(t: crate::cards::CardId) -> Races {
    let d = db().get(t);
    d.assoc.union(d.lobby_req)
}

fn minions(p: &Player) -> impl Iterator<Item = &Minion> {
    p.board
        .iter()
        .chain(p.hand.iter().filter_map(|h| h.item.as_minion()))
}

/// The "no type" trinkets (unnamed client tag 3519; their text is about minions with no type):
/// offered only when you are "in" no type, with 2 (Lesser) / 3 (Greater) minions with no type on
/// board (official post: "3 minions with no type").
/// They still count as no-type trinkets.
pub const NO_TYPE: [&str; 3] = [
    "BG30_MagicItem_403", // Ironforge Anvil
    "BG30_MagicItem_981", // The Eye of Dalaran
    "BG32_MagicItem_281", // Wizard's Pipe (not in the pool)
];

/// "A Health rewinder effect": a minion on board or in hand that rewinds your hero's damage
/// (Soul Rewinder, Ashen Corruptor, Archimonde... -- their text says "rewind it"). The post
/// gives no list; this reading is an assumption.
fn has_rewinder(p: &Player) -> bool {
    minions(p).any(|m| db().get(db().normal_of(m.card)).text.contains("rewind it"))
}

/// Whether trinket `id` may be offered to player `p` under the 2026-04-06 rules.
pub fn offer_rule(g: &Game, p: PlayerId, id: &str) -> bool {
    let pl = &g.players[p];
    let hero = |h: &str| pl.hero_is(h);
    let deathrattle = || pl.board.iter().any(|m| minion_has(m, Hook::Deathrattle));
    match id {
        // Hero exclusions.
        "BG30_MagicItem_888" | "BG30_MagicItem_891" | "BG32_MagicItem_271" => {
            !hero("BG30_HERO_304") && !hero("BG32_HERO_002") // Marin, Buttons
        }
        "BG35_MagicItem_812" => !hero("BG34_HERO_002"), // Corrupted Tome: Clocksworth
        "BG30_MagicItem_847" | "BG30_MagicItem_996" => !hero("TB_BaconShop_HERO_10"), // Gallywix
        "BG35_MagicItem_303" => !hero("TB_BaconShop_HERO_10") && !hero("TB_BaconShop_HERO_55"),
        "BG35_MagicItem_821" | "BG35_MagicItem_821t" => {
            !hero("BG26_HERO_104") && !hero("BG25_HERO_105") // Voone, E.T.C.
        }
        "BG30_MagicItem_821" => !hero("TB_BaconShop_HERO_95"), // Fish Portrait: Greybough
        // Shrine of Evolution / Sacrificial Altar: 6 or more minions.
        "BG32_MagicItem_400" | "BG32_MagicItem_844" => pl.board.len() >= 6,
        // Health rewinder trinkets: Demonic Tapestry, Eye of Sargeras, Nether Pendant,
        // Felburned Ledger.
        "BG35_MagicItem_152" | "BG30_MagicItem_701" | "BG30_MagicItem_541"
        | "BG35_MagicItem_155" => has_rewinder(pl),
        // Scraper Sticker, Beatboxer Portrait: Tier 3 or higher.
        "BG35_MagicItem_301" | "BG35_MagicItem_741" => pl.tier >= 3,
        // Magician's Top Hat: fewer than 5 minions in hand and on board.
        "BG35_MagicItem_815" => minions(pl).count() < 5,
        // Lens Case: you control a Tier 3 minion.
        "BG35_MagicItem_817" => pl.board.iter().any(|m| m.tier == 3),
        // Bob-blehead: not in Demon lobbies.
        "BG30_MagicItem_998" => !g.lobby_races.intersects(Races::DEMON),
        // Ur'zul Sticker, Flaming Portrait: you buffed the Tavern this game (a one-off buff
        // or a Tavern aura).
        "BG35_MagicItem_154" | "BG35_MagicItem_156" => {
            pl.stats.tavern_buffed
                || pl.stats.tavern_atk > 0
                || pl.stats.tavern_hp > 0
                || !pl.stats.tavern_race.is_empty()
        }
        // Murky Sticker: "multiple" Battlecries triggered this game (2+ assumed).
        "BG35_MagicItem_753" => pl.stats.battlecries_triggered >= 2,
        // Dramaloc Sticker: a minion with 10+ Attack in hand.
        "BG35_MagicItem_754" => pl
            .hand
            .iter()
            .filter_map(|h| h.item.as_minion())
            .any(|m| m.atk >= 10),
        // Herald Sticker, Blood Amulet, Unholy Sanctum, Thornspike Pauldron: you control a
        // Deathrattle minion.
        "BG32_MagicItem_306"
        | "BG35_MagicItem_432"
        | "BG32_MagicItem_862"
        | "BG32_MagicItem_862t"
        | "BG35_MagicItem_431t" => deathrattle(),
        // Safety Patch: under 16 Health + Armor.
        "BG35_MagicItem_820" => pl.health + pl.armor < 16,
        // (Baller Portrait's "you sold a Baller" of the post and Ghastly Sticker's "an end of turn
        // minion" of the 2025 post do not hold in client offers: no condition)
        id if NO_TYPE.contains(&id) => {
            let greater = db().get(db().id(id)).kind == crate::cards::CardKind::TrinketGreater;
            let need = g.cfg.trinket_type_min[usize::from(greater)];
            g.offer_minion_types(p)
                .iter()
                .filter(|r| r.is_none())
                .count()
                >= need
        }
        // Sous Chef Sticker ("use your Hero Power an extra time"): not for a hero whose Hero Power
        // is passive.
        "BG35_MagicItem_801" => pl
            .hero_power()
            .and_then(crate::script::script)
            .is_some_and(|s| s.hero_power.is_some() && !s.has_flag(crate::script::flag::PASSIVE)),
        // Tiger Carving: Trigore, Iridescent Skyblazer, Spiked Savior or Rabid Panther.
        "BG30_MagicItem_427" | "BG30_MagicItem_427t" => pl.board.iter().any(|m| {
            matches!(
                db().get(db().normal_of(m.card)).name.as_str(),
                "Trigore the Lasher" | "Iridescent Skyblazer" | "Spiked Savior" | "Rabid Panther"
            )
        }),
        // Divine Signet: a Divine Shield minion.
        "BG32_MagicItem_171" => pl
            .board
            .iter()
            .any(|m| m.kw.has(crate::types::Kw::DIVINE_SHIELD)),
        // Fancy Spellbook, Heart of the Forest: you buffed your Tavern spells this game.
        "BG30_MagicItem_999" | "BG32_MagicItem_801" | "BG32_MagicItem_801t" => {
            pl.stats.spell_atk > 0 || pl.stats.spell_hp > 0
        }
        // Gilded Anchor: you control a Golden minion.
        "BG32_MagicItem_231" | "BG32_MagicItem_231t" => pl.board.iter().any(|m| m.golden),
        _ => true,
    }
}
