pub mod layout;

use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use netrunner_core::cards::CardRegistry;
use netrunner_core::decks::DeckFile;
use netrunner_core::dsl::{CardId, CounterKind};
use netrunner_bots::Personality;
use netrunner_core::rules::Viewer;
use netrunner_core::rules::{
    get_action_mask, ActionSpace, DeckOrder, GamePhase, GameState, PlayerAction, RunPhase, ServerId, Side,
};
use netrunner_core::tutorial::Lesson;
use netrunner_core::view::{ClientView, ServerView};
use netrunner_session::{GameEndReason, LessonSession, LessonStep, Seat, Session, SessionStep, StallReason, SubmitError};

use crate::app::{describe_action, explain_action, push_log_line, App, Coaching, Modal, RenderableView};
use crate::bots;
use crate::config::{BotKind, Config, Mode};
use crate::decks;
use crate::remote;
use crate::replay::Replay;

const CORP_MAX_CLICKS: u32 = 3;
const RUNNER_MAX_CLICKS: u32 = 4;

/// Search budget for a `mcts`/`puct` opponent in interactive play — the
/// agents' own defaults. `--simulations` is a headless flag; a human
/// opponent gets the full-strength bot.
const DEFAULT_SIMULATIONS: usize = 64;

pub async fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    match config.mode {
        Mode::Local => run_local(config),
        Mode::Remote => run_remote(config).await,
    }
}

async fn run_remote(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    // The wire protocol never transmits a `CardRegistry`, so the client
    // builds one locally to resolve card titles. It needs no agreement
    // with the host beyond the embedded pool: the daemon deals published
    // sample decks, whose cards are exactly `register_playable_cards`.
    // While it dealt a filler-padded fixture, this had to be a hand-kept
    // copy of the host's synthetic card ids.
    let registry = decks::sample_deck_registry();
    let joined = match config.spectate {
        Some(match_id) => remote::spectate_remote(&config.server, match_id).await?,
        None => remote::connect_remote(&config.server, config.side.map(Into::into), config.room.clone()).await?,
    };
    let session_token = joined.session_token;
    let mut app = App::new(registry, joined.viewer, joined.tx, joined.rx);

    let mut terminal = ratatui::init();
    let result = run_event_loop(&mut terminal, &mut app, &config.server, session_token);
    ratatui::restore();
    result
}

/// Local, offline human-vs-bot play, pumping a `netrunner_session::Session`
/// directly — no `MatchSession`, no channel, no background task.
///
/// **The human seat is `Seat::External` and only ever sees the masked
/// `ClientView` that `SessionStep::Awaiting` hands over.** That is the
/// point of Phase 1.5: this path used to run through a `PlayerDriver`
/// callback that received the raw `&GameState` and called
/// `build_client_view` itself before rendering — masking by client
/// convention rather than by interface. It is now structural, exactly as it
/// already was for a channel-backed seat.
///
/// Inverting the control flow (the TUI owns the loop; the session is
/// pulled) also removes the `Rc<RefCell<_>>` aliasing the old callback
/// needed, lets I/O errors propagate with `?` instead of `.expect`, drops
/// the `process::exit` that quitting mid-prompt required, and redraws the
/// board after *bot* moves rather than only at human decision points.
fn run_local(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let human_side = match (config.corp, config.runner) {
        (crate::config::BotKind::Human, crate::config::BotKind::Human) => {
            return Err("interactive mode requires exactly one human-controlled side (both --corp and --runner are human)".into());
        }
        (crate::config::BotKind::Human, _) => Side::Corp,
        (_, crate::config::BotKind::Human) => Side::Runner,
        _ => return Err("interactive mode requires exactly one human-controlled side (neither --corp nor --runner is human)".into()),
    };

    let registry = decks::sample_deck_registry();
    let decks_dir = crate::deck_store::resolve_decks_dir(config.decks_dir.as_deref())?;
    let (corp_deck, runner_deck) =
        decks::decks_for_match(&decks_dir, &config.corp_deck, &config.runner_deck, &registry, config.format.into())?;
    let seed = config.seed.unwrap_or_else(rand::random);
    let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;

    let bot_side = human_side.other();
    let bot_kind = if human_side == Side::Corp { config.runner } else { config.corp };
    let (bot_seat, mut indexed_bot) = build_bot_seat(bot_kind, bot_side, seed.wrapping_add(1), &config.model, config.personality_for(bot_side))?;

    let (corp_seat, runner_seat) = match human_side {
        Side::Corp => (Seat::External, bot_seat),
        Side::Runner => (bot_seat, Seat::External),
    };
    let mut session = Session::new(state, registry.clone(), corp_seat, runner_seat);

    let mut ui = LocalUiState::new(registry, human_side);
    let mut terminal = ratatui::init();
    let result = drive_local(&mut terminal, &mut session, &mut ui, indexed_bot.as_mut(), human_side);
    ratatui::restore();
    result
}

/// How a lesson (or a run of lessons) ended, for the caller to decide
/// whether to go on to the next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LessonOutcome {
    /// Every step advanced and the player dismissed the outro.
    Completed,
    /// The player quit, or the match ended before the lesson did.
    Stopped,
}

