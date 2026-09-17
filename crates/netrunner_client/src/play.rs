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
//! their action; on `Ended`, record the rating; on a quit, record a
//! forfeit from `ratings::FORFEIT_FROM_TURN` on and nothing before it.
//! Rated the same way, quitting the same way, so a game played here and a
//! game played in the terminal land on the same ladder by the same rule.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use netrunner_bots::{Level, Personality};
use netrunner_core::cards::CardRegistry;
use netrunner_core::decks::DeckFile;
use netrunner_core::rules::{GameState, PlayerAction, Side};
use netrunner_core::view::ClientView;
use netrunner_session::{PublicHistoryEntry, Seat, Session, SessionStep, StallReason, SubmitError};

/// Re-exported so a client that only ever holds a `MatchHandle` need not
/// name the session crate for the one type its `Ended` carries.
pub use netrunner_session::GameEndReason;

use crate::ratings::{self, BotKind, RatingReport, SeatRating, SeatRatingSpec};

/// Everything a local game against a rung needs: the two decks, which
/// chair is the person's, the rung and style of the other, the seed, and
/// whether the result goes on the ladder.
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
    /// `None` is an unrated game.
    pub rating: Option<RatingFile>,
}

/// Where a rated game is recorded and under what name.
#[derive(Debug, Clone)]
pub struct RatingFile {
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
    /// The human must choose from `view.legal_actions`.
    Awaiting { view: Box<ClientView> },
    /// The engine refused the last `submit`; the human is still awaiting
    /// on the same view. `reason` is `RulesError`'s own message.
    Rejected { reason: String },
    /// The match ended. `report` is what the game did to the player's
    /// rating, `None` for an unrated game; `notice` is a rating file that
    /// would not save, which must not hide the result.
    Ended { winner: Side, reason: GameEndReason, view: Box<ClientView>, report: Option<RatingReport>, notice: Option<String> },
    /// The session stopped without a `GameOver`: a stall, or a bot seat
    /// the session could not resolve. Nothing more will arrive.
    Stalled { reason: String },
}

enum Command {
    Submit(PlayerAction),
    Quit,
}

/// A running match, as the client holds it. Dropping it quits the game
/// the way Escape does in the terminal — a forfeit from turn 3 on.
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
}

