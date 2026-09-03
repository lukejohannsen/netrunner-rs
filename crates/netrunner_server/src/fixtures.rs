//! Builds a legal 45-card Kate "Mac" McCaffrey (Runner) vs. Haas-Bioroid:
//! Engineering the Future (Corp) matchup, for this crate's headless
//! bot-vs-bot driver and its own tests.
//!
//! Deliberately a near-identical copy of `netrunner_cli::decks` rather than
//! a shared dependency: `netrunner_cli` is a binary crate (its `decks`
//! module isn't importable from here), and this fixture is small enough
//! (~100 lines) that inventing a shared crate just to deduplicate it isn't
//! worth the indirection. See `netrunner_cli::decks`'s own doc comment for
//! why the baseline card pool alone isn't a legal deck for either side.

use netrunner_core::cards::{self, CardRegistry};
use netrunner_core::dsl::{CardDefinition, CardId, CardType};
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
        counter_kind: None, numeric_id: None, faction: None, type_line: None, keywords: Vec::new(), set_code: None, influence_cost: None, deck_limit: None, unlimited_influence: false, artist: None, image_url: None, memory_bonus: None, max_hand_size_bonus: None, install_cost_discount_if: None,
        install_cost_discount_amount: None,
        additional_play_cost: None,
        host_ice_gains_subtypes: Vec::new(),
        hosted_breaker_bonus: None,
        hosted_credits_usable_for: None,
        trash_when_empty: false,
        influence_limit: None, installs_on_ice: false, hosted_cards_playable_from_grip: false, ice_rez_cost_modifier: 0, dividends: None, playable_from_archives: false, click_breakable: false, strength_modifier: None, persistent_after_trash: false, root_asset_trash_cost_bonus: 0, unique: false, base_link: None, is_playable: true,
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
