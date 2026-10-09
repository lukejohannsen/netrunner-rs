pub mod catalog;
pub(crate) mod common;
mod embedded;
#[cfg(feature = "fs-loader")]
mod loader;
mod registry;
#[cfg(test)]
mod unimplemented;

#[cfg(test)]
mod tests;

pub use embedded::{embedded_playable_cards, register_embedded_cards};
#[cfg(feature = "fs-loader")]
pub use loader::{load_registry_from_dirs, LoaderError};
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
            ("the_catalyst_convention_breaker", "The Catalyst: Convention Breaker", 30),
            ("the_syndicate_profit_over_principle", "The Syndicate: Profit over Principle", 30),
        ] {
            let card = registry.get(&CardId(id.to_string())).unwrap_or_else(|| panic!("{id} should be registered"));
            assert_eq!(card.title, title);
            assert!(card.is_playable, "{id} is playable for the starter game");
            assert!(card.triggers.is_empty() && card.abilities.is_empty(), "{id} is blank: \"Starter game only.\"");
            assert_eq!(card.min_deck_size, Some(min_deck_size), "{id}: the catalog's printed minimum");
            assert!(card.unlimited_influence, "{id}: the catalog's influence_limit is null");
            assert_eq!(card.deck_rules, [crate::dsl::DeckRule::StarterGameOnly], "{id}: \"Starter game only.\"");
        }
    }

    /// An identity's deckbuilding sentences are its `deck_rules`, held to
    /// the printed text so a new identity that prints one cannot be built
    /// without it: "Starter game only.", "cannot include more than 1 copy
    /// of any card", "up to 2 different agenda cards from each Corp
    /// faction". And no budget in the catalog is `unlimited_influence` in
    /// the card file — the flag a validator reads.
    #[test]
    fn every_identity_states_the_deckbuilding_rules_it_prints() {
        use crate::dsl::DeckRule;
        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        let catalog = catalog::cards();
        for card in registry.iter().filter(|card| card.card_type == crate::dsl::CardType::Identity) {
            let text = card.printed_text.as_deref().unwrap_or_default();
            let mut printed = Vec::new();
            if text.contains("Starter game only.") {
                printed.push(DeckRule::StarterGameOnly);
            }
            if text.contains("Your deck cannot include more than 1 copy of any card.") {
                printed.push(DeckRule::CopiesOfEachCard(1));
            }
            if text.contains("Your deck may include up to 2 different agenda cards from each Corp faction.") {
                printed.push(DeckRule::AgendasFromEachFaction(2));
            }
            assert_eq!(card.deck_rules, printed, "{}", card.id.0);
            if let Some(entry) = catalog.get(&card.id) {
                assert_eq!(card.unlimited_influence, entry.influence_limit.is_none(), "{}: the catalog's influence_limit", card.id.0);
            }
        }
    }
}

#[cfg(test)]
mod reprint_tests {
    use super::*;
    use crate::dsl::CardId;

    /// A reprint is a printing, never a second card. Sure Gamble and Hedge
    /// Fund are printed in all four core sets, and each is one
    /// entry in the catalog and one in the playable registry, under the same
    /// v3 id: the v2 catalog made one `nrdb_<code>` entry per printing, so
    /// the playable card sat beside two catalog copies of itself and a
    /// title fold was what told them apart.
    #[test]
    fn a_reprinted_card_is_one_card_with_several_printings() {
        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        for (id, printings) in [("hedge_fund", 4), ("sure_gamble", 4), ("cleaver", 1)] {
            let id = CardId(id.to_string());
            assert!(registry.get(&id).is_some_and(|card| card.is_playable), "{} is playable", id.0);
            assert!(catalog::cards().get(&id).is_some_and(|card| !card.is_playable), "{} is in the catalog", id.0);
            assert_eq!(catalog::printings_of(&id).count(), printings, "{}", id.0);
        }
    }
}
