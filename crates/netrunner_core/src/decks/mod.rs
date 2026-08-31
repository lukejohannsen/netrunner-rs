//! Null Signal Games' official System Gateway sample decklists, embedded at
//! compile time.
//!
//! These are the seven System Gateway-only decks published at
//! <https://nullsignal.games/players/getting-started-sample-decklists/> —
//! three Runner, four Corp, giving twelve legal matchups. They exist so
//! every consumer that needs a real, playable pair of decks (self-play
//! training, the gym environment, the single-player CLI) draws from one
//! source of truth instead of hand-rolling its own fixture.
//!
//! Authored one-file-per-deck under `data/decks/`, concatenated by
//! `build.rs` and baked in via `include_str!`, mirroring `cards::embedded`
//! exactly — so this stays I/O-free at runtime and data-driven per
//! AGENTS.md rather than hardcoded in Rust.
//!
//! The remaining decks on that page (numbers 8-28) combine System Gateway
//! with *Elevation*, which is embedded as catalog-only metadata with no DSL
//! implementations, so they are not representable here.

use serde::{Deserialize, Serialize};

use crate::dsl::CardId;
use crate::rules::{Deck, Side};

const DECKS_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/decks.json"));

/// One entry in a decklist: a card and how many copies of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeckEntry {
    pub card: CardId,
    pub count: u32,
}

/// A published sample decklist.
///
/// Cards are referenced by registry `id`, never by title — several System
/// Gateway titles carry non-ASCII characters that are easy to mistype
/// (*Tomorrowʼs Headline* uses U+02BC MODIFIER LETTER APOSTROPHE, matching
/// NetrunnerDB's own data, and *Karunā*/*Brân 1.0*/*Tāo Salonga* carry
/// diacritics), and an id typo is caught by the registry lookup while a
/// title typo would silently miss.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampleDeck {
    pub id: String,
    pub name: String,
    pub side: Side,
    pub identity: CardId,
    pub cards: Vec<DeckEntry>,
}

impl SampleDeck {
    /// The `rules::Deck` this decklist describes, ready for
    /// `GameState::setup`. Does not validate — call
    /// `rules::deck::validate_deck` for that.
    pub fn to_deck(&self) -> Deck {
        Deck {
            identity: self.identity.clone(),
            cards: self.cards.iter().map(|entry| (entry.card.clone(), entry.count)).collect(),
        }
    }

    /// Total non-identity cards.
    pub fn size(&self) -> u32 {
        self.cards.iter().map(|entry| entry.count).sum()
    }
}

/// Parses the embedded decklists.
///
/// A failure here is an authoring bug in a checked-in deck file that got
/// past the test suite, not a runtime condition a caller could recover
/// from — so it panics rather than returning a `Result` every consumer
/// would have to `unwrap` anyway, matching `cards::embedded`'s reasoning.
fn parse() -> Vec<SampleDeck> {
    serde_json::from_str(DECKS_JSON).unwrap_or_else(|e| panic!("embedded deck data failed to parse: {e}"))
}

/// Every sample deck, ordered by file name (so, by deck id).
pub fn sample_decks() -> Vec<SampleDeck> {
    parse()
}

/// The sample deck with this id, if one exists.
pub fn by_id(id: &str) -> Option<SampleDeck> {
    parse().into_iter().find(|deck| deck.id == id)
}

/// The sample decks for one side.
pub fn for_side(side: Side) -> Vec<SampleDeck> {
    parse().into_iter().filter(|deck| deck.side == side).collect()
}

/// Every legal `(corp, runner)` pairing, ordered deterministically.
///
/// Four Corp decks against three Runner decks — twelve matchups, which is
/// the pool self-play samples from so training sees the whole card set
/// rather than one fixed matchup's slice of it.
pub fn matchups() -> Vec<(SampleDeck, SampleDeck)> {
    let corps = for_side(Side::Corp);
    let runners = for_side(Side::Runner);
    corps.iter().flat_map(|corp| runners.iter().map(move |runner| (corp.clone(), runner.clone()))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{register_playable_cards, CardRegistry};
    use crate::rules::deck::validate_deck;

    fn registry() -> CardRegistry {
        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        registry
    }

    #[test]
    fn every_sample_deck_is_legal() {
        let registry = registry();
        for deck in sample_decks() {
            validate_deck(&deck.to_deck(), deck.side, &registry)
                .unwrap_or_else(|e| panic!("sample deck {:?} ({}) is not legal: {e:?}", deck.id, deck.name));
        }
    }

    /// Pins the published sizes and agenda points. A card-data edit that
    /// changes an agenda's point value or an identity's minimum deck size
    /// would otherwise silently break a real decklist; this fails instead.
    #[test]
    fn sample_decks_match_the_published_lists() {
        let decks = sample_decks();
        assert_eq!(decks.len(), 7, "seven System Gateway-only sample decks are published");

        let mut sizes: Vec<(String, Side, u32)> =
            decks.iter().map(|deck| (deck.id.clone(), deck.side, deck.size())).collect();
        sizes.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(
            sizes,
            vec![
                ("advanced_yomi".to_string(), Side::Corp, 44),
                ("discretion_advised".to_string(), Side::Corp, 44),
                ("hyper_velocity".to_string(), Side::Corp, 44),
                ("party_hard".to_string(), Side::Runner, 40),
                ("planning_ahead".to_string(), Side::Runner, 40),
                ("quick_and_dirty".to_string(), Side::Corp, 44),
                ("stolen_goods".to_string(), Side::Runner, 40),
            ]
        );
    }

    #[test]
    fn every_corp_deck_carries_eighteen_agenda_points() {
        let registry = registry();
        for deck in for_side(Side::Corp) {
            let points: u32 = deck
                .cards
                .iter()
                .map(|entry| {
                    let card = registry.get(&entry.card).expect("deck card is registered");
                    card.agenda_points.unwrap_or(0) * entry.count
                })
                .sum();
            assert_eq!(points, 18, "{} should carry 18 agenda points", deck.id);
        }
    }

    #[test]
    fn matchups_cover_every_corp_runner_pairing() {
        let matchups = matchups();
        assert_eq!(matchups.len(), 12, "4 Corp decks x 3 Runner decks");
        for (corp, runner) in &matchups {
            assert_eq!(corp.side, Side::Corp);
            assert_eq!(runner.side, Side::Runner);
        }
    }

    #[test]
    fn by_id_finds_a_deck_and_rejects_an_unknown_one() {
        assert_eq!(by_id("party_hard").map(|deck| deck.name), Some("Party Hard".to_string()));
        assert!(by_id("no_such_deck").is_none());
    }
}
