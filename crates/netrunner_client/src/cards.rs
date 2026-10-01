//! The cards a client may offer: which of the registry a format allows,
//! every card the browser lists, and the words a list labels a card with.
//!
//! The validator (`netrunner_core::deck::validator`) is still what decides
//! whether a deck is legal; `legal_in` only keeps cards it would refuse
//! out of the pool a builder or browser shows, so a Startup player never
//! sees a Core Set card the validator would then reject. It was private
//! to the TUI's deck builder; the desktop's builder and card browser need
//! the same answer, which is why it is here. The label helpers came the
//! same way: a type group and a faction name are one spelling in both
//! clients or they are two.

use netrunner_core::card::Faction;
use netrunner_core::cards::{catalog, CardRegistry};
use netrunner_core::dsl::{CardDefinition, CardType};
use netrunner_core::format::{FormatRules, NsgFormat};

use crate::settings::FORMATS;

/// Whether the format's tables allow `card`: in its pool and not banned,
/// by its id. The validator makes the same two checks and is still what
/// decides. A card no NetrunnerDB pool lists (homebrew, a test fixture) is
/// legal only in Casual, which lists no pool.
pub fn legal_in(card: &CardDefinition, rules: &FormatRules) -> bool {
    !rules.banned.contains(&card.id) && rules.in_pool(&card.id)
}

/// The formats whose tables allow `card`, in the order the settings
/// screen lists them — what an inspector prints under "Legal in".
pub fn legal_formats(card: &CardDefinition) -> Vec<NsgFormat> {
    FORMATS.into_iter().filter(|format| legal_in(card, format.rules())).collect()
}

/// The name a set is listed under, by its v3 id — the catalog's; an id the
/// catalog does not know is shown as itself.
pub fn set_name(set: &str) -> &str {
    catalog::set(set).map_or(set, |set| set.name.as_str())
}

/// Every card in `registry` the format allows, in registry order. Callers
/// filter by side and type themselves — a deck builder wants one side and
/// no identities, a browser wants everything.
pub fn format_pool(registry: &CardRegistry, format: NsgFormat) -> Vec<&CardDefinition> {
    let rules = format.rules();
    registry.iter().filter(|card| legal_in(card, rules)).collect()
}

/// Every card in the embedded catalog, with the playable card standing in
/// wherever one implements it — the list a browser shows.
///
/// A registry holds what a match can play (`is_playable`), which is a
/// subset of what was printed; a browser promises "every card", and the
/// gap between the two is information a player wants ("is Boomerang in
/// yet?"). The playable card is preferred because it carries the DSL the
/// inspector's "Engine reads it as" reads, and the catalog fills its
/// printed text and numbers on the way in, so nothing is lost by the
/// swap. **One entry per card, never per printing**: a reprint is the same
/// card, and its printings are the inspector's to list
/// (`netrunner_core::cards::catalog::printings_of`). Until the catalog
/// moved to NetrunnerDB v3 (NSG pool Stage 0d) this was one entry per
/// printing, joined to the playable card by the printing its file named,
/// so the Core Set's Hedge Fund read "not implemented" beside the System
/// Gateway one that was. Sorted by side, type, title, then id, so the
/// order is stable whatever the registry's hash order is.
pub fn catalog(registry: &CardRegistry) -> Vec<CardDefinition> {
    let mut cards: Vec<CardDefinition> = catalog::cards()
        .iter()
        .map(|entry| registry.get(&entry.id).filter(|card| card.is_playable).unwrap_or(entry).clone())
        .collect();
    cards.sort_by(|a, b| {
        (a.side as u8, type_order(&a.card_type), &a.title, &a.id).cmp(&(b.side as u8, type_order(&b.card_type), &b.title, &b.id))
    });
    cards
}

/// Ice of every subtype is one group: a builder filters by "ICE", not by
/// "Barrier".
pub fn type_group(card_type: &CardType) -> &'static str {
    match card_type {
        CardType::Agenda => "Agenda",
        CardType::Asset => "Asset",
        CardType::Upgrade => "Upgrade",
        CardType::Operation => "Operation",
        CardType::Ice(_) => "ICE",
        CardType::Event => "Event",
        CardType::Hardware => "Hardware",
        CardType::Resource => "Resource",
        CardType::Program => "Program",
        CardType::Identity => "Identity",
    }
}

