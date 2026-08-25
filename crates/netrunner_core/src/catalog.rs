//! Embedded default card sets and the in-memory `CardCatalog` index.
//!
//! The embedded fixtures live at `data/cards/*.json`, crate-local to
//! `netrunner_core` — distinct from the repo-root `data/{corp,runner}/*.json`
//! `dsl::Card` fixtures shared across crates via `../../data/...`. Each file
//! is a bare JSON array of `NetrunnerDbCardDto` objects, the same shape
//! NetrunnerDB's `/api/2.0/public/cards` endpoint returns, so embedded
//! fixtures and live/cached API data parse through one shared path
//! (`convert_dtos`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::card::{CardConversionError, CardDefinition, CardId, CardType, NetrunnerDbCardDto};

const SYSTEM_GATEWAY_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/cards/system_gateway.json"));
const ELEVATION_JSON: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/data/cards/elevation.json"));

#[derive(Debug, Error)]
pub enum CardCatalogError {
    #[error("failed to parse embedded card catalog JSON: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("failed to convert card at index {index}: {source}")]
    Conversion { index: usize, source: CardConversionError },
}

/// Converts each DTO via `CardDefinition::try_from`, tagging a failure with
/// its position in `dtos` for a useful error message. Shared by
/// `load_default_core_sets` (parsing the embedded fixtures) and
/// `netrunner_card_sync` (parsing a live/cached NetrunnerDB payload) so
/// there is exactly one DTO-to-catalog conversion path in the workspace.
pub fn convert_dtos(dtos: Vec<NetrunnerDbCardDto>) -> Result<Vec<CardDefinition>, CardCatalogError> {
    dtos.into_iter()
        .enumerate()
        .map(|(index, dto)| {
            CardDefinition::try_from(dto).map_err(|source| CardCatalogError::Conversion { index, source })
        })
        .collect()
}

/// Same conversion as `convert_dtos`, but best-effort: a card this schema
/// doesn't model (e.g. a mini-faction like Apex/Adam/Sunny-Lebeau, absent
/// from the closed `Faction` enum) is skipped and reported rather than
/// aborting the whole batch. Intended for ingesting NetrunnerDB's full,
/// ever-growing live card list (`netrunner_card_sync`), where the fetched
/// data naturally exceeds this catalog's currently-modeled scope — unlike
/// `convert_dtos`, used for the curated embedded fixtures, where any
/// conversion failure is a real bug that should fail loudly instead.
pub fn convert_dtos_lenient(dtos: Vec<NetrunnerDbCardDto>) -> (Vec<CardDefinition>, Vec<(usize, CardConversionError)>) {
    let mut definitions = Vec::new();
    let mut skipped = Vec::new();
    for (index, dto) in dtos.into_iter().enumerate() {
        match CardDefinition::try_from(dto) {
            Ok(definition) => definitions.push(definition),
            Err(source) => skipped.push((index, source)),
        }
    }
    (definitions, skipped)
}

/// A pure, in-memory index of `CardDefinition`s, keyed by `CardId` (and
/// title). Distinct from `cards::CardRegistry` (which indexes `dsl::Card`
/// for the rules engine) — this is the descriptive/deckbuilding catalog.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CardCatalog {
    by_id: HashMap<CardId, CardDefinition>,
    by_title: HashMap<String, CardId>,
}