/// Plays `lessons` in order in one terminal session, stopping at the first
/// the player does not complete. Returns `Completed` only if every lesson
/// was — which is what lets `learn track` hand a graduate the starter game.
pub fn run_lessons(lessons: &[Lesson], registry: &CardRegistry, seed: u64) -> Result<LessonOutcome, Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();
    let mut outcome = LessonOutcome::Completed;
    for lesson in lessons {
        match run_lesson(&mut terminal, lesson, registry, seed) {
            Ok(LessonOutcome::Completed) => continue,
            Ok(LessonOutcome::Stopped) => {
                outcome = LessonOutcome::Stopped;
                break;
            }
            Err(error) => {
                ratatui::restore();
                return Err(error);
            }
        }
    }
    ratatui::restore();
    Ok(outcome)
}

/// Steps through a recorded match in one terminal session. Nothing is
/// submitted anywhere: the keys move a cursor over positions `Replay`
/// has already computed, and `s` swaps the chair the board is seen from.
pub fn run_replay(mut replay: Replay) -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();
    let result = drive_replay(&mut terminal, &mut replay);
    ratatui::restore();
    result
}

fn drive_replay(terminal: &mut ratatui::DefaultTerminal, replay: &mut Replay) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|frame| draw_frame(frame, replay, None))?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && !replay_key(replay, key.code)
        {
            return Ok(());
        }
    }
}

/// One keypress on the replay; `false` means quit. Separate from the loop
/// so the bindings can be tested without a terminal.
fn replay_key(replay: &mut Replay, key: KeyCode) -> bool {
    match key {
        KeyCode::Char('q') | KeyCode::Esc => return false,
        KeyCode::Right | KeyCode::Down | KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Char('l') | KeyCode::Char('j') => {
            replay.step_forward(1)
        }
        KeyCode::Left | KeyCode::Up | KeyCode::Char('h') | KeyCode::Char('k') => replay.step_back(1),
        KeyCode::PageDown => replay.step_forward(10),
        KeyCode::PageUp => replay.step_back(10),
        KeyCode::Home | KeyCode::Char('g') => replay.seek(0),
        KeyCode::End | KeyCode::Char('G') => replay.seek(usize::MAX),
        KeyCode::Char('s') => replay.set_side(replay.side().other()),
        _ => {}
    }
    true
}

/// The unguided starter game (ROADMAP Phase 1.75 §8's graduation): the two
/// preset decks under their own category's rules — 6 points for the
/// starter lists, Standard 7 with the boosters — against the heuristic
/// bot. The only path that plays a starter deck at 6 points; ordinary
/// `--corp-deck the_syndicate_starter` play uses Standard rules, because a
/// saved deck carries no category and guessing one from a name would be
/// the kind of client-side rule the crate map forbids.
pub fn run_starter_game(human_side: Side, corp: &DeckFile, runner: &DeckFile, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let registry = decks::sample_deck_registry();
    let seed = config.seed.unwrap_or_else(rand::random);
    let rules = corp.category.match_rules();
    let (state, _events) = GameState::setup_with(&corp.to_deck(), &runner.to_deck(), &registry, seed, rules, DeckOrder::Shuffled)?;
    let bot = bots::make_agent(BotKind::Heuristic, human_side.other(), seed.wrapping_add(1), DEFAULT_SIMULATIONS, config.personality_for(human_side.other()))
        .expect("the heuristic always has a BotAgent form");
    let (corp_seat, runner_seat) = match human_side {
        Side::Corp => (Seat::External, Seat::Agent(bot)),
        Side::Runner => (Seat::Agent(bot), Seat::External),
    };
    let mut session = Session::new(state, registry.clone(), corp_seat, runner_seat);
    let mut ui = LocalUiState::new(registry, human_side);
    let mut terminal = ratatui::init();
    let result = drive_local(&mut terminal, &mut session, &mut ui, None, human_side);
    ratatui::restore();
    result
}

/// One lesson: intro modal, gated prompts with coaching, outro modal.
///
/// The same pull loop as `drive_local`, pumping a `LessonSession` instead
/// of a bare `Session`. The lesson never touches the engine here: what the
/// player sees is `LessonStep::Prompt`'s `allowed` — a filter over
/// `view.legal_actions` computed in `netrunner_core::tutorial` — and what
/// they submit is a `PlayerAction` taken from that list or, with the
/// escape hatch open, from the full one. Nothing in this module can make
/// an action legal.
fn run_lesson(
    terminal: &mut ratatui::DefaultTerminal,
    lesson: &Lesson,
    registry: &CardRegistry,
    seed: u64,
) -> Result<LessonOutcome, Box<dyn std::error::Error>> {
    let mut session = LessonSession::start(lesson.clone(), registry.clone(), seed)?;
    let mut ui = LocalUiState::new(registry.clone(), lesson.side);
    ui.modal = Some(Modal::new(&lesson.title, &lesson.intro, "Enter to begin"));
    loop {
        let step = session.step()?;
        for entry in session.drain_log() {
            push_log_line(&mut ui.action_log, &entry, &ui.registry, ui.view.as_ref());
        }

        match step {
            LessonStep::Prompt { view, allowed, step, total } => {
                let live = &lesson.steps[step];
                ui.begin_gated_decision(
                    *view,
                    allowed,
                    Coaching {
                        title: lesson.title.clone(),
                        step: step + 1,
                        total,
                        prose: live.prose.clone(),
                        hint: live.hint.clone(),
                        gated: true,
                        showing_all: false,
                    },
                );
                if prompt_human(terminal, &mut ui, |action| session.submit(action))? {
                    return Ok(LessonOutcome::Stopped);
                }
            }
            LessonStep::Complete { view } => {
                ui.finish(*view);
                ui.coaching = None;
                ui.modal = Some(Modal::new(&format!("{} — complete", lesson.title), &lesson.outro, "Enter to continue, q to quit"));
                return Ok(if hold_modal(terminal, &ui)? { LessonOutcome::Stopped } else { LessonOutcome::Completed });
            }
            LessonStep::Ended { winner, reason } => {
                ui.finish(session.session().view_for(lesson.side));
                ui.coaching = None;
                show_game_over(terminal, &ui, winner, reason)?;
                return Ok(LessonOutcome::Stopped);
            }
            LessonStep::Stalled(reason) => return Err(stall_message(reason).into()),
        }
    }
}

