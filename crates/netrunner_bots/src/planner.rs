//! The turn planner: a seat that plans its whole turn and then plays it
//! (Phase 5 §25 Stage 4).
//!
//! **Why a turn and not a ply.** The one-ply chooser (`heuristic`) applies
//! each legal action to one sample of the hidden state and takes the best
//! score, and a static score cannot want something that takes three
//! clicks to be worth anything: an agenda installed into a remote is an
//! exposed agenda, worth less than a credit, until it has been advanced
//! twice and scored — so the chooser never installs it, and the baseline
//! found a score in the turn it was installed 0.017 of the time (§25
//! Stage 1). A plan is a line of actions judged where the line ends, so
//! "install, advance, advance, score" is judged at the score.
//!
//! **What it does at the first decision of its own action phase:** draws
//! one sample of the hidden state, as the chooser does, and searches a
//! beam of lines over it with the opponent frozen — every window the
//! opponent is handed is answered with a pass (`settle`), a decision only
//! the opponent can make ends the line, and a run ends the line the moment
//! it starts: the run is priced as a leaf by the evaluator's mid-run terms
//! and is then played one decision at a time, as it is today, because the
//! ICE the sample imagined is not the ICE the run will meet. The best line
//! is kept and played one action per decision **for as long as the board
//! is the one the line predicted** — each step carries the legal actions
//! the sample offered at that point, and a view whose list differs (a
//! draw brought a different card, the opponent rezzed something, a prompt
//! the sample did not foresee) throws the rest of the line away and plans
//! again from what is there. Every other decision — inside a run, inside a
//! prompt the plan did not make, on the opponent's turn — is the one-ply
//! chooser's, through the same function (`heuristic::choose_one_ply`), so
//! the planner differs from the reference only where it plans.
//!
//! **The beam keeps two things** (`prune`): the `PLAN_BEAM` best partial
//! lines by the score of where they stand, and **the best line under each
//! first action** — because the from-hand line's first action, the
//! install, is the worst-scoring first action on the board by a static
//! reading, and a beam pruned by that reading alone drops it at the first
//! ply and never learns what it was for. A greedy continuation from every
//! first action costs one one-ply choice a click and is what finds it.
//! The whole search is capped at `PLAN_BUDGET` applications of the engine
//! a plan; a Corp with a full hand and five servers would otherwise expand
//! past what a desktop decision can wait for.
//!
//! **A click left over is worth a credit, and a hair more** (`click_floor`):
//! a line that ends before its clicks are spent — a run on the first
//! click, a decision handed to the opponent — is scored where it stands
//! plus one credit's worth of score for each click it has not spent,
//! which is the least a click buys (the basic action) and what the rest of
//! the turn will be re-planned to. Without it a run at the first click
//! would lose to four credit clicks by three credits every time; with it
//! the comparison is the one the one-ply chooser already makes, a run
//! against a credit. The hair (`KEPT_CLICK_EDGE`) is for the tie that
//! comparison leaves — "run, then three credits" and "three credits, then
//! run" score the same to the jitter — and it goes to the click kept,
//! because the turn after a run is planned again knowing what the run
//! found, and the same credits clicked afterwards are the same credits. It
//! is well under a credit, so a line that spends a click on anything at
//! all still beats leaving it. The guide's rate for a click is Stage 5's
//! term in the evaluator; this is the planner's floor until the evaluator
//! carries one.
//!
//! **A parked payment is answered inside the line, never left for the
//! beam.** An install that trashes first, a cost that takes cards, a
//! payment split between pools: the engine parks each as a question and
//! the answer replays the action (`rules::payment`), and the evaluator
//! prices a parked state by resolving every chain of answers and taking
//! the best (`fundamentals::through_parked_payment`) — a product over the
//! asks in the chain. Left as nodes, the beam expanded that tree while
//! the evaluator re-priced its remainder at every node: a Runner with
//! Smartware Distributors and Telework Contracts on the table spent 30 s
//! on one plan, most of it pricing "install this program over that one"
//! four cards and two pools deep. So `settle` answers the chain once
//! (`answer_payment`), as the evaluator would, records each answer as a
//! forced step with the answers the engine will offer, and goes on from
//! the paid state; the pool split alone is not enumerated, because the
//! evaluator prices only the credit pool and so always keeps it.
//!
//! **An opponent's yes-or-no is answered the way that is worst for the
//! seat, and the line goes on** (`opponents_answer`, Stage 6). A decision
//! only the opponent can make ended the line, and a line that ended at a
//! parked paid choice was scored on the parked state with its clicks
//! unspent: Public Trail's "give the Runner 1 tag unless they pay 8[c]"
//! was worth its cost and nothing, because at the leaf no tag had been
//! given and no credits paid, and the kill plan's tag was a card the
//! planner never played (0 of 116 times it held it). So when the line
//! reaches a paid choice offered to the opponent, each of their answers
//! is applied and settled, the one that leaves the seat worst off is
//! taken as theirs, and the line continues from there — the opponent
//! frozen still, but frozen at their best answer to this one question
//! rather than at no answer. If the real answer is the other one, the
//! view is not the one predicted and the turn is planned again, which
//! is the rule every step already plays under. One node of minimax,
//! never a search of the opponent's turn.
//!
//! **The first action is chosen from the view's list, the rest from the
//! sample's.** A sample is consistent with everything the view shows and
//! carries no identity (`determinize`), so its legal actions can differ
//! from the real ones at the root — A Teia's remote limit is the
//! identity's — and the root is the one step whose real list is in hand.
//! The rest of the line is checked against the real list when it is
//! played, which is the same rule one step later.
//!
//! **What a seat plays is its style, or its identity's plan** (Stage 7):
//! a Runner seat given no style reads its own identity off the first
//! view it acts on and plays that faction's chapter (`Style::or_faction`),
//! so an unstyled deck is not a balanced one to the planner; the
//! reference reads no identity, and stays balanced on it.
//!
//! **Rejected:** planning the run through. The sample's ICE is a guess,
//! the opponent's rez is theirs, and the mid-run terms already read the
//! run from the table; a line that assumed a rez would be re-planned at
//! the encounter anyway. Rejected: a two-ply look-ahead to rank the beam
//! instead of the first-action rule — it finds the same lines at the
//! branching factor's cost. Rejected: a plan that survives a divergence
//! by skipping the step it cannot play — a line whose premise has changed
//! is a line whose end was scored on a board that no longer exists.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::PaymentAsk as Ask;
use netrunner_core::rules::{apply_action, current_actor, legal_transitions_for, GamePhase, GameState, PlayerAction, Side};
use netrunner_core::view::ClientView;

