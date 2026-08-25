use serde::{Deserialize, Serialize};

use crate::cards::CardRegistry;
use crate::dsl::{CardId, CardType};
use crate::rules::error::RulesError;
use crate::rules::state::Side;

/// Flat copy-limit applied to every non-identity card in a deck. Real
/// Netrunner allows some identities/cards to override this — no field
/// exists anywhere on `dsl::Card` to source such an override from, and
/// inventing one would be a speculative schema guess with zero precedent
/// in this data-driven card model, so only the flat limit is enforced.
pub const MAX_COPIES_PER_CARD: u32 = 3;

/// A deckbuilding-time deck list: an identity plus a card pool, each entry
/// paired with how many copies are included. Validated by [`validate_deck`]
/// before `GameState::setup` will accept it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deck {
    pub identity: CardId,
    pub cards: Vec<(CardId, u32)>,
}

/// Validates `deck` against `registry` for `side`, checking in order:
/// the identity exists in the registry, is a `CardType::Identity`, and
/// matches `side`; the total non-identity card count meets the identity's
/// `min_deck_size`; every card exists in the registry, respects
/// [`MAX_COPIES_PER_CARD`], matches `side`, and (Runner decks only) isn't
/// an Agenda; and (Corp decks only) the deck's total agenda points fall
/// within the size-derived legal range (see [`agenda_point_range`]).
pub fn validate_deck(deck: &Deck, side: Side, registry: &CardRegistry) -> Result<(), RulesError> {
    let identity = registry
        .get(&deck.identity)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(deck.identity.clone()))?;

    if identity.card_type != CardType::Identity {
        return Err(RulesError::CardNotIdentity { card: deck.identity.clone() });
    }
    if identity.side != side {
        return Err(RulesError::IdentitySideMismatch {
            card: deck.identity.clone(),
            expected: side,
            actual: identity.side,
        });
    }
    let min_deck_size = identity
        .min_deck_size
        .ok_or_else(|| RulesError::IdentityMissingMinDeckSize { card: deck.identity.clone() })?;

    let total_count: u32 = deck.cards.iter().map(|(_, count)| *count).sum();
    if total_count < min_deck_size {
        return Err(RulesError::DeckBelowMinimumSize { size: total_count, minimum: min_deck_size });
    }

    let mut agenda_points_total: u32 = 0;
    for (card_id, count) in &deck.cards {
        let card = registry
            .get(card_id)
            .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;

        if *count > MAX_COPIES_PER_CARD {
            return Err(RulesError::TooManyCopies { card: card_id.clone(), count: *count, max: MAX_COPIES_PER_CARD });
        }
        if card.side != side {
            return Err(RulesError::DeckCardWrongSide {
                card: card_id.clone(),
                expected: side,
                actual: card.side,
            });
        }
        if side == Side::Runner && card.card_type == CardType::Agenda {
            return Err(RulesError::RunnerDeckContainsAgenda { card: card_id.clone() });
        }
        if card.card_type == CardType::Agenda {
            agenda_points_total += card.agenda_points.unwrap_or(0) * count;
        }
    }

    if side == Side::Corp {
        let (min_points, max_points) = agenda_point_range(total_count);
        if agenda_points_total < min_points || agenda_points_total > max_points {
            return Err(RulesError::AgendaPointsOutOfRange {
                points: agenda_points_total,
                min: min_points,
                max: max_points,
            });
        }
    }

    Ok(())
}

