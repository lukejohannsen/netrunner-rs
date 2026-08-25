//! PUCT search over `netrunner_core::rules::ActionSpace`'s fixed
//! `0..ActionSpace::SIZE` index space, backed by a pluggable
//! `crate::policy::PolicyEvaluator` instead of `crate::mcts::MctsAgent`'s
//! random-rollout leaf evaluation. Each `PuctNode` expands lazily into
//! `Edge`s keyed by `ActionSpace` index (only the currently-legal ones, per
//! `get_action_mask`), decoded to a concrete `PlayerAction` via
//! `ActionSpace::action_at` and applied via `GameState::step` exactly once
//! a search actually needs to descend through them.
//!
//! Like `MctsAgent`, this is Information Set search: `PuctAgent::
//! select_action` determinizes one concrete `GameState` sample from the
//! current `ClientView` (`crate::determinize::determinize`) and searches
//! that — see `MctsAgent`'s own doc comment for the shared caveats on what
//! that sampling does and doesn't model. Unlike `MctsAgent`, this runs a
//! single tree rather than several root-parallel ones: genuine PUCT
//! expands one growing tree per decision, driven by the evaluator's priors
//! rather than by exploring every branch equally.

use rand::rngs::StdRng;
use rand::SeedableRng;

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{get_action_mask, ActionSpace, GamePhase, GameState, PlayerAction, Side};
use netrunner_core::view::ClientView;

use crate::agent::BotAgent;
use crate::determinize::determinize;
use crate::policy::PolicyEvaluator;

struct Edge {
    index: usize,
    action: PlayerAction,
    prior: f32,
    visits: u32,
    total_value: f64,
    child: Option<Box<PuctNode>>,
}

struct PuctNode {
    state: GameState,
    visits: u32,
    edges: Vec<Edge>,
    expanded: bool,
}

impl PuctNode {
    fn new(state: GameState) -> Self {
        PuctNode { state, visits: 0, edges: Vec::new(), expanded: false }
    }

    /// Populates `edges` from every legal `(index, action)` pair per
    /// `get_action_mask`, pricing each by `evaluator`'s prior at that
    /// index. Returns the evaluator's scalar value estimate for
    /// `self.state`, for the caller to backpropagate exactly like a
    /// rollout result would be in plain MCTS.
    fn expand(&mut self, registry: &CardRegistry, evaluator: &dyn PolicyEvaluator) -> f32 {
        let mask = get_action_mask(&self.state, registry);
        let (priors, value) = evaluator.evaluate(&self.state, registry);
        debug_assert_eq!(priors.len(), ActionSpace::SIZE, "PolicyEvaluator must return ActionSpace::SIZE priors");

        self.edges = mask
            .iter()
            .enumerate()
            .filter(|&(_, &legal)| legal)
            .map(|(index, _)| {
                let action = ActionSpace::action_at(&self.state, index)
                    .expect("get_action_mask's true entries always decode via action_at");
                Edge { index, action, prior: priors[index], visits: 0, total_value: 0.0, child: None }
            })
            .collect();
        self.expanded = true;
        value
    }

    /// PUCT selection: `argmax_a Q(a) + c_puct * P(a) * sqrt(N_parent) /
    /// (1 + N(a))`, with first-play-urgency `Q = 0` for an unvisited edge.
    fn select_edge(&self, c_puct: f64) -> usize {
        let sqrt_parent_visits = (self.visits.max(1) as f64).sqrt();
        self.edges
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                puct_score(a, sqrt_parent_visits, c_puct)
                    .partial_cmp(&puct_score(b, sqrt_parent_visits, c_puct))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(index, _)| index)
            .expect("select_edge is only called on an expanded node with at least one edge")
    }

    /// The `(visits, total_value)` PUCT has accumulated for `action` at
    /// this node, looked up via `ActionSpace::index_of` rather than by
    /// scanning for `PlayerAction` equality — the fixed-index bridge this
    /// module exists to demonstrate. `None` if `action` isn't one of this
    /// node's legal edges (never expanded, or not legal here).
    pub fn stats_for(&self, action: &PlayerAction) -> Option<(u32, f64)> {
        let index = ActionSpace::index_of(&self.state, action)?;
        self.edges.iter().find(|edge| edge.index == index).map(|edge| (edge.visits, edge.total_value))
    }
}

