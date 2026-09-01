//! PUCT search over `netrunner_core::rules::ActionSpace`'s fixed
//! `0..ActionSpace::SIZE` index space, backed by a pluggable
//! `crate::policy::PolicyEvaluator` instead of `crate::mcts::MctsAgent`'s
//! random-rollout leaf evaluation. Each `PuctNode` below the root expands
//! lazily into `Edge`s over the currently-legal `ActionSpace` indices per
//! `get_action_mask`, decoded to a concrete `PlayerAction` via
//! `ActionSpace::action_at` and applied via `GameState::step` exactly once
//! a search actually needs to descend through them.
//!
//! The **root** is the exception, deliberately: its edges come from the
//! caller's `view.legal_actions` rather than from the sample's own mask,
//! because that list is what `search` reports its results over. See
//! `PuctNode::expand_root`.
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
    /// This edge's `ActionSpace` slot *in its own node's state*. `None`
    /// only ever happens at the root, whose edges come from the caller's
    /// `view.legal_actions` (real state) while the node itself holds a
    /// determinized sample: an action naming a card the sample doesn't
    /// have encodes to nothing here. The edge is still searched — it is a
    /// real option for the caller — it just contributes no prior and no
    /// `visit_counts` slot.
    index: Option<usize>,
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
                Edge { index: Some(index), action, prior: priors[index], visits: 0, total_value: 0.0, child: None }
            })
            .collect();
        self.expanded = true;
        value
    }

    /// Root expansion: `edges` come from the caller's own `actions` —
    /// `view.legal_actions`, computed against the **real** state — rather
    /// than from `get_action_mask` over this node's determinized sample.
    ///
    /// This is the one place the two must not be allowed to diverge.
    /// `search` reports its results over `view.legal_actions`, so an
    /// option the caller may legally submit has to be an edge here or it
    /// is silently unreportable; deriving the root's candidates from the
    /// sample instead made the reported set an *intersection* of two
    /// independently-computed legal sets, which can be — and in self-play
    /// was — empty. Every approximation `determinize` makes (a resampled
    /// hidden card, a counter it doesn't carry) deletes candidates that
    /// way. `MctsAgent::new_root` has always seeded from the caller's list
    /// for the same reason; this brings PUCT in line.
    ///
    /// A candidate that is illegal *in the sample* is kept, not dropped:
    /// `simulate`'s `step` failure path already turns it into a dead
    /// branch carrying a value, exactly as `MctsAgent` does.
    fn expand_root(
        &mut self,
        actions: &[PlayerAction],
        registry: &CardRegistry,
        evaluator: &dyn PolicyEvaluator,
    ) -> f32 {
        let (priors, value) = evaluator.evaluate(&self.state, registry);
        debug_assert_eq!(priors.len(), ActionSpace::SIZE, "PolicyEvaluator must return ActionSpace::SIZE priors");

        // A uniform stand-in for a candidate that doesn't encode against
        // the sample, so it competes on roughly even terms rather than
        // being frozen out by a zero prior it didn't earn.
        let uniform_prior = 1.0 / actions.len().max(1) as f32;
        self.edges = actions
            .iter()
            .map(|action| {
                let index = ActionSpace::index_of(&self.state, action);
                let prior = index.map_or(uniform_prior, |index| priors[index]);
                Edge { index, action: action.clone(), prior, visits: 0, total_value: 0.0, child: None }
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

    /// Runs a full PUCT search from `view`/`registry` and returns every
    /// root visit/value stat gathered, rather than collapsing straight to
    /// a single chosen `PlayerAction` like `select_action` does. Exists so
    /// callers that need the search's full visit-count distribution (e.g.
    /// recording an AlphaZero-style policy target) don't have to duplicate
    /// PUCT's tree-search internals to get it — `select_action` itself is
    /// now just this plus a final `max_by`.
    ///
    /// Unlike `select_action`, this always runs the search even when only
    /// one action is legal (a single-edge root's stats are still
    /// meaningful shape-wise); callers that want `select_action`'s cheap
    /// short-circuit should check `view.legal_actions.len()` themselves
    /// first.
    pub fn search(&mut self, view: &ClientView, registry: &CardRegistry) -> PuctSearchStats {
        assert!(!view.legal_actions.is_empty(), "PuctAgent::search requires at least one legal action");

        let mut rng = StdRng::seed_from_u64(self.seed);
        self.seed = self.seed.wrapping_add(1);
        let sample = determinize(view, registry, &mut rng);

        // Expanded here rather than by the first `simulate`, because only
        // the root may be seeded from `view.legal_actions` — see
        // `expand_root`. Counted as one visit, the same bookkeeping
        // `simulate`'s own expansion branch does, so `puct_score`'s
        // `sqrt(N_parent)` starts from a visited parent.
        let mut root = PuctNode::new(sample);
        root.expand_root(&view.legal_actions, registry, self.evaluator.as_ref());
        root.visits = 1;
        for _ in 0..self.config.iterations {
            simulate(&mut root, registry, self.evaluator.as_ref(), self.side, self.config.c_puct, self.config.max_depth);
        }

        // One `ActionStat` per `view.legal_actions` entry, always: the
        // caller's list is the authoritative candidate set (matching
        // `MctsAgent::new_root`), and `expand_root` made it the root's
        // edge set, so every entry has stats to report even if the search
        // never descended it. `index` is this action's slot *in the
        // determinized sample* and can be absent — see `Edge::index`; such
        // an action still gets a stat, it just claims no `visit_counts`
        // slot. Callers recording a policy target should re-index against
        // whatever state that target is paired with rather than reusing
        // these; see `netrunner_selfplay`.
        let mut visit_counts = vec![0u32; ActionSpace::SIZE];
        let actions: Vec<ActionStat> = root
            .edges
            .iter()
            .map(|edge| {
                if let Some(index) = edge.index {
                    visit_counts[index] = edge.visits;
                }
                ActionStat {
                    index: edge.index,
                    action: edge.action.clone(),
                    visits: edge.visits,
                    total_value: edge.total_value,
                }
            })
            .collect();

        debug_assert_eq!(
            actions.len(),
            view.legal_actions.len(),
            "search must report every action the caller may submit"
        );
        PuctSearchStats { visit_counts, actions }
    }
}

/// One root edge's outcome from `PuctAgent::search`: which
/// `PlayerAction` it is, how many visits/how much total value PUCT
/// accumulated on it, and its `ActionSpace` slot.
#[derive(Debug, Clone)]
pub struct ActionStat {
    /// This action's slot **in the determinized sample the search ran
    /// on** — not in the caller's real state, and the two spaces do not
    /// generally agree (`determinize` resamples hidden zones and rebuilds
    /// `corp.installed` in view order). `None` when the action doesn't
    /// encode against the sample at all. A caller pairing a policy target
    /// with a real-state observation must re-index `action` itself rather
    /// than reuse this.
    pub index: Option<usize>,
    pub action: PlayerAction,
    pub visits: u32,
    pub total_value: f64,
}

/// Full result of one `PuctAgent::search` call.
#[derive(Debug, Clone)]
pub struct PuctSearchStats {
    /// Length `ActionSpace::SIZE`, indexed in the **determinized
    /// sample's** space — see `ActionStat::index`. Zero everywhere except
    /// the slots `actions` below could encode.
    pub visit_counts: Vec<u32>,
    /// Exactly one entry per `view.legal_actions` entry, in that order —
    /// `search` reports every action the caller may submit, with zero
    /// visits for one the search never descended.
    pub actions: Vec<ActionStat>,
}

impl BotAgent for PuctAgent {
    fn select_action(&mut self, view: &ClientView, registry: &CardRegistry) -> PlayerAction {
        assert!(!view.legal_actions.is_empty(), "BotAgent::select_action requires at least one legal action");
        if view.legal_actions.len() == 1 {
            return view.legal_actions[0].clone();
        }

        let stats = self.search(view, registry);
        stats
            .actions
            .iter()
            .max_by(|a, b| {
                a.visits.cmp(&b.visits).then_with(|| a.total_value.partial_cmp(&b.total_value).unwrap_or(std::cmp::Ordering::Equal))
            })
            .map(|stat| stat.action.clone())
            .unwrap_or_else(|| view.legal_actions[0].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::UniformPolicyEvaluator;
    use netrunner_core::dsl::{CardDefinition, CardId, CardType, Cost, Effect, IceType, SubroutineDef};
    use netrunner_core::rules::{
        legal_actions, AccessPhase, AccessState, AgendaPoints, Clicks, CorpState, Credits,
        EncounteredSubroutine, GamePhase, InstallId, InstallSlot, InstalledCard, InstalledRunnerCard, MemoryUnits,
        PaidAbilityWindow, PlayerResources, PublicAccessPhase, RunIce, RunPhase, RunState, RunnerState,
        ServerId, SubroutineStatus, WindowCheckpoint,
    };
    use netrunner_core::view::build_client_view;

    fn blank_card(id: &str, card_type: CardType) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type,
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

    fn empty_corp() -> CorpState {
        CorpState {
            resources: PlayerResources { credits: Credits(0), clicks: Clicks(0), agenda_points: AgendaPoints(0) },
            ..Default::default()
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
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            advancement_tokens: 3,
            ..Default::default()
        }];

        let view = build_client_view(&state, &registry, Side::Corp);
        assert!(view.legal_actions.contains(&PlayerAction::ScoreAgenda { target: InstallId(1) }));

        let mut agent = PuctAgent::with_config(
            Side::Corp,
            123,
            UniformPolicyEvaluator::new(Side::Corp),
            PuctConfig { c_puct: 1.5, iterations: 200, max_depth: 10 },
        );
        let chosen = agent.select_action(&view, &registry);
        assert_eq!(chosen, PlayerAction::ScoreAgenda { target: InstallId(1) });
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

    /// The invariant `search` rests on, tested where it lives: a caller
    /// action that is *illegal in the sample* is still an edge.
    ///
    /// Root edges used to come from `get_action_mask` over the sample, so
    /// such an action simply had no edge and vanished from the results —
    /// with every result gone, self-play panicked. Kept as a dead branch
    /// instead, exactly as `MctsAgent` keeps one (`mcts.rs`'s `Err(_) =>
    /// evaluate_state` arm).
    #[test]
    fn expand_root_keeps_a_candidate_the_sample_rejects() {
        let registry = CardRegistry::new();
        let mut state = GameState::new(0);
        state.corp = empty_corp();
        state.runner = empty_runner();
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.resources.clicks = Clicks(1);

        // Legal here; and one that certainly is not — the Runner has no
        // clicks, and it is not their phase.
        let legal = PlayerAction::GainCreditClick { side: Side::Corp };
        let illegal = PlayerAction::DrawCardClick { side: Side::Runner };
        assert!(legal_actions(&state, &registry).contains(&legal));
        assert!(!legal_actions(&state, &registry).contains(&illegal));

        let mut root = PuctNode::new(state);
        root.expand_root(
            &[legal.clone(), illegal.clone()],
            &registry,
            &UniformPolicyEvaluator::new(Side::Corp),
        );

        let edges: Vec<&PlayerAction> = root.edges.iter().map(|edge| &edge.action).collect();
        assert_eq!(edges, vec![&legal, &illegal], "both candidates must survive, in the caller's order");
    }

    /// A run whose accessed card the *Corp* cannot see: the view masks the
    /// identity (`mask_run_state`'s `card_visible` is false off Archives),
    /// so `determinize` samples some other card into the parked decision.
    /// The Corp's real legal actions still name the true card.
    ///
    /// This is the shape that used to empty `search`'s results: the root
    /// was expanded from the *sample's* legal actions and the caller's
    /// were then looked up in it, so a divergence between the two dropped
    /// options rather than merely mis-valuing them. Nothing here asserts
    /// the sample agrees with reality — it cannot, that is the point of
    /// hiding the card — only that `search` still reports every action its
    /// caller is allowed to submit.
    #[test]
    fn search_reports_every_legal_action_even_when_the_sample_disagrees() {
        let mut registry = CardRegistry::new();
        registry.insert(blank_card("snare", CardType::Asset));
        for filler in ["other_asset_a", "other_asset_b", "other_asset_c"] {
            registry.insert(blank_card(filler, CardType::Asset));
        }

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.corp = empty_corp();
        state.runner = empty_runner();
        state.corp.resources.credits = Credits(9);
        state.runner.resources.clicks = Clicks(3);
        state.corp.r_and_d = vec![CardId("other_asset_a".to_string()), CardId("other_asset_b".to_string())];
        state.active_run = Some(RunState {
            server: ServerId::Remote(0),
            phase: RunPhase::AccessingCard,
            access_state: Some(AccessState {
                server: ServerId::Remote(0),
                phase: AccessPhase::PendingInteractiveTrigger {
                    card_id: CardId("snare".to_string()),
                    cost: Cost::Credits(4),
                    decider: Side::Corp,
                    can_pay: true,
                },
                ..Default::default()
            }),
            jack_out_permitted: true,
            ..Default::default()
        });

        let view = build_client_view(&state, &registry, Side::Corp);
        assert_eq!(view.legal_actions.len(), 2, "the Corp may pay or decline: {:?}", view.legal_actions);
        // The premise: the Corp's view really does hide which card this is.
        let masked = view.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert!(
            matches!(&masked.phase, PublicAccessPhase::PendingInteractiveTrigger { card: None, .. }),
            "off-Archives access must stay masked from the Corp, or this test proves nothing"
        );

        let mut agent = small_agent(Side::Corp);
        let stats = agent.search(&view, &registry);

        assert_eq!(stats.actions.len(), view.legal_actions.len());
        for action in &view.legal_actions {
            assert!(
                stats.actions.iter().any(|stat| stat.action == *action),
                "search dropped {action:?} from its results: {:?}",
                stats.actions
            );
        }
    }

    /// The state that actually panicked self-play, reduced — and now the
    /// test that the cause is gone rather than merely survivable.
    ///
    /// *Tāo Salonga*'s "swap two installed Barriers" parks a
    /// `ChooseCards { source: OpponentInstalled }` over Corp installs, one
    /// of them **unrezzed**. This used to offer the Runner a
    /// `ToggleCardSelection` naming that ICE by `CardId` — a card their own
    /// `ClientView` masks to `None` — and `determinize` then resampled it,
    /// so the caller's actions and the sample's had nothing in common:
    /// `ToggleCardSelection` encoded against nothing and
    /// `ConfirmCardSelection` was rejected because `selected` named cards
    /// the sample lacked. That disjointness emptied `search`'s results.
    ///
    /// The action now carries a *position*, which reveals nothing and
    /// survives resampling, so the assertions below are the inverse of what
    /// they were: no masked `CardId` reaches the Runner at all, and the
    /// search still reports every action the caller may submit.
    #[test]
    fn a_selection_over_masked_opponent_installs_leaks_nothing_and_survives_search() {
        use netrunner_core::dsl::{CardFilter, CardZoneRef};
        use netrunner_core::rules::{PendingChoiceResume, PendingDecision};

        let mut registry = CardRegistry::new();
        for id in ["palisade", "ballista", "funhouse", "tithe", "whitespace"] {
            let mut ice = blank_card(id, CardType::Ice(IceType::Barrier));
            ice.strength = Some(2);
            registry.insert(ice);
        }

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.corp = empty_corp();
        state.runner = empty_runner();
        state.corp.installed = vec![
            InstalledCard {
                card: CardId("palisade".to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(1),
                slot: InstallSlot::Ice,
                rezzed: true,
                ..Default::default()
            },
            // Unrezzed: masked from the Runner, and so resampled away.
            InstalledCard {
                card: CardId("ballista".to_string()),
                install_id: InstallId(2),
                server: ServerId::Hq,
                slot: InstallSlot::Ice,
                rezzed: false,
                ..Default::default()
            },
        ];
        state.pending_decision = Some(PendingDecision::ChooseCards {
            side: Side::Runner,
            source: CardZoneRef::OpponentInstalled,
            filter: CardFilter::CardType(CardType::Ice(IceType::Barrier)),
            min: 2,
            max: 2,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: None,
            // Position 0 — the rezzed Palisade.
            selected: vec![0],
            source_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });

        let view = build_client_view(&state, &registry, Side::Runner);

        // The unrezzed ICE is selectable — real Netrunner lets the Runner
        // swap ICE they cannot identify, so removing the action would be a
        // rules change, not a fix.
        assert!(
            view.legal_actions.contains(&PlayerAction::ToggleCardSelection { position: 1 }),
            "the unrezzed ICE at position 1 is still selectable — {:?}",
            view.legal_actions
        );
        // ...and naming it costs the Runner nothing they did not already
        // know. This is the leak the whole change exists to close: the
        // masked card's title appears nowhere in what the Runner is handed.
        let masked = view.corp.servers.iter().flat_map(|s| s.ice.iter()).find(|i| !i.rezzed).expect("an unrezzed ICE");
        assert_eq!(masked.card, None, "the view really does mask it");
        assert!(
            !format!("{:?}", view.legal_actions).contains("ballista"),
            "no legal action may name a card this view masks — {:?}",
            view.legal_actions
        );
        assert!(
            !format!("{:?}", view.pending_decision).contains("ballista"),
            "nor may the parked decision — {:?}",
            view.pending_decision
        );

        // Many samples: it must hold for every one, not most.
        for seed in 0..25u64 {
            let mut agent = PuctAgent::with_config(
                Side::Runner,
                seed,
                UniformPolicyEvaluator::new(Side::Runner),
                PuctConfig { c_puct: 1.5, iterations: 16, max_depth: 4 },
            );
            let stats = agent.search(&view, &registry);
            assert_eq!(
                stats.actions.len(),
                view.legal_actions.len(),
                "seed {seed}: search dropped an action the caller may submit"
            );
        }
    }

    /// The `MctsAgent` test of the same name, run against PUCT — it had no
    /// mid-run coverage at all before.
    #[test]
    fn does_not_panic_mid_run_with_a_paid_ability_window_open() {
        let mut registry = CardRegistry::new();
        let mut breaker = blank_card("corroder", CardType::Program);
        breaker.side = Side::Runner;
        registry.insert(breaker);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.corp = empty_corp();
        state.runner = empty_runner();
        state.runner.resources.clicks = Clicks(4);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![InstalledRunnerCard {
            card: CardId("corroder".to_string()),
            base_strength: 2,
            ..Default::default()
        }];
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce {
                install_id: netrunner_core::rules::InstallId::PLACEHOLDER,
                card_id: CardId("wall_of_static".to_string()),
                current_strength: 3,
                ice_type: IceType::Barrier,
                subroutines: vec![EncounteredSubroutine {
                    id: 0,
                    definition: SubroutineDef { text: "End the run.".to_string(), effect: Effect::EndTheRun },
                    status: SubroutineStatus::Pending,
                }],
                rezzed: true,
            }],
            ..Default::default()
        });
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            checkpoint: WindowCheckpoint::Run,
            return_phase: Box::new(state.phase),
        });

        let view = build_client_view(&state, &registry, Side::Runner);
        assert!(!view.legal_actions.is_empty());

        let mut agent = small_agent(Side::Runner);
        let stats = agent.search(&view, &registry);
        assert_eq!(stats.actions.len(), view.legal_actions.len());

        let chosen = agent.select_action(&view, &registry);
        assert!(view.legal_actions.contains(&chosen));
    }

    #[test]
    fn search_accumulates_visits_on_the_actions_it_explored() {
        let registry = CardRegistry::new();
        let mut state = GameState::new(0);
        state.corp = empty_corp();
        state.runner = empty_runner();
        state.corp.resources.clicks = Clicks(3);
        state.corp.resources.credits = Credits(5);

        let view = build_client_view(&state, &registry, Side::Corp);
        let mut agent = PuctAgent::with_config(
            Side::Corp,
            0,
            UniformPolicyEvaluator::new(Side::Corp),
            PuctConfig { iterations: 40, ..PuctConfig::default() },
        );
        let stats = agent.search(&view, &registry);

        let total: u32 = stats.actions.iter().map(|stat| stat.visits).sum();
        assert_eq!(total, 40, "every iteration descends exactly one root edge");
        let best = stats.actions.iter().max_by_key(|stat| stat.visits).expect("search reports its actions");
        assert!(best.visits > 0);
        assert_eq!(
            stats.visit_counts[best.index.expect("a plain Corp-turn action encodes against its own state")],
            best.visits,
            "visit_counts must agree with the per-action stats"
        );
    }
}
