//! The cards a client may offer: which of the registry a format allows.
//!
//! The validator (`netrunner_core::deck::validator`) is still what decides
//! whether a deck is legal; this only keeps cards it would refuse out of
//! the pool a builder or browser shows, so a Startup player never sees a
//! Core Set card the validator would then reject. It was private to the
//! TUI's deck builder; the desktop's builder and card browser need the
//! same answer, which is why it is here.

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardDefinition;
use netrunner_core::format::{FormatRules, NsgFormat};

/// Whether the format's tables allow `card`: its pack, and not banned. The
/// validator makes the same two checks and is still what decides.
pub fn legal_in(card: &CardDefinition, rules: &FormatRules) -> bool {
    let Some(code) = card.numeric_id else { return false };
    !rules.banned.contains(&code)
        && rules.allowed_packs.as_ref().is_none_or(|packs| packs.contains(card.set_code.as_deref().unwrap_or("")))
}

/// Every card in `registry` the format allows, in registry order. Callers
/// filter by side and type themselves — a deck builder wants one side and
/// no identities, a browser wants everything.
pub fn format_pool(registry: &CardRegistry, format: NsgFormat) -> Vec<&CardDefinition> {
    let rules = format.rules();
    registry.iter().filter(|card| legal_in(card, &rules)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::cards;

    fn playable() -> CardRegistry {
        let mut registry = CardRegistry::new();
        cards::register_playable_cards(&mut registry);
        registry
    }

    /// Startup is System Gateway plus Elevation; Eternal is everything with
    /// a catalog entry. The Core Set cards are the difference.
    #[test]
    fn the_pool_is_the_formats_packs_and_nothing_without_a_catalog_entry() {
        let registry = playable();
        let startup = format_pool(&registry, NsgFormat::Startup);
        let eternal = format_pool(&registry, NsgFormat::Eternal);
        assert!(startup.iter().all(|card| matches!(card.set_code.as_deref(), Some("sg" | "elev"))));
        assert!(startup.len() < eternal.len(), "Core Set cards are outside Startup");
        assert_eq!(eternal.len(), registry.iter().filter(|card| card.numeric_id.is_some()).count());
        assert!(!startup.iter().any(|card| card.id.0 == "ice_wall"), "Ice Wall is Core Set");
        assert!(eternal.iter().any(|card| card.id.0 == "ice_wall"));
    }
}
