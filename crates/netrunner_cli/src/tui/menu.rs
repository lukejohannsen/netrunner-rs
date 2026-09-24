//! The main menu: what `netrunner_cli` opens with no arguments.
//!
//! Every way of playing is reachable from here without a flag — a game
//! against the computer, a game against a person over the network (hosted
//! here or joined), the Learn to Play track, the deck builder, the ladder,
//! the settings — and a finished game comes back here rather than to the
//! shell. The flags still work and still skip it; they are what scripts
//! and measurements use.
//!
//! **Nothing here plays a game.** A choice becomes a `Launch` carrying the
//! `Config` the equivalent flags would have produced, and the loop hands it
//! to the same `play_local` / `learn::play` / `play_remote` the flag path
//! calls. So there
//! is still one seating rule, one record rule and one deck resolver; the
//! menu is a way of filling in their inputs.
//!
//! **Each launch clones `base`** rather than mutating one shared config:
//! the style picked for the computer's Corp in one game is an unset flag
//! again in the next, so the starter game after it plays its deck's own
//! style, as `learn game` would.
//!
//! Like the new-game form, the whole state machine is a plain struct with a
//! `key` function, tested without a terminal; `run` and `draw` are the thin
//! layer over it.

use std::path::PathBuf;
use std::time::Duration;

use clap::ValueEnum;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::Side;
use netrunner_core::tutorial;

use super::builder::{DeckKey, DeckScreen};
use super::online::{OnlineScreen, OnlineStep};
use super::start::{self, StartChoice, StartKey, StartMenu};
use crate::config::{Config, FormatArg};
use netrunner_client::decks;
use crate::learn::{self, LearnPick};
use crate::record;
use crate::settings::{self, Settings};

/// The main menu's entries, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    PlayComputer,
    Online,
    Learn,
    Decks,
    Record,
    Settings,
    Quit,
}

impl Entry {
    const ALL: [Entry; 7] = [Entry::PlayComputer, Entry::Online, Entry::Learn, Entry::Decks, Entry::Record, Entry::Settings, Entry::Quit];

    fn label(self) -> &'static str {
        match self {
            Entry::PlayComputer => "Play vs Computer",
            Entry::Online => "Play Online",
            Entry::Learn => "Learn to Play",
            Entry::Decks => "Decks",
            Entry::Record => "Record",
            Entry::Settings => "Settings",
            Entry::Quit => "Quit",
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            Entry::PlayComputer => "Pick a side, a deck each, and the computer's strength and style",
            Entry::Online => "Host a game for someone to join, join one by address, or watch",
            Entry::Learn => "Guided lessons for each side, then the starter game",
            Entry::Decks => "Build, copy and edit your own decks, or read the built-in ones",
            Entry::Record => "Your wins and losses against each rung, and the rung to try next",
            Entry::Settings => "Your name and the format decks are checked against",
            Entry::Quit => "Back to the shell",
        }
    }
}

/// Something for the loop to play on its terminal, with the config the
/// equivalent flags would have produced — or, online, the seat the
/// connection already holds.
pub enum Launch {
    Local { config: Box<Config> },
    Learn { pick: LearnPick, config: Box<Config> },
    Remote { joined: Box<crate::remote::Joined>, url: String, brought: Option<String> },
}

/// Which kind of launch just came back, for `Menu::returned` — the
/// launch itself is consumed by playing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Played {
    Local,
    Learn,
    Remote,
}

impl Launch {
    fn kind(&self) -> Played {
        match self {
            Launch::Local { .. } => Played::Local,
            Launch::Learn { .. } => Played::Learn,
            Launch::Remote { .. } => Played::Remote,
        }
    }
}

/// What one key did.
pub enum MenuStep {
    Continue,
    Launch(Launch),
    Quit,
}

/// One row of the Learn to Play screen: a heading, or something to play.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LearnRow {
    label: String,
    pick: Option<LearnPick>,
}

/// The Learn to Play screen — every `learn` subcommand as a list.
#[derive(Debug, Clone)]
struct LearnMenu {
    rows: Vec<LearnRow>,
    cursor: usize,
}

