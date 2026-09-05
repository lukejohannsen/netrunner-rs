//! The one match decision loop: `current_actor` → get that side's action →
//! `apply_action` → check `GameOver`, with a single `MAX_STEPS`.
//!
//! Five places used to own a copy of this — the local single-player runner,
//! the authoritative server, the RL environment's opponent fast-forward,
//! the self-play generator, and one integration test — each with its own
//! step budget and its own idea of what a seat may see. The local ones
//! handed seats the raw `GameState`, so masking there was the client
//! choosing to be polite rather than an interface enforcing anything.
//!
//! # Pulled, not pushed
//!
//! `step` is a *step function*: it resolves as far as it can on its own and
//! then hands control back. It never blocks on a seat and never awaits, so
//! the same loop serves a synchronous terminal, an async socket, a Python
//! RL caller and a PUCT search. **Sync vs. async is a property of who pumps
//! the session, not a reason to fork rules flow** — that is the whole
//! reason this shape was chosen over a blocking `run(seats)` that calls
//! into its seats. The two awkward consumers make it concrete: self-play
//! needs `PuctAgent::search`'s visit counts rather than a bare chosen
//! action, and the gym environment's action arrives from Python long after
//! the session would have had to ask for it.

use netrunner_bots::BotAgent;
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{
    apply_action, current_actor, GamePhase, GameState, PendingDecision, PlayerAction, RulesError, Side, Viewer,
};
use netrunner_core::view::{build_client_view, ClientView};

use crate::history::{HistoryEntry, MatchHistory, PublicHistoryEntry};
use crate::outcome::{classify_end_reason, GameEndReason};

/// Guard against a stalled or looping game running forever. One budget for
/// every consumer — this constant previously existed in four places.
///
/// **Sized from data, not caution.** It was 10,000, which is not a bound on
/// anything a real game does: the Corp's mandatory draw plus the deck-out
/// rule end any game whose turns advance within ~45 Corp turns, and the
/// longest legitimate game in 10,800 recorded self-play games (median 225
/// actions) was **1,992**; over the full 192-matchup cross product the
/// longest random-vs-random game is 1,002, heuristic-vs-random 861,
/// random-vs-heuristic 1,401 (p99 1,159). A game at 10,000 actions is one
/// whose turns have stopped — a card-selection livelock, which
/// `DECISION_BUDGET` now catches at 256 — and letting it run there cost
/// ~40 ordinary games of compute per stall and 44% of every recorded
/// decision in one training window (ROADMAP Phase 2 §5). 2,500 is 25%
/// above anything ever observed and a quarter of the old tax, and both
/// deadlock sweeps — which no longer tolerate budget exhaustion for any
/// seating — are the check that it clips nothing.
pub const MAX_STEPS: u32 = 2_500;

/// How many consecutive applied actions one parked decision may absorb
/// before the session reports `StallReason::DecisionLivelock`.
///
/// Sized against the largest *legitimate* resolution and the smallest
/// useful bound. A bot that never deselects resolves any `ChooseCards` in
/// at most `max + 1` actions, and the widest zone a prompt selects from is
/// `MAX_DECK_ZONE` (50), so 51 is the ceiling for a well-behaved chooser
/// and 256 is five times that. It is also more toggling than a human does
/// in the TUI — which pumps this same `Session` — and forty times cheaper
/// than letting a loop run to `MAX_STEPS`, which is what a livelocked
/// self-play game cost before this existed: ~9,800 recorded decisions of
/// one Corp toggling, 44% of every decision in iterations 5–8 of the third
/// volume run.
pub const DECISION_BUDGET: u32 = 256;

