#[cfg(test)]
use crate::dsl::{CardDefinition, CardId, CardType};
#[cfg(test)]
use crate::rules::Side;

/// A `CardDefinition` with every optional field at its neutral default,
/// requiring only the fields a card can't be meaningful without — the
/// standard fixture constructor for tests that need a card whose exact
/// contents don't matter.
///
/// Real cards are no longer built this way: they're authored as JSON under
/// `data/{corp,runner}` and embedded at compile time (see `cards::embedded`),
/// so this is test-only. `is_playable` is `true` here (unlike
/// `CardDefinition::default()`, which mirrors serde's `false`) because a
/// fixture card exists to be played.
#[cfg(test)]
pub(crate) fn base_card(id: &str, title: &str, side: Side, card_type: CardType, cost: u32) -> CardDefinition {
    CardDefinition {
        id: CardId(id.to_string()),
        title: title.to_string(),
        side,
        card_type,
        cost,
        is_playable: true,
        ..CardDefinition::default()
    }
}
