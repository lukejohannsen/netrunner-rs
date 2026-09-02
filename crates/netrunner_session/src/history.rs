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

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{
    mask_action_for_player, mask_event_for_player, Deck, DeckOrder, GameEvent, GameState, MatchRules, PlayerAction,
    PublicAction, RulesError, Side, Viewer,
};

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
    pub fn for_viewer(&self, state: &GameState, viewer: impl Into<Viewer>) -> PublicHistoryEntry {
        let viewer = viewer.into();
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
///
/// `serde(transparent)`: on the wire it *is* its entry list, so a file of
/// one entry per line (`write_jsonl`) and a single JSON array are the same
/// data, and neither carries a wrapper key that would have to be kept in
/// step with this struct's name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MatchHistory {
    entries: Vec<HistoryEntry>,
}

/// What a recorded history needs besides its actions to be replayed: the
/// seed, the two decks, and the match rules — the exact inputs of
/// `GameState::setup_with`, so `setup` here reproduces the opening
/// position and replaying every recorded action from it reproduces the
/// final one bit for bit (the invariant `Session` pins). Decks are recorded
/// whole rather than by id: a saved custom deck may not exist by that name
/// tomorrow, and the record must not depend on it. There is no footer — the
/// outcome is derivable by replay, and a record cut short by a crash is
/// still a valid prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRecordHeader {
    pub seed: u64,
    pub corp_deck: Deck,
    pub runner_deck: Deck,
    /// `serde(default)` for the same reason `GameState::rules` has it: a
    /// header written without the field is a Standard match.
    #[serde(default)]
    pub rules: MatchRules,
}

impl MatchRecordHeader {
    /// The opening position this record was played from.
    pub fn setup(&self, registry: &CardRegistry) -> Result<(GameState, Vec<GameEvent>), RulesError> {
        GameState::setup_with(&self.corp_deck, &self.runner_deck, registry, self.seed, self.rules, DeckOrder::Shuffled)
    }
}

/// Why a JSON-Lines record could not be read back.
#[derive(Debug, thiserror::Error)]
pub enum HistoryReadError {
    #[error("reading the record: {0}")]
    Io(#[from] io::Error),
    /// One-based, so it matches what an editor shows.
    #[error("line {line} of the record is not valid JSON: {source}")]
    Json {
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("the record is empty — a match record starts with a header line")]
    MissingHeader,
}

impl MatchHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds a history from entries read back off disk — the only way
    /// to construct one outside a `Session`, which is deliberate: a live
    /// history is only ever appended to by `Session::apply`.
    pub fn from_entries(entries: Vec<HistoryEntry>) -> Self {
        Self { entries }
    }

    /// Writes the record as JSON-Lines: the header on the first line, then
    /// one `HistoryEntry` per line. One object per line rather than one
    /// document per file so a record can be appended to while a match is
    /// still running, tailed, and read back without holding the whole
    /// match in memory first.
    pub fn write_jsonl(&self, header: &MatchRecordHeader, mut writer: impl Write) -> io::Result<()> {
        serde_json::to_writer(&mut writer, header)?;
        writeln!(writer)?;
        for entry in &self.entries {
            serde_json::to_writer(&mut writer, entry)?;
            writeln!(writer)?;
        }
        Ok(())
    }

    /// Reads back what `write_jsonl` wrote. Blank lines are skipped, so a
    /// hand-edited record still loads.
    pub fn read_jsonl(reader: impl BufRead) -> Result<(MatchRecordHeader, MatchHistory), HistoryReadError> {
        let mut lines = reader.lines();
        let header_line = lines.next().ok_or(HistoryReadError::MissingHeader)??;
        let header = serde_json::from_str(&header_line).map_err(|source| HistoryReadError::Json { line: 1, source })?;
        let mut entries = Vec::new();
        for (index, line) in lines.enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            entries.push(serde_json::from_str(&line).map_err(|source| HistoryReadError::Json { line: index + 2, source })?);
        }
        Ok((header, MatchHistory { entries }))
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
