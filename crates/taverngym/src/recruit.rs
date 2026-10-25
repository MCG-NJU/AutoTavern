//! Recruit phase: tavern, economy and player actions.

use crate::cards::{CardId, CardKind, cid, db};
use crate::content::FODDER;
use crate::engine::{Ctx, Game};
use crate::event::Event;
use crate::script::{Me, TargetSpec, flag, script};
use crate::state::{Continuation, HandCard, Item, Minion, Pending, ShopSlot};
use crate::types::{EntityId, Kw, Mech, PlayerId, Races, Side};

pub type ActResult = Result<(), &'static str>;

pub const TRIPLE_REWARD: &str = "TB_BaconShop_Triples_01";

// ---------------------------------------------------------------------------- tavern

impl Ctx<'_> {
    /// Minion slots for the owner's current tier (hero/trinket overrides applied).
    pub fn shop_minion_slots(&self, side: Side) -> usize {
        let Some(p) = self.player_ref(side) else {
            return 0;
        };
        let mut n = self.g.cfg.shop_minion_slots[(p.tier as usize - 1).min(6)];
        if p.has_power_id(crate::cardid!("TB_BaconShop_HP_014")) {
            n = n.saturating_sub(1);
        }
        if p.has_trinket_id(crate::cardid!("BG30_MagicItem_841")) {
            n = self
                .g
                .cfg
                .shop_max_cards
                .saturating_sub(self.g.cfg.shop_spell_slots);
        }
        n
    }

    /// Glowing Gauntlet, "The Tavern always has 7 cards": whenever a card is missing it is replaced
    /// at once -- a spell when the Tavern has none, else a minion. Called at the end of every
    /// recruit context.
    pub fn keep_tavern_full(&mut self, side: Side) {
        let Some(p) = self.owner[side] else { return };
        let pl = &self.g.players[p];
        if !pl.has_trinket_id(crate::cardid!("BG30_MagicItem_841"))
            || self.g.phase != crate::engine::Phase::Recruit
            || !pl.alive
            || self.g.ending_turn
        {
            return;
        }
        let tier = pl.tier;
        let cap = self.g.cfg.shop_max_cards;
        while self.g.players[p].shop.len() < cap {
            let has_spell = self.g.players[p]
                .shop
                .iter()
                .any(|s| matches!(s.item, Item::Spell { .. }));
            if !has_spell && self.g.cfg.shop_spell_slots > 0 {
                let Some(s) = self.draw_pool_spell(|d| d.tier <= tier) else {
                    break;
                };
                let sid = self.g.alloc_eid();
                self.g.players[p]
                    .shop
                    .push(ShopSlot::new(sid, Item::spell(s)));
            } else if self.add_shop_minion(side, 1, tier, Races::NONE).is_none() {
                break;
            }
        }
    }

    /// The target a played minion's Battlecry asks for. Magicfin Apprentice asks for the target of
    /// the spell it was taught, like a targeted Battlecry. A Golden Baby N'Zoth makes all
    /// Deathrattle minions Golden and asks for none (its card has no targeting arrow).
    pub fn battlecry_spec(&self, eid: EntityId, s: &crate::script::Script) -> TargetSpec {
        let apprentice = crate::cardid!("BG33_890t");
        let nzoth = crate::cardid!("TB_BaconShop_HERO_93_Buddy");
        match self.minion(eid) {
            Some(m) if db().normal_of(m.card) == apprentice && m.counter > 0 => {
                script(CardId((m.counter - 1) as u16)).map_or(TargetSpec::None, |x| x.spell_target)
            }
            Some(m) if m.golden && db().normal_of(m.card) == nzoth => TargetSpec::None,
            _ => s.battlecry_target,
        }
    }

    /// Lowest Tier the Tavern offers `side`: 3 with Bob-blehead, else 1.
    pub fn tavern_tier_floor(&self, side: Side) -> u8 {
        match self.player_ref(side) {
            Some(p) if p.has_trinket_id(crate::cardid!("BG30_MagicItem_998")) => 3,
            _ => 1,
        }
    }

    /// Draw one minion from the shared pool, weighted by remaining copies.
    pub fn draw_pool_minion(
        &mut self,
        side: Side,
        min_tier: u8,
        max_tier: u8,
        race: Races,
    ) -> Option<Minion> {
        // Bob-blehead: "The Tavern no longer offers cards from Tier 1 or Tier 2" -- every Tavern
        // draw (default fill, extra offers, typed Refreshes).
        let floor = self.tavern_tier_floor(side);
        let min_tier = min_tier.max(floor);
        let max_tier = max_tier.max(min_tier);
        // Two passes (sum, then walk) over the tier range of the compact universe index
        // instead of collecting candidates: this runs for every Tavern slot of every Refresh.
        self.g.universe_index();
        let g = &*self.g;
        let range = g.uidx.range(min_tier, max_tier);
        let fits = |&(tier, races): &(u8, u16)| {
            tier >= min_tier
                && tier <= max_tier
                && (race.is_none() || Races(races).intersects(race))
        };
        let mut total: u64 = 0;
        for i in range.clone() {
            if fits(&g.uidx.meta[i]) {
                total += g.pool.get(g.universe[i]) as u64;
            }
        }
        if total == 0 {
            return None;
        }
        let mut r = (self.g.rng.next_u64() % total) as i64;
        let g = &*self.g;
        let mut pick = None;
        for i in range {
            if !fits(&g.uidx.meta[i]) {
                continue;
            }
            let c = g.universe[i];
            let n = g.pool.get(c);
            if n == 0 {
                continue;
            }
            r -= n as i64;
            if r < 0 {
                pick = Some(c);
                break;
            }
        }
        let pick = pick?;
        let pooled = self.g.pool.take(pick);
        let mut m = self.new_minion(side, pick);
        m.pool_copies = u8::from(pooled);
        self.apply_tavern_buffs(side, &mut m);
        Some(m)
    }

    /// The Tavern's "this game" buffs `m` has while it is in `side`'s Tavern: the client's
    /// BG_ShopBuff_Ench, which it recomputes after a "set to" (`Ctx::set_stats`).
    pub(crate) fn tavern_aura_of(&self, eid: EntityId) -> (i32, i32) {
        if self.combat {
            return (0, 0);
        }
        for s in 0..2 {
            let Some(p) = self.owner[s] else { continue };
            let pl = &self.g.players[p];
            if let Some(m) = pl
                .shop
                .iter()
                .find(|x| x.sid == eid)
                .and_then(|x| x.item.as_minion())
            {
                return tavern_aura(&pl.stats, m);
            }
        }
        (0, 0)
    }

    pub fn apply_tavern_buffs(&self, side: Side, m: &mut Minion) {
        if let Some(p) = self.player_ref(side) {
            // Made for the Tavern: no own-minion trinket auras (Hammer of Twilight) until it
            // is bought (`add_to_hand` adds them).
            crate::engine::apply_tavern_bonuses(&self.g.cfg, p, m);
            let (a, h) = tavern_aura(&p.stats, m);
            m.add_stats(a, h);
            if self.g.cfg.record_log && (a != 0 || h != 0) {
                m.record(None, crate::state::ModKind::Tavern, a, h);
            }
            // The "minions in the Tavern have +N/+N this turn" total so far (Ashen Corruptor,
            // Living Nightmare; kept under Ashen's id): temporary stats, gone at the start of
            // the next turn if still in the Tavern. Acquisition fixes the current bonus.
            let ashen = crate::cardid!("BG32_873");
            let k = p.turn_data.iter().find(|x| x.0 == ashen).map_or(0, |x| x.1);
            if k > 0 {
                m.add_temp_stats(k, k, true);
            }
        }
    }

    /// Return every tavern minion to the pool and clear the tavern.
    pub fn clear_shop(&mut self, side: Side) {
        let Some(p) = self.owner[side] else { return };
        let slots = std::mem::take(&mut self.g.players[p].shop);
        for s in slots {
            if let Item::Minion(m) = s.item {
                self.g.pool.return_minion(&m);
            }
        }
    }

    /// Fill empty minion/spell slots (used after a refresh or when unfreezing).
    pub fn fill_shop(&mut self, side: Side) {
        let Some(p) = self.owner[side] else { return };
        let tier = self.g.players[p].tier;
        let min_tier = if self.g.players[p].has_trinket_id(crate::cardid!("BG30_MagicItem_998")) {
            3
        } else {
            1
        };
        // Filled up to the total card count (34.2 notes): kept (frozen) cards of either kind take
        // slots, e.g. 4 frozen spells on Tier 1 = no new cards.
        let want_min = self.shop_minion_slots(side);
        // Never more than the Tavern holds (7 cards).
        let cap = self
            .shop_fill_cap
            .unwrap_or(want_min + self.g.cfg.shop_spell_slots)
            .min(self.g.cfg.shop_max_cards);
        let have_min = self.g.players[p]
            .shop
            .iter()
            .filter(|s| matches!(s.item, Item::Minion(_)))
            .count();
        let have = self.g.players[p].shop.len();
        let add_min = want_min
            .saturating_sub(have_min)
            .min(cap.saturating_sub(have));
        for _ in 0..add_min {
            let Some(m) = self.draw_pool_minion(side, min_tier, tier, Races::NONE) else {
                break;
            };
            let mut slot = ShopSlot::new(m.eid, Item::Minion(m));
            slot.filled = true;
            self.g.players[p].shop.push(slot);
        }
        let have_sp = self.g.players[p]
            .shop
            .iter()
            .filter(|s| matches!(s.item, Item::Spell { .. }))
            .count();
        let have = self.g.players[p].shop.len();
        let add_sp = self
            .g
            .cfg
            .shop_spell_slots
            .saturating_sub(have_sp)
            .min(cap.saturating_sub(have));
        for _ in 0..add_sp {
            let Some(c) = self.draw_pool_spell(|d| d.tier <= tier) else {
                break;
            };
            let sid = self.g.alloc_eid();
            let mut slot = ShopSlot::new(sid, Item::spell(c));
            slot.filled = true;
            self.g.players[p].shop.push(slot);
        }
    }

    /// Room for a card an effect adds to the Tavern. In a full Tavern it takes the place of
    /// a card of the default Refresh fill (the newest minion first, then the spell): the
    /// default fill has the lowest priority for the 7 slots. False when every card was kept
    /// or added by an effect.
    pub fn shop_make_room(&mut self, side: Side) -> bool {
        let Some(p) = self.owner[side] else {
            return false;
        };
        if self.g.players[p].shop.len() < self.g.cfg.shop_max_cards {
            return true;
        }
        self.g.drop_filled_card(p).is_some()
    }

    /// Add one extra minion (tiers min..=max, optional type) to the Tavern; when it is full
    /// the extra replaces a card of the default fill (`shop_make_room`).
    pub fn add_shop_minion(
        &mut self,
        side: Side,
        min_tier: u8,
        max_tier: u8,
        race: Races,
    ) -> Option<EntityId> {
        let p = self.owner[side]?;
        if !self.shop_make_room(side) {
            return None;
        }
        let m = self.draw_pool_minion(side, min_tier, max_tier, race)?;
        let sid = m.eid;
        self.g.players[p]
            .shop
            .push(ShopSlot::new(sid, Item::Minion(m)));
        Some(sid)
    }

    /// "Refresh the Tavern with <X>": minion slots get only `race` minions (all-type minions
    /// qualify; slots that can't be filled stay empty, and a typeless choice fills none), or
    /// only Tavern spells when `spells`; counts as a Refresh.
    pub fn refresh_shop_with(&mut self, side: Side, race: Races, spells: bool) {
        let Some(p) = self.owner[side] else { return };
        self.clear_shop(side);
        let tier = self.g.players[p].tier;
        let slots = self.shop_minion_slots(side);
        for _ in 0..slots {
            let item = if spells {
                self.draw_pool_spell(|d| d.tier <= tier).map(Item::spell)
            } else if race.is_none() {
                None
            } else {
                self.draw_pool_minion(side, 1, tier, race).map(Item::Minion)
            };
            let Some(item) = item else { break };
            let sid = match &item {
                Item::Minion(m) => m.eid,
                _ => self.g.alloc_eid(),
            };
            self.g.players[p].shop.push(ShopSlot::new(sid, item));
        }
        {
            // The usual Tavern spell slot(s) are filled as normal.
            for _ in 0..self.g.cfg.shop_spell_slots {
                if let Some(c) = self.draw_pool_spell(|d| d.tier <= tier) {
                    let sid = self.g.alloc_eid();
                    self.g.players[p]
                        .shop
                        .push(ShopSlot::new(sid, Item::spell(c)));
                }
            }
        }
        self.finish_refresh(side, false);
    }

    /// An effect's Refresh (card text says "Refresh"): "after the Tavern is Refreshed" and extra
    /// offers trigger, "you Refresh (N times)" doesn't count it.
    pub fn refresh_shop(&mut self, side: Side) {
        self.refresh_shop_ex(side, false);
    }

    /// The Refresh button (paid or free): also counts as "you Refresh".
    pub fn player_refresh_shop(&mut self, side: Side) {
        self.refresh_shop_ex(side, true);
    }

    /// Clear and refill the Tavern. `yours`: the Refresh button (counted); otherwise an effect's
    /// Refresh or the turn-start refill.
    pub fn refresh_shop_ex(&mut self, side: Side, yours: bool) {
        self.clear_shop(side);
        if let Some(f) = self.shop_prefill.take() {
            f(self, side);
        }
        self.fill_shop(side);
        self.shop_fill_cap = None;
        if let Some(f) = self.shop_fixup.take() {
            f(self, side);
        }
        self.finish_refresh(side, yours);
        self.shop_priority.clear();
    }

    /// End of every Refresh: extra offers ("... whenever it is Refreshed"; unfreezing is not a
    /// Refresh), the Refresh count for yours, then the event.
    pub fn finish_refresh(&mut self, side: Side, yours: bool) {
        // Whatever the Refresh itself put in the Tavern (the default fill or a special page:
        // "Refresh with Murlocs", Tess's copies...) has the lowest priority for the 7 slots:
        // the extra offers below take the place of those cards when the Tavern is full.
        let priority = std::mem::take(&mut self.shop_priority);
        if let Some(p) = self.player(side) {
            for s in p.shop.iter_mut() {
                s.filled = !s.kept && !priority.contains(&s.sid);
            }
        }
        self.run_player_hook(side, |s| s.on_fill_shop, "tavern extra");
        self.run_board_hook(side, |s| s.on_fill_shop, 1, "tavern extra");
        // "Add a Fodder to your next Refresh": the Refresh button only, not the turn-start refill
        // or an effect's Refresh.
        if yours {
            self.spawn_fodder(side);
        }
        if yours && let Some(p) = self.player(side) {
            p.stats.refreshes += 1;
        }
        self.emit(Event::Refreshed { side, auto: !yours });
        // A Fodder that came with this Refresh is in the refreshed Tavern: it gets the "after the
        // Tavern is Refreshed" effects (Blood Gem Barrage's Blood Gems) before it feeds itself to a
        // Demon. The minion that then refills its slot comes without a Refresh: it has the Tavern's
        // "this game" bonuses (`draw_pool_minion`), not those effects.
        self.feed_fodder(side);
    }

    /// Clear the frozen mark of Tavern card `sid` (it was transformed / replaced).
    pub fn unfreeze_shop(&mut self, sid: EntityId) {
        for p in self.owner.into_iter().flatten() {
            if let Some(s) = self.g.players[p].shop.iter_mut().find(|s| s.sid == sid) {
                s.frozen = false;
            }
        }
    }

    /// Buff every minion currently in the tavern.
    pub fn buff_tavern(&mut self, side: Side, atk: i32, hp: i32, race: Races) {
        let Some(p) = self.owner[side] else { return };
        if atk > 0 || hp > 0 {
            self.g.players[p].stats.tavern_buffed = true;
        }
        let (record, src) = (self.g.cfg.record_log, self.src);
        for s in self.g.players[p].shop.iter_mut() {
            if let Item::Minion(m) = &mut s.item
                && (race.is_none() || m.races.intersects(race))
            {
                m.add_stats(atk, hp);
                if record {
                    m.record(src, crate::state::ModKind::Add, atk, hp);
                }
            }
        }
    }

    /// Remove any Tavern card (minion or spell) and return it.
    pub fn take_card_from_shop(&mut self, side: Side, sid: EntityId) -> Option<Item> {
        let p = self.owner[side]?;
        let shop = &mut self.g.players[p].shop;
        let i = shop.iter().position(|s| s.sid == sid)?;
        let mut item = shop.remove(i).item;
        if let Item::Minion(m) = &mut item {
            m.keep_tavern_buffs();
        }
        Some(item)
    }

    /// Remove a tavern minion and return it (for steal/consume effects).
    pub fn take_from_shop(&mut self, side: Side, sid: EntityId) -> Option<Minion> {
        let p = self.owner[side]?;
        let shop = &mut self.g.players[p].shop;
        let i = shop.iter().position(|s| s.sid == sid)?;
        match shop.remove(i).item {
            Item::Minion(mut m) => {
                m.keep_tavern_buffs();
                Some(m)
            }
            other => {
                shop.insert(i, ShopSlot::new(sid, other));
                None
            }
        }
    }

    /// "This turn" buff on every minion currently in the tavern (expires at turn start).
    pub fn buff_tavern_temp(&mut self, side: Side, atk: i32, hp: i32) {
        for sid in self.tavern_minions(side) {
            self.buff_temp(sid, atk, hp);
        }
    }

    /// The tavern minion a consume effect takes: random, or the highest-Health one when a
    /// Demon eats with Implicator Portrait.
    pub fn consume_pick(&mut self, side: Side, eater: EntityId) -> Option<EntityId> {
        let demon = self.minion(eater).is_some_and(|m| m.is(Races::DEMON));
        let portrait = demon
            && self
                .player_ref(side)
                .is_some_and(|p| p.has_trinket_id(crate::cardid!("BG32_MagicItem_824")));
        if portrait {
            self.highest_health_tavern(side)
        } else {
            let c = self.tavern_minions(side);
            self.g.rng.pick(&c).copied()
        }
    }

    /// Highest-Health tavern minion (left-most on ties).
    /// Highest-Health Tavern minion (ties: random).
    pub fn highest_health_tavern(&mut self, side: Side) -> Option<EntityId> {
        let p = self.owner[side]?;
        let keyed: Vec<(EntityId, i64)> = self.g.players[p]
            .shop
            .iter()
            .filter_map(|s| s.item.as_minion().map(|m| (s.sid, m.hp as i64)))
            .collect();
        self.pick_max(keyed)
    }

    /// `eater` consumes the tavern minion `sid`: it leaves the tavern (back to the pool) and
    /// `eater` gains `mult` times its stats. Returns the consumed minion's keywords, or None
    /// if `sid` is not a tavern minion.
    pub fn consume(&mut self, side: Side, eater: EntityId, sid: EntityId, mult: i32) -> Option<Kw> {
        let m = self.take_from_shop(side, sid)?;
        self.g.pool.return_minion(&m);
        self.finish_consume(side, eater, &m, mult, true);
        Some(m.kw)
    }

    /// Consume a tavern minion chosen by `consume_pick` (random unless Implicator Portrait).
    pub fn consume_random(&mut self, side: Side, eater: EntityId, mult: i32) -> Option<Kw> {
        let sid = self.consume_pick(side, eater)?;
        self.consume(side, eater, sid, mult)
    }

    /// `eater` consumes a friendly minion on the board: it is removed without dying (no
    /// Deathrattle) and returns to the pool. Returns the consumed minion's stats.
    pub fn consume_friendly(
        &mut self,
        side: Side,
        eater: EntityId,
        target: EntityId,
        mult: i32,
    ) -> Option<(i32, i32)> {
        let (s, i) = self.locate(target)?;
        if s != side || target == eater {
            return None;
        }
        let m = self.boards[s].remove(i);
        if !self.combat {
            self.g.pool.return_minion(&m);
        }
        self.finish_consume(side, eater, &m, mult, false);
        Some((m.atk, m.max_hp))
    }

    fn finish_consume(&mut self, side: Side, eater: EntityId, m: &Minion, mult: i32, tavern: bool) {
        let (atk, hp) = (m.atk, m.max_hp);
        if self.g.cfg.record_log {
            let (who, name) = (self.who(side), m.name().to_string());
            let eater_name = self
                .minion(eater)
                .map(|e| e.name().to_string())
                .unwrap_or_default();
            self.g
                .log(|| format!("{who} {eater_name} consumes {name} ({atk}/{hp})"));
        }
        self.buff(eater, atk.saturating_mul(mult), hp.saturating_mul(mult));
        self.emit(Event::Consumed {
            side,
            eater,
            tavern,
            card: m.card,
            atk,
            hp,
            kw: m.kw,
        });
    }

    /// "Add `n` Fodders to your next `refreshes` Refreshes."
    pub fn add_fodder(&mut self, side: Side, n: u8, refreshes: usize) {
        if let Some(p) = self.player(side) {
            if p.fodder_queue.len() < refreshes {
                p.fodder_queue.resize(refreshes, 0);
            }
            for q in p.fodder_queue.iter_mut().take(refreshes) {
                *q += n;
            }
        }
    }

    /// Put this Refresh's Fodders into the tavern (`finish_refresh` lets them feed once the
    /// Refresh is over).
    fn spawn_fodder(&mut self, side: Side) {
        let Some(p) = self.owner[side] else { return };
        if self.g.players[p].fodder_queue.is_empty() {
            return;
        }
        let n = self.g.players[p].fodder_queue.remove(0);
        let pl = &self.g.players[p];
        let bonus = pl
            .trinkets
            .iter()
            .map(|t| match db().get(*t).id.as_str() {
                "BG35_MagicItem_151" => 4,
                "BG35_MagicItem_151t" => 15,
                _ => 0,
            })
            .sum::<i32>();
        for _ in 0..n {
            // Fodder has priority over the default fill; with no such card left to
            // replace, the Tavern is full of kept and added cards and the Fodder is lost.
            if !self.shop_make_room(side) {
                break;
            }
            let mut m = self.new_minion(side, cid(FODDER));
            m.add_stats(bonus, bonus);
            // A Fodder is a minion in the Tavern like any other card that arrives there:
            // "minions in the Tavern have +X/+X this game" applies to it.
            self.apply_tavern_buffs(side, &mut m);
            let sid = m.eid;
            self.g.players[p]
                .shop
                .push(ShopSlot::new(sid, Item::Minion(m)));
        }
    }

    /// Demon Fodder: "While this is in the Tavern, it feeds itself to a random friendly
    /// Demon and refills the slot." Runs whenever Fodder might be able to feed.
    pub fn feed_fodder(&mut self, side: Side) {
        let Some(p) = self.owner[side] else { return };
        let fodder = crate::cardid!(FODDER);
        loop {
            let found = self.g.players[p]
                .shop
                .iter()
                .enumerate()
                .find_map(|(i, s)| {
                    s.item
                        .as_minion()
                        .filter(|m| db().normal_of(m.card) == fodder)
                        .map(|m| (i, s.sid, if m.golden { 2 } else { 1 }))
                });
            let Some((pos, sid, mult)) = found else {
                return;
            };
            let Some(demon) = self.random_friendly(side, |m| m.is(Races::DEMON)) else {
                return;
            };
            self.consume(side, demon, sid, mult);
            let tier = self.g.players[p].tier;
            if let Some(m) = self.draw_pool_minion(side, 1, tier, Races::NONE) {
                let sid = m.eid;
                let pos = pos.min(self.g.players[p].shop.len());
                self.g.players[p]
                    .shop
                    .insert(pos, ShopSlot::new(sid, Item::Minion(m)));
            }
        }
    }

    /// Sell the board minion at `idx` (gold, pool, "when you sell" effects, Sold event).
    pub fn sell_minion(&mut self, side: Side, idx: usize) {
        let value = self.g.cfg.sell_value;
        let who = self.who(side);
        let m = self.boards[side].remove(idx);
        self.gain_gold(side, value);
        self.g.pool.return_minion(&m);
        let name = m.name();
        self.g.log(|| format!("{who} sells {name}"));
        let baller = db().get(m.normal_card()).name.ends_with("Baller");
        if let Some(pl) = self.player(side) {
            pl.stats.minions_sold += 1;
            pl.stats.ballers_sold += i32::from(baller);
        }
        let cards: Vec<CardId> = std::iter::once(m.card)
            .chain(m.attached.iter().copied())
            .collect();
        for c in cards {
            if let Some(fx) = script(c).and_then(|s| s.on_sell) {
                let golden = if c == m.card {
                    m.golden
                } else {
                    db().get(c).is_golden()
                };
                let me = Me {
                    side,
                    eid: m.eid,
                    card: c,
                    golden,
                    opt: 0,
                    pos: idx,
                    data: m.counter,
                };
                let prev_src = self.src.replace(me.card);
                fx(self, me);
                self.src = prev_src;
            }
        }
        self.pass_hats(side, m.eid);
        self.emit(Event::Sold {
            side,
            card: m.card,
            eid: m.eid,
            atk: m.atk,
            hp: m.max_hp,
        });
    }

    /// Asher the Haberdasher's hats: each +1/+1 hat the sold minion `from` wore passes to a
    /// random friendly minion, which wears it from then on.
    fn pass_hats(&mut self, side: Side, from: EntityId) {
        let Some(pl) = self.player(side) else { return };
        let n = pl.hats.iter().filter(|e| **e == from).count();
        if n == 0 {
            return;
        }
        pl.hats.retain(|e| *e != from);
        let prev = self
            .src
            .replace(crate::cardid!("TB_BaconShop_HERO_36_Buddy"));
        for _ in 0..n {
            if let Some(t) = self.random_friendly(side, |_| true) {
                self.buff(t, 1, 1);
                if let Some(pl) = self.player(side) {
                    pl.hats.push(t);
                }
            }
        }
        self.src = prev;
    }

    /// Finish playing a minion: counters and "after you play" events.
    /// The first half of playing a minion from hand, before its Battlecry: the play counters
    /// and the "whenever you play" listeners (Dragonwing Glider acts before the played minion's
    /// Battlecry). `finish_play` is the "after you play" half.
    pub fn begin_play(&mut self, side: Side, eid: EntityId, played: crate::event::PlayInfo) {
        let (card, golden, elemental) = (
            played.card,
            played.golden,
            played.races.intersects(Races::ELEMENTAL),
        );
        if let Some(p) = self.player(side) {
            p.stats.cards_played += 1;
            p.stats.cards_played_turn += 1;
            p.stats.minions_played_turn += 1;
        }
        if golden {
            // "for each Golden minion you've played" auras (Maritime Extortionist).
            self.bump_player_stat(side, |st| st.golden_played += 1);
        }
        if elemental {
            // Via bump_player_stat: "for each Elemental you played" auras (Frostling).
            self.bump_player_stat(side, |st| {
                st.elementals_played += 1;
                st.elementals_played_turn += 1;
            });
        }
        self.remember_play(side, played.card);
        let prev = std::mem::replace(&mut self.play_phase, 1);
        self.emit(Event::Played { side, eid, played });
        self.emit(Event::CardPlayed { side, card, eid });
        self.play_phase = prev;
    }

    /// The second half, after the Battlecry: "after you play" listeners and what follows.
    pub fn finish_play(&mut self, side: Side, eid: EntityId, mut played: crate::event::PlayInfo) {
        if let Some(m) = self.minion(eid) {
            played.atk = m.atk;
        }
        let card = played.card;
        let prev = std::mem::replace(&mut self.play_phase, 2);
        self.emit(Event::Played { side, eid, played });
        self.emit(Event::CardPlayed { side, card, eid });
        self.play_phase = prev;
        // Spellcraft: a played Spellcraft minion gives its spell right away.
        self.give_spellcraft(side, eid);
        // Tomb Turning: "It dies if you play it this turn."
        if self
            .minion(eid)
            .is_some_and(|m| m.doomed_turn != 0 && m.doomed_turn == self.g.turn)
        {
            self.destroy(eid);
            self.resolve_deaths();
        }
        self.feed_fodder(side);
    }

    fn play_info(&self, side: Side, m: &Minion) -> crate::event::PlayInfo {
        let key = CardId(db().normal_of(m.card).0 | 0x8000);
        crate::event::PlayInfo {
            source: m.eid,
            card: m.card,
            races: m.races,
            atk: m.atk,
            tier: m.tier,
            golden: m.golden,
            battlecry: crate::script::minion_has(m, crate::script::Hook::Battlecry),
            repeated: self.card_data(side, key) == self.g.turn as i32,
        }
    }

    fn remember_play(&mut self, side: Side, card: CardId) {
        let key = CardId(db().normal_of(card).0 | 0x8000);
        self.set_card_data(side, key, self.g.turn as i32);
    }

    /// Target contracts for a Creation's old components whose standalone scripts retain
    /// the historical automatic-target default. The player still chooses for a hand play.
    fn attached_battlecry_spec(&self, eid: EntityId, card: CardId) -> TargetSpec {
        match db().get(db().normal_of(card)).id.as_str() {
            "BG26_RLK_119" => TargetSpec::FriendlyRaceOther(Races::UNDEAD),
            "BG_ICC_094" => TargetSpec::FriendlyOther,
            _ => script(card).map_or(TargetSpec::None, |s| self.battlecry_spec(eid, s)),
        }
    }

    /// Ask for each attached/native Battlecry separately, then announce one completed play.
    fn continue_attached_battlecries(
        &mut self,
        side: Side,
        eid: EntityId,
        mut remaining: Vec<CardId>,
        mult: u32,
        played: Option<crate::event::PlayInfo>,
    ) {
        if remaining.is_empty() {
            if let Some(played) = played {
                self.finish_play(side, eid, played);
            }
            return;
        }
        let card = remaining.remove(0);
        let cont = Continuation::AttachedBattlecry {
            eid,
            card,
            remaining,
            mult,
            opt: 0,
            played,
        };
        if script(card).is_some_and(|s| s.choose_one) {
            self.push_choose_one(side, card, cont, false);
        } else {
            let spec = self.attached_battlecry_spec(eid, card);
            self.resolve_or_ask(side, cont, spec, eid, false);
        }
    }

    fn begin_attached_battlecries(
        &mut self,
        side: Side,
        eid: EntityId,
        played: crate::event::PlayInfo,
    ) {
        let Some(m) = self.minion(eid) else { return };
        let cards = std::iter::once(m.card)
            .chain(m.attached.iter().copied())
            .filter(|card| script(*card).is_some_and(|s| s.battlecry.is_some()))
            .collect();
        let mult = self.battlecry_play_multiplier(side, eid);
        self.continue_attached_battlecries(side, eid, cards, mult, Some(played));
    }

    /// Execute a continuation once its mode/target is known.
    pub fn run_continuation(&mut self, side: Side, cont: Continuation, target: EntityId) {
        match cont {
            Continuation::Battlecry { eid, opt, played } => {
                let played = played.or_else(|| self.minion(eid).map(|m| self.play_info(side, m)));
                self.trigger_battlecry(side, eid, target, opt);
                if let Some(played) = played {
                    self.finish_play(side, eid, played);
                }
            }
            Continuation::AttachedBattlecry {
                eid,
                card,
                remaining,
                mult,
                opt,
                played,
            } => {
                self.trigger_battlecry_part(side, eid, card, target, opt, mult);
                self.continue_attached_battlecries(side, eid, remaining, mult, played);
            }
            Continuation::Spell { card, data, opt } => {
                let times = self.spell_cast_times(side, card, data, target, true);
                for _ in 0..times {
                    self.cast_spell(side, card, data, target, opt);
                }
            }
            Continuation::HandSpell { hid, opt } => {
                self.play_hand_spell(side, hid, opt, target);
            }
            Continuation::Followup { card, first, data } => {
                if let Some(fx) = script(card).and_then(|s| s.followup) {
                    let me = Me {
                        side,
                        eid: first,
                        card,
                        golden: false,
                        opt: 0,
                        pos: 0,
                        data,
                    };
                    let prev_src = self.src.replace(card);
                    fx(self, me, target);
                    self.src = prev_src;
                    self.resolve_deaths();
                }
            }
            Continuation::HeroPower { opt, slot } => {
                let Some(p) = self.owner[side] else { return };
                let slot = slot as usize;
                let Some(hp) = self
                    .player_ref(side)
                    .and_then(|p| p.powers.get(slot))
                    .map(|h| h.card)
                else {
                    return;
                };
                let Some(fx) = script(hp).and_then(|s| s.hero_power) else {
                    return;
                };
                // Pay and count the use only now (targeting was cancellable).
                let price = self.g.hero_power_price(p, slot).unwrap_or(0);
                self.spend_gold(side, price);
                let resets = script(hp).is_some_and(|s| s.has_flag(flag::HP_DISCOUNT_RESETS));
                if let Some(h) = self.player(side).and_then(|pl| pl.powers.get_mut(slot)) {
                    h.uses_turn += 1;
                    h.uses_game += 1;
                    if resets && h.cost_mod < 0 {
                        h.cost_mod = 0;
                    }
                }
                self.g.log(|| format!("p{p} uses hero power"));
                let times = if self
                    .player_ref(side)
                    .is_some_and(|p| p.has_trinket_id(crate::cardid!("BG30_MagicItem_804")))
                {
                    2
                } else {
                    1
                };
                for _ in 0..times {
                    let me = Me {
                        side,
                        eid: 0,
                        card: hp,
                        golden: false,
                        opt,
                        pos: 0,
                        data: 0,
                    };
                    let prev_src = self.src.replace(me.card);
                    fx(self, me, target);
                    self.src = prev_src;
                    self.resolve_deaths();
                }
                // Sous Chef Sticker: after you use your Hero Power, gain 1 Gold.
                let sous_gold = self
                    .player_ref(side)
                    .map_or(0, |p| p.trinket_count(crate::cardid!("BG35_MagicItem_801")))
                    as i32;
                if sous_gold > 0 {
                    self.gain_gold(side, sous_gold);
                    // (Logged: the Gold arrives without any other sign of the trinket.)
                    self.g
                        .log(|| format!("p{p} gains {sous_gold} Gold (Sous Chef Sticker)"));
                }
                // Karl the Lost: after you use your Hero Power, your Divine Shield minions get
                // +2 (+4) Attack.
                let karl = crate::cardid!("TB_BaconShop_HERO_15_Buddy");
                let karls: Vec<i32> = self.boards[side]
                    .iter()
                    .filter(|m| m.is_card_id(karl))
                    .map(|m| if m.golden { 4 } else { 2 })
                    .collect();
                for k in karls {
                    for t in self.friendly_where(side, |m| m.has(Kw::DIVINE_SHIELD)) {
                        self.buff(t, k, 0);
                    }
                }
                // Solemn Serenader: a Hero Power used on a minion gives it half (golden: all)
                // of this minion's Attack; Health on even turns.
                if target != 0 && self.find_any(target).is_some() {
                    let serenader = crate::cardid!("BG26_HERO_102_Buddy");
                    let gifts: Vec<i32> = self.boards[side]
                        .iter()
                        .filter(|m| m.is_card_id(serenader))
                        .map(|m| if m.golden { m.atk } else { m.atk / 2 })
                        .collect();
                    let health = self.g.turn.is_multiple_of(2);
                    for k in gifts {
                        if health {
                            self.buff(target, 0, k);
                        } else {
                            self.buff(target, k, 0);
                        }
                    }
                }
            }
            Continuation::Activate { eid } => {
                let Some(m) = self.minion(eid) else { return };
                let (card, golden) = (m.card, m.golden);
                let efficiency = m
                    .attached
                    .contains(&crate::cardid!("BG36_MidGameEffect_000t78"));
                let cost = script(card)
                    .and_then(|s| s.activate)
                    .map(|a| a.0)
                    .unwrap_or(0);
                if let Some(m) = self.minion_mut(eid) {
                    m.activations += 1;
                }
                self.spend_gold(side, cost);
                let Some((_, _, fx)) = script(card).and_then(|s| s.activate) else {
                    return;
                };
                let times = if efficiency { 2 } else { 1 };
                for _ in 0..times {
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
                    fx(self, me, target);
                    self.src = prev_src;
                    self.resolve_deaths();
                }
            }
        }
        self.resolve_deaths();
    }

    /// Pay for, remove and cast a spell that is still in hand (by hand id).
    pub fn play_hand_spell(&mut self, side: Side, hid: EntityId, opt: u8, target: EntityId) {
        let Some(p) = self.owner[side] else { return };
        let Some(i) = self.g.players[p].hand.iter().position(|h| h.hid == hid) else {
            return;
        };
        let h = self.g.players[p].hand.remove(i);
        let Item::Spell { card, data } = h.item else {
            self.g.players[p].hand.insert(i, h);
            return;
        };
        if h.link != 0
            && let Some(j) = self.g.players[p].hand.iter().position(|x| x.hid == h.link)
        {
            let partner = self.g.players[p].hand.remove(j);
            discard_effects(self, side, partner);
        }
        group_played(self, side, h.group);
        if let Some(pl) = self.player(side) {
            pl.stats.cards_played += 1;
            pl.stats.cards_played_turn += 1;
        }
        let times = self.spell_cast_times(side, card, data, target, true);
        if db().get(card).spellcraft_spell
            && let Some(pl) = self.player(side)
        {
            pl.stats.spellcraft_cast_turn += 1;
        }
        self.emit(Event::CardPlayed {
            side,
            card,
            eid: hid,
        });
        let prev = (
            self.cast_src,
            self.cast_from_hand,
            self.cast_card,
            self.cast_hid,
        );
        self.cast_src = h.src;
        self.cast_hid = hid;
        self.cast_from_hand = true;
        self.cast_card = Some(card);
        // Cast times are fixed when the card is played (before this play counts).
        for _ in 0..times {
            self.cast_spell(side, card, data, target, opt);
        }
        (
            self.cast_src,
            self.cast_from_hand,
            self.cast_card,
            self.cast_hid,
        ) = prev;
    }

    /// The options of one press of the Dark Gift button this turn: by place, as
    /// `RulesConfig::dark_gift_slots` lists them - option k is a minion of its place's Tier, of
    /// your most common type on the board where the place says so (from turn 6 the first, from turn
    /// 10 the first two; the hand does not count), each minion and each gift different, Gilding
    /// only at the offer's lowest Tier. The places are kept (turn 4 is Tier 2, Tier 2, Tier 3 in
    /// this order). Xavius's Hero Power is a free press of the button (the client's 2/2/3 and 4/5/5
    /// agree).
    pub fn dark_gift_options(&mut self, side: Side) -> Vec<Item> {
        let d = db();
        let turn = self.g.turn;
        let n = self.g.cfg.dark_gift_choices.min(3);
        let layout = self.g.cfg.dark_gift_slots(turn);
        let common = if layout.iter().any(|x| x.1) {
            self.most_common_race(side).unwrap_or(Races::NONE)
        } else {
            Races::NONE
        };
        let mut slots: Vec<(u8, Races)> = Vec::with_capacity(n);
        for &(tiers, typed) in layout.iter().take(n) {
            let tier = match tiers {
                [] => return vec![],
                [t] => *t,
                [low, high, ..] => {
                    let pct = if turn == 9 {
                        self.g.cfg.dark_gift_t9_first_t5_pct
                    } else {
                        self.g.cfg.dark_gift_late_t6_pct
                    };
                    if (self.g.rng.below(100) as u64) < pct {
                        *high
                    } else {
                        *low
                    }
                }
            };
            slots.push((tier, if typed { common } else { Races::NONE }));
        }
        let tiers = self.g.cfg.dark_gift_tiers(turn);
        let gifts = crate::content::window_gifts(self);
        let cand: Vec<CardId> = self
            .g
            .universe
            .iter()
            .copied()
            .filter(|c| {
                let def = d.get(*c);
                tiers.contains(&def.tier) && self.g.pool.get(*c) > 0 && gift_discoverable(def, turn)
            })
            .collect();
        crate::content::slotted_gift_options(self, side, &cand, &slots, &gifts)
    }

    /// How many times a spell you cast is cast ("an extra time" / "twice" effects).
    /// `from_hand`: played from hand (a `you_cast` is your spell but not from hand).
    pub(crate) fn spell_cast_times(
        &mut self,
        side: Side,
        _card: CardId,
        data: i32,
        target: EntityId,
        from_hand: bool,
    ) -> u32 {
        let mut n = 1;
        let Some(p) = self.owner[side] else { return 1 };
        let pl = &self.g.players[p];
        // Replica Cathedral: your first spell each turn casts an extra time.
        if pl.stats.spells_cast_turn == 0 {
            n += pl.trinket_count(crate::cardid!("BG30_MagicItem_434"));
        }
        // Hot-Air Surveyor: Blood Gems played from hand cast an extra time (golden: two);
        // "an extra time" stacks per copy.
        let mut gems = 1;
        if from_hand && crate::content::is_blood_gem(_card) {
            let surveyor = crate::cardid!("BG30_121");
            for m in self.boards[side].iter() {
                if m.is_card_id(surveyor) {
                    gems += if m.golden { 2 } else { 1 };
                }
            }
        }
        // Temporal Tampering: your Tavern spells cast an extra time.
        if crate::content::is_tavern_spell_instance(_card, data)
            && self
                .player_ref(side)
                .is_some_and(|p| p.has_reward_id(crate::cardid!("BG28_Reward_501")))
        {
            n += 1;
        }
        // Proud Privateer: Bounties cast twice (golden: three times); "twice" takes the max among
        // copies and adds to the "extra time" sources.
        if crate::content::is_bounty(_card) {
            let privateer = crate::cardid!("BG33_825");
            let k = self.boards[side]
                .iter()
                .filter(|m| m.is_card_id(privateer))
                .map(|m| if m.golden { 3 } else { 2 })
                .max()
                .unwrap_or(1);
            n += k - 1;
        }
        // Spitescale Sushi Roll: your first two Spellcraft spells each turn cast an extra time.
        if db().get(_card).spellcraft_spell
            && let Some(pl) = self.player_ref(side)
            && pl.stats.spellcraft_cast_turn < 2
        {
            n += pl.trinket_count(crate::cardid!("BG30_MagicItem_920"));
        }
        // Balinda Stonehearth: spells that target friendly minions cast twice (golden: three
        // times); "twice" takes the maximum and adds to the other sources - Hot-Air Surveyor's
        // included: Surveyor and Balinda 1 + 1 + 1 = 3, a Golden Surveyor and a Golden Balinda 1 +
        // 2 + 2 = 5.
        let twice = self.balinda_times(side, target);
        gems + (twice - 1) + (n - 1)
    }

    /// Balinda Stonehearth's "Your spells that target friendly minions cast twice (three times)": 1
    /// without her or a friendly target. It also reaches a Blood Gem an effect casts (Victorious
    /// Geomant's, golden Balinda, three casts).
    pub(crate) fn balinda_times(&self, side: Side, target: EntityId) -> u32 {
        if target == 0 || !self.boards[side].iter().any(|m| m.eid == target) {
            return 1;
        }
        let balinda = crate::cardid!("BG35_883");
        self.boards[side]
            .iter()
            .filter(|m| m.is_card_id(balinda))
            .map(|m| if m.golden { 3 } else { 2 })
            .max()
            .unwrap_or(1)
    }

    /// Start a Choose One: ask for the mode, or run both effects combined when an effect
    /// says so (`combined`: the card itself is combined, e.g. from Fandral's Fortune).
    pub fn push_choose_one(
        &mut self,
        side: Side,
        card: CardId,
        cont: Continuation,
        combined: bool,
    ) {
        let power = matches!(cont, Continuation::HeroPower { .. });
        if combined || self.choose_both(side, power) {
            let name = &db().get(card).name;
            self.g
                .log(|| format!("{name}: both Choose One effects combined"));
            self.continue_choose_one(side, card, cont, 2);
            // A Thorned Trailblazer use is only on hold while the spell's target is still
            // being chosen (`Game::cancel` gives it back); otherwise it is spent now.
            let waiting = self.player_ref(side).is_some_and(|p| {
                matches!(
                    p.pending.last(),
                    Some(Pending::Target {
                        cont: Continuation::HandSpell { opt: 2, .. },
                        ..
                    })
                )
            });
            if !waiting {
                self.trailblazer_hold(side, 0);
            }
        } else if let Some(pl) = self.player(side) {
            pl.pending.push(Pending::ChooseOne { card, cont });
        }
    }

    /// Choose One cards with both effects combined: Fandral Staghelm (cards and Hero Powers),
    /// Trailblazer Sticker, or a Thorned Trailblazer use left this turn (consumed only when
    /// nothing else applies).
    fn choose_both(&mut self, side: Side, power: bool) -> bool {
        let fandral = crate::cardid!("BG_OG_044");
        if self.boards[side].iter().any(|m| m.is_card_id(fandral)) {
            return true;
        }
        if power {
            return false;
        }
        if self
            .player_ref(side)
            .is_some_and(|p| p.has_trinket_id(crate::cardid!("BG36_MagicItem_308")))
        {
            return true;
        }
        let trailblazer = crate::cardid!("BG31_327");
        if let Some(m) = self.boards[side]
            .iter_mut()
            .find(|m| m.is_card_id(trailblazer) && m.counter > 0)
        {
            m.counter -= 1;
            let eid = m.eid;
            self.trailblazer_hold(side, eid);
            return true;
        }
        false
    }

    /// Remember (or, with 0, forget) the Thorned Trailblazer whose use the Choose One card
    /// being played has taken: a cancelled play gives it back.
    fn trailblazer_hold(&mut self, side: Side, eid: EntityId) {
        let key = crate::cardid!("BG31_327");
        let Some(p) = self.player(side) else { return };
        p.turn_data.retain(|x| x.0 != key);
        if eid != 0 {
            p.turn_data.push((key, eid as i32));
        }
    }

    /// Continue a Choose One with mode `opt` (0, 1, or 2 = both).
    pub fn continue_choose_one(&mut self, side: Side, card: CardId, cont: Continuation, opt: u8) {
        let s = script(card);
        let cont = match cont {
            Continuation::Battlecry { eid, played, .. } => {
                Continuation::Battlecry { eid, opt, played }
            }
            Continuation::AttachedBattlecry {
                eid,
                card,
                remaining,
                mult,
                played,
                ..
            } => Continuation::AttachedBattlecry {
                eid,
                card,
                remaining,
                mult,
                opt,
                played,
            },
            Continuation::Spell { card, data, .. } => Continuation::Spell { card, data, opt },
            Continuation::HandSpell { hid, .. } => Continuation::HandSpell { hid, opt },
            Continuation::HeroPower { slot, .. } => Continuation::HeroPower { opt, slot },
            c => c,
        };
        let spec = match (&cont, s) {
            (Continuation::Battlecry { .. }, Some(s)) => s.battlecry_target,
            (Continuation::AttachedBattlecry { eid, card, .. }, _) => {
                self.attached_battlecry_spec(*eid, *card)
            }
            (Continuation::Spell { .. } | Continuation::HandSpell { .. }, Some(s)) => {
                s.spell_target
            }
            (Continuation::HeroPower { .. }, Some(s)) => s.hero_power_target,
            _ => TargetSpec::None,
        };
        // Forest's Bounty option B ("your minions +2/+2") has no target.
        let spec = if opt == 1 && db().get(card).id == "BG31_886" {
            TargetSpec::None
        } else {
            spec
        };
        let source = match &cont {
            Continuation::Battlecry { eid, .. } | Continuation::AttachedBattlecry { eid, .. } => {
                *eid
            }
            _ => 0,
        };
        let must = !matches!(
            cont,
            Continuation::Battlecry { .. } | Continuation::AttachedBattlecry { .. }
        );
        self.resolve_or_ask(side, cont, spec, source, must);
    }

    /// Ask for a second target for `card` (choices from `spec`, excluding `first`).
    pub fn ask_followup(
        &mut self,
        side: Side,
        card: CardId,
        first: EntityId,
        data: i32,
        spec: TargetSpec,
    ) {
        let options: Vec<EntityId> = self
            .legal_targets(side, spec, first)
            .into_iter()
            .filter(|e| *e != first)
            .collect();
        if options.is_empty() {
            return;
        }
        let cont = Continuation::Followup { card, first, data };
        if self.combat {
            let t = options[self.g.rng.below(options.len())];
            self.run_continuation(side, cont, t);
        } else if let Some(p) = self.player(side) {
            p.pending.push(Pending::Target { options, cont });
        }
    }

    /// Ask for a target (pushing a pending choice) or run immediately.
    pub fn resolve_or_ask(
        &mut self,
        side: Side,
        cont: Continuation,
        spec: TargetSpec,
        source: EntityId,
        must_target: bool,
    ) {
        if spec == TargetSpec::None {
            self.run_continuation(side, cont, 0);
            return;
        }
        let mut options = self.legal_targets(side, spec, source);
        if matches!(
            cont,
            Continuation::Battlecry { .. } | Continuation::AttachedBattlecry { .. }
        ) {
            // A played minion's Battlecry never targets the minion itself: the client does not let
            // Magicfin Apprentice's taught spell pick its own body, as no targeted Battlecry can.
            options.retain(|e| *e != source);
        }
        if options.is_empty() {
            if !must_target {
                self.run_continuation(side, cont, 0);
            }
            return;
        }
        if let Some(p) = self.player(side) {
            p.pending.push(Pending::Target { options, cont });
        }
    }

    /// Take `item` from a Discover of `source`: the player's pick (`Game::choose`) or the
    /// random pick of a triggered Battlecry (`offer_discover`).
    pub fn take_discover_pick(&mut self, side: Side, source: CardId, item: Item) {
        let from_gift = matches!(
            db().get(source).kind,
            CardKind::DarkGift | CardKind::TrinketLesser | CardKind::TrinketGreater
        ) || crate::content::gift_discover_source(source);
        // The source may take the pick itself (`Script::discover_pick`).
        let item = match script(source).and_then(|s| s.discover_pick) {
            Some(f) => match f(self, side, source, item) {
                Some(item) => item,
                None => {
                    self.check_triples(side);
                    self.resolve_deaths();
                    return;
                }
            },
            None => item,
        };
        let Some(item) = crate::content::consume_discover_choice(self, side, source, item) else {
            self.check_triples(side);
            self.resolve_deaths();
            return;
        };
        let mut item = item;
        // The pick takes a pool copy when one is left (a golden pick holds one copy too).
        if let Item::Minion(m) = &mut item {
            self.take_pool_copy(m);
        }
        let gifts: Vec<CardId> = item
            .as_minion()
            .map(|m| {
                m.attached
                    .iter()
                    .copied()
                    .filter(|a| db().get(*a).kind == CardKind::DarkGift)
                    .collect()
            })
            .unwrap_or_default();
        let card = item.card();
        // A card picked from a trinket's / Quest Reward's Discover keeps their hand protection
        // (with a full hand it waits instead of vanishing).
        let protected = crate::engine::hand_protected(source);
        let prev_src = if protected {
            self.src.replace(source)
        } else {
            self.src
        };
        // The pick triples only after the whole Discover has resolved: after "after you Discover"
        // effects such as Imposing Percussionist's damage, and after a gifted pick's gifts are on
        // (their on_attach would otherwise act on a dead id).
        let prev_no_triples = self.no_triples;
        self.no_triples = true;
        let data = if let Item::Spell { data, .. } = &item {
            *data
        } else {
            0
        };
        let hid = self.add_to_hand(side, item);
        // With a full hand a protected pick waits for space under the hand id it was promised.
        let waiting = if hid.is_none() { self.queued } else { None };
        self.src = prev_src;
        crate::content::after_discover(self, side, source, card, hid, data);
        // A gifted pick gets its gifts before anything hears it was Discovered, so a full copy
        // of it (Sinstone Sticker's) has them too; one that waits for hand space gets them
        // when it arrives.
        if from_gift {
            if let Some(hid) = hid {
                self.attach_gifts(side, hid, &gifts);
            } else if let Some(q) = waiting
                && let Some(pl) = self.player(side)
                && let Some(m) = pl.queue_marks().iter_mut().find(|m| m.hid == q)
            {
                m.gifts = gifts;
            }
        }
        // Only a pick that stays a card is "Discovered" (a Secret put into play or a transform
        // choice is not). One waiting for hand space is a card of yours already: it is Discovered
        // now, and not again when it arrives.
        let kept = match (hid, waiting) {
            (Some(h), _) => self
                .player_ref(side)
                .is_some_and(|p| p.hand.iter().any(|x| x.hid == h)),
            (None, Some(q)) => self
                .player(side)
                .is_some_and(|p| p.queue_marks().iter().any(|m| m.hid == q)),
            (None, None) => false,
        };
        if kept {
            let hid = hid.or(waiting).unwrap_or(0);
            self.emit(Event::Discovered { side, card, hid });
        }
        if let Some(hid) = hid
            && db().get(source).id == "BG31_892"
            && let Some(h) = self
                .player(side)
                .and_then(|pl| pl.hand.iter_mut().find(|h| h.hid == hid))
        {
            h.combined = true;
        }
        self.no_triples = prev_no_triples;
        // No-op while another Discover is still open.
        self.check_triples(side);
        self.resolve_deaths();
    }
}