/// The Corp's and the Runner's types paired by role, so a list of either
/// side reads the same way down: identity, then what scores, what sits
/// in a server, what pays, what runs.
pub fn type_order(card_type: &CardType) -> u8 {
    match card_type {
        CardType::Identity => 0,
        CardType::Agenda | CardType::Event => 1,
        CardType::Asset | CardType::Hardware => 2,
        CardType::Upgrade | CardType::Resource => 3,
        CardType::Operation => 4,
        CardType::Ice(_) | CardType::Program => 5,
    }
}

pub fn faction_label(faction: Faction) -> &'static str {
    match faction {
        Faction::Anarch => "Anarch",
        Faction::Criminal => "Criminal",
        Faction::Shaper => "Shaper",
        Faction::HaasBioroid => "Haas-Bioroid",
        Faction::Jinteki => "Jinteki",
        Faction::Nbn => "NBN",
        Faction::WeylandConsortium => "Weyland",
        Faction::NeutralCorp | Faction::NeutralRunner => "Neutral",
    }
}

/// The order the factions are listed in, each side's paired with the
/// other's so a mixed list interleaves rather than blocks.
pub fn faction_order(faction: Option<Faction>) -> u8 {
    match faction {
        Some(Faction::HaasBioroid) | Some(Faction::Anarch) => 0,
        Some(Faction::Jinteki) | Some(Faction::Criminal) => 1,
        Some(Faction::Nbn) | Some(Faction::Shaper) => 2,
        Some(Faction::WeylandConsortium) => 3,
        Some(Faction::NeutralCorp) | Some(Faction::NeutralRunner) => 4,
        None => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playable() -> CardRegistry {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        registry
    }

    /// Startup is NetrunnerDB's Startup pool (System Gateway, Elevation,
    /// Vantage Point); Eternal is every card. The Core Set cards are the
    /// difference. Asked of the pool itself, not a list of sets, so a set
    /// that joins Startup does not make this stale.
    #[test]
    fn the_pool_is_the_formats_cards() {
        let registry = playable();
        let startup = format_pool(&registry, NsgFormat::Startup);
        let eternal = format_pool(&registry, NsgFormat::Eternal);
        let rules = NsgFormat::Startup.rules();
        assert!(startup.iter().all(|card| rules.in_pool(&card.id)));
        assert!(startup.iter().any(|card| catalog::printed_in(&card.id, "vantage_point")), "Vantage Point is Startup");
        assert!(startup.len() < eternal.len(), "Core Set cards are outside Startup");
        assert_eq!(eternal.len(), registry.len());
        assert!(!startup.iter().any(|card| card.id.0 == "ice_wall"), "Ice Wall is Core Set");
        assert!(eternal.iter().any(|card| card.id.0 == "ice_wall"));
    }

    /// A card's formats follow its pool, by any printing: Ice Wall is
    /// outside Startup and Standard (Null Signal Games' pools) and inside
    /// Snapshot, whose Revised Core Set reprints it; Tithe (System Gateway)
    /// is in every format but Snapshot, a Fantasy Flight Games pool.
    /// Casual holds both.
    #[test]
    fn a_cards_legal_formats_follow_its_pool() {
        use NsgFormat::{Casual, Eternal, Snapshot, Standard, Startup};
        let registry = playable();
        let catalog = catalog(&registry);
        let ice_wall = catalog.iter().find(|card| card.id.0 == "ice_wall").unwrap();
        assert_eq!(legal_formats(ice_wall), vec![Eternal, Snapshot, Casual]);
        let tithe = catalog.iter().find(|card| card.title == "Tithe").unwrap();
        assert_eq!(legal_formats(tithe), vec![Startup, Standard, Eternal, Casual]);
        assert_eq!(set_name("system_gateway"), "System Gateway");
        assert_eq!(set_name("xyz"), "xyz");
    }

    /// The catalog is every card once, the playable card standing in for
    /// it: the Core Set's Hedge Fund is no longer a second, unplayable
    /// entry beside the one the engine plays.
    #[test]
    fn the_catalog_is_every_card_once_with_playable_cards_standing_in() {
        let registry = playable();
        let catalog = catalog(&registry);
        assert_eq!(catalog.len(), netrunner_core::cards::catalog::cards().len());
        let mut ids: Vec<_> = catalog.iter().map(|card| &card.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), catalog.len(), "no card is listed twice");
        let hedge_funds: Vec<_> = catalog.iter().filter(|card| card.title == "Hedge Fund").collect();
        assert_eq!(hedge_funds.len(), 1, "a reprint is the same card");
        assert!(hedge_funds[0].is_playable);
        assert_eq!(catalog.iter().filter(|card| card.is_playable).count(), registry.len());
        assert!(catalog.windows(2).all(|pair| pair[0].side as u8 <= pair[1].side as u8), "Corp before Runner");
    }
}
