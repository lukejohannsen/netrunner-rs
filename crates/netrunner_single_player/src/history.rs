//! Records every resolved action of a `SinglePlayerSession` match — a
//! `(turn_number, side, action, events)` entry per action — for replay or
//! debugging. `netrunner_core::rules::apply_action` already returns each
//! action's own `Vec<GameEvent>`; this module's only job is accumulating
//! them across an entire match, which nothing else in the workspace does —
//! `netrunner_server::MatchSession` discards each action's events after a
//! one-shot use classifying how the game ended.

use netrunner_core::rules::{GameEvent, PlayerAction, Side};

/// One resolved action: which turn it happened during (see
/// `crate::session::SinglePlayerSession::run`'s doc comment for the
/// turn-numbering convention — `GameState` itself has no turn counter),
/// which side submitted it, the action itself, and every `GameEvent`
/// `apply_action` produced applying it.
#[derive(Debug, Clone, PartialEq, Eq)]
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