impl LearnMenu {
    fn new() -> Self {
        let mut rows = Vec::new();
        let heading = |label: &str| LearnRow { label: label.to_string(), pick: None };
        for side in [Side::Corp, Side::Runner] {
            let lessons = tutorial::track(side);
            rows.push(heading(&format!("{side:?} track")));
            rows.push(LearnRow {
                label: format!("  Every {side:?} lesson in order, then the starter game"),
                pick: Some(LearnPick::Track(side)),
            });
            for (index, lesson) in lessons.iter().enumerate() {
                rows.push(LearnRow { label: format!("  {}. {}", index + 1, lesson.title), pick: Some(LearnPick::Lesson(lesson.id.clone())) });
            }
        }
        rows.push(heading("Starter game — Null Signal Games' preset decks, first to 6 points"));
        for side in [Side::Corp, Side::Runner] {
            rows.push(LearnRow { label: format!("  As the {side:?}"), pick: Some(LearnPick::Game { side, boosted: false }) });
        }
        for side in [Side::Corp, Side::Runner] {
            rows.push(LearnRow {
                label: format!("  As the {side:?}, with the booster pack (to 7 points)"),
                pick: Some(LearnPick::Game { side, boosted: true }),
            });
        }
        let cursor = rows.iter().position(|row| row.pick.is_some()).expect("the track has something to play");
        LearnMenu { rows, cursor }
    }

    /// Moves to the next playable row in `delta`'s direction, skipping
    /// headings and wrapping at either end.
    fn move_cursor(&mut self, delta: i32) {
        let len = self.rows.len() as i32;
        let mut next = self.cursor as i32;
        for _ in 0..len {
            next = (next + delta).rem_euclid(len);
            if self.rows[next as usize].pick.is_some() {
                self.cursor = next as usize;
                return;
            }
        }
    }

    fn selected(&self) -> Option<&LearnPick> {
        self.rows[self.cursor].pick.as_ref()
    }
}

/// The settings screen's rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsRow {
    Player,
    Format,
    /// The cards whose "you may" was answered for good
    /// (`netrunner_client::standing`): Enter forgets them all. The
    /// desktop client's settings screen forgets one at a time.
    Answers,
}

const SETTINGS_ROWS: [SettingsRow; 3] = [SettingsRow::Player, SettingsRow::Format, SettingsRow::Answers];

/// Longest player name the form accepts. An id, not an essay.
const MAX_NAME: usize = 32;

#[derive(Debug, Clone)]
struct SettingsForm {
    cursor: usize,
    /// The name being typed, while the name row is open for editing.
    editing: Option<String>,
    settings: Settings,
    /// The name games are recorded under right now — the setting, a
    /// `--player` flag or the login name — which editing starts from.
    current_name: String,
}

enum SettingsKey {
    Continue,
    /// A setting changed; the menu saves it and applies it to `base`.
    Changed,
    Back,
}

impl SettingsForm {
    fn new(settings: Settings, current_name: String) -> Self {
        SettingsForm { cursor: 0, editing: None, settings, current_name }
    }

    fn format(&self) -> FormatArg {
        self.settings.format.map_or(FormatArg::Startup, FormatArg::from)
    }

    fn cycle_format(&mut self, delta: i32) {
        let all = FormatArg::value_variants();
        let index = all.iter().position(|format| *format == self.format()).unwrap_or(0) as i32;
        self.settings.format = Some(all[(index + delta).rem_euclid(all.len() as i32) as usize].into());
    }