// ---------------------------------------------------------------------------- prices

impl Game {
    /// Price of Tavern slot `slot`: (amount, paid with Health, the per-turn rule card used --
    /// its `turn_data` is bumped on purchase).
    pub fn slot_price(&self, p: PlayerId, slot: usize) -> Option<(i32, bool, Option<CardId>)> {
        let pl = &self.players[p];
        let s = pl.shop.get(slot)?;
        let card = s.item.card();
        let used = |id: CardId| pl.turn_data.iter().find(|x| x.0 == id).map_or(0, |x| x.1);
        let health = script(card).is_some_and(|x| x.has_flag(flag::HEALTH_COST));
        let has = |id: CardId| pl.has_trinket_id(id);
        let unused = |id: CardId, limit: i32| {
            pl.trinkets
                .iter()
                .copied()
                .find(|t| t.base() == id && used(*t) < limit)
        };
        match &s.item {
            Item::Minion(m) => {
                // Grifter Portrait: the first Pirate you buy each turn is free.
                let grifter = crate::cardid!("BG32_MagicItem_957");
                if m.is(Races::PIRATE)
                    && let Some(grifter) = unused(grifter, 1)
                {
                    return Some((0, false, Some(grifter)));
                }
                // Warcry Totem: the first two Battlecry minions you buy each turn are free.
                let totem = crate::cardid!("BG36_MagicItem_202");
                if db().get(card).mech.has(Mech::BATTLECRY)
                    && let Some(totem) = unused(totem, 2)
                {
                    return Some((0, false, Some(totem)));
                }
                let mut price = s.price.unwrap_or_else(|| self.minion_price(p));
                // Electrode Attractor: Magnetic Mechs cost (2).
                if m.is(Races::MECH)
                    && m.has(Kw::MAGNETIC)
                    && has(crate::cardid!("BG35_MagicItem_743"))
                {
                    price = 2;
                }
                // Pilgrimp Sticker: one Demon each turn costs Health.
                let pilgrimp = crate::cardid!("BG32_MagicItem_821");
                if m.is(Races::DEMON)
                    && let Some(pilgrimp) = unused(pilgrimp, 1)
                {
                    return Some((price, true, Some(pilgrimp)));
                }
                // Demonic Tapestry (armed after 4 Refreshes): the highest-Tier Tavern minion
                // costs Health.
                let tapestry = crate::cardid!("BG35_MagicItem_152");
                if has(tapestry)
                    && crate::content::tapestry_armed(pl)
                    && pl
                        .shop
                        .iter()
                        .filter_map(|x| x.item.as_minion())
                        .all(|x| x.tier <= m.tier)
                {
                    return Some((price, true, Some(tapestry)));
                }
                Some((price, health || crate::content::sargeras_ready(pl), None))
            }
            Item::Spell { .. } => {
                let price = self.spell_price(p, card);
                // Bazaar Sticker: one Tavern spell each turn costs Health.
                let bazaar = crate::cardid!("BG32_MagicItem_822");
                if crate::content::is_tavern_spell(card)
                    && let Some(bazaar) = unused(bazaar, 1)
                {
                    return Some((price, true, Some(bazaar)));
                }
                Some((price, health || crate::content::sargeras_ready(pl), None))
            }
        }
    }

