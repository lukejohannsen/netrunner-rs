//! A recorded match, stepped through from either chair (ROADMAP Phase 2 §2,
//! Phase 7 §8 item 5).
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
//!
//! **Lifted out of the terminal client** so the desktop can open the same
//! files (`bug_report`'s, and `--headless --record`'s) on its own board;
//! the terminal keeps only its side panel and the `RenderableView` it
//! draws through. **The log is the live log's** — `actions::push_log_line`
//! against the view the action produced — so a replayed game reads, line
//! for line, as the game did when it was played. It used to be a plainer
//! one-line-per-action wording of its own, which read differently from the
//! log the same person had watched.

use std::path::Path;

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{apply_action, GameEvent, GameState, PlayerAction, RulesError, Side};
use netrunner_core::view::{build_client_view, ClientView};
use netrunner_session::{HistoryEntry, MatchHistory, MatchRecordHeader, PublicHistoryEntry};

use crate::actions::{push_log_line, MAX_LOG_LINES};

/// Why a record could not be replayed.
#[derive(Debug)]
pub enum ReplayError {
    /// The record's header does not set up a game.
    Setup(RulesError),
    /// The engine rejected a recorded action. A record is only replayable
    /// by the engine that wrote it (or one that resolves every recorded
    /// action identically), so this is the signature of a rules change
    /// since the match was played — named by entry so the divergence can
    /// be found.
    Diverged { index: usize, action: PlayerAction, error: RulesError },
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::Setup(error) => write!(f, "the record's header does not set up a game: {error:?}"),
            ReplayError::Diverged { index, action, error } => {
                write!(f, "entry {index} ({action:?}) no longer replays: {error} — the rules have changed since this record was written")
            }
        }
    }
}

impl std::error::Error for ReplayError {}

/// Where a replay opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Beginning,
    End,
    /// After this many actions; past the end is the end.
    At(usize),
}

impl std::str::FromStr for Start {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "start" | "beginning" => Ok(Start::Beginning),
            "end" => Ok(Start::End),
            number => number.parse().map(Start::At).map_err(|_| format!("expected start, end or a number of actions, not {s:?}")),
        }
    }
}

/// The chair and the position a record opens at, when the caller leaves
/// either out.
///
/// **A bug report opens where it was saved, from the person's chair**: it
/// is read from the moment something looked wrong, backwards, and a
/// record that names the bot a person played says which chair was
/// theirs. A record between two bots names no one, so it opens as it
/// always did — the Corp's chair, at the setup.
pub fn opening(header: &MatchRecordHeader, side: Option<Side>, at: Option<Start>) -> (Side, Start) {
    let person = header.bot.map(|bot| bot.side.other());
    let side = side.or(person).unwrap_or(Side::Corp);
    let at = at.unwrap_or(if person.is_some() { Start::End } else { Start::Beginning });
    (side, at)
}

/// What names a record to a person: its file name, and the bot it was
/// played against when it is a client's bug report.
pub fn title(path: &Path, header: &MatchRecordHeader) -> String {
    let mut title = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    if let Some(bot) = header.bot {
        title = format!("{title} — against the {} {:?} {:?}", bot.level, bot.personality, bot.side);
    }
    title
}

/// Reads a record file — a `--record` file or a client's bug report.
pub fn read(path: &Path) -> Result<(MatchRecordHeader, MatchHistory), Box<dyn std::error::Error + Send + Sync>> {
    let file = std::io::BufReader::new(std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?);
    Ok(MatchHistory::read_jsonl(file)?)
}

