//! The deck a person brings to a game online, shared by both clients'
//! Play Online screens.
//!
//! **A player brings their own deck** (`ClientMessage::Connect::deck`),
//! and its side is their seat; the host checks it against its format and
//! refuses an illegal one at the door. **There is no "let the host deal"**
//! (Phase 4 §7 stage 3, 26 September 2026): the game is about decks built
//! to surprise, and a server deals nobody a deck unless its operator says
//! so (`ServeOptions::deals`). Everyone has the built-in decks to bring,
//! which are listed first.

use std::path::Path;

use netrunner_core::cards::CardRegistry;
use netrunner_core::decks::DeckFile;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

use crate::deck_store;

/// How a deck is offered: its side, which is the seat it takes, and its
/// name.
pub fn label(deck: &DeckFile) -> String {
    format!("{:?} · {}", deck.side, deck.name)
}

/// Every deck legal in `format` — the built-in ones and the player's own —
/// Corp decks first and each side by name. The host has the last word on
/// its own format, but a deck illegal here is not worth offering. A saved
/// deck that cannot be read is left out rather than hiding the rest
/// (`deck_store::list_lenient`), so the built-in decks are always offered.
pub fn deck_choices(decks_dir: &Path, registry: &CardRegistry, format: NsgFormat) -> Vec<DeckFile> {
    let (stored, _problems) = deck_store::list_lenient(decks_dir);
    let mut decks: Vec<DeckFile> = stored.into_iter().map(|stored| stored.deck).filter(|deck| deck.validate(registry, format).is_ok()).collect();
    decks.sort_by_key(|deck| (deck.side == Side::Runner, deck.name.to_lowercase()));
    decks
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every choice is a deck — there is no deal to ask for — Corp first,
    /// and the built-in decks are there with nothing saved.
    #[test]
    fn the_choices_are_every_legal_deck_corp_first() {
        let dir = std::env::temp_dir().join(format!("netrunner_online_choices_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let decks = deck_choices(&dir, &crate::decks::sample_deck_registry(), NsgFormat::Startup);
        let brick = decks.iter().find(|deck| deck.id == "brick_stack").expect("the built-in decks are listed");
        assert_eq!(brick.side, Side::Corp, "a deck's side is the seat");
        let first_runner = decks.iter().position(|deck| deck.side == Side::Runner).expect("Runner decks too");
        assert!(decks[..first_runner].iter().all(|deck| deck.side == Side::Corp) && decks[first_runner..].iter().all(|deck| deck.side == Side::Runner));
        assert_eq!(label(brick), format!("Corp · {}", brick.name));
    }
}
