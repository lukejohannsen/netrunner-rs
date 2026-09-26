pub mod builder;
pub mod guide;
pub mod layout;
pub mod menu;
pub mod online;
pub mod start;

use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode};
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
    get_action_mask, ActionSpace, DeckOrder, GamePhase, GameState, PlayerAction, RunPhase, ServerId, Side, SubroutineStatus,
};
use netrunner_core::tutorial::Lesson;
use netrunner_core::view::{ClientView, ServerView};
use netrunner_session::{GameEndReason, LessonSession, LessonStep, Seat, Session, SessionStep, SubmitError, UNDO_DEPTH};

use netrunner_client::board::{offered_label, routes, ActionMap, Affordance, Asks, AutoBreak, Next, Route, Target};
use netrunner_client::access::Access;
use netrunner_client::card_face::Face;
use netrunner_client::placement::Placement;
use netrunner_client::selection::Selection;

use crate::app::{card_modal, explain_action, push_log_line, App, CardPicker, Coaching, Modal, RenderableView};
use crate::bots;
use crate::config::{BotKind, Config, Mode};
use netrunner_client::actions::pop_log_entries;
use netrunner_client::play::{lone_pass, stall_message};
use netrunner_client::run_pass::RunPass;
use netrunner_client::standing::{self, optional_trigger, standing_answer, Answer, Answers, PromptKey};
use netrunner_client::decks;
use crate::record::{self, SeatRecord};
use crate::remote;
use crate::replay::Replay;

const CORP_MAX_CLICKS: u32 = 3;
const RUNNER_MAX_CLICKS: u32 = 4;

/// Search budget for a `mcts`/`puct` opponent in interactive play — the
/// agents' own defaults. `--simulations` is a headless flag; a human
/// opponent gets the full-strength bot.
const DEFAULT_SIMULATIONS: usize = 64;

pub async fn run(config: &mut Config) -> Result<(), Box<dyn std::error::Error>> {
    match config.mode {
        Mode::Local => {
            // No side flag at all means the player has not chosen an
            // opponent, which used to be an error ("both --corp and
            // --runner are human") and then a one-shot start screen. Now
            // it is the main menu, which folds each choice back into a
            // copy of `config` so a game it launches runs the flag path
            // and no other. Any side flag skips it.
            if !config.seats_bot(Side::Corp) && !config.seats_bot(Side::Runner) {
                return menu::run(config);
            }
            run_local(config)
        }
        Mode::Remote => run_remote(config).await,
    }
}

async fn run_remote(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    // A server deals nobody a deck (Phase 4 §7 stage 3), so a player
    // always brings one: `--deck`, or the side's own deck flag when only
    // `--side` is given. Neither is a question only the person can answer,
    // so it is refused here rather than by the server. A spectator brings
    // nothing.
    let deck_name = match (&config.deck, config.side.map(Side::from)) {
        _ if config.spectate.is_some() => None,
        (Some(name), _) => Some(name.clone()),
        (None, Some(Side::Corp)) => Some(config.corp_deck.clone()),
        (None, Some(Side::Runner)) => Some(config.runner_deck.clone()),
        (None, None) => return Err("bring a deck: --deck <name>, or --side corp|runner to bring --corp-deck or --runner-deck (a server deals none)".into()),
    };
    let brought = match deck_name {
        Some(name) => {
            let dir = netrunner_client::deck_store::resolve_decks_dir(config.decks_dir.as_deref())?;
            Some(netrunner_client::deck_store::load(&dir, &name)?.deck)
        }
        None => None,
    };
    let brought_id = brought.as_ref().map(|deck| deck.id.clone());
    let goal = match config.spectate {
        Some(match_id) => remote::Goal::Watch { match_id },
        None => {
            let deck = brought.expect("a player always brings a deck");
            let lobby = config.lobby.clone().unwrap_or_else(|| netrunner_server::protocol::format_lobby_id(config.format.into()));
            remote::Goal::Play(remote::seat(&record::player_name(config), lobby, config.password.clone(), deck))
        }
    };
    // Before the terminal is taken, so the lobby wait goes to stderr.
    let joined = remote::connect(&config.server, goal, |position| eprintln!("Waiting in the lobby for another player ({position} waiting)...")).await?;
    let mut terminal = ratatui::init();
    let result = play_remote(&mut terminal, joined, brought_id.as_deref());
    ratatui::restore();
    result
}

/// A seat or a spectator's place at a server's match, played on the
/// caller's terminal until the player leaves. `--mode remote` and the
/// menu's Play Online both end here.
///
/// The wire protocol never transmits a `CardRegistry`, so the client
/// builds one locally to resolve card titles. It needs no agreement with
/// the host beyond the embedded pool: every deck the daemon deals or
/// accepts is validated against it, whose cards are exactly
/// `register_playable_cards`.
///
/// `brought` is the id of the deck this player sent, if any. A daemon
/// older than `Connect::deck` ignores the field and deals, so the dealt id
/// is checked and a mismatch is said on the header rather than discovered
/// by drawing someone else's cards.
pub fn play_remote(
    terminal: &mut ratatui::DefaultTerminal,
    joined: remote::Joined,
    brought: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let registry = decks::sample_deck_registry();
    let dealt = match joined.viewer {
        Viewer::Player(Side::Corp) => Some(joined.decks.0.clone()),
        Viewer::Player(Side::Runner) => Some(joined.decks.1.clone()),
        Viewer::Spectator => None,
    };
    let mut app = App::new(registry, joined.viewer, joined.tx, joined.rx);
    app.follow_link(joined.link);
    app.answers = crate::settings::answers();
    if let (Some(brought), Some(dealt)) = (brought, dealt)
        && brought != dealt
    {
        app.connection_notice = Some(format!("This server dealt you {dealt:?} instead of your deck — it predates bringing your own"));
    }
    run_event_loop(terminal, &mut app)
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
    let mut terminal = ratatui::init();
    let result = play_local(&mut terminal, config);
    ratatui::restore();
    result
}

/// `run_local` on a terminal the caller owns — the main menu's, so a
/// finished game returns to it rather than to the shell.
pub fn play_local(terminal: &mut ratatui::DefaultTerminal, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let human_side = match (config.seats_bot(Side::Corp), config.seats_bot(Side::Runner)) {
        (false, false) => {
            return Err("interactive mode requires exactly one human-controlled side (both --corp and --runner are human)".into());
        }
        (false, true) => Side::Corp,
        (true, false) => Side::Runner,
        (true, true) => return Err("interactive mode requires exactly one human-controlled side (neither --corp nor --runner is human)".into()),
    };

    let registry = decks::sample_deck_registry();
    let decks_dir = netrunner_client::deck_store::resolve_decks_dir(config.decks_dir.as_deref())?;
    let (corp_deck, runner_deck) =
        decks::decks_for_match(&decks_dir, &config.corp_deck, &config.runner_deck, &registry, config.format.into())?;
    let seed = config.seed.unwrap_or_else(rand::random);
    let (state, _events) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, seed)?;

    let bot_side = human_side.other();
    let (bot_kind, bot_deck) = if human_side == Side::Corp { (config.runner, &runner_deck) } else { (config.corp, &corp_deck) };
    let personality = config.personality_for(bot_side, bot_deck)?;
    let (bot_seat, mut indexed_bot) =
        build_bot_seat(config.level_for(bot_side), bot_kind, bot_side, seed.wrapping_add(1), &config.model, personality)?;
    // Opened before the game so a bad record file fails here, not after
    // an hour of play.
    let seat = record::seat_record(config, human_side, config.level_for(bot_side), bot_kind, personality, seed, &corp_deck.id, &runner_deck.id)?;

    let (corp_seat, runner_seat) = match human_side {
        Side::Corp => (Seat::External, bot_seat),
        Side::Runner => (bot_seat, Seat::External),
    };
    // A game against a bot keeps the person's last few moves so one can be
    // taken back (`Session::rewind`); a lesson does not, because its steps
    // are scripted against the actions actually taken.
    let mut session = Session::new(state, registry.clone(), corp_seat, runner_seat).with_undo(UNDO_DEPTH);

    let mut ui = LocalUiState::new(registry, human_side);
    ui.answers = crate::settings::answers();
    drive_local(terminal, &mut session, &mut ui, indexed_bot.as_mut(), human_side, seat)
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