    pub fn minion_price(&self, p: PlayerId) -> i32 {
        let pl = &self.players[p];
        // Aranna Starseeker (unlocked: counter reached 14): first minion each turn is free.
        if let Some(h) = pl
            .power_of(crate::cardid!("TB_BaconShop_HP_065t2"))
            .or_else(|| {
                pl.power_of(crate::cardid!("TB_BaconShop_HP_065"))
                    .filter(|h| h.counter >= 14)
            })
            && h.uses_turn == 0
        {
            return 0;
        }
        if pl.has_power_id(crate::cardid!("TB_BaconShop_HP_054"))
            || pl.has_power_id(crate::cardid!("TB_BaconShop_HP_014"))
            || pl.has_reward_id(crate::cardid!("BG27_Reward_811"))
            || pl
                .effects
                .iter()
                .any(|x| x.card == crate::cardid!("BGS_Treasures_022"))
        {
            2
        } else {
            self.cfg.buy_cost
        }
    }
    pub fn refresh_price(&self, p: PlayerId) -> i32 {
        fn magnus_free(pl: &crate::state::Player) -> bool {
            let magnus = crate::cardid!("TB_BaconShop_HERO_49_Buddy");
            pl.board
                .iter()
                .any(|m| m.is_card_id(magnus) && m.counter > 0)
        }
        let pl = &self.players[p];
        if pl.free_refreshes > 0 || self.pigeon_lord_free(p) || magnus_free(pl) {
            0
        } else if pl.has_power_id(crate::cardid!("TB_BaconShop_HP_054")) {
            2
        } else {
            self.cfg.refresh_cost
        }
    }
    /// The Rat King's minion type this turn: the lobby's types in order, one per turn, from a start
    /// that is random per game (official 17.4 notes). Derived from the seed so it draws nothing
    /// from the game's random stream.
    pub fn rat_king_type(&self) -> Option<Races> {
        let types: Vec<Races> = (0..Races::COUNT)
            .map(|i| Races(1 << i))
            .filter(|r| self.lobby_races.intersects(*r))
            .collect();
        if types.is_empty() {
            return None;
        }
        let start = (self.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 33) as usize;
        Some(types[(start + self.turn as usize) % types.len()])
    }

