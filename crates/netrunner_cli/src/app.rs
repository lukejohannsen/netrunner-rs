//! The interactive TUI's state: the human seat's latest `ClientView`
//! (received over a channel from a background `netrunner_server::
//! MatchSession` task — never the raw `GameState`), UI selection state, and
//! the most recent rejection/game-end notice.
//!
//! Exactly one side is the human seat; the other is always bot-controlled
//! (see `config::Config::corp`'s doc comment) — under real per-side
//! masking there's no coherent way for a single local terminal to
//! represent "both sides, simultaneously, from each one's own point of
//! view," so this app doesn't try to.

use netrunner_client::run_pass::RunPass;
use netrunner_client::standing::{optional_trigger, standing_answer, Answer, Answers};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::{mpsc, watch};

use netrunner_client::board::{offered_label, routes, ActionMap, Asks, AutoBreak, Next, Route};
use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{PlayerAction, Side, Viewer};
use netrunner_core::view::ClientView;
use netrunner_server::protocol::GameEndReason;
use netrunner_server::{ClientMessage, ServerMessage};
use netrunner_client::connection::Link;

// Lifted into the shared client core for the desktop (Phase 7 §3); the
// names stay reachable here so nothing in this crate had to move.
pub use netrunner_client::actions::{explain_action, push_log_line, visible_zones, CardZone};

pub struct App {
    pub registry: CardRegistry,
    /// The perspective this client was given: a seat, or a spectator's.
    pub viewer: Viewer,
    tx: mpsc::UnboundedSender<ClientMessage>,
    rx: mpsc::UnboundedReceiver<ServerMessage>,
    /// `None` until the first `StateUpdate` arrives from the match session
    /// (should be near-instant — the session broadcasts its initial state
    /// before waiting on anything).
    pub view: Option<ClientView>,
    pub selected: usize,
    pub should_quit: bool,
    pub last_rejection: Option<String>,
    pub game_ended: Option<(Side, GameEndReason)>,
    /// Rendered log of every resolved action, from `ServerMessage::
    /// ActionLog`. Remote play had no log at all until the match driver
    /// grew a `MatchHistory` the server could forward.
    pub action_log: Vec<String>,
    /// The connection is down — reconnecting, or gone for good — so
    /// nothing is submitted: an action chosen from the last view may be
    /// stale by the time the seat is back.
    pub connection_lost: bool,
    /// What the header shows while the connection is down, from the
    /// link's status line (`connection::Link::status_line`).
    pub connection_notice: Option<String>,
    /// The connection's state, for a remote match
    /// (`netrunner_client::remote::Joined::link`). A reconnect happens
    /// under `tx`/`rx`, which carry on as they were; this is how the
    /// screen learns of it. `None` for a channel pair with nothing behind
    /// it that can drop.
    link: Option<watch::Receiver<Link>>,
    link_state: Link,
    /// Whose decision is on the clock and when it runs out, from the last
    /// `ServerMessage::DecisionClock`; `None` on a host without a clock.
    pub decision_clock: Option<(Side, Instant)>,
    /// A card's printed text, while the player is reading it.
    pub modal: Option<Modal>,
    /// The card inspector, while it is open.
    pub card_picker: Option<CardPicker>,
    /// Every card's way through the encountered ICE
    /// (`netrunner_client::board::breaks`), listed after the actions.
    pub breaks: Vec<Route>,
    /// What each legal action of the current view goes on to ask
    /// (`netrunner_client::board::preview`), built when the view arrives.
    asks: Asks,
    /// The route under way: each `StateUpdate` asks it for the next step,
    /// which goes back without a keypress.
    breaking: Option<AutoBreak>,
    /// The person's answers to optional triggers
    /// (`netrunner_client::standing`), from the settings file. Empty
    /// until the caller fills it, so a test reads no file.
    pub answers: Answers,
    /// The Corp's "no more this run" (`w`, `netrunner_client::run_pass`).
    run_pass: RunPass,
}

impl App {
    pub fn new(
        registry: CardRegistry,
        viewer: impl Into<Viewer>,
        tx: mpsc::UnboundedSender<ClientMessage>,
        rx: mpsc::UnboundedReceiver<ServerMessage>,
    ) -> Self {
        let mut app = App {
            registry,
            viewer: viewer.into(),
            tx,
            rx,
            view: None,
            selected: 0,
            should_quit: false,
            last_rejection: None,
            game_ended: None,
            action_log: Vec::new(),
            connection_lost: false,
            connection_notice: None,
            link: None,
            link_state: Link::Up,
            decision_clock: None,
            modal: None,
            card_picker: None,
            breaks: Vec::new(),
            asks: Asks::default(),
            breaking: None,
            answers: Answers::default(),
            run_pass: RunPass::default(),
        };
        app.drain_messages();
        app
    }

    /// Follows `link` from now on: the connection's drops and resumes.
    pub fn follow_link(&mut self, link: watch::Receiver<Link>) {
        self.link = Some(link);
        self.read_link();
    }