fn puct_score(edge: &Edge, sqrt_parent_visits: f64, c_puct: f64) -> f64 {
    let q = if edge.visits == 0 { 0.0 } else { edge.total_value / edge.visits as f64 };
    let u = c_puct * edge.prior as f64 * sqrt_parent_visits / (1.0 + edge.visits as f64);
    q + u
}

/// One select→expand→backup pass from `node` down, PUCT-style (no
/// rollout): a terminal state backs up a literal `±1.0`; an unexpanded
/// node is expanded and its evaluator value backed up directly;
/// otherwise the best edge is selected, its child lazily created via
/// `node.state.step`, and the recursive result accumulated onto both the
/// edge and this node. Returns the value backpropagated into `node`
/// itself, always from `side`'s fixed perspective (matching `MctsAgent`/
/// `evaluate_state`'s convention — see the module doc comment).
fn simulate(
    node: &mut PuctNode,
    registry: &CardRegistry,
    evaluator: &dyn PolicyEvaluator,
    side: Side,
    c_puct: f64,
    depth_budget: usize,
) -> f64 {
    if let GamePhase::GameOver(winner) = node.state.phase {
        node.visits += 1;
        return if winner == side { 1.0 } else { -1.0 };
    }

    if !node.expanded {
        let value = node.expand(registry, evaluator) as f64;
        node.visits += 1;
        return value;
    }

    if node.edges.is_empty() {
        // Expanded but stuck (no legal actions and not GameOver): treat as
        // a neutral dead end rather than panicking on `select_edge`.
        node.visits += 1;
        return 0.0;
    }

    if depth_budget == 0 {
        // Depth cutoff on an already-expanded node: re-evaluate its
        // current value without descending further, rather than treating
        // this as a fresh expansion.
        let (_priors, value) = evaluator.evaluate(&node.state, registry);
        node.visits += 1;
        return value as f64;
    }

    let edge_index = node.select_edge(c_puct);

    if node.edges[edge_index].child.is_none() {
        let action = node.edges[edge_index].action.clone();
        match node.state.step(registry, action) {
            Ok((next_state, _events)) => {
                node.edges[edge_index].child = Some(Box::new(PuctNode::new(next_state)));
            }
            // `edge.action` came from `get_action_mask`'s legal slots, so
            // this should never actually fail; treat it as a dead branch
            // rather than corrupting the tree with an unresolved child.
            Err(_) => {
                node.edges[edge_index].visits += 1;
                node.visits += 1;
                return 0.0;
            }
        }
    }

    let value = simulate(
        node.edges[edge_index].child.as_mut().expect("just ensured Some above"),
        registry,
        evaluator,
        side,
        c_puct,
        depth_budget - 1,
    );

    node.edges[edge_index].visits += 1;
    node.edges[edge_index].total_value += value;
    node.visits += 1;
    value
}

/// `c_puct`/`iterations`/`max_depth` for `PuctAgent`. Defaults are kept
/// small enough that a single decision stays fast even in an unoptimized
/// debug build, matching `MctsAgent`'s `DEFAULT_*` constants in spirit.
#[derive(Debug, Clone, Copy)]
pub struct PuctConfig {
    pub c_puct: f64,
    pub iterations: usize,
    pub max_depth: usize,
}

impl Default for PuctConfig {
    fn default() -> Self {
        Self { c_puct: 1.5, iterations: 64, max_depth: 16 }
    }
}

/// PUCT over `ActionSpace`'s fixed index space, driven by a
/// `PolicyEvaluator`. See the module doc comment for the search shape and
/// its relationship to `MctsAgent`.
pub struct PuctAgent {
    side: Side,
    seed: u64,
    evaluator: Box<dyn PolicyEvaluator>,
    config: PuctConfig,
}

impl PuctAgent {
    pub fn new(side: Side, seed: u64, evaluator: impl PolicyEvaluator + 'static) -> Self {
        Self::with_config(side, seed, evaluator, PuctConfig::default())
    }

