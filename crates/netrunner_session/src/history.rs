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

use netrunner_core::rules::{GameEvent, PlayerAction, Side};

/// One resolved action: which turn it happened during (see
/// `crate::session::Session::step`'s doc comment for the turn-numbering
/// convention), which side submitted it, the action itself, and every
/// `GameEvent` `apply_action` produced applying it.
/// `Serialize`/`Deserialize` because an entry crosses the process boundary:
/// `netrunner_server` broadcasts each one to channel seats so a remote
/// client can render a game log. `PlayerAction` and `GameEvent` already
/// derive both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub turn_number: u32,
    pub side: Side,
    pub action: PlayerAction,
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
