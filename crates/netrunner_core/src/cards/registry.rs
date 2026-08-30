use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::card::CardId as NumericCardId;
use crate::dsl::{CardDefinition, CardId};

/// A pure, in-memory index of parsed `dsl::CardDefinition` definitions,
/// keyed primarily by the engine-native slug `CardId`, with a secondary
/// index by NetrunnerDB's numeric `card::CardId` for cards that carry one
/// (`CardDefinition::numeric_id`). Per AGENTS.md's I/O-free rule for
/// `netrunner_core`, this has no filesystem constructor — walking a
/// directory of card JSON files, or fetching live NetrunnerDB data, is a
/// caller's job (`cards::loader`, `netrunner_card_sync`); `from_json` and
/// `from_cards` only ever take already-in-memory data.
///
/// Derives `Serialize`/`Deserialize` (via `CardRegistryWire`, so a
/// deserialized registry always rebuilds `by_numeric_id` through `insert`
/// rather than trusting a stale/absent index in the wire data) so a whole
/// loaded card pool can be snapshotted/transmitted or cached to disk in one
/// shot.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CardRegistry {
    cards: HashMap<CardId, CardDefinition>,
    #[serde(skip)]
    by_numeric_id: HashMap<NumericCardId, CardId>,
}

#[derive(Deserialize)]
struct CardRegistryWire {
    cards: HashMap<CardId, CardDefinition>,
}

impl From<CardRegistryWire> for CardRegistry {
    fn from(wire: CardRegistryWire) -> Self {
        Self::from_cards(wire.cards.into_values().collect())
    }
}

impl<'de> Deserialize<'de> for CardRegistry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        CardRegistryWire::deserialize(deserializer).map(Self::from)
    }
}

impl CardRegistry {
    pub fn new() -> Self {
        Self { cards: HashMap::new(), by_numeric_id: HashMap::new() }
    }

    /// Builds a registry from already-in-memory `CardDefinition`s. A
    /// duplicate `CardId` silently overwrites the earlier entry
    /// (`HashMap::insert`'s own semantics).
    pub fn from_cards(cards: Vec<CardDefinition>) -> Self {
        let mut registry = Self::new();
        for card in cards {
            registry.insert(card);
        }
        registry
    }

    pub fn insert(&mut self, card: CardDefinition) {
        if let Some(numeric_id) = card.numeric_id {
            self.by_numeric_id.insert(numeric_id, card.id.clone());
        }
        self.cards.insert(card.id.clone(), card);
    }

    /// Inserts every definition from `cards`, later entries overwriting
    /// earlier ones on `CardId` collision.
    pub fn merge(&mut self, cards: impl IntoIterator<Item = CardDefinition>) {
        for card in cards {
            self.insert(card);
        }
    }

    pub fn get(&self, id: &CardId) -> Option<&CardDefinition> {
        self.cards.get(id)
    }

    pub fn get_by_numeric_id(&self, id: NumericCardId) -> Option<&CardDefinition> {
        self.by_numeric_id.get(&id).and_then(|slug| self.cards.get(slug))
    }

    /// Linear scan by title — only exercised in tests/tooling, so no title
    /// index is maintained for it.
    pub fn get_by_title(&self, title: &str) -> Option<&CardDefinition> {
        self.cards.values().find(|card| card.title == title)
    }

    /// Every registered card, in unspecified order. Used by a determinizer
    /// (`netrunner_bots::determinize`) to enumerate "every card that could
    /// plausibly be in a hidden zone for side X" — there's no decklist
    /// concept anywhere in this engine, so the full registry pool for a
    /// side is the only candidate set available.
    pub fn iter(&self) -> impl Iterator<Item = &CardDefinition> {
        self.cards.values()
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// Parses one card's JSON text and inserts it — pure, no I/O
    /// performed by this function itself.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let card: CardDefinition = serde_json::from_str(json)?;
        Ok(Self::from_cards(vec![card]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEDGE_FUND_JSON: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/corp/hedge_fund.json"));

    #[test]
    fn from_json_parses_and_inserts_a_single_card() {
        let registry = CardRegistry::from_json(HEDGE_FUND_JSON).expect("valid card JSON");

        let card = registry.get(&CardId("hedge_fund".to_string())).expect("card present");
        assert_eq!(card.title, "Hedge Fund");
    }

    #[test]
    fn get_returns_none_for_an_unknown_card() {
        let registry = CardRegistry::new();
        assert!(registry.get(&CardId("nonexistent".to_string())).is_none());
    }

    #[test]
    fn from_cards_indexes_every_card_by_id() {
        let registry = CardRegistry::from_json(HEDGE_FUND_JSON).expect("valid card JSON");
        let card = registry.get(&CardId("hedge_fund".to_string())).unwrap().clone();

        let registry = CardRegistry::from_cards(vec![card]);
        assert!(registry.get(&CardId("hedge_fund".to_string())).is_some());
    }

    #[test]
    fn card_registry_round_trips_through_json() {
        let registry = CardRegistry::from_json(HEDGE_FUND_JSON).expect("valid card JSON");

        let json = serde_json::to_string(&registry).expect("registry should serialize");
        let restored: CardRegistry = serde_json::from_str(&json).expect("registry should deserialize");

        let card = restored
            .get(&CardId("hedge_fund".to_string()))
            .expect("card present after round-trip");
        assert_eq!(card.title, "Hedge Fund");
    }

    #[test]
    fn iter_yields_every_inserted_card() {
        let registry = CardRegistry::from_cards(vec![blank_card("a"), blank_card("b")]);

        let mut ids: Vec<String> = registry.iter().map(|card| card.id.0.clone()).collect();
        ids.sort();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn get_by_numeric_id_finds_a_card_with_a_matching_numeric_id() {
        let mut card = blank_card("hedge_fund_like");
        card.numeric_id = Some(NumericCardId(1234));
        let registry = CardRegistry::from_cards(vec![card]);

        assert_eq!(registry.get_by_numeric_id(NumericCardId(1234)).unwrap().id, CardId("hedge_fund_like".to_string()));
        assert!(registry.get_by_numeric_id(NumericCardId(9999)).is_none());
    }

    #[test]
    fn numeric_id_index_survives_a_json_round_trip() {
        let mut card = blank_card("hedge_fund_like");
        card.numeric_id = Some(NumericCardId(1234));
        let registry = CardRegistry::from_cards(vec![card]);

        let json = serde_json::to_string(&registry).expect("registry should serialize");
        let restored: CardRegistry = serde_json::from_str(&json).expect("registry should deserialize");

        assert_eq!(restored.get_by_numeric_id(NumericCardId(1234)).unwrap().id, CardId("hedge_fund_like".to_string()));
    }

    #[test]
    fn get_by_title_hits_and_misses() {
        let registry = CardRegistry::from_cards(vec![blank_card("hedge_fund_like")]);

        assert_eq!(registry.get_by_title("hedge_fund_like").unwrap().id, CardId("hedge_fund_like".to_string()));
        assert!(registry.get_by_title("Not A Real Card").is_none());
    }

    fn blank_card(id: &str) -> CardDefinition {
        use crate::dsl::CardType;
        use crate::rules::Side;

        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Operation,
            is_playable: true,
            ..Default::default()
        }
    }
}