/// A record's header alone — its first line — for a list that names
/// records without replaying each one. `None` for a file that is not a
/// record.
pub fn header(path: &Path) -> Option<MatchRecordHeader> {
    use std::io::BufRead;
    let file = std::io::BufReader::new(std::fs::File::open(path).ok()?);
    let line = file.lines().map_while(Result::ok).find(|line| !line.trim().is_empty())?;
    serde_json::from_str(&line).ok()
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
    // Per-chair caches, rebuilt by `set_side`. Each is masked against its
    // own entry's post-state, `states[i + 1]`.
    view: ClientView,
    public: Vec<PublicHistoryEntry>,
    /// Every entry's log lines, one after another, and where each entry's
    /// lines end: the log at position `i` is the lines before `log_ends[i]`
    /// (`log_ends[0]` is 0, the setup).
    log_lines: Vec<String>,
    log_ends: Vec<usize>,
}

impl Replay {
    /// Replays the record from its header, failing on the first action the
    /// engine no longer accepts. Opens at the setup.
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
            public: Vec::new(),
            log_lines: Vec::new(),
            log_ends: Vec::new(),
        };
        replay.set_side(side);
        Ok(replay)
    }

    /// Reads a record and replays it, opening where `opening` says for the
    /// chair and position given.
    pub fn open(path: &Path, registry: CardRegistry, side: Option<Side>, at: Option<Start>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (header, history) = read(path)?;
        let (side, start) = opening(&header, side, at);
        let mut replay = Self::load(&header, history, registry, side, &title(path, &header))?;
        replay.seek(match start {
            Start::Beginning => 0,
            Start::End => usize::MAX,
            Start::At(index) => index,
        });
        Ok(replay)
    }

    pub fn registry(&self) -> &CardRegistry {
        &self.registry
    }

    pub fn side(&self) -> Side {
        self.side
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// How many actions have been applied at the current position.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// How many actions the record holds: the last position's cursor.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The board the chair saw at the current position.
    pub fn view(&self) -> &ClientView {
        &self.view
    }

    /// The chair's copy of the entry that produced position `position`;
    /// `None` at the setup and past the end.
    pub fn entry(&self, position: usize) -> Option<&PublicHistoryEntry> {
        position.checked_sub(1).and_then(|index| self.public.get(index))
    }

    /// The log as the chair read it at the current position, capped as the
    /// live log is.
    pub fn log(&self) -> &[String] {
        let end = self.log_ends[self.cursor];
        &self.log_lines[end.saturating_sub(MAX_LOG_LINES)..end]
    }

    /// The log lines the entry that produced position `position` wrote —
    /// its action and the narration under it; empty at the setup.
    pub fn lines_of(&self, position: usize) -> &[String] {
        match position {
            0 => &[],
            n if n < self.log_ends.len() => &self.log_lines[self.log_ends[n - 1]..self.log_ends[n]],
            _ => &[],
        }
    }

    /// Switches chairs. Everything a seat sees is recomputed from the
    /// cached states; the raw entries are never shown.
    pub fn set_side(&mut self, side: Side) {
        self.side = side;
        self.public = self.entries.iter().zip(&self.states[1..]).map(|(entry, state)| entry.for_viewer(state, side)).collect();
        self.log_lines.clear();
        self.log_ends = vec![0];
        for (entry, state) in self.public.iter().zip(&self.states[1..]) {
            let after = build_client_view(state, &self.registry, side);
            // Written one entry at a time into a log of its own, because
            // `push_log_line` caps what it is given, and a cap mid-record
            // would cut the lines this replay indexes by.
            let mut lines = Vec::new();
            push_log_line(&mut lines, entry, &self.registry, Some(&after));
            self.log_lines.extend(lines);
            self.log_ends.push(self.log_lines.len());
        }
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
    }
}