/// Draws until the open modal is dismissed. Returns `true` if the player
/// quit instead of dismissing it.
fn hold_modal(terminal: &mut ratatui::DefaultTerminal, ui: &LocalUiState) -> Result<bool, Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|frame| draw_frame(frame, ui, None))?;
        if event::poll(Duration::from_millis(100))?
            && let Ok(Event::Key(key)) = event::read()
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
                KeyCode::Enter | KeyCode::Char(' ') => return Ok(false),
                _ => {}
            }
        }
    }
}

fn stall_message(reason: StallReason) -> String {
    match reason {
        StallReason::BudgetExhausted => "match ended without reaching GameOver (step budget exhausted)".to_string(),
        StallReason::NoCurrentActor => "match stalled: no side has a decision pending".to_string(),
        StallReason::NoLegalActions { side } => format!("match deadlocked: {side:?} has priority but no legal action"),
    }
}

/// Splits a `BotKind` into the session seat it becomes and, for the one
/// kind that cannot be a `Seat::Agent`, the agent this module has to pump
/// itself. Same shape as `netrunner_server::PlayerSlot::split`.
///
/// Every scripted kind is a plain `Seat::Agent` over a masked view.
/// `BotKind::Onnx` is the exception: `OnnxPolicyEvaluator` evaluates a whole
/// `GameState` against a fixed `ActionSpace`, so it has no `BotAgent` form
/// and is pumped through the index-based adapter like the RL path. Giving it
/// a view-based form would delete this branch — see `bots::make_agent`.
fn build_bot_seat(
    kind: crate::config::BotKind,
    side: Side,
    seed: u64,
    model: &str,
    personality: Personality,
) -> Result<(Seat, Option<Box<dyn netrunner_bots::Agent>>), String> {
    match kind {
        crate::config::BotKind::Onnx => Ok((Seat::External, Some(bots::make_driver(kind, side, seed, DEFAULT_SIMULATIONS, model, personality)?))),
        _ => {
            let agent = bots::make_agent_with_model(kind, side, seed, DEFAULT_SIMULATIONS, model, personality)?
                .ok_or_else(|| "interactive mode needs a bot on the non-human side".to_string())?;
            Ok((Seat::Agent(agent), None))
        }
    }
}

/// The pull loop: step the session, render whatever it reports, and block
/// on keyboard input only when the *human* seat is the one being asked.
fn drive_local(
    terminal: &mut ratatui::DefaultTerminal,
    session: &mut Session,
    ui: &mut LocalUiState,
    mut indexed_bot: Option<&mut Box<dyn netrunner_bots::Agent>>,
    human_side: Side,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        // Pumped one `step` at a time rather than through `run`, which
        // swallows the bot seat's `Applied` steps: each log line is the
        // human's *masked* copy of that action, and `last_entry_for` reads
        // concealment off the state the action left, so it has to be taken
        // before the next one resolves. Diffing the history after `run`
        // was the old idiom; it also never logged the human's own action,
        // since the mark was taken after `submit`.
        let step = loop {
            match session.step() {
                SessionStep::Applied { .. } => log_last(session, ui, human_side),
                other => break other,
            }
        };

        match step {
            SessionStep::Awaiting { side, view } if side == human_side => {
                ui.begin_decision(*view);
                if prompt_human(terminal, ui, |action| session.submit(action))? {
                    return Ok(());
                }
                log_last(session, ui, human_side);
            }
            // The ONNX bot seat: index-based, so the session cannot
            // resolve it and hands it back here instead.
            SessionStep::Awaiting { side, .. } => {
                let agent = indexed_bot
                    .as_mut()
                    .expect("only the index-based ONNX bot seat is External, and it always has an agent");
                let mask = get_action_mask(session.state(), session.registry());
                let index = agent.select_action(session.state(), session.registry(), &mask);
                let action = ActionSpace::action_at(session.state(), index)
                    .ok_or_else(|| format!("the {side:?} policy chose index {index}, which decodes to no action"))?;
                session.submit(action).map_err(|error| format!("the {side:?} policy chose an action the engine rejected: {error}"))?;
                log_last(session, ui, human_side);
            }
            SessionStep::Ended { winner, reason } => {
                ui.finish(session.view_for(human_side));
                return show_game_over(terminal, ui, winner, reason);
            }
            SessionStep::Stalled(reason) => return Err(stall_message(reason).into()),
            SessionStep::Applied { .. } => unreachable!("the inner loop only breaks once it can no longer apply"),
        }
    }
}

/// Appends the action just applied to the human's log, as the human may
/// see it.
fn log_last(session: &Session, ui: &mut LocalUiState, human_side: Side) {
    if let Some(entry) = session.last_entry_for(human_side) {
        push_log_line(&mut ui.action_log, &entry, &ui.registry, ui.view.as_ref());
    }
}