/// Plays `lessons` in order on the caller's terminal, stopping at the first
/// the player does not complete. Returns `Completed` only if every lesson
/// was — which is what lets `learn track` hand a graduate the starter game.
pub fn play_lessons(
    terminal: &mut ratatui::DefaultTerminal,
    lessons: &[Lesson],
    registry: &CardRegistry,
    seed: u64,
) -> Result<LessonOutcome, Box<dyn std::error::Error>> {
    for lesson in lessons {
        if run_lesson(terminal, lesson, registry, seed)? == LessonOutcome::Stopped {
            return Ok(LessonOutcome::Stopped);
        }
    }
    Ok(LessonOutcome::Completed)
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
///
/// Draws on the caller's terminal: `learn` owns one for the subcommand, the
/// main menu owns one for everything it launches.
pub fn play_starter_game(
    terminal: &mut ratatui::DefaultTerminal,
    human_side: Side,
    corp: &DeckFile,
    runner: &DeckFile,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let registry = decks::sample_deck_registry();
    let seed = config.seed.unwrap_or_else(rand::random);
    let rules = corp.category.match_rules();
    let (state, _events) = GameState::setup_with(&corp.to_deck(), &runner.to_deck(), &registry, seed, rules, DeckOrder::Shuffled)?;
    let bot_deck = if human_side == Side::Corp { runner } else { corp };
    let personality = config.personality_for(human_side.other(), bot_deck)?;
    let bot = bots::make_agent(
        BotKind::Heuristic,
        human_side.other(),
        seed.wrapping_add(1),
        bots::AgentSetup::new(DEFAULT_SIMULATIONS).with_personality(personality),
    )
        .expect("the heuristic always has a BotAgent form");
    // Recorded like any other local game: the starter game is a person's
    // first real opponent, and its result is the first line of their
    // record.
    let seat = record::seat_record(config, human_side, None, BotKind::Heuristic, personality, seed, &corp.id, &runner.id)?;
    let (corp_seat, runner_seat) = match human_side {
        Side::Corp => (Seat::External, Seat::Agent(bot)),
        Side::Runner => (Seat::Agent(bot), Seat::External),
    };
    let mut session = Session::new(state, registry.clone(), corp_seat, runner_seat);
    let mut ui = LocalUiState::new(registry, human_side);
    ui.answers = crate::settings::answers();
    drive_local(terminal, &mut session, &mut ui, None, human_side, seat)
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
        // The board as it now stands, not the one the learner chose from —
        // see `log_last`.
        let now = session.session().view_for(lesson.side);
        for entry in session.drain_log() {
            push_log_line(&mut ui.action_log, &entry, &ui.registry, Some(&now));
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
                // A lesson's `ui.back` is never set, so `TookBack` cannot
                // come back from here.
                if prompt_human(terminal, &mut ui, |action| session.submit(action))? == Prompted::Quit {
                    return Ok(LessonOutcome::Stopped);
                }
            }
            LessonStep::Complete { view } => {
                // Kept whether or not the closing words are dismissed: the
                // lesson is done once its last step is. A file that will
                // not save costs the tick, never the lesson.
                let _ = crate::settings::finish_lesson(&lesson.id);
                ui.finish(*view);
                ui.coaching = None;
                ui.modal = Some(Modal::new(&format!("{} — complete", lesson.title), &lesson.outro, "Enter to continue, q to quit"));
                return Ok(if hold_modal(terminal, &ui)? { LessonOutcome::Stopped } else { LessonOutcome::Completed });
            }
            LessonStep::Ended { winner, reason } => {
                ui.finish(session.session().view_for(lesson.side));
                ui.coaching = None;
                show_game_over(terminal, &ui, winner, reason, None)?;
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
    level: Option<netrunner_bots::Level>,
    kind: crate::config::BotKind,
    side: Side,
    seed: u64,
    model: &str,
    personality: Personality,
) -> Result<(Seat, Option<Box<dyn netrunner_bots::Agent>>), String> {
    // A rung is always a `Seat::Agent`: the ladder is built from the four
    // view-based searches, and deliberately excludes the one kind that
    // needs the index path.
    match kind {
        crate::config::BotKind::Onnx if level.is_none() => {
            Ok((Seat::External, Some(bots::make_driver(kind, side, seed, DEFAULT_SIMULATIONS, model, personality)?)))
        }
        _ => {
            let setup = bots::AgentSetup::new(DEFAULT_SIMULATIONS).with_personality(personality);
            let agent = bots::make_seat_agent(level, kind, side, seed, setup, model)?
                .ok_or_else(|| "interactive mode needs a bot on the non-human side".to_string())?;
            Ok((Seat::Agent(agent), None))
        }
    }
}

/// The pull loop: step the session, render whatever it reports, and block
/// on keyboard input only when the *human* seat is the one being asked.
///
/// A finished game is recorded before the game-over modal is drawn, so
/// the modal can show where it leaves the record; a quit records a loss
/// from `record::FORFEIT_FROM_TURN` on and nothing before it; a stall
/// records nothing. A take-back costs nothing, whichever kind the session
/// says it is — a game against a bot is casual (`netrunner_client::record`).
fn drive_local(
    terminal: &mut ratatui::DefaultTerminal,
    session: &mut Session,
    ui: &mut LocalUiState,
    mut indexed_bot: Option<&mut Box<dyn netrunner_bots::Agent>>,
    human_side: Side,
    seat: SeatRecord,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut seat = Some(seat);
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
            // Nothing to ask: the pass is taken for the person, and the
            // log line is what they see of it.
            SessionStep::Awaiting { side, view } if side == human_side && lone_pass(&view, session.registry()).is_some() => {
                let pass = lone_pass(&view, session.registry()).expect("matched above");
                session.submit(pass).map_err(|error| format!("the lone pass was rejected: {error}"))?;
                log_last(session, ui, human_side);
            }
            // The Corp said "no more this run" (`w`): the window's pass is
            // taken the same way, until the run ends.
            SessionStep::Awaiting { side, view } if side == human_side && ui.run_pass.pass(&view).is_some() => {
                let pass = ui.run_pass.pass(&view).expect("matched above");
                session.submit(pass).map_err(|error| format!("the run's pass was rejected: {error}"))?;
                log_last(session, ui, human_side);
            }
            // A card's "you may" the person answered for good: answered
            // so, and logged under the action it took. Not straight after
            // a take-back, which would give the answer back at once.
            SessionStep::Awaiting { side, view } if side == human_side && !ui.asking_again && standing_answer(&view, session.registry(), &ui.answers).is_some() => {
                let (action, answer) = standing_answer(&view, session.registry(), &ui.answers).expect("matched above");
                let key = optional_trigger(&view, session.registry()).expect("an answer is to an optional trigger").key;
                session.submit(action).map_err(|error| format!("a remembered answer was rejected: {error}"))?;
                log_last(session, ui, human_side);
                ui.action_log.push(format!("           ({})", standing::log_line(&key, answer, &ui.registry)));
            }
            SessionStep::Awaiting { side, view } if side == human_side => {
                ui.asking_again = false;
                // A route through the ICE takes its next step before the
                // person is asked; a step the engine refuses ends the
                // route and the person is asked on the same view.
                let stopped = match ui.continue_break(&view) {
                    Ok(Some(step)) => match session.submit(step) {
                        Ok(()) => {
                            log_last(session, ui, human_side);
                            continue;
                        }
                        Err(error) => {
                            ui.breaking = None;
                            Some(error.to_string())
                        }
                    },
                    Ok(None) => None,
                    Err(reason) => Some(reason),
                };
                ui.begin_decision(*view);
                ui.last_rejection = stopped;
                ui.back = session.can_rewind().is_some();
                match prompt_human(terminal, ui, |action| session.submit(action))? {
                    Prompted::Quit => {
                        if let Some(seat) = seat.take()
                            && let Some(outcome) = record::quit_outcome(session.state().turn, human_side)
                        {
                            seat.finish(outcome)?;
                        }
                        return Ok(());
                    }
                    Prompted::Submitted => log_last(session, ui, human_side),
                    // The state goes back exactly and the log with it.
                    Prompted::TookBack => {
                        if let Some(rewound) = session.rewind() {
                            pop_log_entries(&mut ui.action_log, rewound.removed, "You took that back.");
                            ui.asking_again = true;
                            ui.run_pass.stop();
                        }
                    }
                }
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
                let report = match seat.take() {
                    Some(seat) => Some(seat.finish(record::outcome_of(winner))?.lines().join("\n")),
                    None => None,
                };
                return show_game_over(terminal, ui, winner, reason, report);
            }
            SessionStep::Stalled(reason) => return Err(stall_message(reason).into()),
            SessionStep::Applied { .. } => unreachable!("the inner loop only breaks once it can no longer apply"),
        }
    }
}