    /// While the name is open every printable key is text — `q` included,
    /// which is why editing is checked before anything else.
    fn key(&mut self, key: KeyCode) -> SettingsKey {
        if let Some(name) = &mut self.editing {
            match key {
                KeyCode::Char(c) if !c.is_control() && name.chars().count() < MAX_NAME => name.push(c),
                KeyCode::Backspace => {
                    name.pop();
                }
                KeyCode::Enter => {
                    let name = self.editing.take().unwrap_or_default();
                    let name = name.trim();
                    self.settings.player = (!name.is_empty()).then(|| name.to_string());
                    if let Some(name) = &self.settings.player {
                        self.current_name = name.clone();
                    }
                    return SettingsKey::Changed;
                }
                KeyCode::Esc => self.editing = None,
                _ => {}
            }
            return SettingsKey::Continue;
        }
        match key {
            KeyCode::Esc | KeyCode::Char('q') => SettingsKey::Back,
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Down | KeyCode::Char('j') => {
                let delta = if matches!(key, KeyCode::Up | KeyCode::Char('k')) { -1 } else { 1 };
                self.cursor = (self.cursor as i32 + delta).rem_euclid(SETTINGS_ROWS.len() as i32) as usize;
                SettingsKey::Continue
            }
            KeyCode::Enter | KeyCode::Char(' ') => match SETTINGS_ROWS[self.cursor] {
                SettingsRow::Player => {
                    self.editing = Some(self.current_name.clone());
                    SettingsKey::Continue
                }
                SettingsRow::Format => {
                    self.cycle_format(1);
                    SettingsKey::Changed
                }
                SettingsRow::Answers if !self.settings.answers.is_empty() => {
                    self.settings.answers = Default::default();
                    SettingsKey::Changed
                }
                SettingsRow::Answers => SettingsKey::Continue,
            },
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Right | KeyCode::Char('l') if SETTINGS_ROWS[self.cursor] == SettingsRow::Format => {
                self.cycle_format(if matches!(key, KeyCode::Left | KeyCode::Char('h')) { -1 } else { 1 });
                SettingsKey::Changed
            }
            _ => SettingsKey::Continue,
        }
    }
}

enum Screen {
    Main,
    NewGame(Box<StartMenu>),
    Learn(LearnMenu),
    Decks(Box<DeckScreen>),
    Online(Box<OnlineScreen>),
    Record { lines: Vec<String>, scroll: u16 },
    Settings(SettingsForm),
}

pub struct Menu {
    screen: Screen,
    cursor: usize,
    /// The command line with the settings file applied. Every launch
    /// starts from a clone of it; see the module comment.
    base: Config,
    settings: Settings,
    /// Where settings are saved; `None` when the OS has no data directory,
    /// in which case a change lasts for this session only and says so.
    settings_path: Option<PathBuf>,
    registry: CardRegistry,
    /// The last game played against the computer, so the form reopens on
    /// it and Enter plays again.
    last_game: Option<StartChoice>,
    /// One line under the screen — a game that could not start, a setting
    /// that could not be saved. Cleared by the next key.
    notice: Option<String>,
}

impl Menu {
    pub fn new(base: Config, settings: Settings, settings_path: Option<PathBuf>) -> Self {
        Menu {
            screen: Screen::Main,
            cursor: 0,
            base,
            settings,
            settings_path,
            registry: decks::sample_deck_registry(),
            last_game: None,
            notice: None,
        }
    }

    fn entry(&self) -> Entry {
        Entry::ALL[self.cursor]
    }

    /// Opens the new-game form on the decks and the record as they are now
    /// — re-read every time, so a game's result moves the suggested rung
    /// before the next one.
    fn open_new_game(&mut self) {
        match start::open(&self.base, &self.registry) {
            Ok(mut form) => {
                if let Some(last) = &self.last_game {
                    form.resume_from(last);
                }
                self.screen = Screen::NewGame(Box::new(form));
            }
            Err(error) => {
                self.notice = Some(error);
                self.screen = Screen::Main;
            }
        }
    }

    fn activate(&mut self) -> MenuStep {
        match self.entry() {
            Entry::PlayComputer => self.open_new_game(),
            Entry::Online => {
                let opened = netrunner_client::deck_store::resolve_decks_dir(self.base.decks_dir.as_deref()).and_then(|dir| {
                    OnlineScreen::open(&dir, &self.registry, self.base.format.into(), record::player_name(&self.base), self.base.server.clone())
                });
                match opened {
                    Ok(screen) => self.screen = Screen::Online(Box::new(screen)),
                    Err(error) => self.notice = Some(error),
                }
            }
            Entry::Learn => self.screen = Screen::Learn(LearnMenu::new()),
            // Opened on the format as it stands, so a format changed in
            // Settings changes the pool and the verdicts at once.
            Entry::Decks => {
                let opened = netrunner_client::deck_store::resolve_decks_dir(self.base.decks_dir.as_deref())
                    .and_then(|dir| DeckScreen::open(dir, self.registry.clone(), self.base.format.into()));
                match opened {
                    Ok(screen) => self.screen = Screen::Decks(Box::new(screen)),
                    Err(error) => self.notice = Some(error),
                }
            }
            Entry::Record => {
                let lines = record::standing_lines(&self.base).unwrap_or_else(|error| vec![format!("Could not read the record: {error}")]);
                self.screen = Screen::Record { lines, scroll: 0 };
            }
            Entry::Settings => {
                // A game answers a card's "you may" for good straight into
                // the file (`crate::settings::remember`), so the copy the
                // menu started with is read again: saving it as it was
                // would forget those answers.
                if let Some(saved) = self.settings_path.as_ref().and_then(|path| Settings::load(path).ok()) {
                    self.settings = saved;
                }
                self.screen = Screen::Settings(SettingsForm::new(self.settings.clone(), record::player_name(&self.base)));
            }
            Entry::Quit => return MenuStep::Quit,
        }
        MenuStep::Continue
    }