    /// Pigeon Lord: Refreshes are free while the Tavern lacks the minion type of your Hero
    /// Power (`rat_king_type`).
    fn pigeon_lord_free(&self, p: PlayerId) -> bool {
        let pl = &self.players[p];
        if !pl.has_power_id(crate::cardid!("TB_BaconShop_HP_041"))
            || !pl
                .board
                .iter()
                .any(|m| m.is_card_id(crate::cardid!("TB_BaconShop_HERO_12_Buddy")))
        {
            return false;
        }
        let Some(r) = self.rat_king_type() else {
            return false;
        };
        !pl.shop
            .iter()
            .any(|s| s.item.as_minion().is_some_and(|m| m.races.intersects(r)))
    }
    pub fn upgrade_price(&self, p: PlayerId) -> i32 {
        let pl = &self.players[p];
        let extra = if pl.has_power_id(crate::cardid!("TB_BaconShop_HP_054")) {
            1
        } else {
            0
        };
        (pl.upgrade_cost + extra).max(0)
    }
    pub fn spell_price(&self, p: PlayerId, card: CardId) -> i32 {
        let pl = &self.players[p];
        // Tae'thelan: every third Tavern spell bought costs (0) (counter = spells bought).
        if crate::content::is_tavern_spell(card)
            && pl
                .power_of(crate::cardid!("BG28_HERO_800p"))
                .is_some_and(|h| h.counter % 3 == 2)
        {
            return 0;
        }
        // Advanced Construction (Battlecruiser): 1 Upgrade bought each turn is free.
        if db().get(card).id.starts_with("BG31_HERO_801pt")
            && pl.board.iter().any(|m| {
                m.attached
                    .iter()
                    .any(|a| db().get(*a).id.starts_with("BG31_HERO_801ptf"))
            })
            && pl
                .counters
                .iter()
                .all(|x| !(x.0 == crate::cardid!("BG31_HERO_801ptf") && x.1 == self.turn as i32))
        {
            return 0;
        }
        // Only real Tavern spells consume their discounts; Upgrades are a distinct family.
        if !crate::content::is_tavern_spell(card) {
            return db().get(card).cost;
        }
        // Beyond the Mirage: Tavern spells cost (1) less.
        // Reserve Prices: (1) less this turn per copy cast (`turn_data`).
        let reserve = crate::cardid!("BGS_Treasures_104");
        let mirage = i32::from(pl.has_reward_id(crate::cardid!("BG28_Reward_500")))
            + pl.turn_data
                .iter()
                .find(|x| x.0 == reserve)
                .map_or(0, |x| x.1);
        (db().get(card).cost - pl.stats.spell_discount_next - mirage).max(0)
    }
    pub fn hero_power_price(&self, p: PlayerId, slot: usize) -> Option<i32> {
        let h = self.players[p].powers.get(slot)?;
        let s = script(h.card)?;
        s.hero_power?;
        Some((db().get(h.card).cost + h.cost_mod).max(0))
    }
    pub fn can_upgrade(&self, p: PlayerId) -> bool {
        let pl = &self.players[p];
        pl.tier < self.cfg.max_regular_tier + u8::from(pl.tier7) && pl.gold >= self.upgrade_price(p)
    }

