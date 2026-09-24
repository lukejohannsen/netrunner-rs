//! The one match-driving interface every game screen consumes.
//!
//! **A local match runs on its own thread.** `Session::step` blocks while
//! a `Seat::Agent` searches — seconds at a high rung — and a resource
//! pumped from a render loop would freeze the frame for as long as the bot
//! thinks. So the session lives on a thread of its own, the client holds a
//! [`MatchHandle`], and the two speak over channels: the thread sends a
//! [`MatchMessage`] after every applied action and whenever the human must
//! act, and the client sends a [`PlayerAction`] back. A Bevy system only
//! ever `try_recv`s; a terminal could block. It also makes local, lesson
//! and remote look identical to the screen that draws them: a later phase
//! feeds the same messages off a socket, and the board cannot tell.
//!
//! **`submit` never filters by `legal_actions`** (AGENTS.md, the Session
//! Rule). `Session::submit` is the only authority, and a rejection comes
//! back as [`MatchMessage::Rejected`] with the engine's own words, the
//! same side still awaiting — exactly what the terminal shows in its
//! action list's title.
//!
//! **What runs on the thread is the loop the terminal runs** in
//! `netrunner_cli::tui::drive_local`: pump `step` one action at a time
//! (never `run`, which swallows the bot's `Applied` steps — each log entry
//! is the human's *masked* copy of that action, and `last_entry_for`
//! reads concealment off the state the action left, so it has to be taken
//! before the next one resolves); on `Awaiting` for the human, wait for
//! their action; on `Ended`, record the game; on a quit, record a loss
//! from `record::FORFEIT_FROM_TURN` on and nothing before it. Recorded the
//! same way, quitting the same way, so a game played here and a game
//! played in the terminal land in the same record by the same rule.
//!
//! **A local game is casual, so a take-back costs nothing.** The session
//! still says which kind each one is (`Rewind::Free`, `Rewind::Undo`) and
//! the messages still carry it, because that line is the one a rated game
//! between two people will be held to and the same messages will come off
//! a socket. Nothing here branches on it: nobody rates a game against a
//! bot (`crate::record`), and an undone game is recorded like any other.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use netrunner_bots::{Level, Personality};
use netrunner_core::cards::CardRegistry;
use netrunner_core::decks::DeckFile;
use netrunner_core::rules::{GameState, MatchRules, PlayerAction, Side};
use netrunner_core::view::ClientView;
use netrunner_session::{
    HistoryEntry, MatchHistory, MatchRecordHeader, PublicHistoryEntry, RecordedBot, Seat, Session, SessionStep, StallReason, SubmitError,
    UNDO_DEPTH,
};

pub use netrunner_session::Rewind;

/// Re-exported so a client that only ever holds a `MatchHandle` need not
/// name the session crate for the one type its `Ended` carries.
pub use netrunner_session::GameEndReason;

use crate::record::{self, BotKind, RecordReport, SeatRecord, SeatRecordSpec};

/// Everything a local game against a rung needs: the two decks, which
/// chair is the person's, the rung and style of the other, the seed, and
/// where the result is recorded.
#[derive(Debug, Clone)]
pub struct LocalMatchSpec {
    pub registry: Arc<CardRegistry>,
    pub corp: DeckFile,
    pub runner: DeckFile,
    pub human: Side,
    pub level: Level,
    /// `None` is the deck's own style (`DeckFile::style`), the same as an
    /// unset `--corp-personality`.
    pub style: Option<Personality>,
    pub seed: u64,
    /// `None` records nothing: a session with no data directory to keep
    /// a record in. Never a choice on a form — a game against a bot is
    /// always casual, so there is no unrecorded kind to ask for.
    pub record: Option<RecordFile>,
}

/// Where a game is recorded and under what name.
#[derive(Debug, Clone)]
pub struct RecordFile {
    pub path: PathBuf,
    pub player: String,
}

