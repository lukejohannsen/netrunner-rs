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
//! never a search of the opponent's turn. **A choice between a card's
//! options that the card hands the opponent is the same question with
//! more answers** — Wildcat Strike's "the Corp chooses: gain 6[credit] or
//! draw 4 cards" — and is answered the same way: before it was, the line
//! ended on the parked choice with the event's cost paid and neither
//! option given, so the planner never played the card (Phase 5 §57: 0
//! against random seats' 11 on a pass of the pool, seed 2; 21 since).
//!
//! **The first action is chosen from the view's list, the rest from the
//! sample's.** A sample is consistent with everything the view shows, not
//! identical to it — its hidden cards are drawn (`determinize`) — so its
//! legal actions can differ from the real ones at the root, and the root
//! is the one step whose real list is in hand.
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
use netrunner_core::dsl::{CardId, CardType};
use netrunner_core::rules::PaymentAsk as Ask;
use netrunner_core::rules::{apply_action, current_actor, legal_transitions_for, GamePhase, GameState, InstallId, PendingDecision, PendingPaidChoice, PlayerAction, Side};
use netrunner_core::view::ClientView;

use crate::agent::{is_regressive, BotAgent};
use crate::determinize::determinize;
use crate::eval::{evaluate_state_with, picked_before, searched_answers, Weights};
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
    /// The cards a parked selection showed the seat when the line was
    /// made (`ClientView::selection`) — see `follow`.
    shown: Vec<netrunner_core::view::SelectionCandidate>,
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
    /// phase with nothing parked and no window open, a decision parked
    /// on the seat in the other side's start of turn — its own turn's end
    /// (`owes_its_turns_end`) — or a decision its own identity parked on
    /// it (`its_identitys_decision`: a selection, the choice that leads
    /// into one, an offer), wherever it stands. Everything else is played
    /// one ply.
    fn plannable(&self, view: &ClientView) -> bool {
        if its_identitys_decision(view, self.identity(view), self.side) {
            return true;
        }
        // When no standing plan reached it: a plan is dropped at a turn the
        // view's counter has left (`follow`), and this one has.
        if matches!(view.phase, GamePhase::StartOfTurn(side) if side != self.side)
            && view.active_run.is_none()
            && (view.pending_paid_choice.is_some() || view.pending_decision.is_some())
        {
            return true;
        }
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
        // A selection out of a zone the seat could not see shows it the
        // cards as it is asked, and the line was played on the sample's
        // guesses: the toggles are positions, so the legal actions match
        // and the line would go on choosing among cards it never saw. AU
        // Co.'s search was planned at its offer, before the top 3 cards
        // of R&D were looked at, and the trash was the plan's (Phase 5
        // §40). Planned again, `determinize::seat_selection` puts the
        // cards shown where the view names them.
        let reveals = matches!(
            &view.pending_decision,
            Some(PendingDecision::ChooseCards { source, .. }) if source.shows_the_chooser_hidden_cards()
        );
        if step.expected != view.legal_actions || (reveals && view.selection != plan.shown) {
            self.plan = None;
            self.stats.diverged += 1;
            return None;
        }
        plan.next += 1;
        self.stats.followed += 1;
        Some(step.action.clone())
    }

    /// The seat's own identity, as the view shows it to both chairs.
    fn identity<'v>(&self, view: &'v ClientView) -> Option<&'v CardId> {
        match self.side {
            Side::Corp => view.corp.identity.as_ref(),
            Side::Runner => view.runner.identity.as_ref(),
        }
    }

    /// Searches the beam from `root` and keeps the best line. `None` when
    /// no line could be made, which is a root with no progressive action
    /// — not a state a seat is asked to act on.
    fn plan(&mut self, root: GameState, view: &ClientView, registry: &CardRegistry) -> Option<PlayerAction> {
        let deciding = its_identitys_decision(view, self.identity(view), self.side);
        let mut search = Search {
            side: self.side,
            registry,
            weights: &self.weights,
            root_turn: root.turn,
            root_legal: &view.legal_actions,
            deciding,
            applications: 0,
            answering: 0,
            rng: &mut self.rng,
        };
        let steps = search.best_line(root);
        let turn = view.turn;
        self.stats.applications += search.applications as u64;
        let steps = steps?;
        let first = steps[0].action.clone();
        self.plan = Some(Plan { steps, next: 1, turn, shown: view.selection.clone() });
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
    /// Whether the root is a decision the seat's own identity parked on it
    /// (`its_identitys_decision`): the line goes on while the seat
    /// still owes a decision — the selection, and the install its `then`
    /// asks where to put — in a run or the other side's turn, and stands
    /// where the decision leaves it.
    deciding: bool,
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
            // A sample can offer what the real state refuses: while it
            // carried no identity, a third remote under A Teia's limit,
            // played unchecked, was the 256-seed sweep's seed 106. A
            // sample that offers less than the view only narrows the
            // choice.
            let at_root = steps.is_empty();
            if at_root {
                transitions.retain(|(action, _, _)| self.root_legal.contains(action));
            }
            let expected: Vec<PlayerAction> =
                if at_root { self.root_legal.to_vec() } else { transitions.iter().map(|(action, _, _)| action.clone()).collect() };
            if self.deciding
                && let Some(sets) = whole_sets(&state, self.side, &expected)
            {
                self.expand_sets(state, steps, expected, sets, next, finished);
                return;
            }
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

    /// Each of `sets` toggled in turn from `state` — a step a card, the
    /// first expecting `expected` — and the selection then walked on from
    /// its last card as any node is (`expand`): its confirm, which is
    /// forced at the count, and whatever the decision asks after it. See
    /// `whole_sets`.
    fn expand_sets(&mut self, state: GameState, steps: Vec<Step>, expected: Vec<PlayerAction>, sets: Vec<Vec<PlayerAction>>, next: &mut Vec<Node>, finished: &mut Vec<Finished>) {
        'sets: for set in sets {
            let mut line = steps.clone();
            let mut at = state.clone();
            for (index, toggle) in set.into_iter().enumerate() {
                let expected = if index == 0 { expected.clone() } else { netrunner_core::rules::legal_actions_for(&at, self.registry, self.side) };
                self.applications += 1;
                let Ok((toggled, _)) = apply_action(&at, self.registry, toggle.clone()) else { continue 'sets };
                line.push(Step { expected, action: toggle });
                at = toggled;
            }
            self.expand(Node { score: 0.0, state: at, steps: line }, next, finished);
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
            if self.deciding && current_actor(&state) == Some(self.side) && state.is_resolution_blocked() {
                return (state, Standing::Open);
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
                let Some((_, path)) = self.answer_payment(&state, None) else { return (state, Standing::Leaf) };
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
                // The seat's own turn's end, still resolving: a decision
                // its discard step's triggers handed it (Phase 5 §35).
                Some(actor) if actor == self.side && owes_its_turns_end(&state, self.side, self.root_turn) => return (state, Standing::Open),
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
    /// X are real choices and are all tried; an install's trash picks are
    /// tried as sets, in ascending order (`eval::searched_answers`, Phase 5
    /// §48), with `after` the position the chain picked last.
    fn answer_payment(&mut self, state: &GameState, after: Option<u32>) -> Option<(f64, Vec<PlayerAction>)> {
        let payment = state.pending_payment.as_ref()?;
        let ask = &payment.question;
        let mut answers = searched_answers(ask, after);
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
                self.answer_payment(&next, picked_before(ask, answer, &next)).map(|(score, mut path)| {
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

    /// The opponent's answer to the paid choice, or the choice between a
    /// card's options (`PendingDecision::ChooseEffect`), parked on `state`
    /// that leaves the seat worst off — each answer applied and settled,
    /// the settled states scored where they stand — with the state it
    /// settles to and where that stands, so the line goes on from it;
    /// `None` when nothing of the kind is parked. Nested at most
    /// `ANSWER_DEPTH` deep, so an answer that parks another question is
    /// answered too, and a third is where the second stands.
    fn opponents_answer(&mut self, state: &GameState) -> Option<(GameState, Standing)> {
        let opponent = self.side.other();
        if self.answering >= ANSWER_DEPTH {
            return None;
        }
        let paid = state.pending_paid_choice.as_ref().is_some_and(|choice| choice.side == opponent);
        let chosen = matches!(&state.pending_decision, Some(PendingDecision::ChooseEffect { chooser, .. }) if *chooser == opponent);
        if !paid && !chosen {
            return None;
        }
        let answers: Vec<PlayerAction> = netrunner_core::rules::legal_actions_for(state, self.registry, opponent)
            .into_iter()
            .filter(|action| match action {
                PlayerAction::AcceptPendingPaidChoice { .. } | PlayerAction::DeclinePendingPaidChoice => paid,
                PlayerAction::ResolvePendingChoice { .. } => chosen,
                _ => false,
            })
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
        let standing = self.score(state);
        if self.side != Side::Corp {
            return standing;
        }
        let agendas: Vec<InstallId> = state
            .corp
            .installed
            .iter()
            .filter(|card| card.advancement_tokens > 0 && self.registry.get(&card.card).is_some_and(|def| def.card_type == CardType::Agenda))
            .map(|card| card.install_id)
            .collect();
        let mut best = standing;
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

/// Whether `state` is the tail of `side`'s own turn with a decision
/// parked on it (Phase 5 §35). The engine ends a turn in one step
/// (`turn::finish_turn`): the discard phase's end is dispatched, and the
/// other side's turn is entered — its phase and the turn counter — before
/// a decision those triggers parked is answered. So PT Untaian's "you may
/// pay 1[credit] to place 1 advancement counter", Magdalene Keino-
/// Chemutai's install from what she discarded and Méliès U.'s number are
/// asked of the seat in the *opponent's* start of turn, and by the counter
/// its turn is over. A line reached them, could not pass them, and ended
/// there with the decision unmade; the decision itself fell to the one-ply
/// chooser, whose bound on the selection that follows is the worst card
/// the filter could match (`fundamentals::advancement_upside` — a sprung
/// trap makes it zero), and it declined PT Untaian's advance with a card
/// to put it on 5 times in 16. A decision
/// the seat owes in the other side's start of turn, on the turn its own
/// ended or the next, with no run, is that turn's end: a step of the line,
/// and a root to plan from (`PlanningAgent::plannable`).
fn owes_its_turns_end(state: &GameState, side: Side, root_turn: u32) -> bool {
    matches!(state.phase, GamePhase::StartOfTurn(other) if other != side)
        && (state.turn == root_turn || state.turn == root_turn + 1)
        && state.active_run.is_none()
        && state.is_resolution_blocked()
}

/// Whether `decision` is a selection of the seat's own cards that its own
/// identity's text parked on it (Phase 5 §36): Synapse Global's "you may
/// reveal and install 1 card from HQ, ignoring all costs" when a tag is
/// removed, Poétrï Luxury Brands' install from HQ when an agenda is
/// stolen. Both come in the Runner's turn, one inside its run, where the
/// seat played one ply: a toggle left the selection open and a confirm
/// with nothing chosen closed it, and `fundamentals::pending_decision_
/// upside` prices a "may" at its worst resolution — choosing nothing — so
/// nothing was chosen. Synapse Global was offered its free install 129
/// times in 48 games and took it about five; Poétrï 127 and about nine.
/// Planned, the selection, its confirm and the server the install asks
/// for are steps of a line scored where the install leaves the board.
///
/// **Over the seat's own cards, and the other side's installed ones**,
/// because that is what the evaluator prices where it stands. The second
/// is Tāo Salonga's "you may swap 2 installed pieces of ice" (§38): what
/// two pieces are worth in each other's places is where each stands
/// against the rig, which the Runner reads off a run since §38
/// (`eval::runner::shut_doors`). Before it, planned, the swap and the
/// decline would have tied and the jitter would have swapped at random.
fn its_identitys_selection(decision: Option<&PendingDecision>, identity: Option<&CardId>, side: Side) -> bool {
    let Some(PendingDecision::ChooseCards { side: chooser, source, source_card, prompting_card, .. }) = decision else { return false };
    *chooser == side && priced_zone(source) && identity.is_some_and(|id| prompting_card.as_ref().or(source_card.as_ref()) == Some(id))
}

/// Whether a selection from `source` is one the evaluator prices where it
/// leaves the board: the seat's own cards, and the other side's installed
/// ones (`its_identitys_selection`).
fn priced_zone(source: &netrunner_core::dsl::CardZoneRef) -> bool {
    use netrunner_core::dsl::CardZoneRef;
    matches!(
        source,
        CardZoneRef::OwnHq
            | CardZoneRef::OwnArchives
            | CardZoneRef::OwnRAndD
            | CardZoneRef::OwnStack
            | CardZoneRef::OwnGrip
            | CardZoneRef::OwnHeap
            | CardZoneRef::OwnSetAside
            | CardZoneRef::OwnInstalled
            | CardZoneRef::HostedOnSource
            | CardZoneRef::TopOfOwnStack
            | CardZoneRef::OpponentInstalled
    )
}

/// Whether `decision` is a choice the seat's own identity parked on it
/// whose yes is a selection the evaluator prices (`priced_zone`): Tāo
/// Salonga's "you may swap 2 installed pieces of ice" (§38), and the "may"
/// ahead of a selection of the seat's own cards (§39) — Haas-Bioroid:
/// Precision Design's, Méliès U.'s, Barry "Baz" Wong's, Magdalene
/// Keino-Chemutai's and Sebastião Souza Pessoa's. The selection is planned
/// (`its_identitys_selection`), but the choice that leads into it fell to
/// the one-ply chooser, which prices a parked selection at its worst
/// resolution (`fundamentals::pending_decision_upside`) — choosing
/// nothing — so the yes never beat the no: Tāo's swap was taken 0 times in
/// 634 offers over four pairings, and Barry's install 0 times in 596.
/// Planned, the yes is the selection's best line and the no is the no.
///
/// **Barry's yes is priced against the run it comes in.** The line stands
/// mid-run once the install is made, where the run's leaf reads what is
/// left to break with (`eval::runner`'s run terms), so the install is
/// taken when the rezzed ICE can still be broken after it and declined
/// when it would spend the breaking credits.
///
/// **What it costs was recorded, then re-measured and found to be
/// nothing** (§39, §49). §39 recorded Barry's deck losing games to the
/// yes — 35 → 46 Corp wins over 96 games on one seed, z +1.98 — while the
/// decision itself, played out both ways, was not worse. §49 took the
/// yes against a forced decline over 288 games on three seeds: Corp 163
/// vs 166, z −0.31, per seed −1.35 / +0.33 / +0.38. The recorded number
/// was one seed's drift. Since §44 one ply looks through its own parked
/// decisions and takes the same yes (465 of 1,138 offers, against this
/// rule's 473 of 1,128), so this rule no longer decides Barry's yes;
/// whether the same holds for the other identities' is not measured.
fn its_identitys_choice(decision: Option<&PendingDecision>, identity: Option<&CardId>, side: Side) -> bool {
    use netrunner_core::dsl::Effect;
    let Some(PendingDecision::ChooseEffect { chooser, options, source_card, prompting_card, .. }) = decision else { return false };
    let selects = options.iter().any(|option| {
        let mut found = false;
        option.for_each_effect(&mut |effect| found |= matches!(effect, Effect::PromptChooseCards { source, .. } if priced_zone(source)));
        found
    });
    *chooser == side && selects && identity.is_some_and(|id| prompting_card.as_ref().or(source_card.as_ref()) == Some(id))
}

/// The ways through a selection parked on `side` that takes an exact count
/// of two or more, nothing chosen yet, as whole sets of its toggles in
/// `actions` — `None` for any other state, or past `WHOLE_SETS` sets (§38).
/// Toggled one card a ply, the selection's first card is judged where it
/// stands, before the set it begins is made, so every first card ties
/// and the beam keeps `PLAN_BEAM` of them at random: of Tāo Salonga's
/// pairs among ten pieces of ICE, the best was out of reach one time in
/// eight. As whole sets every pair is scored where its swap leaves the
/// board. Asked only while the seat decides for its own identity
/// (`Search::deciding`): a card's own selections are toggled as before.
fn whole_sets(state: &GameState, side: Side, actions: &[PlayerAction]) -> Option<Vec<Vec<PlayerAction>>> {
    let Some(PendingDecision::ChooseCards { side: chooser, min, max, selected, .. }) = state.pending_decision.as_ref() else { return None };
    if *chooser != side || min != max || *max < 2 || !selected.is_empty() {
        return None;
    }
    let toggles: Vec<&PlayerAction> = actions.iter().filter(|action| matches!(action, PlayerAction::ToggleCardSelection { .. })).collect();
    let count = *max as usize;
    if toggles.len() < count || combinations(toggles.len(), count) > WHOLE_SETS {
        return None;
    }
    let mut sets = Vec::new();
    let mut picked: Vec<usize> = (0..count).collect();
    loop {
        sets.push(picked.iter().map(|&index| toggles[index].clone()).collect());
        // The next combination in lexicographic order: the last index that
        // can move up does, and every one after it follows it.
        let Some(at) = (0..count).rev().find(|&at| picked[at] < toggles.len() - count + at) else { break };
        picked[at] += 1;
        for after in at + 1..count {
            picked[after] = picked[after - 1] + 1;
        }
    }
    Some(sets)
}

/// The most whole sets `whole_sets` enumerates: every pair of sixteen
/// pieces of ICE, which no Corp in the pool reaches.
const WHOLE_SETS: usize = 120;

/// `n` choose `k`, saturating.
fn combinations(n: usize, k: usize) -> usize {
    (0..k).fold(1usize, |acc, i| acc.saturating_mul(n - i) / (i + 1))
}

/// Whether `choice` is a paid choice the seat's own identity offers it
/// (Phase 5 §37): AU Co.'s "when your turn begins, you may remove 2 hosted
/// power counters to look at the top 3 cards of R&D. Trash 1 of those
/// cards and add the rest to HQ". It comes in the seat's own start of
/// turn, ahead of the action phase a plan starts from, so the one-ply
/// chooser answered it, and the accepted side was a selection
/// `pending_decision_upside` prices at its worst: AU Co. declined it 530
/// times in 48 games and took it 46. Planned, the search is steps of the
/// turn's line, and the cards it brings are cards the line can play. PT
/// Untaian's offer at its turn's end is the other one an identity makes
/// its own side, and already a step of the line (§35).
fn its_identitys_offer(choice: Option<&PendingPaidChoice>, identity: Option<&CardId>, side: Side) -> bool {
    choice.is_some_and(|choice| choice.side == side && identity.is_some_and(|id| choice.prompting_card.as_ref().or(choice.source_card.as_ref()) == Some(id)))
}

/// A decision the seat's own identity parked on it, of any of the kinds
/// the seat plans from: `its_identitys_selection`, `its_identitys_choice`
/// or `its_identitys_offer`.
fn its_identitys_decision(view: &ClientView, identity: Option<&CardId>, side: Side) -> bool {
    its_identitys_selection(view.pending_decision.as_ref(), identity, side)
        || its_identitys_choice(view.pending_decision.as_ref(), identity, side)
        || its_identitys_offer(view.pending_paid_choice.as_ref(), identity, side)
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
        let Some(score) = one_ply_score(sample, action, registry, side, weights, OWN_DECISIONS_LOOKED_THROUGH) else { continue };
        let score = score + rng.random::<f64>() * TIE_BREAK_JITTER;
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

/// `action` applied to `state` and scored for `side` as one ply scores it
/// (`one_ply`), or `None` when the sample refuses it.
///
/// A toggle marks a position and moves no card, so every candidate of a
/// selection scored where it stands alike and the jitter chose: Ryō
/// "Phoenix" Ōno's "the Corp trashes 1 card from HQ", answered in the
/// Runner's turn, sent an agenda to Archives 5 of 17 times HQ held
/// something else (Phase 5 §40). Scored where the selection would leave
/// the board once confirmed — exact for one card, and greedy for "up to"
/// or "any number", whose Confirm is already a candidate to beat. A
/// selection that cannot be confirmed yet (two or more cards still owed)
/// is scored as before; the beam's own lines are not this function's.
///
/// **An action that parks a decision on the seat itself is scored by the
/// seat's best answer to it** (Phase 5 §44), `depth` decisions deep. The
/// evaluator charges a parked decision of a side's own
/// `unresolved_decision_weight` (2.0) and credits only a lower bound of
/// what resolving it delivers, which is what stops a selection walking —
/// but the seat is the one who answers, and before the answer nothing
/// after the decision has resolved either. Passing into Brân 1.0's "you
/// may install 1 piece of ice" scored −2.0 with its two "end the run"
/// still waiting behind it, the price of a Mercia B4LL4RD, and LEO
/// Construction trashed one to end a run the ice was about to end: 5 of
/// its 17 uses in 96 games were ties like it, a Corp choosing which of
/// the Runner's programs a subroutine trashes among them. A toggle that
/// leaves its selection open is not looked through: that is the walk the
/// charge exists to stop, and every order of its cards would be priced.
fn one_ply_score(state: &GameState, action: &PlayerAction, registry: &CardRegistry, side: Side, weights: &Weights, depth: u8) -> Option<f64> {
    let (mut next, _events) = apply_action(state, registry, action.clone()).ok()?;
    let mut open = false;
    if matches!(action, PlayerAction::ToggleCardSelection { .. }) {
        match apply_action(&next, registry, PlayerAction::ConfirmCardSelection) {
            Ok((confirmed, _)) => next = confirmed,
            Err(_) => open = true,
        }
    }
    if !open && depth > 0 && next.is_resolution_blocked() && current_actor(&next) == Some(side) {
        let answered = netrunner_core::rules::legal_actions_for(&next, registry, side)
            .iter()
            .filter(|answer| !is_regressive(answer, next.pending_decision.as_ref()))
            .filter_map(|answer| one_ply_score(&next, answer, registry, side, weights, depth - 1))
            .max_by(f64::total_cmp);
        if answered.is_some() {
            return answered;
        }
    }
    Some(evaluate_state_with(&next, side, registry, weights))
}

/// How many of the seat's own decisions, parked one behind another, one
/// ply answers before it scores (`one_ply_score`): a "may" whose yes is a
/// selection is two, and a third is scored where it stands.
const OWN_DECISIONS_LOOKED_THROUGH: u8 = 2;

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

    /// `corp_with_a_scorable_hand` with `n` walls on HQ and one in hand,
    /// and the install of it over them that trashes first, parked on its
    /// first pick.
    fn trash_first_over(n: u32, registry: &mut CardRegistry) -> (GameState, GameState) {
        let mut state = corp_with_a_scorable_hand(registry);
        state.corp.resources.credits = Credits(30);
        state.corp.hq = vec![CardId("wall".to_string())];
        state.corp.installed.retain(|card| card.server != ServerId::Hq);
        state.corp.installed.extend((0..n).map(|i| ice(100 + i, ServerId::Hq)));
        state.next_install_id = 200;
        let install =
            PlayerAction::InstallCard { card_id: CardId("wall".to_string()), zone: ServerId::Hq, slot: InstallSlot::Ice, trash_first: true };
        let parked = apply_action(&state, registry, install).expect("the install parks").0;
        assert!(parked.pending_payment.is_some(), "the premise: the first pick is asked");
        (state, parked)
    }

    /// An install's trash picks are searched as sets (Phase 5 §48): ten
    /// walls on HQ is every non-empty subset once — tried in every order
    /// it was 16,099,400 applications, about 49 minutes (§47) — and the
    /// chain found is one the engine takes to the end of the payment.
    #[test]
    fn the_payment_search_tries_each_set_of_trash_picks_once() {
        let mut registry = CardRegistry::new();
        let (_, parked) = trash_first_over(10, &mut registry);
        let weights = Weights::default();
        let mut rng = StdRng::seed_from_u64(1);
        let legal = Vec::new();
        let mut search = Search {
            side: Side::Corp,
            registry: &registry,
            weights: &weights,
            root_turn: parked.turn,
            root_legal: &legal,
            deciding: false,
            applications: 0,
            answering: 0,
            rng: &mut rng,
        };
        let (_, path) = search.answer_payment(&parked, None).expect("a chain of answers goes through");
        // One application per node of the ascending tree: 2¹⁰ − 1 picks,
        // and a "no more" after each of them but the one that leaves
        // nothing to ask about — all ten picked.
        assert_eq!(search.applications, 1023 + 1022, "each subset once");
        let mut state = parked;
        for answer in path {
            state = apply_action(&state, &registry, answer).expect("each answer applies").0;
        }
        assert!(state.pending_payment.is_none(), "the payment is made");
    }

    /// The evaluator prices a trash-first install by its best set of picks,
    /// and searching sets loses none: over four walls its price is the best
    /// of all fifteen subsets, each applied in ascending order.
    #[test]
    fn the_evaluator_prices_a_trash_first_install_by_its_best_set() {
        let mut registry = CardRegistry::new();
        let (_, parked) = trash_first_over(4, &mut registry);
        let weights = Weights::default();
        let positions: Vec<u32> = match &parked.pending_payment.as_ref().expect("parked").question {
            Ask::Install(question) => question.eligible.iter().map(|candidate| candidate.position).collect(),
            other => panic!("an install's question, not {other:?}"),
        };
        let mut best = f64::NEG_INFINITY;
        for subset in 1u32..(1 << positions.len()) {
            let mut state = parked.clone();
            for (i, position) in positions.iter().enumerate() {
                if subset & (1 << i) != 0 {
                    state = apply_action(&state, &registry, PlayerAction::ToggleCardSelection { position: *position as usize }).expect("a pick applies").0;
                }
            }
            if state.pending_payment.is_some() {
                state = apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("no more").0;
            }
            assert!(state.pending_payment.is_none(), "subset {subset:b} pays");
            best = best.max(evaluate_state_with(&state, Side::Corp, &registry, &weights));
        }
        assert_eq!(evaluate_state_with(&parked, Side::Corp, &registry, &weights), best);
    }

    /// One ply chooses the card a one-card selection sends where it is
    /// scored to go, not by the jitter (§40): Hansei Review's "trash 1
    /// card from HQ" out of an agenda and two Hedge Funds keeps the
    /// agenda on every seed. Before, a toggle was scored on a board it had
    /// not changed, and seed 1 trashed the agenda.
    #[test]
    fn one_ply_keeps_the_agenda_a_one_card_selection_could_trash() {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        for seed in 0..16 {
            let mut state = GameState::new(seed);
            state.phase = GamePhase::Action(Side::Corp);
            state.corp.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
            state.corp.hq = ["hansei_review", "offworld_office", "hedge_fund", "hedge_fund"].map(|card| CardId(card.to_string())).to_vec();
            state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 10];
            state.runner.stack = vec![CardId("sure_gamble".to_string()); 10];
            let play = PlayerAction::PlayOperation { card_id: CardId("hansei_review".to_string()) };
            state = apply_action(&state, &registry, play).expect("Hansei Review is played").0;
            assert!(state.pending_decision.is_some(), "the premise: its trash is a selection");
            let mut rng = StdRng::seed_from_u64(seed);
            for _ in 0..4 {
                if state.pending_decision.is_none() {
                    break;
                }
                let view = build_client_view(&state, &registry, Side::Corp);
                let sample = determinize(&view, &registry, &Knowledge::default(), &mut rng);
                let action = one_ply(&view, &registry, &sample, Side::Corp, &Weights::default(), &mut rng);
                state = apply_action(&state, &registry, action).expect("one ply's action applies").0;
            }
            assert!(state.pending_decision.is_none(), "seed {seed}: the selection is made");
            assert!(state.corp.hq.iter().any(|card| card.0 == "offworld_office"), "seed {seed}: the agenda is kept: {:?}", state.corp.archives);
        }
    }

    /// One ply answers a decision its own pass parks before it scores the
    /// pass (§44): at Brân 1.0 the Runner cannot break, LEO Construction's
    /// trash of Mercia B4LL4RD ends the run, and so does letting Brân's
    /// subroutines fire — whose first, "you may install 1 piece of ice",
    /// parks a choice on the Corp ahead of its two "end the run". Charged
    /// `unresolved_decision_weight` there, the pass cost what Mercia does
    /// and the jitter chose; answered, the pass keeps her on every seed.
    #[test]
    fn one_ply_lets_the_ice_end_the_run_rather_than_pay_to_end_it() {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let card = |id: &str| CardId(id.to_string());
        for seed in 0..16 {
            let mut state = GameState::new(seed);
            state.phase = GamePhase::Action(Side::Runner);
            state.turn = 6;
            state.next_install_id = 20;
            state.corp.identity = Some(card("leo_construction_labor_solutions"));
            state.corp.hq = vec![card("hedge_fund"); 3];
            state.corp.r_and_d = vec![card("hedge_fund"); 10];
            state.corp.resources = PlayerResources { credits: Credits(5), clicks: Clicks(0), agenda_points: AgendaPoints(0) };
            state.corp.installed = vec![
                InstalledCard { card: card("bran_1_0"), install_id: InstallId(10), server: ServerId::Hq, slot: InstallSlot::Ice, rezzed: true, ..Default::default() },
                InstalledCard { card: card("mercia_b4ll4rd"), install_id: InstallId(11), server: ServerId::Hq, slot: InstallSlot::Root, rezzed: true, ..Default::default() },
            ];
            state.runner = empty_runner();
            state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
            state.runner.stack = vec![card("sure_gamble"); 10];
            state = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("the run starts").0;
            let mut rng = StdRng::seed_from_u64(seed);
            for _ in 0..30 {
                if state.active_run.is_none() {
                    break;
                }
                let Some(actor) = current_actor(&state) else { break };
                let view = build_client_view(&state, &registry, actor);
                let action = match actor {
                    // The Runner breaks nothing and goes on.
                    Side::Runner => view
                        .legal_actions
                        .iter()
                        .find(|a| matches!(a, PlayerAction::PassPriority { .. }))
                        .or_else(|| view.legal_actions.iter().find(|a| !matches!(a, PlayerAction::JackOut | PlayerAction::BreakSubroutineWithClick { .. })))
                        .expect("the Runner can go on")
                        .clone(),
                    Side::Corp => {
                        let sample = determinize(&view, &registry, &Knowledge::default(), &mut rng);
                        one_ply(&view, &registry, &sample, Side::Corp, &Weights::default(), &mut rng)
                    }
                };
                state = apply_action(&state, &registry, action).expect("the action applies").0;
            }
            assert!(state.active_run.is_none(), "seed {seed}: the run ended");
            assert!(state.corp.installed.iter().any(|c| c.card.0 == "mercia_b4ll4rd"), "seed {seed}: Mercia is kept: {:?}", state.corp.archives);
        }
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
            shown: Vec::new(),
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
            deciding: false,
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
    /// **The balanced Corp is counted over sixty seeds since Uprising
    /// Stage 1** (3 October 2026): the planner stages since had moved it
    /// to 9 of 20 on `main`, one under the bar, and ten more cards in the
    /// pool moved it to 11. Over sixty it is 25 before the stage and 26
    /// after — about two in five, where twenty seeds land either side of
    /// half by the sample alone. The kill Corp's two claims stay on the
    /// first twenty, where both hold exactly: over sixty, the kill Corp
    /// tags every time against three cards, and against the full grip
    /// once after the stage (seed 35, Hedge Fund first) and never before.
    /// **The full-grip claim is counted over sixty since Uprising Stage 5**
    /// (4 October 2026): five more Corp cards in the prior re-drew every
    /// R&D sample, and the kill Corp tagged against the full grip on seed
    /// 2 of the first twenty — a Hedge Fund and a drawn card that made the
    /// line pay — where on `main` before the stage it tagged on none of
    /// sixty. After it, seeds 2 and 35 of sixty: a lucky sample, as the
    /// balanced Corp's are, and far under the twenty of twenty against
    /// three cards, which still holds exactly.
    /// **The balanced Corp is counted over 180 seeds since Uprising Stage
    /// 6** (4 October 2026): over sixty it stood at 27 to 30 — the bar is
    /// under 30 — and which it was depended on the build (a debug build
    /// with or without debuginfo, a workspace build or the crate's alone,
    /// release), on the same source before the stage as after it. Over
    /// 180 it is 73 before the stage and 78 after, in each build tried:
    /// two in five, as it has been since Midnight Sun.
    /// (With the credits to play Scorched Earth in the same turn, every
    /// planner tags: the line kills inside the turn and scores the win.)
    /// Public Trail's shape without its "play only if" (a successful run
    /// last turn), which a fixture cannot write.
    ///
    /// **Since Phase 5 §51 the tag's leverage is every Corp's**, read off
    /// the punisher in HQ and not off the plan, so the balanced Corp
    /// holding Scorched Earth is no longer the control: it plans the tag
    /// on 117 of 180 seeds where it planned it on 78. The control is the
    /// balanced Corp holding Hedge Fund in Scorched Earth's place, which
    /// has no follow-up to read and tags on 0 of 180 — so the two in five
    /// before §51 were samples that made Scorched Earth's line pay, not
    /// the tag.
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
        const BALANCED_SEEDS: u64 = 180;
        let balanced = (1..=BALANCED_SEEDS)
            .filter(|&seed| plans_to_tag(&mut PlanningAgent::new(Side::Corp, seed), &view))
            .count() as u64;
        assert!(balanced * 2 > BALANCED_SEEDS, "a balanced Corp holding the follow-up reads the tag (§51): {balanced} of {BALANCED_SEEDS}");
        let mut no_follow_up = state.clone();
        no_follow_up.corp.hq = vec![CardId("public_trail_open".to_string()), CardId("hedge_fund".to_string()), CardId("hedge_fund".to_string())];
        let no_follow_up = build_client_view(&no_follow_up, &registry, Side::Corp);
        let unpunished = (1..=BALANCED_SEEDS)
            .filter(|&seed| plans_to_tag(&mut PlanningAgent::new(Side::Corp, seed), &no_follow_up))
            .count() as u64;
        assert!(unpunished * 2 < BALANCED_SEEDS, "a Corp holding no follow-up sees nothing in the tag but a lucky sample: {unpunished} of {BALANCED_SEEDS}");
        let mut safe = state.clone();
        safe.runner.grip = vec![CardId("sure_gamble".to_string()); 6];
        let view = build_client_view(&safe, &registry, Side::Corp);
        const FULL_GRIP_SEEDS: u64 = 60;
        let against_a_full_grip = (1..=FULL_GRIP_SEEDS)
            .filter(|&seed| plans_to_tag(&mut PlanningAgent::with_style(Side::Corp, seed, Style::of(Plan::Kill)), &view))
            .count() as u64;
        assert!(
            against_a_full_grip * 10 < FULL_GRIP_SEEDS,
            "no threat against a full grip, and a tag alone is under the price but for a lucky sample: {against_a_full_grip} of {FULL_GRIP_SEEDS}"
        );
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
                up_to: None,
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

    /// Psychographics is played to score: "X is equal to or less than the
    /// number of tags the Runner has. Place X advancement counters on 1
    /// installed card you can advance." With the Runner on 2 tags, a 3/2
    /// two tokens short and one click, the Corp pays 2[c] and scores,
    /// where one advance would have left it a token short. The card is on
    /// the blind list (Phase 5 §57: random seats 7, the planner 0 on a
    /// pass of the pool), and this says the play is read where it scores;
    /// why the pass never reaches such a turn is not measured — its one
    /// deck holds one copy and must find the Runner tagged.
    #[test]
    fn plays_psychographics_to_score_an_agenda_the_runners_tags_reach() {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut state = corp_state_with_scorable_agenda(&mut registry);
        state.corp.installed[0].advancement_tokens = 1;
        state.corp.resources.clicks = Clicks(1);
        state.corp.hq = vec![CardId("psychographics".to_string())];
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 5];
        state.runner.tags = 2;
        let view = build_client_view(&state, &registry, Side::Corp);
        assert!(view.legal_actions.iter().any(|action| matches!(action, PlayerAction::PlayOperation { .. })), "{:?}", view.legal_actions);

        let mut agent = PlanningAgent::new(Side::Corp, 1);
        let (played, after) = super::tests::play_turn(&mut agent, state, &registry);
        assert_eq!(after.corp.resources.agenda_points, AgendaPoints(2), "{played:?}");
    }

    /// A lockdown is played with a click the Corp has nothing better for:
    /// SYNC Rerouting's "give the Runner 1 tag unless they pay 4[credit]"
    /// and Argus Crackdown's 2 meat damage on a successful run on a server
    /// protected by ice, each read as what it takes from the Runner's next
    /// run (Phase 5 §58). Before, a lockdown paid on a turn the line does
    /// not reach and was a click spent for nothing: the planner never
    /// played either on a pass of the full pool, random seats did.
    #[test]
    fn plays_a_lockdown_with_the_last_click_for_what_it_takes_from_the_runners_next_run() {
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        for lockdown in ["sync_rerouting", "argus_crackdown"] {
            let mut state = GameState::new(0);
            state.phase = GamePhase::Action(Side::Corp);
            state.runner = empty_runner();
            state.runner.resources.credits = Credits(6);
            state.runner.grip = vec![CardId("sure_gamble".to_string()); 5];
            // A full hand and no credits for its Hedge Funds: the click's
            // alternatives are a credit and a card.
            state.corp.resources = PlayerResources { credits: Credits(0), clicks: Clicks(1), agenda_points: AgendaPoints(0) };
            state.corp.hq = vec![CardId(lockdown.to_string())];
            state.corp.hq.extend(vec![CardId("hedge_fund".to_string()); 4]);
            state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 5];
            for (index, server) in [ServerId::Hq, ServerId::RnD].into_iter().enumerate() {
                state.corp.installed.push(InstalledCard {
                    card: CardId("ice_wall".to_string()),
                    install_id: InstallId(index as u32 + 1),
                    server,
                    slot: InstallSlot::Ice,
                    rezzed: true,
                    ..Default::default()
                });
            }
            let mut agent = PlanningAgent::new(Side::Corp, 1);
            let (played, after) = super::tests::play_turn(&mut agent, state, &registry);
            assert!(after.corp.play_area.iter().any(|card| card.card.0 == lockdown), "{lockdown}: {played:?}");
        }
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
            cost_discount_if: None, used_by: None, access: false, from_hand: false, part_of: None }];
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

    /// Carnivore is installed by the Runner whose plan it serves (Phase 5
    /// §52): a Dismantle Runner under René "Loup" Arcemont, early, with
    /// the credits for it and a click that would otherwise be a credit,
    /// puts it on the table — the access trash is a trash a turn to that
    /// plan, and Loup's credit and card pay for the grip it costs — where
    /// a Runner on another plan (Pressure; a balanced seat under an Anarch
    /// identity is Dismantle, `Style::or_faction`) on the same board reads
    /// a 4[c] console as a rig card under its price and keeps the credits. Counted
    /// over seeds, not read off one: the margin is a click's worth at the
    /// stage's horizon.
    #[test]
    fn a_dismantle_runner_under_loup_installs_carnivore_and_another_plan_does_not() {
        use crate::plans::{Plan, Style};
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut inert = blank_card("filler", CardType::Event);
        inert.side = Side::Runner;
        registry.insert(inert);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.turn = 4;
        state.runner = empty_runner();
        state.runner.identity = Some(CardId("rene_loup_arcemont_party_animal".to_string()));
        state.runner.resources = PlayerResources { credits: Credits(6), clicks: Clicks(1), agenda_points: AgendaPoints(0) };
        state.runner.memory_units = MemoryUnits(4);
        state.runner.grip = vec![CardId("carnivore".to_string()), CardId("filler".to_string()), CardId("filler".to_string()), CardId("filler".to_string())];
        state.runner.stack = vec![CardId("filler".to_string()); 10];
        state.corp.hq = vec![CardId("hedge_fund".to_string()); 3];
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 10];
        // Every central iced, and no breaker in the rig: a run is not
        // the click's other use, a credit is.
        state.corp.installed = [(10, ServerId::Hq), (11, ServerId::RnD), (12, ServerId::Archives)]
            .into_iter()
            .map(|(id, server)| InstalledCard { card: CardId("palisade".to_string()), install_id: InstallId(id), server, slot: netrunner_core::rules::InstallSlot::Ice, rezzed: true, ..Default::default() })
            .collect();
        state.next_install_id = 20;
        let view = build_client_view(&state, &registry, Side::Runner);
        let install = PlayerAction::InstallHardware { card_id: CardId("carnivore".to_string()) };
        assert!(view.legal_actions.contains(&install), "{:?}", view.legal_actions);
        const SEEDS: u64 = 20;
        let installs = |style: Style| {
            (1..=SEEDS)
                .filter(|&seed| {
                    let mut agent = PlanningAgent::with_style(Side::Runner, seed, style);
                    agent.observe(&view);
                    agent.select_action(&view, &registry) == install
                })
                .count() as u64
        };
        let dismantle = installs(Style::of(Plan::Dismantle));
        let pressure = installs(Style::of(Plan::Pressure));
        assert!(dismantle * 2 > SEEDS, "a Dismantle Runner under Loup installs Carnivore: {dismantle} of {SEEDS}");
        assert_eq!(pressure, 0, "a Runner on another plan reads a 4[c] console as a rig card under its price: {pressure} of {SEEDS}");
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
                cost_discount_if: None, used_by: None, access: false, from_hand: false, part_of: None }];
            registry.insert(breaker);
        }
        // Inert filler: a blank event, worth nothing held and nothing
        // played. (It was three more copies of Madani, which §52 reads
        // as dead once a console is installed — a second console trashes
        // the first — so the install looked like losing three cards.)
        let mut inert = blank_card("filler", CardType::Event);
        inert.side = Side::Runner;
        registry.insert(inert);
        let filler = || vec![CardId("filler".to_string()); 3];
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

    /// A run event is played for what it puts on the run (Phase 5 §31):
    /// with 1[c], a Cleaver and HQ behind a barrier that costs 2[c] to
    /// break, the Runner plays Overclock and runs HQ on its five run
    /// credits rather than clicking for the credits first — the run the
    /// leaf could not see while it read the Runner's own credits alone,
    /// so the planner played Overclock in 0 of 192 games. The grip holds
    /// three other cards because an emptied grip is the flatline fear,
    /// not the point.
    #[test]
    fn plays_overclock_to_run_through_ice_it_cannot_otherwise_afford() {
        use netrunner_core::dsl::{Effect, IceType, SubroutineDef};
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut wall = blank_card("wall", CardType::Ice(IceType::Barrier));
        wall.strength = Some(3);
        let etr = || SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None };
        wall.subroutines = vec![etr(), etr(), etr()];
        registry.insert(wall);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(1), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.runner.memory_units = MemoryUnits(3);
        state.runner.grip = vec![CardId("overclock".to_string()), CardId("wall".to_string()), CardId("wall".to_string()), CardId("wall".to_string())];
        state.runner.rig = vec![netrunner_core::rules::InstalledRunnerCard { card: CardId("cleaver".to_string()), install_id: InstallId(10), base_strength: 3, ..Default::default() }];
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("wall".to_string()); 3];
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
        let view = build_client_view(&state, &registry, Side::Runner);
        let play = PlayerAction::PlayEvent { card_id: CardId("overclock".to_string()) };
        assert!(view.legal_actions.contains(&play), "{:?}", view.legal_actions);

        let mut agent = PlanningAgent::new(Side::Runner, 3);
        let mut actions = Vec::new();
        for _ in 0..20 {
            match current_actor(&state) {
                Some(Side::Corp) => {
                    let Ok((next, _)) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }) else { break };
                    state = next;
                }
                Some(Side::Runner) => {
                    let view = build_client_view(&state, &registry, Side::Runner);
                    agent.observe(&view);
                    let action = agent.select_action(&view, &registry);
                    assert!(view.legal_actions.contains(&action), "{action:?} is not legal");
                    state = apply_action(&state, &registry, action.clone()).expect("the plan's action applies").0;
                    actions.push(action);
                    if state.active_run.is_some() {
                        break;
                    }
                }
                None => break,
            }
        }
        assert_eq!(actions.first(), Some(&play), "should play Overclock: {actions:?}");
        let run = state.active_run.as_ref().expect("and run on it");
        assert_eq!((run.server, run.bonus_run_credits), (ServerId::Hq, 5), "{actions:?}");
    }

    /// The Corp purges a ripe trojan off its dear ice (Phase 5 §55):
    /// with a Tranquilizer at three counters on its Brân 1.0 — derezzed
    /// by it, and derezzed again every turn it is put back — three
    /// clicks and a hand that offers nothing better, the Corp spends the
    /// turn purging rather than clicking for three credits: the purge
    /// ends three turns of Brân's rez as tax. Before, nothing on the
    /// Corp's side read a program hosted on its ice: a trojan one counter
    /// short on a rezzed piece was purged, since the line sees the derez
    /// at its end, but a piece already down stayed down, and the Corp
    /// re-rezzed it at the Runner's approach instead, 6[c] a run.
    #[test]
    fn purges_a_ripe_trojan_off_its_dear_ice() {
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut dead = blank_card("dead", CardType::Operation);
        dead.side = Side::Corp;
        dead.cost = 99;
        registry.insert(dead);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.turn = 6;
        state.corp.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.corp.identity = Some(CardId("haas_bioroid_precision_design".to_string()));
        state.corp.hq = vec![CardId("dead".to_string()); 4];
        state.corp.r_and_d = vec![CardId("dead".to_string()); 10];
        state.corp.installed = vec![InstalledCard { card: CardId("bran_1_0".to_string()), install_id: InstallId(1), server: ServerId::Hq, slot: InstallSlot::Ice, rezzed: false, ..Default::default() }];
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(0), agenda_points: AgendaPoints(0) };
        state.runner.grip = vec![CardId("dead".to_string()); 4];
        state.runner.rig = vec![netrunner_core::rules::InstalledRunnerCard { card: CardId("tranquilizer".to_string()), install_id: InstallId(10), hosted_on_ice: Some(InstallId(1)), counters: 3, ..Default::default() }];
        state.next_install_id = 20;
        let view = build_client_view(&state, &registry, Side::Corp);
        assert!(view.legal_actions.contains(&PlayerAction::PurgeVirusCounters), "{:?}", view.legal_actions);
        const SEEDS: u64 = 20;
        let purges = (1..=SEEDS)
            .filter(|&seed| {
                // The Corp knows its deck is dead cards, so a draw buys
                // nothing in the sample either: without a deck the sample
                // fills R&D from the pool, and three draws of real cards
                // outscored the purge.
                let dead_deck = netrunner_core::rules::Deck { identity: CardId("haas_bioroid_precision_design".to_string()), cards: vec![(CardId("dead".to_string()), 49)] };
                let mut agent = PlanningAgent::new(Side::Corp, seed).with_knowledge(crate::knowledge::Knowledge::new(netrunner_core::format::NsgFormat::Casual, Some(dead_deck)));
                agent.observe(&view);
                agent.select_action(&view, &registry) == PlayerAction::PurgeVirusCounters
            })
            .count() as u64;
        assert!(purges * 2 > SEEDS, "the Corp purges: {purges} of {SEEDS}");
    }

    /// A trojan that derezzes its host is installed on the dearest rezzed
    /// piece of ice (Phase 5 §54): with 4[c], a Tranquilizer in hand and
    /// a Brân 1.0 rezzed on HQ beside a Whitespace on R&D, the Runner
    /// hosts it on Brân — its 6[c] rez a turn from the third counter —
    /// rather than clicking for credits. Before, a program with no
    /// breaker subtype and no declared income read as a rig card under
    /// its price, and the planner installed Tranquilizer in 0 of 48
    /// games of its decks.
    #[test]
    fn hosts_tranquilizer_on_the_dearest_rezzed_ice() {
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut inert = blank_card("filler", CardType::Event);
        inert.side = Side::Runner;
        registry.insert(inert);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.turn = 4;
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(4), clicks: Clicks(1), agenda_points: AgendaPoints(0) };
        state.runner.memory_units = MemoryUnits(4);
        state.runner.grip = vec![CardId("tranquilizer".to_string()), CardId("filler".to_string()), CardId("filler".to_string()), CardId("filler".to_string())];
        state.runner.stack = vec![CardId("filler".to_string()); 10];
        state.corp.hq = vec![CardId("hedge_fund".to_string()); 3];
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 10];
        state.corp.installed = vec![
            InstalledCard { card: CardId("bran_1_0".to_string()), install_id: InstallId(1), server: ServerId::Hq, slot: InstallSlot::Ice, rezzed: true, ..Default::default() },
            InstalledCard { card: CardId("whitespace".to_string()), install_id: InstallId(2), server: ServerId::RnD, slot: InstallSlot::Ice, rezzed: true, ..Default::default() },
        ];
        state.next_install_id = 20;
        let view = build_client_view(&state, &registry, Side::Runner);
        let on_bran = PlayerAction::InstallProgramOnIce { card_id: CardId("tranquilizer".to_string()), host: InstallId(1), trash_first: false };
        assert!(view.legal_actions.contains(&on_bran), "{:?}", view.legal_actions);
        const SEEDS: u64 = 20;
        let hosted = (1..=SEEDS)
            .filter(|&seed| {
                let mut agent = PlanningAgent::new(Side::Runner, seed);
                agent.observe(&view);
                agent.select_action(&view, &registry) == on_bran
            })
            .count() as u64;
        assert!(hosted * 2 > SEEDS, "Tranquilizer is hosted on Brân 1.0: {hosted} of {SEEDS}");
    }

    /// A run event whose success resolves some of its options is played
    /// for the best of them (Phase 5 §56): with 5[c], a Cleaver, a
    /// Bahia Bands and a Pennyshaver in hand, and the centrals behind a
    /// barrier the rig breaks, the Runner installs Pennyshaver, which
    /// pays on the run, then plays Bahia Bands and runs on it — two
    /// cards and an install without its click for a credit less — where
    /// the same Runner ran bare. Before, the leaf read a
    /// "resolve 2 of the following" rider as nothing, and the planner
    /// had Bahia Bands legal on 172 Runner turns of 48 games of its
    /// decks and played it 0 times.
    #[test]
    fn plays_bahia_bands_for_the_best_two_of_its_options() {
        use netrunner_core::dsl::{Effect, IceType, SubroutineDef};
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut wall = blank_card("wall", CardType::Ice(IceType::Barrier));
        wall.strength = Some(3);
        wall.subroutines = vec![SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None }];
        registry.insert(wall);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.runner.memory_units = MemoryUnits(3);
        state.runner.grip = vec![CardId("bahia_bands".to_string()), CardId("pennyshaver".to_string()), CardId("wall".to_string()), CardId("wall".to_string())];
        state.runner.rig = vec![netrunner_core::rules::InstalledRunnerCard { card: CardId("cleaver".to_string()), install_id: InstallId(10), base_strength: 3, ..Default::default() }];
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("wall".to_string()); 3];
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
        let view = build_client_view(&state, &registry, Side::Runner);
        let play = PlayerAction::PlayEvent { card_id: CardId("bahia_bands".to_string()) };
        assert!(view.legal_actions.contains(&play), "{:?}", view.legal_actions);

        let mut agent = PlanningAgent::new(Side::Runner, 3);
        let mut actions = Vec::new();
        for _ in 0..20 {
            match current_actor(&state) {
                Some(Side::Corp) => {
                    let Ok((next, _)) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }) else { break };
                    state = next;
                }
                Some(Side::Runner) => {
                    let view = build_client_view(&state, &registry, Side::Runner);
                    agent.observe(&view);
                    let action = agent.select_action(&view, &registry);
                    assert!(view.legal_actions.contains(&action), "{action:?} is not legal");
                    state = apply_action(&state, &registry, action.clone()).expect("the plan's action applies").0;
                    actions.push(action);
                    if state.active_run.is_some() {
                        break;
                    }
                }
                None => break,
            }
        }
        // Pennyshaver first, then the event: it pays on the run.
        assert!(actions.contains(&play), "should play Bahia Bands this turn: {actions:?}");
        let run = state.active_run.as_ref().expect("and run on it");
        assert!(run.on_success_effect.is_some(), "the run carries its rider: {actions:?}");
    }

    /// Boomerang is installed on the ice in the way and the run made
    /// through it: with no breaker and HQ behind a rezzed two-subroutine
    /// sentry, "choose 1 installed piece of ice … [trash]: break up to 2
    /// subroutines" is the one way in. Before, a break with no strength
    /// contest broke nothing to the evaluator, and random seats installed
    /// Boomerang 7 times on a pass of the pool where the planner never did.
    #[test]
    fn installs_boomerang_on_the_ice_in_the_way_and_runs_through_it() {
        use netrunner_core::dsl::{Effect, IceType, SubroutineDef};
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut sentry = blank_card("sentry", CardType::Ice(IceType::Sentry));
        sentry.strength = Some(5);
        sentry.subroutines = vec![SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None }; 2];
        registry.insert(sentry);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(4), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.runner.memory_units = MemoryUnits(4);
        state.runner.grip = vec![CardId("boomerang".to_string())];
        state.runner.stack = vec![CardId("sure_gamble".to_string()); 10];
        state.corp.resources.credits = Credits(5);
        // Next to nothing in the centrals, so the remote is the run: a sample
        // that deals R&D an agenda must not outbid the reading under test.
        state.corp.hq = Vec::new();
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];
        state.corp.installed.push(InstalledCard { card: CardId("sentry".to_string()), install_id: InstallId(4), server: ServerId::Remote(0), slot: InstallSlot::Ice, rezzed: true, ..Default::default() });
        state.corp.installed.push(InstalledCard { card: CardId("offworld_office".to_string()), install_id: InstallId(5), server: ServerId::Remote(0), slot: InstallSlot::Root, advancement_tokens: 3, ..Default::default() });
        state.next_install_id = 10;

        let mut agent = PlanningAgent::new(Side::Runner, 3);
        let mut actions = Vec::new();
        for _ in 0..20 {
            match current_actor(&state) {
                Some(Side::Corp) => {
                    let Ok((next, _)) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }) else { break };
                    state = next;
                }
                Some(Side::Runner) => {
                    let view = build_client_view(&state, &registry, Side::Runner);
                    agent.observe(&view);
                    let action = agent.select_action(&view, &registry);
                    assert!(view.legal_actions.contains(&action), "{action:?} is not legal");
                    state = apply_action(&state, &registry, action.clone()).expect("the plan's action applies").0;
                    actions.push(action);
                    if state.active_run.is_some() {
                        break;
                    }
                }
                None => break,
            }
        }
        assert!(state.runner.rig.iter().any(|card| card.card.0 == "boomerang"), "should install Boomerang: {actions:?}");
        assert!(state.active_run.is_some(), "and run: {actions:?}");
    }

    /// An event whose options the opponent chooses between is played for
    /// the worse of them: Wildcat Strike's "the Corp chooses: gain 6[c]
    /// or draw 4 cards" parks a choice of the Corp's, and the line used
    /// to end there with 2[c] and a click spent and nothing gained, so a
    /// Runner with 2[c] clicked for credits instead. Now the Corp's
    /// answer is taken as the one worst for the Runner and the line goes
    /// on, and either answer beats a click for a credit.
    #[test]
    fn plays_wildcat_strike_for_the_worse_of_the_corps_two_answers() {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(2), clicks: Clicks(4), agenda_points: AgendaPoints(0) };
        state.runner.memory_units = MemoryUnits(4);
        state.runner.grip = vec![CardId("wildcat_strike".to_string())];
        state.runner.stack = vec![CardId("sure_gamble".to_string()); 10];
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("hedge_fund".to_string()); 3];
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 5];
        let play = PlayerAction::PlayEvent { card_id: CardId("wildcat_strike".to_string()) };
        assert!(build_client_view(&state, &registry, Side::Runner).legal_actions.contains(&play));

        let mut agent = PlanningAgent::new(Side::Runner, 3);
        let mut actions = Vec::new();
        for _ in 0..20 {
            match current_actor(&state) {
                Some(Side::Corp) => {
                    // The Corp answers the choice with its first option
                    // (the credits) and otherwise passes.
                    let answer = if state.pending_decision.is_some() { PlayerAction::ResolvePendingChoice { option_index: 0 } } else { PlayerAction::PassPriority { side: Side::Corp } };
                    let Ok((next, _)) = apply_action(&state, &registry, answer) else { break };
                    state = next;
                }
                Some(Side::Runner) => {
                    if !matches!(state.phase, GamePhase::Action(Side::Runner)) {
                        break;
                    }
                    let view = build_client_view(&state, &registry, Side::Runner);
                    agent.observe(&view);
                    let action = agent.select_action(&view, &registry);
                    assert!(view.legal_actions.contains(&action), "{action:?} is not legal");
                    state = apply_action(&state, &registry, action.clone()).expect("the plan's action applies").0;
                    actions.push(action);
                    if state.active_run.is_some() || actions.contains(&play) {
                        break;
                    }
                }
                None => break,
            }
        }
        assert!(actions.contains(&play), "should play Wildcat Strike this turn: {actions:?}");
    }

    /// A run event that pays when the run ends is played for what the end
    /// pays (Phase 5 §53): with 6[c], a Cleaver and the centrals behind a
    /// barrier, the Runner plays Bravado and runs — 6[c] plus one for the
    /// wall it passes, against the 3[c] it costs — rather than clicking
    /// for credits. Before, the leaf read a success rider only, and the
    /// planner had Bravado legal on 136 Runner turns of 48 games of its
    /// decks and played it 0 times.
    #[test]
    fn plays_bravado_for_what_the_runs_end_pays() {
        use netrunner_core::dsl::{Effect, IceType, SubroutineDef};
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut wall = blank_card("wall", CardType::Ice(IceType::Barrier));
        wall.strength = Some(3);
        wall.subroutines = vec![SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None }];
        registry.insert(wall);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = empty_runner();
        state.runner.resources = PlayerResources { credits: Credits(6), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.runner.memory_units = MemoryUnits(3);
        state.runner.grip = vec![CardId("bravado".to_string()), CardId("wall".to_string()), CardId("wall".to_string()), CardId("wall".to_string())];
        state.runner.rig = vec![netrunner_core::rules::InstalledRunnerCard { card: CardId("cleaver".to_string()), install_id: InstallId(10), base_strength: 3, ..Default::default() }];
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("wall".to_string()); 3];
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
        let view = build_client_view(&state, &registry, Side::Runner);
        let play = PlayerAction::PlayEvent { card_id: CardId("bravado".to_string()) };
        assert!(view.legal_actions.contains(&play), "{:?}", view.legal_actions);

        let mut agent = PlanningAgent::new(Side::Runner, 3);
        let mut actions = Vec::new();
        for _ in 0..20 {
            match current_actor(&state) {
                Some(Side::Corp) => {
                    let Ok((next, _)) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }) else { break };
                    state = next;
                }
                Some(Side::Runner) => {
                    let view = build_client_view(&state, &registry, Side::Runner);
                    agent.observe(&view);
                    let action = agent.select_action(&view, &registry);
                    assert!(view.legal_actions.contains(&action), "{action:?} is not legal");
                    state = apply_action(&state, &registry, action.clone()).expect("the plan's action applies").0;
                    actions.push(action);
                    if state.active_run.is_some() {
                        break;
                    }
                }
                None => break,
            }
        }
        assert_eq!(actions.first(), Some(&play), "should play Bravado: {actions:?}");
        let run = state.active_run.as_ref().expect("and run on it");
        assert_eq!(run.on_end.len(), 1, "the run carries Bravado's rider: {actions:?}");
    }

    /// A card that pays on a run is installed for what its runs will pay
    /// (Phase 5 §32): with 6[c], four clicks and the centrals behind ICE
    /// the rig cannot break, the Runner installs Red Team over clicking
    /// for credits — four runs of 3[c] over the turns ahead, its click
    /// buying a run the Runner makes anyway — and Pennyshaver beside it,
    /// a credit each successful run. Before, the declared income read
    /// neither (a run's rider and a run's trigger were nobody's income),
    /// and the planner installed them in 0 of 192 games.
    #[test]
    fn installs_the_cards_that_pay_on_a_run() {
        use netrunner_core::dsl::{Effect, IceType, SubroutineDef};
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut wall = blank_card("wall", CardType::Ice(IceType::Barrier));
        wall.strength = Some(5);
        let etr = || SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None };
        wall.subroutines = vec![etr(), etr()];
        registry.insert(wall);

        for (card, cost) in [("red_team", 5), ("pennyshaver", 3)] {
            let mut state = GameState::new(0);
            state.phase = GamePhase::Action(Side::Runner);
            state.runner = empty_runner();
            state.runner.resources = PlayerResources { credits: Credits(cost + 1), clicks: Clicks(4), agenda_points: AgendaPoints(0) };
            state.runner.memory_units = MemoryUnits(4);
            state.runner.grip = vec![CardId(card.to_string()), CardId("wall".to_string()), CardId("wall".to_string()), CardId("wall".to_string())];
            state.corp.resources.credits = Credits(5);
            state.corp.hq = vec![CardId("wall".to_string()); 3];
            state.corp.r_and_d = vec![CardId("wall".to_string()); 10];
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
            let view = build_client_view(&state, &registry, Side::Runner);
            let install = view
                .legal_actions
                .iter()
                .find(|action| match action {
                    PlayerAction::InstallResource { card_id, .. } | PlayerAction::InstallHardware { card_id } => card_id.0 == card,
                    _ => false,
                })
                .cloned()
                .unwrap_or_else(|| panic!("{card} is installable: {:?}", view.legal_actions));
            // The Runner's turn, its clicks and nothing after them.
            let mut agent = PlanningAgent::new(Side::Runner, 3);
            let mut actions = Vec::new();
            while state.phase == GamePhase::Action(Side::Runner) && state.runner.resources.clicks.0 > 0 && actions.len() < 12 {
                if current_actor(&state) != Some(Side::Runner) {
                    let Ok((next, _)) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }) else { break };
                    state = next;
                    continue;
                }
                let view = build_client_view(&state, &registry, Side::Runner);
                agent.observe(&view);
                let action = agent.select_action(&view, &registry);
                state = apply_action(&state, &registry, action.clone()).expect("the plan's action applies").0;
                actions.push(action);
            }
            assert!(actions.contains(&install), "should install {card} this turn: {actions:?}");
        }
    }

    /// An identity's ability is a step of the plan (Phase 5 §33). Topan's
    /// install at 2[credit] less is the only way to Pennyshaver on 1[credit],
    /// and while samples carried no identity it was in the view's list and
    /// no sample's, so the root dropped it and a credit click won.
    #[test]
    fn plays_its_identitys_ability() {
        use netrunner_core::dsl::{Effect, IceType, SubroutineDef};
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut wall = blank_card("wall", CardType::Ice(IceType::Barrier));
        wall.strength = Some(5);
        let etr = || SubroutineDef { text: String::new(), effect: Effect::EndTheRun, only_breakable_by: None };
        wall.subroutines = vec![etr(), etr()];
        registry.insert(wall);

        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = empty_runner();
        state.runner.identity = Some(CardId("topan_ormas_leader".to_string()));
        state.runner.resources = PlayerResources { credits: Credits(1), clicks: Clicks(4), agenda_points: AgendaPoints(0) };
        state.runner.memory_units = MemoryUnits(4);
        state.runner.grip = vec![CardId("pennyshaver".to_string()), CardId("wall".to_string()), CardId("wall".to_string()), CardId("wall".to_string())];
        state.corp.resources.credits = Credits(5);
        state.corp.hq = vec![CardId("wall".to_string()); 3];
        state.corp.r_and_d = vec![CardId("wall".to_string()); 10];
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
        let topan = PlayerAction::ActivateAbility { target: InstallId::RUNNER_IDENTITY, ability_index: 0 };
        let view = build_client_view(&state, &registry, Side::Runner);
        assert!(view.legal_actions.contains(&topan), "the premise: {:?}", view.legal_actions);

        let mut agent = PlanningAgent::new(Side::Runner, 3);
        let mut actions = Vec::new();
        while state.phase == GamePhase::Action(Side::Runner) && state.runner.resources.clicks.0 > 0 && actions.len() < 12 {
            if current_actor(&state) != Some(Side::Runner) {
                let Ok((next, _)) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }) else { break };
                state = next;
                continue;
            }
            let view = build_client_view(&state, &registry, Side::Runner);
            agent.observe(&view);
            let action = agent.select_action(&view, &registry);
            state = apply_action(&state, &registry, action.clone()).expect("the plan's action applies").0;
            actions.push(action);
        }
        assert!(actions.contains(&topan), "should use Topan's install: {actions:?}");
        assert!(state.runner.rig.iter().any(|card| card.card.0 == "pennyshaver"), "and install Pennyshaver with it: {actions:?}");
    }

    /// PT Untaian's "when your discard phase ends … you may pay 1[credit]
    /// to place 1 advancement counter" is asked after the engine has moved
    /// into the Runner's start of turn, and was the one-ply chooser's to
    /// decline (Phase 5 §35). The board is one a recorded game declined
    /// on: Send a Message with a counter on it, the Corp's clicks spent.
    #[test]
    fn takes_its_identitys_advance_at_the_end_of_its_turn() {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.identity = Some(CardId("pt_untaian_lifes_building_blocks".to_string()));
        state.corp.resources = PlayerResources { credits: Credits(8), clicks: Clicks(0), agenda_points: AgendaPoints(0) };
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 10];
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 5];
        state.runner.stack = vec![CardId("sure_gamble".to_string()); 10];
        state.corp.installed.push(InstalledCard {
            card: CardId("send_a_message".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            slot: netrunner_core::rules::InstallSlot::Root,
            advancement_tokens: 1,
            ..Default::default()
        });
        // A trap the Runner has sprung, which another counter buys nothing:
        // the one-ply bound on the selection takes the worst card the
        // filter could match, and this one makes it zero
        // (`fundamentals::advancement_upside`).
        state.corp.installed.push(InstalledCard {
            card: CardId("urtica_cipher".to_string()),
            install_id: InstallId(2),
            server: ServerId::Remote(1),
            slot: netrunner_core::rules::InstallSlot::Root,
            rezzed: true,
            seen_by_runner: true,
            ..Default::default()
        });
        let mut agent = PlanningAgent::new(Side::Corp, 3);
        let mut offered = false;
        for _ in 0..40 {
            if state.phase == GamePhase::Action(Side::Runner) {
                break;
            }
            let Some(actor) = current_actor(&state) else { break };
            offered |= state.pending_paid_choice.is_some();
            let action = if actor == Side::Corp {
                let view = build_client_view(&state, &registry, Side::Corp);
                agent.observe(&view);
                agent.select_action(&view, &registry)
            } else {
                PlayerAction::PassPriority { side: actor }
            };
            state = apply_action(&state, &registry, action).expect("the agent's action applies").0;
        }
        assert!(offered, "the premise: PT Untaian's offer was made");
        assert_eq!(state.corp.installed[0].advancement_tokens, 2, "the counter is placed on Send a Message");
        assert_eq!(state.corp.resources.credits.0, 7, "for 1[credit]");
    }

    /// Synapse Global's "the first time each turn a tag is removed, you may
    /// reveal and install 1 card from HQ, ignoring all costs" is asked of
    /// the Corp in the Runner's turn, where it played one ply and chose
    /// nothing (Phase 5 §36): a selection of its identity's is planned to
    /// the install.
    #[test]
    fn takes_its_identitys_free_install_when_the_runner_removes_a_tag() {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.corp.identity = Some(CardId("synapse_global_faster_than_thought".to_string()));
        state.corp.resources = PlayerResources { credits: Credits(0), clicks: Clicks(0), agenda_points: AgendaPoints(0) };
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 10];
        state.corp.hq = vec![CardId("hedge_fund".to_string()), CardId("pad_campaign".to_string())];
        state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.runner.tags = 1;
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 5];
        state.runner.stack = vec![CardId("sure_gamble".to_string()); 10];
        state = apply_action(&state, &registry, PlayerAction::RemoveTag).expect("the Runner removes its tag").0;
        assert!(state.pending_decision.is_some(), "the premise: Synapse Global asks the Corp");
        let mut agent = PlanningAgent::new(Side::Corp, 3);
        for _ in 0..10 {
            if current_actor(&state) != Some(Side::Corp) {
                break;
            }
            let view = build_client_view(&state, &registry, Side::Corp);
            agent.observe(&view);
            let action = agent.select_action(&view, &registry);
            state = apply_action(&state, &registry, action).expect("the agent's action applies").0;
        }
        assert!(state.pending_decision.is_none(), "the selection is resolved");
        assert!(
            state.corp.installed.iter().any(|card| card.card.0 == "pad_campaign"),
            "PAD Campaign is installed for nothing: {:?}",
            state.corp.installed
        );
        assert_eq!(state.corp.resources.credits.0, 0, "ignoring all costs");
    }

    /// Issuaq Adaptics places a counter, a point, for an agenda "that you
    /// did not install or advance this turn" (§37): the planner advances an
    /// agenda to its requirement and keeps it behind a wall the Runner cannot
    /// break, and scores one that has been ready since an earlier turn.
    #[test]
    fn holds_an_agenda_it_advanced_for_issuaqs_counter_and_scores_one_ready_since() {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let board = |tokens: u32| {
            let mut state = GameState::new(0);
            state.phase = GamePhase::Action(Side::Corp);
            state.corp.identity = Some(CardId("issuaq_adaptics_sustaining_diversity".to_string()));
            state.corp.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
            state.corp.hq = vec![CardId("hedge_fund".to_string()); 3];
            state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 10];
            state.corp.installed = vec![
                InstalledCard { card: CardId("offworld_office".to_string()), install_id: InstallId(1), server: ServerId::Remote(0), slot: netrunner_core::rules::InstallSlot::Root, advancement_tokens: tokens, ..Default::default() },
                InstalledCard { card: CardId("ice_wall".to_string()), install_id: InstallId(2), server: ServerId::Remote(0), slot: netrunner_core::rules::InstallSlot::Ice, rezzed: true, ..Default::default() },
            ];
            state.runner.grip = vec![CardId("sure_gamble".to_string()); 5];
            state.runner.stack = vec![CardId("sure_gamble".to_string()); 10];
            state
        };
        let play_turn = |mut state: GameState| {
            let mut agent = PlanningAgent::new(Side::Corp, 3);
            for _ in 0..20 {
                if current_actor(&state) != Some(Side::Corp) || state.phase != GamePhase::Action(Side::Corp) {
                    break;
                }
                let view = build_client_view(&state, &registry, Side::Corp);
                agent.observe(&view);
                let action = agent.select_action(&view, &registry);
                state = apply_action(&state, &registry, action).expect("the agent's action applies").0;
            }
            state
        };
        let required = registry.get(&CardId("offworld_office".to_string())).and_then(|def| def.advancement_requirement).expect("an agenda");
        let held = play_turn(board(required - 1));
        assert_eq!(held.corp.resources.agenda_points, AgendaPoints(0), "not scored the turn it was advanced");
        assert!(
            held.corp.installed.iter().any(|card| card.install_id == InstallId(1) && card.advancement_tokens >= required),
            "advanced to its requirement and kept: {:?}",
            held.corp.installed
        );
        let scored = play_turn(board(required));
        assert_eq!(scored.corp.resources.agenda_points, AgendaPoints(2), "ready since an earlier turn: scored");
        assert_eq!(scored.corp.identity_counters, 1, "and the counter placed");
    }

    /// AU Co.'s "when your turn begins, you may remove 2 hosted power
    /// counters to look at the top 3 cards of R&D. Trash 1 of those cards
    /// and add the rest to HQ" comes in the Corp's own start of turn, ahead
    /// of the action phase a plan starts from (§37): it is planned, and the
    /// card trashed is not the agenda. With HQ under its floor, where the
    /// evaluator prices the cards it brings; above it a card in HQ is worth
    /// nothing to it (`zone_size_value`), and the search is a tie the line
    /// breaks by what it would play.
    #[test]
    fn takes_au_cos_search_at_its_turn_start_and_keeps_the_agenda() {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut taken = 0;
        for seed in 0..4 {
            let mut state = GameState::new(seed);
            state.phase = GamePhase::Action(Side::Runner);
            state.corp.identity = Some(CardId("au_co_the_gold_standard_in_clones".to_string()));
            state.corp.identity_counters = 2;
            state.corp.resources = PlayerResources { credits: Credits(5), clicks: Clicks(0), agenda_points: AgendaPoints(0) };
            state.corp.hq = vec![CardId("hedge_fund".to_string())];
            // R&D's top is its last card, and the turn's draw takes it first.
            let top = ["offworld_office", "pad_campaign", "hedge_fund", "hedge_fund"].map(|card| CardId(card.to_string()));
            state.corp.r_and_d = [vec![CardId("hedge_fund".to_string()); 10], top.to_vec()].concat();
            state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(0), agenda_points: AgendaPoints(0) };
            state.runner.grip = vec![CardId("sure_gamble".to_string()); 5];
            state.runner.stack = vec![CardId("sure_gamble".to_string()); 10];
            // The Runner ends its turn, and both pass until the Corp's begins.
            for _ in 0..10 {
                if state.pending_paid_choice.is_some() {
                    break;
                }
                let Some(actor) = current_actor(&state) else { break };
                let legal = netrunner_core::rules::legal_actions_for(&state, &registry, actor);
                let action = legal.iter().find(|action| matches!(action, PlayerAction::EndTurn | PlayerAction::PassPriority { .. })).cloned().expect("a pass");
                state = apply_action(&state, &registry, action).expect("the turn moves on").0;
            }
            assert!(state.pending_paid_choice.is_some(), "the premise: AU Co. offers its search");
            // The Corp knows its own deck, so its sample of R&D is this one.
            let deck = netrunner_core::rules::Deck {
                identity: CardId("au_co_the_gold_standard_in_clones".to_string()),
                cards: vec![(CardId("hedge_fund".to_string()), 13), (CardId("offworld_office".to_string()), 1), (CardId("pad_campaign".to_string()), 1)],
            };
            let mut agent = PlanningAgent::new(Side::Corp, seed).with_knowledge(crate::knowledge::Knowledge::new(netrunner_core::format::NsgFormat::Casual, Some(deck)));
            for _ in 0..10 {
                if state.pending_paid_choice.is_none() && state.pending_decision.is_none() {
                    break;
                }
                let view = build_client_view(&state, &registry, Side::Corp);
                agent.observe(&view);
                let action = agent.select_action(&view, &registry);
                state = apply_action(&state, &registry, action).expect("the agent's action applies").0;
            }
            if state.corp.identity_counters == 0 {
                taken += 1;
                assert!(state.corp.hq.iter().any(|card| card.0 == "offworld_office"), "seed {seed}: the agenda is kept: {:?}", state.corp.archives);
            }
        }
        assert_eq!(taken, 4, "the search is taken");
    }

    /// AU Co.'s search is planned at its offer, before the Corp has looked
    /// at the top 3 cards of R&D, and the trash is chosen among them (§40):
    /// with an agenda among the three, it is kept on every seed that takes
    /// the search. Before, the sample's guesses stood at those positions
    /// and the plan made at the offer was followed into the selection, so
    /// the trash fell where the guesses put it: the agenda went in 4 of 7,
    /// and either half of the repair alone — re-planning on the guesses,
    /// or seating the cards in a plan already made — left it at 4 of 7.
    #[test]
    fn au_cos_search_trashes_a_card_it_looked_at_and_keeps_the_agenda() {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let (mut trashed, mut taken) = (Vec::new(), 0);
        for seed in 0..12 {
            let mut state = GameState::new(seed);
            state.phase = GamePhase::Action(Side::Runner);
            state.corp.identity = Some(CardId("au_co_the_gold_standard_in_clones".to_string()));
            state.corp.identity_counters = 2;
            state.corp.resources = PlayerResources { credits: Credits(5), clicks: Clicks(0), agenda_points: AgendaPoints(0) };
            state.corp.hq = vec![CardId("hedge_fund".to_string())];
            // R&D's top is its last card: the search looks at the agenda
            // and two cards that are not, with the deck's other agendas
            // and assets below them.
            let top = ["offworld_office", "hedge_fund", "pad_campaign"].map(|card| CardId(card.to_string()));
            let below = ["send_a_message", "pad_campaign", "offworld_office", "hedge_fund", "send_a_message", "pad_campaign"].map(|card| CardId(card.to_string()));
            state.corp.r_and_d = [below.to_vec(), top.to_vec()].concat();
            state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(0), agenda_points: AgendaPoints(0) };
            state.runner.grip = vec![CardId("sure_gamble".to_string()); 5];
            state.runner.stack = vec![CardId("sure_gamble".to_string()); 10];
            for _ in 0..10 {
                if state.pending_paid_choice.is_some() {
                    break;
                }
                let Some(actor) = current_actor(&state) else { break };
                let legal = netrunner_core::rules::legal_actions_for(&state, &registry, actor);
                let action = legal.iter().find(|action| matches!(action, PlayerAction::EndTurn | PlayerAction::PassPriority { .. })).cloned().expect("a pass");
                state = apply_action(&state, &registry, action).expect("the turn moves on").0;
            }
            assert!(state.pending_paid_choice.is_some(), "the premise: AU Co. offers its search");
            let deck = netrunner_core::rules::Deck {
                identity: CardId("au_co_the_gold_standard_in_clones".to_string()),
                cards: vec![
                    (CardId("hedge_fund".to_string()), 3),
                    (CardId("offworld_office".to_string()), 2),
                    (CardId("send_a_message".to_string()), 2),
                    (CardId("pad_campaign".to_string()), 3),
                ],
            };
            let mut agent = PlanningAgent::new(Side::Corp, seed).with_knowledge(crate::knowledge::Knowledge::new(netrunner_core::format::NsgFormat::Casual, Some(deck)));
            let archived = state.corp.archives.len();
            for _ in 0..10 {
                if state.pending_paid_choice.is_none() && state.pending_decision.is_none() {
                    break;
                }
                let view = build_client_view(&state, &registry, Side::Corp);
                agent.observe(&view);
                let action = agent.select_action(&view, &registry);
                state = apply_action(&state, &registry, action).expect("the agent's action applies").0;
            }
            if state.corp.identity_counters == 0 {
                taken += 1;
                trashed.extend(state.corp.archives[archived..].iter().map(|card| card.card.0.clone()));
            }
        }
        assert!(taken >= 4, "the search is taken: {taken} of 12");
        assert!(!trashed.iter().any(|card| card == "offworld_office"), "the agenda is kept: {trashed:?}");
    }

    /// Poétrï Luxury Brands' "whenever an agenda is stolen, you may install
    /// 1 non-agenda card from HQ" comes in the Runner's turn, as the run
    /// that stole it ends (§36).
    #[test]
    fn installs_from_hq_when_its_agenda_is_stolen() {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.corp.identity = Some(CardId("poetri_luxury_brands_all_the_rage".to_string()));
        state.corp.resources = PlayerResources { credits: Credits(5), clicks: Clicks(0), agenda_points: AgendaPoints(0) };
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 10];
        state.corp.hq = vec![CardId("hedge_fund".to_string()), CardId("pad_campaign".to_string())];
        state.corp.installed.push(InstalledCard {
            card: CardId("offworld_office".to_string()),
            install_id: InstallId(1),
            server: ServerId::Remote(0),
            slot: netrunner_core::rules::InstallSlot::Root,
            ..Default::default()
        });
        state.runner.resources = PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) };
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 5];
        state.runner.stack = vec![CardId("sure_gamble".to_string()); 10];
        state = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("the Runner runs").0;
        let mut agent = PlanningAgent::new(Side::Corp, 3);
        let mut asked = false;
        for _ in 0..60 {
            if state.active_run.is_none() && state.pending_decision.is_none() {
                break;
            }
            let Some(actor) = current_actor(&state) else { break };
            let view = build_client_view(&state, &registry, actor);
            let action = if actor == Side::Corp {
                asked |= state.pending_decision.is_some();
                agent.observe(&view);
                agent.select_action(&view, &registry)
            } else {
                // The Runner goes on and steals what it finds.
                let pick = |action: &&PlayerAction| {
                    matches!(action, PlayerAction::ContinueRun | PlayerAction::SelectCardToAccess { .. } | PlayerAction::StealAgenda { .. } | PlayerAction::PassPriority { .. })
                };
                view.legal_actions.iter().find(pick).cloned().unwrap_or_else(|| view.legal_actions[0].clone())
            };
            state = apply_action(&state, &registry, action).expect("the action applies").0;
        }
        assert!(asked, "the premise: Poétrï asks the Corp");
        assert_eq!(state.runner.resources.agenda_points.0, 2, "the premise: Offworld Office is stolen");
        assert!(
            state.corp.installed.iter().any(|card| card.card.0 == "pad_campaign"),
            "PAD Campaign is installed from HQ: {:?}",
            state.corp.installed
        );
    }

    /// Tāo Salonga steals an agenda on its last click, with a code gate the
    /// rig cannot break and an Ice Wall Corroder breaks, one on R&D and
    /// one on a remote holding nothing (§38). It swaps the code gate off
    /// R&D, which opens R&D for the turns to come, and leaves it on the
    /// remote, where a swap would shut R&D. With the reading off a run
    /// switched off, the swap and the decline tie exactly, and the jitter
    /// took the swap that shuts R&D; with the choice played one ply, the
    /// swap is never taken.
    #[test]
    fn tao_salonga_swaps_the_piece_the_rig_cannot_break_off_rnd_and_never_onto_it() {
        let on_rnd = |state: &GameState| -> Vec<String> {
            state.corp.installed.iter().filter(|card| card.server == ServerId::RnD).map(|card| card.card.0.clone()).collect()
        };
        for seed in 0..6 {
            let state = tao_steals(seed, "enigma", "ice_wall");
            assert_eq!(on_rnd(&state), ["ice_wall"], "seed {seed}: Ice Wall is swapped onto R&D: {:?}", state.corp.installed);
            let state = tao_steals(seed, "ice_wall", "enigma");
            assert_eq!(on_rnd(&state), ["ice_wall"], "seed {seed}: Ice Wall stays on R&D: {:?}", state.corp.installed);
        }
    }

    /// A Tāo Salonga Runner's last-click run on an Offworld Office, stolen,
    /// with `on_rnd` protecting R&D and `on_remote` an empty remote, and
    /// the identity's swap answered by a planner seeded `seed`.
    fn tao_steals(seed: u64, on_rnd: &str, on_remote: &str) -> GameState {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.identity = Some(CardId("tao_salonga_telepresence_magician".to_string()));
        state.corp.resources = PlayerResources { credits: Credits(0), clicks: Clicks(0), agenda_points: AgendaPoints(0) };
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 10];
        state.corp.hq = vec![CardId("hedge_fund".to_string()); 2];
        let installed = |card: &str, id: u32, server: ServerId, slot: netrunner_core::rules::InstallSlot| InstalledCard {
            card: CardId(card.to_string()),
            install_id: InstallId(id),
            server,
            slot,
            rezzed: slot == netrunner_core::rules::InstallSlot::Ice,
            ..Default::default()
        };
        use netrunner_core::rules::InstallSlot;
        state.corp.installed = vec![
            installed("offworld_office", 1, ServerId::Remote(0), InstallSlot::Root),
            installed(on_rnd, 2, ServerId::RnD, InstallSlot::Ice),
            installed(on_remote, 3, ServerId::Remote(1), InstallSlot::Ice),
        ];
        // The run is the turn's last click: with clicks left the line goes
        // on to run R&D through the Ice Wall, and that run's leaf prices
        // the swap without any reading off a run.
        state.runner.resources = PlayerResources { credits: Credits(8), clicks: Clicks(1), agenda_points: AgendaPoints(0) };
        state.runner.rig = vec![netrunner_core::rules::InstalledRunnerCard { card: CardId("corroder".to_string()), install_id: InstallId(4), base_strength: 2, ..Default::default() }];
        state.runner.grip = vec![CardId("sure_gamble".to_string()); 5];
        state.runner.stack = vec![CardId("sure_gamble".to_string()); 10];
        state = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Remote(0) }).expect("the Runner runs").0;
        let mut agent = PlanningAgent::new(Side::Runner, seed);
        let mut asked = false;
        for _ in 0..60 {
            if state.active_run.is_none() && state.pending_decision.is_none() {
                break;
            }
            let Some(actor) = current_actor(&state) else { break };
            let view = build_client_view(&state, &registry, actor);
            let action = if actor == Side::Runner && state.pending_decision.is_some() {
                asked = true;
                agent.observe(&view);
                agent.select_action(&view, &registry)
            } else {
                // Both sides go on, and the Runner steals what it finds.
                let pick = |action: &&PlayerAction| {
                    matches!(action, PlayerAction::ContinueRun | PlayerAction::SelectCardToAccess { .. } | PlayerAction::StealAgenda { .. } | PlayerAction::PassPriority { .. })
                };
                view.legal_actions.iter().find(pick).cloned().unwrap_or_else(|| view.legal_actions[0].clone())
            };
            state = apply_action(&state, &registry, action).expect("the action applies").0;
        }
        assert!(asked, "the premise: Tāo Salonga asks the Runner");
        assert_eq!(state.runner.resources.agenda_points.0, 2, "the premise: Offworld Office is stolen");
        state
    }

    /// Barry "Baz" Wong's "whenever the Corp rezzes a piece of ice, you may
    /// install 1 resource or piece of hardware from your grip" comes in
    /// the Runner's run, as the ice is rezzed (§39). Planned, Open Market
    /// is installed for no click when the run can still break Ice Wall
    /// after paying for it, and declined when the install would spend the
    /// credit Corroder breaks with; played one ply, it was never taken.
    #[test]
    fn barry_installs_from_the_grip_while_the_run_can_still_break() {
        for seed in 0..4 {
            for credits in [3, 4] {
                let state = barry_meets_a_rez(seed, credits);
                assert!(state.runner.rig.iter().any(|card| card.card.0 == "open_market"), "seed {seed}, {credits}[c]: Open Market is installed");
                assert_eq!(state.this_turn.times(netrunner_core::dsl::Trigger::OnSuccessfulRun), 1, "seed {seed}, {credits}[c]: and the run still gets in");
            }
            let state = barry_meets_a_rez(seed, 2);
            assert!(!state.runner.rig.iter().any(|card| card.card.0 == "open_market"), "seed {seed}, 2[c]: the credits are the break's");
            assert_eq!(state.this_turn.times(netrunner_core::dsl::Trigger::OnSuccessfulRun), 1, "seed {seed}, 2[c]: and the run gets in");
        }
    }

    /// A Barry "Baz" Wong Runner with Corroder and `credits` runs HQ, the
    /// Corp rezzes Ice Wall as it is approached, and the identity's "may"
    /// is answered by a planner seeded `seed`, and the rest of the run played
    /// by the same planner. Returns the state when the run ends.
    fn barry_meets_a_rez(seed: u64, credits: u32) -> GameState {
        use netrunner_core::rules::InstallSlot;
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        let mut state = GameState::new(0);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.identity = Some(CardId("barry_baz_wong_tri_maf_veteran".to_string()));
        state.corp.resources = PlayerResources { credits: Credits(5), clicks: Clicks(0), agenda_points: AgendaPoints(0) };
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string()); 10];
        state.corp.hq = vec![CardId("hedge_fund".to_string()); 4];
        state.corp.installed = vec![InstalledCard { card: CardId("ice_wall".to_string()), install_id: InstallId(1), server: ServerId::Hq, slot: InstallSlot::Ice, ..Default::default() }];
        state.runner.resources = PlayerResources { credits: Credits(credits), clicks: Clicks(1), agenda_points: AgendaPoints(0) };
        state.runner.rig = vec![netrunner_core::rules::InstalledRunnerCard { card: CardId("corroder".to_string()), install_id: InstallId(2), base_strength: 2, ..Default::default() }];
        state.runner.grip = vec![CardId("open_market".to_string()), CardId("sure_gamble".to_string()), CardId("sure_gamble".to_string())];
        state.runner.stack = vec![CardId("sure_gamble".to_string()); 10];
        state = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq }).expect("the Runner runs").0;
        let mut agent = PlanningAgent::new(Side::Runner, seed);
        let mut asked = false;
        for _ in 0..80 {
            if state.active_run.is_none() && state.pending_decision.is_none() {
                break;
            }
            let Some(actor) = current_actor(&state) else { break };
            let view = build_client_view(&state, &registry, actor);
            let action = if actor == Side::Corp {
                let rez = PlayerAction::RezIce { ice: InstallId(1) };
                if view.legal_actions.contains(&rez) { rez } else { PlayerAction::PassPriority { side: Side::Corp } }
            } else {
                asked |= state.pending_decision.is_some() && state.active_run.as_ref().is_some_and(|run| run.position == 0);
                agent.observe(&view);
                agent.select_action(&view, &registry)
            };
            state = apply_action(&state, &registry, action).expect("the action applies").0;
        }
        assert!(asked, "the premise: Barry asks the Runner");
        state
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
            part_of: None,
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
            subject: None, when: None, acts_on_subject: false, first_each_turn: false, first_each_encounter: false, granted: false, from_discard: false, from_runner_score_area: false, while_unrezzed: false,
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