/// Returns `(min, max)` legal agenda points for a Corp deck of `size`
/// non-identity cards. The real-game minimum formula is `20 + 2 *
/// floor((size - 45) / 5)` for `size >= 45`, flattening to `18` below that
/// (matching a handful of real identities with a reduced minimum deck
/// size). The `+2` ceiling matches historical Netrunner/Null Signal Games
/// competition-legality rules — an inferred completion of the brief's
/// minimum-only formula, not a free invention.
fn agenda_point_range(size: u32) -> (u32, u32) {
    let min = if size >= 45 { 20 + 2 * ((size - 45) / 5) } else { 18 };
    (min, min + 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::Card;

    fn identity(id: &str, side: Side, min_deck_size: u32) -> Card {
        Card {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type: CardType::Identity,
            cost: 0,
            triggers: Vec::new(),
            abilities: Vec::new(),
            trash_cost: None,
            steal_cost: None,
            advancement_requirement: None,
            agenda_points: None,
            min_deck_size: Some(min_deck_size),
            strength: None,
            subroutines: Vec::new(),
            interactive_on_access: None, subtypes: Vec::new(), play_requirement: None, recurring_credits: None, first_install_discount: None,
        }
    }

    fn card(id: &str, side: Side, card_type: CardType) -> Card {
        Card {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type,
            cost: 0,
            triggers: Vec::new(),
            abilities: Vec::new(),
            trash_cost: None,
            steal_cost: None,
            advancement_requirement: None,
            agenda_points: None,
            min_deck_size: None,
            strength: None,
            subroutines: Vec::new(),
            interactive_on_access: None, subtypes: Vec::new(), play_requirement: None, recurring_credits: None, first_install_discount: None,
        }
    }

    fn agenda(id: &str, points: u32) -> Card {
        let mut c = card(id, Side::Corp, CardType::Agenda);
        c.agenda_points = Some(points);
        c
    }

    /// Registers `total` non-agenda filler cards for `side`, split across
    /// as many distinct `CardId`s as needed to respect
    /// `MAX_COPIES_PER_CARD`, and returns the matching `Deck.cards` entries.
    fn filler_stack(registry: &mut CardRegistry, side: Side, prefix: &str, total: u32) -> Vec<(CardId, u32)> {
        let card_type = if side == Side::Corp { CardType::Asset } else { CardType::Event };
        let mut entries = Vec::new();
        let mut remaining = total;
        let mut i = 0;
        while remaining > 0 {
            let copies = remaining.min(MAX_COPIES_PER_CARD);
            let id = format!("{prefix}_{i}");
            registry.insert(card(&id, side, card_type.clone()));
            entries.push((CardId(id), copies));
            remaining -= copies;
            i += 1;
        }
        entries
    }

    /// Registers `num_cards` distinct 1-copy Agenda cards, each worth
    /// `points_per_card`, and returns the matching `Deck.cards` entries.
    fn agenda_stack(registry: &mut CardRegistry, prefix: &str, points_per_card: u32, num_cards: u32) -> Vec<(CardId, u32)> {
        (0..num_cards)
            .map(|i| {
                let id = format!("{prefix}_{i}");
                registry.insert(agenda(&id, points_per_card));
                (CardId(id), 1)
            })
            .collect()
    }

    /// A minimal, legal 45-card Corp deck: 4 distinct 5-point Agendas (20
    /// agenda points total, within the 45-card range of 20-22) plus 41
    /// filler cards, all respecting the 3-copy limit.
    fn valid_corp_registry_and_deck() -> (CardRegistry, Deck) {
        let mut registry = CardRegistry::new();
        registry.insert(identity("corp_id", Side::Corp, 45));

        let mut cards = agenda_stack(&mut registry, "corp_agenda", 5, 4);
        cards.extend(filler_stack(&mut registry, Side::Corp, "corp_filler", 41));

        let deck = Deck { identity: CardId("corp_id".to_string()), cards };
        (registry, deck)
    }

    fn valid_runner_registry_and_deck() -> (CardRegistry, Deck) {
        let mut registry = CardRegistry::new();
        registry.insert(identity("runner_id", Side::Runner, 45));

        let cards = filler_stack(&mut registry, Side::Runner, "runner_filler", 45);

        let deck = Deck { identity: CardId("runner_id".to_string()), cards };
        (registry, deck)
    }

    #[test]
    fn validate_deck_succeeds_for_well_formed_corp_deck() {
        let (registry, deck) = valid_corp_registry_and_deck();
        assert_eq!(validate_deck(&deck, Side::Corp, &registry), Ok(()));
    }

    #[test]
    fn validate_deck_succeeds_for_well_formed_runner_deck() {
        let (registry, deck) = valid_runner_registry_and_deck();
        assert_eq!(validate_deck(&deck, Side::Runner, &registry), Ok(()));
    }

    #[test]
    fn validate_deck_fails_when_identity_not_in_registry() {
        let registry = CardRegistry::new();
        let deck = Deck { identity: CardId("missing".to_string()), cards: Vec::new() };

        assert_eq!(
            validate_deck(&deck, Side::Corp, &registry),
            Err(RulesError::CardNotFoundInRegistry(CardId("missing".to_string())))
        );
    }

    #[test]
    fn validate_deck_fails_when_identity_card_type_is_not_identity() {
        let mut registry = CardRegistry::new();
        registry.insert(card("not_an_id", Side::Corp, CardType::Asset));
        let deck = Deck { identity: CardId("not_an_id".to_string()), cards: Vec::new() };

        assert_eq!(
            validate_deck(&deck, Side::Corp, &registry),
            Err(RulesError::CardNotIdentity { card: CardId("not_an_id".to_string()) })
        );
    }

    #[test]
    fn validate_deck_fails_when_identity_side_mismatches_requested_side() {
        let (registry, deck) = valid_corp_registry_and_deck();

        assert_eq!(
            validate_deck(&deck, Side::Runner, &registry),
            Err(RulesError::IdentitySideMismatch {
                card: CardId("corp_id".to_string()),
                expected: Side::Runner,
                actual: Side::Corp,
            })
        );
    }

    #[test]
    fn validate_deck_fails_when_below_minimum_deck_size() {
        let mut registry = CardRegistry::new();
        registry.insert(identity("corp_id", Side::Corp, 45));
        let cards = filler_stack(&mut registry, Side::Corp, "corp_filler", 10);
        let deck = Deck { identity: CardId("corp_id".to_string()), cards };

        assert_eq!(
            validate_deck(&deck, Side::Corp, &registry),
            Err(RulesError::DeckBelowMinimumSize { size: 10, minimum: 45 })
        );
    }

    #[test]
    fn validate_deck_fails_when_a_card_exceeds_copy_limit() {
        let mut registry = CardRegistry::new();
        registry.insert(identity("corp_id", Side::Corp, 45));
        registry.insert(card("corp_filler_0", Side::Corp, CardType::Asset));
        let deck = Deck {
            identity: CardId("corp_id".to_string()),
            cards: vec![(CardId("corp_filler_0".to_string()), 45)],
        };

        assert_eq!(
            validate_deck(&deck, Side::Corp, &registry),
            Err(RulesError::TooManyCopies { card: CardId("corp_filler_0".to_string()), count: 45, max: 3 })
        );
    }

    #[test]
    fn validate_deck_fails_when_a_card_side_mismatches_deck_side() {
        let (mut registry, mut deck) = valid_corp_registry_and_deck();
        registry.insert(card("runner_card", Side::Runner, CardType::Program));
        deck.cards.push((CardId("runner_card".to_string()), 3));

        assert_eq!(
            validate_deck(&deck, Side::Corp, &registry),
            Err(RulesError::DeckCardWrongSide {
                card: CardId("runner_card".to_string()),
                expected: Side::Corp,
                actual: Side::Runner,
            })
        );
    }

    #[test]
    fn validate_deck_fails_when_runner_deck_contains_an_agenda() {
        let (mut registry, mut deck) = valid_runner_registry_and_deck();
        // A mislabeled card: Agenda type but tagged Runner-side, to isolate
        // this check from the more general side-mismatch check.
        let mut mislabeled = agenda("mislabeled_agenda", 1);
        mislabeled.side = Side::Runner;
        registry.insert(mislabeled);
        deck.cards.push((CardId("mislabeled_agenda".to_string()), 1));

        assert_eq!(
            validate_deck(&deck, Side::Runner, &registry),
            Err(RulesError::RunnerDeckContainsAgenda { card: CardId("mislabeled_agenda".to_string()) })
        );
    }

    #[test]
    fn validate_deck_fails_when_corp_agenda_points_below_range() {
        let mut registry = CardRegistry::new();
        registry.insert(identity("corp_id", Side::Corp, 45));
        let mut cards = agenda_stack(&mut registry, "corp_agenda", 5, 3); // 15 points, below [20,22]
        cards.extend(filler_stack(&mut registry, Side::Corp, "corp_filler", 42));
        let deck = Deck { identity: CardId("corp_id".to_string()), cards };

        assert_eq!(
            validate_deck(&deck, Side::Corp, &registry),
            Err(RulesError::AgendaPointsOutOfRange { points: 15, min: 20, max: 22 })
        );
    }

    #[test]
    fn validate_deck_fails_when_corp_agenda_points_above_range() {
        let mut registry = CardRegistry::new();
        registry.insert(identity("corp_id", Side::Corp, 45));
        let mut cards = agenda_stack(&mut registry, "corp_agenda", 5, 5); // 25 points, above [20,22]
        cards.extend(filler_stack(&mut registry, Side::Corp, "corp_filler", 40));
        let deck = Deck { identity: CardId("corp_id".to_string()), cards };

        assert_eq!(
            validate_deck(&deck, Side::Corp, &registry),
            Err(RulesError::AgendaPointsOutOfRange { points: 25, min: 20, max: 22 })
        );
    }

    #[test]
    fn agenda_point_range_matches_size_derived_examples() {
        assert_eq!(agenda_point_range(40), (18, 20));
        assert_eq!(agenda_point_range(44), (18, 20));
        assert_eq!(agenda_point_range(45), (20, 22));
        assert_eq!(agenda_point_range(50), (22, 24));
    }
}
