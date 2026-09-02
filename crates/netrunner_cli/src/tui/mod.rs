pub mod layout;

use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardId, CounterKind};
use netrunner_core::rules::{get_action_mask, ActionSpace, GamePhase, GameState, PlayerAction, ServerId, Side};
use netrunner_core::view::{ClientView, ServerView};
use netrunner_server::ServerMessage;
use netrunner_session::{GameEndReason, Seat, Session, SessionStep, StallReason};

use crate::app::{describe_action, push_log_line, App, RenderableView};
use crate::bots;
use crate::config::{Config, Mode};
use crate::decks;
use crate::remote;

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
    // The wire protocol never transmits a `CardRegistry` — every entry
    // point in this repo (headless, local TUI) already agrees on this one
    // fixed Kate-vs-HB matchup out of band, so the remote client just
    // builds the identical registry locally to resolve card titles.
    let registry = decks::kate_vs_hb_registry();
    let (tx, mut rx) = remote::connect_remote(&config.server, config.side.map(Into::into)).await?;

    let human_side = loop {
        match rx.recv().await {
            Some(ServerMessage::MatchJoined { assigned_side, .. }) => break assigned_side,
            Some(_) => continue,
            None => return Err("server closed the connection before assigning a seat".into()),
        }
    };

    let mut app = App::new(registry, human_side, tx, rx);

    let mut terminal = ratatui::init();
    let result = run_event_loop(&mut terminal, &mut app);
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
    let (bot_seat, mut indexed_bot) = build_bot_seat(bot_kind, bot_side, seed.wrapping_add(1), &config.model)?;

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
) -> Result<(Seat, Option<Box<dyn netrunner_bots::Agent>>), String> {
    match kind {
        crate::config::BotKind::Onnx => Ok((Seat::External, Some(bots::make_driver(kind, side, seed, DEFAULT_SIMULATIONS, model)?))),
        _ => {
            let agent = bots::make_agent_with_model(kind, side, seed, DEFAULT_SIMULATIONS, model)?
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
        // `run` swallows the individual `Applied` steps a bot seat
        // resolves, so mark the log first and replay the difference —
        // the idiom that replaced `SinglePlayerSession::with_observer`.
        let mark = session.history().len();
        let step = session.run();
        for entry in &session.history().entries()[mark..] {
            push_log_line(&mut ui.action_log, entry, &ui.registry, ui.view.as_ref());
        }

        match step {
            SessionStep::Awaiting { side, view } if side == human_side => {
                ui.begin_decision(*view);
                if prompt_human(terminal, session, ui)? {
                    return Ok(());
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
                session.submit(action).map_err(|error| format!("the {side:?} policy chose an action the engine rejected: {error:?}"))?;
            }
            SessionStep::Ended { winner, reason } => {
                ui.finish(session.view_for(human_side));
                return show_game_over(terminal, ui, winner, reason);
            }
            SessionStep::Stalled(reason) => {
                return Err(match reason {
                    StallReason::BudgetExhausted => "match ended without reaching GameOver (step budget exhausted)".into(),
                    StallReason::NoCurrentActor => "match stalled: no side has a decision pending".into(),
                    StallReason::NoLegalActions { side } => {
                        format!("match deadlocked: {side:?} has priority but no legal action").into()
                    }
                });
            }
            SessionStep::Applied { .. } => unreachable!("`run` only returns once it can no longer apply"),
        }
    }
}

/// Renders the current position and blocks on keyboard input until the
/// human submits a legal choice. Returns `true` if they quit instead.
///
/// `mask`'s `true` positions are paired with the `PlayerAction`s
/// `ActionSpace::action_at` decodes them to — except that the human seat
/// now works straight off `view.legal_actions`, which is already the
/// per-side filtered list, with no `ActionSpace` round trip at all.
fn prompt_human(
    terminal: &mut ratatui::DefaultTerminal,
    session: &mut Session,
    ui: &mut LocalUiState,
) -> Result<bool, Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|frame| draw_frame(frame, ui, None))?;

        if event::poll(Duration::from_millis(100))?
            && let Ok(Event::Key(key)) = event::read()
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
                KeyCode::Up | KeyCode::Char('k') => ui.move_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => ui.move_selection(1),
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some(action) = ui.selected_action() {
                        session.submit(action)?;
                        return Ok(false);
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
}

impl LocalUiState {
    fn new(registry: CardRegistry, human_side: Side) -> Self {
        Self { registry, human_side, view: None, selected: 0, action_log: Vec::new() }
    }

    /// Installs the view the session just handed us for a fresh human
    /// decision, resetting the highlight.
    fn begin_decision(&mut self, view: ClientView) {
        self.view = Some(view);
        self.selected = 0;
    }

    /// Final board for the game-over screen: no decision is pending, so
    /// nothing is selectable.
    fn finish(&mut self, view: ClientView) {
        self.view = Some(view);
        self.selected = 0;
    }

    fn legal_actions(&self) -> &[PlayerAction] {
        self.view.as_ref().map_or(&[], |view| view.legal_actions.as_slice())
    }

    fn selected_action(&self) -> Option<PlayerAction> {
        self.legal_actions().get(self.selected).cloned()
    }

    fn move_selection(&mut self, delta: i32) {
        let len = self.legal_actions().len();
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

    fn human_side(&self) -> Side {
        self.human_side
    }

    fn view(&self) -> Option<&ClientView> {
        self.view.as_ref()
    }

    fn selected(&self) -> usize {
        self.selected
    }

    fn legal_action_labels(&self) -> Vec<String> {
        self.legal_actions().iter().map(|action| describe_action(action, &self.registry, self.view.as_ref())).collect()
    }

    fn action_log(&self) -> &[String] {
        &self.action_log
    }
}

fn run_event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    while !app.should_quit {
        app.drain_messages();
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
    let regions = layout::build_layout_with_log(frame.area());
    draw_header(frame, regions.header, ui);
    draw_board(frame, regions.board, ui);
    draw_actions(frame, regions.actions, ui);
    draw_action_log(frame, regions.log, ui.action_log());
    if let Some((winner, reason)) = game_over {
        draw_game_over_modal(frame, winner, reason);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &impl RenderableView) {
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
        "Turn {} | Phase: {phase_label} | You: {:?} | Corp: {}c {} AP:{} BP:{} | Runner: {}c {} Tags:{} MU:{} AP:{}",
        view.turn,
        app.human_side(),
        view.corp.credits,
        click_pool(view.corp.clicks, CORP_MAX_CLICKS),
        view.corp.agenda_points,
        view.corp.bad_publicity,
        view.runner.credits,
        click_pool(view.runner.clicks, RUNNER_MAX_CLICKS),
        view.runner.tags,
        view.runner.memory_units,
        view.runner.agenda_points,
    );
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

fn format_run(run: &netrunner_core::rules::PublicRunState, registry: &CardRegistry) -> Vec<String> {
    let mut lines = vec![format!("Run on {} — {:?} (ICE {}/{})", server_label(run.server), run.phase, run.position, run.ice.len())];
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
    let title = if labels.is_empty() { "Waiting for the other side..." } else { "Legal actions (Up/Down, Enter to act, q to quit)" };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_action_log(frame: &mut Frame, area: Rect, log: &[String]) {
    let visible = log.iter().rev().take(10).rev();
    let items: Vec<ListItem> = visible.map(|line| ListItem::new(line.clone())).collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Match log"));
    frame.render_widget(list, area);
}

fn draw_game_over_modal(frame: &mut Frame, winner: Side, reason: GameEndReason) {
    let area = centered_rect(40, 20, frame.area());
    let text = vec![
        Line::from(Span::styled(format!("{winner:?} wins! ({reason:?})"), Style::default().add_modifier(Modifier::BOLD))),
        Line::from("Press q to quit."),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text).alignment(Alignment::Center).block(Block::default().borders(Borders::ALL).title("Game over").style(Style::default().fg(Color::Yellow))),
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