    /// The link's latest state, onto the screen. Read ahead of the
    /// messages, because a resume's `Link::Up` comes before the fresh view
    /// it brings, and that view should be answered like any other.
    fn read_link(&mut self) {
        if let Some(link) = &mut self.link
            && link.has_changed().unwrap_or(false)
        {
            let state = link.borrow_and_update().clone();
            if state == Link::Up && self.link_state != Link::Up {
                // The log is not replayed (see `ServerMessage::ActionLog`),
                // and says so.
                self.action_log.push("(reconnected — actions resolved while away are not listed)".to_string());
                self.connection_notice = None;
            }
            self.connection_lost = state != Link::Up;
            self.link_state = state;
        }
        if self.link_state != Link::Up {
            // Recomputed every tick: the line counts the seconds.
            self.connection_notice = self.link_state.status_line(Instant::now());
        }
    }

    /// Non-blocking drain of every message the match session has sent
    /// since the last poll — called once at construction and once per TUI
    /// render tick, mirroring the ~100ms `event::poll` cadence the render
    /// loop already uses for keyboard input.
    pub fn drain_messages(&mut self) {
        self.read_link();
        loop {
            let message = match self.rx.try_recv() {
                Ok(message) => message,
                Err(mpsc::error::TryRecvError::Empty) => break,
                // Every sender is gone: the bridge task behind the socket
                // ended. Distinct from `Empty`, which is just "nothing
                // yet" — the two used to be treated alike, so a dropped
                // socket looked like a very quiet opponent.
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.connection_lost = true;
                    if self.connection_notice.is_none() && !self.is_game_over() {
                        self.connection_notice = Some("Connection lost. Press q to quit.".to_string());
                    }
                    break;
                }
            };
            match message {
                ServerMessage::StateUpdate(view) => {
                    if self.selected >= view.legal_actions.len() {
                        self.selected = 0;
                    }
                    // A seat asked only to pass is not asked: the pass
                    // goes straight back (`netrunner_client::play::
                    // lone_pass`). A spectator's view lists nothing.
                    self.run_pass.see(&view);
                    if !self.connection_lost
                        && let Some(pass) = netrunner_client::play::lone_pass(&view, &self.registry)
                    {
                        let _ = self.tx.send(ClientMessage::SubmitAction(pass));
                    } else if !self.connection_lost
                        && let Some(pass) = self.run_pass.pass(&view)
                    {
                        // The Corp said "no more this run": the same, until
                        // the run ends.
                        let _ = self.tx.send(ClientMessage::SubmitAction(pass));
                    } else if !self.connection_lost
                        && let Some((action, _)) = standing_answer(&view, &self.registry, &self.answers)
                    {
                        // A card's "you may" answered for good goes back
                        // the same way. Online has no take-back to ask
                        // again after.
                        let _ = self.tx.send(ClientMessage::SubmitAction(action));
                    }
                    self.last_rejection = None;
                    self.breaks.clear();
                    // A route under way sends its next step on the update
                    // where the seat has priority again; otherwise the
                    // update's routes are listed for the person.
                    match self.breaking.as_ref().map(|driver| driver.next(&view, &self.registry)) {
                        Some(Next::Submit(step)) if !self.connection_lost => {
                            let _ = self.tx.send(ClientMessage::SubmitAction(step));
                        }
                        Some(Next::Wait) => {}
                        Some(Next::Stopped(reason)) => {
                            self.breaking = None;
                            self.last_rejection = Some(reason);
                        }
                        Some(Next::Submit(_) | Next::Done) | None => {
                            self.breaking = None;
                            self.breaks = routes(&view, &self.registry);
                        }
                    }
                    if self.selected >= self.offered_actions_in(&view).len() + self.breaks.len() {
                        self.selected = 0;
                    }
                    self.asks = Asks::of(&view, &self.registry);
                    self.view = Some(*view);
                }
                ServerMessage::ActionLog(entry) => {
                    push_log_line(&mut self.action_log, &entry, &self.registry, self.view.as_ref())
                }
                ServerMessage::ActionRejected { reason } => {
                    self.breaking = None;
                    self.last_rejection = Some(reason);
                }
                ServerMessage::GameEnded { winner, reason } => {
                    self.game_ended = Some((winner, reason));
                    self.decision_clock = None;
                }
                ServerMessage::DecisionClock { side, remaining } => self.decision_clock = Some((side, Instant::now() + remaining)),
                // Handshake replies, consumed in `remote` before the
                // channel pair ever reaches `App`.
                ServerMessage::MatchJoined { .. }
                | ServerMessage::Spectating { .. }
                | ServerMessage::Queued { .. }
                | ServerMessage::ConnectRejected { .. }
                | ServerMessage::MatchList { .. }
                | ServerMessage::ResumeRejected { .. }
                | ServerMessage::Challenge { .. }
                | ServerMessage::Identified { .. }
                | ServerMessage::IdentifyRefused { .. }
                | ServerMessage::SignSeat { .. }
                | ServerMessage::Rated { .. } => {}
                // An attached connection's lobby replies, which a game
                // never carries.
                ServerMessage::Attached { .. }
                | ServerMessage::Lobbies { .. }
                | ServerMessage::LobbyJoined { .. }
                | ServerMessage::LobbyRefused { .. }
                | ServerMessage::LobbyLeft
                | ServerMessage::SeekRefused { .. }
                | ServerMessage::SeekCancelled
                | ServerMessage::BackInLobby { .. } => {}
            }
        }
    }

    pub fn is_game_over(&self) -> bool {
        self.game_ended.is_some()
    }

    pub fn legal_actions(&self) -> &[PlayerAction] {
        self.view.as_ref().map_or(&[], |view| view.legal_actions.as_slice())
    }

    /// The list the pane shows and Enter submits from: every legal action,
    /// less a second copy's selection toggle, which the first copy's row
    /// stands for (`netrunner_client::selection`).
    fn offered_actions(&self) -> Vec<PlayerAction> {
        netrunner_client::selection::shown(self.legal_actions(), self.view.as_ref(), &self.registry)
    }

    fn offered_actions_in(&self, view: &ClientView) -> Vec<PlayerAction> {
        netrunner_client::selection::shown(&view.legal_actions, Some(view), &self.registry)
    }

    /// `y` / `n`: the prompt's optional trigger answered `answer` now and
    /// from now on (`netrunner_client::standing`), kept in the settings
    /// file.
    fn remember(&mut self, answer: Answer) {
        if self.connection_lost {
            return;
        }
        let Some(prompt) = self.view.as_ref().and_then(|view| optional_trigger(view, &self.registry)) else { return };
        let Some(action) = prompt.action(answer) else { return };
        self.answers.set(prompt.key.clone(), Some(answer));
        if let Err(error) = crate::settings::remember(prompt.key, answer) {
            self.last_rejection = Some(format!("the answer is kept for this game only: {error}"));
        }
        let _ = self.tx.send(ClientMessage::SubmitAction(action));
    }

    /// `w`: the rest of this run passed for the Corp from this window
    /// on, or, while that is on, asked again (`netrunner_client::run_pass`).
    fn toggle_run_pass(&mut self) {
        if self.run_pass.is_on() {
            self.run_pass.stop();
            return;
        }
        if self.connection_lost {
            return;
        }
        if let Some(pass) = self.view.as_ref().and_then(|view| self.run_pass.start(view)) {
            let _ = self.tx.send(ClientMessage::SubmitAction(pass));
        }
    }

    fn submit_selected_action(&mut self) {
        // The action would vanish into a closed channel, and the view it
        // was chosen from may be stale by the time the seat is back.
        if self.connection_lost {
            return;
        }
        let offered = self.offered_actions();
        if let Some(action) = offered.get(self.selected).cloned() {
            let _ = self.tx.send(ClientMessage::SubmitAction(action));
            return;
        }
        // Past the actions: a route through the encountered ICE.
        let (Some(route), Some(view)) = (self.breaks.get(self.selected - offered.len()), self.view.as_ref()) else { return };
        let driver = AutoBreak::new(route, view, &self.registry);
        match driver.next(view, &self.registry) {
            Next::Submit(step) => {
                self.breaking = Some(driver);
                let _ = self.tx.send(ClientMessage::SubmitAction(step));
            }
            Next::Stopped(reason) => self.last_rejection = Some(reason),
            Next::Wait | Next::Done => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.is_game_over() {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                self.should_quit = true;
            }
            return;
        }

        // The same inspector keys as the local path (`tui::prompt_human`):
        // a modal dismisses on Esc/Enter/Space, the picker moves on
        // Up/Down and opens a card on Enter, and `c` opens the picker.
        if self.modal.is_some() {
            match key.code {
                // Back to the list the card was chosen from, if there is one.
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Backspace | KeyCode::Left => self.modal = None,
                KeyCode::Char('c') | KeyCode::Char('q') if self.card_picker.is_some() => {
                    self.modal = None;
                    self.card_picker = None;
                }
                _ => {}
            }
            return;
        }
        if let Some(picker) = &mut self.card_picker {
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('c') => self.card_picker = None,
                KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => {
                    if !picker.back() {
                        self.card_picker = None;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => picker.move_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => picker.move_selection(1),
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right | KeyCode::Char('l') => {
                    if let Some(id) = picker.enter() {
                        self.modal = Some(card_modal(&id, &self.registry));
                    }
                }
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.submit_selected_action(),
            KeyCode::Char('y') => self.remember(Answer::Always),
            KeyCode::Char('n') => self.remember(Answer::Never),
            KeyCode::Char('w') => self.toggle_run_pass(),
            KeyCode::Char('c') => {
                if let Some(view) = &self.view {
                    self.card_picker = Some(CardPicker::open(view, &self.registry));
                }
            }
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: i32) {
        let len = self.offered_actions().len() + self.breaks.len();
        if len == 0 {
            return;
        }
        let next = (self.selected as i32 + delta).rem_euclid(len as i32);
        self.selected = next as usize;
    }
}

/// What `tui::{draw_header, draw_board, draw_actions}` need to render one
/// frame — implemented by `App` (the remote/channel-backed path) and by
/// `tui::LocalUiState` (the local `netrunner_single_player`-backed path),
/// so both share the same rendering code instead of duplicating it.
pub trait RenderableView {
    fn registry(&self) -> &CardRegistry;
    /// Whose eyes the board is drawn through — a seat's, or a spectator's.
    fn viewer(&self) -> Viewer;
    fn view(&self) -> Option<&ClientView>;
    fn selected(&self) -> usize;
    /// Labels for the actions on offer, in the order they are listed.
    /// Under a lesson this is the gated subset, not all of
    /// `view.legal_actions` — see `tui::LocalUiState::offered_actions`.
    fn legal_action_labels(&self) -> Vec<String>;
    /// The action the highlight sits on, for the coaching panel to explain.
    fn selected_action(&self) -> Option<PlayerAction>;
    /// The running action log. Both paths have one now, so both render the
    /// same four-region layout.
    fn action_log(&self) -> &[String];
    /// What the board may act on right now, and in which mood, so a card
    /// the engine will accept something on is drawn in colour
    /// (`netrunner_client::board::affordance`). Built from the view the
    /// surface is already holding, which is also the list the actions
    /// pane draws — so the board and the pane agree by construction.
    ///
    /// `None` turns the colour off wholesale, which is what a surface
    /// with no decision to offer wants (the replay viewer) and what a
    /// lesson wants: a lesson narrows the offered actions to the step's
    /// own (`tui::LocalUiState::offered_actions`), and a board glowing at
    /// the rest would be arguing with the lesson.
    fn action_map(&self) -> Option<ActionMap> {
        if self.coaching().is_some() {
            return None;
        }
        self.view().map(|view| ActionMap::build(view, self.registry()))
    }

    /// A title for the actions pane other than the live game's — the
    /// replay viewer lists a step's events there, where a title promising
    /// "Enter to act" would lie. `None` keeps the default.
    fn actions_title(&self) -> Option<String> {
        None
    }
    /// A line about a key the list does not show — the local game's way
    /// to take the last move back. `None` wherever there is none.
    fn notice(&self) -> Option<String> {
        None
    }
    /// The engine's reason for refusing the last submission, until the
    /// next state arrives. `None` on a path that has none to show.
    fn last_rejection(&self) -> Option<&str> {
        None
    }
    /// A connection-status line for the header, while the remote path is
    /// between sockets. `None` everywhere else — the local path has no
    /// connection to lose.
    fn connection_notice(&self) -> Option<&str> {
        None
    }
    /// Whose decision is on the host's clock and how long is left, for the
    /// header. `None` on the local path and on a host without a clock.
    fn decision_clock(&self) -> Option<(Side, Duration)> {
        None
    }
    /// Lesson coaching to render beside the board; `None` outside a lesson,
    /// which is the only case the remote path ever has.
    fn coaching(&self) -> Option<&Coaching> {
        None
    }
    /// The card inspector's list, while it is open.
    fn card_picker(&self) -> Option<&CardPicker> {
        None
    }
    /// An open popup, which owns the keyboard until dismissed.
    fn modal(&self) -> Option<&Modal> {
        None
    }
}

/// A popup that owns the keyboard until dismissed: lesson intros and
/// outros, the game-over notice. The one modal the TUI had before this —
/// game over — was a `Clear` + bordered `Paragraph` with its own key loop
/// and no routing; `tui::draw_modal` generalises the drawing and
/// `tui::prompt_human` routes keys to an open one first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Modal {
    pub title: String,
    pub body: String,
    pub footer: String,
}

/// The card inspector: the places the viewer may look, then the cards in
/// the place they chose, then one card's printed text (`card_modal`).
/// Two levels because a heap or a wide board is dozens of cards, and a
/// person looks in a place before they look at a card — "what is in
/// Archives?" is the question, and a flat list answers a different one.
/// `back` retreats one level at a time so a reader can go from one
/// card's text to the next without reopening anything; only `back` at the
/// top level closes the inspector.
///
/// **Built from the `ClientView` and nothing else.** A card is listed only
/// if the view names it — the viewer's own hand, a rezzed or owned
/// install, a faceup archived card, the heap, a scored agenda — so the
/// mask decides what can be inspected exactly as it decides what is
/// drawn. An unrezzed opponent's card has no `CardId` in the view and
/// therefore no entry here; a place with nothing to show is not listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardPicker {
    pub zones: Vec<CardZone>,
    /// The highlighted place.
    pub zone: usize,
    /// The highlighted card within it, once the reader has stepped in;
    /// `None` while they are still choosing a place.
    pub card: Option<usize>,
}

