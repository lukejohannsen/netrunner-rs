//! Fixed-shape categorical policy/value evaluation over
//! `netrunner_core::rules::ActionSpace`, for PUCT-style search
//! (`crate::puct`). A `PolicyEvaluator` estimates `(priors over
//! ActionSpace::SIZE, value)` for a `GameState`; `PuctAgent` expands search
//! nodes with it exactly as AlphaZero-style search expects a policy/value
//! network to behave — this module just doesn't require one to exist yet.

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{get_action_mask, GameState, Side};

use crate::eval::{evaluate_state_with, Weights};
use crate::personality::Personality;

/// Evaluates a leaf `GameState`: a categorical prior over every one of
/// `ActionSpace::SIZE` fixed action slots (illegal slots MUST be `0.0` —
/// `PuctNode::expand` trusts this without re-masking the result) plus a
/// scalar value estimate of the position, bounded to roughly `[-1, 1]`
/// (AlphaZero-style value-head convention) so it backpropagates cleanly
/// alongside the literal `±1.0` terminal values `crate::puct::simulate`
/// assigns at `GameOver`.
pub trait PolicyEvaluator: Send + Sync {
    fn evaluate(&self, state: &GameState, registry: &CardRegistry) -> (Vec<f32>, f32);
}

/// Lets a boxed trait object stand in for `impl PolicyEvaluator + 'static`
/// (e.g. `PuctAgent::with_config`'s evaluator parameter) — needed by any
/// caller that picks between differently-typed evaluators (say,
/// `UniformPolicyEvaluator` vs. `OnnxPolicyEvaluator`) at runtime and can
/// only name the choice as `Box<dyn PolicyEvaluator>`.
impl PolicyEvaluator for Box<dyn PolicyEvaluator> {
    fn evaluate(&self, state: &GameState, registry: &CardRegistry) -> (Vec<f32>, f32) {
        (**self).evaluate(state, registry)
    }
}

/// Rescales `evaluate_state`'s unbounded score into PUCT's expected
/// `[-1, 1]` value range via `tanh`. `evaluate_state` swings roughly
/// ±20/agenda point plus smaller credit/tag/board terms, so a mid-game
/// lead of ~±60 lands around ±0.5 rather than saturating immediately.
const VALUE_SQUASH_SCALE: f64 = 100.0;

/// Baseline evaluator with no learned network behind it: priors are
/// uniform over whichever slots `get_action_mask` marks legal, and value is
/// `evaluate_state` squashed into `[-1, 1]`. Lets `PuctAgent` run standalone
/// — and its search machinery be tested — before any real policy/value
/// network exists.
pub struct UniformPolicyEvaluator {
    pub side: Side,
    /// The value head's terms; a `Personality` biases the search's
    /// leaves exactly as it biases the one-ply heuristic.
    pub weights: Weights,
}

impl UniformPolicyEvaluator {
    pub fn new(side: Side) -> Self {
        Self::with_personality(side, Personality::Balanced)
    }

    pub fn with_personality(side: Side, personality: Personality) -> Self {
        Self { side, weights: personality.weights() }
    }
}

impl PolicyEvaluator for UniformPolicyEvaluator {
    fn evaluate(&self, state: &GameState, registry: &CardRegistry) -> (Vec<f32>, f32) {
        let mask = get_action_mask(state, registry);
        let legal_count = mask.iter().filter(|&&legal| legal).count();
        // No legal actions (an over/stuck state) still needs a
        // correctly-sized, all-zero prior vector — `PuctNode::expand`
        // handles that by simply producing no edges.
        let prior = if legal_count == 0 { 0.0 } else { 1.0 / legal_count as f32 };
        let priors = mask.iter().map(|&legal| if legal { prior } else { 0.0 }).collect();

        let value = (evaluate_state_with(state, self.side, registry, &self.weights) / VALUE_SQUASH_SCALE).tanh() as f32;
        (priors, value)
    }
}