/// How one side's decisions get made.
///
/// **Two variants, not four.** An earlier sketch of this work proposed
/// `Bot`/`LocalHuman`/`Channel`/`Indexed`. Under a pull-shaped loop those
/// last three are the same thing — a seat the session cannot resolve by
/// itself — and they differ only in *who* pumps the session and in whether
/// the pump speaks `PlayerAction` or a fixed `ActionSpace` index. That
/// index conversion is `ActionSpace::action_at`'s job at the boundary, not
/// a kind of seat. Splitting them here would have put four variants in this
/// crate's permanent surface to describe one distinction.
pub enum Seat {
    /// Resolved in-process, synchronously, from a masked `ClientView` —
    /// the seat never sees `GameState`, so fog of war holds structurally.
    Agent(Box<dyn BotAgent>),
    /// Resolved by whoever pumps the session: a channel, a terminal, a
    /// Python caller, a search. `step` yields `Awaiting` and stops until
    /// `submit` supplies the action.
    External,
}

/// Why a session stopped without reaching `GameOver`.
///
/// Every previous copy of this loop wrote `let Some(side) =
/// current_actor(&state) else { break }` alongside `for _ in 0..MAX_STEPS`,
/// which left callers unable to tell these apart — each then reported
/// whichever one its error message happened to name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StallReason {
    /// `current_actor` named nobody. Expected and benign at
    /// `GamePhase::StartOfTurn`, which resolves inside `apply_action`
    /// rather than between actions.
    NoCurrentActor,
    /// The side to act has no legal action at all — the deadlock shape
    /// `no_panics_or_deadlocks_across_many_seeds_system_gateway` hunts for.
    /// Reported rather than panicked because every `BotAgent` in the
    /// workspace asserts a non-empty `legal_actions` (`RandomAgent`,
    /// `PuctAgent`), so dispatching into one here would abort the process.
    /// The sweep still catches it: it asserts the match reached `GameOver`.
    NoLegalActions { side: Side },
    /// `MAX_STEPS` actions were applied without the game ending.
    BudgetExhausted,
    /// One parked decision absorbed `DECISION_BUDGET` consecutive actions
    /// without resolving. A **livelock**, and the thing `BudgetExhausted`
    /// was hiding: every 10,000-action "stall" in three volume runs was a
    /// Corp toggling the same 3–5 HQ cards on and off inside a
    /// `min == max` `ChooseCards` prompt — 98.7% `ToggleCardSelection`,
    /// Confirm taken once in 9,888 steps — while turns never advanced, so
    /// the deck-out rule that would otherwise have ended the game in ~45
    /// Corp turns never got the chance (ROADMAP Phase 2 §5). Reported with
    /// the card that parked the decision, because the question a stall
    /// raises is *which card*, and the pending decision already knows.
    DecisionLivelock { side: Side, source_card: Option<CardId>, actions: u32 },
}

/// What one `step` did. Deliberately **owned**, borrowing nothing from the
/// session: a caller holding an `Awaiting` has to be able to call `submit`,
/// which a borrow of `self` would forbid. Detail about an `Applied` step
/// comes from `Session::last_entry` instead.
///
/// `Debug` so a pump can name an unexpected step in its own error message —
/// every consumer has some "this should have been `Ended`" path. It is only
/// derivable because the variant payloads stop at `ClientView`; `Seat`
/// cannot derive it, since it holds a `Box<dyn BotAgent>`.
#[derive(Debug)]
pub enum SessionStep {
    /// A `Seat::Agent` chose and its action was applied.
    Applied { side: Side },
    /// `side` is a `Seat::External` and must supply an action via `submit`.
    Awaiting { side: Side, view: Box<ClientView> },
    /// The game reached `GamePhase::GameOver`. Idempotent — every
    /// subsequent `step` returns this again, so a polling pump is safe.
    Ended { winner: Side, reason: GameEndReason },
    Stalled(StallReason),
}