impl CardPicker {
    pub fn open(view: &ClientView, registry: &CardRegistry) -> Self {
        Self { zones: visible_zones(view, registry), zone: 0, card: None }
    }

    pub fn current_zone(&self) -> Option<&CardZone> {
        self.zones.get(self.zone)
    }

    /// Up/Down at whichever level the reader is on.
    pub fn move_selection(&mut self, delta: i32) {
        let step = |index: usize, len: usize| if len == 0 { 0 } else { (index as i32 + delta).rem_euclid(len as i32) as usize };
        match self.card {
            None => self.zone = step(self.zone, self.zones.len()),
            Some(card) => self.card = Some(step(card, self.current_zone().map_or(0, |zone| zone.cards.len()))),
        }
    }

    /// Enter: step into the highlighted place, or hand back the
    /// highlighted card to be read. The list is left as it stands either
    /// way, so `back` from the card's text lands on the same card.
    pub fn enter(&mut self) -> Option<CardId> {
        match self.card {
            None => {
                if self.current_zone().is_some_and(|zone| !zone.cards.is_empty()) {
                    self.card = Some(0);
                }
                None
            }
            Some(card) => self.current_zone().and_then(|zone| zone.cards.get(card)).map(|(_, id)| id.clone()),
        }
    }

    /// Esc: one level up. `false` when already at the top, which is the
    /// caller's cue to close the inspector.
    pub fn back(&mut self) -> bool {
        if self.card.take().is_some() {
            return true;
        }
        false
    }

