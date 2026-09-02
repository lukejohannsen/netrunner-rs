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
    use crate::dsl::CardId;

    /// "The Catalyst: Convention Breaker" (Runner, code 30076) and "The
    /// Syndicate: Profit over Principle" (Corp, code 30077) are System
    /// Gateway's tutorial-only starter identities — their NetrunnerDB
    /// `stripped_text` is literally `"Starter game only."`. They were kept
    /// unplayable while no tutorial existed (a blank identity legal in
    /// Standard is a deckbuilding hole); *Learn to Play* (ROADMAP Phase
    /// 1.75) is the use. The assertion that matters is **blankness**: no
    /// triggers, no abilities — that is what keeps them honest as tutorial
    /// identities, and it is the reason implementing them cost nothing.
    #[test]
    fn starter_identities_are_playable_and_blank() {
        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        for (id, title, min_deck_size) in [
            ("the_catalyst", "The Catalyst: Convention Breaker", 30),
            ("the_syndicate", "The Syndicate: Profit over Principle", 30),
        ] {
            let card = registry.get(&CardId(id.to_string())).unwrap_or_else(|| panic!("{id} should be registered"));
            assert_eq!(card.title, title);
            assert!(card.is_playable, "{id} is playable for the starter game");
            assert!(card.triggers.is_empty() && card.abilities.is_empty(), "{id} is blank: \"Starter game only.\"");
            assert_eq!(card.min_deck_size, Some(min_deck_size), "{id}: the catalog's printed minimum");
            assert!(card.unlimited_influence, "{id}: the catalog's influence_limit is null");
        }
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
    /// slug. This test proves only one of them ever ends up
    /// `is_playable: true` — `rules::deck::validate_deck` rejects the
    /// rest — not that the registry deduplicates them into a single
    /// entry.
    ///
    /// The expected count differs per title because these cards have been
    /// printed a different number of times: *Hedge Fund* and *Sure Gamble*
    /// are in the Core Set catalog as well as System Gateway's, so each
    /// contributes its own `nrdb_<code>` entry, while *Cleaver* is System
    /// Gateway only. Spelled out per title rather than collapsed to "more
    /// than one" so an unexpected extra printing still fails here.
    #[test]
    fn hedge_fund_and_sure_gamble_have_exactly_one_playable_entry_after_merging_baseline_and_sg_catalog() {
        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        let sg_catalog = load_embedded_netrunnerdb_sets().expect("embedded sets should parse");
        registry.merge(sg_catalog.iter().cloned());

        for (title, printings) in [("Hedge Fund", 3), ("Sure Gamble", 3), ("Cleaver", 2)] {
            let matches: Vec<_> = registry.iter().filter(|c| c.title == title).collect();
            assert_eq!(
                matches.len(),
                printings,
                "expected the hand-authored {title} entry to coexist with its catalog printings"
            );
            let playable: Vec<_> = matches.iter().filter(|c| c.is_playable).collect();
            assert_eq!(playable.len(), 1, "expected exactly one playable {title} entry, found {playable:?}");
        }
    }
}
