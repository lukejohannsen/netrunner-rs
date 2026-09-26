//! What a person chooses to play a game online, shared by both clients'
//! Play Online screens: the deck they bring or the host's deal.
//!
//! **A player brings their own deck** (`ClientMessage::Connect::deck`),
//! and its side is their seat; the host checks it against its format and
//! refuses an illegal one at the door. "Let the host deal" is offered too,
//! for either side or a preferred one — what `--mode remote` always did.

use std::path::Path;

use netrunner_core::cards::CardRegistry;
use netrunner_core::decks::DeckFile;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

use crate::deck_store;

/// A deck of the player's, or the host's deal.
#[derive(Debug, Clone, PartialEq)]
pub enum DeckChoice {
    /// Let the host deal, preferring this side (or neither).
    Dealt(Option<Side>),
    Brought(Box<DeckFile>),
}

impl DeckChoice {
    pub fn label(&self) -> String {
        match self {
            DeckChoice::Dealt(None) => "Let the host deal me a deck — either side".to_string(),
            DeckChoice::Dealt(Some(side)) => format!("Let the host deal me a deck — as the {side:?}"),
            DeckChoice::Brought(deck) => format!("{:?} · {}", deck.side, deck.name),
        }
    }

    /// The seat asked for: a brought deck's side, or the preference.
    pub fn side(&self) -> Option<Side> {
        match self {
            DeckChoice::Dealt(side) => *side,
            DeckChoice::Brought(deck) => Some(deck.side),
        }
    }

    pub fn deck(&self) -> Option<DeckFile> {
        match self {
            DeckChoice::Dealt(_) => None,
            DeckChoice::Brought(deck) => Some((**deck).clone()),
        }
    }
}

/// The host's three deals, then every saved deck legal in `format`,
/// Corp decks first and each side by name. The host has the last word on
/// its own format, but a deck illegal here is not worth offering.
pub fn deck_choices(decks_dir: &Path, registry: &CardRegistry, format: NsgFormat) -> Result<Vec<DeckChoice>, String> {
    let mut decks = vec![DeckChoice::Dealt(None), DeckChoice::Dealt(Some(Side::Corp)), DeckChoice::Dealt(Some(Side::Runner))];
    let mut owned: Vec<DeckFile> =
        deck_store::list(decks_dir)?.into_iter().map(|stored| stored.deck).filter(|deck| deck.validate(registry, format).is_ok()).collect();
    owned.sort_by_key(|deck| (deck.side == Side::Runner, deck.name.to_lowercase()));
    decks.extend(owned.into_iter().map(|deck| DeckChoice::Brought(Box::new(deck))));
    Ok(decks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choices_are_the_hosts_deal_then_every_legal_deck() {
        let dir = std::env::temp_dir().join(format!("netrunner_online_choices_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let decks = deck_choices(&dir, &crate::decks::sample_deck_registry(), NsgFormat::Startup).unwrap();
        assert_eq!(decks[0], DeckChoice::Dealt(None));
        assert_eq!(DeckChoice::Dealt(Some(Side::Runner)).side(), Some(Side::Runner));
        let brick = decks.iter().find(|choice| choice.deck().is_some_and(|deck| deck.id == "brick_stack")).expect("the built-in decks are listed");
        assert_eq!(brick.side(), Some(Side::Corp), "a deck's side is the seat");
    }
}
