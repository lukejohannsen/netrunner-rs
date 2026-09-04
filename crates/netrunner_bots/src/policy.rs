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

/// Mixes another evaluator's priors toward uniform over the legal set:
/// `P' = (1 - epsilon) * P + epsilon / legal_count`, value untouched.
///
/// **An instrument, like `SplitEvaluator`, and for a measured reason.**
/// `rejected_iter_008.onnx` ranks better than chance — 44.4% top-1 against
/// the search's own argmax over 47,327 held-out steps, where uniform scores
/// 30.1% — yet seating its priors alone scores 0.1745 in the arena against
/// 0.4141 for its value alone. The diagnosis is calibration, not ranking:
/// the head is *peakier than its own training target* (top prior 0.452 mean
/// against the search's 0.349) and, in the 56% of steps where it is wrong,
/// gives the correct action a median prior of 0.110. `puct_score`'s
/// exploration term is linear in the prior and there is no root Dirichlet
/// noise, so a starved action is never explored back. A uniform prior
/// starves nothing, which is how a worse ranking wins.
///
/// `epsilon` is therefore a dial between the two: `0.0` is the network's
/// prior untouched, `1.0` is the uniform search's own prior. Sweeping it
/// says whether the loss is recoverable by calibration alone.
pub struct MixedPriorEvaluator {
    inner: Box<dyn PolicyEvaluator>,
    epsilon: f32,
}

impl MixedPriorEvaluator {
    pub fn new(inner: Box<dyn PolicyEvaluator>, epsilon: f32) -> Self {
        Self { inner, epsilon: epsilon.clamp(0.0, 1.0) }
    }
}

impl PolicyEvaluator for MixedPriorEvaluator {
    fn evaluate(&self, state: &GameState, registry: &CardRegistry) -> (Vec<f32>, f32) {
        let (priors, value) = self.inner.evaluate(state, registry);
        if self.epsilon <= 0.0 {
            return (priors, value);
        }
        // Mixed over the *mask*, not over whichever slots came back
        // nonzero: a legal slot the inner evaluator gave exactly 0.0 (an
        // underflowed softmax tail) is precisely the starved case this
        // exists to lift.
        let mask = get_action_mask(state, registry);
        let legal_count = mask.iter().filter(|&&legal| legal).count();
        if legal_count == 0 {
            return (priors, value);
        }
        let uniform = 1.0 / legal_count as f32;
        let mixed = priors
            .iter()
            .zip(&mask)
            .map(|(&prior, &legal)| if legal { (1.0 - self.epsilon) * prior + self.epsilon * uniform } else { 0.0 })
            .collect();
        (mixed, value)
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
    fn mixing_all_the_way_to_uniform_reproduces_the_uniform_evaluators_prior() {
        let (registry, state) = sample_corp_turn_state();
        let uniform = UniformPolicyEvaluator::new(Side::Corp);
        let (expected, _) = uniform.evaluate(&state, &registry);

        let mixed = MixedPriorEvaluator::new(Box::new(UniformPolicyEvaluator::new(Side::Corp)), 1.0);
        let (priors, _) = mixed.evaluate(&state, &registry);

        for (index, (&got, &want)) in priors.iter().zip(&expected).enumerate() {
            assert!((got - want).abs() < 1e-6, "index {index}: {got} != {want}");
        }
    }

    #[test]
    fn mixing_lifts_a_starved_legal_slot_and_leaves_illegal_slots_at_zero() {
        // The case the dial exists for: a legal action the inner evaluator
        // gave no mass at all still gets its uniform share.
        struct OneHot;
        impl PolicyEvaluator for OneHot {
            fn evaluate(&self, state: &GameState, registry: &CardRegistry) -> (Vec<f32>, f32) {
                let mask = get_action_mask(state, registry);
                let first = mask.iter().position(|&legal| legal).expect("the fixture state has legal actions");
                let mut priors = vec![0.0; ActionSpace::SIZE];
                priors[first] = 1.0;
                (priors, 0.0)
            }
        }

        let (registry, state) = sample_corp_turn_state();
        let mask = get_action_mask(&state, &registry);
        let legal_count = mask.iter().filter(|&&legal| legal).count();
        assert!(legal_count > 1, "this test needs a state with a slot to starve");

        let mixed = MixedPriorEvaluator::new(Box::new(OneHot), 0.5);
        let (priors, _) = mixed.evaluate(&state, &registry);

        let share = 0.5 / legal_count as f32;
        for (index, &legal) in mask.iter().enumerate() {
            if legal {
                assert!(priors[index] >= share - 1e-6, "legal index {index} was left starved at {}", priors[index]);
            } else {
                assert_eq!(priors[index], 0.0, "index {index} is illegal but got mass");
            }
        }
        let sum: f32 = priors.iter().sum();
        assert!((sum - 1.0).abs() < 1e-4, "mixing must stay a distribution, summed to {sum}");
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