use crate::agent::{is_regressive, BotAgent};
use crate::determinize::determinize;
use crate::eval::{evaluate_state_with, Weights};
use crate::heuristic::choose_one_ply;
use crate::knowledge::Knowledge;
use crate::plans::Style;

/// Partial lines kept at each ply beside the best line under each first
/// action. Six, because a click's candidates are about a dozen and the
/// first-action rule already keeps that many; the beam's own slots are
/// for the lines whose *second* action was the good one.
pub const PLAN_BEAM: usize = 6;

/// Actions a line may hold before it is scored where it stands. Four
/// clicks with a window and a prompt behind each is sixteen; a discard
/// phase adds a handful. A line this long is a prompt walking, and the
/// floor prices what it left unspent.
pub const PLAN_MAX_LINE: usize = 24;

/// Applications of the engine one plan may spend, counted as the
/// transitions the beam expands plus the passes it settles. About the
/// cost of two dozen one-ply decisions, which is what a turn of the
/// reference chooser costs in the sweeps, so a planner seat is a few
/// times the reference and not an order of magnitude.
pub const PLAN_BUDGET: usize = 2_500;

/// What a kept click is worth over a credit, as a share of one: the
/// tie-break between running now and running after the credits (module
/// docs). Two orders of magnitude over the jitter and one under a credit.
pub const KEPT_CLICK_EDGE: f64 = 0.05;

/// The jitter the one-ply chooser adds, for the same reason: ties broken
/// by the sample's draw rather than by `legal_actions` order.
const TIE_BREAK_JITTER: f64 = 1e-3;

/// How many of the opponent's yes-or-nos a line is answered through
/// (`opponents_answer`): an answer that parks a second question is
/// answered too, and a third is where the second stands.
const ANSWER_DEPTH: u8 = 2;

/// One action of a line, with the legal actions the sample offered where
/// it was chosen — the prediction a real view is checked against before
/// the action is played.
#[derive(Debug, Clone)]
struct Step {
    expected: Vec<PlayerAction>,
    action: PlayerAction,
}

/// A line being played: the steps, how many have been played, and the
/// turn it was made in (a line never outlives its turn).
#[derive(Debug, Clone)]
struct Plan {
    steps: Vec<Step>,
    next: usize,
    turn: u32,
}

/// How the planner has been deciding — what a measurement quotes beside
/// the win rate: a plan whose lines hold is a plan; one re-planned every
/// decision is a slower one-ply chooser.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlanStats {
    /// Plans made from a fresh sample.
    pub planned: u32,
    /// Decisions played from a standing plan without re-planning.
    pub followed: u32,
    /// Plans thrown away because the view was not the one predicted.
    pub diverged: u32,
    /// Decisions handed to the one-ply chooser (runs, prompts, the
    /// opponent's turn, a plan the beam could not make).
    pub one_ply: u32,
    /// Engine applications spent planning, summed over every plan.
    pub applications: u64,
}

/// The seat that plans its turn. `new`, `with_style` and
/// `with_knowledge` are `HeuristicAgent`'s, so a driver seats either the
/// same way.
pub struct PlanningAgent {
    side: Side,
    rng: StdRng,
    /// The style the seat was given, kept beside the weights it resolves
    /// to because the faction it may default to is read off the first
    /// view (`weights_for`).
    style: Style,
    weights: Weights,
    /// Whether `weights` has been resolved against the seat's own
    /// identity (`Style::or_faction`, Stage 7). Once, since the identity
    /// is public and set for the game.
    resolved: bool,
    knowledge: Knowledge,
    plan: Option<Plan>,
    stats: PlanStats,
}

impl PlanningAgent {
    pub fn new(side: Side, seed: u64) -> Self {
        Self::with_style(side, seed, Style::BALANCED)
    }

    /// `new`, scoring with `style.planned_weights()`: the first plan's
    /// profile at the guide's rate (`Weights::at_the_guides_rate`, Stage
    /// 5) with every plan the style stacks switched on
    /// (`Weights::with_plans`, Stage 6) — and, for a Runner seat whose
    /// style names no plan, its identity's faction's plan
    /// (`Style::or_faction`, Stage 7), read off the first view the seat
    /// acts on. The economy and the plans are the planner's; the one-ply
    /// reference keeps the profile alone, so the planner is measured
    /// against a chooser that has not moved.
    pub fn with_style(side: Side, seed: u64, style: Style) -> Self {
        Self {
            side,
            rng: StdRng::seed_from_u64(seed),
            style,
            weights: style.planned_weights(side),
            resolved: false,
            knowledge: Knowledge::default(),
            plan: None,
            stats: PlanStats::default(),
        }
    }

    /// The style the seat plays, once its own identity has been read:
    /// the one it was given, or its faction's plan when it was given
    /// none. For a report and a test.
    pub fn style(&self) -> Style {
        self.style
    }