    pub fn zone_labels(&self) -> Vec<String> {
        self.zones.iter().map(|zone| format!("{} ({})", zone.name, zone.cards.len())).collect()
    }

    pub fn card_labels(&self) -> Vec<String> {
        self.current_zone().map(|zone| zone.cards.iter().map(|(label, _)| label.clone()).collect()).unwrap_or_default()
    }
}

pub use netrunner_client::card_face::Face;
pub use netrunner_client::prose::engine_reading;

/// One card as a person reads it: the type line, the printed numbers,
/// the printed text with its line breaks, the flavour. The engine's DSL is
/// not shown — the printed text is what the DSL was written from, and it
/// is the sentence a player can act on.
pub fn card_modal(id: &CardId, registry: &CardRegistry) -> Modal {
    let Some(card) = registry.get(id) else {
        return Modal::new(&id.0, "This card is not in the registry.", "Esc to close");
    };
    // The printed card comes from `card_face::Face`, which is the one
    // authority on what a card prints and where. Three copies of this
    // used to exist — here, the desktop browser's `numbers_line`, and
    // nothing at all at an access — and all three built the numbers off
    // `CardDefinition`, so all three printed `Cost 0` on an agenda.
    // Symbols render as `Symbol::fallback` (`¢`, `»`) rather than the
    // `[credit]` tokens this printed before: a terminal has the
    // stand-ins for exactly this, and the tokens were the raw JSON
    // showing through.
    let mut lines = Face::of(card).lines(false);

    // What the engine will actually do, beside the words it was written
    // from: each trigger, ability and subroutine as the engine reads it
    // (`prose::describe_effect` over the DSL), and the printed clause the
    // author linked to it where there is one. This is the troubleshooting
    // view — a player who thinks the card is doing something its text does
    // not say can see both here, and an erratum that changes the text
    // shows up as a clause that no longer matches.
    let engine = engine_reading(card, registry);
    if !engine.is_empty() {
        lines.push(String::new());
        lines.push("Engine reads it as:".to_string());
        lines.extend(engine);
    }
    Modal::new(&card.title, &lines.join("\n"), "Esc to close")
}