/// What the match thread tells the client. Every variant that carries a
/// view carries the *human's* masked view of the state it describes, so
/// a screen never holds anything it may not show.
#[derive(Debug)]
pub enum MatchMessage {
    /// An action was applied — either seat's — and this is the human's
    /// copy of the log entry and the board it left. One per action, in
    /// order, so the match log and the view-diff both see every step.
    Applied { entry: PublicHistoryEntry, view: Box<ClientView> },
    /// What taking the last move back would cost, as of the `Awaiting`
    /// that follows: `None` when there is nothing to take back, which is
    /// also how a match starts. Sent only when it changes, so a game in
    /// which nobody ever goes back hears it a few times a turn and a
    /// client that ignores it misses nothing. Its own message rather than a field on `Awaiting`
    /// because it is the match thread's to know and not the view's: a
    /// `ClientView` is what the engine shows a seat, and a take-back is
    /// the driver's (`netrunner_session::Session::rewind`).
    Back { rewind: Option<Rewind> },
    /// The human must choose from `view.legal_actions`.
    Awaiting { view: Box<ClientView> },
    /// The person's last move was taken back: `view` is the board it was
    /// made from and `removed` is how many `Applied` entries no longer
    /// happened, newest first — the log drops them and the board snaps
    /// back without a transition, since nothing moved *to* here.
    Rewound { view: Box<ClientView>, removed: usize, kind: Rewind },
    /// The engine refused the last `submit`; the human is still awaiting
    /// on the same view. `reason` is `RulesError`'s own message.
    Rejected { reason: String },
    /// The match ended. `report` is where the game leaves the player's
    /// record, `None` when none is kept; `notice` is a record file that
    /// would not save, which must not hide the result.
    Ended { winner: Side, reason: GameEndReason, view: Box<ClientView>, report: Option<RecordReport>, notice: Option<String> },
    /// The session stopped without a `GameOver`: a stall, or a bot seat
    /// the session could not resolve. Nothing more will arrive.
    Stalled { reason: String },
}

enum Command {
    Submit(PlayerAction),
    Rewind,
    Quit,
}

/// A running match, as the client holds it. Dropping it quits the game
/// the way Escape does in the terminal — a loss from turn 3 on.
///
/// The receiver sits behind a mutex only because a `Receiver` is not
/// `Sync` and a Bevy resource has to be; one uncontended lock per frame
/// is the whole cost, and it keeps `std::sync::mpsc` in place of a
/// crate for one channel.
pub struct MatchHandle {
    commands: Sender<Command>,
    messages: Mutex<Receiver<MatchMessage>>,
    human: Side,
    registry: Arc<CardRegistry>,
    finished: bool,
    thread: Option<JoinHandle<()>>,
    header: MatchRecordHeader,
    /// The match thread's history, mirrored after every applied action and
    /// every take-back, so `record` can be read on a frame without asking
    /// the thread anything. **A mirror, not a request**: the thread may be
    /// inside a search for seconds, and a bug report pressed while the bot
    /// thinks must not freeze the window until it answers.
    history: Arc<Mutex<Vec<HistoryEntry>>>,
}

impl MatchHandle {
    /// Sets the game up and starts its thread. Everything that can fail
    /// fails *here* — an illegal deck, a record file that will not load —
    /// so a screen can show the reason as a notice rather than a game
    /// dying on its first frame (Phase 6's rule: a game that fails to
    /// start is a notice, not a drop to the shell).
    pub fn start_local(spec: LocalMatchSpec) -> Result<Self, String> {
        let LocalMatchSpec { registry, corp, runner, human, level, style, seed, record } = spec;
        let bot_side = human.other();
        let bot_deck = if bot_side == Side::Corp { &corp } else { &runner };
        let personality = personality_for(style, bot_deck)?;
        let (state, _events) = GameState::setup(&corp.to_deck(), &runner.to_deck(), &registry, seed).map_err(|e| e.to_string())?;
        // `GameState::setup` is Standard rules on a shuffled deck, which is
        // exactly what the header's `setup` rebuilds from these fields.
        let header = MatchRecordHeader {
            seed,
            corp_deck: corp.to_deck(),
            runner_deck: runner.to_deck(),
            rules: MatchRules::default(),
            bot: Some(RecordedBot { side: bot_side, level, personality }),
        };
        // A rung is always a `Seat::Agent`: the ladder is built from the
        // view-based searches and deliberately excludes the one kind that
        // needs the index path (see `netrunner_cli::tui::build_bot_seat`).
        let bot = Seat::Agent(level.spec(bot_side).with_personality(personality).agent(seed.wrapping_add(1)));
        // Opened before the game so a bad record file fails now, not
        // after an hour of play.
        let record = match record {
            Some(RecordFile { path, player }) => Some(SeatRecord::open(SeatRecordSpec {
                path,
                player,
                human,
                level: Some(level),
                kind: BotKind::Heuristic,
                personality,
                seed,
                corp_deck: corp.id.clone(),
                runner_deck: runner.id.clone(),
            })?),
            None => None,
        };
        let (corp_seat, runner_seat) = match human {
            Side::Corp => (Seat::External, bot),
            Side::Runner => (bot, Seat::External),
        };
        let session = Session::new(state, (*registry).clone(), corp_seat, runner_seat).with_undo(UNDO_DEPTH);
        let (command_tx, command_rx) = mpsc::channel();
        let (message_tx, message_rx) = mpsc::channel();
        let history = Arc::new(Mutex::new(Vec::new()));
        let mirror = Arc::clone(&history);
        let thread = thread::Builder::new()
            .name("netrunner-match".to_string())
            .spawn(move || drive(session, human, record, &mirror, command_rx, message_tx))
            .map_err(|e| format!("could not start the match thread: {e}"))?;
        Ok(Self {
            commands: command_tx,
            messages: Mutex::new(message_rx),
            human,
            registry,
            finished: false,
            thread: Some(thread),
            header,
            history,
        })
    }

