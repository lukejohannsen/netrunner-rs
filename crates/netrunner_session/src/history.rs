//! Records every resolved action of a `Session` match — a `(turn_number,
//! side, action, events)` entry per action — for replay, a UI action log,
//! or debugging. `netrunner_core::rules::apply_action` already returns each
//! action's own `Vec<GameEvent>`; this module's only job is accumulating
//! them across an entire match.
//!
//! This used to live in `netrunner_single_player`, which is why nothing
//! else in the workspace had a match log: `netrunner_server::MatchSession`
//! discarded each action's events after a one-shot use classifying how the
//! game ended, and couldn't reach this type anyway. Recording now happens
//! inside the one shared `Session`, so every path gets it for free.

use serde::{Deserialize, Serialize};

use netrunner_core::rules::{mask_action_for_player, mask_event_for_player, GameEvent, GameState, PlayerAction, PublicAction, Side};

/// One resolved action: which turn it happened during (see
/// `crate::session::Session::step`'s doc comment for the turn-numbering
/// convention), which side submitted it, the action itself, and every
/// `GameEvent` `apply_action` produced applying it.
///
/// This is the *full* record — the acting side's action and the raw events
/// — and it never leaves the process: what a seat receives is
/// `PublicHistoryEntry`, built by `for_viewer`. `Serialize`/`Deserialize`
/// so a history can be written out and replayed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub turn_number: u32,
    pub side: Side,
    pub action: PlayerAction,
    pub events: Vec<GameEvent>,
}

impl HistoryEntry {
    /// This entry as `viewer` is entitled to see it, masked by
    /// `netrunner_core::rules::masking` against `state` — which must be the
    /// position *this* action produced, because concealment is read from
    /// where the named cards now sit. `Session::last_entry_for` is the
    /// caller that has that state to hand; there is deliberately no batch
    /// form over older entries, since masking one against a later state is
    /// exactly the mistake that would reveal a card trashed since.
    pub fn for_viewer(&self, state: &GameState, viewer: Side) -> PublicHistoryEntry {
        PublicHistoryEntry {
            turn_number: self.turn_number,
            side: self.side,
            action: mask_action_for_player(&self.action, self.side, viewer),
            events: self.events.iter().filter_map(|event| mask_event_for_player(event, state, viewer)).collect(),
        }
    }
}

/// A `HistoryEntry` as one seat sees it: the opponent's card-naming actions
/// reduced to their public shape and every event the viewer may not see
/// dropped. This is what crosses the process boundary —
/// `netrunner_server` sends each seat its own copy after every action, and
/// the TUI's match log renders the same type on both the local and the
/// remote path so the two cannot disagree about what a player may read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicHistoryEntry {
    pub turn_number: u32,
    pub side: Side,
    pub action: PublicAction,
    pub events: Vec<GameEvent>,
}

/// The full ordered action/event log of one match, in resolution order.
#[derive(Debug, Clone, Default)]
pub struct MatchHistory {
    entries: Vec<HistoryEntry>,
}

impl MatchHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn record(&mut self, turn_number: u32, side: Side, action: PlayerAction, events: Vec<GameEvent>) {
        self.entries.push(HistoryEntry { turn_number, side, action, events });
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