/// Renders the current position and blocks on keyboard input until the
/// human submits a choice the engine accepts. Returns `true` if they quit
/// instead.
///
/// The human seat works straight off `view.legal_actions` (or, under a
/// lesson, the gated subset of it), with no `ActionSpace` round trip.
/// `submit` is whichever session is being pumped — a bare `Session` or a
/// `LessonSession` — so this one prompt serves both paths.
///
/// A rules rejection is shown, not fatal: it used to propagate with `?`
/// and drop the player out of the TUI, which was tolerable for a list
/// built from `legal_actions` (the engine agrees with it by construction)
/// and indefensible in a tutorial. The rejection line clears on the next
/// decision.
///
/// Key routing, in priority order: an open modal swallows everything but
/// its dismissal; `a` toggles the lesson escape hatch; then the list.
fn prompt_human(
    terminal: &mut ratatui::DefaultTerminal,
    ui: &mut LocalUiState,
    mut submit: impl FnMut(PlayerAction) -> Result<(), SubmitError>,
) -> Result<bool, Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|frame| draw_frame(frame, ui, None))?;

        if event::poll(Duration::from_millis(100))?
            && let Ok(Event::Key(key)) = event::read()
        {
            if ui.modal.is_some() {
                match key.code {
                    KeyCode::Char('q') => return Ok(true),
                    KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Esc => ui.modal = None,
                    _ => {}
                }
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
                KeyCode::Up | KeyCode::Char('k') => ui.move_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => ui.move_selection(1),
                KeyCode::Char('a') => ui.toggle_show_all(),
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some(action) = ui.selected_action() {
                        match submit(action) {
                            Ok(()) => return Ok(false),
                            // `Display`, not `Debug` — see the same
                            // change in `MatchSession`'s reject arm.
                            Err(SubmitError::Rules(error)) => ui.last_rejection = Some(error.to_string()),
                            Err(error) => return Err(error.into()),
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Holds the end-of-match summary on screen until the player dismisses it.
fn show_game_over(
    terminal: &mut ratatui::DefaultTerminal,
    ui: &LocalUiState,
    winner: Side,
    reason: GameEndReason,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|frame| draw_frame(frame, ui, Some((winner, reason))))?;
        if event::poll(Duration::from_millis(100))?
            && let Ok(Event::Key(key)) = event::read()
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        {
            return Ok(());
        }
    }
}

/// Local-play TUI state: the same renderable data `App` carries (registry,
/// human side, current masked `ClientView`, selection, action log).
///
/// It no longer keeps a `legal_actions_cache` of `(index, PlayerAction)`
/// pairs, nor a `last_events` copy. The human seat submits a
/// `PlayerAction` straight from `view.legal_actions` — no `ActionSpace`
/// round trip — and `SessionStep::Ended` already carries the classified
/// reason that `last_events` existed to reconstruct.
struct LocalUiState {
    registry: CardRegistry,
    human_side: Side,
    view: Option<ClientView>,
    selected: usize,
    action_log: Vec<String>,
    /// The live lesson step's filter over `view.legal_actions`, as handed
    /// over by `LessonStep::Prompt`. Empty outside a lesson.
    ///
    /// **A lesson step narrows `view.legal_actions`; it never widens
    /// them.** This vector only ever holds elements of the current view's
    /// legal list — it is populated from `LessonProgress::allowed`, a
    /// filter — so presenting it is a UI affordance like sorting: it
    /// cannot make an illegal action legal, and nothing here calls
    /// `apply_action` or re-derives legality (ROADMAP Phase 1.75 §6).
    allowed: Vec<PlayerAction>,
    /// The escape hatch: `a` shows every legal action regardless of the
    /// step's filter, so a step whose predicate matches nothing — the
    /// deadlock shape, reintroduced at the UI layer — can never strand the
    /// player.
    show_all: bool,
    coaching: Option<Coaching>,
    modal: Option<Modal>,
    last_rejection: Option<String>,
}

impl LocalUiState {
    fn new(registry: CardRegistry, human_side: Side) -> Self {
        Self {
            registry,
            human_side,
            view: None,
            selected: 0,
            action_log: Vec::new(),
            allowed: Vec::new(),
            show_all: false,
            coaching: None,
            modal: None,
            last_rejection: None,
        }
    }

    /// Installs the view the session just handed us for a fresh human
    /// decision, resetting the highlight and the rejection notice.
    fn begin_decision(&mut self, view: ClientView) {
        self.view = Some(view);
        self.selected = 0;
        self.allowed.clear();
        self.last_rejection = None;
    }

    /// `begin_decision` under a lesson: the step's filtered list and its
    /// coaching. An empty filter falls back to the full list (and says so
    /// in the panel) rather than offering nothing.
    fn begin_gated_decision(&mut self, view: ClientView, allowed: Vec<PlayerAction>, mut coaching: Coaching) {
        debug_assert!(allowed.iter().all(|action| view.legal_actions.contains(action)), "a lesson may only narrow the legal list");
        self.begin_decision(view);
        coaching.gated = !allowed.is_empty();
        coaching.showing_all = self.show_all;
        self.allowed = allowed;
        self.coaching = Some(coaching);
    }

    /// Final board for the game-over screen: no decision is pending, so
    /// nothing is selectable.
    fn finish(&mut self, view: ClientView) {
        self.view = Some(view);
        self.selected = 0;
        self.allowed.clear();
    }

    /// The actions the list shows: the lesson's subset when one applies
    /// and the escape hatch is closed, otherwise every legal action.
    fn offered_actions(&self) -> &[PlayerAction] {
        let legal = self.view.as_ref().map_or(&[][..], |view| view.legal_actions.as_slice());
        if self.coaching.is_some() && !self.show_all && !self.allowed.is_empty() { &self.allowed } else { legal }
    }

    fn selected_action(&self) -> Option<PlayerAction> {
        self.offered_actions().get(self.selected).cloned()
    }

    fn toggle_show_all(&mut self) {
        if self.coaching.is_none() {
            return;
        }
        self.show_all = !self.show_all;
        self.selected = 0;
        if let Some(coaching) = &mut self.coaching {
            coaching.showing_all = self.show_all;
        }
    }

    fn move_selection(&mut self, delta: i32) {
        let len = self.offered_actions().len();
        if len == 0 {
            return;
        }
        let next = (self.selected as i32 + delta).rem_euclid(len as i32);
        self.selected = next as usize;
    }
}

impl RenderableView for LocalUiState {
    fn registry(&self) -> &CardRegistry {
        &self.registry
    }

    fn viewer(&self) -> Viewer {
        Viewer::Player(self.human_side)
    }

    fn view(&self) -> Option<&ClientView> {
        self.view.as_ref()
    }

    fn selected(&self) -> usize {
        self.selected
    }

    fn legal_action_labels(&self) -> Vec<String> {
        self.offered_actions().iter().map(|action| describe_action(action, &self.registry, self.view.as_ref())).collect()
    }

    fn selected_action(&self) -> Option<PlayerAction> {
        LocalUiState::selected_action(self)
    }

    fn action_log(&self) -> &[String] {
        &self.action_log
    }

    fn last_rejection(&self) -> Option<&str> {
        self.last_rejection.as_deref()
    }

    fn coaching(&self) -> Option<&Coaching> {
        self.coaching.as_ref()
    }

    fn modal(&self) -> Option<&Modal> {
        self.modal.as_ref()
    }
}

/// The remote render loop. A lost connection is handled *here*, between
/// frames, rather than inside `App` or `remote`: the board keeps drawing
/// with the last view and the reconnect notice, and `q` still quits, while
/// `Reconnector` makes one bounded attempt per tick. A game that has
/// already ended is not resumed — the `GameEnded` is on screen and the
/// server has dropped the seat's ticket anyway. A spectator has no token
/// and is not resumed either: it can spectate again from the command line.
fn run_event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    server_url: &str,
    session_token: Option<uuid::Uuid>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reconnector: Option<remote::Reconnector> = None;
    while !app.should_quit {
        app.drain_messages();
        if app.connection_lost && !app.is_game_over() {
            match session_token {
                Some(session_token) => {
                    let attempt = reconnector.get_or_insert_with(|| remote::Reconnector::new(server_url.to_string(), session_token));
                    match attempt.try_resume()? {
                        Some(joined) => {
                            app.reconnected(joined.tx, joined.rx);
                            reconnector = None;
                        }
                        None => app.connection_notice = Some(attempt.status_line()),
                    }
                }
                None => app.connection_notice = Some("Connection lost — spectate again to rejoin. Press q to quit.".to_string()),
            }
        }
        terminal.draw(|frame| draw_frame(frame, app, app.game_ended))?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            app.handle_key(key);
        }
    }
    Ok(())
}

