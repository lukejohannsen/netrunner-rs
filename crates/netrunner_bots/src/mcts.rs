use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{
    apply_action, current_actor, legal_actions as engine_legal_actions, GamePhase, GameState, PlayerAction, Side,
};
use netrunner_core::view::ClientView;

use crate::agent::BotAgent;
use crate::determinize::determinize;
use crate::eval::{evaluate_state_with, Weights};
use crate::personality::Personality;
use crate::puct::{pick_action, ActionStat, CycleGuard, ESCAPE_RNG_SALT};

// Both `Node::new` (per expansion) and `rollout` (per rollout ply) call
// `netrunner_core::rules::legal_actions`, which itself validates every
// candidate by running `apply_action` on a cloned state — so each rollout
// ply costs roughly (candidate count) `apply_action` calls, not one. A
// budget of `iterations * max_depth` naively suggests the cost, but the
// real cost is closer to `iterations * max_depth * average_branching`
// `apply_action`-equivalents per tree. These defaults are kept small enough
// that a single decision stays well under a second even in an unoptimized
// debug build; `with_config` is there for callers (e.g. a future gym) that
// want to trade wall-clock for search strength deliberately.
const DEFAULT_ITERATIONS: usize = 64;
const DEFAULT_MAX_DEPTH: usize = 16;
const DEFAULT_EXPLORATION: f64 = std::f64::consts::SQRT_2;
const MAX_TREES: usize = 4;

/// Information Set MCTS over `netrunner_core`'s own `apply_action`/
/// `legal_actions`: each of `trees` independent, single-threaded searches
/// (root-parallel — no shared mutable tree, no locking) determinizes its
/// *own* concrete `GameState` sample from the current `ClientView` (see
/// `determinize`) before searching — that per-tree resampling of hidden
/// information is what makes this ISMCTS rather than plain perfect-info
/// MCTS. Every tree's root expands the identical, already-correctly-
/// side-filtered `view.legal_actions` (not a freshly recomputed
/// `legal_actions` on the sample, which would be redundant — ownership
/// filtering doesn't depend on hidden info); everything below the root
/// (expansion/rollout, representing hypothetical continuations from that
/// tree's own sample) uses the engine's normal `legal_actions` exactly as a
/// perfect-information search would. Root-level visit/value stats are
/// merged by `PlayerAction` equality once all trees finish.
pub struct MctsAgent {
    side: Side,
    iterations: usize,
    max_depth: usize,
    exploration: f64,
    trees: usize,
    seed: u64,
    /// What `select_action` last chose, and how many times in a row — the
    /// input to `pick_action`'s cycle break. Same bookkeeping as
    /// `PuctAgent`, for the same reason: see `MAX_GREEDY_REPEATS`. A bare
    /// argmax over root visits re-picks a state-preserving action forever.
    cycle: CycleGuard,
    /// Leaf and rollout evaluation terms — `Weights::default()` unless a
    /// `Personality` was asked for through `with_personality`.
    weights: Weights,
}

impl MctsAgent {
    pub fn new(side: Side, seed: u64) -> Self {
        Self::with_iterations(side, seed, DEFAULT_ITERATIONS)
    }

    /// `new`, with only the per-decision iteration budget changed — for a
    /// harness that wants many cheap games rather than a strong opponent,
    /// without restating (and then drifting from) the other defaults.
    pub fn with_iterations(side: Side, seed: u64, iterations: usize) -> Self {
        let trees = rayon::current_num_threads().clamp(1, MAX_TREES);
        Self::with_config(side, seed, iterations, DEFAULT_MAX_DEPTH, DEFAULT_EXPLORATION, trees)
    }

    pub fn with_config(side: Side, seed: u64, iterations: usize, max_depth: usize, exploration: f64, trees: usize) -> Self {
        Self {
            side,
            iterations: iterations.max(1),
            max_depth: max_depth.max(1),
            exploration,
            trees: trees.max(1),
            seed,
            cycle: CycleGuard::default(),
            weights: Weights::default(),
        }
    }

    /// The same search, scoring leaves and rollouts with
    /// `personality.weights()`.
    pub fn with_personality(mut self, personality: Personality) -> Self {
        self.weights = personality.weights();
        self
    }
}

