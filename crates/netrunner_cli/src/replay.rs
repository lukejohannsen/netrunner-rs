//! `netrunner_cli replay <record.jsonl>`: step-by-step post-match review of
//! a history written by `--headless --record` (ROADMAP Phase 2 §2).
//!
//! A replay is not a second engine path. `apply_action` is a pure function
//! of state and action, so every position of the match is recomputed from
//! the record's header and re-derived actions — the same replay the
//! `Session` tests pin as bit-identical — and cached up front; stepping is
//! then indexing. Nothing here re-derives legality or renders a
//! `GameState`: the viewer picks a *chair*, and every frame is the
//! `ClientView` that seat would have received, with that seat's masked
//! copy of the log (`HistoryEntry::for_viewer`, against each entry's own
//! post-action state — the contract `Session::last_entry_for` documents).
//! A record holds the raw history, so this is where per-viewer masking
//! happens for a replay, exactly where it happens for a live match.

use std::path::Path;

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{apply_action, GameEvent, GameState, PlayerAction, RulesError, Side, Viewer};
use netrunner_core::view::{build_client_view, ClientView};
use netrunner_session::{HistoryEntry, MatchHistory, MatchRecordHeader};

use crate::app::{describe_public_action, Coaching, RenderableView, MAX_LOG_LINES};

/// Why a record could not be replayed.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("the record's header does not set up a game: {0:?}")]
    Setup(RulesError),
    /// The engine rejected a recorded action. A record is only replayable
    /// by the engine that wrote it (or one that resolves every recorded
    /// action identically), so this is the signature of a rules change
    /// since the match was played — named by entry so the divergence can
    /// be found.
    #[error("entry {index} ({action:?}) no longer replays: {error:?} — the rules have changed since this record was written")]
    Diverged { index: usize, action: PlayerAction, error: RulesError },
}

/// Every position of one recorded match, from the chair of `side`.
pub struct Replay {
    registry: CardRegistry,
    side: Side,
    /// `states[i]` is the position after `i` actions; `states[0]` is the
    /// setup. One more than the entry count.
    states: Vec<GameState>,
    entries: Vec<HistoryEntry>,
    /// Where the viewer stands: `0..=entries.len()`.
    cursor: usize,
    title: String,
    // Per-chair caches, rebuilt by `set_side`. Each log line and event list
    // is masked against its own entry's post-state, `states[i + 1]`.
    view: ClientView,
    log_lines: Vec<String>,
    event_lines: Vec<Vec<String>>,
    /// The side panel for the current position. Owned and rebuilt on every
    /// move because `RenderableView::coaching` hands out a borrow.
    coaching: Coaching,
}

impl Replay {
    /// Replays the record from its header, failing on the first action the
    /// engine no longer accepts.
    pub fn load(header: &MatchRecordHeader, history: MatchHistory, registry: CardRegistry, side: Side, title: &str) -> Result<Self, ReplayError> {
        let (setup, _events) = header.setup(&registry).map_err(ReplayError::Setup)?;
        let entries: Vec<HistoryEntry> = history.entries().to_vec();
        let mut states = Vec::with_capacity(entries.len() + 1);
        states.push(setup);
        for (index, entry) in entries.iter().enumerate() {
            let previous = states.last().expect("states starts with the setup");
            let (next, _events) = apply_action(previous, &registry, entry.action.clone())
                .map_err(|error| ReplayError::Diverged { index, action: entry.action.clone(), error })?;
            states.push(next);
        }
        let view = build_client_view(&states[0], &registry, side);
        let mut replay = Self {
            registry,
            side,
            states,
            entries,
            cursor: 0,
            title: title.to_string(),
            view,
            log_lines: Vec::new(),
            event_lines: Vec::new(),
            coaching: Coaching { title: String::new(), step: 0, total: 0, prose: String::new(), hint: None, gated: true, showing_all: false },
        };
        replay.set_side(side);
        Ok(replay)
    }

    /// Reads a `--record` file and replays it.
    pub fn open(path: &Path, registry: CardRegistry, side: Side) -> Result<Self, Box<dyn std::error::Error>> {
        let file = std::io::BufReader::new(std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?);
        let (header, history) = MatchHistory::read_jsonl(file)?;
        let title = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
        Ok(Self::load(&header, history, registry, side, &title)?)
    }

