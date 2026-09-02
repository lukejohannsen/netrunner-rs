//! Supplies the System Gateway decks the gym environment plays.
//!
//! This used to build a Kate "Mac" McCaffrey vs. Haas-Bioroid: Engineering
//! the Future pair padded to legal size with blank filler cards — a deck
//! containing no System Gateway card at all, and whose Corp half was ~40%
//! blank agendas behind 6 ICE. Decks now come from `netrunner_core::decks`,
//! Null Signal Games' seven published System Gateway sample decklists.

use netrunner_core::cards::{self, CardRegistry};
use netrunner_core::decks;
use netrunner_core::rules::Deck;

/// The card pool for every episode: every implemented card, no filler.
pub fn registry() -> CardRegistry {
    let mut registry = CardRegistry::new();
    cards::register_playable_cards(&mut registry);
    registry
}

/// The `(corp, runner)` decks for an episode with this seed.
///
/// Rotates through every sample-deck pairing rather than pinning one,
/// so an agent trained here generalizes across the card pool instead of
/// overfitting a single matchup. Deterministic in `seed`, so an episode
/// replays identically.
pub fn decks_for_seed(seed: u64) -> (Deck, Deck) {
    let matchups = decks::matchups();
    let (corp, runner) = &matchups[(seed % matchups.len() as u64) as usize];
    (corp.to_deck(), runner.to_deck())
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{validate_deck, GameState, Side};

    #[test]
    fn every_seeded_pairing_is_legal_and_sets_up() {
        let registry = registry();
        // One seed per pairing covers all of them.
        for seed in 0..decks::matchups().len() as u64 {
            let (corp_deck, runner_deck) = decks_for_seed(seed);
            assert_eq!(validate_deck(&corp_deck, Side::Corp, &registry), Ok(()), "seed {seed}");
            assert_eq!(validate_deck(&runner_deck, Side::Runner, &registry), Ok(()), "seed {seed}");
            assert!(GameState::setup(&corp_deck, &runner_deck, &registry, seed).is_ok(), "seed {seed}");
        }
    }

    #[test]
    fn seeds_rotate_through_every_pairing() {
        // Keyed by deck, not identity: two published lists can share an
        // identity (Planning Ahead and Flow and Ebb are both Tāo Salonga).
        let pairings = decks::matchups().len();
        let distinct: std::collections::HashSet<Vec<String>> = (0..pairings as u64)
            .map(|seed| {
                let (corp, runner) = decks::matchups()[(seed % pairings as u64) as usize].clone();
                vec![corp.id, runner.id]
            })
            .collect();
        assert_eq!(distinct.len(), pairings, "every Corp deck x every Runner deck");
        // And `decks_for_seed` walks the same list.
        let (corp, runner) = decks_for_seed(pairings as u64);
        let (first_corp, first_runner) = &decks::matchups()[0];
        assert_eq!((corp.identity, runner.identity), (first_corp.to_deck().identity, first_runner.to_deck().identity));
    }
}