/// Appends the action just applied to the human's log, as the human may
/// see it — against the board the action *left*, as the server's and the
/// desktop's logs are: `ui.view` is still the one the human chose from, on
/// which a card just installed is not yet anywhere and a card just
/// selected is not yet selected.
///
/// Every applied action comes through here, the bot's included, so it is
/// also where a run pass sees the run end.
fn log_last(session: &Session, ui: &mut LocalUiState, human_side: Side) {
    let after = session.view_for(human_side);
    ui.run_pass.see(&after);
    if let Some(entry) = session.last_entry_for(human_side) {
        push_log_line(&mut ui.action_log, &entry, &ui.registry, Some(&after));
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
/// How `prompt_human` was left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prompted {
    Quit,
    Submitted,
    /// `u`: the caller owns the session, so it does the rewinding.
    TookBack,
}

fn prompt_human(
    terminal: &mut ratatui::DefaultTerminal,
    ui: &mut LocalUiState,
    mut submit: impl FnMut(PlayerAction) -> Result<(), SubmitError>,
) -> Result<Prompted, Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|frame| draw_frame(frame, ui, None))?;

        if event::poll(Duration::from_millis(100))?
            && let Ok(Event::Key(key)) = event::read()
        {
            // The inspector owns its own modal (a card's text over its
            // list); every other modal — a lesson's intro or outro — is
            // dismissed here, and `q` on one of those quits.
            if ui.inspector_key(key.code) {
                continue;
            }
            if ui.modal.is_some() {
                match key.code {
                    KeyCode::Char('q') => return Ok(Prompted::Quit),
                    KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Esc => ui.modal = None,
                    _ => {}
                }
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(Prompted::Quit),
                // Goes at once: nothing rides on a game against a bot.
                KeyCode::Char('u') if ui.back => return Ok(Prompted::TookBack),
                KeyCode::Up | KeyCode::Char('k') => ui.move_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => ui.move_selection(1),
                KeyCode::Char('a') => ui.toggle_show_all(),
                // `w`: the rest of this run passed for the Corp, from this
                // window on; pressed again, asked again.
                KeyCode::Char('w') if ui.run_pass.is_on() => ui.run_pass.stop(),
                KeyCode::Char('w') => {
                    if let Some(pass) = ui.start_run_pass() {
                        match submit(pass) {
                            Ok(()) => return Ok(Prompted::Submitted),
                            Err(SubmitError::Rules(error)) => {
                                ui.run_pass.stop();
                                ui.last_rejection = Some(error.to_string());
                            }
                            Err(error) => return Err(error.into()),
                        }
                    }
                }
                // `y` / `n`: this card's "you may", answered the same way
                // from now on (`netrunner_client::standing`) — the
                // option's own action, submitted as Enter would.
                KeyCode::Char(letter @ ('y' | 'n')) => {
                    let answer = if letter == 'y' { Answer::Always } else { Answer::Never };
                    if let Some((key, action)) = ui.remember(answer) {
                        match submit(action) {
                            Ok(()) => {
                                if let Err(error) = crate::settings::remember(key, answer) {
                                    ui.last_rejection = Some(format!("the answer is kept for this game only: {error}"));
                                }
                                return Ok(Prompted::Submitted);
                            }
                            Err(SubmitError::Rules(error)) => ui.last_rejection = Some(error.to_string()),
                            Err(error) => return Err(error.into()),
                        }
                    }
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some(action) = ui.selected_action().or_else(|| ui.start_break()) {
                        match submit(action) {
                            Ok(()) => return Ok(Prompted::Submitted),
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
///
/// `note` is appended to the summary — the record lines for a local game
/// — and is `None` on the remote path, where the daemon keeps the rating
/// book and tells the client nothing about it yet.
fn show_game_over(
    terminal: &mut ratatui::DefaultTerminal,
    ui: &LocalUiState,
    winner: Side,
    reason: GameEndReason,
    note: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|frame| draw_frame(frame, ui, Some((winner, reason, note.as_deref()))))?;
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
    /// The card inspector, while it is open (`c`).
    card_picker: Option<CardPicker>,
    last_rejection: Option<String>,
    /// Whether `u` has a move to take back (`Session::can_rewind`), set
    /// per decision.
    back: bool,
    /// Every card's way through the encountered ICE
    /// (`netrunner_client::board::breaks`), listed after the actions.
    /// None under a lesson: the lessons teach the pump and the break.
    breaks: Vec<Route>,
    /// What each legal action of this decision's view goes on to ask
    /// (`netrunner_client::board::preview`).
    asks: Asks,
    /// The route under way; `drive_local` asks it for the next step
    /// before the person is asked anything.
    breaking: Option<AutoBreak>,
    /// The person's answers to optional triggers
    /// (`netrunner_client::standing`), from the settings file.
    answers: Answers,
    /// A move was just taken back: the prompt it restores is asked, not
    /// answered from `answers`.
    asking_again: bool,
    /// The Corp's "no more this run" (`w`,
    /// `netrunner_client::run_pass`). The bot's run does not wait for a
    /// key, so it is stopped at the next question the run asks, or by
    /// the run ending.
    run_pass: RunPass,
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
            card_picker: None,
            last_rejection: None,
            back: false,
            breaks: Vec::new(),
            asks: Asks::default(),
            breaking: None,
            answers: Answers::default(),
            asking_again: false,
            run_pass: RunPass::default(),
        }
    }

    /// `w`: the run pass turned on, and this window's pass returned to
    /// submit. `None` under a lesson, which passes nothing for the
    /// person, and outside a run's window.
    fn start_run_pass(&mut self) -> Option<PlayerAction> {
        if self.coaching.is_some() {
            return None;
        }
        self.run_pass.start(self.view.as_ref()?)
    }

    /// The optional trigger on the prompt, answered `answer` for good:
    /// the answer is kept in `answers` and the action that gives it is
    /// returned to submit. `None` under a lesson, which answers nothing
    /// for the person, and when the prompt is not one or has no option
    /// the answer means.
    fn remember(&mut self, answer: Answer) -> Option<(PromptKey, PlayerAction)> {
        if self.coaching.is_some() {
            return None;
        }
        let prompt = optional_trigger(self.view.as_ref()?, &self.registry)?;
        let action = prompt.action(answer)?;
        self.answers.set(prompt.key.clone(), Some(answer));
        Some((prompt.key, action))
    }

    /// One inspector keypress, shared by the live prompt and the tests:
    /// `true` if the key was the inspector's — a modal or the picker was
    /// open, or `c` opened it — so the caller does not also treat it as an
    /// action-list key.
    fn inspector_key(&mut self, key: KeyCode) -> bool {
        if self.modal.is_some() && self.card_picker.is_some() {
            // A card's text, opened from the list: Esc goes back to the
            // list on the same card; `c` or `q` closes the whole inspector.
            match key {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Backspace | KeyCode::Left => self.modal = None,
                KeyCode::Char('c') | KeyCode::Char('q') => {
                    self.modal = None;
                    self.card_picker = None;
                }
                _ => {}
            }
            return true;
        }
        if let Some(picker) = &mut self.card_picker {
            match key {
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
            return true;
        }
        if key == KeyCode::Char('c')
            && let Some(view) = &self.view
        {
            self.card_picker = Some(CardPicker::open(view, &self.registry));
            return true;
        }
        false
    }

    /// Installs the view the session just handed us for a fresh human
    /// decision, resetting the highlight and the rejection notice.
    fn begin_decision(&mut self, view: ClientView) {
        self.view = Some(view);
        self.selected = 0;
        // A list built from the last decision's view would be stale.
        self.card_picker = None;
        self.allowed.clear();
        self.last_rejection = None;
        self.breaks = match (&self.view, &self.coaching) {
            (Some(view), None) => routes(view, &self.registry),
            _ => Vec::new(),
        };
        self.asks = self.view.as_ref().map(|view| Asks::of(view, &self.registry)).unwrap_or_default();
    }

    /// `begin_decision` under a lesson: the step's filtered list and its
    /// coaching. An empty filter falls back to the full list (and says so
    /// in the panel) rather than offering nothing.
    fn begin_gated_decision(&mut self, view: ClientView, allowed: Vec<PlayerAction>, mut coaching: Coaching) {
        debug_assert!(allowed.iter().all(|action| view.legal_actions.contains(action)), "a lesson may only narrow the legal list");
        self.begin_decision(view);
        // The lessons teach the pump and the break; a route would skip them.
        self.breaks.clear();
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
    /// and the escape hatch is closed, otherwise every legal action — less
    /// a second copy's selection toggle, which the first copy's row stands
    /// for (`netrunner_client::selection`).
    fn offered_actions(&self) -> Vec<PlayerAction> {
        let legal = self.view.as_ref().map_or(&[][..], |view| view.legal_actions.as_slice());
        let offered = if self.coaching.is_some() && !self.show_all && !self.allowed.is_empty() { &self.allowed } else { legal };
        netrunner_client::selection::shown(offered, self.view.as_ref(), &self.registry)
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

    /// Starts the selected row's route, if the selection is one: its
    /// first step, for the caller to submit. A route that no longer holds
    /// says why on the rejection line.
    fn start_break(&mut self) -> Option<PlayerAction> {
        let route = self.breaks.get(self.selected.checked_sub(self.offered_actions().len())?)?;
        let view = self.view.as_ref()?;
        let driver = AutoBreak::new(route, view, &self.registry);
        match driver.next(view, &self.registry) {
            Next::Submit(step) => {
                self.breaking = Some(driver);
                Some(step)
            }
            Next::Stopped(reason) => {
                self.last_rejection = Some(reason);
                None
            }
            Next::Wait | Next::Done => None,
        }
    }

    /// The running route's next step on `view`, or why it stopped; a
    /// route that finished or stopped is cleared.
    fn continue_break(&mut self, view: &ClientView) -> Result<Option<PlayerAction>, String> {
        let Some(driver) = &self.breaking else { return Ok(None) };
        match driver.next(view, &self.registry) {
            Next::Submit(step) => Ok(Some(step)),
            Next::Wait => Ok(None),
            Next::Done => {
                self.breaking = None;
                Ok(None)
            }
            Next::Stopped(reason) => {
                self.breaking = None;
                Err(reason)
            }
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
        let mut labels: Vec<String> =
            self.offered_actions().iter().map(|action| self.asks.label(action, offered_label(action, &self.registry, self.view.as_ref()))).collect();
        if let Some(view) = &self.view {
            labels.extend(self.breaks.iter().map(|route| route.label(view, &self.registry)));
        }
        labels
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
    fn card_picker(&self) -> Option<&CardPicker> {
        self.card_picker.as_ref()
    }
    fn actions_title(&self) -> Option<String> {
        self.view.as_ref().and_then(|view| crate::prose::decision_prompt(view, &self.registry))
    }
    fn notice(&self) -> Option<String> {
        let remember = self.coaching.is_none().then(|| self.view.as_ref().and_then(|view| crate::app::remember_hint(view, &self.registry))).flatten();
        let run_pass = self.coaching.is_none().then(|| self.view.as_ref().and_then(|view| crate::app::run_pass_hint(self.run_pass, view))).flatten();
        let notices: Vec<String> = [self.back.then(|| "u to take it back".to_string()), remember, run_pass].into_iter().flatten().collect();
        (!notices.is_empty()).then(|| notices.join(" · "))
    }
}

/// The remote render loop. A lost connection is not handled here: the
/// driver behind `app`'s channels reconnects on its own
/// (`netrunner_client::remote`), and `App` shows the link's state and
/// holds submissions until it is back — so the board keeps drawing with
/// the last view and `q` still quits, and nothing here blocks.
fn run_event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    while !app.should_quit {
        app.drain_messages();
        terminal.draw(|frame| draw_frame(frame, app, app.game_ended.map(|(winner, reason)| (winner, reason, None))))?;
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
fn draw_frame(frame: &mut Frame, ui: &impl RenderableView, game_over: Option<(Side, GameEndReason, Option<&str>)>) {
    let regions = layout::build_layout(frame.area(), ui.coaching().is_some());
    draw_header(frame, regions.header, ui);
    draw_board(frame, regions.board, ui);
    if let Some(coach) = regions.coach {
        draw_coach(frame, coach, ui);
    }
    draw_actions(frame, regions.actions, ui);
    draw_action_log(frame, regions.log, ui.action_log());
    // The accessed card, over the board and only over the board: the
    // actions pane keeps its keys, and the person reads the card with
    // `Steal …` / `Trash …` / `Pass on …` already highlighted beneath
    // it. Drawn after the board and before the overlays that own the
    // whole screen.
    if let Some(view) = ui.view()
        && let Some(access) = Access::of(view, ui.registry())
    {
        draw_card(frame, regions.board, &access.face, &access.title(), &access.facts());
    } else if let Some(view) = ui.view()
        && let Some((title, face)) = card_in_question(view, ui)
    {
        draw_card(frame, regions.board, &face, &title, &[]);
    }
    if let Some((winner, reason, note)) = game_over {
        let body = match note {
            Some(note) => format!("{winner:?} wins! ({reason:?})\n\n{note}"),
            None => format!("{winner:?} wins! ({reason:?})"),
        };
        draw_modal(frame, &Modal::new("Game over", &body, "Press q or Esc to leave the table."));
    } else if let Some(modal) = ui.modal() {
        draw_modal(frame, modal);
    } else if let Some(picker) = ui.card_picker() {
        draw_card_picker(frame, picker);
    }
}

/// The card a choice is about, when it is a card and not a place: the
/// card under the cursor of a card selection ("Select Hedge Fund" shows
/// Hedge Fund), or the card an install from a card's text is placing.
/// Drawn like an access, over the board with the actions pane still
/// live, because the person cannot choose between cards by their names.
/// Not the card *asking* (`Prompt::card`'s other answer): a run on a
/// server of the Runner's choice would then cover the servers being
/// chosen between, and the prompt's title already names it.
fn card_in_question(view: &ClientView, ui: &impl RenderableView) -> Option<(String, Face)> {
    let registry = ui.registry();
    let face = |id: &CardId| registry.get(id).map(Face::of);
    if let Some(selection) = Selection::of(view, registry) {
        let Some(PlayerAction::ToggleCardSelection { position }) = ui.selected_action() else { return None };
        let candidate = selection.candidate(position)?;
        let title = if candidate.selected { format!("Selected — {}", selection.display(candidate)) } else { selection.display(candidate) };
        return Some((title, face(candidate.card.as_ref()?)?));
    }
    let placement = Placement::of(view, registry)?;
    Some((format!("Installing {}", placement.card_name()), face(placement.card()?)?))
}

/// A card, centred over the board: the card being accessed, or the one a
/// choice is about (`card_in_question`).
///
/// **Not a [`Modal`]**, which owns the keyboard until dismissed: the
/// person must still be able to act, and the actions pane below this is
/// where they do it — already listing `Steal …`, `Trash …` and `Pass on
/// …` with Up/Down/Enter unchanged. So this covers `regions.board` only,
/// leaving the pane visible, which is also why it carries no buttons of
/// its own. Listing the actions twice was the alternative, and it would
/// have meant either a second key model for one prompt or two lists that
/// could disagree.
///
/// The card's lines are `card_face::Face::lines` — the same printed card
/// the inspector and the desktop show — and the facts under it are
/// `Access::facts`, which are the live costs rather than the printed
/// ones.
fn draw_card(frame: &mut Frame, area: Rect, face: &Face, title: &str, facts: &[String]) {
    let mut text: Vec<Line> = face.lines(false).into_iter().map(Line::from).collect();
    if !facts.is_empty() {
        text.push(Line::from(""));
        for fact in facts {
            text.push(Line::from(Span::styled(fact.clone(), Style::default().fg(Color::Cyan))));
        }
    }
    // Sized to the card, not to the region: a card is a tall narrow
    // thing, and a panel stretched over a wide terminal put four words
    // on each line with the rest empty. The height counts *wrapped*
    // rows, not lines — counting lines clipped the facts off the bottom
    // of a card whose text is a paragraph.
    let width = 56.min(area.width);
    let inner = width.saturating_sub(2).max(1) as usize;
    let rows: usize = text
        .iter()
        .map(|line| {
            let chars = line.spans.iter().map(|span| span.content.chars().count()).sum::<usize>();
            chars.div_ceil(inner).max(1)
        })
        .sum();
    let height = (rows as u16 + 2).min(area.height);
    let panel = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, panel);
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title.to_string())
                .style(Style::default().fg(Color::Yellow)),
        ),
        panel,
    );
}

/// The inspector's list, centred over the board like a modal, with the
/// same highlight as the actions pane.
fn draw_card_picker(frame: &mut Frame, picker: &CardPicker) {
    let area = centered_rect(72, 72, frame.area());
    frame.render_widget(Clear, area);
    let outer = Block::default()
        .borders(Borders::ALL)
        .title("Cards — Up/Down move, Enter opens, Esc backs out, c closes")
        .style(Style::default().fg(Color::Yellow));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    if picker.zones.is_empty() {
        frame.render_widget(Paragraph::new("Nothing to inspect yet."), inner);
        return;
    }
    let [places, cards] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .areas(inner);
    let at_cards = picker.card.is_some();
    let column = |title: &str, rows: Vec<String>, selected: Option<usize>, active: bool| {
        let border = if active { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) };
        let list = List::new(rows.into_iter().map(ListItem::new).collect::<Vec<_>>())
            .block(Block::default().borders(Borders::ALL).title(title.to_string()).border_style(border))
            .highlight_style(if active { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default().add_modifier(Modifier::BOLD) });
        (list, ListState::default().with_selected(selected))
    };
    let (list, mut state) = column("Where", picker.zone_labels(), Some(picker.zone), !at_cards);
    frame.render_stateful_widget(list, places, &mut state);
    let zone_name = picker.current_zone().map(|zone| zone.name.clone()).unwrap_or_default();
    let (list, mut state) = column(&zone_name, picker.card_labels(), picker.card, at_cards);
    frame.render_stateful_widget(list, cards, &mut state);
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

/// The viewer's own side is always the bottom block, the way a table is
/// laid out: your cards nearest you, the opponent's across from you.
/// The Corp block keeps the larger share wherever it sits, because it
/// holds the servers and, during a run, the phase strip; a spectator or
/// a not-yet-connected client sees the Corp on top. Returns the two
/// areas as `(corp, runner)`.
fn board_areas(area: Rect, viewer: Viewer) -> (Rect, Rect) {
    let corp_on_top = !matches!(viewer, Viewer::Player(Side::Corp));
    let (top, bottom) = if corp_on_top { (60, 40) } else { (40, 60) };
    let [top_area, bottom_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(top), Constraint::Percentage(bottom)])
        .areas(area);
    if corp_on_top { (top_area, bottom_area) } else { (bottom_area, top_area) }
}

/// A block's title says whose it is, so a player who has just swapped
/// chairs in a replay — or simply sat down — need not work it out from
/// which block moved.
fn board_title(side: Side, viewer: Viewer) -> String {
    let what = match side {
        Side::Corp => "Corp servers",
        Side::Runner => "Runner rig",
    };
    match viewer {
        Viewer::Player(mine) if mine == side => format!("You — {what}"),
        Viewer::Player(_) => format!("Opponent — {what}"),
        Viewer::Spectator => what.to_string(),
    }
}

fn draw_board(frame: &mut Frame, area: Rect, app: &impl RenderableView) {
    let viewer = app.viewer();
    let (corp_area, runner_area) = board_areas(area, viewer);
    let corp_title = board_title(Side::Corp, viewer);
    let runner_title = board_title(Side::Runner, viewer);

    let Some(view) = app.view() else {
        frame.render_widget(Block::default().borders(Borders::ALL).title(corp_title), corp_area);
        frame.render_widget(Block::default().borders(Borders::ALL).title(runner_title), runner_area);
        return;
    };

    // One map per frame rather than per card: `ActionMap::build` labels
    // every legal action, and the hand and the rig both ask it.
    let actions = app.action_map();
    let mut corp_lines = Vec::new();
    // `Some` only for the viewer's own hand — the masking layer decided
    // that, and this draws exactly what it handed over. Until this line
    // existed the TUI printed the count and nothing else, so a human
    // chose "keep or mulligan" over five cards they could not see, and
    // the only way to learn what was in hand was to read the action list.
    if let Some(cards) = &view.corp.hq_cards {
        corp_lines.push(hand_line(cards, app.registry(), actions.as_ref()));
        corp_lines.push(Line::from(""));
    }
    // Archives, R&D, HQ — always, with their counts — then the remotes,
    // from either seat (`board::table_servers`), so a central is on the
    // same line all game. The engine's order put HQ first and listed a
    // server only once something was installed on it, so every install
    // on an empty central moved the lines under it.
    for server in netrunner_client::board::table_servers(view) {
        corp_lines.push(Line::from(format_server(&server, view, app.registry())));
    }
    if let Some(run) = &view.active_run {
        corp_lines.push(Line::from(""));
        corp_lines.push(run_phase_strip(run.phase));
        corp_lines.extend(format_run(view, run, app.registry()));
    }
    frame.render_widget(
        Paragraph::new(corp_lines).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title(corp_title)),
        corp_area,
    );

    let mut runner_lines = vec![
        Line::from(format!("Grip: {} cards   Stack: {} cards   Heap: {} cards", view.runner.grip_count, view.runner.stack_count, view.runner.heap.len())),
    ];
    if let Some(cards) = &view.runner.grip_cards {
        runner_lines.push(hand_line(cards, app.registry(), actions.as_ref()));
    }
    runner_lines.push(Line::from(""));
    // The rig in its three rows, programs the line nearest the Corp's
    // block from either chair (`board::rig::rows_top_down`) — the same
    // rows the desktop draws. Every row is listed, so the first install
    // of a kind moves no line. Copies nothing tells apart are one entry
    // with a count, as on the desktop (`board::rig::stacked`).
    let chair = if matches!(viewer, Viewer::Player(Side::Corp)) { Side::Corp } else { Side::Runner };
    for (row, stacks) in netrunner_client::board::rig::stacked(view, app.registry(), chair, actions.as_ref()) {
        let cards: Vec<_> = stacks.iter().map(|stack| stack.first()).collect();
        let mut spans = vec![Span::raw(format!("{:<10} ", format!("{}:", row.label())))];
        if cards.is_empty() {
            spans.push(Span::raw("—"));
        }
        for (i, card) in cards.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" · "));
            }
            let counters = counter_label(Some(&card.card), card.counters, app.registry());
            let strength = app.registry().get(&card.card).and_then(|def| def.strength).map(|_| format!("str {}", card.current_strength));
            let host = card.hosted_on_ice.map(|ice| format!("on {}", netrunner_client::board::rig::host_label(view, app.registry(), ice)));
            let facts = [strength.unwrap_or_default(), counters.trim_start_matches(", ").to_string(), host.unwrap_or_default()].into_iter().filter(|f| !f.is_empty()).collect::<Vec<_>>().join(", ");
            let title = match stacks[i].count() {
                1 => card_title(&card.card, app.registry()),
                n => format!("{} ×{n}", card_title(&card.card, app.registry())),
            };
            let label = if facts.is_empty() { title } else { format!("{title} ({facts})") };
            let mood = actions.as_ref().and_then(|map| map.affordance(&Target::Install(card.install_id)));
            // A Trojan's row entry is its ghost: dim unless it can act.
            let style = match mood {
                None if netrunner_client::board::rig::is_ghost(card) => Style::default().add_modifier(Modifier::DIM),
                _ => mood_style(mood),
            };
            spans.push(Span::styled(label, style));
        }
        runner_lines.push(Line::from(spans));
    }
    frame.render_widget(
        Paragraph::new(runner_lines).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title(runner_title)),
        runner_area,
    );
}

