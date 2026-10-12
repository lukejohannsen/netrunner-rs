//! The deck a person brings to a game online, shared by both clients'
//! Play Online screens.
//!
//! **A player brings their own deck** (`Chair`, on the seek), and its
//! side is their seat; the host checks it against its format and
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
use netrunner_protocol::{Level, LobbyInfo, QueueMode};

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

/// A lobby as a line names it (Phase 4 §9): the server's queues by their
/// mode and format — "Ranked · Startup", "Casual · Standard" — and a
/// friend queue by the name its maker gave it and its format.
pub fn lobby_title(lobby: &LobbyInfo) -> String {
    let format = crate::settings::format_label(lobby.format);
    match lobby.mode {
        QueueMode::Ranked | QueueMode::Casual => format!("{} · {format}", lobby.mode.name()),
        QueueMode::Friends => format!("{} · {format}", lobby.name),
    }
}

/// Whom a seek is against, in a pill's words: a person, or a bot at a
/// rung of the ladder, named as the new-game form names it.
pub fn opponent_label(bot: Option<Level>) -> String {
    match bot {
        None => "A person".to_string(),
        Some(level) => format!("{} bot", rung_label(level)),
    }
}

/// A rung of the ladder by its name, capitalised: a pill beside "A person"
/// in a row that says it is choosing an opponent.
pub fn rung_label(level: Level) -> String {
    let mut letters = level.name().chars();
    letters.next().map_or(String::new(), |first| first.to_uppercase().chain(letters).collect())
}

/// A side preferred by a seek that brought both decks, in a pill's words.
pub fn preference_label(prefer: Option<Side>) -> &'static str {
    match prefer {
        None => "No preference",
        Some(Side::Corp) => "Prefer Corp",
        Some(Side::Runner) => "Prefer Runner",
    }
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

    #[test]
    fn a_lobby_is_named_by_its_mode_or_its_maker() {
        let lobby = |id: &str, mode| LobbyInfo { id: id.into(), name: "Friday".into(), format: NsgFormat::Startup, mode, password: false, rated: false, bots: Vec::new(), players: 0, seeking: 0 };
        assert_eq!(lobby_title(&lobby("ranked-startup", QueueMode::Ranked)), "Ranked · Startup");
        assert_eq!(lobby_title(&lobby("casual-startup", QueueMode::Casual)), "Casual · Startup");
        assert_eq!(lobby_title(&lobby("K7M2QX", QueueMode::Friends)), "Friday · Startup");
        assert_eq!(opponent_label(None), "A person");
        assert_eq!(opponent_label(Some(Level::Veteran)), "Veteran bot");
    }
}