    /// Resolves the weights against the seat's own identity in `view`
    /// the first time a view is seen; the identity is public in both
    /// chairs' views, so a seat with no deck to read still knows its
    /// faction. A fixture with no identity keeps the style as given.
    fn weights_for(&mut self, view: &ClientView, registry: &CardRegistry) {
        if self.resolved {
            return;
        }
        let identity = match self.side {
            Side::Corp => view.corp.identity.as_ref(),
            Side::Runner => view.runner.identity.as_ref(),
        };
        let faction = identity.and_then(|id| registry.get(id)).and_then(|def| def.faction);
        self.style = self.style.or_faction(self.side, faction);
        self.weights = self.style.planned_weights(self.side);
        self.resolved = true;
    }

    /// The same planner, sampling from what `knowledge` admits.
    pub fn with_knowledge(mut self, knowledge: Knowledge) -> Self {
        self.knowledge = knowledge;
        self
    }

    pub fn stats(&self) -> PlanStats {
        self.stats
    }

    /// Whether `view` is a decision to plan from: the seat's own action
    /// phase with nothing parked and no window open. Everything else is
    /// played one ply.
    fn plannable(&self, view: &ClientView) -> bool {
        view.phase == GamePhase::Action(self.side)
            && view.active_player == self.side
            && view.active_run.is_none()
            && view.paid_ability_window.is_none()
            && view.active_trace.is_none()
            && view.pending_prevention.is_none()
            && view.pending_paid_choice.is_none()
            && view.pending_decision.is_none()
            && view.pending_payment.is_none()
    }

    /// The next action of the standing plan, if the view is the one it
    /// predicted. A plan is followed only into the view its sample
    /// offered the same legal actions on: the list is what a hidden card
    /// changes (a draw, a rez, a prompt), and it is compared whole.
    fn follow(&mut self, view: &ClientView) -> Option<PlayerAction> {
        let plan = self.plan.as_mut()?;
        if plan.turn != view.turn || plan.next >= plan.steps.len() {
            self.plan = None;
            return None;
        }
        let step = &plan.steps[plan.next];
        if step.expected != view.legal_actions {
            self.plan = None;
            self.stats.diverged += 1;
            return None;
        }
        plan.next += 1;
        self.stats.followed += 1;
        Some(step.action.clone())
    }

    /// Searches the beam from `root` and keeps the best line. `None` when
    /// no line could be made, which is a root with no progressive action
    /// — not a state a seat is asked to act on.
    fn plan(&mut self, root: GameState, view: &ClientView, registry: &CardRegistry) -> Option<PlayerAction> {
        let mut search = Search {
            side: self.side,
            registry,
            weights: &self.weights,
            root_turn: root.turn,
            root_legal: &view.legal_actions,
            applications: 0,
            answering: 0,
            rng: &mut self.rng,
        };
        let steps = search.best_line(root);
        let turn = view.turn;
        self.stats.applications += search.applications as u64;
        let steps = steps?;
        let first = steps[0].action.clone();
        self.plan = Some(Plan { steps, next: 1, turn });
        self.stats.planned += 1;
        Some(first)
    }
}

impl BotAgent for PlanningAgent {
    fn select_action(&mut self, view: &ClientView, registry: &CardRegistry) -> PlayerAction {
        assert!(!view.legal_actions.is_empty(), "BotAgent::select_action requires at least one legal action");
        if let Some(action) = self.follow(view) {
            return action;
        }
        self.weights_for(view, registry);
        let sample = determinize(view, registry, &self.knowledge, &mut self.rng);
        if self.plannable(view)
            && let Some(action) = self.plan(sample.clone(), view, registry)
        {
            return action;
        }
        self.stats.one_ply += 1;
        choose_one_ply(view, registry, &sample, self.side, &self.weights, &mut self.rng)
    }

    fn observe(&mut self, view: &ClientView) {
        self.knowledge.observe(view);
    }
}

/// Where a settled state stands, for the line that reached it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Standing {
    /// The seat's own decision, in its own turn, with nothing parked for
    /// the opponent: the line goes on.
    Open,
    /// The turn is over, or the game is: the line is scored as it stands.
    Ended,
    /// The line cannot be planned further — a run has begun, or the
    /// opponent has a decision no pass answers — and is scored where it
    /// stands plus the floor for its unspent clicks.
    Leaf,
}

/// A partial line and the state it has reached.
struct Node {
    state: GameState,
    steps: Vec<Step>,
    score: f64,
}

/// A finished line and its score.
struct Finished {
    steps: Vec<Step>,
    score: f64,
}

struct Search<'a> {
    side: Side,
    registry: &'a CardRegistry,
    weights: &'a Weights,
    root_turn: u32,
    /// The view's own legal actions, which the root expands and nothing
    /// else — see `expand`.
    root_legal: &'a [PlayerAction],
    applications: usize,
    /// How many opponent's answers deep `settle` is (`opponents_answer`).
    answering: u8,
    rng: &'a mut StdRng,
}

