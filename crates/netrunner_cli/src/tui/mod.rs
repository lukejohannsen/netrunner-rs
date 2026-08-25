pub mod layout;

use std::time::Duration;

use crossterm::event::{self, Event};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{mask_state_for_player, GamePhase, GameState, PublicGameState, RunState, ServerId, Side};

use crate::app::{describe_action, App};
use crate::bots;
use crate::config::{Config, ViewAs};
use crate::decks;

const CORP_MAX_CLICKS: u32 = 3;
const RUNNER_MAX_CLICKS: u32 = 4;

pub fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let registry = decks::kate_vs_hb_registry();
    let (corp_deck, runner_deck) = decks::kate_vs_hb_decks();
    let seed = config.seed.unwrap_or_else(rand::random);
    let (state, _events) = GameState::setup(&corp_deck, &runner_deck, &registry, seed)?;

    let corp_agent = bots::make_agent(config.corp_agent, Side::Corp, seed);
    let runner_agent = bots::make_agent(config.runner_agent, Side::Runner, seed.wrapping_add(1));
    let mut app = App::new(state, registry, config.view_as, corp_agent, runner_agent);

    let mut terminal = ratatui::init();
    let result = run_event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run_event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    while !app.should_quit {
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
    draw_log(frame, regions.log, app);
    draw_actions(frame, regions.actions, app);
    if app.is_game_over() {
        draw_game_over_modal(frame, app);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let phase_label = match app.state.phase {
        GamePhase::Mulligan(side) => format!("Mulligan ({side:?})"),
        GamePhase::StartOfTurn(side) => format!("Start of turn ({side:?})"),
        GamePhase::Action(side) => format!("Action ({side:?})"),
        GamePhase::Discard { side, required } => format!("Discard ({side:?}, {required} remaining)"),
        GamePhase::GameOver(winner) => format!("Game over — {winner:?} wins"),
    };
    let model = build_board_model(app);
    let text = format!(
        "Phase: {phase_label} | View: {:?} | Corp: {}c {} AP:{} BP:{} | Runner: {}c {} Tags:{} MU:{} AP:{}",
        app.view_as,
        model.corp_credits,
        click_pool(model.corp_clicks, CORP_MAX_CLICKS),
        model.corp_agenda_points,
        model.corp_bad_publicity,
        model.runner_credits,
        click_pool(model.runner_clicks, RUNNER_MAX_CLICKS),
        model.runner_tags,
        model.runner_mu,
        model.runner_agenda_points,
    );
    frame.render_widget(Paragraph::new(text), area);
}

fn click_pool(current: u32, max: u32) -> String {
    (0..max).map(|i| if i < current { "[x]" } else { "[ ]" }).collect::<Vec<_>>().join("")
}

fn draw_board(frame: &mut Frame, area: Rect, app: &App) {
    let [corp_area, runner_area] =
        Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);

    let model = build_board_model(app);

    let mut corp_lines = vec![
        Line::from(format!("HQ: {} cards   R&D: {} cards   Archives: {} cards", model.corp_hq_count, model.corp_rd_count, model.corp_archives.len())),
        Line::from(""),
    ];
    for server in installed_by_server(&model.corp_installed) {
        corp_lines.push(Line::from(server));
    }
    if let Some(run) = &model.active_run {
        corp_lines.push(Line::from(""));
        for line in format_run(run, &app.registry) {
            corp_lines.push(Line::from(line));
        }
    }
    frame.render_widget(
        Paragraph::new(corp_lines).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title("Corp servers")),
        corp_area,
    );

    let mut runner_lines =
        vec![Line::from(format!("Grip: {} cards   Stack: {} cards   Heap: {} cards", model.runner_grip_count, model.runner_stack_count, model.runner_heap.len())), Line::from("")];
    if model.runner_rig.is_empty() {
        runner_lines.push(Line::from("(rig empty)"));
    } else {
        runner_lines.extend(model.runner_rig.iter().map(|entry| Line::from(entry.clone())));
    }
    frame.render_widget(
        Paragraph::new(runner_lines).wrap(Wrap { trim: false }).block(Block::default().borders(Borders::ALL).title("Runner rig")),
        runner_area,
    );
}

