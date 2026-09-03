//! The decks this crate seats a match on.
//!
//! Thin by design: the pool is `netrunner_core::decks`, Null Signal
//! Games' published sample decklists, and this module is only where the
//! daemon, its headless driver and its tests agree on *which* matchup a
//! given seed plays. It used to be something else — a hand-built Kate
//! "Mac" McCaffrey vs. Haas-Bioroid pair padded out to legal size with
//! blank `CardDefinition`s, a deliberate near-copy of the same fixture in
//! `netrunner_cli`. That fixture is gone: 24 of the Corp's 45 cards and 27
//! of the Runner's 45 had no text at all, so every human match the daemon
//! hosted, and every rating it wrote, was played on a board that mostly
//! did nothing. `ROADMAP.md` Phase 2 §3 records the same fixture's effect
//! on self-play — 5,000 games, every one a Corp loss.
//!
//! Deleting it also removed a cross-crate coupling. The wire protocol
//! never transmits a `CardRegistry`, so `netrunner_cli --mode remote`
//! builds its own to resolve card titles; while the filler existed, the
//! two crates had to agree on its counts by hand. Every card the daemon
//! can now deal is in `register_playable_cards`, so they agree by
//! construction.

use netrunner_core::cards::{self, CardRegistry};
use netrunner_core::decks;
use netrunner_core::rules::Deck;

/// Every implemented card, and nothing synthetic.
///
/// The same body as `netrunner_cli::decks::sample_deck_registry`. Kept as
/// its own function rather than shared through a new crate: it is one
/// line, and the duplication its predecessor apologised for was the
/// seventeen filler definitions, not this.
pub fn sample_registry() -> CardRegistry {
    let mut registry = CardRegistry::new();
    cards::register_playable_cards(&mut registry);
    registry
}

/// One dealt matchup: the two decks, each with the published id it came
/// from. A struct rather than a 4-tuple because the ids travel with the
/// decks all the way to `MatchJoined`, and `(String, Deck, String, Deck)`
/// is two positions away from being silently swapped.
#[derive(Debug, Clone)]
pub struct DealtMatchup {
    pub corp_id: String,
    pub corp: Deck,
    pub runner_id: String,
    pub runner: Deck,
}

/// The published matchup `seed` plays: `matchups()[seed % len]`, the
/// rotation `netrunner_cli --all-matchups` and the gym already use.
///
/// Rotating rather than fixing a pair means a daemon left running deals
/// its way around the whole 16 × 12 pool, so the games it rates are drawn
/// from the same distribution every bot in the workspace is measured on.
pub fn sample_decks_for_seed(seed: u64) -> DealtMatchup {
    let matchups = decks::matchups();
    let (corp, runner) = &matchups[(seed % matchups.len() as u64) as usize];
    DealtMatchup {
        corp_id: corp.id.clone(),
        corp: corp.to_deck(),
        runner_id: runner.id.clone(),
        runner: runner.to_deck(),
    }
}

/// The first matchup, for tests that want *a* legal game and do not care
/// which.
pub fn sample_decks() -> (Deck, Deck) {
    let dealt = sample_decks_for_seed(0);
    (dealt.corp, dealt.runner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{validate_deck, GameState, Side};

    #[test]
    fn decks_are_individually_legal() {
        let registry = sample_registry();

        for seed in 0..decks::matchups().len() as u64 {
            let dealt = sample_decks_for_seed(seed);
            assert_eq!(validate_deck(&dealt.corp, Side::Corp, &registry), Ok(()), "{}", dealt.corp_id);
            assert_eq!(validate_deck(&dealt.runner, Side::Runner, &registry), Ok(()), "{}", dealt.runner_id);
        }
    }

    #[test]
    fn decks_pass_full_game_setup() {
        let registry = sample_registry();
        let (corp_deck, runner_deck) = sample_decks();

        assert!(GameState::setup(&corp_deck, &runner_deck, &registry, 42).is_ok());
    }

    /// The property the daemon's seed policy rests on: consecutive matches
    /// deal *different* matchups, so a long-lived daemon covers the pool
    /// instead of replaying one pairing.
    #[test]
    fn consecutive_seeds_rotate_through_the_pool() {
        let pair = |seed| {
            let dealt = sample_decks_for_seed(seed);
            (dealt.corp_id, dealt.runner_id)
        };
        let len = decks::matchups().len() as u64;

        assert_ne!(pair(0), pair(1));
        assert_eq!(pair(0), pair(len), "the rotation wraps at the pool size");
        let distinct: std::collections::BTreeSet<(String, String)> = (0..len).map(pair).collect();
        assert_eq!(distinct.len() as u64, len, "one pass deals every matchup exactly once");
    }
}
