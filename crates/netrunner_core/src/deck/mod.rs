//! Deckbuilding-time decklists, validated against the catalog/format layer
//! (`catalog::CardCatalog`, `format::NsgFormat`) rather than the rules
//! engine's playable `dsl::Card` model.
//!
//! Distinct from `rules::deck::Deck`/`rules::deck::validate_deck`, which
//! validate a deck against `cards::CardRegistry` (the engine's gameplay AST,
//! keyed by `dsl::CardId` string slugs) purely for whether `GameState::
//! setup` can play it — structural rules only, no influence/faction/pack
//! legality, since `dsl::Card` carries none of that data. This module
//! validates the deckbuilding concerns `CardDefinition` *does* carry
//! (`influence_cost`, `pack_code`, `faction`) against NSG's competitive
//! formats, keyed by `card::CardId` (NetrunnerDB's numeric codes) — the two
//! validators serve different questions and neither supersedes the other.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::card::CardId;

pub mod validator;

pub use validator::{validate_deck, DeckValidationError, ValidationReport};

/// A deckbuilding-time decklist: an identity plus a card pool, each entry
/// paired with how many copies are included. Deserializes from the common
/// NetrunnerDB/community-tool JSON deck-export shape:
/// `{"identity": "30001", "cards": {"30002": 3, "30015": 2}}` — `cards`'
/// keys are always JSON strings (the JSON spec has no non-string object
/// key), which `CardId`'s own `Deserialize` impl already parses correctly
/// as a map key (serde_json routes primitive-typed map keys through the
/// same string-parsing path regardless of the surrounding value's JSON
/// syntax). `identity` additionally accepts a bare JSON integer, since some
/// export tools emit unquoted card codes for scalar fields even though
/// `cards`' keys are necessarily quoted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Decklist {
    pub identity: CardId,
    pub cards: HashMap<CardId, u32>,
}

impl<'de> Deserialize<'de> for Decklist {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            identity: FlexibleCardId,
            cards: HashMap<CardId, u32>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(Decklist { identity: raw.identity.0, cards: raw.cards })
    }
}

/// A `CardId` that deserializes from either a JSON string (`"30001"`, the
/// common case — NetrunnerDB codes are conventionally zero-padded, and
/// `u32`'s own string parsing tolerates the leading zeros) or a bare JSON
/// integer (`30001`).
struct FlexibleCardId(CardId);

impl<'de> Deserialize<'de> for FlexibleCardId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = FlexibleCardId;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a NetrunnerDB card code, as a numeric string or a bare integer")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                v.parse::<u32>()
                    .map(|code| FlexibleCardId(CardId(code)))
                    .map_err(|_| E::invalid_value(serde::de::Unexpected::Str(v), &self))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                u32::try_from(v)
                    .map(|code| FlexibleCardId(CardId(code)))
                    .map_err(|_| E::invalid_value(serde::de::Unexpected::Unsigned(v), &self))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_the_standard_netrunnerdb_export_shape() {
        let json = r#"{"identity": "30001", "cards": {"30002": 3, "30015": 2}}"#;
        let deck: Decklist = serde_json::from_str(json).expect("valid decklist JSON");

        assert_eq!(deck.identity, CardId(30001));
        assert_eq!(deck.cards.get(&CardId(30002)), Some(&3));
        assert_eq!(deck.cards.get(&CardId(30015)), Some(&2));
    }

    #[test]
    fn deserializes_an_integer_identity_code() {
        let json = r#"{"identity": 30001, "cards": {}}"#;
        let deck: Decklist = serde_json::from_str(json).expect("valid decklist JSON");

        assert_eq!(deck.identity, CardId(30001));
    }

    #[test]
    fn tolerates_zero_padded_identity_codes() {
        let json = r#"{"identity": "01001", "cards": {}}"#;
        let deck: Decklist = serde_json::from_str(json).expect("valid decklist JSON");

        assert_eq!(deck.identity, CardId(1001));
    }

    #[test]
    fn rejects_a_non_numeric_identity_code() {
        let json = r#"{"identity": "not-a-code", "cards": {}}"#;
        assert!(serde_json::from_str::<Decklist>(json).is_err());
    }

    #[test]
    fn round_trips_through_serialization() {
        let mut cards = HashMap::new();
        cards.insert(CardId(30002), 3);
        let deck = Decklist { identity: CardId(30001), cards };

        let json = serde_json::to_string(&deck).expect("serializes");
        let round_tripped: Decklist = serde_json::from_str(&json).expect("deserializes back");
        assert_eq!(round_tripped, deck);
    }
}
