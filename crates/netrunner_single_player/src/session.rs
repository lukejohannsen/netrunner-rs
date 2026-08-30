//! A synchronous, single-process match runner: drives a full game from an
//! already-`GameState::setup` state to `GameOver`, resolving each pending
//! decision to whichever side actually has one (`current_actor`) and
//! getting that side's chosen `ActionSpace` index from a `PlayerDriver` —
//! either a `netrunner_bots::Agent` (blanket-implemented as a
//! `PlayerDriver` below) or a `HumanPromptDriver` wrapping a caller-supplied
//! synchronous input callback. Mirrors the decision-loop shape of
//! `netrunner_server::match_session::MatchSession` (`current_actor`/
//! `apply_action`/`GamePhase::GameOver`), but synchronously and without any
//! channel/async runtime — this crate depends on nothing but
//! `netrunner_core` and `netrunner_bots`.

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{apply_action, current_actor, get_action_mask, ActionSpace, GameEvent, GamePhase, GameState, Side};

use crate::history::{HistoryEntry, MatchHistory};

/// Guard against a stalled/looping game running forever — same budget as
/// `netrunner_server::match_session::MatchSession`'s own `MAX_STEPS`.
pub const MAX_STEPS: u32 = 10_000;

/// `SinglePlayerSession::with_observer`'s callback type.
type ActionObserver = Box<dyn FnMut(&HistoryEntry)>;

/// Picks one legal `ActionSpace` index (`0..ActionSpace::SIZE`) for the
/// current `GameState`, guaranteed `mask[index]` is `true`.
///
/// Blanket-implemented for every `netrunner_bots::Agent` below, so
/// `IndexedRandomAgent`/`IndexedHeuristicAgent`/`IndexedOnnxAgent` are
/// already valid drivers with no adapter code needed. A synchronous human
/// input handler implements this directly, or wraps a prompt callback in
/// `HumanPromptDriver`.
pub trait PlayerDriver {
    fn select_action(&mut self, state: &GameState, registry: &CardRegistry, mask: &[bool]) -> usize;
}

impl<A: netrunner_bots::Agent> PlayerDriver for A {
    fn select_action(&mut self, state: &GameState, registry: &CardRegistry, mask: &[bool]) -> usize {
        netrunner_bots::Agent::select_action(self, state, registry, mask)
    }
}

/// Wraps a synchronous human-input callback (e.g. "print the legal actions
/// `mask`/`ActionSpace::action_at` decode to, read a choice, return its
/// index") as a `PlayerDriver`. This crate performs no I/O itself — the
/// callback is the embedding application's responsibility, so no
/// stdin/terminal/UI dependency is pulled in here.
pub struct HumanPromptDriver<F>
where
    F: FnMut(&GameState, &CardRegistry, &[bool]) -> usize,
{
    prompt: F,
}

impl<F> HumanPromptDriver<F>
where
    F: FnMut(&GameState, &CardRegistry, &[bool]) -> usize,
{
    pub fn new(prompt: F) -> Self {
        Self { prompt }
    }
}

impl<F> PlayerDriver for HumanPromptDriver<F>
where
    F: FnMut(&GameState, &CardRegistry, &[bool]) -> usize,
{
    fn select_action(&mut self, state: &GameState, registry: &CardRegistry, mask: &[bool]) -> usize {
        (self.prompt)(state, registry, mask)
    }
}

/// A local, synchronous single-match host: owns the real `GameState` and
/// runs it to completion against two `PlayerDriver`s, recording every
/// resolved action into a `MatchHistory`.
pub struct SinglePlayerSession {
    state: GameState,
    registry: CardRegistry,
    corp: Box<dyn PlayerDriver>,
    runner: Box<dyn PlayerDriver>,
    history: MatchHistory,
    turn_number: u32,
    on_action: Option<ActionObserver>,
}

impl SinglePlayerSession {
    pub fn new(state: GameState, registry: CardRegistry, corp: Box<dyn PlayerDriver>, runner: Box<dyn PlayerDriver>) -> Self {
        Self { state, registry, corp, runner, history: MatchHistory::new(), turn_number: 0, on_action: None }
    }

    /// Registers a callback invoked once per resolved action (either side),
    /// immediately after it's appended to the match history. The only
    /// observation point external to `run`'s own blocking loop — a caller
    /// that wants to narrate bot moves live (e.g. a UI's action log) has no
    /// other way to see them, since `run` doesn't return control between
    /// actions. Only `&HistoryEntry` is passed, not the resulting
    /// `GameState` — a caller that also needs the post-action state has to
    /// get it from its own `PlayerDriver`'s `state` parameter instead (only
    /// available at that driver's own decision points).
    pub fn with_observer(mut self, observer: impl FnMut(&HistoryEntry) + 'static) -> Self {
        self.on_action = Some(Box::new(observer));
        self
    }

    /// Runs the match to completion (or until `MAX_STEPS` is exhausted) and
    /// returns the final `GameState` plus the full action/event history
    /// recorded along the way. Callers check
    /// `matches!(final_state.phase, GamePhase::GameOver(_))` to tell a real
    /// conclusion from budget exhaustion.
    ///
    /// **Turn-numbering convention** (`GameState` itself has no turn
    /// counter): `turn_number` starts at `0` and covers every Mulligan-phase
    /// action; it increments by 1 immediately *after* an action whose
    /// returned events include `GameEvent::TurnStarted` (emitted by
    /// `rules::turn::enter_start_of_turn`, which fires both on the Runner's
    /// mulligan resolution entering Corp's first turn and on every
    /// subsequent `end_turn` handoff) — so the action that causes a turn
    /// transition is itself still logged under the turn that was ending,
    /// and turn `1` is Corp's opening turn.
    pub fn run(mut self) -> (GameState, MatchHistory) {
        for _ in 0..MAX_STEPS {
            if matches!(self.state.phase, GamePhase::GameOver(_)) {
                break;
            }
            let Some(side) = current_actor(&self.state) else { break };

            let mask = get_action_mask(&self.state, &self.registry);

            let index = {
                let SinglePlayerSession { state, registry, corp, runner, .. } = &mut self;
                let driver: &mut Box<dyn PlayerDriver> = match side {
                    Side::Corp => corp,
                    Side::Runner => runner,
                };
                driver.select_action(state, registry, &mask)
            };
            assert!(mask[index], "PlayerDriver for {side:?} selected illegal index {index}");

            let action = ActionSpace::action_at(&self.state, index)
                .unwrap_or_else(|| panic!("index {index} does not decode to any action for the current state"));

            let (next, events) = apply_action(&self.state, &self.registry, action.clone()).unwrap_or_else(|error| {
                panic!("PlayerDriver for {side:?} chose a mask-legal index {index} that apply_action rejected: {error:?}")
            });

            let entered_new_turn = events.iter().any(|event| matches!(event, GameEvent::TurnStarted { .. }));
            self.history.record(self.turn_number, side, action, events);
            if let Some(observer) = self.on_action.as_mut() {
                observer(self.history.entries().last().expect("just recorded"));
            }
            if entered_new_turn {
                self.turn_number += 1;
            }

            self.state = next;
        }
        (self.state, self.history)
    }
}
