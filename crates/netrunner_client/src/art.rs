//! Which printing's picture a card shows.
//!
//! A card is one card however many sets print it (NetrunnerDB v3 splits a
//! card from its printings, and so does `netrunner_core::cards::catalog`),
//! but a picture belongs to a printing: Hedge Fund has one scan from the
//! Core Set and another from System Gateway. Every place a client draws a
//! card's face, its scan, an identity's avatar or the text face's flavour
//! asks this module which printing that is, and nothing else does — so the
//! choice a person makes later (Phase 7 §10, art per printing) changes one
//! function rather than thirty call sites.
//!
//! **Today it is the newest embedded printing**, the order v3 lists a
//! card's `printing_ids` in. Until the catalog moved to v3 (NSG pool Stage
//! 0d) a card showed the printing its card file named, `numeric_id`, which
//! was the oldest for the twelve Core Set cards later reprinted — so those
//! show their newer scan now.

use netrunner_core::card::PrintingId;
use netrunner_core::cards::catalog::{self, Printing};
use netrunner_core::dsl::CardDefinition;

/// The printing `card` is drawn as: its newest embedded printing, or for a
/// card the catalog does not carry (homebrew, a test fixture) the printing
/// its file was built from, if it names one.
pub fn printing_for(card: &CardDefinition) -> Option<PrintingId> {
    catalog::latest_printing(&card.id).map(|printing| printing.id).or(card.built_from)
}

/// The catalog's record of the printing `card` is drawn as — its set, its
/// illustrator and its flavour.
pub fn printing_record(card: &CardDefinition) -> Option<&'static Printing> {
    printing_for(card).and_then(catalog::printing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::CardId;

    fn card(id: &str) -> CardDefinition {
        CardDefinition { id: CardId(id.to_string()), ..CardDefinition::default() }
    }

    /// Hedge Fund is drawn as System Gateway's printing, the newer of its
    /// two; a card the catalog does not know falls back to its own file's
    /// printing, and to none.
    #[test]
    fn a_card_is_drawn_as_its_newest_printing() {
        assert_eq!(printing_for(&card("hedge_fund")), Some(PrintingId(30075)));
        assert_eq!(printing_record(&card("hedge_fund")).map(|printing| printing.set.as_str()), Some("system_gateway"));
        let homebrew = CardDefinition { built_from: Some(PrintingId(99_001)), ..card("homebrew") };
        assert_eq!(printing_for(&homebrew), Some(PrintingId(99_001)));
        assert_eq!(printing_for(&card("homebrew")), None);
    }
}
