//! The card pools and decks a local match is played with.
//!
//! [`sample_deck_registry`] + [`decks_for_match`] is the whole of it:
//! every implemented card, and Null Signal Games' published sample decks
//! (or a saved/custom deck) validated before play. There used to be a
//! second registry here, `kate_vs_hb_registry`, kept only for `--mode
//! remote`: the wire protocol transmits no `CardRegistry`, so the remote
//! client builds its own to resolve card titles, and while the daemon
//! dealt a filler-padded fixture this module had to mirror
//! `netrunner_server::fixtures`' synthetic card ids constant for
//! constant. The daemon now deals sample decks, so every card it can
//! deal is in `register_playable_cards` and the two sides agree without
//! anything to keep in step.

use std::path::Path;

use netrunner_core::cards::{self, CardRegistry};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::{Deck, Side};

use crate::deck_store;

/// The card pool for every match this client plays, local or remote.
///
/// No synthetic filler: the published pool is large enough to build real,
/// legal decks from, which is why the registry the remote client needs
/// and the registry local play needs are now the same one.
pub fn sample_deck_registry() -> CardRegistry {
    let mut registry = CardRegistry::new();
    cards::register_playable_cards(&mut registry);
    registry
}

/// Resolves the Corp and Runner decks a match will be played with, checking
/// each against both validators before handing it to `GameState::setup`.
///
/// **Validation is a hard failure, deliberately.** `GameState::setup` would
/// catch an unplayable deck by itself, but not an illegal one — a deck two
/// influence over the limit sets up and plays perfectly well. Refusing here
/// means "this deck is legal" has one meaning whether it is checked by
/// `deck validate` or by starting a game.
///
/// `dir` is the saved-deck directory; names resolve through
/// `deck_store::load_for_side`, so a built-in id, a saved deck name and a
/// path all work.
pub fn decks_for_match(
    dir: &Path,
    corp_name: &str,
    runner_name: &str,
    registry: &CardRegistry,
    format: NsgFormat,
) -> Result<(Deck, Deck), String> {
    let find = |name: &str, side: Side| -> Result<Deck, String> {
        let stored = deck_store::load_for_side(dir, name, side)?;
        stored
            .deck
            .validate(registry, format)
            .map_err(|e| format!("deck {:?} cannot be played: {e}", stored.deck.id))?;
        Ok(stored.deck.to_deck())
    };
    Ok((find(corp_name, Side::Corp)?, find(runner_name, Side::Runner)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::CardId;
    use netrunner_core::rules::{validate_deck, GameState};

    /// An empty directory that does not exist, so these tests resolve only
    /// built-in decks and never depend on what the developer has saved.
    fn no_saved_decks() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("netrunner_cli_no_decks_{}", std::process::id()))
    }

    fn resolve(corp: &str, runner: &str) -> Result<(Deck, Deck), String> {
        let registry = sample_deck_registry();
        decks_for_match(&no_saved_decks(), corp, runner, &registry, NsgFormat::Startup)
    }

    #[test]
    fn built_in_decks_resolve_and_are_legal() {
        let registry = sample_deck_registry();
        let (corp_deck, runner_deck) = resolve("discretion_advised", "stolen_goods").expect("both ids exist");

        assert_eq!(validate_deck(&corp_deck, Side::Corp, &registry), Ok(()));
        assert_eq!(validate_deck(&runner_deck, Side::Runner, &registry), Ok(()));
        assert!(GameState::setup(&corp_deck, &runner_deck, &registry, 5).is_ok());
    }

    #[test]
    fn the_cli_defaults_name_real_decks() {
        // Keeps `Config::corp_deck`/`runner_deck`'s defaults honest — a
        // renamed deck file would otherwise break the CLI's no-flag path.
        assert!(resolve("discretion_advised", "stolen_goods").is_ok());
    }

    #[test]
    fn an_unknown_deck_id_lists_the_real_ones() {
        let error = resolve("not_a_deck", "stolen_goods").expect_err("unknown id");
        assert!(error.contains("discretion_advised"), "error should list available decks: {error}");
    }

    #[test]
    fn asking_for_a_runner_deck_in_the_corp_slot_is_rejected() {
        let error = resolve("stolen_goods", "stolen_goods").expect_err("wrong side");
        assert!(error.contains("not Corp"), "{error}");
    }

    /// The whole point of the deck store: a deck saved to disk plays exactly
    /// like a built-in one, through the same flag.
    #[test]
    fn a_saved_deck_can_be_played() {
        let dir = std::env::temp_dir().join(format!(
            "netrunner_cli_saved_deck_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let mut saved = netrunner_core::decks::by_id("discretion_advised").expect("built-in deck");
        saved.id = "my_corp_deck".to_string();
        saved.category = netrunner_core::decks::DeckCategory::Custom;
        std::fs::write(dir.join("my_corp_deck.json"), saved.to_json().expect("serializes")).expect("write");

        let registry = sample_deck_registry();
        let resolved = decks_for_match(&dir, "my_corp_deck", "stolen_goods", &registry, NsgFormat::Startup);
        let _ = std::fs::remove_dir_all(&dir);

        let (corp_deck, runner_deck) = resolved.expect("a saved deck resolves like a built-in one");
        assert!(GameState::setup(&corp_deck, &runner_deck, &registry, 7).is_ok());
    }

    /// An illegal deck is refused *before* a match starts, not silently
    /// played. `GameState::setup` would not catch this on its own: an
    /// over-influence deck sets up and plays perfectly well.
    #[test]
    fn an_illegal_saved_deck_is_refused_at_match_start() {
        let dir = std::env::temp_dir().join(format!(
            "netrunner_cli_illegal_deck_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        // Swapped, not appended, so the deck size and agenda points stay
        // legal and the *format* check is what fails: Ice Wall is Core Set,
        // and Startup's pool is System Gateway plus Elevation.
        let mut illegal = netrunner_core::decks::by_id("discretion_advised").expect("built-in deck");
        illegal.id = "illegal_deck".to_string();
        illegal.cards[0].card = CardId("ice_wall".to_string());
        std::fs::write(dir.join("illegal_deck.json"), illegal.to_json().expect("serializes")).expect("write");

        let registry = sample_deck_registry();
        let resolved = decks_for_match(&dir, "illegal_deck", "stolen_goods", &registry, NsgFormat::Startup);
        let _ = std::fs::remove_dir_all(&dir);

        let error = resolved.expect_err("a Startup-illegal deck must not start a match");
        assert!(error.contains("illegal_deck"), "the error should name the deck: {error}");
    }
}