/// One renderer for both paths. The remote path used to fall back to the
/// three-region `build_layout` purely because it had no log to show; now
/// that `ServerMessage::ActionLog` feeds `App::action_log`, both sides
/// render the same four regions.
fn draw_frame(frame: &mut Frame, ui: &impl RenderableView, game_over: Option<(Side, GameEndReason)>) {
    let regions = layout::build_layout(frame.area(), ui.coaching().is_some());
    draw_header(frame, regions.header, ui);
    draw_board(frame, regions.board, ui);
    if let Some(coach) = regions.coach {
        draw_coach(frame, coach, ui);
    }
    draw_actions(frame, regions.actions, ui);
    draw_action_log(frame, regions.log, ui.action_log());
    if let Some((winner, reason)) = game_over {
        draw_modal(frame, &Modal::new("Game over", &format!("{winner:?} wins! ({reason:?})"), "Press q to quit."));
    } else if let Some(modal) = ui.modal() {
        draw_modal(frame, modal);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &impl RenderableView) {
    if let Some(notice) = app.connection_notice() {
        frame.render_widget(Paragraph::new(notice.to_string()).style(Style::default().fg(Color::Red)), area);
        return;
    }
    let Some(view) = app.view() else {
        frame.render_widget(Paragraph::new("Connecting..."), area);
        return;
    };
    let phase_label = match view.phase {
        GamePhase::Mulligan(side) => format!("Mulligan ({side:?})"),
        GamePhase::StartOfTurn(side) => format!("Start of turn ({side:?})"),
        GamePhase::Action(side) => format!("Action ({side:?})"),
        GamePhase::Discard { side, required } => format!("Discard ({side:?}, {required} remaining)"),
        GamePhase::GameOver(winner) => format!("Game over — {winner:?} wins"),
    };
    let text = format!(
        "Turn {} | Phase: {phase_label} | You: {} | Corp: {}c {} AP:{}/{to_win} BP:{} | Runner: {}c {} Tags:{} MU:{} AP:{}/{to_win}",
        view.turn,
        match app.viewer() {
            Viewer::Player(side) => format!("{side:?}"),
            Viewer::Spectator => "Spectator".to_string(),
        },
        view.corp.credits,
        click_pool(view.corp.clicks, CORP_MAX_CLICKS),
        view.corp.agenda_points,
        view.corp.bad_publicity,
        view.runner.credits,
        click_pool(view.runner.clicks, RUNNER_MAX_CLICKS),
        view.runner.tags,
        view.runner.memory_units,
        view.runner.agenda_points,
        to_win = view.rules.winning_agenda_points,
    );
    let text = match app.decision_clock() {
        Some((side, remaining)) => format!("{text} | Clock: {side:?} {}s", remaining.as_secs()),
        None => text,
    };
    frame.render_widget(Paragraph::new(text), area);
}

fn click_pool(current: u32, max: u32) -> String {
    (0..max).map(|i| if i < current { "[x]" } else { "[ ]" }).collect::<Vec<_>>().join("")
}

fn draw_board(frame: &mut Frame, area: Rect, app: &impl RenderableView) {
    let [corp_area, runner_area] =
        Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);

    let Some(view) = app.view() else {
        frame.render_widget(Block::default().borders(Borders::ALL).title("Corp servers"), corp_area);
        frame.render_widget(Block::default().borders(Borders::ALL).title("Runner rig"), runner_area);
        return;
    };

    let mut corp_lines = vec![
        Line::from(format!("HQ: {} cards   R&D: {} cards   Archives: {} cards", view.corp.hq_count, view.corp.rd_count, view.corp.archives.len())),
        Line::from(""),
    ];
    for server in &view.corp.servers {
        corp_lines.push(Line::from(format_server(server, app.registry())));
    }
    if let Some(run) = &view.active_run {
        corp_lines.push(Line::from(""));
        corp_lines.push(run_phase_strip(run.phase));
        for line in format_run(run, app.registry()) {
            corp_lines.push(Line::from(line));
        }
    }
    frame.render_widget(
        Paragraph::new(corp_lines).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title("Corp servers")),
        corp_area,
    );

    let mut runner_lines = vec![
        Line::from(format!("Grip: {} cards   Stack: {} cards   Heap: {} cards", view.runner.grip_count, view.runner.stack_count, view.runner.heap.len())),
        Line::from(""),
    ];
    if view.runner.rig.is_empty() {
        runner_lines.push(Line::from("(rig empty)"));
    } else {
        runner_lines.extend(view.runner.rig.iter().map(|card| {
            let counters = counter_label(Some(&card.card), card.counters, app.registry());
            Line::from(format!("{} (str {}{counters})", card_title(&card.card, app.registry()), card.current_strength))
        }));
    }
    frame.render_widget(
        Paragraph::new(runner_lines).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title("Runner rig")),
        runner_area,
    );
}