/// The viewer's own hand on one wrapped line: `Hand: Hedge Fund
/// (Operation, 5c) · Palisade (Barrier ICE, 3c)`. One line rather than
/// one per card because the Corp area also holds the servers and, during
/// a run, the phase strip, inside 60% of a `Min(10)` board.
fn hand_line(cards: &[CardId], registry: &CardRegistry, actions: Option<&ActionMap>) -> Line<'static> {
    if cards.is_empty() {
        return Line::from("Hand: (empty)");
    }
    let describe = |id: &CardId| match registry.get(id) {
        Some(card) => {
            let kind = match &card.card_type {
                netrunner_core::dsl::CardType::Ice(ice) => format!("{ice:?} ICE"),
                other => format!("{other:?}"),
            };
            format!("{} ({kind}, {}c)", card.title, card.cost)
        }
        None => id.0.clone(),
    };
    // A span per card rather than one string, so each carries its own
    // mood: what the engine will accept on that card right now
    // (`netrunner_client::board::affordance`). The desktop draws the same
    // two moods as a glow; here they are the only colour on the line, so
    // a plain title is a card with nothing to do.
    let mut line = Line::from("Hand: ");
    for (i, id) in cards.iter().enumerate() {
        if i > 0 {
            line.push_span(Span::raw(" · "));
        }
        let mood = actions.and_then(|map| map.affordance(&Target::HandCard(id.clone())));
        line.push_span(Span::styled(describe(id), mood_style(mood)));
    }
    line
}

