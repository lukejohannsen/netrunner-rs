//! Builds a legal 45-card Kate "Mac" McCaffrey (Runner) vs. Haas-Bioroid:
//! Engineering the Future (Corp) matchup for headless self-play and the
//! interactive TUI's single-game bootstrap.
//!
//! `netrunner_core::cards::register_playable_cards` alone can't reach a legal
//! deck for either side: the baseline pool has only 7 Corp cards (max 21
//! copies, and only `hostile_takeover` is an Agenda — max 3 agenda points)
//! and 6 Runner cards (max 18 copies), while `validate_deck` requires 45
//! non-identity cards for both `haas_bioroid_engineering_the_future`/
//! `kate_mccaffrey` (their `min_deck_size`) plus 20-22 agenda points for the
//! Corp side. This module layers synthetic filler cards on top of the
//! baseline set to close that gap, without touching `netrunner_core` itself.

use std::path::Path;

use netrunner_core::cards::{self, CardRegistry};
use netrunner_core::dsl::{CardDefinition, CardId, CardType};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::{Deck, Side};

use crate::deck_store;

const CORP_IDENTITY: &str = "haas_bioroid_engineering_the_future";
const RUNNER_IDENTITY: &str = "kate_mccaffrey";

const BASELINE_CORP_CARDS: [&str; 7] =
    ["hedge_fund", "scorched_earth", "hostile_takeover", "pad_campaign", "snare", "enigma", "wall_of_static"];
const BASELINE_RUNNER_CARDS: [&str; 6] =
    ["sure_gamble", "diesel", "the_makers_eye", "account_siphon", "corroder", "gordian_blade"];

/// 6 distinct 1-point filler Agendas, 3 copies each: 18 cards / 18 agenda
/// points, on top of baseline's 21 cards / 3 points (`hostile_takeover` x3)
/// — 39 cards / 21 points so far.
const FILLER_AGENDA_COUNT: u32 = 6;
/// 2 distinct 0-cost filler Assets, 3 copies each: 6 more cards / 0 points —
/// 45 cards / 21 points total, satisfying `validate_deck`'s 45-card /
/// 20-22-point requirement for a 45-`min_deck_size` Corp identity.
const FILLER_ASSET_COUNT: u32 = 2;
/// 9 distinct filler Events, 3 copies each: 27 cards, on top of baseline's
/// 18 — 45 cards total, satisfying the 45-card Runner requirement.
const FILLER_EVENT_COUNT: u32 = 9;

fn blank_card(id: String, side: Side, card_type: CardType) -> CardDefinition {
    CardDefinition {
        title: id.clone(),
        id: CardId(id),
        side,
        card_type,
        cost: 0,
        triggers: Vec::new(),
        abilities: Vec::new(),
        trash_cost: None,
        steal_cost: None,
        advancement_requirement: None,
        agenda_points: None,
        min_deck_size: None,
        strength: None,
        subroutines: Vec::new(),
        interactive_on_access: None,
        subtypes: Vec::new(),
        play_requirement: None,
        recurring_credits: None,
        first_install_discount: None,
        memory_cost: None,
        counter_kind: None, numeric_id: None, faction: None, type_line: None, keywords: Vec::new(), set_code: None, influence_cost: None, deck_limit: None, artist: None, image_url: None, memory_bonus: None, max_hand_size_bonus: None, install_cost_discount_if: None, installs_on_ice: false, click_breakable: false, strength_modifier: None, persistent_after_trash: false, is_playable: true,
    }
}

fn filler_agenda_id(index: u32) -> String {
    format!("filler_agenda_{index}")
}

fn filler_asset_id(index: u32) -> String {
    format!("filler_asset_{index}")
}

fn filler_event_id(index: u32) -> String {
    format!("filler_event_{index}")
}

/// A populated registry: the full baseline Core Set suite plus this
/// module's filler cards.
pub fn kate_vs_hb_registry() -> CardRegistry {
    let mut registry = CardRegistry::new();
    cards::register_playable_cards(&mut registry);

    for index in 0..FILLER_AGENDA_COUNT {
        let mut agenda = blank_card(filler_agenda_id(index), Side::Corp, CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(1);
        registry.insert(agenda);
    }
    for index in 0..FILLER_ASSET_COUNT {
        registry.insert(blank_card(filler_asset_id(index), Side::Corp, CardType::Asset));
    }
    for index in 0..FILLER_EVENT_COUNT {
        registry.insert(blank_card(filler_event_id(index), Side::Runner, CardType::Event));
    }

    registry
}

/// `(corp_deck, runner_deck)` — each a legal 45-card deck against
/// `kate_vs_hb_registry()`, per `validate_deck`.
pub fn kate_vs_hb_decks() -> (Deck, Deck) {
    let mut corp_cards: Vec<(CardId, u32)> =
        BASELINE_CORP_CARDS.into_iter().map(|id| (CardId(id.to_string()), 3)).collect();
    corp_cards.extend((0..FILLER_AGENDA_COUNT).map(|index| (CardId(filler_agenda_id(index)), 3)));
    corp_cards.extend((0..FILLER_ASSET_COUNT).map(|index| (CardId(filler_asset_id(index)), 3)));

    let mut runner_cards: Vec<(CardId, u32)> =
        BASELINE_RUNNER_CARDS.into_iter().map(|id| (CardId(id.to_string()), 3)).collect();
    runner_cards.extend((0..FILLER_EVENT_COUNT).map(|index| (CardId(filler_event_id(index)), 3)));

    let corp_deck = Deck { identity: CardId(CORP_IDENTITY.to_string()), cards: corp_cards };
    let runner_deck = Deck { identity: CardId(RUNNER_IDENTITY.to_string()), cards: runner_cards };
    (corp_deck, runner_deck)
}

/// The card pool for local play against the published sample decks: every
/// implemented card, with no synthetic filler.
///
/// Unlike [`kate_vs_hb_registry`], this needs no filler at all — the
/// System Gateway pool is large enough to build real, legal decks from.
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

    #[test]
    fn decks_are_individually_legal() {
        let registry = kate_vs_hb_registry();
        let (corp_deck, runner_deck) = kate_vs_hb_decks();

        assert_eq!(validate_deck(&corp_deck, Side::Corp, &registry), Ok(()));
        assert_eq!(validate_deck(&runner_deck, Side::Runner, &registry), Ok(()));
    }

    #[test]
    fn decks_pass_full_game_setup() {
        let registry = kate_vs_hb_registry();
        let (corp_deck, runner_deck) = kate_vs_hb_decks();

        assert!(GameState::setup(&corp_deck, &runner_deck, &registry, 42).is_ok());
    }
}