/// One line per masked event. The `Debug` rendering, deliberately: this is
/// a review tool, the events have already been masked for the chair, and a
/// per-variant prose renderer (`describe_action`'s sibling for events) is
/// the narration work ROADMAP Phase 4 §1 leaves open. A long payload
/// (`SubroutineFired`'s `Effect`) is truncated so the list stays one line
/// per event.
pub fn describe_event(event: &GameEvent) -> String {
    let rendered = format!("{event:?}");
    const LIMIT: usize = 110;
    if rendered.chars().count() > LIMIT {
        let cut: String = rendered.chars().take(LIMIT - 1).collect();
        format!("{cut}…")
    } else {
        rendered
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
        let header = MatchRecordHeader { seed, corp_deck: corp.to_deck(), runner_deck: runner.to_deck(), rules: MatchRules::default(), bot: None };
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
        assert_eq!((replay.cursor(), replay.len()), (0, total));
        assert!(replay.log().is_empty());
        assert!(replay.entry(0).is_none());

        replay.step_forward(1);
        assert!(!replay.log().is_empty());
        assert!(!replay.entry(1).expect("the first action").events.is_empty(), "the first action produced at least one event");

        replay.seek(usize::MAX);
        assert_eq!(replay.cursor(), total, "seek clamps to the last position");
        assert!(matches!(replay.view().phase, netrunner_core::rules::GamePhase::GameOver(_)));
        assert!(replay.log().len() <= MAX_LOG_LINES);

        replay.step_back(usize::MAX);
        assert_eq!(replay.cursor(), 0);
    }

    /// The log at every position is the live log as the chair was sent it:
    /// each masked entry through `push_log_line` against the view it
    /// produced, capped the same way.
    #[test]
    fn the_log_at_each_position_is_the_live_log_to_that_point() {
        let (header, history, registry) = recorded_game(5);
        let (setup, _) = header.setup(&registry).unwrap();
        let mut replay = Replay::load(&header, history.clone(), registry.clone(), Side::Corp, "test").unwrap();
        let mut state = setup;
        let mut live = Vec::new();
        for (index, entry) in history.entries().iter().enumerate() {
            state = apply_action(&state, &registry, entry.action.clone()).unwrap().0;
            let view = build_client_view(&state, &registry, Side::Corp);
            push_log_line(&mut live, &entry.for_viewer(&state, Side::Corp), &registry, Some(&view));
            replay.seek(index + 1);
            assert_eq!(replay.log(), live.as_slice(), "position {}", index + 1);
            assert_eq!(replay.view(), &view);
        }
    }

    /// The Runner's chair never reads the Corp's facedown installs by
    /// title: the log lines are the masked entries, not the raw record.
    #[test]
    fn the_runners_chair_shows_the_corps_installs_as_concealed() {
        let (header, history, registry) = recorded_game(3);
        let corp_installs = history
            .entries()
            .iter()
            .filter(|entry| entry.side == Side::Corp && matches!(entry.action, PlayerAction::InstallCard { .. }))
            .count();
        assert!(corp_installs > 0, "a random Corp installs something in a whole game");

        let mut replay = Replay::load(&header, history, registry, Side::Runner, "test").unwrap();
        let everything = |replay: &Replay| replay.log_lines.join("\n");
        replay.seek(usize::MAX);
        let runner_log = everything(&replay);
        assert!(runner_log.contains("Install a card into"), "{runner_log}");
        replay.set_side(Side::Corp);
        let corp_log = everything(&replay);
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

    /// A bug report opens at its end from the person's chair; a record
    /// between two bots opens where it always did; a flag wins either way.
    #[test]
    fn a_bug_report_opens_at_its_end_from_the_persons_chair() {
        use netrunner_bots::{Level, Personality};
        let (plain, _, _) = recorded_game(4);
        assert_eq!(opening(&plain, None, None), (Side::Corp, Start::Beginning));
        let report = MatchRecordHeader {
            bot: Some(netrunner_session::RecordedBot { side: Side::Corp, level: Level::Elite, personality: Personality::Glacier }),
            ..plain
        };
        assert_eq!(opening(&report, None, None), (Side::Runner, Start::End));
        assert_eq!(opening(&report, Some(Side::Corp), Some(Start::At(7))), (Side::Corp, Start::At(7)));
        assert_eq!("end".parse::<Start>(), Ok(Start::End));
        assert_eq!(" Start ".parse::<Start>(), Ok(Start::Beginning));
        assert_eq!("12".parse::<Start>(), Ok(Start::At(12)));
        assert!("later".parse::<Start>().is_err());
    }
}