/// Why a `submit` was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SubmitError {
    /// The match is already over.
    #[error("the match has already ended")]
    Ended,
    /// Nothing is awaiting an action and `current_actor` names nobody, so
    /// there is no side to attribute the action to.
    #[error("no side has a decision pending")]
    NoActor,
    /// The engine rejected the action. The state is unchanged and the same
    /// side is still awaiting.
    ///
    /// `transparent`, so `RulesError`'s own authored message is what a
    /// caller displays. It used to be `"the engine rejected the action:
    /// {0:?}"`, which printed the `Debug` struct literal and so hid all 97
    /// `#[error(...)]` messages in `rules::error` from every user of this
    /// type — including the two places a player actually reads a rejection,
    /// `MatchSession`'s `ActionRejected` and the TUI's `last_rejection`.
    /// The prefix carried nothing a caller cannot say itself, and this
    /// variant is a pure wrapper, which is exactly what `transparent` is
    /// for (`SelfPlayError::Rules` already relates to it the same way).
    #[error(transparent)]
    Rules(#[from] RulesError),
}

/// One match: the authoritative `GameState`, its two seats, and the action
/// log accumulated along the way.
pub struct Session {
    state: GameState,
    registry: CardRegistry,
    corp: Seat,
    runner: Seat,
    history: MatchHistory,
    record_history: bool,
    max_steps: u32,
    steps: u32,
    /// See `DECISION_BUDGET`.
    decision_budget: u32,
    /// Consecutive applied actions during which `state.pending_decision`
    /// has stayed `Some`. Reset the moment it clears, so a chain of prompts
    /// that each resolve promptly never accumulates; only a decision that
    /// keeps *not* resolving counts up.
    decision_actions: u32,
    /// Set by `Awaiting`, cleared by a successful `submit`. Only ever an
    /// attribution hint — never a gate on what `submit` accepts; see its
    /// doc comment.
    awaiting: Option<Side>,
    /// Computed at the moment `GameOver` was produced, because
    /// `classify_end_reason` needs that action's own `GameEvent`s and they
    /// are gone by the time a later `step` observes the phase — and are
    /// never in `history` at all when recording is off.
    ended: Option<(Side, GameEndReason)>,
}

impl Session {
    pub fn new(state: GameState, registry: CardRegistry, corp: Seat, runner: Seat) -> Self {
        Session {
            state,
            registry,
            corp,
            runner,
            history: MatchHistory::new(),
            record_history: true,
            max_steps: MAX_STEPS,
            steps: 0,
            decision_budget: DECISION_BUDGET,
            decision_actions: 0,
            awaiting: None,
            ended: None,
        }
    }

    /// Override `DECISION_BUDGET` — the number of consecutive actions one
    /// parked decision may absorb before `StallReason::DecisionLivelock`.
    pub fn with_decision_budget(mut self, decision_budget: u32) -> Self {
        self.decision_budget = decision_budget;
        self
    }

    /// Stop recording a `MatchHistory`. For a long-running RL environment,
    /// whose episodes accumulate a log nothing will ever read.
    pub fn without_history(mut self) -> Self {
        self.record_history = false;
        self
    }

    /// Override the `MAX_STEPS` safety budget.
    pub fn with_max_steps(mut self, max_steps: u32) -> Self {
        self.max_steps = max_steps;
        self
    }