impl Modal {
    pub fn new(title: &str, body: &str, footer: &str) -> Self {
        Self { title: title.to_string(), body: body.to_string(), footer: footer.to_string() }
    }
}

/// What the coaching panel shows during one lesson step — a projection of
/// `netrunner_core::tutorial::Step` plus the two UI facts it needs (which
/// step this is, and whether the escape hatch is open).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coaching {
    pub title: String,
    /// One-based, for display.
    pub step: usize,
    pub total: usize,
    pub prose: String,
    pub hint: Option<String>,
    /// Whether the step's filter matched at least one legal action. When
    /// it did not, the list falls back to every legal action and the panel
    /// says so — the lesson's gate can narrow the list, never empty it.
    pub gated: bool,
    /// The escape hatch is open: the player asked to see every legal
    /// action rather than the step's subset.
    pub showing_all: bool,
}

impl RenderableView for App {
    fn registry(&self) -> &CardRegistry {
        &self.registry
    }

    fn viewer(&self) -> Viewer {
        self.viewer
    }

    fn view(&self) -> Option<&ClientView> {
        self.view.as_ref()
    }

    fn selected(&self) -> usize {
        self.selected
    }

    fn legal_action_labels(&self) -> Vec<String> {
        let mut labels: Vec<String> =
            self.offered_actions().iter().map(|action| self.asks.label(action, offered_label(action, &self.registry, self.view.as_ref()))).collect();
        if let Some(view) = &self.view {
            labels.extend(self.breaks.iter().map(|route| route.label(view, &self.registry)));
        }
        labels
    }

