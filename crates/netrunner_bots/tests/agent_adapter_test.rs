//! Integration tests for `netrunner_bots::agent_adapter`: every index-based
//! `Agent` (`IndexedRandomAgent`/`IndexedHeuristicAgent`, and
//! `IndexedOnnxAgent` under the `onnx` feature) must always select an index
//! `netrunner_core::rules::get_action_mask` marks legal.

use netrunner_bots::{Agent, BotAgentIndexAdapter, HeuristicAgent, IndexedHeuristicAgent, RandomAgent};
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardDefinition, CardId, CardType};
use netrunner_core::rules::{
    get_action_mask, AgendaPoints, Clicks, CorpState, Credits, GamePhase, GameState, MemoryUnits, PlayerResources,
    RunnerState, Side,
};

fn blank_card(id: &str, side: Side, card_type: CardType, cost: u32) -> CardDefinition {
    CardDefinition {
        id: CardId(id.to_string()),
        title: id.to_string(),
        side,
        card_type,
        cost,
        is_playable: true,
        ..Default::default()
    }
}

fn empty_runner() -> RunnerState {
    RunnerState {
        resources: PlayerResources { credits: Credits(0), clicks: Clicks(0), agenda_points: AgendaPoints(0) },
        memory_units: MemoryUnits(0),
        ..Default::default()
    }
}

/// A Corp click phase with a mix of hand cards (an Operation and an Asset)
/// giving several distinct legal action kinds.
fn sample_corp_turn_state() -> (CardRegistry, GameState) {
    let mut registry = CardRegistry::new();
    registry.insert(blank_card("hedge_fund", Side::Corp, CardType::Operation, 5));
    registry.insert(blank_card("pad_campaign", Side::Corp, CardType::Asset, 2));

    let mut state = GameState::new(0);
    state.phase = GamePhase::Action(Side::Corp);
    state.runner = empty_runner();
    state.corp = CorpState {
        resources: PlayerResources { credits: Credits(10), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
        hq: vec![CardId("hedge_fund".to_string()), CardId("pad_campaign".to_string())],
        ..Default::default()
    };
    (registry, state)
}

#[test]
fn indexed_random_agent_always_selects_a_legal_index() {
    let (registry, state) = sample_corp_turn_state();
    let mask = get_action_mask(&state, &registry);

    for seed in 0..20 {
        let mut agent = BotAgentIndexAdapter::new(RandomAgent::new(seed), Side::Corp);
        let index = agent.select_action(&state, &registry, &mask);
        assert!(mask[index], "seed {seed} selected illegal index {index}");
    }
}

#[test]
fn indexed_heuristic_agent_always_selects_a_legal_index() {
    let (registry, state) = sample_corp_turn_state();
    let mask = get_action_mask(&state, &registry);

    for seed in 0..20 {
        let mut agent: IndexedHeuristicAgent = BotAgentIndexAdapter::new(HeuristicAgent::new(Side::Corp, seed), Side::Corp);
        let index = agent.select_action(&state, &registry, &mask);
        assert!(mask[index], "seed {seed} selected illegal index {index}");
    }
}

#[test]
fn to_observation_vector_matches_obs_size() {
    let (registry, state) = sample_corp_turn_state();

    assert_eq!(netrunner_bots::to_observation_vector(&state, &registry, Side::Corp).len(), netrunner_bots::OBS_SIZE);
    assert_eq!(netrunner_bots::to_observation_vector(&state, &registry, Side::Runner).len(), netrunner_bots::OBS_SIZE);
}

#[cfg(feature = "onnx")]
#[test]
fn indexed_onnx_agent_always_selects_a_legal_index() {
    use netrunner_bots::{IndexedOnnxAgent, OnnxPolicyEvaluator};
    use netrunner_bots::onnx_fixture::write_fixture_model;

    let model_file = write_fixture_model();
    let evaluator = OnnxPolicyEvaluator::new(model_file.path.to_str().unwrap(), Side::Corp)
        .expect("hand-built fixture model should load successfully");

    let (registry, state) = sample_corp_turn_state();
    let mask = get_action_mask(&state, &registry);

    let mut agent = IndexedOnnxAgent::new(evaluator);
    let index = agent.select_action(&state, &registry, &mask);
    assert!(mask[index], "selected illegal index {index}");
}
