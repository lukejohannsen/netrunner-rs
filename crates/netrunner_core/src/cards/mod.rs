pub(crate) mod common;
mod embedded;
#[cfg(feature = "fs-loader")]
mod loader;
pub mod netrunnerdb;
mod registry;

#[cfg(test)]
mod tests;

pub use embedded::{embedded_playable_cards, register_embedded_cards};
#[cfg(feature = "fs-loader")]
pub use loader::{load_registry_from_dirs, LoaderError};
pub use netrunnerdb::{convert_dtos, convert_dtos_lenient, load_embedded_netrunnerdb_sets, EmbeddedSetsError};
pub use registry::CardRegistry;

/// Registers every hand-authored, gameplay-complete card into `registry` —
/// the baseline Core Set suite plus every implemented System Gateway card.
/// The single entry point a caller (server, gym, CLI, client) reaches for to
/// get a populated `CardRegistry`.
///
/// Cards come from the compile-time-embedded `data/corp`/`data/runner` JSON
/// (see `embedded`), so this needs no feature flag and touches no
/// filesystem. The registry is a lookup pool, not a behavior input — legal
/// actions derive from what's actually in hand/rig/installed — so carrying
/// cards a given match never uses is free.
pub fn register_playable_cards(registry: &mut CardRegistry) {
    register_embedded_cards(registry);
}

#[cfg(test)]
mod sg_starter_identity_tests {
    use super::*;
    use crate::card::CardId as NumericCardId;

    /// "The Catalyst: Convention Breaker" (Runner, code 30076) and "The
    /// Syndicate: Profit over Principle" (Corp, code 30077) are System
    /// Gateway's tutorial-only starter identities — their NetrunnerDB
    /// `stripped_text` is literally `"Starter game only."`, with no rules
    /// text at all to implement. They stay `is_playable: false`
    /// permanently; this is a documented decision, not a gap to fill in a
    /// later milestone.
    #[test]
    fn starter_only_identities_have_no_rules_text_and_stay_permanently_unplayable() {
        let registry = load_embedded_netrunnerdb_sets().expect("embedded sets should parse");

        let catalyst = registry.get_by_numeric_id(NumericCardId(30076)).expect("The Catalyst should be in the SG catalog");
        assert_eq!(catalyst.title, "The Catalyst: Convention Breaker");
        assert!(!catalyst.is_playable);

        let syndicate =
            registry.get_by_numeric_id(NumericCardId(30077)).expect("The Syndicate should be in the SG catalog");
        assert_eq!(syndicate.title, "The Syndicate: Profit over Principle");
        assert!(!syndicate.is_playable);
    }
}

#[cfg(test)]
mod sg_reprint_dedup_tests {
    use super::*;

    /// Sure Gamble, Hedge Fund, and Cleaver are exact System Gateway
    /// reprints of existing hand-authored baseline cards (see
    /// `data/corp/hedge_fund.json`/`data/runner/sure_gamble.json`/
    /// `data/runner/cleaver.json`'s SG metadata). Loading the
    /// hand-authored baseline JSON alongside the embedded NetrunnerDB
    /// catalog conversion necessarily produces *two* distinct
    /// `CardRegistry` entries per title, not one merged entry — the
    /// NetrunnerDB conversion path slugs every card `nrdb_<numeric_id>`
    /// (`cards::netrunnerdb::convert_one`), which never collides with the
    /// hand-authored `hedge_fund`/`sure_gamble`/`cleaver` `dsl::CardId`
    /// slug. This test proves only one of the two ever ends up
    /// `is_playable: true` — `rules::deck::validate_deck` rejects the
    /// other — not that the registry deduplicates them into a single
    /// entry.
    #[test]
    fn hedge_fund_and_sure_gamble_have_exactly_one_playable_entry_after_merging_baseline_and_sg_catalog() {
        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        let sg_catalog = load_embedded_netrunnerdb_sets().expect("embedded sets should parse");
        registry.merge(sg_catalog.iter().cloned());

        for title in ["Hedge Fund", "Sure Gamble", "Cleaver"] {
            let matches: Vec<_> = registry.iter().filter(|c| c.title == title).collect();
            assert_eq!(matches.len(), 2, "expected both the hand-authored and catalog-only {title} entries to coexist");
            let playable: Vec<_> = matches.iter().filter(|c| c.is_playable).collect();
            assert_eq!(playable.len(), 1, "expected exactly one playable {title} entry, found {playable:?}");
        }
    }
}