impl BotAgent for MctsAgent {
    fn select_action(&mut self, view: &ClientView, registry: &CardRegistry) -> PlayerAction {
        assert!(!view.legal_actions.is_empty(), "BotAgent::select_action requires at least one legal action");
        if view.legal_actions.len() == 1 {
            return view.legal_actions[0].clone();
        }

        let side = self.side;
        let max_depth = self.max_depth;
        let exploration = self.exploration;
        let trees = self.trees;
        let per_tree_iterations = (self.iterations / trees).max(1);
        let weights = self.weights;
        let base_seed = self.seed;
        self.seed = self.seed.wrapping_add(trees as u64);

        let per_tree_stats: Vec<Vec<(PlayerAction, u32, f64)>> = (0..trees)
            .into_par_iter()
            .map(|tree_index| {
                let mut rng = StdRng::seed_from_u64(base_seed.wrapping_add(tree_index as u64));
                let sample = determinize(view, registry, &mut rng);
                let mut root = Node::new_root(sample, view.legal_actions.clone());
                for _ in 0..per_tree_iterations {
                    simulate(&mut root, registry, side, max_depth, exploration, &mut rng, &weights);
                }
                root.children.into_iter().map(|(action, child)| (action, child.visits, child.total_value)).collect()
            })
            .collect();

        let mut merged: Vec<(PlayerAction, u32, f64)> = Vec::new();
        for tree_stats in per_tree_stats {
            for (action, visits, value) in tree_stats {
                match merged.iter_mut().find(|(existing, _, _)| *existing == action) {
                    Some(entry) => {
                        entry.1 += visits;
                        entry.2 += value;
                    }
                    None => merged.push((action, visits, value)),
                }
            }
        }

        // `index: None` throughout — MCTS never encodes against the
        // `ActionSpace`, and `pick_action` reads only the action, visits
        // and value.
        let stats: Vec<ActionStat> = merged
            .into_iter()
            .map(|(action, visits, total_value)| ActionStat { index: None, action, visits, total_value })
            .collect();
        // `self.seed` advanced above, so the escape draw changes per call
        // and the agent stays a pure function of its construction seed.
        let mut rng = StdRng::seed_from_u64(self.seed ^ ESCAPE_RNG_SALT);
        let avoid = self.cycle.cycling();
        let chosen = pick_action(&stats, false, &avoid, &mut rng)
            .map(|stat| stat.action.clone())
            .unwrap_or_else(|| view.legal_actions[0].clone());
        self.cycle.record(&chosen, !avoid.is_empty());
        chosen
    }
}

struct Node {
    state: GameState,
    untried: Vec<PlayerAction>,
    children: Vec<(PlayerAction, Node)>,
    visits: u32,
    total_value: f64,
}

impl Node {
    fn new(state: GameState, registry: &CardRegistry) -> Self {
        let untried = engine_legal_actions(&state, registry);
        Node { state, untried, children: Vec::new(), visits: 0, total_value: 0.0 }
    }

    /// Root-only constructor: seeds `untried` from the caller's own
    /// `ClientView::legal_actions` instead of recomputing it against
    /// `state` — see the struct doc comment on `MctsAgent`.
    fn new_root(state: GameState, legal_actions: Vec<PlayerAction>) -> Self {
        Node { state, untried: legal_actions, children: Vec::new(), visits: 0, total_value: 0.0 }
    }

    fn is_terminal(&self) -> bool {
        matches!(self.state.phase, GamePhase::GameOver(_))
    }
}

