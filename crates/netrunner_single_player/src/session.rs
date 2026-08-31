//! The index-based adapter over `netrunner_session::Session`: drives a full
//! match synchronously against two `netrunner_bots::Agent`s, which choose a
//! fixed `ActionSpace` index rather than a `PlayerAction`.
//!
//! **This is the one place a seat still sees the raw `GameState`, and that
//! is deliberate.** `netrunner_bots::Agent` exists for the RL path —
//! `netrunner_gym`, `netrunner_selfplay`, and `IndexedOnnxAgent`, whose
//! policy network is shaped around a whole `GameState` and a fixed action
//! space. Every *other* seat in the workspace now goes through
//! `netrunner_session::Seat::Agent` and can only ever see a masked
//! `ClientView`. Confining the unmasked view to this adapter is the point:
//! it used to be the shape of the local human path too, which meant the
//! local TUI masked itself only by convention.
//!
//! The decision loop itself is gone from here — `current_actor`,
//! `apply_action`, `MAX_STEPS` and the `MatchHistory` all live in
//! `netrunner_session::Session`. What remains is the index ↔ `PlayerAction`
//! conversion at the seat boundary.

use netrunner_bots::Agent;
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{get_action_mask, ActionSpace, GameState, Side};
use netrunner_session::{MatchHistory, Seat, Session, SessionStep};

pub use netrunner_session::MAX_STEPS;

/// A local, synchronous single-match host: owns the real `GameState` and
/// runs it to completion against two index-based agents, recording every
/// resolved action into a `MatchHistory`.
pub struct SinglePlayerSession {
    session: Session,
    corp: Box<dyn Agent>,
    runner: Box<dyn Agent>,
}

impl SinglePlayerSession {
    pub fn new(state: GameState, registry: CardRegistry, corp: Box<dyn Agent>, runner: Box<dyn Agent>) -> Self {
        // Both seats are `External`: an index-based agent cannot be a
        // `Seat::Agent`, which speaks `ClientView` + `PlayerAction`. The
        // pump below is exactly that adapter.
        Self { session: Session::new(state, registry, Seat::External, Seat::External), corp, runner }
    }

    /// Runs the match to completion (or until the step budget is exhausted)
    /// and returns the final `GameState` plus the full action/event history
    /// recorded along the way. Callers check
    /// `matches!(final_state.phase, GamePhase::GameOver(_))` to tell a real
    /// conclusion from budget exhaustion.
    ///
    /// See `netrunner_session::Session::step` for the turn-numbering
    /// convention the history follows.
    pub fn run(mut self) -> (GameState, MatchHistory) {
        // Ends on `Ended`, or on a stall the session already characterized.
        while let SessionStep::Awaiting { side, .. } = self.session.step() {
            let state = self.session.state();
            let registry = self.session.registry();
            let mask = get_action_mask(state, registry);

            let agent: &mut Box<dyn Agent> = match side {
                Side::Corp => &mut self.corp,
                Side::Runner => &mut self.runner,
            };
            let index = agent.select_action(state, registry, &mask);
            assert!(mask[index], "Agent for {side:?} selected illegal index {index}");

            let action = ActionSpace::action_at(state, index)
                .unwrap_or_else(|| panic!("index {index} does not decode to any action for the current state"));

            self.session.submit(action).unwrap_or_else(|error| {
                panic!("Agent for {side:?} chose a mask-legal index {index} that the engine rejected: {error:?}")
            });
        }
        self.session.into_parts()
    }
}