fn installed_by_server(installed: &[InstalledDisplay]) -> Vec<String> {
    let mut servers: Vec<ServerId> = installed.iter().map(|c| c.server).collect();
    servers.sort_by_key(server_sort_key);
    servers.dedup();

    servers
        .into_iter()
        .map(|server| {
            let cards: Vec<String> = installed
                .iter()
                .filter(|c| c.server == server)
                .map(|c| {
                    let rez = if c.rezzed { "rezzed" } else { "unrezzed" };
                    if c.advancement_tokens > 0 {
                        format!("{} ({rez}, {} adv)", c.label, c.advancement_tokens)
                    } else {
                        format!("{} ({rez})", c.label)
                    }
                })
                .collect();
            let contents = if cards.is_empty() { "(empty)".to_string() } else { cards.join(", ") };
            format!("{}: {contents}", server_label(server))
        })
        .collect()
}

fn server_label(server: ServerId) -> String {
    match server {
        ServerId::Hq => "HQ".to_string(),
        ServerId::RnD => "R&D".to_string(),
        ServerId::Archives => "Archives".to_string(),
        ServerId::Remote(n) => format!("Remote {n}"),
    }
}

fn server_sort_key(server: &ServerId) -> (u8, u32) {
    match server {
        ServerId::Hq => (0, 0),
        ServerId::RnD => (1, 0),
        ServerId::Archives => (2, 0),
        ServerId::Remote(n) => (3, *n),
    }
}

fn format_run(run: &RunState, registry: &CardRegistry) -> Vec<String> {
    let mut lines = vec![format!(
        "Run on {} — {:?} (ICE {}/{})",
        server_label(run.server),
        run.phase,
        run.position,
        run.ice.len()
    )];
    for (index, ice) in run.ice.iter().enumerate() {
        let marker = if index == run.position { ">" } else { " " };
        let rez = if ice.rezzed { "rezzed" } else { "unrezzed" };
        lines.push(format!("{marker} {} [{rez}, str {}]", card_title(&ice.card_id, registry), ice.current_strength));
    }
    lines
}

fn draw_log(frame: &mut Frame, area: Rect, app: &App) {
    let visible_rows = area.height.saturating_sub(2) as usize;
    let total = app.event_log.len();
    let max_scroll = total.saturating_sub(visible_rows);
    let scroll = app.log_scroll.min(max_scroll);
    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(visible_rows);

    let items: Vec<ListItem> = app.event_log[start..end].iter().map(|event| ListItem::new(format!("{event:?}"))).collect();
    let title = if app.focus == crate::app::Focus::Log { "Event log (focused — Up/Down to scroll)" } else { "Event log" };
    frame.render_widget(List::new(items).block(Block::default().borders(Borders::ALL).title(title)), area);
}