/// Select (UCT) -> expand -> rollout -> backpropagate, one simulation from
/// `node` down. Returns the value backpropagated into `node` itself so the
/// caller (a parent frame, or `select_action` for the true root) can fold
/// it into its own running total.
fn simulate(node: &mut Node, registry: &CardRegistry, side: Side, depth_budget: usize, exploration: f64, rng: &mut StdRng, w: &Weights) -> f64 {
    if node.is_terminal() || depth_budget == 0 {
        let value = evaluate_state_with(&node.state, side, registry, w);
        node.visits += 1;
        node.total_value += value;
        return value;
    }

    if let Some(action) = pop_untried(&mut node.untried, rng) {
        let value = match apply_action(&node.state, registry, action.clone()) {
            Ok((next_state, _events)) => {
                let child_depth_budget = depth_budget.saturating_sub(1);
                let rollout_value = rollout(&next_state, registry, side, child_depth_budget, rng, w);
                let mut child = Node::new(next_state, registry);
                child.visits = 1;
                child.total_value = rollout_value;
                node.children.push((action, child));
                rollout_value
            }
            // `node.untried` comes from `legal_actions` (or, at the root,
            // `view.legal_actions`), so this should never actually fail;
            // treat it as a dead branch rather than corrupting the tree
            // with an unresolved candidate.
            Err(_) => evaluate_state_with(&node.state, side, registry, w),
        };
        node.visits += 1;
        node.total_value += value;
        return value;
    }

    if node.children.is_empty() {
        let value = evaluate_state_with(&node.state, side, registry, w);
        node.visits += 1;
        node.total_value += value;
        return value;
    }

    let parent_visits = node.visits.max(1) as f64;
    // Negamax: values are from `side` throughout, so where the opponent
    // decides the edge they take is the one that *minimises* them. See
    // `PuctNode::select_edge` for the history — both searches maximised
    // the agent's value at every node until September 2026.
    let agent_to_move = current_actor(&node.state).is_none_or(|actor| actor == side);
    let chosen = node
        .children
        .iter_mut()
        .max_by(|(_, a), (_, b)| {
            uct(a, parent_visits, exploration, agent_to_move)
                .partial_cmp(&uct(b, parent_visits, exploration, agent_to_move))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("children is non-empty");
    let value = simulate(&mut chosen.1, registry, side, depth_budget - 1, exploration, rng, w);
    node.visits += 1;
    node.total_value += value;
    value
}

fn uct(child: &Node, parent_visits: f64, exploration: f64, agent_to_move: bool) -> f64 {
    let visits = child.visits.max(1) as f64;
    let exploitation = child.total_value / visits;
    let exploitation = if agent_to_move { exploitation } else { -exploitation };
    exploitation + exploration * (parent_visits.ln() / visits).sqrt()
}

fn pop_untried(untried: &mut Vec<PlayerAction>, rng: &mut StdRng) -> Option<PlayerAction> {
    if untried.is_empty() {
        return None;
    }
    let index = rng.random_range(0..untried.len());
    Some(untried.swap_remove(index))
}

/// Plays `state` forward with a lightweight, heuristically-weighted random
/// policy until `GamePhase::GameOver` or `depth_budget` is exhausted, then
/// evaluates the result. Deliberately cheaper than `HeuristicAgent`'s own
/// one-ply lookahead (no per-step `apply_action`-and-evaluate over every
/// candidate) since a rollout runs many times per expanded node.
fn rollout(start: &GameState, registry: &CardRegistry, side: Side, mut depth_budget: usize, rng: &mut StdRng, w: &Weights) -> f64 {
    let mut state = start.clone();
    while depth_budget > 0 && !matches!(state.phase, GamePhase::GameOver(_)) {
        let legal = engine_legal_actions(&state, registry);
        if legal.is_empty() {
            break;
        }
        let action = &legal[weighted_index(&legal, rng)];
        match apply_action(&state, registry, action.clone()) {
            Ok((next, _events)) => state = next,
            Err(_) => break,
        }
        depth_budget -= 1;
    }
    evaluate_state_with(&state, side, registry, w)
}

/// Rough priority weights biasing the rollout policy toward
/// game-progressing actions over idle clicks, without the cost of actually
/// evaluating each candidate's resulting state.
fn action_weight(action: &PlayerAction) -> f64 {
    match action {
        PlayerAction::ScoreAgenda { .. } | PlayerAction::StealAgenda { .. } => 8.0,
        PlayerAction::InitiateRun { .. } => 3.0,
        PlayerAction::InstallCard { .. } | PlayerAction::InstallHardware { .. } | PlayerAction::InstallProgram { .. } => 2.5,
        PlayerAction::PlayEvent { .. } | PlayerAction::PlayOperation { .. } | PlayerAction::AdvanceCard { .. } => 2.0,
        PlayerAction::RezIce { .. } => 1.5,
        PlayerAction::EndTurn => 0.5,
        _ => 1.0,
    }
}

fn weighted_index(actions: &[PlayerAction], rng: &mut StdRng) -> usize {
    let weights: Vec<f64> = actions.iter().map(action_weight).collect();
    let total: f64 = weights.iter().sum();
    let mut pick = rng.random::<f64>() * total;
    for (index, weight) in weights.iter().enumerate() {
        if pick < *weight {
            return index;
        }
        pick -= *weight;
    }
    actions.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{CardDefinition, CardId, CardType, IceType};
    use netrunner_core::rules::{
        AgendaPoints, Clicks, CorpState, Credits, EncounteredSubroutine, InstallId, InstalledCard, InstalledRunnerCard,
        MemoryUnits, PaidAbilityWindow, PlayerResources, RunIce, RunPhase, RunState, RunnerState, ServerId, SubroutineStatus,
        WindowCheckpoint,
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

    fn small_agent(side: Side) -> MctsAgent {
        MctsAgent::with_config(side, 99, 40, 8, DEFAULT_EXPLORATION, 2)
    }

    #[test]
    fn always_returns_a_member_of_legal_actions() {
        let registry = CardRegistry::new();
        let mut state = netrunner_core::rules::GameState::new(0);
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
    fn does_not_panic_mid_run_with_a_paid_ability_window_open() {
        let mut registry = CardRegistry::new();
        let mut breaker = blank_card("corroder", CardType::Program);
        registry.insert({
            breaker.side = Side::Runner;
            breaker
        });

        let mut state = netrunner_core::rules::GameState::new(0);
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
                    definition: netrunner_core::dsl::SubroutineDef {
                        text: "End the run.".to_string(),
                        effect: netrunner_core::dsl::Effect::EndTheRun,
                        only_breakable_by: None,
                    },
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

        let mut state = netrunner_core::rules::GameState::new(0);
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

        let mut agent = MctsAgent::with_config(Side::Corp, 123, 200, 10, DEFAULT_EXPLORATION, 2);
        let chosen = agent.select_action(&view, &registry);
        assert_eq!(chosen, PlayerAction::ScoreAgenda { target: InstallId(1) });
    }

    /// The bookkeeping `pick_action`'s cycle break depends on, pinned at
    /// the agent: a repeat count that grows on an identical pick, resets on
    /// a different one, and at the bound strikes the repeated action.
    /// Which action greedy prefers on this view is not pinned — rollouts
    /// vary with the advancing seed — so the test plants the repeat state
    /// rather than trying to provoke it.
    #[test]
    fn select_action_strikes_the_repeated_action_at_the_bound() {
        let registry = CardRegistry::new();
        let mut state = GameState::new(0);
        state.corp = empty_corp();
        state.runner = empty_runner();
        state.corp.resources.clicks = Clicks(3);
        state.corp.resources.credits = Credits(5);
        let view = build_client_view(&state, &registry, Side::Corp);
        assert!(view.legal_actions.len() > 1);

        let mut agent = MctsAgent::with_config(Side::Corp, 3, 16, 4, DEFAULT_EXPLORATION, 1);
        let first = agent.select_action(&view, &registry);
        assert!(agent.cycle.cycling().is_empty(), "a first pick is not a cycle");

        // Fill the window with one action, the shape the guard exists for.
        for _ in 0..=crate::puct::MAX_GREEDY_REPEATS {
            agent.cycle.record(&first, false);
        }
        assert_eq!(agent.cycle.cycling(), vec![first.clone()], "a full window of one action is a cycle");

        let escaped = agent.select_action(&view, &registry);
        assert_ne!(escaped, first, "at the bound the repeated set is struck from the candidates");
        assert!(view.legal_actions.contains(&escaped));
        assert!(agent.cycle.cycling().is_empty(), "firing clears the window that caused it");
    }

    /// UCT at an opponent node prefers the child that is worst for the
    /// agent — the negamax half of the search.
    #[test]
    fn uct_flips_sign_where_the_opponent_decides() {
        let child = |total_value: f64| Node {
            state: GameState::new(0),
            untried: Vec::new(),
            children: Vec::new(),
            visits: 10,
            total_value,
        };
        let good = child(8.0);
        let bad = child(-5.0);
        assert!(uct(&good, 20.0, DEFAULT_EXPLORATION, true) > uct(&bad, 20.0, DEFAULT_EXPLORATION, true));
        assert!(uct(&good, 20.0, DEFAULT_EXPLORATION, false) < uct(&bad, 20.0, DEFAULT_EXPLORATION, false));
    }

    #[test]
    fn single_legal_action_short_circuits() {
        let registry = CardRegistry::new();
        let mut state = netrunner_core::rules::GameState::new(0);
        state.phase = GamePhase::Mulligan(Side::Corp);
        state.corp = empty_corp();
        state.runner = empty_runner();

        let view = build_client_view(&state, &registry, Side::Corp);
        let mut agent = small_agent(Side::Corp);
        let chosen = agent.select_action(&view, &registry);
        assert!(view.legal_actions.contains(&chosen));
    }
}
