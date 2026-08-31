//! Supplies the real System Gateway matchups self-play trains on.
//!
//! This used to build a Kate "Mac" McCaffrey vs. Haas-Bioroid: Engineering
//! the Future deck pair padded to legal size with blank filler cards. That
//! fixture produced training data with no learning signal: the Corp deck
//! was 18 blank agendas and 2 blank assets in 45 cards behind only 6 ICE,
//! so the Runner walked in and stole, and every one of 5,000 recorded games
//! ended in a Corp loss. A constant outcome teaches a value head nothing,
//! and no System Gateway card appeared in a single game.
//!
//! Decks now come from `netrunner_core::decks` — Null Signal Games' seven
//! published System Gateway sample decklists, giving twelve real matchups
//! over the whole implemented card set.

use netrunner_core::cards::{self, CardRegistry};
use netrunner_core::decks::{self, DeckFile};
use netrunner_core::rules::Deck;

/// The card pool for every self-play game: every implemented card, with no
/// synthetic filler.
pub fn registry() -> CardRegistry {
    let mut registry = CardRegistry::new();
    cards::register_playable_cards(&mut registry);
    registry
}

/// One playable pairing: the two decks plus the ids that identify it in a
/// recorded trajectory.
#[derive(Debug, Clone)]
pub struct Matchup {
    pub corp: DeckFile,
    pub runner: DeckFile,
}

impl Matchup {
    /// `"<corp_id>_vs_<runner_id>"` — recorded in each trajectory so a
    /// training run's data stays attributable to the decks that produced it.
    pub fn id(&self) -> String {
        format!("{}_vs_{}", self.corp.id, self.runner.id)
    }

    pub fn decks(&self) -> (Deck, Deck) {
        (self.corp.to_deck(), self.runner.to_deck())
    }
}

/// Every Corp/Runner pairing of the sample decks, in deterministic order.
pub fn matchups() -> Vec<Matchup> {
    decks::matchups().into_iter().map(|(corp, runner)| Matchup { corp, runner }).collect()
}

/// The matchup whose [`Matchup::id`] is `id`, if one exists.
pub fn matchup_by_id(id: &str) -> Option<Matchup> {
    matchups().into_iter().find(|matchup| matchup.id() == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{validate_deck, GameState, Side};

    #[test]
    fn every_matchup_is_legal_and_sets_up() {
        let registry = registry();
        let matchups = matchups();
        assert_eq!(matchups.len(), 12);

        for matchup in matchups {
            let (corp_deck, runner_deck) = matchup.decks();
            assert_eq!(validate_deck(&corp_deck, Side::Corp, &registry), Ok(()), "{}", matchup.id());
            assert_eq!(validate_deck(&runner_deck, Side::Runner, &registry), Ok(()), "{}", matchup.id());
            assert!(GameState::setup(&corp_deck, &runner_deck, &registry, 42).is_ok(), "{}", matchup.id());
        }
    }

    #[test]
    fn matchup_ids_are_unique_and_resolvable() {
        let ids: Vec<String> = matchups().iter().map(Matchup::id).collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "matchup ids must be unique");

        for id in &ids {
            assert!(matchup_by_id(id).is_some(), "{id} should resolve");
        }
        assert!(matchup_by_id("not_a_matchup").is_none());
    }

    /// The blank-filler fixture this module replaced had no real cards in
    /// it at all; guard against regressing to anything like it.
    #[test]
    fn decks_contain_only_real_registered_cards() {
        let registry = registry();
        for matchup in matchups() {
            for deck in [&matchup.corp, &matchup.runner] {
                for entry in &deck.cards {
                    let card = registry.get(&entry.card).unwrap_or_else(|| panic!("{:?} is registered", entry.card));
                    assert!(card.is_playable, "{:?} must be playable", entry.card);
                    assert!(
                        !card.id.0.starts_with("filler_"),
                        "{:?} looks like synthetic filler",
                        entry.card
                    );
                }
            }
        }
    }
}
