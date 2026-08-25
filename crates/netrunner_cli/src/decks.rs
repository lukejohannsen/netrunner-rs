//! Builds a legal 45-card Kate "Mac" McCaffrey (Runner) vs. Haas-Bioroid:
//! Engineering the Future (Corp) matchup for headless self-play and the
//! interactive TUI's single-game bootstrap.
//!
//! `netrunner_core::cards::register_baseline_set` alone can't reach a legal
//! deck for either side: the baseline pool has only 7 Corp cards (max 21
//! copies, and only `hostile_takeover` is an Agenda — max 3 agenda points)
//! and 6 Runner cards (max 18 copies), while `validate_deck` requires 45
//! non-identity cards for both `haas_bioroid_engineering_the_future`/
//! `kate_mccaffrey` (their `min_deck_size`) plus 20-22 agenda points for the
//! Corp side. This module layers synthetic filler cards on top of the
//! baseline set to close that gap, without touching `netrunner_core` itself.

use netrunner_core::cards::{self, CardRegistry};
use netrunner_core::dsl::{Card, CardId, CardType};
use netrunner_core::rules::{Deck, Side};

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

fn blank_card(id: String, side: Side, card_type: CardType) -> Card {
    Card {
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
    cards::register_baseline_set(&mut registry);

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

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{validate_deck, GameState};

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