    pub fn with_config(side: Side, seed: u64, evaluator: impl PolicyEvaluator + 'static, config: PuctConfig) -> Self {
        let config = PuctConfig { iterations: config.iterations.max(1), max_depth: config.max_depth.max(1), ..config };
        Self { side, seed, evaluator: Box::new(evaluator), config }
    }
}

impl BotAgent for PuctAgent {
    fn select_action(&mut self, view: &ClientView, registry: &CardRegistry) -> PlayerAction {
        assert!(!view.legal_actions.is_empty(), "BotAgent::select_action requires at least one legal action");
        if view.legal_actions.len() == 1 {
            return view.legal_actions[0].clone();
        }

        let mut rng = StdRng::seed_from_u64(self.seed);
        self.seed = self.seed.wrapping_add(1);
        let sample = determinize(view, registry, &mut rng);

        let mut root = PuctNode::new(sample);
        for _ in 0..self.config.iterations {
            simulate(&mut root, registry, self.evaluator.as_ref(), self.side, self.config.c_puct, self.config.max_depth);
        }

        // Read root visit/value stats back out via `stats_for` (indexed
        // through `ActionSpace::index_of`) rather than iterating
        // `root.edges` directly — walking the caller's own already-
        // side-filtered `view.legal_actions` is the authoritative
        // candidate list, matching `MctsAgent::new_root`'s same choice to
        // trust it over recomputing legality.
        view.legal_actions
            .iter()
            .filter_map(|action| root.stats_for(action).map(|(visits, total_value)| (action, visits, total_value)))
            .max_by(|(_, a_visits, a_value), (_, b_visits, b_value)| {
                a_visits.cmp(b_visits).then_with(|| a_value.partial_cmp(b_value).unwrap_or(std::cmp::Ordering::Equal))
            })
            .map(|(action, _, _)| action.clone())
            .unwrap_or_else(|| view.legal_actions[0].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::UniformPolicyEvaluator;
    use netrunner_core::dsl::{Card, CardId, CardType};
    use netrunner_core::rules::{
        legal_actions, AgendaPoints, Clicks, CorpState, Credits, GamePhase, InstallSlot, InstalledCard, MemoryUnits,
        PlayerResources, RunnerState, ServerId,
    };
    use netrunner_core::view::build_client_view;

    fn blank_card(id: &str, card_type: CardType) -> Card {
        Card {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
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

    fn empty_runner() -> RunnerState {
        RunnerState {
            identity: None,
            scored_agendas: Vec::new(),
            resources: PlayerResources { credits: Credits(0), clicks: Clicks(0), agenda_points: AgendaPoints(0) },
            memory_units: MemoryUnits(0),
            brain_damage: 0,
            tags: 0,
            grip: Vec::new(),
            stack: Vec::new(),
            rig: Vec::new(),
            heap: Vec::new(),
            link_strength: 0,
            first_hq_run_used_this_turn: false,
            first_install_discount_used_this_turn: false,
        }
    }

    fn empty_corp() -> CorpState {
        CorpState {
            identity: None,
            bad_publicity: 0,
            first_install_used_this_turn: false,
            recurring_credits: 0,
            recurring_credits_max: 0,
            scored_agendas: Vec::new(),
            resources: PlayerResources { credits: Credits(0), clicks: Clicks(0), agenda_points: AgendaPoints(0) },
            hq: Vec::new(),
            r_and_d: Vec::new(),
            archives: Vec::new(),
            installed: Vec::new(),
        }
    }

    fn small_agent(side: Side) -> PuctAgent {
        PuctAgent::with_config(
            side,
            99,
            UniformPolicyEvaluator::new(side),
            PuctConfig { c_puct: 1.5, iterations: 40, max_depth: 8 },
        )
    }

    #[test]
    fn always_returns_a_member_of_legal_actions() {
        let registry = CardRegistry::new();
        let mut state = GameState::new(0);
        state.corp = empty_corp();
        state.runner = empty_runner();
        state.corp.resources.clicks = Clicks(3);
        state.corp.resources.credits = Credits(5);

        let view = build_client_view(&state, &registry, Side::Corp);
        assert!(!view.legal_actions.is_empty());

        let mut agent = small_agent(Side::Corp);
        let chosen = agent.select_action(&view, &registry);
        assert!(view.legal_actions.contains(&chosen));
    }

    #[test]
    fn single_legal_action_short_circuits() {
        let registry = CardRegistry::new();
        let mut state = GameState::new(0);
        state.phase = GamePhase::Mulligan(Side::Corp);
        state.corp = empty_corp();
        state.runner = empty_runner();

        let view = build_client_view(&state, &registry, Side::Corp);
        let mut agent = small_agent(Side::Corp);
        let chosen = agent.select_action(&view, &registry);
        assert!(view.legal_actions.contains(&chosen));
    }

    #[test]
    fn favors_scoring_an_immediately_winning_agenda() {
        let mut registry = CardRegistry::new();
        let mut agenda = blank_card("winning_agenda", CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(7);
        registry.insert(agenda);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.runner = empty_runner();
        state.corp = empty_corp();
        state.corp.resources.clicks = Clicks(3);
        state.corp.resources.credits = Credits(5);
        state.corp.installed = vec![InstalledCard {
            card: CardId("winning_agenda".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: false,
            advancement_tokens: 3,
        }];

        let view = build_client_view(&state, &registry, Side::Corp);
        assert!(view.legal_actions.contains(&PlayerAction::ScoreAgenda { card_id: CardId("winning_agenda".to_string()) }));

        let mut agent = PuctAgent::with_config(
            Side::Corp,
            123,
            UniformPolicyEvaluator::new(Side::Corp),
            PuctConfig { c_puct: 1.5, iterations: 200, max_depth: 10 },
        );
        let chosen = agent.select_action(&view, &registry);
        assert_eq!(chosen, PlayerAction::ScoreAgenda { card_id: CardId("winning_agenda".to_string()) });
    }

    #[test]
    fn expand_produces_exactly_the_edges_legal_actions_reports() {
        let mut registry = CardRegistry::new();
        registry.insert(blank_card("hedge_fund", CardType::Operation));

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.runner = empty_runner();
        state.corp = empty_corp();
        state.corp.resources.clicks = Clicks(3);
        state.corp.resources.credits = Credits(10);
        state.corp.hq = vec![CardId("hedge_fund".to_string())];

        let mut node = PuctNode::new(state.clone());
        let evaluator = UniformPolicyEvaluator::new(Side::Corp);
        node.expand(&registry, &evaluator);

        // `PlayerAction` isn't `Hash`, so compare as multisets via mutual
        // containment rather than collecting into a `HashSet`.
        let mut expected = legal_actions(&state, &registry);
        let mut actual: Vec<PlayerAction> = node.edges.iter().map(|edge| edge.action.clone()).collect();
        assert_eq!(expected.len(), actual.len());
        expected.retain(|candidate| {
            if let Some(position) = actual.iter().position(|other| other == candidate) {
                actual.remove(position);
                false
            } else {
                true
            }
        });
        assert!(expected.is_empty(), "legal_actions produced actions missing from expand()'s edges");
        assert!(actual.is_empty(), "expand() produced edges missing from legal_actions");
    }

    #[test]
    fn stats_for_reports_visits_after_search_for_the_chosen_action() {
        let registry = CardRegistry::new();
        let mut state = GameState::new(0);
        state.corp = empty_corp();
        state.runner = empty_runner();
        state.corp.resources.clicks = Clicks(3);
        state.corp.resources.credits = Credits(5);

        let mut root = PuctNode::new(state.clone());
        let evaluator = UniformPolicyEvaluator::new(Side::Corp);
        for _ in 0..40 {
            simulate(&mut root, &registry, &evaluator, Side::Corp, 1.5, 8);
        }

        let best = root.edges.iter().max_by_key(|edge| edge.visits).expect("root should have expanded edges");
        let (visits, _total_value) = root.stats_for(&best.action).expect("stats_for should find the chosen edge");
        assert!(visits > 0);
        assert_eq!(visits, best.visits);
    }
}
