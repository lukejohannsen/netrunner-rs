//! The turn planner: a seat that plans its whole turn and then plays it
//! (Phase 5 §25 Stage 4).
//!
//! **Why a turn and not a ply.** A one-ply chooser (`one_ply`) applies
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
//! choice (`one_ply`): every legal action applied to the sample and the
//! best score taken, which was the whole of the reference chooser.
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
//! **And it keeps one line per position** (`prune`, Phase 5 §26): two
//! orders of the same clicks — advance then ICE, ICE then advance — reach
//! one state and one future, and a beam that held both held three
//! positions in six slots. That is how a glacier Corp with three clicks,
//! 5[c] and an unadvanced 3/2 behind two pieces of ICE played advance,
//! ICE, advance and scored next turn: "advance, advance" missed the last
//! slot by a tenth of a point, a ply before the free score it led to —
//! not the beam's width (twelve tried) nor its budget. A line that
//! reaches the state a kept line reached is dropped, its first action
//! counted as kept, and the slot goes to a line that reaches somewhere
//! else; the engine keeps the turn log's same-action table sorted so the
//! two orders are one `GameState`.
//!
//! **A line still being built is judged by the free score it leads to**
//! (`judged`, §26): a third advance is a third token to the evaluator and
//! a point to the game, and the beam judged the line before the score.
//! So an open line's score for the beam is the better of where it stands
//! and where a score from there would leave it, tried on each installed
//! agenda with a token on; the score is still a step of its own at the
//! next ply, and a finished line is scored where it ends. Alone it does
//! not find the line above — "advance, advance" is dropped a ply before
//! any free action exists — and with the positions folded it lifts the
//! third advance to the top of the beam a ply early, which is what a
//! crowded beam needs; measured as a Corp gain, alone on the same games
//! (the §26 entry). The Runner has nothing free of the kind to walk
//! through: the pool's [click]-less abilities are a breaker's inside a
//! run, an interrupt, or a trash-self. Rejected: walking every free
//! action through by probing each open node's legal list — the probe
//! applies every candidate, a dozen a node over some two hundred nodes a
//! ply, several times the budget.
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
//! so an unstyled deck is not a balanced one to the planner.
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
use netrunner_core::dsl::CardType;
use netrunner_core::rules::PaymentAsk as Ask;
use netrunner_core::rules::{apply_action, current_actor, legal_transitions_for, GamePhase, GameState, InstallId, PlayerAction, Side};
use netrunner_core::view::ClientView;

use crate::agent::{is_regressive, BotAgent};
use crate::determinize::determinize;
use crate::eval::{evaluate_state_with, Weights};
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
    /// Decisions made one ply (runs, prompts, the opponent's turn, a
    /// plan the beam could not make).
    pub one_ply: u32,
    /// Engine applications spent planning, summed over every plan.
    pub applications: u64,
}

/// The seat that plans its turn: the bot a person meets at every rung of
/// the ladder (`difficulty`), and the one every measurement seats.
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
    /// acts on.
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
        one_ply(view, registry, &sample, self.side, &self.weights, &mut self.rng)
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
                    Standing::Open => next.push(Node { score: self.judged(&settled), state: settled, steps: line }),
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

    /// What a line still being built is judged by for the beam: where it
    /// stands, or where a free score from there would leave it, whichever
    /// is better — see the module docs ("a free action is walked through
    /// before a line is judged"). The score is tried on each installed
    /// agenda and the engine says which are ready; a refused score costs
    /// nothing, and an installed agenda is a card or two. The line is not
    /// moved: the score is still a step of its own at the next ply, and a
    /// finished line is scored where it ends.
    fn judged(&mut self, state: &GameState) -> f64 {
        let mut best = self.score(state);
        if self.side != Side::Corp {
            return best;
        }
        let agendas: Vec<InstallId> = state
            .corp
            .installed
            .iter()
            .filter(|card| card.advancement_tokens > 0 && self.registry.get(&card.card).is_some_and(|def| def.card_type == CardType::Agenda))
            .map(|card| card.install_id)
            .collect();
        for target in agendas {
            self.applications += 1;
            if let Ok((scored, _)) = apply_action(state, self.registry, PlayerAction::ScoreAgenda { target }) {
                best = best.max(self.score(&scored));
            }
        }
        best
    }

    /// The score where a line stands, plus the floor for the clicks it
    /// has not spent — see the module docs.
    fn leaf_score(&mut self, state: &GameState) -> f64 {
        self.score(state) + click_floor(state, self.side, self.weights)
    }
}