    fn base_upgrade_cost(&self, to_tier: u8) -> i32 {
        let i = (to_tier as usize).saturating_sub(2).min(5);
        self.cfg.upgrade_costs[i]
    }

    fn check_turn(&self, p: PlayerId) -> ActResult {
        let pl = &self.players[p];
        if !pl.alive {
            return Err("player eliminated");
        }
        if pl.ended_turn {
            return Err("turn already ended");
        }
        if !pl.pending.is_empty() {
            return Err("a choice is pending");
        }
        Ok(())
    }

    // ------------------------------------------------------------------------ actions

    pub fn buy(&mut self, p: PlayerId, slot: usize) -> ActResult {
        self.check_turn(p)?;
        let pl = &self.players[p];
        let s = pl.shop.get(slot).ok_or("no such tavern slot")?;
        // A spell cast when bought never enters the hand.
        let cast_on_buy = script(s.item.card()).is_some_and(|x| x.has_flag(flag::CAST_ON_BUY));
        if pl.hand.len() >= self.cfg.hand_size && !cast_on_buy {
            return Err("hand is full");
        }
        let is_minion = matches!(s.item, Item::Minion(_));
        let (price, health_cost, rule) = self.slot_price(p, slot).ok_or("no such tavern slot")?;
        if pl.gold < price && !health_cost {
            return Err("not enough gold");
        }
        let slot_item = self.players[p].shop.remove(slot);
        self.with_recruit(p, |ctx| {
            let card = slot_item.item.card();
            let eid = slot_item.sid;
            let item = slot_item.item;
            let cast_now =
                !is_minion && script(card).is_some_and(|s| s.has_flag(flag::CAST_ON_BUY));
            // The bought card triples only after "after you buy" effects have acted on it
            // (Cooked Book's +2/+2 would otherwise look for a card that has merged away).
            let prev_no_triples = ctx.no_triples;
            ctx.no_triples = true;
            // The bought card is in the hand before anything the payment sets off: with nine cards
            // in hand it takes the last place, and a card that "after you spend Gold" would give
            // (Sky Admiral Rogers' Bounty) finds none.
            let hid = if cast_now {
                None
            } else {
                ctx.add_to_hand(0, item)
            };
            // The Gold is paid now; "after you spend Gold" acts after "after you buy" (Magicfin
            // Mycologist's two Apprentices reached the hand before Enterprising Escapee's Lockbox).
            let paid = if health_cost {
                ctx.hero_damage(0, price);
                false
            } else {
                ctx.pay_gold(0, price)
            };
            if let Some(r) = rule {
                crate::content::price_rule_used(ctx, 0, r);
            }
            {
                let pl = ctx.player(0).unwrap();
                pl.stats.cards_bought += 1;
                if is_minion {
                    pl.stats.minions_bought += 1;
                } else if crate::content::is_tavern_spell(card) {
                    pl.stats.spell_discount_next = 0;
                }
            }
            let name = &db().get(card).name;
            ctx.g.log(|| format!("p{p} buys {name} for {price}"));
            if cast_now {
                // Buying it is casting it: your cast.
                ctx.you_cast(0, card, 0, 0, 0);
            }
            crate::content::starcraft::note_upgrade_bought(ctx, 0, card, price);
            ctx.emit(Event::Bought {
                side: 0,
                card,
                eid: hid.unwrap_or(eid),
                sid: eid,
                price,
            });
            if paid {
                ctx.emit(Event::GoldSpent {
                    side: 0,
                    amount: price,
                });
            }
            ctx.no_triples = prev_no_triples;
            ctx.check_triples(0);
            ctx.resolve_deaths();
        });
        Ok(())
    }