impl Search<'_> {
    fn best_line(&mut self, root: GameState) -> Option<Vec<Step>> {
        let mut finished: Vec<Finished> = Vec::new();
        let mut frontier = vec![Node { score: self.score(&root), state: root, steps: Vec::new() }];
        while !frontier.is_empty() {
            let mut next: Vec<Node> = Vec::new();
            for node in frontier {
                if self.applications >= PLAN_BUDGET || node.steps.len() >= PLAN_MAX_LINE {
                    if !node.steps.is_empty() {
                        finished.push(Finished { score: self.leaf_score(&node.state), steps: node.steps });
                    }
                    continue;
                }
                self.expand(node, &mut next, &mut finished);
            }
            frontier = prune(next);
        }
        finished.into_iter().max_by(|a, b| a.score.total_cmp(&b.score)).map(|best| best.steps)
    }

    /// Every progressive action from `node`, each settled and sorted into
    /// `next` (open) or `finished` (ended, or a leaf). A node with one
    /// way forward is walked through rather than branched on — a window
    /// the seat can only pass, a prompt at its maximum — so a forced move
    /// costs the beam nothing.
    fn expand(&mut self, node: Node, next: &mut Vec<Node>, finished: &mut Vec<Finished>) {
        let mut state = node.state;
        let mut steps = node.steps;
        loop {
            let mut transitions = legal_transitions_for(&state, self.registry, self.side);
            self.applications += transitions.len();
            // At the root the real list is known, and it is the one the
            // first action is chosen from — the one-ply chooser's own rule
            // ("every candidate from the view, applied to the sample").
            // The sample carries no identity (`determinize` says why), so
            // it can offer what the real state refuses: a third remote
            // under A Teia's limit, played unchecked, was the 256-seed
            // sweep's seed 106. A sample that offers less than the view
            // only narrows the choice.
            let at_root = steps.is_empty();
            if at_root {
                transitions.retain(|(action, _, _)| self.root_legal.contains(action));
            }
            let expected: Vec<PlayerAction> =
                if at_root { self.root_legal.to_vec() } else { transitions.iter().map(|(action, _, _)| action.clone()).collect() };
            let mut children: Vec<(PlayerAction, GameState)> = transitions
                .into_iter()
                .filter(|(action, _, _)| !is_regressive(action, state.pending_decision.as_ref()))
                .map(|(action, next, _)| (action, next))
                .collect();
            if children.is_empty() {
                // Nothing progressive to do — a prompt with only deselects,
                // which `agent::progressive` says cannot happen. Score
                // what is there rather than lose the line.
                if !steps.is_empty() {
                    finished.push(Finished { score: self.leaf_score(&state), steps });
                }
                return;
            }
            let forced = children.len() == 1 && steps.len() < PLAN_MAX_LINE;
            if forced {
                let (action, child) = children.pop().expect("one child");
                steps.push(Step { expected, action });
                let (settled, standing) = self.settle(child, &mut steps);
                match standing {
                    Standing::Open => {
                        state = settled;
                        continue;
                    }
                    Standing::Ended => finished.push(Finished { score: self.score(&settled), steps }),
                    Standing::Leaf => finished.push(Finished { score: self.leaf_score(&settled), steps }),
                }
                return;
            }
            for (action, child) in children {
                let mut line = steps.clone();
                line.push(Step { expected: expected.clone(), action });
                let (settled, standing) = self.settle(child, &mut line);
                match standing {
                    Standing::Open => next.push(Node { score: self.score(&settled), state: settled, steps: line }),
                    Standing::Ended => finished.push(Finished { score: self.score(&settled), steps: line }),
                    Standing::Leaf => finished.push(Finished { score: self.leaf_score(&settled), steps: line }),
                }
            }
            return;
        }
    }

    /// The opponent frozen: every priority handed to them is a pass,
    /// and so is any the seat is handed once its turn is over. Stops at
    /// the seat's next decision in its own turn, at the end of the turn,
    /// at a run, or at a decision only the opponent can make.
    fn settle(&mut self, mut state: GameState, steps: &mut Vec<Step>) -> (GameState, Standing) {
        loop {
            if matches!(state.phase, GamePhase::GameOver(_)) {
                return (state, Standing::Ended);
            }
            if state.active_run.is_some() {
                return (state, Standing::Leaf);
            }
            if state.pending_payment.as_ref().is_some_and(|payment| payment.side == self.side) {
                // A parked payment of the seat's own is answered here, as
                // forced steps of the line, never left for the beam — see
                // the module docs. `None` when no chain of answers goes
                // through, which `payment::could_ask` says cannot happen
                // for a parked action.
                let Some((_, path)) = self.answer_payment(&state) else { return (state, Standing::Leaf) };
                for answer in path {
                    if steps.len() >= PLAN_MAX_LINE {
                        return (state, Standing::Leaf);
                    }
                    let expected = netrunner_core::rules::legal_actions_for(&state, self.registry, self.side);
                    self.applications += expected.len() + 1;
                    let Ok((next, _)) = apply_action(&state, self.registry, answer.clone()) else { return (state, Standing::Leaf) };
                    steps.push(Step { expected, action: answer });
                    state = next;
                }
                continue;
            }
            let turn_over = state.turn != self.root_turn
                || matches!(state.phase, GamePhase::Action(side) | GamePhase::Discard { side, .. } | GamePhase::StartOfTurn(side) if side != self.side);
            match current_actor(&state) {
                None => return (state, Standing::Ended),
                Some(actor) if actor != self.side || turn_over => {
                    self.applications += 1;
                    match apply_action(&state, self.registry, PlayerAction::PassPriority { side: actor }) {
                        Ok((next, _)) => state = next,
                        // The opponent has to decide something — a
                        // choice a card of ours hands them — or the seat
                        // is asked something in the opponent's turn. The
                        // line ends here either way; what follows is
                        // theirs, or one ply's. A yes-or-no of theirs is
                        // answered their best way and the line goes on
                        // (module docs).
                        Err(_) => {
                            if !turn_over
                                && let Some(answered) = self.opponents_answer(&state)
                            {
                                return answered;
                            }
                            return (state, if turn_over { Standing::Ended } else { Standing::Leaf });
                        }
                    }
                }
                Some(_) => return (state, Standing::Open),
            }
        }
    }

    /// The best chain of answers to the payment parked on `state`, and
    /// the score of the state it leaves: each answer applied, the chain
    /// followed while the result is still parked for the seat, the
    /// unparked end scored — the evaluator's own reading of a parked
    /// payment (`fundamentals::through_parked_payment`), with the path
    /// kept and one difference: **a pool split is not enumerated.** The
    /// most the question allows from the class it names is the one
    /// answer tried (the evaluator prices the credit pool alone, so it
    /// would pick it anyway), and the rest only if that one is refused.
    /// Which card a cost takes, which of a card's printed prices, and an
    /// X are real choices and are all tried.
    fn answer_payment(&mut self, state: &GameState) -> Option<(f64, Vec<PlayerAction>)> {
        let payment = state.pending_payment.as_ref()?;
        let ask = &payment.question;
        let mut answers = ask.answers();
        if let Ask::Pools(question) = ask {
            answers.retain(|answer| *answer != question.max);
            answers.insert(0, question.max);
        }
        let mut best: Option<(f64, Vec<PlayerAction>)> = None;
        for answer in answers {
            let action = ask.action_for(answer);
            self.applications += 1;
            let Ok((next, _)) = apply_action(state, self.registry, action.clone()) else { continue };
            let found = if next.pending_payment.as_ref().is_some_and(|p| p.side == self.side) {
                self.answer_payment(&next).map(|(score, mut path)| {
                    path.insert(0, action);
                    (score, path)
                })
            } else {
                Some((self.score(&next), vec![action]))
            };
            if let Some(found) = found {
                if best.as_ref().is_none_or(|(score, _)| found.0 > *score) {
                    best = Some(found);
                }
                if matches!(ask, Ask::Pools(_)) {
                    break;
                }
            }
        }
        best
    }

    /// The opponent's answer to the paid choice parked on `state` that
    /// leaves the seat worst off — each answer applied and settled, the
    /// settled states scored where they stand — with the state it
    /// settles to and where that stands, so the line goes on from it;
    /// `None` when nothing of the kind is parked. Nested at most
    /// `ANSWER_DEPTH` deep, so an answer that parks another question is
    /// answered too, and a third is where the second stands.
    fn opponents_answer(&mut self, state: &GameState) -> Option<(GameState, Standing)> {
        let opponent = self.side.other();
        if state.pending_paid_choice.as_ref()?.side != opponent || self.answering >= ANSWER_DEPTH {
            return None;
        }
        let answers: Vec<PlayerAction> = netrunner_core::rules::legal_actions_for(state, self.registry, opponent)
            .into_iter()
            .filter(|action| matches!(action, PlayerAction::AcceptPendingPaidChoice { .. } | PlayerAction::DeclinePendingPaidChoice))
            .collect();
        self.applications += answers.len();
        let mut worst: Option<(f64, GameState, Standing)> = None;
        self.answering += 1;
        for answer in answers {
            let Ok((next, _)) = apply_action(state, self.registry, answer) else { continue };
            // The answer's own steps are the opponent's, not the line's:
            // the next step's expectation is read off the answered state.
            let mut theirs = Vec::new();
            let (settled, standing) = self.settle(next, &mut theirs);
            let score = match standing {
                Standing::Ended => self.score(&settled),
                Standing::Open | Standing::Leaf => self.leaf_score(&settled),
            };
            if worst.as_ref().is_none_or(|(w, _, _)| score < *w) {
                worst = Some((score, settled, standing));
            }
        }
        self.answering -= 1;
        worst.map(|(_, settled, standing)| (settled, standing))
    }

    fn score(&mut self, state: &GameState) -> f64 {
        evaluate_state_with(state, self.side, self.registry, self.weights) + self.rng.random::<f64>() * TIE_BREAK_JITTER
    }

    /// The score where a line stands, plus the floor for the clicks it
    /// has not spent — see the module docs.
    fn leaf_score(&mut self, state: &GameState) -> f64 {
        self.score(state) + click_floor(state, self.side, self.weights)
    }
}