    /// Advances the match as far as it can without outside help.
    ///
    /// Resolves a `Seat::Agent`'s decision inline and returns `Applied`;
    /// returns `Awaiting` when the side to act is `Seat::External`. Only an
    /// applied action consumes budget, so a caller may poll `step` freely —
    /// a TUI on a 100ms render tick, or a server re-entering after a stray
    /// message, cannot exhaust `MAX_STEPS` by waiting. Prefer `awaiting()`
    /// for that poll, though: `step` rebuilds a `ClientView` each time, and
    /// `build_client_view` clones a `GameState` per legality candidate.
    ///
    /// **Turn-numbering convention:** each history entry is recorded under
    /// `GameState::turn` as read from the state the action was chosen
    /// *against*, before `apply_action` produces the next one. So `0`
    /// covers every Mulligan-phase action, turn `1` is Corp's opening turn,
    /// and the action that causes a turn transition is itself still logged
    /// under the turn that was ending — reading the pre-action state is
    /// precisely what preserves that last property.
    pub fn step(&mut self) -> SessionStep {
        if let GamePhase::GameOver(winner) = self.state.phase {
            // `ended` is set by whichever `apply` produced GameOver. A
            // session handed an already-finished state never had that
            // chance, so fall back to classifying with no events.
            let (winner, reason) = self
                .ended
                .unwrap_or_else(|| (winner, classify_end_reason(&[], winner, &self.state)));
            return SessionStep::Ended { winner, reason };
        }
        let Some(side) = current_actor(&self.state) else {
            return SessionStep::Stalled(StallReason::NoCurrentActor);
        };
        if self.steps >= self.max_steps {
            return SessionStep::Stalled(StallReason::BudgetExhausted);
        }
        if let Some(decision) = self.state.pending_decision.as_ref().filter(|_| self.decision_actions >= self.decision_budget) {
            // `side` is the chooser: `current_actor` puts a parked
            // decision's owner ahead of everything but a trace or a paid
            // choice, and neither of those can loop.
            let source_card = match decision {
                PendingDecision::ChooseCards { source_card, .. }
                | PendingDecision::ChooseEffect { source_card, .. }
                | PendingDecision::ChooseServer { source_card, .. } => source_card.clone(),
                PendingDecision::ChooseTriggerOrder { .. } => None,
            };
            return SessionStep::Stalled(StallReason::DecisionLivelock {
                side,
                source_card,
                actions: self.decision_actions,
            });
        }

        let view = build_client_view(&self.state, &self.registry, side);
        if view.legal_actions.is_empty() {
            return SessionStep::Stalled(StallReason::NoLegalActions { side });
        }

        // Destructured rather than `self.seat_mut(side)` so the agent can
        // be borrowed mutably while `registry` is borrowed immutably.
        let Session { registry, corp, runner, .. } = self;
        let seat = match side {
            Side::Corp => corp,
            Side::Runner => runner,
        };
        let action = match seat {
            Seat::External => {
                self.awaiting = Some(side);
                return SessionStep::Awaiting { side, view: Box::new(view) };
            }
            Seat::Agent(agent) => agent.select_action(&view, registry),
        };

        // A `BotAgent` only ever picks from `view.legal_actions`, so a
        // rejection here is a bug in the agent, not a recoverable
        // condition — unlike `submit`, whose caller may be an arbitrary
        // remote client.
        self.apply(side, action)
            .unwrap_or_else(|error| panic!("Seat::Agent for {side:?} chose an action apply_action rejected: {error:?}"));
        SessionStep::Applied { side }
    }

    /// Supplies an action from outside the session.
    ///
    /// On `Err` the state is **unchanged** and the same side stays
    /// awaiting, so the next `step` re-yields its `Awaiting`. Both external
    /// pumps depend on exactly that: the server answers a bad client action
    /// with `ActionRejected` and keeps the match alive, and the RL
    /// environment scores an illegal index as a penalty without ending the
    /// episode.
    ///
    /// **Deliberately not gated on a prior `Awaiting`, and deliberately not
    /// filtered by that side's `legal_actions`.** `get_action_mask` is
    /// side-agnostic — `legal_actions` documents that per-side legality is
    /// *not* gated by `current_actor`, because e.g. `RezIce` is legal for
    /// the Corp during a Runner-priority window — and the RL environment
    /// submits straight from that mask without consulting `current_actor`
    /// at all. Re-deriving legality here would reject actions the engine
    /// accepts today and silently shift the training distribution. The
    /// engine's own guards in `apply_action` are the only authority;
    /// `awaiting` merely names the side to record the entry under.
    pub fn submit(&mut self, action: PlayerAction) -> Result<(), SubmitError> {
        if matches!(self.state.phase, GamePhase::GameOver(_)) {
            return Err(SubmitError::Ended);
        }
        let side = self.awaiting.or_else(|| current_actor(&self.state)).ok_or(SubmitError::NoActor)?;
        self.apply(side, action).map_err(SubmitError::Rules)?;
        self.awaiting = None;
        Ok(())
    }

