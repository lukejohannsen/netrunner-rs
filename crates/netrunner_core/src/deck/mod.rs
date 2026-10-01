//! Deckbuilding-time decklists, validated against the same unified
//! `cards::CardRegistry`/`dsl::CardDefinition` model the rules engine plays
//! from, checking deckbuilding-legality concerns (`influence_cost`,
//! `faction`, the format's pool and lists) against NSG's competitive formats
//! (`format::NsgFormat`).
//!
//! Distinct from `rules::deck::Deck`/`rules::deck::validate_deck`, which
//! validate a deck against the same `CardRegistry` and checking gameplay-
//! executability only — structural rules plus `CardDefinition::is_playable`,
//! no influence/faction/pack legality. A NetrunnerDB-sourced `Decklist` may
//! legitimately reference cards this engine has no DSL data for yet
//! (`is_playable: false`); this validator doesn't reject those, since that's
//! not the question it answers — the two validators serve different
//! questions and neither supersedes the other.
//!
//! # The pipeline
//!
//! Both validators run on every deck, and `decks::DeckFile::validate` is the
//! single seam that runs them, so no caller has to know which to invoke:
//!
//! ```text
//! decks::DeckFile  (authored: slug-keyed, carries metadata)
//!   ├─ to_deck()     → rules::Deck     → rules::deck::validate_deck
//!   │                                    "can this engine play it?"
//!   │                                    implemented cards, copy limit,
//!   │                                    deck size, agenda points
//!   └─ to_decklist() → deck::Decklist  → deck::validator::validate_deck
//!                                        "is it legal to build?"
//!                                        influence, format pool, banlist,
//!                                        per-card deck_limit
//! ```
//!
//! Gameplay executability is checked first: "references a card the engine
//! cannot play" is a more fundamental complaint than "two influence over",
//! and reporting it second would bury it.
//!
//! `Decklist` is reached by *converting* a `DeckFile`, never by parsing
//! user-supplied NetrunnerDB JSON — deckbuilding happens against the cards
//! this engine implements. Both are keyed by the card's id, NetrunnerDB's
//! v3 slug: the decklist used to be keyed by printing code, because printed
//! legality was defined over NetrunnerDB's v2 metadata and a card file's
//! one printing (`numeric_id`) was the join. A format's pool and lists are
//! cards now (`format::FormatRules`), so the decklist is the deck's counts
//! summed per card and nothing else.
//!
//! The one rule the two validators share is `rules::deck::agenda_point_range`,
//! called from both. `MAX_COPIES_PER_CARD` is deliberately duplicated rather
//! than shared — see `validator::MAX_COPIES_PER_CARD`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::dsl::CardId;

pub mod validator;

pub use validator::{influence_per_copy, tally_deck, validate_deck, validate_deck_with_rules, AgendaTally, DeckTally, DeckValidationError, ValidationReport};

/// A deckbuilding-time decklist: an identity plus a card pool, each entry
/// paired with how many copies are included, every card by id —
/// `{"identity": "zahya_sadeghi_versatile_smuggler", "cards": {"sure_gamble": 3}}`,
/// the shape of NetrunnerDB v3's `card_slots`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decklist {
    pub identity: CardId,
    pub cards: HashMap<CardId, u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_card_ids_as_netrunnerdb_v3_writes_them() {
        let json = r#"{"identity": "rene_loup_arcemont_party_animal", "cards": {"wildcat_strike": 3, "sure_gamble": 2}}"#;
        let deck: Decklist = serde_json::from_str(json).expect("valid decklist JSON");

        assert_eq!(deck.identity, CardId("rene_loup_arcemont_party_animal".to_string()));
        assert_eq!(deck.cards.get(&CardId("wildcat_strike".to_string())), Some(&3));
        assert_eq!(deck.cards.get(&CardId("sure_gamble".to_string())), Some(&2));
    }

    #[test]
    fn round_trips_through_serialization() {
        let mut cards = HashMap::new();
        cards.insert(CardId("wildcat_strike".to_string()), 3);
        let deck = Decklist { identity: CardId("rene_loup_arcemont_party_animal".to_string()), cards };

        let json = serde_json::to_string(&deck).expect("serializes");
        let round_tripped: Decklist = serde_json::from_str(&json).expect("deserializes back");
        assert_eq!(round_tripped, deck);
    }
}
