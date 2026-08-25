//! Builds a legal 45-card Kate "Mac" McCaffrey (Runner) vs. Haas-Bioroid:
//! Engineering the Future (Corp) matchup for `NetrunnerEnv`.
//!
//! Deliberately a near-identical copy of `netrunner_server::fixtures`
//! (itself a documented copy of `netrunner_cli::decks`) rather than a
//! shared dependency — see that module's own doc comment for why each
//! crate needing a real playable matchup keeps its own small copy instead
//! of inventing a shared crate for ~100 lines.

use netrunner_core::cards::{self, CardRegistry};
use netrunner_core::dsl::{Card, CardId, CardType};
use netrunner_core::rules::{Deck, Side};

const CORP_IDENTITY: &str = "haas_bioroid_engineering_the_future";
const RUNNER_IDENTITY: &str = "kate_mccaffrey";

const BASELINE_CORP_CARDS: [&str; 7] =
    ["hedge_fund", "scorched_earth", "hostile_takeover", "pad_campaign", "snare", "enigma", "wall_of_static"];
const BASELINE_RUNNER_CARDS: [&str; 6] =
    ["sure_gamble", "diesel", "the_makers_eye", "account_siphon", "corroder", "gordian_blade"];

const FILLER_AGENDA_COUNT: u32 = 6;
const FILLER_ASSET_COUNT: u32 = 2;
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