/// One credit's worth of score, and the edge, for each unspent click —
/// the least a click buys, and the preference for keeping it. What the
/// evaluator already prices a click at (`Weights::click_weight`, the
/// guide's rate since Stage 5) is not paid twice: at that rate the floor
/// is the edge alone.
fn click_floor(state: &GameState, side: Side, weights: &Weights) -> f64 {
    let unpriced = (weights.own_credit_weight - weights.click_weight).max(0.0);
    f64::from(state.resources(side).clicks.0) * (unpriced + weights.own_credit_weight * KEPT_CLICK_EDGE)
}

/// `PLAN_BEAM` best nodes, plus the best node under each first action not
/// among them — see the module docs for why the first action is kept.
fn prune(mut nodes: Vec<Node>) -> Vec<Node> {
    nodes.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Node> = Vec::new();
    let mut first_actions: Vec<PlayerAction> = Vec::new();
    for node in nodes {
        let first = &node.steps[0].action;
        let new_first = !first_actions.contains(first);
        if kept.len() < PLAN_BEAM || new_first {
            if new_first {
                first_actions.push(first.clone());
            }
            kept.push(node);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::dsl::{CardDefinition, CardId, CardType, IceType};
    use netrunner_core::rules::{
        AgendaPoints, Clicks, CorpState, Credits, InstallId, InstallSlot, InstalledCard, MemoryUnits, PlayerResources,
        RunnerState, ServerId,
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

    fn ice(id: u32, server: ServerId) -> InstalledCard {
        InstalledCard {
            card: CardId("wall".to_string()),
            install_id: InstallId(id),
            server,
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        }
    }

    /// A Corp with a two-point agenda needing two advancements in hand,
    /// three clicks and the credits, its centrals iced and no remote:
    /// the from-hand line is install, advance, advance, score in one
    /// turn, and the install on its own is a naked agenda.
    fn corp_with_a_scorable_hand(registry: &mut CardRegistry) -> GameState {
        let mut agenda = blank_card("agenda", CardType::Agenda);
        agenda.advancement_requirement = Some(2);
        agenda.agenda_points = Some(2);
        registry.insert(agenda);
        registry.insert(blank_card("wall", CardType::Ice(IceType::Barrier)));
        registry.insert(blank_card("filler", CardType::Operation));
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.turn = 5;
        // A real game's counter is past every id on the table.
        state.next_install_id = 20;
        state.runner = empty_runner();
        state.corp = CorpState {
            resources: PlayerResources { credits: Credits(8), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
            hq: vec![CardId("agenda".to_string()), CardId("filler".to_string())],
            r_and_d: vec![CardId("filler".to_string()); 10],
            installed: vec![
                ice(10, ServerId::Hq),
                ice(11, ServerId::Hq),
                ice(12, ServerId::RnD),
                ice(13, ServerId::RnD),
                ice(14, ServerId::Archives),
            ],
            ..Default::default()
        };
        state
    }

    /// Plays `agent` through its own turn on `state`, returning every
    /// action it took until the turn passed.
    fn play_turn(agent: &mut PlanningAgent, mut state: GameState, registry: &CardRegistry) -> (Vec<PlayerAction>, GameState) {
        let side = Side::Corp;
        let mut played = Vec::new();
        for _ in 0..40 {
            if !matches!(state.phase, GamePhase::Action(s) | GamePhase::Discard { side: s, .. } if s == side) {
                break;
            }
            let view = build_client_view(&state, registry, side);
            if view.legal_actions.is_empty() {
                break;
            }
            agent.observe(&view);
            let action = agent.select_action(&view, registry);
            assert!(view.legal_actions.contains(&action), "{action:?} is not legal");
            played.push(action.clone());
            state = apply_action(&state, registry, action).expect("the plan's action applies").0;
        }
        (played, state)
    }

    /// The line the one-ply chooser never finds (§25 Stage 1: a score in
    /// the turn of its install 0.017 of the time): install into the
    /// fort, advance twice, score — and the whole line is followed from
    /// one plan, because nothing hidden moved.
    #[test]
    fn scores_an_agenda_from_hand_in_one_turn_and_follows_the_line() {
        let mut registry = CardRegistry::new();
        let state = corp_with_a_scorable_hand(&mut registry);
        let view = build_client_view(&state, &registry, Side::Corp);
        let mut one_ply = crate::heuristic::HeuristicAgent::new(Side::Corp, 1);
        assert!(
            !matches!(one_ply.select_action(&view, &registry), PlayerAction::InstallCard { .. }),
            "the reference chooser does not install a naked agenda"
        );

        let mut planner = PlanningAgent::new(Side::Corp, 1);
        let (played, after) = play_turn(&mut planner, state, &registry);
        assert!(
            matches!(played.first(), Some(PlayerAction::InstallCard { zone: ServerId::Remote(0), .. })),
            "installs the agenda first: {played:?}"
        );
        assert_eq!(played.iter().filter(|a| matches!(a, PlayerAction::AdvanceCard { .. })).count(), 2, "{played:?}");
        assert!(played.iter().any(|a| matches!(a, PlayerAction::ScoreAgenda { .. })), "{played:?}");
        assert_eq!(after.corp.resources.agenda_points, AgendaPoints(2));
        let stats = planner.stats();
        assert_eq!(stats.planned, 1, "{stats:?}");
        assert_eq!(stats.diverged, 0, "{stats:?}");
        assert!(stats.followed >= 3, "{stats:?}");
        assert!(stats.applications <= PLAN_BUDGET as u64, "{stats:?}");
    }

    /// A draw is the hidden card the sample guessed: the line planned on
    /// the sample's draw is dropped when the real draw differs, and the
    /// turn is planned again from the card that came. The plan is also
    /// never followed across a turn.
    #[test]
    fn re_plans_when_the_board_is_not_the_one_predicted() {
        let mut registry = CardRegistry::new();
        let mut state = corp_with_a_scorable_hand(&mut registry);
        // Nothing to score: a hand of nothing and a deck of something
        // worth drawing into, so the plan opens with a draw.
        let mut asset = blank_card("asset", CardType::Asset);
        asset.cost = 0;
        registry.insert(asset);
        state.corp.hq.clear();
        state.corp.r_and_d = vec![CardId("asset".to_string()); 6];
        state.corp.resources.credits = Credits(2);
        let view = build_client_view(&state, &registry, Side::Corp);
        let mut planner = PlanningAgent::new(Side::Corp, 2);
        let first = planner.select_action(&view, &registry);
        assert!(planner.plan.is_some());
        // A view from the next turn is never the plan's, whatever it
        // offers.
        let mut other = state.clone();
        other.turn += 1;
        other.phase = GamePhase::Action(Side::Runner);
        let runner_view = build_client_view(&other, &registry, Side::Corp);
        assert!(planner.follow(&runner_view).is_none());
        assert!(planner.plan.is_none(), "a plan never outlives its turn");
        // And a view of the same turn whose legal actions differ from
        // the predicted ones is not followed.
        let (after, _) = apply_action(&state, &registry, first).unwrap();
        planner.plan = Some(Plan {
            steps: vec![Step { expected: vec![PlayerAction::EndTurn], action: PlayerAction::EndTurn }],
            next: 0,
            turn: after.turn,
        });
        let view = build_client_view(&after, &registry, Side::Corp);
        assert_ne!(view.legal_actions, vec![PlayerAction::EndTurn]);
        let chosen = planner.select_action(&view, &registry);
        assert!(view.legal_actions.contains(&chosen));
        assert_eq!(planner.stats().diverged, 1);
    }

    /// A run ends the line where it starts: the planner initiates the run
    /// and then plays it one ply at a time, and the decisions inside it
    /// are not counted as followed.
    #[test]
    fn a_run_is_a_leaf_and_is_played_one_ply_inside() {
        let mut registry = CardRegistry::new();
        let mut agenda = blank_card("agenda", CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(2);
        registry.insert(agenda);
        registry.insert(blank_card("filler", CardType::Operation));
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.turn = 4;
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(4), agenda_points: AgendaPoints(0) };
        state.runner.grip = vec![CardId("filler".to_string()); 3];
        state.corp.hq = vec![CardId("filler".to_string()); 2];
        state.corp.r_and_d = vec![CardId("filler".to_string()); 5];
        state.corp.installed.push(InstalledCard {
            card: CardId("agenda".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            advancement_tokens: 2,
            ..Default::default()
        });
        let view = build_client_view(&state, &registry, Side::Runner);
        let mut planner = PlanningAgent::new(Side::Runner, 3);
        let chosen = planner.select_action(&view, &registry);
        assert_eq!(chosen, PlayerAction::InitiateRun { server: ServerId::Remote(0) }, "the advanced remote is the run");
        let plan = planner.plan.as_ref().expect("a plan was made");
        assert_eq!(plan.steps.len(), 1, "the run is the end of the line: {:?}", plan.steps);
        let (mut state, _) = apply_action(&state, &registry, chosen).unwrap();
        let mut inside = 0;
        for _ in 0..30 {
            if state.active_run.is_none() {
                break;
            }
            let Some(actor) = current_actor(&state) else { break };
            let view = build_client_view(&state, &registry, actor);
            let action = match actor {
                Side::Runner => {
                    inside += 1;
                    planner.select_action(&view, &registry)
                }
                Side::Corp => PlayerAction::PassPriority { side: Side::Corp },
            };
            state = apply_action(&state, &registry, action).unwrap().0;
        }
        assert!(inside > 0, "the Runner decided inside the run");
        assert_eq!(planner.stats().followed, 0);
        assert_eq!(planner.stats().one_ply, inside);
    }

    /// The beam's pruning keeps the best line under every first action,
    /// however it scores, and the best `PLAN_BEAM` overall.
    #[test]
    fn pruning_keeps_the_beam_and_every_first_action() {
        let node = |first: u32, score: f64| Node {
            state: GameState::new(0),
            steps: vec![Step { expected: Vec::new(), action: PlayerAction::AdvanceCard { target: InstallId(first) } }],
            score,
        };
        let mut nodes = Vec::new();
        for first in 0..10 {
            for k in 0..3 {
                nodes.push(node(first, f64::from(first * 3 + k)));
            }
        }
        let kept = prune(nodes);
        let firsts: Vec<u32> = kept
            .iter()
            .filter_map(|n| match n.steps[0].action {
                PlayerAction::AdvanceCard { target } => Some(target.0),
                _ => None,
            })
            .collect();
        // The top six are 29, 28, 27 (first 9) and 26, 25, 24 (first 8);
        // then one each for firsts 7..0.
        assert_eq!(kept.len(), PLAN_BEAM + 8, "{firsts:?}");
        for first in 0..10 {
            assert!(firsts.contains(&first), "first action {first} was pruned: {firsts:?}");
        }
        assert!(kept.windows(2).all(|pair| pair[0].score >= pair[1].score), "kept in score order");
    }

    /// A Runner seat given no style reads its plan off its own identity
    /// on the first view — a Criminal is played under pressure — and a
    /// seat given a style keeps it; a fixture with no identity stays
    /// balanced. The reference reads nothing of the kind.
    #[test]
    fn a_runner_seat_reads_its_plan_off_its_own_identity() {
        use crate::plans::{Plan, Style};
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.turn = 4;
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(4), agenda_points: AgendaPoints(0) };
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 3];
        state.runner.stack = vec![CardId("sure_gamble".to_string()); 10];
        state.corp.hq = vec![CardId("hedge_fund".to_string()); 4];
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 20];
        let seated = |identity: Option<&str>, style: Style| {
            let mut state = state.clone();
            state.runner.identity = identity.map(|id| CardId(id.to_string()));
            let view = build_client_view(&state, &registry, Side::Runner);
            let mut planner = PlanningAgent::with_style(Side::Runner, 3, style);
            assert_eq!(planner.style(), style, "as given, until a view is seen");
            let chosen = planner.select_action(&view, &registry);
            assert!(view.legal_actions.contains(&chosen));
            (planner.style(), planner.weights)
        };
        let (style, weights) = seated(Some("zahya_sadeghi"), Style::BALANCED);
        assert_eq!(style, Style::of(Plan::Pressure), "a Criminal identity");
        assert_eq!(weights, Style::of(Plan::Pressure).planned_weights(Side::Runner));
        assert_eq!(seated(Some("rene_loup_arcemont"), Style::BALANCED).0, Style::of(Plan::Dismantle), "an Anarch identity");
        assert_eq!(seated(Some("zahya_sadeghi"), Style::of(Plan::Rig)).0, Style::of(Plan::Rig), "the style given overrides the identity");
        assert_eq!(seated(None, Style::BALANCED).0, Style::BALANCED, "no identity to read");
        assert_eq!(seated(Some("the_catalyst"), Style::BALANCED).0, Style::BALANCED, "a neutral identity has no chapter");
    }

    /// The kill plan's lever: Public Trail's "give the Runner 1 tag unless
    /// they pay 8[c]" is a choice only the Runner makes, so a line that
    /// played it ended at the parked choice and was worth its cost.
    /// Answered the Runner's worst-for-the-Corp way — a tag, since they
    /// cannot pay — and continued, a kill Corp holding Scorched Earth
    /// against a grip of three, with the credits to play it next turn
    /// but not this one, tags first: the threat is priced. A balanced
    /// Corp, which reads no leverage in a tag, does not; and the kill
    /// Corp does not either against a grip the damage would not reach,
    /// because a tag's leverage alone is under Public Trail's price.
    /// (With the credits to play Scorched Earth in the same turn, every
    /// planner tags: the line kills inside the turn and scores the win.)
    /// Public Trail's shape without its "play only if" (a successful run
    /// last turn), which a fixture cannot write.
    #[test]
    fn a_kill_corp_plays_public_trail_because_the_runners_answer_is_priced() {
        use crate::plans::{Plan, Style};
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let trail_card: CardDefinition = serde_json::from_str(r#"{
            "id": "public_trail_open", "title": "Public Trail", "side": "Corp", "card_type": "Operation", "cost": 4,
            "triggers": [{ "trigger": "OnPlay", "subject": "This", "effects": [{ "OfferPaidChoice": {
                "side": "Runner", "cost": { "Credits": 8 }, "if_paid": { "Sequence": [] },
                "if_declined": { "GiveTags": { "Fixed": 1 } }, "text": "Give the Runner 1 tag unless they pay 8[credit]" } }] }],
            "is_playable": true }"#).expect("a card file");
        registry.insert(trail_card);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.turn = 6;
        state.next_install_id = 20;
        state.runner = empty_runner();
        state.runner.resources.credits = Credits(3);
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 3];
        state.runner.stack = vec![CardId("sure_gamble".to_string()); 10];
        state.corp = CorpState {
            resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
            hq: vec![CardId("public_trail_open".to_string()), CardId("scorched_earth".to_string()), CardId("hedge_fund".to_string())],
            r_and_d: vec![CardId("hedge_fund".to_string()); 10],
            installed: vec![ice(10, ServerId::Hq), ice(11, ServerId::RnD)],
            ..Default::default()
        };
        for card in &mut state.corp.installed {
            card.card = CardId("palisade".to_string());
        }
        let view = build_client_view(&state, &registry, Side::Corp);
        let trail = Some(PlayerAction::PlayOperation { card_id: CardId("public_trail_open".to_string()) });
        assert!(view.legal_actions.contains(trail.as_ref().unwrap()), "the tag is playable: {:?}", view.legal_actions);
        // The line, not its first action: the Hedge Fund may come first.
        let plans_to_tag = |agent: &mut PlanningAgent, view: &ClientView| {
            agent.select_action(view, &registry);
            agent.plan.as_ref().is_some_and(|plan| plan.steps.iter().any(|step| Some(&step.action) == trail.as_ref()))
        };
        assert!(plans_to_tag(&mut PlanningAgent::with_style(Side::Corp, 1, Style::of(Plan::Kill)), &view), "the kill Corp tags this turn");
        assert!(!plans_to_tag(&mut PlanningAgent::new(Side::Corp, 1), &view), "a balanced Corp sees nothing in the tag");
        let mut safe = state.clone();
        safe.runner.grip = vec![CardId("sure_gamble".to_string()); 6];
        let view = build_client_view(&safe, &registry, Side::Corp);
        assert!(!plans_to_tag(&mut PlanningAgent::with_style(Side::Corp, 1, Style::of(Plan::Kill)), &view), "no threat against a full grip, and a tag alone is under the price");
    }

    #[test]
    fn always_returns_a_member_of_legal_actions() {
        let mut registry = CardRegistry::new();
        let state = corp_with_a_scorable_hand(&mut registry);
        let view = build_client_view(&state, &registry, Side::Corp);
        let mut agent = PlanningAgent::new(Side::Corp, 2);
        let chosen = agent.select_action(&view, &registry);
        assert!(view.legal_actions.contains(&chosen));
    }
}

#[cfg(test)]
mod cost {
    //! `cargo test --release -p netrunner_bots -- --ignored planner_cost --nocapture`:
    //! what a planner seat costs over a few sample-deck games, and how
    //! its lines hold — the numbers the §25 Stage 4 entry quotes.
    use super::*;
    use netrunner_core::decks;
    use netrunner_core::format::NsgFormat;

    #[test]
    #[ignore]
    fn planner_cost() {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        for (n, (corp_deck, runner_deck)) in decks::matchups().iter().take(6).enumerate() {
            let seed = n as u64 + 1;
            let (mut state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed).unwrap();
            let knowing = |deck: &decks::DeckFile| Knowledge::new(NsgFormat::Casual, Some(deck.to_deck()));
            let mut corp = PlanningAgent::new(Side::Corp, seed).with_knowledge(knowing(corp_deck));
            let mut runner = PlanningAgent::new(Side::Runner, seed).with_knowledge(knowing(runner_deck));
            let mut slowest = 0.0_f64;
            let started = std::time::Instant::now();
            for _ in 0..6000 {
                if matches!(state.phase, GamePhase::GameOver(_)) {
                    break;
                }
                let Some(actor) = current_actor(&state) else { break };
                let view = netrunner_core::view::build_client_view(&state, &registry, actor);
                if view.legal_actions.is_empty() {
                    break;
                }
                let agent = if actor == Side::Corp { &mut corp } else { &mut runner };
                agent.observe(&view);
                let asked = std::time::Instant::now();
                let action = agent.select_action(&view, &registry);
                slowest = slowest.max(asked.elapsed().as_secs_f64());
                state = apply_action(&state, &registry, action).unwrap().0;
            }
            eprintln!(
                "game {n} {}/{} turn {} {:.1}s, slowest decision {slowest:.2}s\n  corp {:?}\n  runner {:?}",
                corp_deck.id,
                runner_deck.id,
                state.turn,
                started.elapsed().as_secs_f64(),
                corp.stats(),
                runner.stats()
            );
        }
    }

    /// The floor is what the evaluator does not already price: a credit's
    /// worth and the edge at the reference weights, the edge alone at the
    /// guide's rate, where the click term carries the credit.
    #[test]
    fn the_click_floor_pays_only_what_the_evaluator_does_not() {
        use netrunner_core::rules::Clicks;
        let mut state = GameState::new(0);
        state.corp.resources.clicks = Clicks(2);
        let reference = Weights::default();
        let guide = reference.at_the_guides_rate();
        assert!((click_floor(&state, Side::Corp, &reference) - 2.0 * reference.own_credit_weight * (1.0 + KEPT_CLICK_EDGE)).abs() < 1e-9);
        assert!((click_floor(&state, Side::Corp, &guide) - 2.0 * guide.own_credit_weight * KEPT_CLICK_EDGE).abs() < 1e-9);
    }
}