/// The one-ply choice: every legal action applied to `sample`, the result
/// scored by `weights` for `side`, the best taken — what the planner plays
/// wherever it does not plan (a run, a prompt, the opponent's turn). It was
/// the whole of `HeuristicAgent`, the fixed reference every stage of the
/// rebuild was measured against until Stage 8 deleted it (the planner had
/// beaten it on both chairs); the function is the planner's own now, and
/// the ladder's every rung is the planner.
fn one_ply(
    view: &ClientView,
    registry: &CardRegistry,
    sample: &GameState,
    side: Side,
    weights: &Weights,
    rng: &mut StdRng,
) -> PlayerAction {
    let mut best: Option<(f64, usize)> = None;
    for (index, action) in view.legal_actions.iter().enumerate() {
        // A deselect scores exactly like the select it undoes — the
        // board is identical — so the jitter decides, and a one-ply
        // chooser can walk a card selection forever. See
        // `agent::is_regressive`.
        if crate::agent::is_regressive(action, view.pending_decision.as_ref()) {
            continue;
        }
        let Ok((next, _events)) = apply_action(sample, registry, action.clone()) else { continue };
        let score = evaluate_state_with(&next, side, registry, weights) + rng.random::<f64>() * TIE_BREAK_JITTER;
        if best.is_none_or(|(best_score, _)| score > best_score) {
            best = Some((score, index));
        }
    }

    // `view.legal_actions` came from `legal_actions_for`, whose
    // ownership filtering doesn't depend on hidden info (see its doc
    // comment), so every candidate above should already succeed
    // against the determinized `sample` too — falling back to the
    // first entry only guards against a hypothetical future
    // divergence, not an expected case.
    best.map_or_else(
        || crate::agent::progressive(&view.legal_actions, view.pending_decision.as_ref())[0].clone(),
        |(_, index)| view.legal_actions[index].clone(),
    )
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
/// among them — see the module docs for why the first action is kept —
/// with **one node per position**: a line that reaches the state a kept
/// line has already reached is the same future under another order of
/// the same clicks, and is dropped. Two states are compared only when
/// their scores are within the jitter, since the evaluator is a function
/// of the state; a full comparison of every pair would cost more than
/// the ply it prunes.
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
            if kept.iter().any(|k| (k.score - node.score).abs() <= TIE_BREAK_JITTER && k.state == node.state) {
                // The first action is recorded all the same: the kept
                // twin stands for it, and a weaker line that merely
                // begins the same way would be no addition.
                continue;
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
    pub(super) fn play_turn(agent: &mut PlanningAgent, mut state: GameState, registry: &CardRegistry) -> (Vec<PlayerAction>, GameState) {
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
        let mut rng = StdRng::seed_from_u64(1);
        let sample = determinize(&view, &registry, &Knowledge::default(), &mut rng);
        assert!(
            !matches!(one_ply(&view, &registry, &sample, Side::Corp, &Weights::default(), &mut rng), PlayerAction::InstallCard { .. }),
            "one ply does not install a naked agenda"
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

    /// One node per position: a line that reaches the state a kept line
    /// reached is the same clicks in another order and is dropped, and
    /// its first action counts as kept — the twin stands for it — so the
    /// slot goes to a line that reaches somewhere else. A state that is
    /// merely scored alike is not a twin.
    #[test]
    fn pruning_folds_two_orders_of_the_same_clicks_into_one_line() {
        let state_with = |credits: u32| {
            let mut state = GameState::new(0);
            state.corp.resources.credits = Credits(credits);
            state
        };
        let node = |first: u32, credits: u32, score: f64| Node {
            state: state_with(credits),
            steps: vec![Step { expected: Vec::new(), action: PlayerAction::AdvanceCard { target: InstallId(first) } }],
            score,
        };
        // Enough distinct positions to fill the beam; then six twins of
        // one position, each under its own first action, scored within
        // the jitter of each other; a different position scored the same
        // as one of them; a poorer line under the first action of one of
        // the twins; and a poorer line under a first action of its own.
        let mut nodes: Vec<Node> = (0..PLAN_BEAM as u32).map(|n| node(100 + n, 20 + n, 20.0 + f64::from(n))).collect();
        nodes.extend((0..6).map(|first| node(first, 5, 10.0 + f64::from(first) * TIE_BREAK_JITTER / 10.0)));
        nodes.push(node(6, 7, 10.0));
        nodes.push(node(1, 3, 1.0));
        nodes.push(node(7, 9, 0.5));
        let kept = prune(nodes);
        let describe: Vec<(u32, u32)> = kept
            .iter()
            .skip(PLAN_BEAM)
            .map(|n| match n.steps[0].action {
                PlayerAction::AdvanceCard { target } => (target.0, n.state.corp.resources.credits.0),
                _ => unreachable!(),
            })
            .collect();
        // After the beam: the 5[c] position once, under the best-scored
        // twin; the 7[c] position; the 9[c] line for its first action —
        // and not the 3[c] line, whose first action a twin stands for.
        assert_eq!(describe, vec![(5, 5), (6, 7), (7, 9)], "{describe:?}");
    }

    /// An open line is judged by the better of where it stands and the
    /// free score from there: a third advance is a third token to the
    /// evaluator and a point to the game, and the beam judged it before
    /// the score it led to. A finished line is still scored where it ends.
    #[test]
    fn an_open_line_is_judged_by_the_free_score_it_leads_to() {
        let mut registry = CardRegistry::new();
        let state = corp_with_a_scorable_hand(&mut registry);
        let mut ready = state.clone();
        ready.corp.installed.push(InstalledCard {
            card: CardId("agenda".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            advancement_tokens: 2,
            ..Default::default()
        });
        let mut short = ready.clone();
        short.corp.installed.last_mut().expect("the agenda").advancement_tokens = 1;
        let weights = Weights::default();
        let mut rng = StdRng::seed_from_u64(1);
        let legal = Vec::new();
        let mut search = Search {
            side: Side::Corp,
            registry: &registry,
            weights: &weights,
            root_turn: state.turn,
            root_legal: &legal,
            applications: 0,
            answering: 0,
            rng: &mut rng,
        };
        let scored = apply_action(&ready, &registry, PlayerAction::ScoreAgenda { target: InstallId(1) }).expect("ready to score").0;
        let judged = search.judged(&ready);
        assert!(judged > search.score(&ready) + 1.0, "judged by the point the free score wins");
        assert!((judged - search.score(&scored)).abs() < 2.0 * TIE_BREAK_JITTER, "and by nothing more");
        assert!((search.judged(&short) - search.score(&short)).abs() < 2.0 * TIE_BREAK_JITTER, "a token short, the score is refused and the line stands where it is");
        assert_eq!(search.applications, 2, "one try per advanced agenda");
        let mut runner = Search { side: Side::Runner, ..search };
        assert!((runner.judged(&ready) - runner.score(&ready)).abs() < 2.0 * TIE_BREAK_JITTER, "nothing is free for the Runner");
        assert_eq!(runner.applications, 2, "and nothing is tried");
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
        let (style, weights) = seated(Some("zahya_sadeghi_versatile_smuggler"), Style::BALANCED);
        assert_eq!(style, Style::of(Plan::Pressure), "a Criminal identity");
        assert_eq!(weights, Style::of(Plan::Pressure).planned_weights(Side::Runner));
        assert_eq!(seated(Some("rene_loup_arcemont_party_animal"), Style::BALANCED).0, Style::of(Plan::Dismantle), "an Anarch identity");
        assert_eq!(seated(Some("zahya_sadeghi_versatile_smuggler"), Style::of(Plan::Rig)).0, Style::of(Plan::Rig), "the style given overrides the identity");
        assert_eq!(seated(None, Style::BALANCED).0, Style::BALANCED, "no identity to read");
        assert_eq!(seated(Some("the_catalyst_convention_breaker"), Style::BALANCED).0, Style::BALANCED, "a neutral identity has no chapter");
    }

    /// The kill plan's lever: Public Trail's "give the Runner 1 tag unless
    /// they pay 8[c]" is a choice only the Runner makes, so a line that
    /// played it ended at the parked choice and was worth its cost.
    /// Answered the Runner's worst-for-the-Corp way — a tag, since they
    /// cannot pay — and continued, a kill Corp holding Scorched Earth
    /// against a grip of three, with the credits to play it next turn
    /// but not this one, tags first: the threat is priced. A balanced
    /// Corp, which reads no leverage in a tag, does so only when a
    /// sample happens to make the line pay; and the kill Corp does not
    /// against a grip the damage would not reach, because a tag's
    /// leverage alone is under Public Trail's price.
    ///
    /// **Counted over twenty agent seeds, not read off one.** The test
    /// pinned seed 1 until Midnight Sun Stage 1 (2 October 2026), when
    /// eleven more cards in the pool moved that one sample and the
    /// balanced Corp tagged. On `main` before the stage the balanced
    /// Corp already tagged on 7 seeds of 20 (6 after it), the kill Corp
    /// on all 20 and, against the full grip, on none: the claim is the
    /// difference between the plans, which one seed could only witness.
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
        const SEEDS: u64 = 20;
        let tagging = |plan: Option<Plan>, view: &ClientView| {
            (1..=SEEDS)
                .filter(|&seed| {
                    let mut agent = match plan {
                        Some(plan) => PlanningAgent::with_style(Side::Corp, seed, Style::of(plan)),
                        None => PlanningAgent::new(Side::Corp, seed),
                    };
                    plans_to_tag(&mut agent, view)
                })
                .count() as u64
        };
        assert_eq!(tagging(Some(Plan::Kill), &view), SEEDS, "the kill Corp tags this turn, whatever the sample");
        let balanced = tagging(None, &view);
        assert!(balanced * 2 < SEEDS, "a balanced Corp sees nothing in the tag but a lucky sample: {balanced} of {SEEDS}");
        let mut safe = state.clone();
        safe.runner.grip = vec![CardId("sure_gamble".to_string()); 6];
        let view = build_client_view(&safe, &registry, Side::Corp);
        assert_eq!(tagging(Some(Plan::Kill), &view), 0, "no threat against a full grip, and a tag alone is under the price");
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

/// The positions the one-ply reference was pinned on, played by the
/// planner: each is a decision the evaluator makes in one ply (inside a
/// run, at a prompt) or a turn the plan opens with, and the planner is
/// held to the same choice.
#[cfg(test)]
mod positions {
    use super::*;
    use crate::plans::Style;
    use netrunner_core::dsl::{CardDefinition, CardId, CardType};
    use netrunner_core::rules::{
        AgendaPoints, Clicks, CorpState, Credits, GamePhase, InstallId, InstalledCard, MemoryUnits, PlayerResources,
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

    /// A Corp state with 3 clicks, an installed Agenda already advanced to
    /// meet its scoring requirement, and one other legal click action
    /// (`GainCreditClick`) — `ScoreAgenda` should dominate `evaluate_state`
    /// since it's worth an immediate agenda-point swing while the other
    /// candidate is worth nothing.
    fn corp_state_with_scorable_agenda(registry: &mut CardRegistry) -> GameState {
        let mut agenda = blank_card("winning_agenda", CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(2);
        registry.insert(agenda);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.runner = empty_runner();
        state.corp = CorpState {
            resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
            installed: vec![InstalledCard {
                card: CardId("winning_agenda".to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                advancement_tokens: 3,
                ..Default::default()
            }],
            ..Default::default()
        };
        state
    }

    /// Every Corp plan finishes the fort around a point it cannot win this
    /// turn: a 3/2 behind one piece of ICE with the centrals iced, an ICE
    /// in HQ and two clicks, and each plan ends the turn with the second
    /// piece in front and one token on — the fort terms every profile
    /// carries since Phase 5 §23, when the agenda here was naked and rush
    /// advanced it anyway. Judged by where the turn ends: the beam folds
    /// two orders of the same clicks into one line (§26), so which is
    /// played first is the jitter's. This was "a fast-advance Corp
    /// advances where a glacier Corp installs ICE", a first click read
    /// off a three-click turn — and once the beam could see the free
    /// score, every plan won the point (`behind_two_pieces_of_ice_…`),
    /// and on every variant tried with the point out of reach (two
    /// clicks, a 4/2, a 5/2, naked or behind one piece) the four plans
    /// ended the turn in the same place: the first click the test had
    /// pinned was two orders of one line, told apart by the jitter.
    #[test]
    fn every_corp_plan_finishes_the_fort_around_a_point_it_cannot_win_this_turn() {
        let mut registry = CardRegistry::new();
        let mut agenda = blank_card("agenda", CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(2);
        registry.insert(agenda);
        registry.insert(blank_card("wall", CardType::Ice(netrunner_core::dsl::IceType::Barrier)));

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.runner = empty_runner();
        state.corp = CorpState {
            resources: PlayerResources { credits: Credits(5), clicks: Clicks(2), agenda_points: AgendaPoints(0) },
            hq: vec![CardId("wall".to_string())],
            installed: std::iter::once(InstalledCard {
                card: CardId("agenda".to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                ..Default::default()
            })
            .chain(
                [ServerId::Remote(0), ServerId::Hq, ServerId::Hq, ServerId::RnD, ServerId::RnD, ServerId::Archives]
                    .into_iter()
                    .enumerate()
                    .map(|(n, server)| InstalledCard {
                        card: CardId("wall".to_string()),
                        install_id: InstallId(10 + n as u32),
                        server,
                        slot: netrunner_core::rules::InstallSlot::Ice,
                        ..Default::default()
                    }),
            )
            .collect(),
            ..Default::default()
        };
        for plan in [crate::plans::Plan::FastAdvance, crate::plans::Plan::Glacier, crate::plans::Plan::Traps, crate::plans::Plan::Kill] {
            let mut agent = PlanningAgent::with_style(Side::Corp, 1, Style::of(plan));
            let (played, after) = super::tests::play_turn(&mut agent, state.clone(), &registry);
            let agenda = after.corp.installed.iter().find(|card| card.install_id == InstallId(1)).expect("the agenda stays installed");
            let ice_in_front = after.corp.installed.iter().filter(|card| card.server == ServerId::Remote(0) && card.slot == netrunner_core::rules::InstallSlot::Ice).count();
            assert_eq!((agenda.advancement_tokens, ice_in_front), (1, 2), "{plan:?}: (tokens, ICE in front) after {played:?}");
        }
    }

    /// PT Untaian's discard-phase offer, which the Corp declined every
    /// one of the 332 times it was made across 192 heuristic-vs-heuristic
    /// games: pay 1[c] and put an advancement token on an installed card.
    /// Accepting hands the Corp a `PromptChooseCards`, so before
    /// `PENDING_DECISION_UPSIDE_WEIGHT` the accept was charged the
    /// prompt's penalty on top of the credit while declining resolved to
    /// nothing and was free.
    #[test]
    fn accepts_a_paid_choice_that_buys_an_advancement_token() {
        use netrunner_core::dsl::{CardFilter, CardZoneRef, Cost, Effect};
        use netrunner_core::rules::{PendingPaidChoice, PendingPaidChoiceResume};
        let mut registry = CardRegistry::new();
        let mut agenda = blank_card("agenda", CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(2);
        registry.insert(agenda);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.runner = empty_runner();
        state.corp = CorpState {
            resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
            installed: vec![InstalledCard {
                card: CardId("agenda".to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                ..Default::default()
            }],
            ..Default::default()
        };
        state.pending_paid_choice = Some(PendingPaidChoice {
            text: None,
            side: Side::Corp,
            cost: Cost::Credits(1),
            if_paid: Effect::PromptChooseCards {
                side: Side::Corp,
                source: CardZoneRef::OwnInstalled,
                filter: CardFilter::All(vec![CardFilter::Advanceable, CardFilter::Unrezzed]),
                min: 1,
                max: 1,
                reveal: false,
                shuffle_after: false,
                destination: None,
                then: Some(Box::new(Effect::PlaceAdvancementCounters(netrunner_core::dsl::Amount::Fixed(1)))),
                count: None,
            },
            if_declined: Effect::Sequence(Vec::new()),
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: PendingPaidChoiceResume::None,
        });

        let view = build_client_view(&state, &registry, Side::Corp);
        assert!(view.legal_actions.contains(&PlayerAction::DeclinePendingPaidChoice));
        let chosen = PlanningAgent::new(Side::Corp, 1).select_action(&view, &registry);
        assert_eq!(chosen, PlayerAction::AcceptPendingPaidChoice { cost_option_index: None });
    }


    /// A ready agenda is scored this turn. Not necessarily on the first
    /// click: the score is free and the turn's credits are the same
    /// credits before or after it, so the lines tie and the jitter
    /// orders them — the reference chooser, which saw one action, scored
    /// at once.
    #[test]
    fn scores_a_ready_agenda_this_turn() {
        let mut registry = CardRegistry::new();
        let state = corp_state_with_scorable_agenda(&mut registry);
        let view = build_client_view(&state, &registry, Side::Corp);
        assert!(view.legal_actions.contains(&PlayerAction::ScoreAgenda { target: InstallId(1) }));

        let mut agent = PlanningAgent::new(Side::Corp, 1);
        let (played, after) = super::tests::play_turn(&mut agent, state, &registry);
        assert!(played.contains(&PlayerAction::ScoreAgenda { target: InstallId(1) }), "{played:?}");
        assert_eq!(after.corp.resources.agenda_points, AgendaPoints(2));
    }

    /// The Runner-side counterpart: a rezzed ICE the rig cannot break
    /// makes a run worth less than a credit, and an unrezzed one does not
    /// (ROADMAP Phase 2 §5's eagerness item).
    #[test]
    fn prefers_a_credit_to_running_into_rezzed_ice_it_cannot_break() {
        use netrunner_core::dsl::{Effect, IceType, SubroutineDef};
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        let mut wall = blank_card("wall", CardType::Ice(IceType::Barrier));
        wall.strength = Some(1);
        wall.subroutines = vec![SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None }];
        registry.insert(wall);

        let state_with_ice = |rezzed| {
            let mut state = GameState::new(0);
            state.phase = GamePhase::Action(Side::Runner);
            state.runner = empty_runner();
            state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
            state.corp.resources.credits = Credits(5);
            // Something behind the ICE to run for: a run is worth its
            // hidden accesses, and an empty HQ and R&D would offer none.
            state.corp.hq = vec![CardId("wall".to_string())];
            state.corp.r_and_d = vec![CardId("wall".to_string()); 3];
            for (index, server) in [ServerId::Hq, ServerId::RnD, ServerId::Archives].into_iter().enumerate() {
                state.corp.installed.push(InstalledCard {
                    card: CardId("wall".to_string()),
                    install_id: InstallId(index as u32 + 1),
                    server,
                    slot: InstallSlot::Ice,
                    rezzed,
                    ..Default::default()
                });
            }
            state
        };

        let rezzed = state_with_ice(true);
        let view = build_client_view(&rezzed, &registry, Side::Runner);
        assert!(view.legal_actions.iter().any(|a| matches!(a, PlayerAction::InitiateRun { .. })));
        let chosen = PlanningAgent::new(Side::Runner, 3).select_action(&view, &registry);
        assert!(!matches!(chosen, PlayerAction::InitiateRun { .. }), "ran into rezzed ICE with no breaker: {chosen:?}");

        let unrezzed = state_with_ice(false);
        let view = build_client_view(&unrezzed, &registry, Side::Runner);
        let chosen = PlanningAgent::new(Side::Runner, 3).select_action(&view, &registry);
        assert!(matches!(chosen, PlayerAction::InitiateRun { .. }), "unrezzed ICE is no reason not to run: {chosen:?}");
    }

    /// With a breaker in grip it cannot yet afford, a rezzed piece of the
    /// ICE it breaks on the table and open servers to run, the Runner
    /// clicks for the credit rather than running (ROADMAP Phase 2 §5's
    /// savings item). The rezzed piece is Stage 7's condition: a breaker
    /// for ICE the Corp has never shown is not saved for.
    #[test]
    fn saves_for_a_breaker_in_grip_instead_of_running_an_open_server() {
        use netrunner_core::dsl::{Effect, IceType, SubroutineBreakCount, Trigger};
        use netrunner_core::dsl::AbilityDef;
        let mut registry = CardRegistry::new();
        let mut cleaver = blank_card("cleaver", CardType::Program);
        cleaver.side = Side::Runner;
        cleaver.cost = 3;
        cleaver.memory_cost = Some(1);
        cleaver.abilities = vec![AbilityDef {
            text: None,
            trigger: Trigger::Paid,
            cost: None,
            requirement: None,
            effect: Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: Some(IceType::Barrier) },
            cost_discount_if: None, used_by: None, access: false, from_hand: false }];
        registry.insert(cleaver);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(1), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.runner.memory_units = MemoryUnits(4);
        state.runner.grip = vec![CardId("cleaver".to_string())];
        let mut wall = blank_card("wall", CardType::Ice(IceType::Barrier));
        wall.strength = Some(1);
        wall.subroutines = vec![netrunner_core::dsl::SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None }];
        registry.insert(wall);
        state.corp.installed.push(InstalledCard {
            card: CardId("wall".to_string()),
            install_id: InstallId(1),
            server: ServerId::Hq,
            slot: netrunner_core::rules::InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        });
        state.corp.r_and_d = vec![CardId("cleaver".to_string()); 5];
        let view = build_client_view(&state, &registry, Side::Runner);
        assert!(view.legal_actions.iter().any(|a| matches!(a, PlayerAction::InitiateRun { .. })));
        assert!(view.legal_actions.contains(&PlayerAction::GainCreditClick { side: Side::Runner }));

        let chosen = PlanningAgent::new(Side::Runner, 3).select_action(&view, &registry);
        assert_eq!(chosen, PlayerAction::GainCreditClick { side: Side::Runner }, "should save for Cleaver");
    }

    /// A Matryoshka with nothing hosted breaks nothing, and a copy in the
    /// grip is the click that makes it a breaker (Phase 5 §28): every
    /// server behind a rezzed piece it could then break, and a remote with
    /// an advanced card in it, the Runner hosts the copy and runs the
    /// remote rather than clicking for credits — the line the evaluator
    /// could not see while a break paid in copies was an unpriced cost,
    /// so the planner never hosted one (0 of 144 Hit List games at seed
    /// 2, Parhelion Stage 7). The remote is what pays for the two clicks:
    /// a hosted copy has no standing value of its own, so a central's one
    /// hidden access (0.6) does not buy the host click and the run click
    /// (0.8) at the balanced rate — a host is planned only ahead of a run
    /// worth both, which is the reading §28 measured and accepted. The
    /// grip holds three other cards because an emptied grip is the
    /// flatline fear, not the point.
    #[test]
    fn hosts_a_copy_of_matryoshka_before_running() {
        use netrunner_core::dsl::{Effect, IceType, SubroutineDef};
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut wall = blank_card("wall", CardType::Ice(IceType::Sentry));
        wall.strength = Some(2);
        wall.subroutines = vec![SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None }];
        registry.insert(wall);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.runner.memory_units = MemoryUnits(2);
        state.runner.grip = vec![CardId("matryoshka".to_string()), CardId("wall".to_string()), CardId("wall".to_string()), CardId("wall".to_string())];
        state.runner.rig = vec![netrunner_core::rules::InstalledRunnerCard {
            card: CardId("matryoshka".to_string()),
            install_id: InstallId(10),
            base_strength: 2,
            ..Default::default()
        }];
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("wall".to_string())];
        state.corp.r_and_d = vec![CardId("wall".to_string()); 3];
        for (index, server) in [ServerId::Hq, ServerId::RnD, ServerId::Archives].into_iter().enumerate() {
            state.corp.installed.push(InstalledCard {
                card: CardId("wall".to_string()),
                install_id: InstallId(index as u32 + 1),
                server,
                slot: InstallSlot::Ice,
                rezzed: true,
                ..Default::default()
            });
        }
        // A remote worth the run: a face-down card with two tokens on,
        // behind a piece the hosted copy breaks.
        state.corp.installed.push(InstalledCard {
            card: CardId("wall".to_string()),
            install_id: InstallId(4),
            server: ServerId::Remote(0),
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        });
        state.corp.installed.push(InstalledCard {
            card: CardId("wall".to_string()),
            install_id: InstallId(5),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            advancement_tokens: 2,
            ..Default::default()
        });
        let view = build_client_view(&state, &registry, Side::Runner);
        let host = PlayerAction::ActivateAbility { target: InstallId(10), ability_index: 0 };
        assert!(view.legal_actions.contains(&host), "{:?}", view.legal_actions);

        // The Runner's clicks up to the run it starts; the Corp passes
        // every window it is handed in between.
        let mut agent = PlanningAgent::new(Side::Runner, 3);
        let mut actions = Vec::new();
        for _ in 0..20 {
            match current_actor(&state) {
                Some(Side::Corp) => {
                    // A window the Corp is handed is passed; its own turn
                    // means the Runner's is over.
                    let Ok((next, _)) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }) else { break };
                    state = next;
                }
                Some(Side::Runner) => {
                    let view = build_client_view(&state, &registry, Side::Runner);
                    agent.observe(&view);
                    let action = agent.select_action(&view, &registry);
                    assert!(view.legal_actions.contains(&action), "{action:?} is not legal");
                    state = apply_action(&state, &registry, action.clone()).expect("the plan's action applies").0;
                    let ran = matches!(action, PlayerAction::InitiateRun { .. });
                    actions.push(action);
                    if ran {
                        break;
                    }
                }
                None => break,
            }
        }
        assert_eq!(actions.first(), Some(&host), "should host the copy first: {actions:?}");
        assert!(matches!(actions.last(), Some(PlayerAction::InitiateRun { .. })), "and then run: {actions:?}");
        assert_eq!(state.runner.rig[0].hosted_cards.len(), 1);
    }

    /// Madani is installed for the clicks it promises on the programs the
    /// grip holds, and a program hosted on it is installed for no click
    /// (Phase 5 §30): with Madani and three breakers in hand the Runner
    /// cannot yet afford, the click puts Madani on the table rather than
    /// a credit in the pool; and with a breaker already hosted and one
    /// click left the free install is taken and the click is still spent
    /// on something else. A breaker the Runner can afford is installed
    /// outright — a strong card on the table now beats one parked at half
    /// its value — so Madani's place is the programs that are waiting:
    /// for credits, for memory, for the turns to install them one a
    /// click. Hosting itself is not pinned: it is worth the other half of
    /// each program's click, which a grip at the floor or a run worth more
    /// outbids, and is measured in play rather than asserted.
    #[test]
    fn installs_madani_for_its_promise_and_installs_a_hosted_program_free() {
        use netrunner_core::dsl::{AbilityDef, Effect, IceType, SubroutineBreakCount, Trigger};
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        for (id, subtype) in [("fracter", IceType::Barrier), ("killer", IceType::Sentry)] {
            let mut breaker = blank_card(id, CardType::Program);
            breaker.side = Side::Runner;
            breaker.cost = 1;
            breaker.memory_cost = Some(1);
            breaker.abilities = vec![AbilityDef {
                text: None,
                trigger: Trigger::Paid,
                cost: None,
                requirement: None,
                effect: Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: Some(subtype) },
                cost_discount_if: None, used_by: None, access: false, from_hand: false }];
            registry.insert(breaker);
        }
        let filler = || vec![CardId("madani".to_string()); 3];
        let base = |clicks: u32| {
            let mut state = GameState::new(0);
            state.phase = GamePhase::Action(Side::Runner);
            state.runner = empty_runner();
            state.runner.resources = PlayerResources { credits: Credits(6), clicks: Clicks(clicks), agenda_points: AgendaPoints(0) };
            state.runner.memory_units = MemoryUnits(4);
            state.corp.hq = vec![CardId("madani".to_string()); 3];
            state.corp.r_and_d = vec![CardId("madani".to_string()); 5];
            state
        };
        let play = |mut state: GameState, registry: &CardRegistry| {
            let mut agent = PlanningAgent::new(Side::Runner, 3);
            let mut actions = Vec::new();
            for _ in 0..20 {
                match current_actor(&state) {
                    Some(Side::Corp) => {
                        let Ok((next, _)) = apply_action(&state, registry, PlayerAction::PassPriority { side: Side::Corp }) else { break };
                        state = next;
                    }
                    Some(Side::Runner) => {
                        let view = build_client_view(&state, registry, Side::Runner);
                        agent.observe(&view);
                        let action = agent.select_action(&view, registry);
                        assert!(view.legal_actions.contains(&action), "{action:?} is not legal");
                        state = apply_action(&state, registry, action.clone()).expect("the plan's action applies").0;
                        actions.push(action);
                    }
                    None => break,
                }
            }
            (actions, state)
        };

        // Madani in hand beside three breakers it cannot afford, and a
        // click that would otherwise be a credit: installed.
        let mut dear = registry.clone();
        for id in ["fracter", "killer"] {
            let mut card = dear.get(&CardId(id.to_string())).expect("registered").clone();
            card.cost = 4;
            dear.insert(card);
        }
        let mut decoder = dear.get(&CardId("fracter".to_string())).expect("registered").clone();
        decoder.id = CardId("decoder".to_string());
        decoder.abilities[0].effect = Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: Some(IceType::CodeGate) };
        dear.insert(decoder);
        let mut state = base(1);
        state.runner.resources.credits = Credits(3);
        state.runner.grip = [vec![CardId("madani".to_string()), CardId("fracter".to_string()), CardId("killer".to_string()), CardId("decoder".to_string())], filler()].concat();
        let (actions, after) = play(state, &dear);
        assert!(actions.contains(&PlayerAction::InstallHardware { card_id: CardId("madani".to_string()) }), "{actions:?}");
        assert!(after.runner.rig.iter().any(|card| card.card.0 == "madani"));

        // A breaker hosted on Madani and one click: the free install, and
        // the click spent besides.
        let mut state = base(1);
        state.runner.grip = [vec![CardId("killer".to_string())], filler()].concat();
        state.runner.rig = vec![netrunner_core::rules::InstalledRunnerCard {
            card: CardId("madani".to_string()),
            install_id: InstallId(10),
            hosted_cards: vec![CardId("fracter".to_string())],
            ..Default::default()
        }];
        let install_free = PlayerAction::ActivateAbility { target: InstallId(10), ability_index: 1 };
        let (actions, after) = play(state, &registry);
        assert!(actions.contains(&install_free), "should install the hosted program free: {actions:?}");
        assert!(after.runner.rig.iter().any(|card| card.card.0 == "fracter"), "{:?}", after.runner.rig);
        assert!(
            actions.iter().any(|a| matches!(a, PlayerAction::InstallProgram { .. } | PlayerAction::GainCreditClick { .. } | PlayerAction::InitiateRun { .. } | PlayerAction::DrawCardClick { .. })),
            "the click is still spent: {actions:?}"
        );
    }

    /// With an empty grip, open servers and a stack to draw from, the
    /// Runner draws before it runs (ROADMAP Phase 2 §5's draw item).
    #[test]
    fn draws_with_an_empty_grip_instead_of_running_an_open_server() {
        let mut registry = CardRegistry::new();
        let mut filler = blank_card("filler", CardType::Resource);
        filler.side = Side::Runner;
        filler.cost = 9;
        registry.insert(filler);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.runner.stack = vec![CardId("filler".to_string()); 5];
        // A Corp with cards to draw: with R&D empty it decked out at its
        // next turn start, every turn-ending line was a won game, and the
        // pin was the jitter's (found in Phase 5 §26).
        registry.insert(blank_card("corp_filler", CardType::Operation));
        state.corp.r_and_d = vec![CardId("corp_filler".to_string()); 10];
        let view = build_client_view(&state, &registry, Side::Runner);
        assert!(view.legal_actions.iter().any(|a| matches!(a, PlayerAction::InitiateRun { .. })));
        assert!(view.legal_actions.contains(&PlayerAction::DrawCardClick { side: Side::Runner }));

        let chosen = PlanningAgent::new(Side::Runner, 3).select_action(&view, &registry);
        assert_eq!(chosen, PlayerAction::DrawCardClick { side: Side::Runner });
    }

    /// Past the floor the Runner drew only after damage. Now it draws when
    /// the card its sample puts on top of the stack is one it would
    /// install: a breaker for a subtype the rig cannot break. The sample's
    /// stack comes from the registry when no decklist is known, so the
    /// registry decides what the draw finds.
    #[test]
    fn draws_at_the_floor_when_the_stack_holds_a_breaker_and_not_when_it_holds_junk() {
        use netrunner_core::dsl::{AbilityDef, Effect, IceType, SubroutineBreakCount, Trigger};
        let runner_card = |id: &str, card_type: CardType, cost: u32| {
            let mut def = blank_card(id, card_type);
            def.side = Side::Runner;
            def.cost = cost;
            def.memory_cost = Some(1);
            def
        };
        let choose = |card: CardDefinition| {
            let mut registry = CardRegistry::new();
            registry.insert(card);
            let mut state = open_board(&mut registry);
            state.runner.memory_units = MemoryUnits(4);
            state.runner.stack = vec![CardId("filler".to_string()); 10];
            // Nothing worth running — a rezzed "End the run" barrier on
            // each central and no breaker — so the choice is between a
            // draw and a credit. It was an empty HQ and R&D, and a Corp
            // with no R&D decks out at its next turn start, so every
            // turn-ending line was a won game and the pin was the
            // jitter's (found in Phase 5 §26).
            let mut wall = blank_card("wall", CardType::Ice(IceType::Barrier));
            wall.strength = Some(1);
            wall.subroutines = vec![netrunner_core::dsl::SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None }];
            registry.insert(wall);
            state.corp.resources.credits = Credits(5);
            for (index, server) in [ServerId::Hq, ServerId::RnD, ServerId::Archives].into_iter().enumerate() {
                state.corp.installed.push(InstalledCard {
                    card: CardId("wall".to_string()),
                    install_id: InstallId(index as u32 + 1),
                    server,
                    slot: netrunner_core::rules::InstallSlot::Ice,
                    rezzed: true,
                    ..Default::default()
                });
            }
            let view = build_client_view(&state, &registry, Side::Runner);
            assert!(view.legal_actions.contains(&PlayerAction::DrawCardClick { side: Side::Runner }));
            PlanningAgent::new(Side::Runner, 3).select_action(&view, &registry)
        };
        let mut cleaver = runner_card("cleaver", CardType::Program, 3);
        cleaver.abilities = vec![AbilityDef {
            text: None,
            trigger: Trigger::Paid,
            cost: None,
            requirement: None,
            effect: Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: Some(IceType::Barrier) },
            cost_discount_if: None,
            used_by: None,
            access: false,
            from_hand: false,
        }];
        assert_eq!(choose(cleaver), PlayerAction::DrawCardClick { side: Side::Runner }, "a breaker on top is worth the draw");
        assert_eq!(
            choose(runner_card("pricey", CardType::Resource, 4)),
            PlayerAction::GainCreditClick { side: Side::Runner },
            "a card the Runner would never install is not"
        );
    }

    /// The Corp-side counterpart of the Runner's draw test: with an empty
    /// HQ and a stocked R&D, the Corp clicks to draw rather than for a
    /// credit (ROADMAP Phase 2 §5's Corp item).
    #[test]
    fn corp_draws_with_an_empty_hq_instead_of_clicking_for_a_credit() {
        let mut registry = CardRegistry::new();
        registry.insert(blank_card("filler", CardType::Asset));

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.runner = empty_runner();
        state.corp.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.corp.r_and_d = vec![CardId("filler".to_string()); 10];
        let view = build_client_view(&state, &registry, Side::Corp);
        assert!(view.legal_actions.contains(&PlayerAction::DrawCardClick { side: Side::Corp }));
        assert!(view.legal_actions.contains(&PlayerAction::GainCreditClick { side: Side::Corp }));

        let chosen = PlanningAgent::new(Side::Corp, 3).select_action(&view, &registry);
        assert_eq!(chosen, PlayerAction::DrawCardClick { side: Side::Corp });
    }

    /// With an ICE-protected remote and a naked one both open, an agenda
    /// goes behind the ICE (ROADMAP Phase 2 §5's placement item). Two
    /// pieces, because one is still an exposed agenda to every Corp
    /// profile (`EXPOSED_AGENDA_WEIGHT`, Phase 5 §23), and a credit click
    /// beats it.
    #[test]
    fn installs_an_agenda_behind_ice_rather_than_into_a_naked_remote() {
        use netrunner_core::dsl::IceType;
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        let mut agenda = blank_card("agenda", CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(2);
        registry.insert(agenda);
        registry.insert(blank_card("wall", CardType::Ice(IceType::Barrier)));
        registry.insert(blank_card("filler", CardType::Operation));

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.runner = empty_runner();
        state.corp.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.corp.r_and_d = vec![CardId("filler".to_string()); 10];
        state.corp.hq = vec![CardId("agenda".to_string()), CardId("filler".to_string()), CardId("filler".to_string())];
        // Remote 0 has ICE and an empty root; remote 1 is a naked empty root
        // (an ICE-less remote is represented by nothing at all, so the
        // "naked" option is the fresh remote the engine always offers).
        for n in 1..=2 {
            state.corp.installed.push(InstalledCard {
                card: CardId("wall".to_string()),
                install_id: InstallId(n),
                server: ServerId::Remote(0),
                slot: InstallSlot::Ice,
                ..Default::default()
            });
        }
        let view = build_client_view(&state, &registry, Side::Corp);
        let agenda_installs: Vec<_> = view
            .legal_actions
            .iter()
            .filter(|a| matches!(a, PlayerAction::InstallCard { card_id, slot: InstallSlot::Root, .. } if card_id.0 == "agenda"))
            .collect();
        assert!(agenda_installs.len() >= 2, "expected both a protected and a naked remote on offer: {agenda_installs:?}");

        // Judged over the turn: the planner may draw first — its sample
        // puts one of the registry's three cards on top of R&D, and a
        // wall there is worth the draw — and the beam holds one line per
        // position (§26), so which click the install falls on is the
        // jitter's. Where the agenda goes is not.
        let mut agent = PlanningAgent::new(Side::Corp, 3);
        let (played, after) = super::tests::play_turn(&mut agent, state, &registry);
        let installed = after.corp.installed.iter().find(|card| card.card.0 == "agenda").unwrap_or_else(|| panic!("the agenda was not installed: {played:?}"));
        assert_eq!(installed.server, ServerId::Remote(0), "should install the agenda behind the ICE: {played:?}");
    }

    /// A Runner at the floor with credits, open centrals and something to
    /// find in each of them: the position the three tests below vary.
    fn open_board(registry: &mut CardRegistry) -> GameState {
        registry.insert(blank_card("filler", CardType::Operation));
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.runner.grip = vec![CardId("filler".to_string()); 3];
        state.corp.hq = vec![CardId("filler".to_string()); 2];
        state.corp.r_and_d = vec![CardId("filler".to_string()); 5];
        state
    }

    /// Urtica Cipher's shape: an asset that deals damage when accessed.
    fn ambush(id: &str) -> CardDefinition {
        use netrunner_core::dsl::{DamageType, Effect, Trigger, TriggeredEffect};
        let mut def = blank_card(id, CardType::Asset);
        def.triggers = vec![TriggeredEffect {
            subject: None, when: None, acts_on_subject: false, first_each_turn: false, first_each_encounter: false, granted: false, from_heap: false,
            text: None,
            trigger: Trigger::OnAccessed,
            effects: vec![Effect::DealDamage(DamageType::Net, 2)],
            requirement: None,
        }];
        def
    }

    /// The person's first desktop game (ROADMAP Phase 7 §3): the
    /// `operator` Runner ran Archives three times into a face-up Urtica
    /// Cipher. With the ambush face-up beside a card it has not seen, the
    /// Runner runs somewhere else.
    #[test]
    fn does_not_run_archives_into_an_ambush_it_can_see() {
        use netrunner_core::rules::ArchivedCard;
        let mut registry = CardRegistry::new();
        registry.insert(ambush("urtica_cipher"));
        let mut state = open_board(&mut registry);
        state.corp.archives = vec![
            ArchivedCard::faceup(CardId("urtica_cipher".to_string())),
            ArchivedCard { card: CardId("filler".to_string()), facedown: true },
        ];
        let view = build_client_view(&state, &registry, Side::Runner);
        assert!(view.legal_actions.contains(&PlayerAction::InitiateRun { server: ServerId::Archives }));
        for seed in 1..=8 {
            let chosen = PlanningAgent::new(Side::Runner, seed).select_action(&view, &registry);
            assert!(matches!(chosen, PlayerAction::InitiateRun { server } if server != ServerId::Archives), "seed {seed}: {chosen:?}");
        }
        // Face down, the same card is one more thing to see, and Archives
        // is as good a run as any central.
        state.corp.archives[0].facedown = true;
        let view = build_client_view(&state, &registry, Side::Runner);
        let runs_archives = (1..=8).any(|seed| {
            PlanningAgent::new(Side::Runner, seed).select_action(&view, &registry) == PlayerAction::InitiateRun { server: ServerId::Archives }
        });
        assert!(runs_archives, "two unseen cards in Archives outrank one in HQ");
    }

    /// A faceup agenda in Archives is worth running for only when the
    /// Runner can pay to steal it. Under a rezzed Magistrate Revontulet
    /// (3[credit] more to steal) with no credits, the one-ply Runner ran
    /// Archives four times a turn, passed the agenda each time and never
    /// clicked for a credit, until the game ran out of steps (the 256-seed
    /// view sweep, seed 120, Paid Content against Borrowed Time).
    #[test]
    fn runs_archives_for_a_faceup_agenda_only_when_it_can_pay_to_steal_it() {
        use netrunner_core::cards::register_playable_cards;
        use netrunner_core::rules::{ArchivedCard, InstallId, InstallSlot, InstalledCard};
        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        let mut state = open_board(&mut registry);
        state.corp.archives = vec![ArchivedCard::faceup(CardId("orbital_superiority".to_string()))];
        state.corp.installed = vec![InstalledCard {
            install_id: InstallId(40),
            card: CardId("magistrate_revontulet".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: true,
            ..Default::default()
        }];
        let runs_archives = |state: &GameState| {
            let view = build_client_view(state, &registry, Side::Runner);
            (1..=8).any(|seed| PlanningAgent::new(Side::Runner, seed).select_action(&view, &registry) == PlayerAction::InitiateRun { server: ServerId::Archives })
        };
        assert!(runs_archives(&state), "with 5[credit] the steal is paid for");
        state.runner.resources.credits = Credits(0);
        assert!(!runs_archives(&state), "with nothing, the agenda cannot be stolen");
    }

    /// A trap the Runner has sprung is one it can see (`InstalledCard::
    /// seen_by_runner`): the same advanced face-down Urtica that draws the
    /// run while unseen — two tokens are an agenda about to score, as far
    /// as the Runner knows — is left alone once it has been accessed.
    #[test]
    fn does_not_run_back_into_a_trap_it_has_sprung() {
        use netrunner_core::dsl::{Amount, DamageType, Effect};
        let mut registry = CardRegistry::new();
        let mut urtica = ambush("urtica_cipher");
        urtica.advancement_requirement = Some(0);
        urtica.triggers[0].effects.push(Effect::DealDamageAmount(DamageType::Net, Amount::HostedAdvancementTokens));
        registry.insert(urtica);
        let mut state = open_board(&mut registry);
        state.corp.installed.push(InstalledCard {
            card: CardId("urtica_cipher".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            advancement_tokens: 2,
            ..Default::default()
        });
        let runs_it = |state: &GameState| {
            let view = build_client_view(state, &registry, Side::Runner);
            (1..=4).filter(|seed| {
                PlanningAgent::new(Side::Runner, *seed).select_action(&view, &registry)
                    == PlayerAction::InitiateRun { server: ServerId::Remote(0) }
            }).count()
        };
        assert_eq!(runs_it(&state), 4, "unseen, two tokens are worth the run");
        state.corp.installed[0].seen_by_runner = true;
        assert_eq!(runs_it(&state), 0, "seen, it is four net damage into a grip of three");
    }

    /// Where the Corp is scoring is where the Runner goes: a face-down
    /// card with two tokens beats every central.
    #[test]
    fn runs_the_advanced_remote_before_a_central() {
        let mut registry = CardRegistry::new();
        let mut agenda = blank_card("agenda", CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(2);
        registry.insert(agenda);
        let mut state = open_board(&mut registry);
        state.corp.installed.push(InstalledCard {
            card: CardId("agenda".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            advancement_tokens: 2,
            ..Default::default()
        });
        let view = build_client_view(&state, &registry, Side::Runner);
        for seed in 1..=4 {
            let chosen = PlanningAgent::new(Side::Runner, seed).select_action(&view, &registry);
            assert_eq!(chosen, PlayerAction::InitiateRun { server: ServerId::Remote(0) }, "seed {seed}");
        }
    }

    /// The trash lever through the real run: the Runner runs a naked
    /// remote holding a rezzed asset, reaches the access, and pays 2[c]
    /// to trash it — but leaves it at 4[c].
    #[test]
    fn trashes_an_affordable_asset_on_access_and_leaves_a_dear_one() {
        use netrunner_core::rules::legal_actions;
        let accessed = |trash_cost: u32| {
            let mut registry = CardRegistry::new();
            let mut nico = blank_card("nico_campaign", CardType::Asset);
            nico.trash_cost = Some(trash_cost);
            registry.insert(nico);
            let mut state = open_board(&mut registry);
            state.corp.installed.push(InstalledCard {
                card: CardId("nico_campaign".to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                rezzed: true,
                ..Default::default()
            });
            let (mut state, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).unwrap();
            let trash = PlayerAction::TrashAccessedCard { card_id: CardId("nico_campaign".to_string()) };
            for _ in 0..20 {
                let legal = legal_actions(&state, &registry);
                if legal.contains(&trash) {
                    break;
                }
                let step = legal
                    .iter()
                    .find(|a| matches!(a, PlayerAction::PassPriority { .. } | PlayerAction::ContinueRun | PlayerAction::CompleteRun))
                    .cloned()
                    .unwrap_or_else(|| panic!("no way forward from {legal:?}"));
                state = apply_action(&state, &registry, step).unwrap().0;
            }
            let view = build_client_view(&state, &registry, Side::Runner);
            assert!(view.legal_actions.contains(&trash), "reached the access: {:?}", view.legal_actions);
            PlanningAgent::new(Side::Runner, 1).select_action(&view, &registry)
        };
        assert_eq!(accessed(2), PlayerAction::TrashAccessedCard { card_id: CardId("nico_campaign".to_string()) });
        assert_eq!(accessed(4), PlayerAction::PassAccessedCard { card_id: CardId("nico_campaign".to_string()) });
    }


    /// The position §25 Stage 8 recorded rather than fixed: the same
    /// agenda behind *two* pieces of ICE, where the fort is finished and
    /// the point is three advances and a free score away. The glacier
    /// planner played advance, ICE, advance and scored next turn, because
    /// its beam held three pairs of the same clicks in another order
    /// (advance-then-ICE beside ICE-then-advance) and "advance, advance"
    /// missed the last slot by a tenth of a point. One node per position
    /// (`prune`) frees the slots, and the free score is walked through
    /// before the third advance is judged (`Search::judged`), so every
    /// Corp plan wins the point now (Phase 5 §26).
    #[test]
    fn behind_two_pieces_of_ice_every_corp_plan_scores_the_point_its_third_advance_wins() {
        let mut registry = CardRegistry::new();
        let mut agenda = blank_card("agenda", CardType::Agenda);
        agenda.advancement_requirement = Some(3);
        agenda.agenda_points = Some(2);
        registry.insert(agenda);
        registry.insert(blank_card("wall", CardType::Ice(netrunner_core::dsl::IceType::Barrier)));

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.runner = empty_runner();
        state.corp = CorpState {
            resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
            hq: vec![CardId("wall".to_string())],
            installed: std::iter::once(InstalledCard {
                card: CardId("agenda".to_string()),
                install_id: InstallId(1),
                server: ServerId::Remote(0),
                ..Default::default()
            })
            .chain(
                [ServerId::Remote(0), ServerId::Remote(0), ServerId::Hq, ServerId::Hq, ServerId::RnD, ServerId::RnD, ServerId::Archives]
                    .into_iter()
                    .enumerate()
                    .map(|(n, server)| InstalledCard {
                        card: CardId("wall".to_string()),
                        install_id: InstallId(10 + n as u32),
                        server,
                        slot: netrunner_core::rules::InstallSlot::Ice,
                        ..Default::default()
                    }),
            )
            .collect(),
            ..Default::default()
        };
        let advance = PlayerAction::AdvanceCard { target: InstallId(1) };
        let score = PlayerAction::ScoreAgenda { target: InstallId(1) };
        for plan in [crate::plans::Plan::Glacier, crate::plans::Plan::FastAdvance, crate::plans::Plan::Traps] {
            let mut agent = PlanningAgent::with_style(Side::Corp, 1, Style::of(plan));
            let (played, after) = super::tests::play_turn(&mut agent, state.clone(), &registry);
            assert_eq!(played.iter().filter(|a| **a == advance).count(), 3, "{plan:?}: {played:?}");
            assert!(played.contains(&score), "{plan:?}: {played:?}");
            assert_eq!(after.corp.resources.agenda_points, AgendaPoints(2), "{plan:?}");
            let stats = agent.stats();
            assert_eq!((stats.planned, stats.diverged), (1, 0), "{plan:?}: one plan, followed whole: {stats:?}");
        }
    }

    #[test]
    fn always_returns_a_member_of_legal_actions() {
        let mut registry = CardRegistry::new();
        let state = corp_state_with_scorable_agenda(&mut registry);
        let view = build_client_view(&state, &registry, Side::Corp);

        let mut agent = PlanningAgent::new(Side::Corp, 2);
        let chosen = agent.select_action(&view, &registry);
        assert!(view.legal_actions.contains(&chosen));
    }
}