impl MatchHandle {
    /// Sets the game up and starts its thread. Everything that can fail
    /// fails *here* — an illegal deck, a ratings file that will not load —
    /// so a screen can show the reason as a notice rather than a game
    /// dying on its first frame (Phase 6's rule: a game that fails to
    /// start is a notice, not a drop to the shell).
    pub fn start_local(spec: LocalMatchSpec) -> Result<Self, String> {
        let LocalMatchSpec { registry, corp, runner, human, level, style, seed, rating } = spec;
        let bot_side = human.other();
        let bot_deck = if bot_side == Side::Corp { &corp } else { &runner };
        let personality = personality_for(style, bot_deck)?;
        let (state, _events) = GameState::setup(&corp.to_deck(), &runner.to_deck(), &registry, seed).map_err(|e| e.to_string())?;
        // A rung is always a `Seat::Agent`: the ladder is built from the
        // view-based searches and deliberately excludes the one kind that
        // needs the index path (see `netrunner_cli::tui::build_bot_seat`).
        let bot = Seat::Agent(level.spec(bot_side).with_personality(personality).agent(seed.wrapping_add(1)));
        // Opened before the game so a bad ratings file fails now, not
        // after an hour of play.
        let rating = match rating {
            Some(RatingFile { path, player }) => Some(SeatRating::open(SeatRatingSpec {
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
        let session = Session::new(state, (*registry).clone(), corp_seat, runner_seat);
        let (command_tx, command_rx) = mpsc::channel();
        let (message_tx, message_rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("netrunner-match".to_string())
            .spawn(move || drive(session, human, rating, command_rx, message_tx))
            .map_err(|e| format!("could not start the match thread: {e}"))?;
        Ok(Self { commands: command_tx, messages: Mutex::new(message_rx), human, registry, finished: false, thread: Some(thread) })
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

    /// `quit`, then wait for the thread — so a test can assert the rating
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
/// only thing `view` lists: `Some` exactly when `legal_actions` is one
/// `PassPriority`.
///
/// **A client policy, not a rule.** Passing over and over is the whole of
/// a run from the other chair, and a click that has no alternative asks
/// nothing of the person (Phase 7 §3's list, item 2). The engine still
/// opens every window and still hears every pass; the client only stops
/// asking. Nothing wider than the lone pass is taken — a lone `EndTurn`
/// or a lone access decision is a moment the person may want to look at,
/// and a pass beside anything else (a rez, an ability) is a real choice.
/// A lesson does not use it: a step that teaches passing must be pressed.
pub fn lone_pass(view: &ClientView) -> Option<PlayerAction> {
    match view.legal_actions.as_slice() {
        [pass @ PlayerAction::PassPriority { .. }] => Some(pass.clone()),
        _ => None,
    }
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
/// quits or goes away; a rated game is recorded on every one of those
/// paths that the terminal records it on.
fn drive(mut session: Session, human: Side, mut rating: Option<SeatRating>, commands: Receiver<Command>, messages: Sender<MatchMessage>) {
    // A send to a client that has dropped its handle is a quit.
    let forfeit = |session: &Session, rating: &mut Option<SeatRating>| {
        if let Some(rating) = rating.take()
            && let Some(outcome) = ratings::quit_outcome(session.state().turn, human)
        {
            let _ = rating.finish(outcome);
        }
    };
    loop {
        let step = loop {
            match session.step() {
                SessionStep::Applied { .. } => {
                    if send_applied(&session, human, &messages).is_err() {
                        return forfeit(&session, &mut rating);
                    }
                }
                other => break other,
            }
        };
        match step {
            SessionStep::Awaiting { side, view } if side == human => {
                if messages.send(MatchMessage::Awaiting { view }).is_err() {
                    return forfeit(&session, &mut rating);
                }
                loop {
                    match commands.recv() {
                        Ok(Command::Submit(action)) => match session.submit(action) {
                            Ok(()) => {
                                if send_applied(&session, human, &messages).is_err() {
                                    return forfeit(&session, &mut rating);
                                }
                                break;
                            }
                            // `Display`, not `Debug` — the engine's authored
                            // message, as `MatchSession` and the TUI show it.
                            Err(SubmitError::Rules(error)) => {
                                if messages.send(MatchMessage::Rejected { reason: error.to_string() }).is_err() {
                                    return forfeit(&session, &mut rating);
                                }
                            }
                            Err(error) => {
                                let _ = messages.send(MatchMessage::Stalled { reason: error.to_string() });
                                return;
                            }
                        },
                        Ok(Command::Quit) | Err(_) => return forfeit(&session, &mut rating),
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
                let (report, notice) = match rating.take().map(|rating| rating.finish(ratings::outcome_of(winner))) {
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
    use crate::ratings::LocalRatings;
    use std::path::Path;

    fn temp_dir(name: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("netrunner_client_play_{name}_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn spec(human: Side, seed: u64, rating: Option<RatingFile>) -> LocalMatchSpec {
        let registry = Arc::new(crate::decks::sample_deck_registry());
        let corp = netrunner_core::decks::by_id("discretion_advised").expect("built-in deck").clone();
        let runner = netrunner_core::decks::by_id("stolen_goods").expect("built-in deck").clone();
        LocalMatchSpec { registry, corp, runner, human, level: Level::Novice, style: None, seed, rating }
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
                MatchMessage::Rejected { reason } => panic!("a legal action was rejected: {reason}"),
                message @ (MatchMessage::Ended { .. } | MatchMessage::Stalled { .. }) => return (message, applied),
            }
        }
    }

    /// A whole game against the bottom rung, the human seat played by the
    /// test off `legal_actions`, ends with `Ended` and a report when rated.
    #[test]
    fn a_local_match_plays_to_the_end_and_is_recorded() {
        let dir = temp_dir("rated");
        let path = dir.join("ratings.json");
        let rating = Some(RatingFile { path: path.clone(), player: "tester".to_string() });
        let mut handle = MatchHandle::start_local(spec(Side::Runner, 3, rating)).unwrap();
        assert_eq!(handle.side(), Side::Runner);
        let (last, applied) = play_out(&mut handle, |view| view.legal_actions[0].clone());
        match last {
            MatchMessage::Ended { report, notice, view, .. } => {
                assert!(report.is_some(), "a rated game reports what it did");
                assert_eq!(notice, None);
                assert_eq!(view.viewer.side(), Some(Side::Runner), "the final board is the human's own view");
            }
            other => panic!("{other:?}"),
        }
        assert!(applied > 10, "{applied} applied messages: one per action, both seats");
        assert!(handle.is_finished());
        let book = LocalRatings::load(&path).unwrap();
        let (won, drawn, lost) = book.record_against("tester", Side::Runner, &Level::Novice.rating_id());
        assert_eq!(won + drawn + lost, 1, "one game on the ladder");
        let _ = std::fs::remove_dir_all(dir);
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
    /// a forfeit — the terminal's rule, through the same function.
    #[test]
    fn quitting_records_a_forfeit_only_from_turn_three() {
        let dir = temp_dir("quit");
        let path = dir.join("ratings.json");
        let rating = || Some(RatingFile { path: path.clone(), player: "quitter".to_string() });

        let handle = MatchHandle::start_local(spec(Side::Corp, 7, rating())).unwrap();
        handle.join();
        let games = |path: &Path| LocalRatings::load(path).map(|b| b.record_against("quitter", Side::Corp, &Level::Novice.rating_id())).unwrap_or((0, 0, 0));
        assert_eq!(games(&path), (0, 0, 0), "nothing before turn 3");

        let mut handle = MatchHandle::start_local(spec(Side::Corp, 7, rating())).unwrap();
        loop {
            match handle.wait().unwrap() {
                MatchMessage::Awaiting { view } if view.turn >= 3 => break,
                MatchMessage::Awaiting { view } => handle.submit(view.legal_actions[0].clone()).unwrap(),
                MatchMessage::Applied { .. } => {}
                other => panic!("{other:?}"),
            }
        }
        handle.join();
        assert_eq!(games(&path), (0, 0, 1), "a quit from turn 3 is a loss on the ladder");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Over a whole game from the Runner's chair — where the Corp's turn
    /// asks for pass after pass — the lone pass is found exactly when it
    /// is the only action, and taking it for the person finishes the game.
    #[test]
    fn a_lone_pass_is_taken_only_when_it_is_the_only_action() {
        let mut handle = MatchHandle::start_local(spec(Side::Runner, 3, None)).unwrap();
        let mut lone = 0;
        loop {
            match handle.wait().expect("the thread is alive until it says Ended") {
                MatchMessage::Awaiting { view } => {
                    match lone_pass(&view) {
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
                MatchMessage::Applied { .. } => {}
                MatchMessage::Rejected { reason } => panic!("the lone pass was rejected: {reason}"),
                MatchMessage::Ended { .. } => break,
                MatchMessage::Stalled { reason } => panic!("{reason}"),
            }
        }
        assert!(lone > 0, "the Runner is asked to pass alone during the Corp's turn");
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