    pub fn side(&self) -> Side {
        self.side
    }

    /// Switches chairs. Everything a seat sees is recomputed from the
    /// cached states; the raw entries are never shown.
    pub fn set_side(&mut self, side: Side) {
        self.side = side;
        self.log_lines = self
            .entries
            .iter()
            .zip(&self.states[1..])
            .map(|(entry, state)| {
                let public = entry.for_viewer(state, side);
                format!("[turn {}] {:?}: {}", public.turn_number, public.side, describe_public_action(&public.action, &self.registry, None))
            })
            .collect();
        self.event_lines = self
            .entries
            .iter()
            .zip(&self.states[1..])
            .map(|(entry, state)| entry.for_viewer(state, side).events.iter().map(describe_event).collect())
            .collect();
        self.refresh_view();
    }

    pub fn seek(&mut self, cursor: usize) {
        self.cursor = cursor.min(self.entries.len());
        self.refresh_view();
    }

    pub fn step_forward(&mut self, by: usize) {
        self.seek(self.cursor.saturating_add(by));
    }

    pub fn step_back(&mut self, by: usize) {
        self.seek(self.cursor.saturating_sub(by));
    }

    fn refresh_view(&mut self) {
        self.view = build_client_view(&self.states[self.cursor], &self.registry, self.side);
        self.coaching = self.build_coaching();
    }

    /// The masked events of the action that produced the current position;
    /// empty at the setup.
    fn current_events(&self) -> &[String] {
        match self.cursor {
            0 => &[],
            n => &self.event_lines[n - 1],
        }
    }

    fn build_coaching(&self) -> Coaching {
        let prose = match self.cursor {
            0 => "Setup — the opening position, before any action.".to_string(),
            n => {
                let entry = &self.entries[n - 1];
                format!("Turn {}, {:?} acted.\n\n{}", entry.turn_number, entry.side, self.log_lines[n - 1])
            }
        };
        Coaching {
            title: format!("Replay — {} ({:?}'s chair)", self.title, self.side),
            step: self.cursor,
            total: self.entries.len(),
            prose,
            hint: Some("→/Space next, ← back, PgUp/PgDn ±10, Home/End, s swap chair, q quit".to_string()),
            gated: true,
            showing_all: false,
        }
    }
}

/// One line per masked event. The `Debug` rendering, deliberately: this is
/// a review tool, the events have already been masked for the chair, and a
/// per-variant prose renderer (`describe_action`'s sibling for events) is
/// the narration work ROADMAP Phase 4 §1 leaves open. A long payload
/// (`SubroutineFired`'s `Effect`) is truncated so the list stays one line
/// per event.
fn describe_event(event: &GameEvent) -> String {
    let rendered = format!("{event:?}");
    const LIMIT: usize = 110;
    if rendered.chars().count() > LIMIT {
        let cut: String = rendered.chars().take(LIMIT - 1).collect();
        format!("{cut}…")
    } else {
        rendered
    }
}

impl RenderableView for Replay {
    fn registry(&self) -> &CardRegistry {
        &self.registry
    }

    fn viewer(&self) -> Viewer {
        Viewer::Player(self.side)
    }

    fn view(&self) -> Option<&ClientView> {
        Some(&self.view)
    }

    fn selected(&self) -> usize {
        0
    }

    /// The actions pane shows what the last action *did*, not what could be
    /// done next: a replay has no decision to offer.
    fn legal_action_labels(&self) -> Vec<String> {
        self.current_events().to_vec()
    }

    fn actions_title(&self) -> Option<String> {
        Some(match self.cursor {
            0 => "Events (none yet — → to step)".to_string(),
            n => format!("Events of step {n}, as {:?} may see them", self.side),
        })
    }

    fn selected_action(&self) -> Option<PlayerAction> {
        None
    }

    fn action_log(&self) -> &[String] {
        let end = self.cursor;
        let start = end.saturating_sub(MAX_LOG_LINES);
        &self.log_lines[start..end]
    }