    fn selected_action(&self) -> Option<PlayerAction> {
        self.offered_actions().get(self.selected).cloned()
    }

    fn action_log(&self) -> &[String] {
        &self.action_log
    }

    fn last_rejection(&self) -> Option<&str> {
        self.last_rejection.as_deref()
    }
    fn connection_notice(&self) -> Option<&str> {
        self.connection_notice.as_deref()
    }
    fn decision_clock(&self) -> Option<(Side, Duration)> {
        self.decision_clock.map(|(side, deadline)| (side, deadline.saturating_duration_since(Instant::now())))
    }
    fn modal(&self) -> Option<&Modal> {
        self.modal.as_ref()
    }
    fn card_picker(&self) -> Option<&CardPicker> {
        self.card_picker.as_ref()
    }
    /// The card that parked a decision names the pane: "Bigger Picture
    /// asks — choose one".
    fn actions_title(&self) -> Option<String> {
        self.view.as_ref().and_then(|view| crate::prose::decision_prompt(view, &self.registry))
    }
    fn notice(&self) -> Option<String> {
        let view = self.view.as_ref()?;
        let notices: Vec<String> = [remember_hint(view, &self.registry), run_pass_hint(self.run_pass, view)].into_iter().flatten().collect();
        (!notices.is_empty()).then(|| notices.join(" · "))
    }
}

/// The keys that answer a card's "you may" for good, when the prompt is
/// one: "y: always, n: never" (`netrunner_client::standing`). Both
/// terminal paths show it.
pub(crate) fn remember_hint(view: &ClientView, registry: &CardRegistry) -> Option<String> {
    let prompt = optional_trigger(view, registry)?;
    let keys: Vec<&str> = prompt
        .offered()
        .into_iter()
        .map(|answer| match answer {
            Answer::Always => "y: always",
            Answer::Never => "n: never",
        })
        .collect();
    Some(keys.join(", "))
}

