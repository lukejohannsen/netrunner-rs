pub mod layout;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardId, CounterKind};
use netrunner_core::rules::{ActionSpace, GameEvent, GamePhase, GameState, PlayerAction, ServerId, Side};
use netrunner_core::view::{build_client_view, ClientView, ServerView};
use netrunner_server::{classify_end_reason, GameEndReason, ServerMessage};
use netrunner_single_player::{HistoryEntry, HumanPromptDriver, PlayerDriver, SinglePlayerSession};

use crate::app::{describe_action, App, RenderableView};
use crate::bots;
use crate::config::{Config, Mode};
use crate::decks;
use crate::remote;

const CORP_MAX_CLICKS: u32 = 3;
const RUNNER_MAX_CLICKS: u32 = 4;

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

/// Local, offline human-vs-bot play via `netrunner_single_player::
/// SinglePlayerSession` — no `MatchSession`, no channel, no background
/// task. Runs entirely synchronously on this thread: `SinglePlayerSession::
/// run` blocks until `GameOver`, so the human's own input/render loop lives
/// inside the `HumanPromptDriver` callback (`prompt_human`), and bot moves
/// in between are narrated live via `with_observer` into `LocalUiState::
/// action_log` — see this module's plan-time doc notes on why the board
/// itself only redraws at human decision points (the observer only gets
/// `&HistoryEntry`, not the resulting `GameState`).
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
    let (corp_deck, runner_deck) = decks::sample_decks(&config.corp_deck, &config.runner_deck)?;
    let seed = config.seed.unwrap_or_else(rand::random);
    let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;

    let bot_side = human_side.other();
    let bot_kind = if human_side == Side::Corp { config.runner } else { config.corp };
    let bot_driver: Box<dyn PlayerDriver> =
        bots::make_driver(bot_kind, bot_side, seed.wrapping_add(1), &config.model)?;

    let ui = Rc::new(RefCell::new(LocalUiState::new(registry.clone(), human_side)));
    let terminal = Rc::new(RefCell::new(ratatui::init()));

    let human_driver: Box<dyn PlayerDriver> = {
        let ui = Rc::clone(&ui);
        let terminal = Rc::clone(&terminal);
        Box::new(HumanPromptDriver::new(move |state: &GameState, registry: &CardRegistry, mask: &[bool]| {
            prompt_human(&ui, &terminal, state, registry, mask)
        }))
    };

    let (corp_driver, runner_driver) = match human_side {
        Side::Corp => (human_driver, bot_driver),
        Side::Runner => (bot_driver, human_driver),
    };

    let observer_ui = Rc::clone(&ui);
    let session = SinglePlayerSession::new(state, registry.clone(), corp_driver, runner_driver)
        .with_observer(move |entry| observer_ui.borrow_mut().push_log_line(entry));

    let (final_state, _history) = session.run();

    let winner = match final_state.phase {
        GamePhase::GameOver(winner) => winner,
        _ => {
            ratatui::restore();
            return Err("match ended without reaching GameOver (step budget exhausted)".into());
        }
    };
    let reason = classify_end_reason(&ui.borrow().last_events, winner, &final_state);

    {
        let mut ui_mut = ui.borrow_mut();
        ui_mut.view = Some(build_client_view(&final_state, &registry, human_side));
        ui_mut.legal_actions_cache.clear();
    }
    terminal.borrow_mut().draw(|frame| draw_local(frame, &ui.borrow(), Some((winner, reason))))?;

    // Block on one final keypress before restoring the terminal, so the
    // summary modal stays visible until the player is done reading it.
    loop {
        if event::poll(Duration::from_millis(100))?
            && let Ok(Event::Key(key)) = event::read()
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        {
            break;
        }
    }
    ratatui::restore();
    Ok(())
}