impl CardCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses both embedded core-set JSON fixtures (System Gateway,
    /// Elevation) through `convert_dtos` and inserts every successfully
    /// converted card.
    pub fn load_default_core_sets() -> Result<Self, CardCatalogError> {
        let mut catalog = Self::new();
        for json in [SYSTEM_GATEWAY_JSON, ELEVATION_JSON] {
            let dtos: Vec<NetrunnerDbCardDto> = serde_json::from_str(json)?;
            catalog.merge(convert_dtos(dtos)?);
        }
        Ok(catalog)
    }

    pub fn get_by_id(&self, id: CardId) -> Option<&CardDefinition> {
        self.by_id.get(&id)
    }

    pub fn get_by_title(&self, title: &str) -> Option<&CardDefinition> {
        self.by_title.get(title).and_then(|id| self.by_id.get(id))
    }

    /// Every card of the given type, in unspecified order — mirrors
    /// `CardRegistry::iter()`'s "return an iterator, caller collects if it
    /// needs a Vec" convention.
    pub fn filter_by_type(&self, card_type: CardType) -> impl Iterator<Item = &CardDefinition> {
        self.by_id.values().filter(move |def| def.card_type == card_type)
    }

    /// Inserts (or overwrites-by-id) one already-converted definition —
    /// pure, no I/O.
    pub fn insert_definition(&mut self, definition: CardDefinition) {
        self.by_title.insert(definition.title.clone(), definition.id);
        self.by_id.insert(definition.id, definition);
    }

    /// Inserts every definition from `defs`, later entries overwriting
    /// earlier ones on `CardId` collision (`HashMap::insert`'s own
    /// last-write-wins semantics).
    pub fn merge(&mut self, defs: impl IntoIterator<Item = CardDefinition>) {
        for def in defs {
            self.insert_definition(def);
        }
    }

    /// Consumes this catalog, yielding its definitions — used by
    /// `netrunner_card_sync` to layer one catalog's contents onto another.
    pub fn into_definitions(self) -> impl Iterator<Item = CardDefinition> {
        self.by_id.into_values()
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::Side;

    #[test]
    fn load_default_core_sets_is_non_empty() {
        let catalog = CardCatalog::load_default_core_sets().expect("embedded sets should parse");
        assert!(!catalog.is_empty());
        assert!(catalog.len() >= 159, "expected at least the 77 + 82 known System Gateway/Elevation cards");
    }

    #[test]
    fn get_by_id_and_title_hit_and_miss() {
        let catalog = CardCatalog::load_default_core_sets().expect("embedded sets should parse");

        let by_title = catalog.get_by_title("Wildcat Strike").expect("known card");
        assert_eq!(by_title.id, CardId(30002));
        assert_eq!(catalog.get_by_id(CardId(30002)).unwrap().title, "Wildcat Strike");

        assert!(catalog.get_by_id(CardId(999_999)).is_none());
        assert!(catalog.get_by_title("Not A Real Card").is_none());
    }

    #[test]
    fn convert_dtos_lenient_skips_unconvertible_cards_but_keeps_the_rest() {
        use crate::card::NetrunnerDbCardDto;

        let good = NetrunnerDbCardDto {
            code: "1".to_string(),
            title: "Good Card".to_string(),
            type_code: "event".to_string(),
            side_code: "runner".to_string(),
            faction_code: "anarch".to_string(),
            pack_code: "test".to_string(),
            text: None,
            keywords: None,
            cost: None,
            strength: None,
            advancement_cost: None,
            agenda_points: None,
            trash_cost: None,
            faction_cost: None,
            memory_cost: None,
            minimum_deck_size: None,
            base_link: None,
            uniqueness: None,
        };
        let mut unmodeled_faction = good.clone();
        unmodeled_faction.code = "2".to_string();
        unmodeled_faction.faction_code = "apex".to_string();

        let (defs, skipped) = convert_dtos_lenient(vec![good, unmodeled_faction]);

        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].title, "Good Card");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].0, 1);
    }

    #[test]
    fn filter_by_type_returns_only_matching_cards() {
        let catalog = CardCatalog::load_default_core_sets().expect("embedded sets should parse");

        let identities: Vec<&CardDefinition> = catalog.filter_by_type(CardType::Identity).collect();
        assert!(!identities.is_empty());
        assert!(identities.iter().all(|def| def.card_type == CardType::Identity));
    }

    fn blank_definition(id: u32, title: &str) -> CardDefinition {
        CardDefinition {
            id: CardId(id),
            title: title.to_string(),
            card_type: CardType::Event,
            faction: crate::card::Faction::NeutralRunner,
            side: Side::Runner,
            pack_code: "test".to_string(),
            cost: None,
            strength: None,
            advancement_requirement: None,
            agenda_points: None,
            trash_cost: None,
            influence_cost: None,
            memory_cost: None,
            min_deck_size: None,
            base_link: None,
            unique: false,
            keywords: Vec::new(),
            text: None,
        }
    }

    #[test]
    fn merge_overwrites_by_id() {
        let mut catalog = CardCatalog::new();
        catalog.insert_definition(blank_definition(1, "Original Title"));
        catalog.merge(vec![blank_definition(1, "Renamed Title")]);

        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog.get_by_id(CardId(1)).unwrap().title, "Renamed Title");
    }
}