    pub fn sell(&mut self, p: PlayerId, idx: usize) -> ActResult {
        self.check_turn(p)?;
        if idx >= self.players[p].board.len() {
            return Err("no such minion");
        }
        self.with_recruit(p, |ctx| {
            ctx.sell_minion(0, idx);
            ctx.resolve_deaths();
        });
        Ok(())
    }

    fn take_hand_card(&mut self, p: PlayerId, idx: usize) -> Result<HandCard, &'static str> {
        let pl = &self.players[p];
        let h = pl.hand.get(idx).ok_or("no such hand card")?;
        if h.locked > 0 {
            return Err("card is locked");
        }
        Ok(self.players[p].hand.remove(idx))
    }

    /// Discard the partner of a linked pair.
    fn discard_link(ctx: &mut Ctx, link: EntityId) {
        if link == 0 {
            return;
        }
        let Some(p) = ctx.owner[0] else { return };
        if let Some(i) = ctx.g.players[p].hand.iter().position(|h| h.hid == link) {
            let h = ctx.g.players[p].hand.remove(i);
            discard_effects(ctx, 0, h);
            return;
        }
        // The partner is still waiting for hand space: it never arrives.
        let pl = &mut ctx.g.players[p];
        if let Some(i) = pl.queue_marks().iter().position(|m| m.hid == link) {
            let (item, _) = pl.queue_take(i);
            if let Item::Minion(m) = &item {
                ctx.g.pool.return_minion(m);
            }
        }
    }

    pub fn play(&mut self, p: PlayerId, hand_idx: usize, pos: usize) -> ActResult {
        self.check_turn(p)?;
        let h = self.players[p]
            .hand
            .get(hand_idx)
            .ok_or("no such hand card")?;
        match &h.item {
            Item::Minion(m) => {
                if self.players[p].board.len() >= self.cfg.board_size {
                    return Err("board is full");
                }
                let _ = m;
                let h = self.take_hand_card(p, hand_idx)?;
                let tier = self.players[p].tier;
                self.with_recruit(p, |ctx| {
                    let Item::Minion(m) = h.item else {
                        unreachable!()
                    };
                    let reward = m.golden && m.triple_reward;
                    let played = ctx.play_info(0, &m);
                    let card = m.card;
                    let name = m.name();
                    ctx.g.log(|| format!("p{p} plays {name} at {pos}"));
                    let Some(eid) = ctx.summon(0, pos, m) else {
                        return;
                    };
                    Self::discard_link(ctx, h.link);
                    group_played(ctx, 0, h.group);
                    if reward {
                        let item = crate::content::triple_reward_item(ctx, 0, tier);
                        ctx.add_to_hand(0, item);
                    }
                    ctx.begin_play(0, eid, played);
                    let has_attached_bc = ctx.minion(eid).is_some_and(|m| {
                        m.attached
                            .iter()
                            .any(|c| script(*c).is_some_and(|s| s.battlecry.is_some()))
                    });
                    let sc = script(card);
                    if has_attached_bc {
                        ctx.begin_attached_battlecries(0, eid, played);
                    } else {
                        match sc {
                            Some(s) if s.battlecry.is_some() => {
                                if s.choose_one {
                                    ctx.push_choose_one(
                                        0,
                                        card,
                                        Continuation::Battlecry {
                                            eid,
                                            opt: 0,
                                            played: Some(played),
                                        },
                                        h.combined,
                                    );
                                } else {
                                    let spec = ctx.battlecry_spec(eid, s);
                                    ctx.resolve_or_ask(
                                        0,
                                        Continuation::Battlecry {
                                            eid,
                                            opt: 0,
                                            played: Some(played),
                                        },
                                        spec,
                                        eid,
                                        false,
                                    );
                                }
                            }
                            _ => ctx.finish_play(0, eid, played),
                        }
                    }
                    ctx.resolve_deaths();
                });
                Ok(())
            }
            Item::Spell { card, data } => {
                let (card, data) = (*card, *data);
                let s = script(card);
                if s.is_some_and(|s| s.has_flag(flag::UNPLAYABLE)) {
                    return Err("card can't be played");
                }
                // Next-combat spells / Boon of Beetles: one pending effect at a time.
                if s.is_some_and(|s| s.has_flag(flag::NEXT_COMBAT | flag::ONE_PENDING))
                    && self.players[p].effects.iter().any(|x| x.card == card)
                {
                    return Err("its effect is already pending");
                }
                let spec = s.map(|s| s.spell_target).unwrap_or(TargetSpec::None);
                let choose = s.is_some_and(|s| s.choose_one);
                if spec != TargetSpec::None
                    && !choose
                    && self.with_recruit(p, |ctx| ctx.legal_targets(0, spec, 0).is_empty())
                {
                    return Err("no legal target");
                }
                let _ = data;
                let h = &self.players[p].hand[hand_idx];
                if h.locked > 0 {
                    return Err("card is locked");
                }
                let (hid, combined) = (h.hid, h.combined);
                // The card stays in hand (unpaid) until its mode/target is chosen, so the
                // choice can be cancelled like in the game client.
                self.with_recruit(p, |ctx| {
                    let cont = Continuation::HandSpell { hid, opt: 0 };
                    if choose {
                        ctx.push_choose_one(0, card, cont, combined);
                    } else {
                        ctx.resolve_or_ask(0, cont, spec, 0, true);
                    }
                });
                Ok(())
            }
        }
    }

    /// Whether a hand minion can magnetize onto a board minion.
    pub fn can_magnetize(&self, p: PlayerId, hand_idx: usize, board_idx: usize) -> bool {
        let pl = &self.players[p];
        let (Some(h), Some(host)) = (pl.hand.get(hand_idx), pl.board.get(board_idx)) else {
            return false;
        };
        let Item::Minion(m) = &h.item else {
            return false;
        };
        if !m.has(Kw::MAGNETIC) || h.locked > 0 {
            return false;
        }
        if m.is_card_id(crate::cardid!("BG_DEEP_015")) {
            host.is(Races::MECH) || host.is(Races::UNDEAD)
        } else if m.is_card_id(crate::cardid!("BG31_859")) {
            host.is(Races::MECH) || host.is(Races::ELEMENTAL)
        } else if db().get(m.card).id.starts_with("BG25_807t") {
            // Mecha-Demons: "Can Magnetize to Mechs and Demons."
            host.is(Races::MECH) || host.is(Races::DEMON)
        } else {
            host.is(Races::MECH)
        }
    }

    pub fn magnetize(&mut self, p: PlayerId, hand_idx: usize, board_idx: usize) -> ActResult {
        self.check_turn(p)?;
        if !self.can_magnetize(p, hand_idx, board_idx) {
            return Err("can't magnetize there");
        }
        let h = self.take_hand_card(p, hand_idx)?;
        self.with_recruit(p, |ctx| {
            let (link, group) = (h.link, h.group);
            let Item::Minion(m) = h.item else {
                unreachable!()
            };
            let card = m.card;
            let (golden, reward, tier) = (m.golden, m.triple_reward, ctx.tier(0));
            let played = ctx.play_info(0, &m);
            let elemental = m.races.intersects(Races::ELEMENTAL);
            let host = ctx.boards[0][board_idx].eid;
            magnetize_onto(ctx, 0, host, m);
            // Magnetizing from hand is playing that minion: its linked partner is discarded and a
            // linked group counts it, as when it is played to the board.
            Self::discard_link(ctx, link);
            group_played(ctx, 0, group);
            // Magnetizing from hand is playing that minion: play counters, "after you play a
            // minion" (the host stands for the merged minion) and a golden magnet's Triple Reward.
            // Effect-driven Magnetizations are not plays.
            if let Some(pl) = ctx.player(0) {
                pl.stats.cards_played += 1;
                pl.stats.cards_played_turn += 1;
                pl.stats.minions_played_turn += 1;
            }
            if golden {
                ctx.bump_player_stat(0, |st| st.golden_played += 1);
            }
            if elemental {
                ctx.bump_player_stat(0, |st| {
                    st.elementals_played += 1;
                    st.elementals_played_turn += 1;
                });
            }
            if golden && reward {
                let item = crate::content::triple_reward_item(ctx, 0, tier);
                ctx.add_to_hand(0, item);
            }
            ctx.magnet_play = true;
            ctx.remember_play(0, card);
            ctx.emit(Event::Played {
                side: 0,
                eid: host,
                played,
            });
            ctx.emit(Event::CardPlayed {
                side: 0,
                card,
                eid: host,
            });
            ctx.magnet_play = false;
            ctx.resolve_deaths();
        });
        Ok(())
    }

    pub fn move_minion(&mut self, p: PlayerId, from: usize, to: usize) -> ActResult {
        self.check_turn(p)?;
        let b = &mut self.players[p].board;
        if from >= b.len() || to >= b.len() || from == to {
            return Err("bad move");
        }
        let m = b.remove(from);
        b.insert(to, m);
        Ok(())
    }

    pub fn refresh(&mut self, p: PlayerId) -> ActResult {
        self.check_turn(p)?;
        let price = self.refresh_price(p);
        // Malchezaar: some Refreshes each turn cost Health instead of Gold.
        let mal = if price > 0 { self.malchezaar(p) } else { None };
        let pl = &mut self.players[p];
        if pl.gold < price && mal.is_none() {
            return Err("not enough gold");
        }
        // Which free Refresh is used: a Refresh that is free anyway (Pigeon Lord) uses none; this
        // turn's allowance (Magnus's Buddy) goes before banked free Refreshes.
        if !self.pigeon_lord_free(p) {
            let pl = &mut self.players[p];
            let magnus = crate::cardid!("TB_BaconShop_HERO_49_Buddy");
            if let Some(m) = pl
                .board
                .iter_mut()
                .find(|m| m.is_card_id(magnus) && m.counter > 0)
            {
                m.counter -= 1;
            } else if pl.free_refreshes > 0 {
                pl.free_refreshes -= 1;
            }
        }
        self.with_recruit(p, |ctx| {
            if let Some(i) = mal {
                ctx.boards[0][i].counter -= 1;
                // Malchezaar deals it: Jaraxxus' Bloodfury counts it.
                let prev = ctx.src.replace(ctx.boards[0][i].card);
                ctx.hero_damage(0, price);
                ctx.src = prev;
            } else {
                ctx.spend_gold(0, price);
            }
            crate::content::player_refresh(ctx, 0);
            ctx.resolve_deaths();
        });
        Ok(())
    }

    /// Board index of a Malchezaar with a health-paid Refresh left this turn.
    pub fn malchezaar(&self, p: PlayerId) -> Option<usize> {
        let malchezaar = crate::cardid!("BG26_524");
        self.players[p]
            .board
            .iter()
            .position(|m| m.is_card_id(malchezaar) && m.counter > 0)
    }

    /// The Freeze button still answers this turn (`RulesConfig::freeze_presses_per_turn`).
    pub fn can_freeze(&self, p: PlayerId) -> bool {
        // Sindragosa has no Freeze to press: her Tavern freezes by itself at the end of each turn.
        if self.players[p].has_power_id(crate::cardid!("TB_BaconShop_HP_014")) {
            return false;
        }
        let cap = self.cfg.freeze_presses_per_turn;
        cap == 0 || self.players[p].freezes_this_turn < cap
    }

    pub fn freeze(&mut self, p: PlayerId) -> ActResult {
        self.check_turn(p)?;
        if !self.can_freeze(p) {
            return Err("no Freeze left this turn");
        }
        self.players[p].freezes_this_turn += 1;
        // Per card: the button freezes every current card unless they all are frozen already, in
        // which case it unfreezes them.
        let pl = &mut self.players[p];
        let all = !pl.shop.is_empty() && pl.shop.iter().all(|s| s.frozen);
        pl.set_frozen(!all);
        Ok(())
    }

    pub fn upgrade(&mut self, p: PlayerId) -> ActResult {
        self.check_turn(p)?;
        if !self.can_upgrade(p) {
            return Err("can't upgrade");
        }
        let price = self.upgrade_price(p);
        let new_tier = self.players[p].tier + 1;
        let top = self.cfg.max_regular_tier + u8::from(self.players[p].tier7);
        let next_cost = if new_tier < top {
            self.base_upgrade_cost(new_tier + 1)
        } else {
            0
        };
        self.with_recruit(p, |ctx| {
            ctx.spend_gold(0, price);
            let pl = ctx.player(0).unwrap();
            pl.tier = new_tier;
            pl.upgrade_cost = next_cost;
            ctx.g.log(|| format!("p{p} upgrades to tier {new_tier}"));
            crate::content::release_ready_held(ctx, 0, 0);
            ctx.emit(Event::Upgraded {
                side: 0,
                tier: new_tier,
            });
            ctx.resolve_deaths();
        });
        Ok(())
    }

    pub fn can_hero_power(&self, p: PlayerId, slot: usize) -> bool {
        let pl = &self.players[p];
        let Some(h) = pl.powers.get(slot) else {
            return false;
        };
        let hp = h.card;
        let Some(s) = script(hp) else { return false };
        if s.hero_power.is_none() || s.has_flag(flag::PASSIVE) {
            return false;
        }
        // Sous Chef (Buddy): your Hero Power can be used 1 (golden: 2) extra times each turn.
        let sous_chef = crate::cardid!("BG21_HERO_020_Buddy");
        let extra: u32 = pl
            .board
            .iter()
            .filter(|m| m.is_card_id(sous_chef))
            .map(|m| if m.golden { 2 } else { 1 })
            .sum::<u32>()
            // Sous Chef Sticker: an extra use each turn.
            + pl.trinket_count(crate::cardid!("BG35_MagicItem_801"));
        if h.uses_turn >= s.hero_power_uses + extra || h.cooldown > 0 {
            return false;
        }
        let Some(price) = self.hero_power_price(p, slot) else {
            return false;
        };
        if pl.gold < price {
            return false;
        }
        crate::content::hero_power_available(self, p, hp)
    }

    pub fn hero_power(&mut self, p: PlayerId, slot: usize) -> ActResult {
        self.check_turn(p)?;
        if !self.can_hero_power(p, slot) {
            return Err("hero power unavailable");
        }
        let hp = self.players[p].powers[slot].card;
        let s = script(hp).unwrap();
        let spec = s.hero_power_target;
        let choose = s.choose_one;
        if spec != TargetSpec::None
            && !choose
            && self.with_recruit(p, |ctx| ctx.legal_targets(0, spec, 0).is_empty())
        {
            return Err("no legal target");
        }
        // Payment and use count happen when the continuation runs (after targeting).
        self.with_recruit(p, |ctx| {
            let cont = Continuation::HeroPower {
                opt: 0,
                slot: slot as u8,
            };
            if choose {
                ctx.push_choose_one(0, hp, cont, false);
            } else {
                ctx.resolve_or_ask(0, cont, spec, 0, true);
            }
        });
        Ok(())
    }

    pub fn can_activate(&self, p: PlayerId, idx: usize) -> bool {
        let pl = &self.players[p];
        let Some(m) = pl.board.get(idx) else {
            return false;
        };
        let Some((cost, spec, _)) = script(m.card).and_then(|s| s.activate) else {
            return false;
        };
        if m.activations >= 1 || pl.gold < cost {
            return false;
        }
        // The exact targets of `Ctx::legal_targets` for this minion: an Activate that would find
        // none is not legal.
        self.has_targets_from(p, spec, m.eid)
    }

    pub fn activate(&mut self, p: PlayerId, idx: usize) -> ActResult {
        self.check_turn(p)?;
        if !self.can_activate(p, idx) {
            return Err("can't activate");
        }
        let m = &self.players[p].board[idx];
        let eid = m.eid;
        let (_, spec, _) = script(m.card).unwrap().activate.unwrap();
        // Payment and the per-turn use happen when the continuation runs (after targeting).
        self.with_recruit(p, |ctx| {
            ctx.resolve_or_ask(0, Continuation::Activate { eid }, spec, eid, true);
        });
        Ok(())
    }

    pub fn can_dark_gift(&self, p: PlayerId) -> bool {
        let pl = &self.players[p];
        self.turn >= self.cfg.dark_gift_unlock_turn
            && pl.dark_gift_uses < self.cfg.dark_gift_uses
            && !pl.dark_gift_used_turn
            && pl.gold >= self.cfg.dark_gift_cost
            && pl.hand.len() < self.cfg.hand_size
            && !self.gift_universe.is_empty()
    }

    /// Dark Gift button: pay, then Discover one of three minions each carrying a different Dark
    /// Gift (Tiers by `RulesConfig::dark_gift_slots`; weights unknown -> uniform).
    pub fn dark_gift(&mut self, p: PlayerId) -> ActResult {
        self.check_turn(p)?;
        if !self.can_dark_gift(p) {
            return Err("dark gift unavailable");
        }
        let cost = self.cfg.dark_gift_cost;
        self.with_recruit(p, |ctx| {
            ctx.spend_gold(0, cost);
            let pl = ctx.player(0).unwrap();
            pl.dark_gift_uses += 1;
            pl.dark_gift_used_turn = true;
            let options = ctx.dark_gift_options(0);
            ctx.g
                .log(|| format!("p{p} uses the Dark Gift button ({} options)", options.len()));
            if !options.is_empty() {
                let source = options
                    .first()
                    .and_then(|i| i.as_minion())
                    .map(|m| m.attached[0])
                    .unwrap();
                ctx.player(0)
                    .unwrap()
                    .pending
                    .push(Pending::Discover { source, options });
            }
        });
        Ok(())
    }

    /// Back out of a cancellable pending choice (spell/hero power/Activate targeting).
    pub fn cancel(&mut self, p: PlayerId) -> ActResult {
        let pl = &mut self.players[p];
        match pl.pending.last() {
            Some(top) if top.cancellable() => {
                let combined = matches!(
                    top,
                    Pending::Target {
                        cont: Continuation::HandSpell { opt: 2, .. },
                        ..
                    }
                );
                pl.pending.pop();
                // Nothing is spent until the target is chosen: a Thorned Trailblazer use
                // taken when the combined choice was opened comes back.
                let key = crate::cardid!("BG31_327");
                if combined && let Some(i) = pl.turn_data.iter().position(|x| x.0 == key) {
                    let eid = pl.turn_data.remove(i).1 as EntityId;
                    if let Some(m) = pl.board.iter_mut().find(|m| m.eid == eid) {
                        m.counter += 1;
                    }
                }
                self.triples_after_menu(p);
                Ok(())
            }
            _ => Err("nothing to cancel"),
        }
    }

    /// A paid trinket offer that has come to the top with nothing affordable in it is
    /// skipped, as `offer_trinkets` skips one at creation: it cannot be declined, so the
    /// seat would have no legal action.
    fn drop_unaffordable_trinket_offers(&mut self, p: PlayerId) {
        loop {
            let pl = &self.players[p];
            let Some(Pending::Trinket {
                options,
                free: false,
                replace: None,
            }) = pl.pending.last()
            else {
                return;
            };
            if options.iter().any(|t| self.trinket_price(p, *t) <= pl.gold) {
                return;
            }
            let pl = &mut self.players[p];
            pl.pending.pop();
            pl.trinket_pivots = pl.trinket_pivot_stack.pop().unwrap_or_default();
            self.log(|| format!("p{p}: no affordable trinket, offer skipped"));
        }
    }

    fn take_trinket(&mut self, p: PlayerId, t: CardId, free: bool, replace: Option<CardId>) {
        let cost = if free { 0 } else { self.trinket_price(p, t) };
        // The discounts belong to the regular offer: a free or replacement choice opened on
        // top of it leaves them alone. A paid offer waiting underneath gets its own back.
        if !free && replace.is_none() {
            let pl = &mut self.players[p];
            pl.trinket_pivots = pl.trinket_pivot_stack.pop().unwrap_or_default();
        }
        self.with_recruit(p, |ctx| {
            ctx.spend_gold(0, cost);
            let name = &db().get(t).name;
            ctx.g.log(|| format!("p{p} buys trinket {name}"));
            match replace {
                Some(old) => crate::content::replace_trinket(ctx, 0, old, t),
                None => crate::content::grant_trinket(ctx, 0, t),
            }
            // Souvenir Stand: when you buy a Greater Trinket, it becomes a copy of it -- a second
            // trinket with its own counters, whose "when you get it" effect happens again.
            if db().get(t).kind == CardKind::TrinketGreater {
                let stand = db().id("BG30_MagicItem_888");
                if ctx
                    .player_ref(0)
                    .is_some_and(|pl| pl.trinkets.contains(&stand))
                {
                    crate::content::replace_trinket(ctx, 0, stand, t.copy());
                }
            }
            ctx.resolve_deaths();
        });
    }

    /// Whether option `i` of the top pending choice can be taken now.
    pub fn option_legal(&self, p: PlayerId, i: usize) -> bool {
        let pl = &self.players[p];
        match pl.pending.last() {
            None => false,
            Some(Pending::Trinket { options, free, .. }) => options
                .get(i)
                .is_some_and(|t| *free || self.trinket_price(p, *t) <= pl.gold),
            // The last index leaves the Timewarped Tavern; a card needs its Chronum.
            Some(Pending::Timewarp {
                options, chronum, ..
            }) => i == options.len() || options.get(i).is_some_and(|o| o.1 <= *chronum),
            Some(top) => i < top.len(),
        }
    }

    /// Resolve the top pending choice with option `i`.
    pub fn choose(&mut self, p: PlayerId, i: usize) -> ActResult {
        if self.players[p].pending.is_empty() {
            return Err("nothing to choose");
        }
        if !self.option_legal(p, i) {
            return Err("that option can't be chosen");
        }
        let top = self.players[p].pending.pop().unwrap();
        let menu = matches!(top, Pending::Target { .. } | Pending::ChooseOne { .. });
        match top {
            Pending::Trinket {
                options,
                free,
                replace,
            } => {
                self.take_trinket(p, options[i], free, replace);
                self.drop_unaffordable_trinket_offers(p);
            }
            Pending::Quest { options, types } => {
                let (q, r) = options[i];
                let t = types.get(i).copied().unwrap_or((0, 0));
                self.with_recruit(p, |ctx| ctx.start_quest_typed(0, q, r, t));
            }
            Pending::Timewarp {
                mut options,
                mut chronum,
                major,
            } => {
                let bought = (i < options.len()).then(|| options.remove(i));
                if let Some((_, cost)) = &bought {
                    chronum -= cost;
                }
                if self.cfg.experimental_mode {
                    self.players[p].chronum_balance = chronum;
                }
                // The visit goes on while a card is affordable; it sits under whatever
                // the bought card opens.
                if bought.is_some()
                    && (self.cfg.experimental_mode || options.iter().any(|o| o.1 <= chronum))
                {
                    self.players[p].pending.push(Pending::Timewarp {
                        options,
                        chronum,
                        major,
                    });
                } else {
                    // Unspent Chronum is lost (only Timewarped Investment carries one over).
                    self.log(|| format!("p{p} leaves the Timewarp"));
                }
                if let Some((item, _)) = bought {
                    self.with_recruit(p, |ctx| {
                        crate::content::timewarp::bought(ctx, 0, item);
                        ctx.check_triples(0);
                        ctx.resolve_deaths();
                    });
                    if self.cfg.experimental_mode {
                        self.players[p].pending.retain(|x| !matches!(x, Pending::Timewarp { options, chronum, .. } if !options.iter().any(|o| o.1 <= *chronum)));
                    }
                }
            }
            Pending::HeroSelect { options } => {
                self.select_hero(p, options[i]);
            }
            Pending::HeroPowerChoice { options, slot } => {
                let card = options[i];
                self.set_hero_power(p, slot as usize, card);
                // King of Duality: a power picked from Genn's Turn 4 offer acts at once if it would
                // have acted at this turn's start (Xavius / Tickatus fire on Turn 4). A power a
                // spell swaps in waits for its next turn.
                let genn = crate::cardid!("BG35_HERO_001p");
                if let Some(x) = self.players[p]
                    .turn_data
                    .iter_mut()
                    .find(|x| x.0 == genn && x.1 > 0)
                {
                    x.1 -= 1;
                    if let Some(fx) = script(card).and_then(|s| s.start_of_turn) {
                        self.with_recruit(p, |ctx| {
                            let me = Me {
                                side: 0,
                                eid: 0,
                                card,
                                golden: false,
                                opt: 0,
                                pos: 0,
                                data: 0,
                            };
                            let prev_src = ctx.src.replace(card);
                            fx(ctx, me);
                            ctx.src = prev_src;
                            ctx.resolve_deaths();
                        });
                    }
                }
            }
            Pending::Discover { source, options } => {
                let item = options.into_iter().nth(i).unwrap();
                self.with_recruit(p, |ctx| ctx.take_discover_pick(0, source, item));
            }
            Pending::ChooseOne { card, cont } => {
                let opt = i as u8;
                self.with_recruit(p, |ctx| ctx.continue_choose_one(0, card, cont, opt));
            }
            Pending::Target { options, cont } => {
                // The play goes ahead: a Thorned Trailblazer use on hold is spent.
                let key = crate::cardid!("BG31_327");
                self.players[p].turn_data.retain(|x| x.0 != key);
                let t = options[i];
                self.with_recruit(p, |ctx| ctx.run_continuation(0, cont, t));
            }
        }
        if menu {
            self.triples_after_menu(p);
        }
        Ok(())
    }

    /// The triples an open target / Choose One menu held back (`Ctx::check_triples`), once it is
    /// answered or cancelled. Only then: answering a menu adds no triple check where none was held
    /// back.
    fn triples_after_menu(&mut self, p: PlayerId) {
        if std::mem::take(&mut self.players[p].triples_held)
            && self.phase == crate::engine::Phase::Recruit
            && self.players[p].alive
        {
            self.with_recruit(p, |ctx| ctx.check_triples(0));
        }
    }
}

