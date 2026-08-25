use crate::dsl::{Card, CardId, CardType};
use crate::rules::Side;

/// A `Card` with every optional field at its neutral default — the common
/// starting point for every card definition in this module, so each card
/// only has to spell out the fields its own text actually needs (mirrors
/// `dsl::card::tests::card_with_triggers`'s existing "minimal fixture, fill
/// in what matters" convention, generalized to every field).
pub(crate) fn base_card(id: &str, title: &str, side: Side, card_type: CardType, cost: u32) -> Card {
    Card {
        id: CardId(id.to_string()),
        title: title.to_string(),
        side,
        card_type,
        cost,
        triggers: Vec::new(),
        abilities: Vec::new(),
        trash_cost: None,
        steal_cost: None,
        advancement_requirement: None,
        agenda_points: None,
        min_deck_size: None,
        strength: None,
        subroutines: Vec::new(),
        interactive_on_access: None,
        subtypes: Vec::new(),
        play_requirement: None,
        recurring_credits: None,
        first_install_discount: None,
    }
}