    /// Saves the form's settings and applies them to `base` at once. An
    /// edit made here outranks a flag given at launch: it is the newer and
    /// more specific request of the two.
    fn settings_changed(&mut self, settings: Settings) {
        self.base.player = settings.player.clone();
        self.base.format = settings.format.map_or(FormatArg::Startup, FormatArg::from);
        self.notice = match &self.settings_path {
            Some(path) => settings.save(path).err().map(|error| format!("Not saved: {error}")),
            None => Some("No data directory, so this lasts until you quit".to_string()),
        };
        self.settings = settings;
    }

    /// One keypress.
    pub fn key(&mut self, key: KeyCode) -> MenuStep {
        self.notice = None;
        match &mut self.screen {
            Screen::Main => match key {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.cursor = (self.cursor + Entry::ALL.len() - 1) % Entry::ALL.len();
                    MenuStep::Continue
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.cursor = (self.cursor + 1) % Entry::ALL.len();
                    MenuStep::Continue
                }
                KeyCode::Enter | KeyCode::Char(' ') => self.activate(),
                KeyCode::Esc | KeyCode::Char('q') => MenuStep::Quit,
                _ => MenuStep::Continue,
            },
            Screen::NewGame(form) => match start::key(form, key) {
                StartKey::Continue => MenuStep::Continue,
                StartKey::Back => {
                    self.screen = Screen::Main;
                    MenuStep::Continue
                }
                StartKey::Start(choice) => {
                    let mut config = self.base.clone();
                    start::apply_choice(&choice, &mut config);
                    self.last_game = Some(choice);
                    MenuStep::Launch(Launch::Local { config: Box::new(config) })
                }
            },
            Screen::Learn(learn) => match key {
                KeyCode::Up | KeyCode::Char('k') => {
                    learn.move_cursor(-1);
                    MenuStep::Continue
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    learn.move_cursor(1);
                    MenuStep::Continue
                }
                KeyCode::Enter | KeyCode::Char(' ') => match learn.selected() {
                    Some(pick) => MenuStep::Launch(Launch::Learn { pick: pick.clone(), config: Box::new(self.base.clone()) }),
                    None => MenuStep::Continue,
                },
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.screen = Screen::Main;
                    MenuStep::Continue
                }
                _ => MenuStep::Continue,
            },
            Screen::Decks(decks) => {
                if decks.key(key) == DeckKey::Back {
                    self.screen = Screen::Main;
                }
                MenuStep::Continue
            }
            Screen::Online(online) => {
                let step = online.key(key);
                self.online_step(step)
            }
            Screen::Record { scroll, .. } => {
                match key {
                    KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => *scroll = scroll.saturating_add(1),
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => self.screen = Screen::Main,
                    _ => {}
                }
                MenuStep::Continue
            }
            Screen::Settings(form) => match form.key(key) {
                SettingsKey::Continue => MenuStep::Continue,
                SettingsKey::Changed => {
                    let settings = form.settings.clone();
                    self.settings_changed(settings);
                    MenuStep::Continue
                }
                SettingsKey::Back => {
                    self.screen = Screen::Main;
                    MenuStep::Continue
                }
            },
        }
    }

    fn online_step(&mut self, step: OnlineStep) -> MenuStep {
        match step {
            OnlineStep::Continue => MenuStep::Continue,
            OnlineStep::Back => {
                self.screen = Screen::Main;
                MenuStep::Continue
            }
            OnlineStep::Play { joined, url, brought } => MenuStep::Launch(Launch::Remote { joined, url, brought }),
        }
    }

    /// Called every frame, key or no key: a connection in progress is
    /// polled here, so the lobby wait draws and a seat is taken the moment
    /// the server offers it.
    pub fn tick(&mut self) -> MenuStep {
        match &mut self.screen {
            Screen::Online(online) => {
                let step = online.tick();
                self.online_step(step)
            }
            _ => MenuStep::Continue,
        }
    }

    /// Back from a launch. A game against the computer reopens the form on
    /// the game just played; a lesson leaves the Learn screen where it was;
    /// an online game stops the server this player hosted, if they did.
    /// A game that stopped on an error — a deck that failed validation, a
    /// record file that would not open — says why under the screen instead
    /// of dropping the player out of the TUI.
    pub fn returned(&mut self, played: Played, error: Option<String>) {
        match played {
            Played::Local => self.open_new_game(),
            Played::Learn => {}
            Played::Remote => {
                if let Screen::Online(online) = &mut self.screen {
                    online.returned();
                }
            }
        }
        if let Some(error) = error {
            self.notice = Some(format!("That game stopped: {error}"));
        }
    }

    pub fn draw(&self, frame: &mut Frame) {
        let [body, footer] =
            Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());
        match &self.screen {
            Screen::Main => self.draw_main(frame, body),
            Screen::NewGame(form) => start::draw(frame, body, form),
            Screen::Learn(learn) => draw_learn(frame, body, learn),
            Screen::Decks(decks) => decks.draw(frame, body),
            Screen::Online(online) => online.draw(frame, body),
            Screen::Record { lines, scroll } => draw_record(frame, body, lines, *scroll),
            Screen::Settings(form) => self.draw_settings(frame, body, form),
        }
        let line = match &self.notice {
            Some(notice) => Line::from(Span::styled(notice.clone(), Style::default().fg(Color::Red))),
            None => Line::from(Span::styled(
                format!("Playing as {} · {} format", record::player_name(&self.base), format_name(self.base.format)),
                Style::default().fg(Color::DarkGray),
            )),
        };
        frame.render_widget(Paragraph::new(line), footer);
    }

    fn draw_main(&self, frame: &mut Frame, area: Rect) {
        let area = centered(area, 72, Entry::ALL.len() as u16 * 2 + 7);
        let [title, list, help] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(2)])
            .areas(area);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("N E T R U N N E R", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
                Line::from(Span::styled("a game of the Null Signal Games card pool", Style::default().fg(Color::DarkGray))),
            ]),
            title,
        );
        let items: Vec<ListItem> = Entry::ALL
            .iter()
            .map(|entry| {
                ListItem::new(vec![
                    Line::from(Span::styled(entry.label(), Style::default().add_modifier(Modifier::BOLD))),
                    Line::from(Span::styled(format!("  {}", entry.blurb()), Style::default().fg(Color::Gray))),
                ])
            })
            .collect();
        let mut state = ListState::default();
        state.select(Some(self.cursor));
        frame.render_stateful_widget(List::new(items).highlight_style(Style::default().add_modifier(Modifier::REVERSED)), list, &mut state);
        frame.render_widget(Paragraph::new("Up/Down choose, Enter selects, q quits"), help);
    }

    fn draw_settings(&self, frame: &mut Frame, area: Rect, form: &SettingsForm) {
        let player = match (&form.editing, &form.settings.player) {
            (Some(name), _) => format!("{name}▏  (Enter saves, Esc cancels)"),
            (None, Some(name)) => name.clone(),
            (None, None) => format!("{}  (not set — Enter to choose one)", form.current_name),
        };
        let answers = match form.settings.answers.iter().count() {
            0 => "none — a card's \"you may\" can be answered with y (always) or n (never)".to_string(),
            1 => "1 card  (Enter forgets it)".to_string(),
            count => format!("{count} cards  (Enter forgets them all)"),
        };
        let rows = [format!("Player name   {player}"), format!("Format        {}  (Left/Right to change)", format_name(form.format())), format!("Answers       {answers}")];
        let items: Vec<ListItem> = rows.into_iter().map(ListItem::new).collect();
        let mut state = ListState::default();
        state.select(Some(form.cursor));
        let [list, about] =
            Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(5), Constraint::Min(0)]).areas(area);
        frame.render_stateful_widget(
            List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Settings — Up/Down choose, Enter edits, Esc goes back"))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
            list,
            &mut state,
        );
        let path = |result: Result<PathBuf, String>| result.map_or_else(|error| error, |path| path.display().to_string());
        let about_lines = vec![
            Line::from("Your name is what your games are recorded under and what a server sees. The format is what deck legality is checked against:"),
            Line::from("Startup is System Gateway and Elevation, the pool this game ships; the Core Set needs Eternal."),
            Line::from(""),
            Line::from(format!("Settings     {}", self.settings_path.as_ref().map_or_else(|| "not saved".to_string(), |p| p.display().to_string()))),
            Line::from(format!("Saved decks  {}", path(netrunner_client::deck_store::resolve_decks_dir(self.base.decks_dir.as_deref())))),
            Line::from(format!("Record       {}", path(record::resolve_record_file(self.base.record_file.as_deref())))),
        ];
        frame.render_widget(Paragraph::new(about_lines).wrap(Wrap { trim: false }), about);
    }
}