/// Whether a Discover of a minion with a Dark Gift (the button, Xavius, Tras'tath, Sinister
/// Invitation...) can offer `def`: `gift_compatible`, and not Dark Paradox, which already is a
/// minion with a Dark Gift. Maldraxxus Dagger, which copies a friendly minion, is not limited: a
/// Paradox's gifts stack like on a triple.
pub fn gift_discoverable(def: &crate::cards::CardDef, turn: u32) -> bool {
    !def.id.starts_with("BG36_360") && gift_compatible(def, turn)
}

/// Dark Gift minion compatibility (the general rules; examples, not a complete list).
pub fn gift_compatible(def: &crate::cards::CardDef, turn: u32) -> bool {
    if def.mech.has(Mech::MAGNETIC) {
        return false;
    }
    if turn < 5 && (def.mech.has(Mech::BATTLECRY) || def.mech.has(Mech::CHOOSE_ONE)) {
        return false;
    }
    // Client data instead of text matching: hsdata 253216 tag 4785 (excluded from Dark Gifts; the
    // wiki.gg list) and tag 4955 (sell-trigger / hand-linked minions). Normal forms; out-of-pool
    // entries kept so the lists stay complete.
    !GIFT_EXCLUDED.contains(&def.id.as_str()) && !GIFT_SELL_OR_HAND.contains(&def.id.as_str())
}