    /// The chair the person sits in.
    pub fn side(&self) -> Side {
        self.human
    }

    pub fn registry(&self) -> &CardRegistry {
        &self.registry
    }

    /// The next message, if one has arrived. Never blocks: a render loop
    /// calls this once a frame.
    pub fn poll(&mut self) -> Option<MatchMessage> {
        let received = self.messages.lock().map_or(Err(TryRecvError::Disconnected), |rx| rx.try_recv());
        match received {
            Ok(message) => {
                if matches!(message, MatchMessage::Ended { .. } | MatchMessage::Stalled { .. }) {
                    self.finished = true;
                }
                Some(message)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                // The thread is gone without saying why — it panicked. Say
                // so once, then nothing.
                if self.finished {
                    None
                } else {
                    self.finished = true;
                    Some(MatchMessage::Stalled { reason: "the match thread stopped unexpectedly".to_string() })
                }
            }
        }
    }

    /// The next message, waiting for it. For a pump with nothing else to
    /// do — a test, or a terminal. `None` once the thread has gone.
    pub fn wait(&mut self) -> Option<MatchMessage> {
        let received = self.messages.lock().map_err(|_| ()).and_then(|rx| rx.recv().map_err(|_| ()));
        match received {
            Ok(message) => {
                if matches!(message, MatchMessage::Ended { .. } | MatchMessage::Stalled { .. }) {
                    self.finished = true;
                }
                Some(message)
            }
            Err(_) => None,
        }
    }

    /// Hands the engine an action. Not filtered, not checked: the answer
    /// is the next `Applied` or `Rejected`. `Err` only if the match is
    /// already over.
    pub fn submit(&self, action: PlayerAction) -> Result<(), String> {
        self.commands.send(Command::Submit(action)).map_err(|_| "the match has ended".to_string())
    }

    /// Asks for the last move back. Answered by `Rewound`, or by
    /// `Rejected` when there is none to take — what it would cost was in
    /// the `Back` before the last `Awaiting`.
    ///
    /// **Not a `PlayerAction`, and not a way round one**: the state
    /// restored is one the engine produced, the history loses the entries
    /// since so the record still replays, and `submit` still never
    /// filters. It costs nothing, whichever kind it is: a local game is
    /// casual (the module doc).
    pub fn rewind(&self) -> Result<(), String> {
        self.commands.send(Command::Rewind).map_err(|_| "the match has ended".to_string())
    }

    /// The match so far as a record that replays: the header it was set
    /// up from and every action applied since, take-backs already taken
    /// out. What a bug report saves (`crate::bug_report`), and what
    /// `netrunner_cli replay` opens. Never blocks on the match thread.
    ///
    /// It may trail the board by the action whose `Applied` is in flight,
    /// never lead it, and it is always a prefix that replays.
    pub fn record(&self) -> (MatchRecordHeader, MatchHistory) {
        let entries = self.history.lock().map(|entries| entries.clone()).unwrap_or_default();
        (self.header.clone(), MatchHistory::from_entries(entries))
    }

    /// `Ended` or `Stalled` has been received; nothing more will come.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Leaves the game. A forfeit is recorded from turn 3 on, as the
    /// terminal's `q` does. The thread is not waited for: it may be inside
    /// a search, and it will see the quit when that search returns.
    pub fn quit(&mut self) {
        let _ = self.commands.send(Command::Quit);
    }