fn format_server(server: &ServerView, registry: &CardRegistry) -> String {
    let describe = |card: &netrunner_core::rules::PublicInstalledCard| {
        let rez = if card.rezzed { "rezzed" } else { "unrezzed" };
        let label = card.card.as_ref().map(|id| card_title(id, registry)).unwrap_or_else(|| "???".to_string());
        // `None` means this viewer may not see the count at all (unrezzed,
        // and not theirs), which renders the same as "none placed".
        let counters = counter_label(card.card.as_ref(), card.counters.unwrap_or(0), registry);
        if card.advancement_tokens > 0 {
            format!("{label} ({rez}, {} adv{counters})", card.advancement_tokens)
        } else {
            format!("{label} ({rez}{counters})")
        }
    };
    let cards: Vec<String> = server.ice.iter().chain(server.root.iter()).map(describe).collect();
    let contents = if cards.is_empty() { "(empty)".to_string() } else { cards.join(", ") };
    format!("{}: {contents}", server_label(server.server))
}

fn server_label(server: ServerId) -> String {
    match server {
        ServerId::Hq => "HQ".to_string(),
        ServerId::RnD => "R&D".to_string(),
        ServerId::Archives => "Archives".to_string(),
        ServerId::Remote(n) => format!("Remote {n}"),
    }
}

/// Null Signal's six run phases with the current one highlighted — the
/// [Run Timing Guide](https://nullsignal.games/players/learn-to-play/run-guide/)
/// is the one reference both role guides share, so a player who has read
/// it should see its words on screen. The engine's `RunPhase` maps onto
/// them directly except for *Movement*, which is the moment between
/// passing one piece of ice and approaching the next and is never a
/// decision point here: it is listed so the strip matches the guide, and
/// is never lit.
fn run_phase_strip(phase: RunPhase) -> Line<'static> {
    const NAMES: [&str; 6] = ["Initiation", "Approach ice", "Encounter ice", "Movement", "Success", "Run ends"];
    let lit = match phase {
        RunPhase::Initiation => 0,
        RunPhase::ApproachIce => 1,
        RunPhase::EncounterIce => 2,
        RunPhase::AccessingCard | RunPhase::Success => 4,
        RunPhase::Ended => 5,
    };
    let mut spans = Vec::with_capacity(NAMES.len() * 2);
    for (index, name) in NAMES.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" › "));
        }
        let text = format!("{}. {name}", index + 1);
        spans.push(if index == lit {
            Span::styled(text, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(text, Style::default().fg(Color::DarkGray))
        });
    }
    Line::from(spans)
}

