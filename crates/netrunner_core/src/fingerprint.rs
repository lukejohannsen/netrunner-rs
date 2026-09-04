//! One stable hash over the data every game is played with: the playable
//! card pool and the sample deck pool.
//!
//! Exists because a training corpus is only as good as the engine that
//! generated it, and that has now gone wrong three times without
//! announcing itself (ROADMAP Phase 2 §5). The second 2,400-game run's
//! loop shelled out to `cargo run` for every stage, so iterations 8 and 9
//! silently recompiled against three *Elevation* stages landing in the
//! working tree beside them: the deck pool grew from 12 matchups to 36 and
//! the card-identity planes reindexed, while the recorded observation and
//! action widths — the only thing the trainer checked — never moved.
//! Recording this value in each trajectory turns that mixture into a
//! refusal instead of a training run.
//!
//! What it does not catch is a rules change in Rust with no card- or
//! deck-data edit. Pinning the built binary for the life of a run
//! (`scripts/run_iteration_loop.py`) is what covers that; the two together
//! are the fix.

use std::sync::OnceLock;

use crate::cards::{self, CardRegistry};
use crate::decks;

/// FNV-1a offset basis and prime, 64-bit.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// A 16-hex-digit fingerprint of the embedded card pool
/// (`cards::register_playable_cards`) and the sample deck pool
/// (`decks::matchups`), each entry hashed as its serialized JSON in id
/// order.
///
/// **FNV-1a by hand rather than `std::hash::DefaultHasher`**: this value is
/// compared across processes and across days, and `DefaultHasher`'s output
/// is explicitly not guaranteed stable between Rust releases. Hashing the
/// JSON rather than the ids means a card whose *text* changed, or a
/// decklist whose contents changed, fingerprints differently even though
/// nothing was added or removed.
pub fn pool_fingerprint() -> String {
    // Cached: the embedded pool cannot change within a process, and
    // self-play asks for this once per game across every core.
    static FINGERPRINT: OnceLock<String> = OnceLock::new();
    FINGERPRINT.get_or_init(compute_pool_fingerprint).clone()
}

fn compute_pool_fingerprint() -> String {
    let mut registry = CardRegistry::new();
    cards::register_playable_cards(&mut registry);

    let mut entries: Vec<String> = registry
        .iter()
        .map(|card| format!("card {} {}", card.id.0, serde_json::to_string(card).unwrap_or_default()))
        .collect();
    entries.extend(decks::matchups().iter().flat_map(|(corp, runner)| {
        [corp, runner].map(|deck| format!("deck {} {}", deck.id, serde_json::to_string(deck).unwrap_or_default()))
    }));
    entries.sort_unstable();
    entries.dedup();

    let mut hash = FNV_OFFSET;
    for entry in entries {
        for byte in entry.as_bytes().iter().chain(b"\n") {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fingerprint_is_sixteen_stable_hex_digits() {
        let fingerprint = pool_fingerprint();
        assert_eq!(fingerprint.len(), 16);
        assert!(fingerprint.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(fingerprint, pool_fingerprint(), "the same pool must hash the same in the same process");
    }

    /// The property that makes it worth recording: the two shapes of change
    /// that went unnoticed mid-run — a card joining the pool, and a card's
    /// text changing without the pool's size moving — both change it.
    #[test]
    fn a_changed_pool_changes_the_fingerprint() {
        let baseline = hash_entries(&entries());

        let mut with_a_new_card = entries();
        with_a_new_card.push("card a_card_from_the_next_set {}".to_string());
        assert_ne!(hash_entries(&with_a_new_card), baseline, "a card joining the pool must change it");

        let mut edited = entries();
        edited[0].push_str(" (errata)");
        assert_ne!(hash_entries(&edited), baseline, "an edited card must change it");
    }

    /// The entry list `pool_fingerprint` hashes, so a test can perturb it.
    /// Not exported: hashing an arbitrary pool is not something a caller
    /// should do — the point of the value is that it names the *canonical*
    /// one.
    fn entries() -> Vec<String> {
        let mut registry = CardRegistry::new();
        cards::register_playable_cards(&mut registry);
        let mut entries: Vec<String> = registry
            .iter()
            .map(|card| format!("card {} {}", card.id.0, serde_json::to_string(card).unwrap_or_default()))
            .collect();
        entries.sort_unstable();
        entries
    }

    fn hash_entries(entries: &[String]) -> u64 {
        let mut sorted = entries.to_vec();
        sorted.sort_unstable();
        let mut hash = FNV_OFFSET;
        for entry in sorted {
            for byte in entry.as_bytes().iter().chain(b"\n") {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
        hash
    }
}