    /// Pumps `step` until the match ends, stalls, or needs an external
    /// seat's action. An all-`Seat::Agent` session plays a whole match in
    /// one call.
    ///
    /// Intermediate `Applied` steps are swallowed. A caller that needs each
    /// one as a *viewer* — a UI action log — must pump `step` itself and
    /// read `last_entry_for` after each `Applied`, because that mask is
    /// taken against the state the action left. A caller that wants the
    /// raw entries (the lesson gate, coverage) can still take a mark first
    /// and diff the history afterwards:
    ///
    /// ```ignore
    /// let mark = session.history().len();
    /// let outcome = session.run();
    /// for entry in &session.history().entries()[mark..] { /* ... */ }
    /// ```
    pub fn run(&mut self) -> SessionStep {
        loop {
            match self.step() {
                SessionStep::Applied { .. } => continue,
                other => return other,
            }
        }
    }

    /// Applies, then records — never the reverse. Recording a rejected
    /// action would put it in the log and break the replay invariant
    /// `history_records_every_resolved_action_with_matching_turn_and_side`
    /// asserts: replaying every recorded action from a fresh setup must
    /// reproduce the final state exactly. That also means nothing here may
    /// ever advance the state except through a recorded action.
    fn apply(&mut self, side: Side, action: PlayerAction) -> Result<(), RulesError> {
        let (next, events) = apply_action(&self.state, &self.registry, action.clone())?;
        if let GamePhase::GameOver(winner) = next.phase {
            self.ended = Some((winner, classify_end_reason(&events, winner, &next)));
        }
        // `self.state` is still the pre-action state here — that is what
        // logs a turn-ending action under the turn it ended, rather than
        // the one it started.
        if self.record_history {
            self.history.record(self.state.turn, side, action, events);
        }
        self.state = next;
        self.steps += 1;
        // Only an action that leaves a decision parked counts toward the
        // livelock budget, and resolving one clears the count — so the
        // budget measures "how long has *this* decision been open", not
        // "how many prompts has this game had".
        if self.state.pending_decision.is_some() {
            self.decision_actions += 1;
        } else {
            self.decision_actions = 0;
        }
        Ok(())
    }

    /// Which side, if any, `step` last reported as `Awaiting`. Lets a pump
    /// poll without paying for a `ClientView` rebuild.
    pub fn awaiting(&self) -> Option<Side> {
        self.awaiting
    }

    pub fn state(&self) -> &GameState {
        &self.state
    }

    pub fn registry(&self) -> &CardRegistry {
        &self.registry
    }

    pub fn history(&self) -> &MatchHistory {
        &self.history
    }

    /// The most recently recorded action. Always `None` when the session
    /// was built `without_history`.
    pub fn last_entry(&self) -> Option<&HistoryEntry> {
        self.history.entries().last()
    }

    /// The most recently recorded action as `viewer` may see it — the only
    /// form of a log entry that should ever reach a seat. Valid immediately
    /// after the `submit`/`Applied` that produced it: the mask reads the
    /// current state, which is that action's post-state and nobody else's.
    /// A pump that lets several actions resolve before reading the log
    /// (the TUI's bot turns) must therefore `step` and read after each.
    pub fn last_entry_for(&self, viewer: impl Into<Viewer>) -> Option<PublicHistoryEntry> {
        self.last_entry().map(|entry| entry.for_viewer(&self.state, viewer))
    }

    /// The masked view `viewer` is entitled to — a seat's, or a
    /// spectator's. The only way any caller should be rendering this match.
    pub fn view_for(&self, viewer: impl Into<Viewer>) -> ClientView {
        build_client_view(&self.state, &self.registry, viewer)
    }

    /// How many actions have been applied, against the step budget.
    pub fn steps(&self) -> u32 {
        self.steps
    }

    /// Consumes the session for its final state and log.
    pub fn into_parts(self) -> (GameState, MatchHistory) {
        (self.state, self.history)
    }
}
