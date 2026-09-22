//! `netrunner_cli replay <record.jsonl>`: step-by-step post-match review of
//! a history written by `--headless --record`, or of a client's bug report
//! (ROADMAP Phase 2 §2, Phase 7 §4ag).
//!
//! The replay itself — every position cached up front, each chair's masked
//! copy of the log — is `netrunner_client::replay`, which the desktop's
//! replay screen opens too. What is the terminal's is here: the side panel
//! that counts positions and names the keys, and the `RenderableView` the
//! shared renderer draws, whose actions pane shows what the last action
//! *did* rather than what could be done next.

use std::path::Path;

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{PlayerAction, Side, Viewer};
use netrunner_core::view::ClientView;
use netrunner_client::replay::{self as core_replay, describe_event};

pub use netrunner_client::replay::Start;

use crate::app::{Coaching, RenderableView};

/// A recorded match from one chair, with the terminal's side panel.
pub struct Replay {
    inner: core_replay::Replay,
    /// Owned and rebuilt on every move because `RenderableView::coaching`
    /// hands out a borrow.
    coaching: Coaching,
}

impl Replay {
    /// Replays the record from its header, opening at the setup.
    #[cfg(test)]
    pub fn load(
        header: &netrunner_session::MatchRecordHeader,
        history: netrunner_session::MatchHistory,
        registry: CardRegistry,
        side: Side,
        title: &str,
    ) -> Result<Self, core_replay::ReplayError> {
        Ok(Self::wrap(core_replay::Replay::load(header, history, registry, side, title)?))
    }

    /// Reads a record and replays it, opening where
    /// `netrunner_client::replay::opening` says for the flags given.
    pub fn open(path: &Path, registry: CardRegistry, side: Option<Side>, at: Option<Start>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self::wrap(core_replay::Replay::open(path, registry, side, at)?))
    }

    fn wrap(inner: core_replay::Replay) -> Self {
        let mut replay = Self { inner, coaching: Coaching { title: String::new(), step: 0, total: 0, prose: String::new(), hint: None, gated: true, showing_all: false } };
        replay.refresh();
        replay
    }

    pub fn side(&self) -> Side {
        self.inner.side()
    }

    pub fn set_side(&mut self, side: Side) {
        self.inner.set_side(side);
        self.refresh();
    }

    pub fn seek(&mut self, cursor: usize) {
        self.inner.seek(cursor);
        self.refresh();
    }

    pub fn step_forward(&mut self, by: usize) {
        self.inner.step_forward(by);
        self.refresh();
    }

    pub fn step_back(&mut self, by: usize) {
        self.inner.step_back(by);
        self.refresh();
    }

    fn refresh(&mut self) {
        let cursor = self.inner.cursor();
        let prose = match self.inner.entry(cursor) {
            None => "Setup — the opening position, before any action.".to_string(),
            Some(entry) => format!("Turn {}, {:?} acted.\n\n{}", entry.turn_number, entry.side, self.inner.lines_of(cursor).join("\n")),
        };
        self.coaching = Coaching {
            title: format!("Replay — {} ({:?}'s chair)", self.inner.title(), self.inner.side()),
            step: cursor,
            total: self.inner.len(),
            prose,
            hint: Some("→/Space next, ← back, PgUp/PgDn ±10, Home/End, s swap chair, q quit".to_string()),
            gated: true,
            showing_all: false,
        };
    }
}

impl RenderableView for Replay {
    fn registry(&self) -> &CardRegistry {
        self.inner.registry()
    }

    fn viewer(&self) -> Viewer {
        Viewer::Player(self.inner.side())
    }

    fn view(&self) -> Option<&ClientView> {
        Some(self.inner.view())
    }

    fn selected(&self) -> usize {
        0
    }

    /// The actions pane shows what the last action *did*, not what could be
    /// done next: a replay has no decision to offer.
    fn legal_action_labels(&self) -> Vec<String> {
        self.inner.entry(self.inner.cursor()).map(|entry| entry.events.iter().map(describe_event).collect()).unwrap_or_default()
    }

    /// Nothing glows in a replay, for the same reason: the cards on a
    /// recorded board are not the viewer's to act on.
    fn action_map(&self) -> Option<netrunner_client::board::ActionMap> {
        None
    }

    fn actions_title(&self) -> Option<String> {
        Some(match self.inner.cursor() {
            0 => "Events (none yet — → to step)".to_string(),
            n => format!("Events of step {n}, as {:?} may see them", self.inner.side()),
        })
    }

    fn selected_action(&self) -> Option<PlayerAction> {
        None
    }

    fn action_log(&self) -> &[String] {
        self.inner.log()
    }

    fn coaching(&self) -> Option<&Coaching> {
        Some(&self.coaching)
    }
}
