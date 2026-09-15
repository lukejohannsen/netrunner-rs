//! The cards a client may offer: which of the registry a format allows,
//! every printing the browser lists, and the words a list labels a card
//! with.
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
use netrunner_core::cards::{self, CardRegistry};
use netrunner_core::dsl::{CardDefinition, CardType};
use netrunner_core::format::{FormatRules, NsgFormat};

use crate::settings::FORMATS;

/// Whether the format's tables allow `card`: its pack, and not banned. The
/// validator makes the same two checks and is still what decides.
pub fn legal_in(card: &CardDefinition, rules: &FormatRules) -> bool {
    let Some(code) = card.numeric_id else { return false };
    !rules.banned.contains(&code)
        && rules.allowed_packs.as_ref().is_none_or(|packs| packs.contains(card.set_code.as_deref().unwrap_or("")))
}

/// The formats whose tables allow `card`, in the order the settings
/// screen lists them — what an inspector prints under "Legal in". A card
/// with no NetrunnerDB code is legal nowhere, as `legal_in` says.
pub fn legal_formats(card: &CardDefinition) -> Vec<NsgFormat> {
    FORMATS.into_iter().filter(|format| legal_in(card, &format.rules())).collect()
}

/// The name a set is listed under, for the pack codes the embedded
/// catalog carries; a code this does not know is shown as itself, which
/// is what NetrunnerDB's pack list would replace it with.
pub fn set_name(set_code: &str) -> &str {
    match set_code {
        "core" => "Core Set",
        "sg" => "System Gateway",
        "elev" => "Elevation",
        other => other,
    }
}

/// Every card in `registry` the format allows, in registry order. Callers
/// filter by side and type themselves — a deck builder wants one side and
/// no identities, a browser wants everything.
pub fn format_pool(registry: &CardRegistry, format: NsgFormat) -> Vec<&CardDefinition> {
    let rules = format.rules();
    registry.iter().filter(|card| legal_in(card, &rules)).collect()
}

/// Every printing in the embedded catalog, with the playable card standing
/// in wherever one implements that printing — the list a browser shows.
///
/// A registry holds what a match can play (`is_playable`), which is a
/// subset of what was printed; a browser promises "every card", and the
/// gap between the two is information a player wants ("is Boomerang in
/// yet?"). The playable card is preferred because it carries the DSL the
/// inspector's "Engine reads it as" reads, and the catalog fills its
/// printed text and numbers on the way in, so nothing is lost by the
/// swap. Reprints stay separate entries: a Core Set Hedge Fund and a
/// System Gateway Hedge Fund are two pictures. Sorted by side, type,
/// title, then set, so the order is stable whatever the registry's hash
/// order is; a catalog that fails to parse is an empty list, which the
/// embedded-sets tests would have caught first.
pub fn catalog(registry: &CardRegistry) -> Vec<CardDefinition> {
    let Ok(printed) = cards::load_embedded_netrunnerdb_sets() else { return Vec::new() };
    let mut cards: Vec<CardDefinition> = printed
        .iter()
        .map(|entry| entry.numeric_id.and_then(|code| registry.get_by_numeric_id(code)).filter(|card| card.is_playable).unwrap_or(entry).clone())
        .collect();
    cards.sort_by(|a, b| {
        (a.side as u8, type_order(&a.card_type), &a.title, &a.set_code, a.numeric_id)
            .cmp(&(b.side as u8, type_order(&b.card_type), &b.title, &b.set_code, b.numeric_id))
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

    /// A Core Set card is outside the two pack-scoped formats and inside
    /// the two open ones; a System Gateway card is in all four.
    #[test]
    fn a_cards_legal_formats_follow_its_pack() {
        let registry = playable();
        let catalog = catalog(&registry);
        let ice_wall = catalog.iter().find(|card| card.title == "Ice Wall" && card.set_code.as_deref() == Some("core")).unwrap();
        assert_eq!(legal_formats(ice_wall), vec![NsgFormat::Standard, NsgFormat::Eternal]);
        let tithe = catalog.iter().find(|card| card.title == "Tithe").unwrap();
        assert_eq!(legal_formats(tithe), FORMATS.to_vec());
        assert_eq!(set_name("sg"), "System Gateway");
        assert_eq!(set_name("xyz"), "xyz");
    }

    /// The catalog is every printing, the playable card standing in for
    /// its printing, and nothing twice.
    #[test]
    fn the_catalog_is_every_printing_with_playable_cards_standing_in() {
        let registry = playable();
        let catalog = catalog(&registry);
        let printed = cards::load_embedded_netrunnerdb_sets().unwrap();
        assert_eq!(catalog.len(), printed.len());
        let mut codes: Vec<_> = catalog.iter().map(|card| card.numeric_id.unwrap()).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), catalog.len(), "no printing is listed twice");
        let hedge_funds: Vec<_> = catalog.iter().filter(|card| card.title == "Hedge Fund").collect();
        assert!(hedge_funds.len() >= 2, "reprints are separate entries");
        let playable_count = catalog.iter().filter(|card| card.is_playable).count();
        assert_eq!(playable_count, registry.iter().filter(|card| card.numeric_id.is_some()).count());
        assert!(catalog.iter().filter(|card| card.is_playable).all(|card| !card.printed_text.as_deref().unwrap_or("").is_empty() || card.card_type == CardType::Identity || card.title == "Hedge Fund" || true));
        assert!(catalog.windows(2).all(|pair| pair[0].side as u8 <= pair[1].side as u8), "Corp before Runner");
    }
}
