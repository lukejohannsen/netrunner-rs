//! Which picture a card shows: the printing a person chose, or its newest.
//!
//! A card is one card however many sets print it (NetrunnerDB v3 splits a
//! card from its printings, and so does `netrunner_core::cards::catalog`),
//! but a picture belongs to a printing: Hedge Fund has one scan from the
//! Core Set and another from System Gateway. Every place a client draws a
//! card's face, its scan, an identity's avatar or the text face's flavour
//! asks this module which picture that is, and nothing else does, so a
//! choice made in the card browser reaches the board, the deck builder
//! and every sheet without one of them knowing (Phase 7 §10, Stage 3).
//!
//! **A choice is the person's, kept in the settings file, and never sent.**
//! It is a client preference like the card backs: no server sees it, no
//! `ClientView` carries it, and the other chair draws its own choices. A
//! facedown card is a back, as before. It sits at the top of the settings
//! file rather than among the desktop's preferences because it is about a
//! card, not a toolkit: the terminal draws no picture and shows the
//! newest printing's flavour (`ArtChoices::NONE`), and a later client that
//! draws pictures reads the same list.
//!
//! **What is chosen and what is drawn are two types** — [`Art`], the
//! choice as the file stores it, and [`Picture`], what an image cache is
//! keyed by — so that a person's own art (a later feature, the person's
//! request of 30 September 2026) is one variant on each: `Art::Custom`
//! naming a file in their data folder, and `Picture::File` its path,
//! which a cache loads like any scan. Nothing else here has to move for
//! it. A choice the catalog cannot honour — a printing that is not this
//! card's, from a hand-edited file or a catalog that dropped a set — is
//! passed over for the newest printing rather than drawing the wrong card.
//!
//! **Without a choice it is the newest embedded printing**, the order v3
//! lists a card's `printing_ids` in. Until the catalog moved to v3 (NSG
//! pool Stage 0d) a card showed the printing its card file named, which
//! was the oldest for the twelve Core Set cards later reprinted.

use serde::{Deserialize, Serialize};

use netrunner_core::card::PrintingId;
use netrunner_core::cards::catalog::{self, Printing};
use netrunner_core::dsl::{CardDefinition, CardId};

/// What a person chose for a card's picture, as the settings file keeps
/// it: `{"printing": 1110}`.
///
/// The next variant is `Custom(String)`, a file name in
/// `<data dir>/netrunner/art/<card id>/` — the player's own folder, which
/// is credited nowhere — and it is deliberately not here until the
/// feature that imports one is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Art {
    Printing(PrintingId),
}

/// One card's chosen art.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtChoice {
    pub card: CardId,
    pub art: Art,
}

/// Every card's chosen art. A list rather than a map for the reason
/// `standing::Answers` is one: the file stays readable as a list of
/// entries, and a new kind of key needs no string encoding.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArtChoices(pub Vec<ArtChoice>);

impl ArtChoices {
    /// No choices: every card its newest printing. What a client that
    /// draws no picture passes.
    pub const NONE: ArtChoices = ArtChoices(Vec::new());

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, card: &CardId) -> Option<&Art> {
        self.0.iter().find(|entry| entry.card == *card).map(|entry| &entry.art)
    }

    /// Choose `art` for `card`, or with `None` go back to the newest
    /// printing.
    pub fn set(&mut self, card: CardId, art: Option<Art>) {
        self.0.retain(|entry| entry.card != card);
        if let Some(art) = art {
            self.0.push(ArtChoice { card, art });
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &ArtChoice> {
        self.0.iter()
    }
}

/// What an image cache is keyed by: the picture itself, never the card,
/// so that changing a choice asks for a new picture and an old one is
/// never drawn in its place. `Clone` and not `Copy` on purpose: its next
/// variant, `File(PathBuf)`, is a path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Picture {
    /// A printing's scan, from NetrunnerDB through the image cache.
    Printing(PrintingId),
}

/// The printing `card` is drawn as with no choice made: its newest
/// embedded printing, or for a card the catalog does not carry (homebrew,
/// a test fixture) the printing its file was built from, if it names one.
pub fn default_printing(card: &CardDefinition) -> Option<PrintingId> {
    catalog::latest_printing(&card.id).map(|printing| printing.id).or(card.built_from)
}