    /// `quit`, then wait for the thread — so a test can assert the record
    /// file was written. A screen never needs this.
    pub fn join(mut self) {
        self.quit();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for MatchHandle {
    fn drop(&mut self) {
        self.quit();
    }
}

/// The style a bot plays a deck in: the flag if one was given, else the
/// deck's own (`DeckFile::style`). One rule for the terminal's
/// `Config::personality_for` and the desktop's form, so
/// `--corp-level 4 --corp-personality rush` and the desktop's "Rush"
/// choice seat the same bot.
pub fn personality_for(flag: Option<Personality>, deck: &DeckFile) -> Result<Personality, String> {
    match flag {
        Some(personality) => Ok(personality),
        None => Personality::for_deck(deck),
    }
}

/// The pass a client takes for the person, when passing priority is the
/// only thing `view` lists worth stopping for: `Some` exactly when
/// `legal_actions` is one `PassPriority` and, beside it, nothing but
/// rezzes of traps (`board::rez::is_idle_rez`).
///
/// **A client policy, not a rule.** Passing over and over is the whole of
/// a run from the other chair, and a click that has no alternative asks
/// nothing of the person (Phase 7 §3's list, item 2). The engine still
/// opens every window and still hears every pass; the client only stops
/// asking. Nothing wider than the lone pass is taken — a lone `EndTurn`
/// or a lone access decision is a moment the person may want to look at,
/// and a pass beside anything else (a rez, an ability) is a real choice.
/// **A trap's rez is the one exception:** Urtica Cipher does its work
/// face down and rezzing it only shows the Runner what it is, so it is no
/// choice at all — and an installed trap used to stop the Corp in every
/// window of the Runner's turn to offer it. It stays on the card's menu.
/// A lesson does not use it: a step that teaches passing must be pressed.
pub fn lone_pass(view: &ClientView, registry: &CardRegistry) -> Option<PlayerAction> {
    let mut passes = view.legal_actions.iter().filter(|action| matches!(action, PlayerAction::PassPriority { .. }));
    let pass = passes.next()?;
    let rest_idle = view.legal_actions.iter().filter(|action| !matches!(action, PlayerAction::PassPriority { .. })).all(|action| crate::board::rez::is_idle_rez(action, view, registry));
    (passes.next().is_none() && rest_idle).then(|| pass.clone())
}

/// A stall, in the words the terminal reports it with.
pub fn stall_message(reason: StallReason) -> String {
    match reason {
        StallReason::BudgetExhausted => "match ended without reaching GameOver (step budget exhausted)".to_string(),
        StallReason::NoCurrentActor => "match stalled: no side has a decision pending".to_string(),
        StallReason::NoLegalActions { side } => format!("match deadlocked: {side:?} has priority but no legal action"),
        StallReason::DecisionLivelock { side, source_card, actions } => format!(
            "match livelocked: {side:?} spent {actions} actions inside {}'s prompt without resolving it",
            source_card.as_ref().map_or("an unnamed card", |card| card.0.as_str())
        ),
    }
}

/// The match thread. Returns when the game ends, stalls, or the client
/// quits or goes away; the game is recorded on every one of those paths
/// that the terminal records it on.
fn drive(
    mut session: Session,
    human: Side,
    mut seat: Option<SeatRecord>,
    history: &Mutex<Vec<HistoryEntry>>,
    commands: Receiver<Command>,
    messages: Sender<MatchMessage>,
) {
    // A send to a client that has dropped its handle is a quit.
    let forfeit = |session: &Session, seat: &mut Option<SeatRecord>| {
        if let Some(seat) = seat.take()
            && let Some(outcome) = record::quit_outcome(session.state().turn, human)
        {
            let _ = seat.finish(outcome);
        }
    };
    let mut back = None;
    loop {
        let step = loop {
            match session.step() {
                SessionStep::Applied { .. } => {
                    mirror(&session, history);
                    if send_applied(&session, human, &messages).is_err() {
                        return forfeit(&session, &mut seat);
                    }
                }
                other => break other,
            }
        };
        match step {
            SessionStep::Awaiting { side, view } if side == human => {
                let rewind = session.can_rewind();
                if rewind != back {
                    back = rewind;
                    if messages.send(MatchMessage::Back { rewind }).is_err() {
                        return forfeit(&session, &mut seat);
                    }
                }
                if messages.send(MatchMessage::Awaiting { view }).is_err() {
                    return forfeit(&session, &mut seat);
                }
                loop {
                    match commands.recv() {
                        Ok(Command::Submit(action)) => match session.submit(action) {
                            Ok(()) => {
                                mirror(&session, history);
                                if send_applied(&session, human, &messages).is_err() {
                                    return forfeit(&session, &mut seat);
                                }
                                break;
                            }
                            // `Display`, not `Debug` — the engine's authored
                            // message, as `MatchSession` and the TUI show it.
                            Err(SubmitError::Rules(error)) => {
                                if messages.send(MatchMessage::Rejected { reason: error.to_string() }).is_err() {
                                    return forfeit(&session, &mut seat);
                                }
                            }
                            Err(error) => {
                                let _ = messages.send(MatchMessage::Stalled { reason: error.to_string() });
                                return;
                            }
                        },
                        Ok(Command::Rewind) => match session.rewind() {
                            Some(rewound) => {
                                mirror(&session, history);
                                let view = Box::new(session.view_for(human));
                                if messages.send(MatchMessage::Rewound { view, removed: rewound.removed, kind: rewound.kind }).is_err() {
                                    return forfeit(&session, &mut seat);
                                }
                                break;
                            }
                            None => {
                                if messages.send(MatchMessage::Rejected { reason: "there is no move to take back".to_string() }).is_err() {
                                    return forfeit(&session, &mut seat);
                                }
                            }
                        },
                        Ok(Command::Quit) | Err(_) => return forfeit(&session, &mut seat),
                    }
                }
            }
            SessionStep::Awaiting { side, .. } => {
                // A rung is always a `Seat::Agent`, so this is a seating
                // bug, not a state the loop can drive out of.
                let _ = messages.send(MatchMessage::Stalled { reason: format!("the {side:?} bot seat could not be resolved in-process") });
                return;
            }
            SessionStep::Ended { winner, reason } => {
                let view = Box::new(session.view_for(human));
                let (report, notice) = match seat.take().map(|seat| seat.finish(record::outcome_of(winner))) {
                    Some(Ok(report)) => (Some(report), None),
                    Some(Err(error)) => (None, Some(format!("the result could not be recorded: {error}"))),
                    None => (None, None),
                };
                let _ = messages.send(MatchMessage::Ended { winner, reason, view, report, notice });
                return;
            }
            SessionStep::Stalled(reason) => {
                let _ = messages.send(MatchMessage::Stalled { reason: stall_message(reason) });
                return;
            }
            SessionStep::Applied { .. } => unreachable!("the inner loop only breaks once it can no longer apply"),
        }
    }
}

/// Brings the client's copy of the history level with the session's: cut
/// back to it after a take-back, then the entries since added. Called
/// after every change, so a take-back never leaves an entry behind that a
/// later action of the same length would hide.
fn mirror(session: &Session, history: &Mutex<Vec<HistoryEntry>>) {
    let Ok(mut copy) = history.lock() else { return };
    let entries = session.history().entries();
    copy.truncate(entries.len());
    let known = copy.len();
    copy.extend_from_slice(&entries[known..]);
}

/// The action just applied, as the human may see it, with the board it
/// left. Read immediately, before the next action resolves — see the
/// module doc for why.
fn send_applied(session: &Session, human: Side, messages: &Sender<MatchMessage>) -> Result<(), ()> {
    let Some(entry) = session.last_entry_for(human) else { return Ok(()) };
    let view = Box::new(session.view_for(human));
    messages.send(MatchMessage::Applied { entry, view }).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::LocalRecord;
    use std::path::Path;

    fn temp_dir(name: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("netrunner_client_play_{name}_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn spec(human: Side, seed: u64, record: Option<RecordFile>) -> LocalMatchSpec {
        let registry = Arc::new(crate::decks::sample_deck_registry());
        let corp = netrunner_core::decks::by_id("discretion_advised").expect("built-in deck").clone();
        let runner = netrunner_core::decks::by_id("stolen_goods").expect("built-in deck").clone();
        LocalMatchSpec { registry, corp, runner, human, level: Level::Novice, style: None, seed, record }
    }

    /// The pump a client is: wait for `Awaiting`, submit the first legal
    /// action, until the game ends. Every `Applied` in between is the
    /// human's masked copy, so the count of them is the count of actions
    /// the human was told about.
    fn play_out(handle: &mut MatchHandle, choose: impl Fn(&ClientView) -> PlayerAction) -> (MatchMessage, usize) {
        let mut applied = 0;
        loop {
            match handle.wait().expect("the thread is alive until it says Ended") {
                MatchMessage::Awaiting { view } => handle.submit(choose(&view)).unwrap(),
                MatchMessage::Applied { .. } => applied += 1,
                MatchMessage::Back { .. } => {}
                MatchMessage::Rewound { .. } => panic!("nothing asked for a move back"),
                MatchMessage::Rejected { reason } => panic!("a legal action was rejected: {reason}"),
                message @ (MatchMessage::Ended { .. } | MatchMessage::Stalled { .. }) => return (message, applied),
            }
        }
    }

    /// A whole game against the bottom rung, the human seat played by the
    /// test off `legal_actions`, ends with `Ended` and a report.
    #[test]
    fn a_local_match_plays_to_the_end_and_is_recorded() {
        let dir = temp_dir("recorded");
        let path = dir.join("record.json");
        let record = Some(RecordFile { path: path.clone(), player: "tester".to_string() });
        let mut handle = MatchHandle::start_local(spec(Side::Runner, 3, record)).unwrap();
        assert_eq!(handle.side(), Side::Runner);
        let (last, applied) = play_out(&mut handle, |view| view.legal_actions[0].clone());
        match last {
            MatchMessage::Ended { report, notice, view, .. } => {
                assert!(report.is_some(), "a recorded game reports where it leaves the record");
                assert_eq!(notice, None);
                assert_eq!(view.viewer.side(), Some(Side::Runner), "the final board is the human's own view");
            }
            other => panic!("{other:?}"),
        }
        assert!(applied > 10, "{applied} applied messages: one per action, both seats");
        assert!(handle.is_finished());
        let log = LocalRecord::load(&path).unwrap();
        let (won, drawn, lost) = log.record_against("tester", Side::Runner, &Level::Novice.record_id());
        assert_eq!(won + drawn + lost, 1, "one game in the record");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// An undo past something newly seen goes back exactly and costs
    /// nothing: a game against a bot is casual, so it is recorded like any
    /// other and the end has nothing to say about it. This test once
    /// asserted the opposite — that the undo took the game's rating.
    #[test]
    fn an_undone_game_is_recorded_like_any_other() {
        let dir = temp_dir("undone");
        let path = dir.join("record.json");
        let record = Some(RecordFile { path: path.clone(), player: "tester".to_string() });
        let mut handle = MatchHandle::start_local(spec(Side::Runner, 3, record)).unwrap();
        let (mut offered, mut undone, mut before) = (None, false, None);
        let last = loop {
            match handle.wait().expect("the thread is alive until it says Ended") {
                MatchMessage::Back { rewind } => offered = rewind,
                MatchMessage::Awaiting { view } if offered == Some(Rewind::Undo) && !undone => {
                    undone = true;
                    handle.rewind().unwrap();
                    let MatchMessage::Rewound { view: restored, removed, kind } = handle.wait().unwrap() else { panic!("a move was there to undo") };
                    assert_eq!(kind, Rewind::Undo);
                    assert!(removed >= 1);
                    assert_eq!(Some(&restored.runner.clicks), before.as_ref(), "the board the move was made from");
                    assert_ne!(restored.runner.clicks, view.runner.clicks);
                }
                MatchMessage::Awaiting { view } => {
                    before = Some(view.runner.clicks);
                    handle.submit(view.legal_actions[0].clone()).unwrap();
                }
                MatchMessage::Applied { .. } | MatchMessage::Rewound { .. } => {}
                MatchMessage::Rejected { reason } => panic!("{reason}"),
                message @ (MatchMessage::Ended { .. } | MatchMessage::Stalled { .. }) => break message,
            }
        };
        assert!(undone, "the first legal action is a click sooner or later");
        let MatchMessage::Ended { report, notice, .. } = last else { panic!("{last:?}") };
        assert!(report.is_some());
        assert_eq!(notice, None);
        let (won, drawn, lost) = LocalRecord::load(&path).unwrap().record_against("tester", Side::Runner, &Level::Novice.record_id());
        assert_eq!(won + drawn + lost, 1, "an undone game is in the record");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// What a bug report saves replays to the board the person is looking
    /// at, take-backs and all: the header rebuilds the opening, the
    /// entries are both seats' actions, and the one a take-back removed is
    /// gone from the copy as it is from the session. Checked at every
    /// decision, so a mirror that fell behind or kept a taken-back entry
    /// fails at the first one.
    #[test]
    fn the_record_replays_to_the_board_the_person_sees() {
        let mut handle = MatchHandle::start_local(spec(Side::Runner, 11, None)).unwrap();
        let registry = handle.registry().clone();
        let (mut offered, mut taken_back, mut decisions) = (None, 0, 0);
        while decisions < 80 {
            match handle.wait().expect("the thread is alive") {
                MatchMessage::Back { rewind } => offered = rewind,
                MatchMessage::Awaiting { view } => {
                    decisions += 1;
                    let (header, history) = handle.record();
                    assert_eq!(header.bot, Some(RecordedBot { side: Side::Corp, level: Level::Novice, personality: header.bot.unwrap().personality }));
                    let (mut state, _) = header.setup(&registry).expect("the header sets up");
                    for entry in history.entries() {
                        state = netrunner_core::rules::apply_action(&state, &registry, entry.action.clone()).expect("replays").0;
                    }
                    assert_eq!(netrunner_core::view::build_client_view(&state, &registry, Side::Runner), *view, "decision {decisions}");
                    if offered.is_some() && decisions % 7 == 0 {
                        taken_back += 1;
                        handle.rewind().unwrap();
                    } else {
                        handle.submit(view.legal_actions[0].clone()).unwrap();
                    }
                }
                MatchMessage::Applied { .. } | MatchMessage::Rewound { .. } => {}
                MatchMessage::Rejected { reason } => panic!("{reason}"),
                MatchMessage::Ended { .. } | MatchMessage::Stalled { .. } => break,
            }
        }
        assert!(taken_back >= 2, "the test took {taken_back} moves back; it is about take-backs");
    }

    /// The rejection path: an action the engine refuses comes back as
    /// `Rejected` with the engine's words, and the same seat is still
    /// awaiting — the next legal submit is applied.
    #[test]
    fn a_refused_action_is_reported_and_the_seat_stays_awaiting() {
        let mut handle = MatchHandle::start_local(spec(Side::Corp, 5, None)).unwrap();
        let MatchMessage::Awaiting { view } = handle.wait().unwrap() else { panic!("the Corp mulligans first") };
        handle.submit(PlayerAction::EndTurn).unwrap();
        let MatchMessage::Rejected { reason } = handle.wait().unwrap() else { panic!("an end of turn during the mulligan is illegal") };
        assert!(!reason.is_empty() && !reason.starts_with("Rules("), "the engine's own message: {reason}");
        handle.submit(view.legal_actions[0].clone()).unwrap();
        let MatchMessage::Applied { entry, .. } = handle.wait().unwrap() else { panic!("the legal action was applied") };
        assert_eq!(entry.side, Side::Corp);
        handle.join();
    }

    /// Quitting before turn 3 records nothing; quitting from turn 3 on is
    /// a loss — the terminal's rule, through the same function.
    #[test]
    fn quitting_records_a_loss_only_from_turn_three() {
        let dir = temp_dir("quit");
        let path = dir.join("record.json");
        let record = || Some(RecordFile { path: path.clone(), player: "quitter".to_string() });

        let handle = MatchHandle::start_local(spec(Side::Corp, 7, record())).unwrap();
        handle.join();
        let games = |path: &Path| LocalRecord::load(path).map(|b| b.record_against("quitter", Side::Corp, &Level::Novice.record_id())).unwrap_or((0, 0, 0));
        assert_eq!(games(&path), (0, 0, 0), "nothing before turn 3");

        let mut handle = MatchHandle::start_local(spec(Side::Corp, 7, record())).unwrap();
        loop {
            match handle.wait().unwrap() {
                MatchMessage::Awaiting { view } if view.turn >= 3 => break,
                MatchMessage::Awaiting { view } => handle.submit(view.legal_actions[0].clone()).unwrap(),
                MatchMessage::Applied { .. } | MatchMessage::Back { .. } => {}
                other => panic!("{other:?}"),
            }
        }
        handle.join();
        assert_eq!(games(&path), (0, 0, 1), "a quit from turn 3 is a loss in the record");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Over a whole game from the Runner's chair — where the Corp's turn
    /// asks for pass after pass — the lone pass is found exactly when it
    /// is the only action, and taking it for the person finishes the game.
    #[test]
    fn a_lone_pass_is_taken_only_when_it_is_the_only_action() {
        let mut handle = MatchHandle::start_local(spec(Side::Runner, 3, None)).unwrap();
        let registry = crate::decks::sample_deck_registry();
        let mut lone = 0;
        loop {
            match handle.wait().expect("the thread is alive until it says Ended") {
                MatchMessage::Awaiting { view } => {
                    match lone_pass(&view, &registry) {
                        Some(pass) => {
                            assert_eq!(view.legal_actions, vec![pass.clone()]);
                            lone += 1;
                            handle.submit(pass).unwrap();
                        }
                        None => {
                            assert!(view.legal_actions.len() != 1 || !matches!(view.legal_actions[0], PlayerAction::PassPriority { .. }));
                            handle.submit(view.legal_actions[0].clone()).unwrap();
                        }
                    }
                }
                MatchMessage::Applied { .. } | MatchMessage::Back { .. } | MatchMessage::Rewound { .. } => {}
                MatchMessage::Rejected { reason } => panic!("the lone pass was rejected: {reason}"),
                MatchMessage::Ended { .. } => break,
                MatchMessage::Stalled { reason } => panic!("{reason}"),
            }
        }
        assert!(lone > 0, "the Runner is asked to pass alone during the Corp's turn");
    }

    /// Over a whole game from the Corp's chair, "no more this run" pressed
    /// at the first window of every run: each pass it takes is legal and
    /// in a run, it never answers a prompt, and it is off again once the
    /// run is — so the next run is pressed for afresh.
    #[test]
    fn the_rest_of_a_run_is_passed_and_the_next_run_asks_again() {
        let registry = crate::decks::sample_deck_registry();
        let (mut pressed, mut passed, mut beside_a_choice) = (0, 0, 0);
        for seed in 0..6 {
        let mut handle = MatchHandle::start_local(spec(Side::Corp, seed, None)).unwrap();
        let mut run_pass = crate::run_pass::RunPass::default();
        loop {
            match handle.wait().expect("the thread is alive until it says Ended") {
                MatchMessage::Awaiting { view } => {
                    run_pass.see(&view);
                    if let Some(pass) = run_pass.pass(&view) {
                        assert!(view.active_run.is_some() && view.pending_decision.is_none());
                        assert!(view.legal_actions.contains(&pass));
                        passed += 1;
                        if lone_pass(&view, &registry).is_none() {
                            beside_a_choice += 1;
                        }
                        handle.submit(pass).unwrap();
                        continue;
                    }
                    if view.active_run.is_none() {
                        assert!(!run_pass.is_on(), "a run pass outlived its run");
                    }
                    if let Some(pass) = run_pass.start(&view) {
                        pressed += 1;
                        handle.submit(pass).unwrap();
                        continue;
                    }
                    // An install before anything else and never a rez, so
                    // the Corp holds unrezzed ICE when the Runner runs and
                    // the pass sits beside a choice. A selection is
                    // confirmed as soon as it can be, or it toggles one
                    // card on and off for ever.
                    let first = &view.legal_actions[0];
                    let action = if view.pending_decision.is_some() {
                        view.legal_actions.iter().find(|action| matches!(action, PlayerAction::ConfirmCardSelection)).unwrap_or(first)
                    } else {
                        let mut actions = view.legal_actions.iter();
                        actions
                            .clone()
                            .find(|action| matches!(action, PlayerAction::InstallCard { .. }))
                            .or_else(|| actions.find(|action| !matches!(action, PlayerAction::RezIce { .. })))
                            .unwrap_or(first)
                    };
                    handle.submit(action.clone()).unwrap();
                }
                MatchMessage::Applied { view, .. } => run_pass.see(&view),
                MatchMessage::Back { .. } | MatchMessage::Rewound { .. } => {}
                MatchMessage::Rejected { reason } => panic!("a run pass was rejected: {reason}"),
                MatchMessage::Ended { .. } => break,
                MatchMessage::Stalled { reason } => panic!("{reason}"),
            }
        }
        }
        assert!(pressed > 1, "pressed in more than one run: {pressed}");
        assert!(passed > 0 && beside_a_choice > 0, "passed {passed}, {beside_a_choice} of them beside a choice");
    }

    /// The deck's own style is the default and a flag overrides it.
    #[test]
    fn the_style_is_the_flag_else_the_decks_own() {
        let deck = netrunner_core::decks::by_id("discretion_advised").unwrap();
        let own = personality_for(None, &deck).unwrap();
        assert_eq!(Some(own), deck.style.as_deref().and_then(|s| s.parse().ok()).or(Some(Personality::Balanced)));
        assert_eq!(personality_for(Some(Personality::Glacier), &deck).unwrap(), Personality::Glacier);
    }
}