fn format_name(format: FormatArg) -> String {
    format.to_possible_value().map_or_else(|| format!("{format:?}"), |value| value.get_name().to_string())
}

fn draw_learn(frame: &mut Frame, area: Rect, learn: &LearnMenu) {
    let items: Vec<ListItem> = learn
        .rows
        .iter()
        .map(|row| match row.pick {
            None => ListItem::new(Line::from(Span::styled(row.label.clone(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))),
            Some(_) => ListItem::new(row.label.clone()),
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(learn.cursor));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Learn to Play — Up/Down choose, Enter plays, Esc goes back"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
        &mut state,
    );
}

fn draw_record(frame: &mut Frame, area: Rect, lines: &[String], scroll: u16) {
    let text: Vec<Line> = lines.iter().map(|line| Line::from(line.clone())).collect();
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title("Record — Up/Down scroll, Esc goes back"))
            .scroll((scroll, 0)),
        area,
    );
}

/// A `width` × `height` box centred in `area`, shrunk to fit.
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect { x: area.x + (area.width - width) / 2, y: area.y + (area.height - height) / 2, width, height }
}

/// Runs the menu until the player quits, on one terminal for everything it
/// launches.
pub fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let settings_path = settings::resolve_settings_file().ok();
    // `main` already applied the file to `config`; the menu needs the
    // settings themselves to edit them. A file that will not parse was
    // reported there and is edited from the defaults here.
    let settings = settings_path.as_deref().and_then(|path| Settings::load(path).ok()).unwrap_or_default();
    let mut menu = Menu::new(config.clone(), settings, settings_path);
    let mut terminal = ratatui::init();
    let result = drive(&mut terminal, &mut menu);
    ratatui::restore();
    result
}