fn format_run(run: &netrunner_core::rules::PublicRunState, registry: &CardRegistry) -> Vec<String> {
    let mut lines = vec![format!("Run on {} (ICE {}/{})", server_label(run.server), run.position, run.ice.len())];
    for (index, ice) in run.ice.iter().enumerate() {
        let marker = if index == run.position { ">" } else { " " };
        let rez = if ice.rezzed { "rezzed" } else { "unrezzed" };
        match &ice.identity {
            Some(identity) => lines.push(format!("{marker} {} [{rez}, str {}]", card_title(&identity.card, registry), identity.current_strength)),
            None => lines.push(format!("{marker} ??? [{rez}]")),
        }
    }
    lines
}

fn draw_actions(frame: &mut Frame, area: Rect, app: &impl RenderableView) {
    let labels = app.legal_action_labels();
    let items: Vec<ListItem> = labels.iter().map(|label| ListItem::new(label.clone())).collect();
    let mut state = ListState::default();
    if !labels.is_empty() {
        state.select(Some(app.selected()));
    }
    let mut title = Line::from(if let Some(title) = app.actions_title() {
        title
    } else if labels.is_empty() {
        "Waiting for the other side...".to_string()
    } else if app.coaching().is_some() {
        "Actions (Up/Down, Enter to act, a to show all, q to quit)".to_string()
    } else {
        "Legal actions (Up/Down, Enter to act, q to quit)".to_string()
    });
    if let Some(rejection) = app.last_rejection() {
        title.push_span(Span::styled(format!("  rejected: {rejection}"), Style::default().fg(Color::Red)));
    }
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}

/// The lesson's coaching panel: where the player is, what to do, what the
/// highlighted action would do, and — during a run — the phase strip.
fn draw_coach(frame: &mut Frame, area: Rect, app: &impl RenderableView) {
    let Some(coaching) = app.coaching() else { return };
    let mut lines = vec![
        Line::from(Span::styled(format!("Step {} of {}", coaching.step, coaching.total), Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
    ];
    for paragraph in coaching.prose.split('\n') {
        lines.push(Line::from(paragraph.to_string()));
    }
    if let Some(hint) = &coaching.hint {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(format!("Hint: {hint}"), Style::default().fg(Color::Cyan))));
    }
    if !coaching.gated {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "This step matches none of your legal actions, so every legal action is shown.",
            Style::default().fg(Color::Yellow),
        )));
    } else if coaching.showing_all {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Showing every legal action — press a to return to the lesson's list.", Style::default().fg(Color::Yellow))));
    }
    if let Some(action) = app.selected_action() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Selected action:", Style::default().add_modifier(Modifier::UNDERLINED))));
        lines.push(Line::from(explain_action(&action, app.registry(), app.view())));
    }
    if let Some(run) = app.view().and_then(|view| view.active_run.as_ref()) {
        lines.push(Line::from(""));
        lines.push(run_phase_strip(run.phase));
    }
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title(coaching.title.clone())),
        area,
    );
}

fn draw_action_log(frame: &mut Frame, area: Rect, log: &[String]) {
    let visible = log.iter().rev().take(10).rev();
    let items: Vec<ListItem> = visible.map(|line| ListItem::new(line.clone())).collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Match log"));
    frame.render_widget(list, area);
}

/// A centered popup over whatever is underneath. Sized to the body: a
/// two-line game-over notice gets a small box, a lesson intro a large one.
fn draw_modal(frame: &mut Frame, modal: &Modal) {
    let long = modal.body.len() > 120;
    let area = centered_rect(if long { 70 } else { 40 }, if long { 60 } else { 20 }, frame.area());
    let mut text = Vec::new();
    for paragraph in modal.body.split('\n') {
        text.push(Line::from(paragraph.to_string()));
    }
    text.push(Line::from(""));
    text.push(Line::from(Span::styled(modal.footer.clone(), Style::default().add_modifier(Modifier::BOLD))));
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text)
            .alignment(if long { Alignment::Left } else { Alignment::Center })
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(modal.title.clone()).style(Style::default().fg(Color::Yellow))),
        area,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let [_, vertical, _] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage((100 - percent_y) / 2), Constraint::Percentage(percent_y), Constraint::Percentage((100 - percent_y) / 2)])
        .areas(area);
    let [_, horizontal, _] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage((100 - percent_x) / 2), Constraint::Percentage(percent_x), Constraint::Percentage((100 - percent_x) / 2)])
        .areas(vertical);
    horizontal
}

fn card_title(id: &CardId, registry: &CardRegistry) -> String {
    registry.get(id).map(|c| c.title.clone()).unwrap_or_else(|| id.0.clone())
}