/// The `HumanPromptDriver` callback body for local play: renders the
/// current position and blocks on keyboard input until the human submits a
/// legal choice, returning its `ActionSpace` index. `mask`'s `true`
/// positions are paired with the `PlayerAction` `ActionSpace::action_at`
/// decodes them to (guaranteed `Some` for every masked-true index, per
/// `get_action_mask`'s own construction) — no separate `index_of` round
/// trip needed, since the index is already in hand from `mask`'s
/// enumeration.
///
/// `.expect`s on I/O here rather than propagating an error, since
/// `HumanPromptDriver`'s callback signature returns a bare `usize` with no
/// room for a `Result` — a broken terminal is treated as unrecoverable,
/// same as `run_event_loop`'s `?` would treat it, just via panic instead of
/// an `Err` return.
fn prompt_human(
    ui: &Rc<RefCell<LocalUiState>>,
    terminal: &Rc<RefCell<ratatui::DefaultTerminal>>,
    state: &GameState,
    registry: &CardRegistry,
    mask: &[bool],
) -> usize {
    let human_side = ui.borrow().human_side;
    let view = build_client_view(state, registry, human_side);
    let legal_actions_cache: Vec<(usize, PlayerAction)> = mask
        .iter()
        .enumerate()
        .filter_map(|(index, &legal)| {
            if !legal {
                return None;
            }
            ActionSpace::action_at(state, index).map(|action| (index, action))
        })
        .collect();

    {
        let mut ui_mut = ui.borrow_mut();
        ui_mut.view = Some(view);
        ui_mut.legal_actions_cache = legal_actions_cache;
        ui_mut.selected = 0;
    }

    loop {
        terminal.borrow_mut().draw(|frame| draw_local(frame, &ui.borrow(), None)).expect("failed to draw the local TUI frame");

        if event::poll(Duration::from_millis(100)).unwrap_or(false)
            && let Ok(Event::Key(key)) = event::read()
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    ratatui::restore();
                    std::process::exit(0);
                }
                KeyCode::Up | KeyCode::Char('k') => ui.borrow_mut().move_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => ui.borrow_mut().move_selection(1),
                KeyCode::Enter | KeyCode::Char(' ') => {
                    let ui_ref = ui.borrow();
                    if let Some((index, _)) = ui_ref.legal_actions_cache.get(ui_ref.selected) {
                        return *index;
                    }
                }
                _ => {}
            }
        }
    }
}

/// Local-play TUI state: the same renderable data `App` carries (registry,
/// human side, current masked `ClientView`, selection), plus what only the
/// local `SinglePlayerSession` path has — `legal_actions_cache` (index-
/// paired, so `Enter` can resolve straight back to a `usize`), a running
/// `action_log` narrating every resolved action (human or bot) via
/// `SinglePlayerSession::with_observer`, and `last_events`, the most
/// recently resolved action's events (needed for `classify_end_reason`'s
/// end-of-match summary once the session finishes).
struct LocalUiState {
    registry: CardRegistry,
    human_side: Side,
    view: Option<ClientView>,
    legal_actions_cache: Vec<(usize, PlayerAction)>,
    selected: usize,
    action_log: Vec<String>,
    last_events: Vec<GameEvent>,
}

const MAX_LOG_LINES: usize = 200;

impl LocalUiState {
    fn new(registry: CardRegistry, human_side: Side) -> Self {
        Self {
            registry,
            human_side,
            view: None,
            legal_actions_cache: Vec::new(),
            selected: 0,
            action_log: Vec::new(),
            last_events: Vec::new(),
        }
    }

    fn push_log_line(&mut self, entry: &HistoryEntry) {
        self.action_log.push(format!("[turn {}] {:?}: {}", entry.turn_number, entry.side, describe_action(&entry.action, &self.registry)));
        if self.action_log.len() > MAX_LOG_LINES {
            let excess = self.action_log.len() - MAX_LOG_LINES;
            self.action_log.drain(0..excess);
        }
        self.last_events = entry.events.clone();
    }

    fn move_selection(&mut self, delta: i32) {
        let len = self.legal_actions_cache.len();
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
        self.legal_actions_cache.iter().map(|(_, action)| describe_action(action, &self.registry)).collect()
    }
}

fn run_event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    while !app.should_quit {
        app.drain_messages();
        terminal.draw(|frame| draw(frame, app))?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            app.handle_key(key);
        }
    }
    Ok(())
}

fn draw(frame: &mut Frame, app: &App) {
    let regions = layout::build_layout(frame.area());
    draw_header(frame, regions.header, app);
    draw_board(frame, regions.board, app);
    draw_actions(frame, regions.actions, app);
    if let Some((winner, reason)) = app.game_ended {
        draw_game_over_modal(frame, winner, reason);
    }
}

fn draw_local(frame: &mut Frame, ui: &LocalUiState, game_over: Option<(Side, GameEndReason)>) {
    let regions = layout::build_layout_with_log(frame.area());
    draw_header(frame, regions.header, ui);
    draw_board(frame, regions.board, ui);
    draw_actions(frame, regions.actions, ui);
    draw_action_log(frame, regions.log, &ui.action_log);
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