/// hsdata tag 4785.
const GIFT_EXCLUDED: [&str; 13] = [
    "BG23_318",   // Leeroy the Reckless
    "BG24_018",   // Tortollan Blue Shell
    "BG25_039",   // Pufferquil
    "BG26_137",   // Bream Counter
    "BG32_237",   // Intrepid Botanist
    "BG32_330",   // Flighty Scout
    "BG34_231",   // Old Soul
    "BG34_639",   // Egg of the Endtimes
    "BG36_369",   // Greedy Conniver
    "BG36_620",   // Boom-in-a-Box
    "BG_DAL_775", // Tunnel Blaster
    "BGS_049",    // Freedealing Gambler
    "BGS_131",    // Deadly Spore
];

/// hsdata tag 4955 (normal forms).
const GIFT_SELL_OR_HAND: [&str; 31] = [
    "BG20_301",                   // Sun-Bacon Relaxer
    "BG22_202",                   // Tad
    "BG22_404",                   // Friend of a Friend
    "BG24_017",                   // Ball of Minions
    "BG24_715",                   // Patient Scout
    "BG24_HERO_100_Buddy",        // Shady Aristocrat
    "BG27_508",                   // King Varian
    "BG30_009",                   // Smoldering Gardener
    "BG31_816",                   // Fire Baller
    "BG31_817",                   // Windfall Tornado
    "BG31_818",                   // Snow Baller
    "BG31_827",                   // Coast Guard
    "BG31_875",                   // Tardy Traveler
    "BG31_HERO_811_Buddy",        // Broken Horn
    "BG32_860",                   // Shoalfin Mystic
    "BG33_140",                   // River Skipper
    "BG34_230",                   // Minted Corsair
    "BG34_Giant_333",             // Timewarped Scout
    "BG36_098",                   // Zoatroid
    "BG36_100",                   // Wandering Willbreaker
    "BG36_181",                   // Air Baller
    "BG36_206",                   // Snarky Shark
    "BG36_308",                   // Faceless Operative
    "BGDUO31_201",                // Gathering Stormer
    "BGDUO_HERO_100_Buddy",       // Faceless One
    "BGS_115",                    // Sellemental
    "TB_BaconShop_HERO_38_Buddy", // Crazy Monkey
    "TB_BaconShop_HERO_40_Buddy", // Maxwell, Mighty Steed
    "TB_BaconShop_HERO_68_Buddy", // Sharkbait
    "TB_BaconShop_HERO_91_Buddy", // Phyresz
    "TB_BaconShop_HERO_94_Buddy", // Ticket Collector
];

/// Merge a Magnetic minion into `host`: stats, keywords and scripts transfer.
pub fn magnetize_onto(ctx: &mut Ctx, side: Side, host: EntityId, m: Minion) {
    ctx.guard(crate::guard::Site::Magnetize, u32::from(m.card.0));
    // What the magnet only has "this turn" / "until next turn" stays temporary on the host (a
    // triple inherits it the same way).
    let mut kw = Kw(m.kw.0 & !m.temp_kw.0);
    kw.clear(Kw::MAGNETIC);
    let mut temp_kw = m.temp_kw;
    temp_kw.clear(Kw::MAGNETIC);
    let (ta, th) = (m.temp_atk, m.temp_hp);
    // The magnet's stats as they are go onto the host, its "this game" bonuses included: an Undead
    // Prosthetic Hand brings the Undead Attack it has (the host does not re-derive its own kind
    // of bonus). An attached Volumizer that keeps growing on the host (`volumizer_attached_grow`)
    // is the exception.
    let grows = ctx.g.cfg.volumizer_attached_grow && crate::content::is_volumizer(m.card);
    // Hammer of Twilight's Attack is an aura the host has already: it does not come along (a hand
    // Annoy-o-Module with +46/+23 from a Greater Hammer added 20/39, its Health part included).
    let hammer = ctx
        .player_ref(side)
        .map_or(0, |p| crate::engine::hammer_attack(&p.stats, &p.trinkets))
        .min(m.pbonus_atk.max(0));
    let (a, h) = if grows {
        (m.atk - m.pbonus_atk - ta, m.max_hp - m.pbonus_hp - th)
    } else {
        (m.atk - hammer - ta, m.max_hp - th)
    };
    let record = ctx.g.cfg.record_log;
    // Drone Duplicator: an armed Duplicator host repeats this Magnetization (counter = extra
    // times, reset once used).
    let mut times = 1;
    if let Some(hm) = ctx.minion_mut(host)
        && hm.is_card_id(crate::cardid!("BG36_506"))
        && hm.counter > 0
    {
        times += hm.counter;
        hm.counter = 0;
    }
    // The Magnetized minion's copy goes back to the pool right away (27.0).
    if !ctx.combat {
        ctx.g.pool.return_minion(&m);
    }
    // (An attached Volumizer or Dr. Boom's Monster is a fixed enchantment the same way: its
    // stats now, which hold the bonus so far, before this Magnetization counts.)
    for _ in 0..times {
        if let Some(hm) = ctx.minion_mut(host) {
            hm.add_stats(a, h);
            if record {
                hm.record(Some(m.card), crate::state::ModKind::Magnetic, a, h);
            }
            hm.shield_max = hm.shield_max.max(m.shield_max);
            hm.give(kw);
            let (old_a, old_h) = (hm.atk, hm.max_hp);
            hm.add_stats(ta, th);
            hm.temp_atk = hm.temp_atk.saturating_add(hm.atk.saturating_sub(old_a));
            hm.temp_hp = hm.temp_hp.saturating_add(hm.max_hp.saturating_sub(old_h));
            let fresh = Kw(temp_kw.0 & !hm.kw.0);
            if fresh != Kw::NONE {
                let had = hm.temp_kw;
                let bonus = hm.bonus_kw;
                hm.give(fresh);
                hm.bonus_kw = bonus;
                hm.temp_kw = had.union(fresh);
            }
            // The magnet's Blood Gems are in its Magnetization now, not the host's own Gems (what
            // Confiscate and the like can take).
            hm.attach(m.card, 0);
            hm.extend_attachments_from(&m);
            hm.gained_dr.extend(m.gained_dr.iter().copied());
        }
        // The magnet's stats are stats gained by the host.
        let (a, h) = (a.saturating_add(ta), h.saturating_add(th));
        if a > 0 || h > 0 {
            let prev_src = ctx.src.replace(m.card);
            ctx.stats_changed(host, a.max(0), h.max(0));
            ctx.src = prev_src;
        }
        let name = m.name();
        let who = ctx.who(side);
        ctx.g.log(|| format!("{who} magnetizes {name}"));
        // "The first time this is... Magnetized" effects.
        if (!crate::content::is_volumizer(m.card) || !m.first_play_reward_used)
            && let Some(fx) = script(m.card).and_then(|s| s.on_attach)
        {
            let me = Me {
                side,
                eid: host,
                card: m.card,
                golden: m.golden,
                opt: 0,
                pos: 0,
                data: 0,
            };
            let prev_src = ctx.src.replace(m.card);
            fx(ctx, me);
            ctx.src = prev_src;
        }
        ctx.bump_player_stat(side, |st| st.magnetized += 1);
        ctx.emit(Event::Magnetized {
            side,
            host,
            card: m.card,
        });
    }
    // Polarizing Beatboxer: a Magnetization to a different minion also Magnetizes to it
    // (golden: twice) -- the whole magnet, including what is attached to it. Echoes don't
    // echo again.
    if !ctx.echo_magnetize {
        let beatboxer = crate::cardid!("BG26_149");
        let boxers: Vec<(EntityId, bool)> = ctx.boards[side]
            .iter()
            .filter(|b| b.eid != host && b.alive() && b.is_card_id(beatboxer))
            .map(|b| (b.eid, b.golden))
            .collect();
        ctx.echo_magnetize = true;
        for (b, golden) in boxers {
            for _ in 0..if golden { 2 } else { 1 } {
                let mut echo = m.clone();
                echo.eid = ctx.g.alloc_eid();
                echo.pool_copies = 0;
                magnetize_onto(ctx, side, b, echo);
            }
        }
        ctx.echo_magnetize = false;
    }
}

/// Effects of discarding a hand card (discard is its own event).
pub fn discard_effects(ctx: &mut Ctx, side: Side, h: HandCard) {
    let card = h.item.card();
    if let Item::Minion(m) = &h.item {
        ctx.g.pool.return_minion(m);
    }
    ctx.bump_player_stat(side, |st| st.cards_discarded += 1);
    let name = &db().get(card).name;
    let who = ctx.who(side);
    ctx.g.log(|| format!("{who} discards {name}"));
    crate::content::on_discard(ctx, side, card);
    ctx.emit(Event::Discarded { side, card });
    if let Item::Minion(m) = &h.item {
        crate::content::writhing_tentacles(ctx, side, m);
    }
}

/// Whether a hand card can be discarded: never a locked card (a held card is locked too) nor a
/// Lockbox, whose countdown is its spell data while `locked` stays 0 (strictly forbidden). Discard
/// targets and random discards choose among these only.
pub fn discardable(h: &HandCard) -> bool {
    h.locked == 0 && h.item.card() != crate::cardid!(crate::content::LOCKBOX)
}

/// Discard a specific hand card (by hand id). Returns its card.
pub fn discard_card(ctx: &mut Ctx, side: Side, hid: EntityId) -> Option<CardId> {
    let p = ctx.owner[side]?;
    let i = ctx.g.players[p].hand.iter().position(|h| h.hid == hid)?;
    if !discardable(&ctx.g.players[p].hand[i]) {
        return None;
    }
    let h = ctx.g.players[p].hand.remove(i);
    let card = h.item.card();
    discard_effects(ctx, side, h);
    Some(card)
}

/// A card of linked group `group` was played: the group has one play less; at zero the rest of the
/// group is discarded.
pub fn group_played(ctx: &mut Ctx, side: Side, group: EntityId) {
    if group == 0 {
        return;
    }
    let Some(p) = ctx.owner[side] else { return };
    let mut done = false;
    for h in ctx.g.players[p]
        .hand
        .iter_mut()
        .filter(|h| h.group == group)
    {
        h.group_left = h.group_left.saturating_sub(1);
        done |= h.group_left == 0;
    }
    while done && let Some(i) = ctx.g.players[p].hand.iter().position(|h| h.group == group) {
        let h = ctx.g.players[p].hand.remove(i);
        discard_effects(ctx, side, h);
    }
}

/// Add `cards` to hand as one linked group: after `plays` of them are played, the rest
/// are discarded.
pub fn give_linked_group(ctx: &mut Ctx, side: Side, cards: &[CardId], plays: u8) {
    let hids: Vec<EntityId> = cards
        .iter()
        .filter_map(|c| ctx.give_card(side, *c))
        .collect();
    let (Some(&group), Some(p)) = (hids.first(), ctx.owner[side]) else {
        return;
    };
    for h in ctx.g.players[p]
        .hand
        .iter_mut()
        .filter(|h| hids.contains(&h.hid))
    {
        h.group = group;
        h.group_left = plays;
    }
}

/// Add two cards to hand as a linked pair: playing one discards the other.
pub fn give_linked_pair(ctx: &mut Ctx, side: Side, a: CardId, b: CardId) {
    // A card that has to wait for hand space is linked all the same (`Ctx::queued`).
    let ha = ctx.give_card(side, a).or(ctx.queued);
    let hb = ctx.give_card(side, b).or(ctx.queued);
    let (Some(ha), Some(hb)) = (ha, hb) else {
        return;
    };
    ctx.set_hand_link(side, ha, hb);
    ctx.set_hand_link(side, hb, ha);
}

/// The Tavern's "this game" stats for `m` (all Tavern minions, Elementals, a minion type).
pub(crate) fn tavern_aura(st: &crate::state::PlayerStats, m: &Minion) -> (i32, i32) {
    let (mut a, mut h) = (st.tavern_atk, st.tavern_hp);
    if m.races.intersects(Races::ELEMENTAL) {
        a = a.saturating_add(st.tavern_elemental_atk);
        h = h.saturating_add(st.tavern_elemental_hp);
    }
    for &(r, ra, rh) in &st.tavern_race {
        if m.races.intersects(Races(r)) {
            a = a.saturating_add(ra);
            h = h.saturating_add(rh);
        }
    }
    (a, h)
}
