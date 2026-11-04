//! Returned 37.0.2 content and its public offer parameters.
use super::*;

const BAIT: &str = "BG30_MagicItem_973";

fn bait_type(c: &mut Ctx, side: Side, card: CardId) -> Races {
    super::trinkets_p2::compass_type(c, side, card)
}

/// Fix the type before the player chooses; viewing a menu consumes no randomness.
pub(crate) fn prepare_options(c: &mut Ctx, side: Side, options: &[CardId]) {
    for &card in options {
        if card.base() == crate::cardid!(BAIT) {
            // A new unowned offer is parameterized from the current public board/hand.
            // Keeping an earlier declined offer's parameter would make this menu stale.
            if c.player_ref(side)
                .is_some_and(|p| !p.has_trinket_id(card.base()))
            {
                c.set_card_data(side, card, 0);
            }
            bait_type(c, side, card);
        }
    }
}

pub fn scripts() -> Vec<(&'static str, Script)> {
    vec![(
        BAIT,
        Script {
            on_attach: Some(|c, me| {
                bait_type(c, me.side, me.card);
            }),
            on_fill_shop: Some(|c, me| {
                let race = bait_type(c, me.side, me.card);
                let tier = c.tier(me.side);
                if !race.is_none() {
                    for _ in 0..2 {
                        c.add_shop_minion(me.side, 1, tier, race);
                    }
                }
            }),
            ..E
        },
    )]
}