/// `", 3 virus"` for a card carrying counters, or `""` for one carrying
/// none. The *kind* is looked up here rather than read off the view:
/// `counter_kind` is static card data every client already has a
/// `CardRegistry` for, so `masking` deliberately doesn't duplicate it into
/// `PublicInstalledCard` (see that field's doc comment).
///
/// A zero count renders as nothing at all — every card would otherwise
/// carry a permanent `0 counters` badge. That makes it visually identical
/// to a card whose counters are *hidden*, which is fine here: the
/// distinction is real in the view and a richer client may use it, but
/// there is nothing useful for this one to draw in either case.
fn counter_label(card: Option<&CardId>, counters: u32, registry: &CardRegistry) -> String {
    if counters == 0 {
        return String::new();
    }
    let kind = card
        .and_then(|id| registry.get(id))
        .and_then(|definition| definition.counter_kind)
        .map(|kind| match kind {
            CounterKind::Virus => "virus",
            CounterKind::Power => "power",
            CounterKind::Credit => "credits",
        })
        .unwrap_or("counters");
    format!(", {counters} {kind}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::tutorial;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Draws a lesson prompt — coaching panel, gated list, intro modal —
    /// and then the escape hatch, into a test backend. There is no
    /// interactive test of the TUI, so this is what keeps the lesson
    /// layout from panicking on a real terminal: every widget the lesson
    /// path adds is rendered here at least once.
    #[test]
    fn a_lesson_prompt_renders_with_coaching_modal_and_escape_hatch() {
        let registry = decks::sample_deck_registry();
        let lesson = tutorial::track(Side::Corp).into_iter().next().expect("a Corp lesson is embedded");
        let mut session = LessonSession::start(lesson.clone(), registry.clone(), 0).unwrap();
        let LessonStep::Prompt { view, allowed, step, total } = session.step().unwrap() else {
            panic!("the first lesson step is a prompt");
        };
        let mut ui = LocalUiState::new(registry, Side::Corp);
        ui.modal = Some(Modal::new(&lesson.title, &lesson.intro, "Enter to begin"));
        let live = &lesson.steps[step];
        ui.begin_gated_decision(
            *view,
            allowed.clone(),
            Coaching { title: lesson.title.clone(), step: step + 1, total, prose: live.prose.clone(), hint: live.hint.clone(), gated: true, showing_all: false },
        );
        assert_eq!(ui.offered_actions(), allowed.as_slice(), "the list is the gated subset");

        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| draw_frame(frame, &ui, None)).unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Step 1 of"), "the coaching panel is drawn");
        assert!(rendered.contains("Enter to begin"), "the intro modal is drawn");

        ui.modal = None;
        ui.toggle_show_all();
        assert!(ui.offered_actions().len() >= allowed.len(), "the escape hatch shows every legal action");
        assert!(ui.coaching().is_some_and(|c| c.showing_all));
        ui.last_rejection = Some("NotYourTurn".to_string());
        terminal.draw(|frame| draw_frame(frame, &ui, None)).unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("rejected: NotYourTurn"), "the rejection line is drawn");
        assert!(rendered.contains("Showing every legal action"), "the escape hatch is announced");
    }

    /// The replay path through the shared renderer: the events pane title,
    /// the coaching panel's step counter, and the chair swap all draw, on a
    /// test backend, at the setup and at the end of a recorded game.
    #[test]
    fn a_replay_renders_its_events_pane_and_step_counter() {
        use netrunner_bots::RandomAgent;
        use netrunner_core::rules::MatchRules;
        use netrunner_session::MatchRecordHeader;

        let registry = decks::sample_deck_registry();
        let (corp, runner) = netrunner_core::decks::matchups().into_iter().next().expect("a sample matchup");
        let header = MatchRecordHeader { seed: 5, corp_deck: corp.to_deck(), runner_deck: runner.to_deck(), rules: MatchRules::default() };
        let (state, _events) = header.setup(&registry).unwrap();
        let mut session = Session::new(
            state,
            registry.clone(),
            Seat::Agent(Box::new(RandomAgent::new(5))),
            Seat::Agent(Box::new(RandomAgent::new(6))),
        );
        assert!(matches!(session.run(), SessionStep::Ended { .. }));
        let (_state, history) = session.into_parts();
        let total = history.len();
        let mut replay = Replay::load(&header, history, registry, Side::Runner, "game_00000.jsonl").unwrap();

        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        terminal.draw(|frame| draw_frame(frame, &replay, None)).unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains(&format!("Step 0 of {total}")), "the coaching panel counts positions");
        assert!(rendered.contains("Events (none yet"), "the actions pane is retitled for events");

        assert!(replay_key(&mut replay, KeyCode::End));
        assert!(replay_key(&mut replay, KeyCode::Char('s')));
        assert_eq!(replay.side(), Side::Corp);
        terminal.draw(|frame| draw_frame(frame, &replay, None)).unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains(&format!("Step {total} of {total}")));
        assert!(rendered.contains("Game over"), "the header shows the recorded ending");
        assert!(!replay_key(&mut replay, KeyCode::Char('q')), "q quits");
    }

    /// The strip lights exactly one of Null Signal's six phases, and never
    /// Movement.
    #[test]
    fn the_run_phase_strip_lights_one_phase() {
        for (phase, expected) in [
            (RunPhase::Initiation, "1. Initiation"),
            (RunPhase::ApproachIce, "2. Approach ice"),
            (RunPhase::EncounterIce, "3. Encounter ice"),
            (RunPhase::AccessingCard, "5. Success"),
            (RunPhase::Success, "5. Success"),
            (RunPhase::Ended, "6. Run ends"),
        ] {
            let line = run_phase_strip(phase);
            let lit: Vec<&str> = line
                .spans
                .iter()
                .filter(|span| span.style.add_modifier.contains(Modifier::BOLD))
                .map(|span| span.content.as_ref())
                .collect();
            assert_eq!(lit, vec![expected], "{phase:?}");
        }
    }
}