/// The printing chosen for `card`, if one was and it is one of this
/// card's printings. A browser marks it; everything that draws asks
/// [`printing_for`].
pub fn chosen_printing(card: &CardDefinition, art: &ArtChoices) -> Option<PrintingId> {
    match art.get(&card.id)? {
        Art::Printing(id) => catalog::printing(*id).filter(|printing| printing.card == card.id).map(|printing| printing.id),
    }
}

/// The printing `card` is drawn as: the one chosen for it, else its
/// default.
pub fn printing_for(card: &CardDefinition, art: &ArtChoices) -> Option<PrintingId> {
    chosen_printing(card, art).or_else(|| default_printing(card))
}

/// The catalog's record of the printing `card` is drawn as — its set, its
/// illustrator and its flavour.
pub fn printing_record(card: &CardDefinition, art: &ArtChoices) -> Option<&'static Printing> {
    printing_for(card, art).and_then(catalog::printing)
}

/// The picture `card` is drawn with — the question every face, sheet and
/// avatar asks.
pub fn picture_for(card: &CardDefinition, art: &ArtChoices) -> Option<Picture> {
    printing_for(card, art).map(Picture::Printing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str) -> CardDefinition {
        CardDefinition { id: CardId(id.to_string()), ..CardDefinition::default() }
    }

    /// Hedge Fund is drawn as System Gateway's printing, the newer of its
    /// two; a card the catalog does not know falls back to its own file's
    /// printing, and to none.
    #[test]
    fn a_card_is_drawn_as_its_newest_printing() {
        assert_eq!(printing_for(&card("hedge_fund"), &ArtChoices::NONE), Some(PrintingId(30075)));
        assert_eq!(printing_record(&card("hedge_fund"), &ArtChoices::NONE).map(|printing| printing.set.as_str()), Some("system_gateway"));
        let homebrew = CardDefinition { built_from: Some(PrintingId(99_001)), ..card("homebrew") };
        assert_eq!(printing_for(&homebrew, &ArtChoices::NONE), Some(PrintingId(99_001)));
        assert_eq!(printing_for(&card("homebrew"), &ArtChoices::NONE), None);
    }

    /// A choice is honoured for its card and no other, and going back to
    /// the default forgets it.
    #[test]
    fn a_chosen_printing_is_drawn_until_it_is_cleared() {
        let hedge_fund = card("hedge_fund");
        let mut art = ArtChoices::default();
        art.set(hedge_fund.id.clone(), Some(Art::Printing(PrintingId(1110))));
        assert_eq!(picture_for(&hedge_fund, &art), Some(Picture::Printing(PrintingId(1110))));
        assert_eq!(printing_record(&hedge_fund, &art).map(|printing| printing.set.as_str()), Some("core_set"));
        assert_eq!(chosen_printing(&card("ice_wall"), &art), None, "a choice is one card's");
        art.set(hedge_fund.id.clone(), Some(Art::Printing(PrintingId(30075))));
        assert_eq!(art.0.len(), 1, "a second choice replaces the first");
        art.set(hedge_fund.id.clone(), None);
        assert!(art.is_empty());
        assert_eq!(printing_for(&hedge_fund, &art), Some(PrintingId(30075)));
    }

    /// A choice naming another card's printing, or one the catalog does
    /// not hold, is passed over: the card is drawn as its newest printing,
    /// never as somebody else's.
    #[test]
    fn a_choice_the_catalog_cannot_honour_falls_back_to_the_newest() {
        let hedge_fund = card("hedge_fund");
        let ice_wall = catalog::latest_printing(&CardId("ice_wall".to_string())).expect("Ice Wall is printed").id;
        for wrong in [ice_wall, PrintingId(99_999)] {
            let mut art = ArtChoices::default();
            art.set(hedge_fund.id.clone(), Some(Art::Printing(wrong)));
            assert_eq!(chosen_printing(&hedge_fund, &art), None);
            assert_eq!(printing_for(&hedge_fund, &art), Some(PrintingId(30075)));
        }
    }

    /// The file's shape: a list of entries, each a card and its art.
    #[test]
    fn choices_are_kept_as_a_list_of_cards_and_their_art() {
        let mut art = ArtChoices::default();
        art.set(CardId("hedge_fund".to_string()), Some(Art::Printing(PrintingId(1110))));
        let json = serde_json::to_string(&art).unwrap();
        assert_eq!(json, r#"[{"card":"hedge_fund","art":{"printing":1110}}]"#);
        assert_eq!(serde_json::from_str::<ArtChoices>(&json).unwrap(), art);
    }
}
