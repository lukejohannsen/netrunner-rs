pub mod layout;

use std::time::Duration;

use crossterm::event::{self, Event};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use tokio::sync::mpsc;

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{GamePhase, GameState, ServerId, Side};
use netrunner_core::view::ServerView;
use netrunner_server::{MatchSession, PlayerSlot};

use crate::app::{describe_action, App};
use crate::bots;
use crate::config::Config;
use crate::decks;

const CORP_MAX_CLICKS: u32 = 3;
const RUNNER_MAX_CLICKS: u32 = 4;

pub async fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let human_side = match (config.corp, config.runner) {
        (crate::config::BotKind::Human, crate::config::BotKind::Human) => {
            return Err("interactive mode requires exactly one human-controlled side (both --corp and --runner are human)".into());
        }
        (crate::config::BotKind::Human, _) => Side::Corp,
        (_, crate::config::BotKind::Human) => Side::Runner,
        _ => return Err("interactive mode requires exactly one human-controlled side (neither --corp nor --runner is human)".into()),
    };

    let registry = decks::kate_vs_hb_registry();
    let (corp_deck, runner_deck) = decks::kate_vs_hb_decks();
    let seed = config.seed.unwrap_or_else(rand::random);
    let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;

    let (server_tx, app_rx) = mpsc::unbounded_channel();
    let (app_tx, server_rx) = mpsc::unbounded_channel();
    let human_slot = PlayerSlot::Channel { tx: server_tx, rx: server_rx };

    let (corp_slot, runner_slot) = match human_side {
        Side::Corp => {
            let runner_agent = bots::make_agent(config.runner, Side::Runner, seed.wrapping_add(1)).expect("runner is bot-controlled");
            (human_slot, PlayerSlot::Bot(runner_agent))
        }
        Side::Runner => {
            let corp_agent = bots::make_agent(config.corp, Side::Corp, seed).expect("corp is bot-controlled");
            (PlayerSlot::Bot(corp_agent), human_slot)
        }
    };

    let session = MatchSession::new(state, registry.clone(), corp_slot, runner_slot);
    tokio::spawn(session.run());

    let mut app = App::new(registry, human_side, app_tx, app_rx);

    let mut terminal = ratatui::init();
    let result = run_event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
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
    if app.is_game_over() {
        draw_game_over_modal(frame, app);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let Some(view) = &app.view else {
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
        "Phase: {phase_label} | You: {:?} | Corp: {}c {} AP:{} BP:{} | Runner: {}c {} Tags:{} MU:{} AP:{}",
        app.human_side,
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

fn draw_board(frame: &mut Frame, area: Rect, app: &App) {
    let [corp_area, runner_area] =
        Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);

    let Some(view) = &app.view else {
        frame.render_widget(Block::default().borders(Borders::ALL).title("Corp servers"), corp_area);
        frame.render_widget(Block::default().borders(Borders::ALL).title("Runner rig"), runner_area);
        return;
    };

    let mut corp_lines = vec![
        Line::from(format!("HQ: {} cards   R&D: {} cards   Archives: {} cards", view.corp.hq_count, view.corp.rd_count, view.corp.archives.len())),
        Line::from(""),
    ];
    for server in &view.corp.servers {
        corp_lines.push(Line::from(format_server(server, &app.registry)));
    }
    if let Some(run) = &view.active_run {
        corp_lines.push(Line::from(""));
        for line in format_run(run, &app.registry) {
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
        runner_lines.extend(view.runner.rig.iter().map(|card| Line::from(format!("{} (str {})", card_title(&card.card, &app.registry), card.current_strength))));
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
        if card.advancement_tokens > 0 {
            format!("{label} ({rez}, {} adv)", card.advancement_tokens)
        } else {
            format!("{label} ({rez})")
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

fn draw_actions(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app.legal_actions().iter().map(|action| ListItem::new(describe_action(action, &app.registry))).collect();
    let mut state = ListState::default();
    if !app.legal_actions().is_empty() {
        state.select(Some(app.selected));
    }
    let title = if app.legal_actions().is_empty() { "Waiting for the other side..." } else { "Legal actions (Up/Down, Enter to act, q to quit)" };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_game_over_modal(frame: &mut Frame, app: &App) {
    let Some((winner, reason)) = app.game_ended else { return };
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