fn drive(terminal: &mut ratatui::DefaultTerminal, menu: &mut Menu) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|frame| menu.draw(frame))?;
        let step = if event::poll(Duration::from_millis(100))? {
            // Press only: Windows reports a release for every key as well,
            // and the menu would otherwise act twice on one keystroke.
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => menu.key(key.code),
                _ => MenuStep::Continue,
            }
        } else {
            menu.tick()
        };
        match step {
            MenuStep::Continue => {}
            MenuStep::Quit => return Ok(()),
            MenuStep::Launch(launch) => {
                let played = launch.kind();
                let result = match launch {
                    Launch::Local { config } => super::play_local(terminal, &config),
                    Launch::Learn { pick, config } => learn::play(terminal, &pick, &config),
                    Launch::Remote { joined, url, brought } => super::play_remote(terminal, *joined, &url, brought.as_deref()),
                };
                menu.returned(played, result.err().map(|error| error.to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use netrunner_bots::Personality;
    use netrunner_core::format::NsgFormat;

    use crate::config::BotKind;

    /// A menu whose decks, record and settings all live in a fresh temp
    /// directory, so no test reads or writes the player's real files.
    fn menu(name: &str) -> (Menu, PathBuf) {
        let dir = std::env::temp_dir().join(format!("netrunner_menu_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let decks = dir.join("decks");
        let record = dir.join("record.json");
        let config = Config::try_parse_from([
            "netrunner_cli",
            "--decks-dir",
            decks.to_str().unwrap(),
            "--record-file",
            record.to_str().unwrap(),
            "--player",
            "tester",
        ])
        .unwrap();
        (Menu::new(config, Settings::default(), Some(dir.join("settings.json"))), dir)
    }

    fn press(menu: &mut Menu, keys: &[KeyCode]) -> MenuStep {
        let mut last = MenuStep::Continue;
        for key in keys {
            last = menu.key(*key);
        }
        last
    }

    fn go_to(menu: &mut Menu, entry: Entry) {
        while menu.entry() != entry {
            menu.key(KeyCode::Down);
        }
        menu.key(KeyCode::Enter);
    }

    #[test]
    fn the_main_menu_wraps_and_q_quits() {
        let (mut menu, _) = menu("wraps");
        assert_eq!(menu.entry(), Entry::PlayComputer);
        menu.key(KeyCode::Up);
        assert_eq!(menu.entry(), Entry::Quit, "Up from the top wraps to the bottom");
        assert!(matches!(menu.key(KeyCode::Enter), MenuStep::Quit));
        let (mut fresh, _) = self::menu("wraps_q");
        assert!(matches!(fresh.key(KeyCode::Char('q')), MenuStep::Quit));
    }

    #[test]
    fn play_vs_computer_launches_the_flag_form_and_leaves_base_untouched() {
        let (mut menu, dir) = menu("launch");
        go_to(&mut menu, Entry::PlayComputer);
        assert!(matches!(menu.screen, Screen::NewGame(_)));
        let MenuStep::Launch(Launch::Local { config }) = menu.key(KeyCode::Enter) else { panic!("Enter plays") };
        let choice = menu.last_game.clone().expect("the launch is remembered");
        assert_eq!(choice.human, Side::Corp);
        assert_eq!((config.corp, config.runner), (BotKind::Human, BotKind::Heuristic));
        assert_eq!(config.runner_level, Some(choice.level));
        assert_eq!((menu.base.corp, menu.base.runner), (BotKind::Human, BotKind::Human), "a launch clones base");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_game_returns_to_the_form_on_the_game_just_played() {
        let (mut menu, dir) = menu("returns");
        go_to(&mut menu, Entry::PlayComputer);
        // The Runner chair.
        let MenuStep::Launch(launch) = press(&mut menu, &[KeyCode::Down, KeyCode::Enter]) else { panic!() };
        menu.returned(launch.kind(), None);
        let Screen::NewGame(form) = &menu.screen else { panic!("back on the form") };
        let again = form.choice().unwrap();
        assert_eq!(again.human, Side::Runner);
        assert!(menu.notice.is_none());
        menu.returned(launch.kind(), Some("deck is not legal".to_string()));
        assert_eq!(menu.notice.as_deref(), Some("That game stopped: deck is not legal"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_style_chosen_for_one_game_does_not_follow_into_the_next() {
        let (mut menu, dir) = menu("leak");
        go_to(&mut menu, Entry::PlayComputer);
        // Style pane, first written profile for the Runner bot.
        let MenuStep::Launch(Launch::Local { config }) =
            press(&mut menu, &[KeyCode::Tab, KeyCode::Tab, KeyCode::Down, KeyCode::Enter])
        else {
            panic!()
        };
        assert_eq!(config.runner_personality, Some(Personality::Aggressive));
        press(&mut menu, &[KeyCode::Esc]);
        go_to(&mut menu, Entry::Learn);
        let MenuStep::Launch(Launch::Learn { config, .. }) = menu.key(KeyCode::Enter) else { panic!() };
        assert_eq!((config.corp_personality, config.runner_personality), (None, None));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_learn_screen_offers_every_lesson_and_never_lands_on_a_heading() {
        let learn = LearnMenu::new();
        let lessons = tutorial::track(Side::Corp).len() + tutorial::track(Side::Runner).len();
        let playable = learn.rows.iter().filter(|row| row.pick.is_some()).count();
        assert_eq!(playable, lessons + 2 + 4, "every lesson, both tracks, four starter games");
        assert_eq!(learn.selected(), Some(&LearnPick::Track(Side::Corp)));
        let mut walked = learn.clone();
        for _ in 0..learn.rows.len() * 2 {
            walked.move_cursor(1);
            assert!(walked.selected().is_some());
        }
        walked.move_cursor(-1);
        assert!(walked.selected().is_some());
        let mut up = learn.clone();
        up.move_cursor(-1);
        assert_eq!(up.selected(), Some(&LearnPick::Game { side: Side::Runner, boosted: true }), "Up from the top wraps past the heading");
    }

    #[test]
    fn every_learn_pick_names_a_real_lesson() {
        for row in LearnMenu::new().rows {
            if let Some(LearnPick::Lesson(id)) = row.pick {
                assert!(tutorial::by_id(&id).is_some(), "{id}");
            }
        }
    }

    #[test]
    fn a_name_typed_in_settings_is_saved_and_rates_the_next_game() {
        let (mut menu, dir) = menu("name");
        go_to(&mut menu, Entry::Settings);
        // `q` is a letter while the name is open, not "back".
        press(&mut menu, &[KeyCode::Enter, KeyCode::Char('q'), KeyCode::Char('u'), KeyCode::Char('x'), KeyCode::Backspace, KeyCode::Enter]);
        assert_eq!(menu.base.player.as_deref(), Some("testerqu"), "the name opens on the current one");
        assert_eq!(Settings::load(&dir.join("settings.json")).unwrap().player.as_deref(), Some("testerqu"));
        assert!(menu.notice.is_none(), "{:?}", menu.notice);
        press(&mut menu, &[KeyCode::Esc]);
        go_to(&mut menu, Entry::PlayComputer);
        let MenuStep::Launch(Launch::Local { config }) = menu.key(KeyCode::Enter) else { panic!() };
        assert_eq!(record::player_name(&config), "testerqu");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_format_cycles_and_is_saved() {
        let (mut menu, dir) = menu("format");
        go_to(&mut menu, Entry::Settings);
        press(&mut menu, &[KeyCode::Down, KeyCode::Right]);
        assert_eq!(menu.base.format, FormatArg::Standard);
        press(&mut menu, &[KeyCode::Left, KeyCode::Left]);
        assert_eq!(menu.base.format, FormatArg::Snapshot, "wraps backwards");
        assert_eq!(Settings::load(&dir.join("settings.json")).unwrap().format, Some(NsgFormat::Snapshot));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn escape_from_every_screen_goes_back_to_the_main_menu() {
        for entry in [Entry::PlayComputer, Entry::Online, Entry::Learn, Entry::Decks, Entry::Record, Entry::Settings] {
            let (mut menu, dir) = menu(&format!("esc_{entry:?}"));
            go_to(&mut menu, entry);
            assert!(!matches!(menu.screen, Screen::Main), "{entry:?} opens a screen");
            menu.key(KeyCode::Esc);
            assert!(matches!(menu.screen, Screen::Main), "{entry:?}");
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn only_an_invocation_with_no_bot_on_either_chair_opens_the_menu() {
        let seats = |args: &[&str]| {
            let config = Config::try_parse_from(args).unwrap();
            (config.seats_bot(Side::Corp), config.seats_bot(Side::Runner))
        };
        assert_eq!(seats(&["netrunner_cli"]), (false, false), "the menu");
        assert_eq!(seats(&["netrunner_cli", "--corp-deck", "brick_stack"]), (false, false), "a deck is not a seat");
        assert_eq!(seats(&["netrunner_cli", "--runner", "heuristic"]), (false, true));
        // What `record` tells a new player to type: a rung is a bot.
        assert_eq!(seats(&["netrunner_cli", "--runner-level", "3"]), (false, true));
        assert_eq!(seats(&["netrunner_cli", "--runner", "human", "--corp-level", "veteran"]), (true, false));
    }

    #[test]
    fn a_fresh_record_says_so_on_the_record_screen() {
        let (mut menu, dir) = menu("record");
        go_to(&mut menu, Entry::Record);
        let Screen::Record { lines, .. } = &menu.screen else { panic!() };
        assert!(lines[0].starts_with("tester"), "{lines:?}");
        assert!(lines.iter().any(|line| line.contains("no games yet")), "{lines:?}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