fn draw_actions(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app.legal.iter().map(|action| ListItem::new(describe_action(action, &app.registry))).collect();
    let mut state = ListState::default();
    if !app.legal.is_empty() {
        state.select(Some(app.selected));
    }
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Legal actions (Up/Down, Enter to act, Tab to switch focus, q to quit)"))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_game_over_modal(frame: &mut Frame, app: &App) {
    let GamePhase::GameOver(winner) = app.state.phase else { return };
    let area = centered_rect(40, 20, frame.area());
    let text = vec![Line::from(Span::styled(format!("{winner:?} wins!"), Style::default().add_modifier(Modifier::BOLD))), Line::from("Press q to quit.")];
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

struct InstalledDisplay {
    server: ServerId,
    label: String,
    rezzed: bool,
    advancement_tokens: u32,
}

struct BoardModel {
    corp_hq_count: usize,
    corp_rd_count: usize,
    corp_archives: Vec<String>,
    corp_installed: Vec<InstalledDisplay>,
    corp_credits: u32,
    corp_clicks: u32,
    corp_agenda_points: u32,
    corp_bad_publicity: u32,
    runner_grip_count: usize,
    runner_stack_count: usize,
    runner_heap: Vec<String>,
    runner_rig: Vec<String>,
    runner_credits: u32,
    runner_clicks: u32,
    runner_agenda_points: u32,
    runner_tags: u32,
    runner_mu: u32,
    active_run: Option<RunState>,
}

fn build_board_model(app: &App) -> BoardModel {
    match app.view_as {
        ViewAs::Omniscient => build_from_raw(&app.state, &app.registry),
        ViewAs::Corp => build_from_public(mask_state_for_player(&app.state, Side::Corp), &app.registry),
        ViewAs::Runner => build_from_public(mask_state_for_player(&app.state, Side::Runner), &app.registry),
    }
}

fn build_from_raw(state: &GameState, registry: &CardRegistry) -> BoardModel {
    BoardModel {
        corp_hq_count: state.corp.hq.len(),
        corp_rd_count: state.corp.r_and_d.len(),
        corp_archives: state.corp.archives.iter().map(|id| card_title(id, registry)).collect(),
        corp_installed: state
            .corp
            .installed
            .iter()
            .map(|c| InstalledDisplay { server: c.server, label: card_title(&c.card, registry), rezzed: c.rezzed, advancement_tokens: c.advancement_tokens })
            .collect(),
        corp_credits: state.corp.resources.credits.0,
        corp_clicks: state.corp.resources.clicks.0,
        corp_agenda_points: state.corp.resources.agenda_points.0,
        corp_bad_publicity: state.corp.bad_publicity,
        runner_grip_count: state.runner.grip.len(),
        runner_stack_count: state.runner.stack.len(),
        runner_heap: state.runner.heap.iter().map(|id| card_title(id, registry)).collect(),
        runner_rig: state.runner.rig.iter().map(|c| format!("{} (str {})", card_title(&c.card, registry), c.effective_strength())).collect(),
        runner_credits: state.runner.resources.credits.0,
        runner_clicks: state.runner.resources.clicks.0,
        runner_agenda_points: state.runner.resources.agenda_points.0,
        runner_tags: state.runner.tags,
        runner_mu: state.runner.memory_units.0,
        active_run: state.active_run.clone(),
    }
}

fn build_from_public(public: PublicGameState, registry: &CardRegistry) -> BoardModel {
    use netrunner_core::rules::MaskedZone;

    fn zone_count(zone: &MaskedZone) -> usize {
        match zone {
            MaskedZone::Visible(cards) => cards.len(),
            MaskedZone::Hidden { count } => *count as usize,
        }
    }

    BoardModel {
        corp_hq_count: zone_count(&public.corp.hq),
        corp_rd_count: zone_count(&public.corp.r_and_d),
        corp_archives: public.corp.archives.iter().map(|id| card_title(id, registry)).collect(),
        corp_installed: public
            .corp
            .installed
            .iter()
            .map(|c| InstalledDisplay {
                server: c.server,
                label: c.card.as_ref().map(|id| card_title(id, registry)).unwrap_or_else(|| "???".to_string()),
                rezzed: c.rezzed,
                advancement_tokens: c.advancement_tokens,
            })
            .collect(),
        corp_credits: public.corp.resources.credits.0,
        corp_clicks: public.corp.resources.clicks.0,
        corp_agenda_points: public.corp.resources.agenda_points.0,
        corp_bad_publicity: public.corp.bad_publicity,
        runner_grip_count: zone_count(&public.runner.grip),
        runner_stack_count: zone_count(&public.runner.stack),
        runner_heap: public.runner.heap.iter().map(|id| card_title(id, registry)).collect(),
        runner_rig: public.runner.rig.iter().map(|c| format!("{} (str {})", card_title(&c.card, registry), c.current_strength)).collect(),
        runner_credits: public.runner.resources.credits.0,
        runner_clicks: public.runner.resources.clicks.0,
        runner_agenda_points: public.runner.resources.agenda_points.0,
        runner_tags: public.runner.tags,
        runner_mu: public.runner.memory_units.0,
        active_run: public.active_run,
    }
}
