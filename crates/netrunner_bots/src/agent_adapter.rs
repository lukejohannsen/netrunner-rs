//! Index-based `Agent` adapters bridging this crate's existing view-based
//! `agent::BotAgent` trait (`random::RandomAgent`, `heuristic::
//! HeuristicAgent`) and the fixed `policy::PolicyEvaluator` trait
//! (`onnx_policy::OnnxPolicyEvaluator`, `onnx` feature) to a single,
//! minimal interface: pick one `0..ActionSpace::SIZE` index, guaranteed
//! legal per `netrunner_core::rules::get_action_mask`.
//!
//! These are pure plumbing, not new decision logic — `RandomAgent`/
//! `HeuristicAgent`/`OnnxPolicyEvaluator`'s actual behavior is unchanged;
//! this module only converts between their existing `ClientView`/
//! `PlayerAction`- or logits-shaped interfaces and a flat `usize` index,
//! for callers (e.g. a fixed-action-space consumer) that want to work with
//! indices directly rather than build a `ClientView` or run masked-softmax
//! themselves.

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{ActionSpace, GameState, Side};
use netrunner_core::view::build_client_view;

use crate::agent::BotAgent;

/// Picks one action-space index (`0..ActionSpace::SIZE`) for `state`,
/// guaranteed `mask[index]` is `true`.
pub trait Agent {
    /// `mask` is expected to be `get_action_mask(state, registry)` —
    /// passed in rather than recomputed internally so a caller driving
    /// several agents against the same state only pays for it once.
    fn select_action(&mut self, state: &GameState, registry: &CardRegistry, mask: &[bool]) -> usize;
}

/// Adapts any `BotAgent` (view/`PlayerAction`-based) to `Agent`
/// (state/index-based): builds the `ClientView` for `side`, delegates to
/// the wrapped agent, then converts its chosen `PlayerAction` back to an
/// index via `ActionSpace::index_of`.
pub struct BotAgentIndexAdapter<A: BotAgent> {
    inner: A,
    side: Side,
}

impl<A: BotAgent> BotAgentIndexAdapter<A> {
    pub fn new(inner: A, side: Side) -> Self {
        Self { inner, side }
    }
}

impl<A: BotAgent> Agent for BotAgentIndexAdapter<A> {
    fn select_action(&mut self, state: &GameState, registry: &CardRegistry, mask: &[bool]) -> usize {
        let view = build_client_view(state, registry, self.side);
        let action = self.inner.select_action(&view, registry);
        if let Some(index) = ActionSpace::index_of(state, &action) {
            debug_assert!(mask[index], "BotAgent chose an action illegal per mask at index {index}");
            return index;
        }
        // `action` is legal (it came straight from `view.legal_actions`)
        // but isn't representable in the fixed `ActionSpace` — its own doc
        // comment documents this as an expected possibility: a dynamic
        // field (e.g. "which installed card") can exceed the space's fixed
        // caps (`MAX_INSTALLED_PER_SIDE` and friends) in an unusually long
        // game, without that action ever becoming illegal. Rather than
        // panic the whole session over this capacity edge case, fall back
        // to the first mask-legal index instead of the agent's original
        // (unrepresentable) pick.
        mask.iter().position(|&legal| legal).unwrap_or_else(|| {
            panic!("BotAgent chose {action:?} (no ActionSpace index) and no mask-legal index exists as a fallback")
        })
    }
}

/// Index-based `random::RandomAgent`, via `BotAgentIndexAdapter`. Named
/// `Indexed*` (not bare `RandomAgent`) to avoid colliding with the
/// existing view-based `random::RandomAgent`/`crate::RandomAgent` this
/// wraps — both stay independently reachable.
pub type IndexedRandomAgent = BotAgentIndexAdapter<crate::random::RandomAgent>;

/// Index-based `heuristic::HeuristicAgent`, via `BotAgentIndexAdapter`.
/// Same naming rationale as `IndexedRandomAgent`.
pub type IndexedHeuristicAgent = BotAgentIndexAdapter<crate::heuristic::HeuristicAgent>;

/// Index-based adapter over `onnx_policy::OnnxPolicyEvaluator`: evaluates
/// the policy/value network, then argmaxes its priors restricted to
/// `mask`'s legal indices. `OnnxPolicyEvaluator::evaluate`'s masked softmax
/// already zeroes every illegal entry, so this is a direct, deterministic
/// pick — no re-masking needed here.
#[cfg(feature = "onnx")]
pub struct IndexedOnnxAgent {
    evaluator: crate::onnx_policy::OnnxPolicyEvaluator,
}

#[cfg(feature = "onnx")]
impl IndexedOnnxAgent {
    pub fn new(evaluator: crate::onnx_policy::OnnxPolicyEvaluator) -> Self {
        Self { evaluator }
    }
}

#[cfg(feature = "onnx")]
impl Agent for IndexedOnnxAgent {
    fn select_action(&mut self, state: &GameState, registry: &CardRegistry, mask: &[bool]) -> usize {
        use crate::policy::PolicyEvaluator;

        let (priors, _value) = self.evaluator.evaluate(state, registry);
        debug_assert_eq!(priors.len(), mask.len());

        priors
            .iter()
            .enumerate()
            .filter(|&(index, _)| mask[index])
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(index, _)| index)
            .unwrap_or_else(|| panic!("IndexedOnnxAgent::select_action requires at least one legal action"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{CardDefinition, CardId, CardType};
    use netrunner_core::rules::{
        get_action_mask, AgendaPoints, Clicks, CorpState, Credits, GamePhase, MemoryUnits, PlayerResources,
        RunnerState,
    };

    use crate::heuristic::HeuristicAgent;
    use crate::random::RandomAgent;

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

    #[cfg(feature = "onnx")]
    #[test]
    fn indexed_onnx_agent_always_selects_a_legal_index() {
        let model_file = crate::onnx_fixture::write_fixture_model();
        let evaluator = crate::onnx_policy::OnnxPolicyEvaluator::new(model_file.path.to_str().unwrap(), Side::Corp)
            .expect("hand-built fixture model should load successfully");

        let (registry, state) = sample_corp_turn_state();
        let mask = get_action_mask(&state, &registry);

        let mut agent = IndexedOnnxAgent::new(evaluator);
        let index = agent.select_action(&state, &registry, &mask);
        assert!(mask[index], "selected illegal index {index}");
    }
}