/// Takes its priors from one evaluator and its value from another.
///
/// **An instrument, not a player.** A `PolicyEvaluator` answers two
/// questions at once — "which moves are worth looking at" and "how good is
/// this position" — and when a network-backed search loses to a
/// network-free one, the aggregate result cannot say which of the two
/// answers went wrong. Pairing one source's priors with the other's value
/// separates them: run the arena twice, once each way, and the half that
/// carries the loss is the half at fault.
///
/// That question is live. Across three volume runs (ROADMAP Phase 2 §5)
/// every candidate scored 0.22–0.48 against the uniform search, and there
/// is a specific reason to suspect the priors rather than the value:
/// `OnnxPolicyEvaluator` encodes its observation from a **fixed** side
/// while `get_action_mask` returns the **side-to-move**'s legal set, and
/// training only ever pairs a side's observation with that same side's
/// visit counts. `ActionSpace` is heavily side-partitioned, so at an
/// opponent-to-move node the softmax renormalizes over exactly the slots
/// training drove toward −∞. The value head has no equivalent problem: its
/// fixed perspective is what `puct::simulate`'s negamax convention wants.
///
/// Costs two forward passes per node, which is the point of it being a
/// diagnostic rather than something to seat in a real match.
pub struct SplitEvaluator {
    priors_from: Box<dyn PolicyEvaluator>,
    value_from: Box<dyn PolicyEvaluator>,
}

impl SplitEvaluator {
    pub fn new(priors_from: Box<dyn PolicyEvaluator>, value_from: Box<dyn PolicyEvaluator>) -> Self {
        Self { priors_from, value_from }
    }
}

impl PolicyEvaluator for SplitEvaluator {
    fn evaluate(&self, state: &GameState, registry: &CardRegistry) -> (Vec<f32>, f32) {
        let (priors, _discarded_value) = self.priors_from.evaluate(state, registry);
        let (_discarded_priors, value) = self.value_from.evaluate(state, registry);
        (priors, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{CardDefinition, CardId, CardType};
    use netrunner_core::rules::{
        ActionSpace, AgendaPoints, Clicks, CorpState, Credits, GamePhase, MemoryUnits, PlayerResources, RunnerState,
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
    fn priors_are_correctly_sized_and_nonzero_exactly_where_the_mask_is_legal() {
        let (registry, state) = sample_corp_turn_state();
        let mask = get_action_mask(&state, &registry);
        let evaluator = UniformPolicyEvaluator::new(Side::Corp);

        let (priors, _value) = evaluator.evaluate(&state, &registry);
        assert_eq!(priors.len(), ActionSpace::SIZE);

        for (index, &legal) in mask.iter().enumerate() {
            if legal {
                assert!(priors[index] > 0.0, "index {index} is legal but has a zero prior");
            } else {
                assert_eq!(priors[index], 0.0, "index {index} is illegal but has a nonzero prior");
            }
        }
    }

    #[test]
    fn priors_sum_to_one_over_legal_slots() {
        let (registry, state) = sample_corp_turn_state();
        let evaluator = UniformPolicyEvaluator::new(Side::Corp);

        let (priors, _value) = evaluator.evaluate(&state, &registry);
        let sum: f32 = priors.iter().sum();
        assert!((sum - 1.0).abs() < 1e-4, "priors summed to {sum}, expected ~1.0");
    }

    #[test]
    fn value_is_bounded_and_favors_the_leading_side() {
        let mut leading = GameState::new(0);
        leading.corp.resources.agenda_points = AgendaPoints(5);

        let corp_evaluator = UniformPolicyEvaluator::new(Side::Corp);
        let runner_evaluator = UniformPolicyEvaluator::new(Side::Runner);
        let registry = CardRegistry::new();

        let (_priors, corp_value) = corp_evaluator.evaluate(&leading, &registry);
        let (_priors, runner_value) = runner_evaluator.evaluate(&leading, &registry);

        assert!(corp_value > 0.0);
        assert!(runner_value < 0.0);
        assert!((-1.0..=1.0).contains(&corp_value));
        assert!((-1.0..=1.0).contains(&runner_value));
    }

    #[test]
    fn no_legal_actions_yields_an_all_zero_correctly_sized_prior() {
        let mut state = GameState::new(0);
        state.phase = GamePhase::GameOver(Side::Runner);
        let registry = CardRegistry::new();
        let evaluator = UniformPolicyEvaluator::new(Side::Corp);

        let (priors, _value) = evaluator.evaluate(&state, &registry);
        assert_eq!(priors.len(), ActionSpace::SIZE);
        assert!(priors.iter().all(|&p| p == 0.0));
    }
}