/// The colour a mood is drawn in: purple for a move at the person's own
/// pace, yellow for a moment that will pass, unstyled for a card the
/// engine offers nothing on. Named colours rather than RGB, because this
/// pane's palette is the terminal's sixteen and a truecolour purple would
/// be the only exception in the file.
fn mood_style(mood: Option<Affordance>) -> Style {
    match mood {
        Some(Affordance::Usable) => Style::default().fg(Color::Magenta),
        Some(Affordance::Conditional) => Style::default().fg(Color::Yellow),
        None => Style::default(),
    }
}

fn format_server(server: &ServerView, view: &ClientView, registry: &CardRegistry) -> String {
    let describe = |card: &netrunner_core::rules::PublicInstalledCard| {
        let rez = if card.rezzed { "rezzed" } else { "unrezzed" };
        let label = card.card.as_ref().map(|id| card_title(id, registry)).unwrap_or_else(|| "???".to_string());
        // `None` means this viewer may not see the count at all (unrezzed,
        // and not theirs), which renders the same as "none placed".
        let counters = counter_label(card.card.as_ref(), card.counters.unwrap_or(0), registry);
        // A Trojan is on its ice here too, as on the desktop's tile
        // (`board::rig::hosted_on`); the rig's line calls it a ghost.
        let hosted: Vec<String> = netrunner_client::board::rig::hosted_on(view, card.install_id).into_iter().map(|trojan| card_title(&trojan.card, registry)).collect();
        let hosts = if hosted.is_empty() { String::new() } else { format!(" [hosts {}]", hosted.join(", ")) };
        if card.advancement_tokens > 0 {
            format!("{label} ({rez}, {} adv{counters}){hosts}", card.advancement_tokens)
        } else {
            format!("{label} ({rez}{counters}){hosts}")
        }
    };
    let cards: Vec<String> = server.ice.iter().chain(server.root.iter()).map(describe).collect();
    let contents = if cards.is_empty() { "(empty)".to_string() } else { cards.join(", ") };
    let count = match server.server {
        ServerId::Archives => format!(" ({})", view.corp.archives.len()),
        ServerId::RnD => format!(" ({})", view.corp.rd_count),
        ServerId::Hq => format!(" ({})", view.corp.hq_count),
        ServerId::Remote(_) => String::new(),
    };
    format!("{}{count}: {contents}", server_label(server.server))
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
/// them directly. *Movement* was listed and never lit until the engine had
/// the phase (Rules Audit item 7): it is where the Runner decides whether
/// to jack out, before the next approach.
fn run_phase_strip(phase: RunPhase) -> Line<'static> {
    const NAMES: [&str; 6] = ["Initiation", "Approach ice", "Encounter ice", "Movement", "Success", "Run ends"];
    let lit = match phase {
        RunPhase::Initiation => 0,
        RunPhase::ApproachIce => 1,
        RunPhase::EncounterIce => 2,
        RunPhase::Movement => 3,
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

/// The run as lines: the server and how far in, then each piece of ice
/// with the one being met marked — and, while it is being encountered,
/// its type line and strength, then its subroutines under it, each
/// struck through when broken, red when it has fired, plain while it is
/// still pending
/// (`board::facts::encounter_subroutines`). The marks are here rather
/// than a keypress away in the card sheet because an encounter is
/// decided in, not read about afterwards.
fn format_run(view: &ClientView, run: &netrunner_core::rules::PublicRunState, registry: &CardRegistry) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(format!("Run on {} (ICE {}/{})", server_label(run.server), run.position, run.ice.len()))];
    let met = netrunner_client::board::encounter_subroutines(view);
    for (index, ice) in run.ice.iter().enumerate() {
        let marker = if index == run.position { ">" } else { " " };
        let rez = if ice.rezzed { "rezzed" } else { "unrezzed" };
        match &ice.identity {
            Some(identity) => lines.push(Line::from(format!("{marker} {} [{rez}, str {}]", card_title(&identity.card, registry), identity.current_strength))),
            None => lines.push(Line::from(format!("{marker} ??? [{rez}]"))),
        }
        let Some(met) = met.as_ref().filter(|met| met.install == ice.install_id) else { continue };
        // The type line and the strength against the printed one, the
        // desktop's encounter panel's words (`Encounter::strength_line`).
        let type_line = met.card.as_ref().and_then(|id| registry.get(id)).and_then(|def| def.type_line.clone());
        lines.push(Line::from(format!("    {}", [type_line, Some(met.strength_line(registry))].into_iter().flatten().collect::<Vec<_>>().join(" · "))));
        for sub in &met.subroutines {
            let (style, mark) = match sub.status {
                SubroutineStatus::Broken => (Style::default().fg(Color::DarkGray).add_modifier(Modifier::CROSSED_OUT), "x"),
                SubroutineStatus::Resolved => (Style::default().fg(Color::Red), "!"),
                SubroutineStatus::Pending => (Style::default(), " "),
            };
            lines.push(Line::from(vec![
                Span::raw(format!("    [{mark}] ")),
                Span::styled(sub.text.clone(), style),
            ]));
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
        "Actions (Up/Down, Enter to act, a to show all, c to read a card, q to quit)".to_string()
    } else {
        "Legal actions (Up/Down, Enter to act, c to read a card, q to quit)".to_string()
    });
    if let Some(notice) = app.notice() {
        title.push_span(Span::styled(format!("  {notice}"), Style::default().fg(Color::Yellow)));
    }
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
pub(crate) fn draw_modal(frame: &mut Frame, modal: &Modal) {
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
    use netrunner_client::actions::describe_action;
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

    /// A Trojan is on its ice here too: the ice's entry says what it
    /// hosts, and the program row's entry is the ghost, naming the host.
    #[test]
    fn a_trojan_is_listed_on_its_ice_and_its_row_entry_names_the_host() {
        use netrunner_core::dsl::CardId;
        use netrunner_core::rules::{InstallId, InstallSlot, PublicInstalledCard, PublicInstalledRunnerCard, ServerId};
        use netrunner_core::view::{build_client_view, ServerView};

        let registry = decks::sample_deck_registry();
        let corp_deck = netrunner_core::decks::by_id("discretion_advised").unwrap().to_deck();
        let runner_deck = netrunner_core::decks::by_id("stolen_goods").unwrap().to_deck();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 3).unwrap();
        let mut view = build_client_view(&state, &registry, Side::Runner);
        let ice = InstallId(9100);
        if !view.corp.servers.iter().any(|s| s.server == ServerId::Hq) {
            view.corp.servers.push(ServerView { server: ServerId::Hq, ice: Vec::new(), root: Vec::new() });
        }
        let hq = view.corp.servers.iter_mut().find(|s| s.server == ServerId::Hq).unwrap();
        hq.ice.push(PublicInstalledCard { install_id: ice, position: 0, server: ServerId::Hq, slot: InstallSlot::Ice, rezzed: true, card: Some(CardId("ice_wall".into())), advancement_tokens: 0, counters: Some(0), seen_by_runner: true });
        view.runner.rig.push(PublicInstalledRunnerCard { card: CardId("botulus".into()), install_id: InstallId(9101), current_strength: 0, hosted_on_ice: Some(ice), hosted_on_program: None, hosted_cards: Vec::new(), hosted_cards_playable: false, counters: 1 });

        let mut ui = LocalUiState::new(registry, Side::Runner);
        ui.begin_decision(view);
        let mut terminal = Terminal::new(TestBackend::new(200, 50)).unwrap();
        terminal.draw(|frame| draw_frame(frame, &ui, None)).unwrap();
        let buffer = terminal.backend().buffer();
        let rows: Vec<String> = (0..buffer.area.height).map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol().to_string()).collect()).collect();
        assert!(rows.iter().any(|row| row.contains("Ice Wall (rezzed) [hosts Botulus]")), "the ice lists its Trojan:\n{}", rows.join("\n"));
        let programs = rows.iter().find(|row| row.contains("Programs:")).unwrap();
        assert!(programs.contains("Botulus (") && programs.contains("on Ice Wall"), "the ghost names its host: {programs}");
    }

    /// The player's own hand is drawn, and only theirs: the same position
    /// rendered from each chair shows that chair's cards and none of the
    /// other's. Before this test the board printed the hand's *count* and
    /// stopped, so the mulligan was decided over five unseen cards.
    #[test]
    fn the_board_draws_the_viewers_hand_and_never_the_opponents() {
        use netrunner_core::view::build_client_view;

        let registry = decks::sample_deck_registry();
        let corp_deck = netrunner_core::decks::by_id("discretion_advised").unwrap().to_deck();
        let runner_deck = netrunner_core::decks::by_id("stolen_goods").unwrap().to_deck();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 3).unwrap();
        assert!(matches!(state.phase, GamePhase::Mulligan(_)), "the opening decision");

        let corp_view = build_client_view(&state, &registry, Side::Corp);
        let corp_hand: Vec<String> = corp_view.corp.hq_cards.clone().expect("the Corp sees HQ").iter().map(|id| card_title(id, &registry)).collect();
        let runner_view = build_client_view(&state, &registry, Side::Runner);
        let runner_hand: Vec<String> =
            runner_view.runner.grip_cards.clone().expect("the Runner sees the grip").iter().map(|id| card_title(id, &registry)).collect();
        assert!(!corp_hand.is_empty() && !runner_hand.is_empty());

        // The buffer as rows, so the test can say which block is *below*
        // which and not only what is on screen.
        let render = |ui: &LocalUiState| -> Vec<String> {
            let mut terminal = Terminal::new(TestBackend::new(160, 50)).unwrap();
            terminal.draw(|frame| draw_frame(frame, ui, None)).unwrap();
            let buffer = terminal.backend().buffer();
            (0..buffer.area.height)
                .map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol().to_string()).collect::<String>())
                .collect()
        };
        let row_of = |rows: &[String], needle: &str| rows.iter().position(|row| row.contains(needle)).unwrap_or_else(|| panic!("{needle:?} is drawn"));
        let all = |rows: &[String]| rows.join("\n");

        let mut as_corp = LocalUiState::new(registry.clone(), Side::Corp);
        as_corp.begin_decision(corp_view);
        let rows = render(&as_corp);
        let rendered = all(&rows);
        assert!(rendered.contains("Hand:"), "the hand line is drawn");
        assert!(rendered.contains(&corp_hand[0]), "the Corp sees its own cards: {}", corp_hand[0]);
        assert!(runner_hand.iter().all(|title| !rendered.contains(title.as_str())), "the Corp never sees the grip");
        // Your side is the bottom block, whichever chair you sit in.
        assert!(row_of(&rows, "You — Corp servers") > row_of(&rows, "Opponent — Runner rig"), "the Corp's own block is below the Runner's");

        let mut as_runner = LocalUiState::new(registry, Side::Runner);
        as_runner.begin_decision(runner_view);
        let rows = render(&as_runner);
        let rendered = all(&rows);
        assert!(rendered.contains(&runner_hand[0]), "the Runner sees its own cards: {}", runner_hand[0]);
        assert!(corp_hand.iter().all(|title| !rendered.contains(title.as_str())), "the Runner never sees HQ");
        assert!(row_of(&rows, "You — Runner rig") > row_of(&rows, "Opponent — Corp servers"), "the Runner's own block is below the Corp's");
        // The rig is three rows, listed before anything is installed, with
        // programs the line nearest the Corp's block from either chair.
        let (programs, hardware, resources) = (row_of(&rows, "Programs:"), row_of(&rows, "Hardware:"), row_of(&rows, "Resources:"));
        assert!(programs < hardware && hardware < resources, "the Runner's chair: {programs} {hardware} {resources}");
        let corp_rows = render(&as_corp);
        let (programs, hardware, resources) = (row_of(&corp_rows, "Programs:"), row_of(&corp_rows, "Hardware:"), row_of(&corp_rows, "Resources:"));
        assert!(resources < hardware && hardware < programs, "the Corp's chair: {resources} {hardware} {programs}");
        // Nothing is installed yet, and the three centrals are listed
        // anyway, Archives to HQ with their counts, from this seat as from
        // the Corp's — the lines a remote would join below.
        let (archives, rnd, hq) = (row_of(&rows, "Archives (0): (empty)"), row_of(&rows, "R&D ("), row_of(&rows, "HQ ("));
        assert!(archives < rnd && rnd < hq, "Archives, R&D, HQ: {archives} {rnd} {hq}");
        assert!(!rendered.contains("HQ: "), "the old count line is gone");
        let rows = render(&as_corp);
        assert!(row_of(&rows, "Archives (0)") < row_of(&rows, "R&D (") && row_of(&rows, "R&D (") < row_of(&rows, "HQ ("), "the same order from the Corp's seat");
    }

    /// `c` lists the places the viewer may look — their hand, never the
    /// other's — Enter steps into one and then reads a card, and Esc backs
    /// out one level at a time so the next card is one keypress away. The
    /// list is built from the view, so the mask that decides what is drawn
    /// decides what can be inspected.
    #[test]
    fn the_inspector_lists_places_then_cards_and_escape_backs_out_one_level() {
        use netrunner_core::view::build_client_view;

        let registry = decks::sample_deck_registry();
        let corp_deck = netrunner_core::decks::by_id("discretion_advised").unwrap().to_deck();
        let runner_deck = netrunner_core::decks::by_id("stolen_goods").unwrap().to_deck();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 3).unwrap();
        let view = build_client_view(&state, &registry, Side::Corp);
        let hand = view.corp.hq_cards.clone().expect("the Corp sees HQ");

        let mut ui = LocalUiState::new(registry.clone(), Side::Corp);
        assert!(!ui.inspector_key(KeyCode::Char('c')), "nothing to inspect before a view arrives");
        ui.begin_decision(view);
        assert!(ui.inspector_key(KeyCode::Char('c')));
        let picker = ui.card_picker.clone().expect("the picker opened");
        let places = picker.zone_labels();
        assert_eq!(places[0], format!("Hand (HQ) ({})", hand.len()), "{places:?}");
        assert!(places.iter().all(|p| !p.starts_with("Hand (grip)")), "the Runner's hand is not the Corp's to read");
        assert!(places.iter().any(|p| p.starts_with("Identities (2)")), "{places:?}");
        assert!(picker.card.is_none(), "the reader starts at the places");

        let mut terminal = Terminal::new(TestBackend::new(160, 50)).unwrap();
        terminal.draw(|frame| draw_frame(frame, &ui, None)).unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Where") && rendered.contains("Esc backs out"), "the two columns are drawn");

        // Enter steps into the hand; Down and Enter read its second card.
        assert!(ui.inspector_key(KeyCode::Enter));
        assert_eq!(ui.card_picker.as_ref().unwrap().card, Some(0));
        assert!(ui.inspector_key(KeyCode::Down));
        assert!(ui.inspector_key(KeyCode::Enter));
        let modal = ui.modal.clone().expect("the card's text is up");
        let card = registry.get(&hand[1]).unwrap();
        assert_eq!(modal.title, card.title);
        let text = card.printed_text.as_deref().expect("every sample-deck card has printed text");
        assert!(modal.body.contains(text.lines().next().unwrap()), "{}", modal.body);
        assert!(ui.card_picker.is_some(), "the list stays open behind the text");
        terminal.draw(|frame| draw_frame(frame, &ui, None)).unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Esc to close"), "the modal is drawn over the list");

        // Esc: text -> the same card in the list -> the places -> closed.
        assert!(ui.inspector_key(KeyCode::Esc));
        assert!(ui.modal.is_none());
        assert_eq!(ui.card_picker.as_ref().unwrap().card, Some(1), "back on the card that was read");
        assert!(ui.inspector_key(KeyCode::Esc));
        assert_eq!(ui.card_picker.as_ref().unwrap().card, None, "back at the places");
        assert!(ui.inspector_key(KeyCode::Esc));
        assert!(ui.card_picker.is_none(), "closed");

        // `c` from a card's text closes the whole inspector at once.
        ui.inspector_key(KeyCode::Char('c'));
        ui.inspector_key(KeyCode::Enter);
        ui.inspector_key(KeyCode::Enter);
        assert!(ui.modal.is_some());
        ui.inspector_key(KeyCode::Char('c'));
        assert!(ui.modal.is_none() && ui.card_picker.is_none());
    }

    /// Mid-game the places fill up — installs, Archives, the heap — and
    /// every card the inspector lists has printed text to show. Driven by
    /// two random agents through the real `Session` so the view is one a
    /// game actually reaches, not a fixture.
    #[test]
    fn mid_game_the_inspector_reaches_the_places_that_have_filled_up() {
        use netrunner_bots::RandomAgent;

        let registry = decks::sample_deck_registry();
        let corp_deck = netrunner_core::decks::by_id("discretion_advised").unwrap().to_deck();
        let runner_deck = netrunner_core::decks::by_id("stolen_goods").unwrap().to_deck();
        let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 11).unwrap();
        let mut session = Session::new(
            state,
            registry.clone(),
            Seat::Agent(Box::new(RandomAgent::new(1))),
            Seat::Agent(Box::new(RandomAgent::new(2))),
        );
        for _ in 0..400 {
            match session.step() {
                SessionStep::Applied { .. } | SessionStep::Awaiting { .. } => {}
                SessionStep::Ended { .. } | SessionStep::Stalled(_) => break,
            }
        }
        assert!(session.state().turn >= 4, "the game got under way: turn {}", session.state().turn);

        for side in [Side::Corp, Side::Runner] {
            let picker = CardPicker::open(&session.view_for(side), &registry);
            let places = picker.zone_labels();
            let own_hand = if side == Side::Corp { "Hand (HQ)" } else { "Hand (grip)" };
            let other_hand = if side == Side::Corp { "Hand (grip)" } else { "Hand (HQ)" };
            assert!(places[0].starts_with(own_hand), "{side:?}: the own hand is always first, even empty: {places:?}");
            assert!(places.iter().all(|p| !p.starts_with(other_hand)), "{side:?} must not see the other hand: {places:?}");
            assert!(
                places.iter().any(|p| p.starts_with("Corp servers") || p.starts_with("Archives") || p.starts_with("Heap") || p.starts_with("Runner rig")),
                "{side:?}: after {} turns the table should have something on it: {places:?}",
                session.state().turn
            );
            for zone in &picker.zones {
                for (label, id) in &zone.cards {
                    let card = registry.get(id).unwrap_or_else(|| panic!("{label} is not in the registry"));
                    assert!(card.printed_text.is_some(), "{label} has no printed text");
                    // Against the *rendered* first line, not the raw one:
                    // the modal draws printed symbols as their terminal
                    // stand-ins (`¢`, `»`), so a card whose text holds a
                    // `[credit]` token never matches the JSON verbatim.
                    let rendered = netrunner_client::card_face::Face::of(card).body_text(false);
                    assert!(card_modal(id, &registry).body.contains(rendered.lines().next().unwrap()), "{label}");
                }
            }
        }
    }

    /// A parked choice is labelled by what each option does and the pane
    /// says which card is asking — "Option 0 | Option 1" was a menu with
    /// no words on it, for a decision the card text explains.
    #[test]
    fn a_parked_choice_is_labelled_by_what_it_does_and_who_asks() {
        use netrunner_core::dsl::{CardId, Effect};
        use netrunner_core::rules::{PendingChoiceResume, PendingDecision};
        use netrunner_core::view::build_client_view;

        let registry = decks::sample_deck_registry();
        let corp_deck = netrunner_core::decks::by_id("discretion_advised").unwrap().to_deck();
        let runner_deck = netrunner_core::decks::by_id("stolen_goods").unwrap().to_deck();
        let (mut state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 3).unwrap();
        // Bigger Picture's choice, parked the way `Effect::PresentChoice`
        // parks it: give a tag, or take the Runner's credits per tag.
        // Parked the way the engine parks it, clauses included: the
        // label is the card's own words, and the empty clause is the
        // "may" declined.
        state.pending_decision = Some(PendingDecision::ChooseEffect {
            chooser: Side::Corp,
            options: vec![Effect::GiveTags(1), Effect::Sequence(vec![])],
            option_texts: vec!["Give the Runner 1 tag.".to_string(), String::new()],
            source_card: Some(CardId("bigger_picture".to_string())),
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });
        let view = build_client_view(&state, &registry, Side::Corp);

        assert_eq!(describe_action(&PlayerAction::ResolvePendingChoice { option_index: 0 }, &registry, Some(&view)), "Give the Runner 1 tag");
        assert_eq!(describe_action(&PlayerAction::ResolvePendingChoice { option_index: 1 }, &registry, Some(&view)), "Do not");
        assert_eq!(describe_action(&PlayerAction::ResolvePendingChoice { option_index: 0 }, &registry, None), "Choose option 1", "without a view the index is all there is");
        // A card with no clauses linked falls back to the prose rendering.
        let mut bare = state.clone();
        if let Some(PendingDecision::ChooseEffect { option_texts, .. }) = &mut bare.pending_decision {
            option_texts.clear();
        }
        let bare_view = build_client_view(&bare, &registry, Side::Corp);
        assert_eq!(describe_action(&PlayerAction::ResolvePendingChoice { option_index: 1 }, &registry, Some(&bare_view)), "Do nothing");
        assert_eq!(crate::prose::decision_prompt(&view, &registry).as_deref(), Some("Bigger Picture asks — choose one"));

        let mut ui = LocalUiState::new(registry, Side::Corp);
        ui.begin_decision(view);
        let mut terminal = Terminal::new(TestBackend::new(160, 50)).unwrap();
        terminal.draw(|frame| draw_frame(frame, &ui, None)).unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Bigger Picture asks"), "the pane is titled by the asking card");
    }

    /// An access draws the card, not its name: the person's report was
    /// that all they got was a title. The panel sits over the board and
    /// the actions pane keeps its own list, so the card and the choices
    /// are both on screen.
    #[test]
    fn an_access_draws_the_card_over_the_board_and_leaves_the_actions_pane() {
        use netrunner_core::rules::{MaskedZone, PublicAccessPhase, PublicAccessState, PublicRunState, RunPhase, ServerId};
        use netrunner_core::view::build_client_view;

        let registry = decks::sample_deck_registry();
        let corp_deck = netrunner_core::decks::by_id("discretion_advised").unwrap().to_deck();
        let runner_deck = netrunner_core::decks::by_id("stolen_goods").unwrap().to_deck();
        let (mut state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 3).unwrap();
        state.phase = GamePhase::Action(Side::Runner);
        let card = netrunner_core::dsl::CardId("send_a_message".to_string());
        let definition = registry.get(&card).expect("an agenda in the pool");

        let mut view = build_client_view(&state, &registry, Side::Runner);
        view.active_run = Some(PublicRunState {
            server: ServerId::Hq,
            phase: RunPhase::AccessingCard,
            ice: Vec::new(),
            position: 0,
            access_state: Some(PublicAccessState {
                server: ServerId::Hq,
                candidates: Vec::new(),
                from_zone: 0,
                resolved_cards: MaskedZone::Hidden { count: 0 },
                pending_install: None,
                phase: PublicAccessPhase::PendingChoice { card: Some(card.clone()), trash_cost: None, mandatory_steal: true, steal_cost: None },
            }),
            jack_out_permitted: false,
            declared_successful: false,
            bad_publicity_credits: 0,
            bonus_run_credits: 0,
            redirect_on_approach: None,
        });
        view.legal_actions = vec![PlayerAction::StealAgenda { card_id: card.clone() }];

        let mut ui = LocalUiState::new(registry.clone(), Side::Runner);
        ui.begin_decision(view);
        let mut terminal = Terminal::new(TestBackend::new(160, 50)).unwrap();
        terminal.draw(|frame| draw_frame(frame, &ui, None)).unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());

        assert!(rendered.contains("Accessing Send a Message"), "the panel is titled by the card");
        assert!(rendered.contains("Advancement"), "the printed numbers are on screen: {rendered}");
        assert!(rendered.contains("must be stolen"), "the live fact is under the card");
        // The card's own words, from the top of its printed text. Only
        // the opening is asserted: the panel wraps, so a whole printed
        // line is split across buffer rows.
        let printed = definition.printed_text.as_deref().unwrap();
        let opening: String = printed.split_whitespace().take(3).collect::<Vec<_>>().join(" ");
        assert!(rendered.contains(&opening), "the card's own words are shown: {opening}");
        assert!(rendered.contains("Steal Send a Message"), "the actions pane still lists the choice");
    }

    /// A card-selection prompt lists its cards by name — the person's
    /// report was a list of `Toggle selection of card N` — with a second
    /// copy folded into the first copy's row, and the pane says what is
    /// chosen. Once the one card allowed is chosen, the list is the confirm
    /// and the way back: "Select a different card".
    #[test]
    fn a_card_selection_lists_its_cards_by_name_and_says_what_is_chosen() {
        use netrunner_core::dsl::{CardFilter, CardZoneRef};
        use netrunner_core::rules::{PendingChoiceResume, PendingDecision};
        use netrunner_core::view::build_client_view;

        let registry = decks::sample_deck_registry();
        let corp_deck = netrunner_core::decks::by_id("discretion_advised").unwrap().to_deck();
        let runner_deck = netrunner_core::decks::by_id("stolen_goods").unwrap().to_deck();
        let (mut state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, 3).unwrap();
        state.phase = GamePhase::Action(Side::Corp);
        let first = state.corp.hq[0].clone();
        let other = state.corp.hq.iter().find(|c| **c != first).cloned().expect("an opening hand of two titles");
        state.corp.hq = vec![first.clone(), first.clone(), other.clone()];
        let title = |id: &netrunner_core::dsl::CardId| registry.get(id).unwrap().title.clone();
        let decision = |selected: Vec<usize>| PendingDecision::ChooseCards {
            side: Side::Corp,
            source: CardZoneRef::OwnHq,
            filter: CardFilter::Any,
            min: 1,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: None,
            selected,
            source_card: None,
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        };

        state.pending_decision = Some(decision(Vec::new()));
        let mut ui = LocalUiState::new(registry.clone(), Side::Corp);
        ui.begin_decision(build_client_view(&state, &registry, Side::Corp));
        assert_eq!(ui.legal_action_labels(), vec![format!("Select {}", title(&first)), format!("Select {}", title(&other))], "two copies, one row");
        let mut terminal = Terminal::new(TestBackend::new(160, 50)).unwrap();
        terminal.draw(|frame| draw_frame(frame, &ui, None)).unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("Nothing selected yet"), "the pane says nothing is chosen");
        assert!(!rendered.contains("Toggle selection"));
        // The card under the cursor is drawn over the board, its own words
        // and all: a choice between cards is not made from their names.
        let printed = registry.get(&first).unwrap().printed_text.clone().unwrap_or_default();
        // Its first plain words: a `[subroutine]` is drawn as a glyph.
        let opening: String = printed.split_whitespace().skip_while(|w| w.contains('[')).take(3).collect::<Vec<_>>().join(" ");
        assert!(rendered.contains(&opening), "the highlighted card's text is shown: {opening}");

        state.pending_decision = Some(decision(vec![2]));
        ui.begin_decision(build_client_view(&state, &registry, Side::Corp));
        assert_eq!(ui.legal_action_labels(), vec![format!("Confirm {}", title(&other)), "Select a different card".to_string()]);
        assert_eq!(ui.actions_title(), Some(format!("choose 1 card from HQ (Selected: {})", title(&other))));
    }

    /// The split follows the viewer: the Corp block keeps the larger share
    /// wherever it sits, and a spectator sees the Corp on top.
    #[test]
    fn the_board_puts_the_viewers_side_at_the_bottom_and_gives_the_corp_the_room() {
        let area = Rect::new(0, 0, 100, 30);
        let (corp, runner) = board_areas(area, Viewer::Player(Side::Corp));
        assert!(corp.y > runner.y, "the Corp player's block is below");
        assert!(corp.height > runner.height, "and still the larger one");
        let (corp, runner) = board_areas(area, Viewer::Player(Side::Runner));
        assert!(runner.y > corp.y, "the Runner player's block is below");
        assert!(corp.height > runner.height);
        let (corp, runner) = board_areas(area, Viewer::Spectator);
        assert!(corp.y < runner.y, "a spectator sees the Corp on top");
        assert_eq!(board_title(Side::Corp, Viewer::Spectator), "Corp servers");
        assert_eq!(board_title(Side::Runner, Viewer::Player(Side::Runner)), "You — Runner rig");
        assert_eq!(board_title(Side::Runner, Viewer::Player(Side::Corp)), "Opponent — Runner rig");
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
        let header = MatchRecordHeader { seed: 5, corp_deck: corp.to_deck(), runner_deck: runner.to_deck(), rules: MatchRules::default(), bot: None, order: Default::default() };
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

    /// Mid-encounter with Wall of Static and a Corroder: the route is a
    /// row after the actions, with its price, and Enter on it hands back
    /// its first step — the pump — and leaves the route running for
    /// `drive_local`, which takes the break on the next view.
    #[test]
    fn a_route_through_the_ice_is_a_row_after_the_actions() {
        use netrunner_core::dsl::{CardId, CardType};
        use netrunner_core::rules::{
            apply_action, EncounteredSubroutine, InstallId, InstallSlot, InstalledCard, InstalledRunnerCard, PaidAbilityWindow, RunIce,
            RunPhase, RunState, ServerId, SubroutineStatus, WindowCheckpoint,
        };
        use netrunner_core::view::build_client_view;

        let registry = decks::sample_deck_registry();
        let wall = registry.get(&CardId("wall_of_static".to_string())).unwrap().clone();
        let CardType::Ice(ice_type) = wall.card_type else { panic!() };
        let mut state = GameState::new(1);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.credits.0 = 5;
        state.runner.rig.push(InstalledRunnerCard { card: CardId("corroder".to_string()), install_id: InstallId(100), base_strength: 2, ..Default::default() });
        state.corp.installed.push(InstalledCard { install_id: InstallId(1), card: wall.id.clone(), server: ServerId::Hq, slot: InstallSlot::Ice, rezzed: true, ..Default::default() });
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce {
                card_id: wall.id.clone(),
                install_id: InstallId(1),
                ice_type,
                subroutines: wall.subroutines.iter().enumerate().map(|(id, sub)| EncounteredSubroutine { id, definition: sub.clone(), status: SubroutineStatus::Pending }).collect(),
                rezzed: true,
            }],
            ..Default::default()
        });
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            checkpoint: WindowCheckpoint::Run,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
        });

        // The board itself says the state of the encounter: the wall's
        // subroutine is listed under it, unticked, before anything is
        // broken.
        let board = |ui: &LocalUiState| -> String {
            let mut terminal = Terminal::new(TestBackend::new(160, 50)).unwrap();
            terminal.draw(|frame| draw_frame(frame, ui, None)).unwrap();
            let buffer = terminal.backend().buffer();
            (0..buffer.area.height)
                .map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol().to_string()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n")
        };

        let mut ui = LocalUiState::new(registry.clone(), Side::Runner);
        ui.begin_decision(build_client_view(&state, &registry, Side::Runner));
        assert!(board(&ui).contains("[ ] End the run"), "the pending subroutine is on the board:\n{}", board(&ui));
        assert!(board(&ui).contains("Ice: Barrier · Strength 3"), "the type line and the strength are under the ice:\n{}", board(&ui));
        let labels = ui.legal_action_labels();
        assert_eq!(labels.last().map(String::as_str), Some("Break Wall of Static with Corroder · 2 credits"));
        ui.selected = labels.len() - 1;
        assert_eq!(ui.selected_action(), None, "a route is not an action");

        let pump = ui.start_break().expect("the route's first step");
        state = apply_action(&state, &registry, pump).unwrap().0;
        state = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).unwrap().0;
        let view = build_client_view(&state, &registry, Side::Runner);
        let step = ui.continue_break(&view).expect("still on route").expect("the break");
        state = apply_action(&state, &registry, step).unwrap().0;
        assert_eq!(state.runner.resources.credits.0, 3);
        let view = build_client_view(&state, &registry, Side::Runner);
        assert_eq!(ui.continue_break(&view), Ok(None), "done");
        assert!(ui.breaking.is_none());

        // And once the route has run, the same line is ticked — the
        // state of the encounter read off the board, not the log.
        ui.begin_decision(view);
        assert!(board(&ui).contains("[x] End the run"), "the broken subroutine is ticked:\n{}", board(&ui));
    }
}