    fn coaching(&self) -> Option<&Coaching> {
        Some(&self.coaching)
    }
}

#[cfg(test)]
mod tests {
    use netrunner_bots::RandomAgent;
    use netrunner_core::decks;
    use netrunner_core::rules::MatchRules;
    use netrunner_session::{Seat, Session, SessionStep};

    use super::*;

    fn recorded_game(seed: u64) -> (MatchRecordHeader, MatchHistory, CardRegistry) {
        let registry = crate::decks::sample_deck_registry();
        let (corp, runner) = decks::matchups().into_iter().next().expect("a sample matchup");
        let header = MatchRecordHeader { seed, corp_deck: corp.to_deck(), runner_deck: runner.to_deck(), rules: MatchRules::default() };
        let (state, _events) = header.setup(&registry).unwrap();
        let mut session = Session::new(
            state,
            registry.clone(),
            Seat::Agent(Box::new(RandomAgent::new(seed))),
            Seat::Agent(Box::new(RandomAgent::new(seed.wrapping_add(1)))),
        );
        assert!(matches!(session.run(), SessionStep::Ended { .. }));
        let (_state, history) = session.into_parts();
        (header, history, registry)
    }

    /// Every position is the one the seat would have seen live, and the
    /// last is the recorded game's end.
    #[test]
    fn a_replay_walks_every_position_from_setup_to_game_over() {
        let (header, history, registry) = recorded_game(3);
        let total = history.len();
        let mut replay = Replay::load(&header, history, registry, Side::Runner, "test").unwrap();
        let position = |replay: &Replay| replay.coaching().map(|c| (c.step, c.total)).unwrap();
        assert_eq!(position(&replay), (0, total));
        assert!(replay.action_log().is_empty());
        assert!(replay.legal_action_labels().is_empty());

        replay.step_forward(1);
        assert_eq!(replay.action_log().len(), 1);
        assert!(!replay.legal_action_labels().is_empty(), "the first action produced at least one event");

        replay.seek(usize::MAX);
        assert_eq!(position(&replay), (total, total), "seek clamps to the last position");
        assert!(matches!(replay.view().unwrap().phase, netrunner_core::rules::GamePhase::GameOver(_)));
        assert_eq!(replay.action_log().len(), total.min(MAX_LOG_LINES));

        replay.step_back(usize::MAX);
        assert_eq!(position(&replay), (0, total));
    }

    /// The Runner's chair never reads the Corp's facedown installs by
    /// title: the log lines are the masked entries, not the raw record.
    #[test]
    fn the_runners_chair_shows_the_corps_installs_as_concealed() {
        let (header, history, registry) = recorded_game(3);
        let corp_installs: Vec<String> = history
            .entries()
            .iter()
            .filter(|entry| entry.side == Side::Corp && matches!(entry.action, PlayerAction::InstallCard { .. }))
            .map(|entry| format!("{:?}", entry.action))
            .collect();
        assert!(!corp_installs.is_empty(), "a random Corp installs something in a whole game");

        let mut replay = Replay::load(&header, history, registry, Side::Runner, "test").unwrap();
        replay.seek(usize::MAX);
        let runner_log = replay.action_log().join("\n");
        assert!(runner_log.contains("Install a card into"), "{runner_log}");
        replay.set_side(Side::Corp);
        let corp_log = replay.action_log().join("\n");
        assert!(!corp_log.contains("Install a card into"), "the Corp sees its own installs by name: {corp_log}");
    }

    /// A record from a different engine names the entry that no longer
    /// replays rather than showing a wrong board.
    #[test]
    fn a_record_the_engine_no_longer_accepts_fails_by_entry() {
        let (header, history, registry) = recorded_game(3);
        let mut entries = history.entries().to_vec();
        entries[0].action = PlayerAction::EndTurn;
        let error = Replay::load(&header, MatchHistory::from_entries(entries), registry, Side::Corp, "test").err().expect("EndTurn during the mulligan is illegal");
        assert!(matches!(error, ReplayError::Diverged { index: 0, action: PlayerAction::EndTurn, .. }), "{error}");
    }
}