/// The key for the Corp's "no more this run", when it is on or on offer
/// (`netrunner_client::run_pass`). Both terminal paths show it.
pub(crate) fn run_pass_hint(run_pass: RunPass, view: &ClientView) -> Option<String> {
    if run_pass.is_on() {
        Some("passing the rest of this run — w to stop".to_string())
    } else {
        run_pass.offered(view).then(|| "w: pass the rest of this run".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The card modal is the printed card, not the DSL: type line, the
    /// numbers, the text with its line breaks and symbols, the flavour.
    #[test]
    fn a_card_modal_is_the_printed_card() {
        let registry = netrunner_client::decks::sample_deck_registry();
        let modal = card_modal(&CardId("hedge_fund".to_string()), &registry);
        assert_eq!(modal.title, "Hedge Fund");
        assert!(modal.body.contains("Operation"), "{}", modal.body);
        assert!(modal.body.contains("Cost 5"), "{}", modal.body);
        // `Symbol::fallback`, not the `[credit]` token the card JSON
        // holds: a terminal has a stand-in for every printed symbol, and
        // the token was the raw data showing through.
        assert!(modal.body.contains("Gain 9¢"), "{}", modal.body);
        assert_eq!(modal.footer, "Esc to close");
        let unknown = card_modal(&CardId("no_such_card".to_string()), &registry);
        assert!(unknown.body.contains("not in the registry"));
    }

    use netrunner_bots::RandomAgent;
    use netrunner_core::rules::GameState;
    use netrunner_server::{MatchSession, PlayerSlot};

    fn setup() -> (GameState, CardRegistry) {
        let registry = netrunner_client::decks::sample_deck_registry();
        let corp_deck = netrunner_core::decks::by_id("discretion_advised").expect("built-in deck").to_deck();
        let runner_deck = netrunner_core::decks::by_id("stolen_goods").expect("built-in deck").to_deck();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 1).expect("legal decks set up cleanly");
        (state, registry)
    }

    fn spawn_session(state: GameState, registry: CardRegistry, corp_slot: PlayerSlot, runner_slot: PlayerSlot) {
        let session = MatchSession::new(state, registry, corp_slot, runner_slot);
        tokio::spawn(session.run());
    }

    #[tokio::test]
    async fn human_seat_receives_its_own_view_and_can_submit_an_action() {
        let (state, registry) = setup();
        let (server_tx, app_rx) = mpsc::unbounded_channel();
        let (app_tx, server_rx) = mpsc::unbounded_channel();
        let corp_slot = PlayerSlot::Channel { tx: server_tx, rx: server_rx };
        let runner_slot = PlayerSlot::Bot(Box::new(RandomAgent::new(2)));

        spawn_session(state, registry.clone(), corp_slot, runner_slot);

        let mut app = App::new(registry, Side::Corp, app_tx, app_rx);
        // Give the background task a moment to deliver the initial view.
        for _ in 0..50 {
            if app.view.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            app.drain_messages();
        }

        let (initial_phase, initial_credits) = {
            let view = app.view.as_ref().expect("initial view delivered");
            assert_eq!(view.viewer.side(), Some(Side::Corp));
            (view.phase, view.corp.credits)
        };
        assert!(!app.legal_actions().is_empty());
        assert!(app.legal_actions().iter().all(|action| matches!(action, PlayerAction::KeepHand | PlayerAction::TakeMulligan)));

        app.handle_key(KeyEvent::from(KeyCode::Enter));
        for _ in 0..50 {
            app.drain_messages();
            if app.view.as_ref().is_some_and(|v| v.phase != initial_phase || v.corp.credits != initial_credits) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }
}

/// `App`'s half of reconnection: it follows the link, holds submissions
/// while it is down, and carries on down the same channels when it is up.
#[cfg(test)]
mod connection_tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use netrunner_core::rules::{GamePhase, GameState};
    use netrunner_core::view::build_client_view;

    fn app_with_channels() -> (App, mpsc::UnboundedSender<ServerMessage>, mpsc::UnboundedReceiver<ClientMessage>) {
        let registry = netrunner_client::decks::sample_deck_registry();
        let (server_tx, rx) = mpsc::unbounded_channel();
        let (tx, client_rx) = mpsc::unbounded_channel();
        (App::new(registry, Side::Corp, tx, rx), server_tx, client_rx)
    }

    fn a_view(registry: &CardRegistry) -> ClientView {
        let (corp_deck, runner_deck) = netrunner_server::fixtures::sample_decks();
        let (mut state, _) = GameState::setup(&corp_deck, &runner_deck, registry, 1).unwrap();
        state.phase = GamePhase::Action(Side::Corp);
        build_client_view(&state, registry, Side::Corp)
    }

    /// A view that lists only a pass is answered with the pass as it
    /// arrives; a pass beside anything else waits for a key.
    #[test]
    fn a_lone_pass_is_sent_without_a_key_and_a_pass_with_company_is_not() {
        let (mut app, server_tx, mut client_rx) = app_with_channels();
        let pass = PlayerAction::PassPriority { side: Side::Corp };
        let mut view = a_view(&app.registry);
        view.legal_actions = vec![pass.clone(), PlayerAction::EndTurn];
        server_tx.send(ServerMessage::StateUpdate(Box::new(view.clone()))).unwrap();
        app.drain_messages();
        assert!(client_rx.try_recv().is_err(), "a real choice is the person's");

        view.legal_actions = vec![pass.clone()];
        server_tx.send(ServerMessage::StateUpdate(Box::new(view))).unwrap();
        app.drain_messages();
        assert!(matches!(client_rx.try_recv(), Ok(ClientMessage::SubmitAction(sent)) if sent == pass));
        assert!(client_rx.try_recv().is_err(), "sent once");
    }

    /// The Corp's first window of a real run, from a local game played
    /// by its first legal action.
    fn a_run_window() -> ClientView {
        use netrunner_client::play::{LocalMatchSpec, MatchHandle, MatchMessage};
        let registry = std::sync::Arc::new(netrunner_client::decks::sample_deck_registry());
        let corp = netrunner_core::decks::by_id("discretion_advised").unwrap();
        let runner = netrunner_core::decks::by_id("stolen_goods").unwrap();
        let spec = LocalMatchSpec { registry, corp, runner, human: Side::Corp, level: netrunner_bots::Level::Novice, style: None, seed: 3, rules: Default::default(), record: None };
        let mut handle = MatchHandle::start_local(spec).unwrap();
        loop {
            match handle.wait().expect("a run comes before the end") {
                MatchMessage::Awaiting { view } if RunPass::default().offered(&view) => return *view,
                MatchMessage::Awaiting { view } => handle.submit(view.legal_actions[0].clone()).unwrap(),
                MatchMessage::Ended { .. } | MatchMessage::Stalled { .. } => panic!("no run"),
                _ => {}
            }
        }
    }

    /// `w` in a run's window sends its pass, the next window of the run
    /// is passed without a key, and the run's end turns it off; the
    /// notice names the key either way.
    #[test]
    fn w_passes_the_rest_of_a_run() {
        let (mut app, server_tx, mut client_rx) = app_with_channels();
        let mut window = a_run_window();
        let pass = PlayerAction::PassPriority { side: Side::Corp };
        // Company for the pass, so the lone pass does not take it.
        window.legal_actions.push(PlayerAction::EndTurn);
        server_tx.send(ServerMessage::StateUpdate(Box::new(window.clone()))).unwrap();
        app.drain_messages();
        assert!(client_rx.try_recv().is_err(), "a window beside a choice waits");
        assert_eq!(RenderableView::notice(&app).as_deref(), Some("w: pass the rest of this run"));

        app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE));
        assert!(matches!(client_rx.try_recv(), Ok(ClientMessage::SubmitAction(sent)) if sent == pass));
        server_tx.send(ServerMessage::StateUpdate(Box::new(window.clone()))).unwrap();
        app.drain_messages();
        assert!(matches!(client_rx.try_recv(), Ok(ClientMessage::SubmitAction(sent)) if sent == pass), "the next window goes by itself");
        assert_eq!(RenderableView::notice(&app).as_deref(), Some("passing the rest of this run — w to stop"));

        let mut after = window.clone();
        after.active_run = None;
        server_tx.send(ServerMessage::StateUpdate(Box::new(after))).unwrap();
        server_tx.send(ServerMessage::StateUpdate(Box::new(window))).unwrap();
        app.drain_messages();
        assert!(client_rx.try_recv().is_err(), "the next run asks again");
    }

    /// A card's "you may" answered for good is answered as it arrives,
    /// and one never answered waits for a key, which the notice names.
    #[test]
    fn a_remembered_answer_is_sent_without_a_key() {
        use netrunner_client::standing::{optional_trigger, PromptKey};
        use netrunner_core::dsl::{CardId, Effect};
        use netrunner_core::rules::{PendingChoiceResume, PendingDecision};

        let (mut app, server_tx, mut client_rx) = app_with_channels();
        app.registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut app.registry);
        let mitra = CardId("mitra_aman".to_string());
        let mut texts = Vec::new();
        for trigger in &app.registry.get(&mitra).unwrap().triggers {
            for effect in &trigger.effects {
                effect.for_each_effect(&mut |effect| {
                    if let Effect::PresentChoice { texts: printed, .. } = effect
                        && printed.len() == 2
                    {
                        texts.clone_from(printed);
                    }
                });
            }
        }
        let mut view = a_view(&app.registry);
        view.pending_decision = Some(PendingDecision::ChooseEffect {
            chooser: Side::Corp,
            options: vec![Effect::Sequence(Vec::new()), Effect::Sequence(Vec::new())],
            option_texts: texts.clone(),
            source_card: Some(mitra.clone()),
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });
        view.legal_actions = (0..2).map(|option_index| PlayerAction::ResolvePendingChoice { option_index }).collect();
        assert!(optional_trigger(&view, &app.registry).is_some());

        server_tx.send(ServerMessage::StateUpdate(Box::new(view.clone()))).unwrap();
        app.drain_messages();
        assert!(client_rx.try_recv().is_err(), "never answered: the person's");
        assert_eq!(RenderableView::notice(&app).as_deref(), Some("y: always, n: never"));

        app.answers.set(PromptKey { card: mitra, clauses: vec![texts[0].clone()] }, Some(Answer::Never));
        server_tx.send(ServerMessage::StateUpdate(Box::new(view))).unwrap();
        app.drain_messages();
        assert!(matches!(client_rx.try_recv(), Ok(ClientMessage::SubmitAction(PlayerAction::ResolvePendingChoice { option_index: 1 }))));
        assert!(client_rx.try_recv().is_err(), "sent once");
    }

    #[test]
    fn a_closed_channel_is_a_lost_connection_not_an_empty_one() {
        let (mut app, server_tx, _client_rx) = app_with_channels();
        app.drain_messages();
        assert!(!app.connection_lost, "nothing yet is not a disconnect");
        drop(server_tx);
        app.drain_messages();
        assert!(app.connection_lost);
    }

    #[test]
    fn nothing_is_submitted_while_the_link_is_down_and_the_same_channel_carries_on() {
        let (mut app, server_tx, mut client_rx) = app_with_channels();
        let (link_tx, link) = watch::channel(Link::Up);
        app.follow_link(link);
        let view = a_view(&app.registry);
        server_tx.send(ServerMessage::StateUpdate(Box::new(view.clone()))).unwrap();
        app.drain_messages();
        let lost_at = Instant::now();
        link_tx.send_replace(Link::Reconnecting { attempts: 1, since: lost_at });
        app.drain_messages();
        assert!(app.connection_lost);
        assert!(app.view.is_some(), "the last view stays on screen");
        assert!(app.connection_notice.as_deref().is_some_and(|notice| notice.contains("reconnecting (attempt 1")), "{:?}", app.connection_notice);

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(client_rx.try_recv().is_err(), "an action chosen from a possibly-stale view is not sent");

        link_tx.send_replace(Link::Up);
        server_tx.send(ServerMessage::StateUpdate(Box::new(view))).unwrap();
        app.drain_messages();
        assert!(!app.connection_lost);
        assert_eq!(app.connection_notice, None);
        assert!(app.action_log.last().unwrap().contains("reconnected"));

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(client_rx.try_recv(), Ok(ClientMessage::SubmitAction(_))), "submissions go down the same channel");
    }

    #[test]
    fn a_link_down_for_good_says_why() {
        let (mut app, _server_tx, _client_rx) = app_with_channels();
        let (link_tx, link) = watch::channel(Link::Up);
        app.follow_link(link);
        link_tx.send_replace(Link::Down(netrunner_client::connection::ConnectionError::Rejected("the match is over".into())));
        app.drain_messages();
        assert!(app.connection_lost);
        assert!(app.connection_notice.as_deref().is_some_and(|notice| notice.contains("the match is over")));
    }
}
