//! The deck builder: the main menu's Decks screen.
//!
//! Everything `netrunner_cli deck new/add/remove/show/validate` does, in
//! the TUI — list every deck, read a built-in one, start a deck from an
//! identity or from a copy of any deck, then edit it against a card pool
//! with the deck's running totals and its legality always on screen.
//!
//! **The list is the deck picker's, and the editor is the shared one**
//! (Phase 7 §16 stage (c), 11 October 2026). The list reads
//! `netrunner_client::deck_picker::rows` — every deck a person may see,
//! told the way every other list tells it: the identity, whether the deck
//! is the person's own, its standing in the format and every format it
//! is legal in — and is a `Picker` over both sides: `/` searches by the
//! deck's name or its identity's, `l` narrows to a format. The editor is
//! `netrunner_client::deck_editor::Editor`, the state the desktop's
//! builder drives: the pool, the filters, the copy limit and the
//! verdict are its, so a key here is an `Intent` and nothing more. Until
//! then this file kept a pool and a copy limit of its own, and the
//! desktop another — two builders that could disagree about one deck.
//!
//! **Legality comes from the validators, never from here.** The verdict
//! line is `deck_builder::status`, which runs both validators as
//! starting a game runs them, and the running totals are the deck's
//! `tally`. A fourth copy is refused by the editor, read off the card.
//!
//! **The pool is the format's.** Only cards legal in the format the
//! "Legal in" filter names are offered — Settings' to start with — so a
//! Startup player is never shown a Core Set card the validator would
//! refuse; a deck that already holds one (a copy of an Eternal list) still
//! shows and can remove it.
//!
//! **Every edit is saved as it is made**, as `deck add` saves on every
//! add: a deck under construction is legitimately illegal, so there is no
//! "valid enough to save" point to wait for, and nothing to lose on a
//! quit. Built-in decks are read-only — they are published lists — and
//! are edited by copying.
//!
//! The same shape as the menu: a plain struct, `key(KeyCode)` tested
//! without a terminal, `draw` a thin layer over it.

use std::fmt;
use std::path::PathBuf;

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use netrunner_core::cards::CardRegistry;
use netrunner_core::deck::{influence_per_copy, DeckTally};
use netrunner_core::decks::DeckFile;
use netrunner_core::dsl::{CardDefinition, CardId};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

use crate::app::{card_modal, Modal};
use netrunner_client::cards::{faction_label, type_group};
use netrunner_client::deck_builder::{self, CardBook, PoolSort, Standing, MAX_NAME};
use netrunner_client::deck_editor::{Editor as State, Intent, Outcome};
use netrunner_client::deck_picker::{self, Intent as ListIntent, Picker, Row};
use netrunner_client::deck_store;
use netrunner_client::settings::{format_label, FORMATS};

/// What one key did, for the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckKey {
    Continue,
    Back,
}

/// What a typed name is for.
#[derive(Debug, Clone)]
enum NamePurpose {
    New { identity: CardId },
    Copy { from: Box<DeckFile> },
}

enum Mode {
    List,
    /// A built-in deck, read-only: the row, and how far it is scrolled.
    View { index: usize, scroll: u16 },
    Edit(Box<Editor>),
    PickSide { cursor: usize },
    PickIdentity { side: Side, identities: Vec<CardId>, cursor: usize },
    Name { purpose: NamePurpose, text: String },
    ConfirmDelete { index: usize },
}

impl fmt::Debug for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mode::List => write!(f, "List"),
            Mode::View { index, scroll } => write!(f, "View {{ index: {index}, scroll: {scroll} }}"),
            Mode::Edit(editor) => write!(f, "Edit({})", editor.state.deck().name),
            Mode::PickSide { cursor } => write!(f, "PickSide {{ cursor: {cursor} }}"),
            Mode::PickIdentity { side, cursor, .. } => write!(f, "PickIdentity {{ side: {side:?}, cursor: {cursor} }}"),
            Mode::Name { text, .. } => write!(f, "Name({text:?})"),
            Mode::ConfirmDelete { index } => write!(f, "ConfirmDelete({index})"),
        }
    }
}

pub struct DeckScreen {
    dir: PathBuf,
    registry: CardRegistry,
    /// The builder's catalog (`cards::catalog`): the playable registry
    /// first, then what the engine does not play yet, so an imported
    /// deck's unplayable card still has a title.
    catalog: Vec<CardDefinition>,
    format: NsgFormat,
    /// The list: every deck a person may see, with the search and the
    /// cursor over them. Its `rows` are what `View`, `ConfirmDelete` and
    /// Enter index.
    picker: Picker,
    /// The list's search box has the keys.
    searching: bool,
    mode: Mode,
    /// One line under the screen: a save that failed, a refusal.
    notice: Option<String>,
}

impl DeckScreen {
    pub fn open(dir: PathBuf, registry: CardRegistry, format: NsgFormat) -> Result<Self, String> {
        let catalog = netrunner_client::cards::catalog(&registry);
        let mut screen = DeckScreen {
            dir,
            registry,
            catalog,
            format,
            picker: Picker::open_all(format, Vec::new(), None),
            searching: false,
            mode: Mode::List,
            notice: None,
        };
        screen.reload(None);
        Ok(screen)
    }

    fn book(&self) -> CardBook<'_> {
        CardBook::new(&self.registry, &self.catalog)
    }

    /// Re-reads the directory into the list, keeping the search and the
    /// format filter, with the cursor on `onto` when it names a deck —
    /// after a save or a copy — else where it was. A deck file that
    /// cannot be read is the notice, never a reason to list nothing.
    fn reload(&mut self, onto: Option<&str>) {
        let (rows, problems) = deck_picker::rows(Some(&self.dir), self.book(), self.format);
        let was = self.picker.under_cursor().map(|row| row.deck.id.clone());
        let chosen = onto.map(str::to_string).or(was);
        let mut picker = Picker::open_all(self.format, rows, chosen.as_deref());
        picker.apply(ListIntent::Query(self.picker.query.clone()));
        picker.apply(ListIntent::Legal(self.picker.legal));
        self.picker = picker;
        if !problems.is_empty() {
            self.notice = Some(problems.join(" · "));
        }
    }

    fn card_title(&self, id: &CardId) -> String {
        self.book().title(id)
    }

    fn open_editor(&mut self, deck: DeckFile) {
        let state = State::open(deck, false, self.book(), self.format);
        self.mode = Mode::Edit(Box::new(Editor::new(state)));
    }

    /// Writes `deck`, reporting a failure under the screen rather than
    /// losing the edit: the editor keeps the deck either way.
    fn save(&mut self, deck: &DeckFile) {
        if let Err(error) = deck_store::save(&self.dir, deck) {
            self.notice = Some(format!("Not saved: {error}"));
        }
    }

    /// Every id a new deck's must not be: the saved decks' and every
    /// built-in one's — the test decks too, which the list leaves out
    /// but the store still refuses a clash with.
    fn taken(&self) -> Vec<String> {
        deck_store::list_lenient(&self.dir).0.into_iter().map(|stored| stored.deck.id).collect()
    }

    /// Creates the deck a name prompt was for, saves it and opens it.
    fn create(&mut self, purpose: NamePurpose, name: &str) {
        let taken = self.taken();
        let deck = match purpose {
            NamePurpose::New { identity } => match self.book().get(&identity) {
                Some(identity) => deck_builder::new_deck(name, identity, &taken),
                None => {
                    self.notice = Some(format!("{} is not a card the builder knows", identity.0));
                    self.mode = Mode::List;
                    return;
                }
            },
            // The copy keeps the list, its notes and its style — the point of
            // copying a published deck is to start from all of it — and is
            // `Custom`, because it is no longer the published list.
            NamePurpose::Copy { from } => deck_builder::copy_of(&from, name, &taken),
        };
        match deck_store::save(&self.dir, &deck) {
            Ok(_) => {
                self.reload(Some(&deck.id));
                self.open_editor(deck);
            }
            Err(error) => {
                self.notice = Some(format!("Not saved: {error}"));
                self.mode = Mode::List;
            }
        }
    }

    pub fn key(&mut self, key: KeyCode) -> DeckKey {
        self.notice = None;
        match std::mem::replace(&mut self.mode, Mode::List) {
            Mode::List => return self.list_key(key),
            Mode::View { index, mut scroll } => match key {
                KeyCode::Up | KeyCode::Char('k') => {
                    scroll = scroll.saturating_sub(1);
                    self.mode = Mode::View { index, scroll };
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    scroll = scroll.saturating_add(1);
                    self.mode = Mode::View { index, scroll };
                }
                KeyCode::Char('c') => self.start_copy(index),
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {}
                _ => self.mode = Mode::View { index, scroll },
            },
            Mode::PickSide { mut cursor } => match key {
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Down | KeyCode::Char('j') => {
                    cursor = 1 - cursor;
                    self.mode = Mode::PickSide { cursor };
                }
                KeyCode::Enter => {
                    let side = [Side::Corp, Side::Runner][cursor];
                    let identities = deck_builder::identities(&self.registry, side, self.format).into_iter().map(|card| card.id.clone()).collect();
                    self.mode = Mode::PickIdentity { side, identities, cursor: 0 };
                }
                KeyCode::Esc | KeyCode::Char('q') => {}
                _ => self.mode = Mode::PickSide { cursor },
            },
            Mode::PickIdentity { side, identities, mut cursor } => match key {
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Down | KeyCode::Char('j') if !identities.is_empty() => {
                    let delta = if matches!(key, KeyCode::Up | KeyCode::Char('k')) { -1 } else { 1 };
                    cursor = (cursor as i32 + delta).rem_euclid(identities.len() as i32) as usize;
                    self.mode = Mode::PickIdentity { side, identities, cursor };
                }
                KeyCode::Enter if !identities.is_empty() => {
                    let identity = identities[cursor].clone();
                    self.mode = Mode::Name { purpose: NamePurpose::New { identity }, text: String::new() };
                }
                KeyCode::Esc | KeyCode::Char('q') => self.mode = Mode::PickSide { cursor: usize::from(side == Side::Runner) },
                _ => self.mode = Mode::PickIdentity { side, identities, cursor },
            },
            Mode::Name { purpose, mut text } => match key {
                KeyCode::Char(c) if !c.is_control() && text.chars().count() < MAX_NAME => {
                    text.push(c);
                    self.mode = Mode::Name { purpose, text };
                }
                KeyCode::Backspace => {
                    text.pop();
                    self.mode = Mode::Name { purpose, text };
                }
                KeyCode::Enter => {
                    let name = text.trim().to_string();
                    if name.is_empty() {
                        self.mode = Mode::Name { purpose, text };
                    } else {
                        self.create(purpose, &name);
                    }
                }
                KeyCode::Esc => {}
                _ => self.mode = Mode::Name { purpose, text },
            },
            Mode::ConfirmDelete { index } => {
                if key == KeyCode::Char('y') {
                    let id = self.picker.rows[index].deck.id.clone();
                    match deck_store::delete(&self.dir, &id) {
                        Ok(()) => {
                            self.reload(None);
                            self.notice.get_or_insert_with(|| format!("Deleted {id}"));
                        }
                        Err(error) => self.notice = Some(error),
                    }
                }
            }
            Mode::Edit(mut editor) => match editor.key(key, &self.registry, self.book()) {
                EditorKey::Continue => self.mode = Mode::Edit(editor),
                EditorKey::Changed => {
                    self.save(editor.state.deck());
                    self.mode = Mode::Edit(editor);
                }
                EditorKey::Back => {
                    let id = editor.state.deck().id.clone();
                    self.reload(Some(&id));
                }
            },
        }
        DeckKey::Continue
    }

    fn list_key(&mut self, key: KeyCode) -> DeckKey {
        if self.searching {
            match key {
                KeyCode::Char(c) if !c.is_control() => {
                    let query = format!("{}{c}", self.picker.query);
                    self.picker.apply(ListIntent::Query(query));
                }
                KeyCode::Backspace => {
                    let mut query = self.picker.query.clone();
                    query.pop();
                    self.picker.apply(ListIntent::Query(query));
                }
                KeyCode::Enter | KeyCode::Down | KeyCode::Up => self.searching = false,
                KeyCode::Esc => {
                    self.searching = false;
                    self.picker.apply(ListIntent::Query(String::new()));
                }
                _ => {}
            }
            return DeckKey::Continue;
        }
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                self.picker.apply(ListIntent::Move(-1));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.picker.apply(ListIntent::Move(1));
            }
            KeyCode::Enter => {
                // Enter opens rather than chooses: an illegal saved deck
                // opens in the editor to be mended, where the picker's
                // own Choose would refuse it.
                if let Some(row) = self.picker.under_cursor() {
                    if row.saved {
                        self.open_editor(row.deck.clone());
                    } else {
                        self.mode = Mode::View { index: self.picker.cursor, scroll: 0 };
                    }
                }
            }
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Char('l') => {
                let next = match self.picker.legal {
                    None => Some(FORMATS[0]),
                    Some(current) => FORMATS.iter().position(|format| *format == current).and_then(|at| FORMATS.get(at + 1).copied()),
                };
                self.picker.apply(ListIntent::Legal(next));
            }
            KeyCode::Char('n') => self.mode = Mode::PickSide { cursor: 0 },
            KeyCode::Char('c') => {
                if self.picker.under_cursor().is_some() {
                    self.start_copy(self.picker.cursor);
                }
            }
            KeyCode::Char('d') => {
                if let Some(row) = self.picker.under_cursor() {
                    if row.saved {
                        self.mode = Mode::ConfirmDelete { index: self.picker.cursor };
                    } else {
                        self.notice = Some("Built-in decks are published lists and cannot be deleted — copy one to change it".to_string());
                    }
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => return DeckKey::Back,
            _ => {}
        }
        DeckKey::Continue
    }

    fn start_copy(&mut self, index: usize) {
        let from = self.picker.rows[index].deck.clone();
        let names: Vec<String> = self.picker.rows.iter().map(|row| row.deck.name.clone()).collect();
        let text = deck_builder::copy_name(&from.name, &names);
        self.mode = Mode::Name { purpose: NamePurpose::Copy { from: Box::new(from) }, text };
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let [body, footer] =
            Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(0), Constraint::Length(1)]).areas(area);
        match &self.mode {
            Mode::List | Mode::ConfirmDelete { .. } => self.draw_list(frame, body),
            Mode::View { index, scroll } => self.draw_view(frame, body, &self.picker.rows[*index], *scroll),
            Mode::Edit(editor) => editor.draw(frame, body, self.book()),
            Mode::PickSide { cursor } => {
                let rows = vec!["Corp".to_string(), "Runner".to_string()];
                draw_choice(frame, body, "New deck — which side? (Enter picks, Esc cancels)", rows, *cursor);
            }
            Mode::PickIdentity { side, identities, cursor } => {
                let rows = identities
                    .iter()
                    .map(|id| {
                        let card = self.book().get(id);
                        let faction = card.and_then(|card| card.faction).map_or("", faction_label);
                        let size = card.and_then(|card| card.min_deck_size).map_or(String::new(), |size| format!(" · {size} cards"));
                        let influence = card.map_or(String::new(), |card| match (card.unlimited_influence, card.influence_limit) {
                            (true, _) => " · no influence limit".to_string(),
                            (false, Some(limit)) => format!(" · {limit} influence"),
                            (false, None) => String::new(),
                        });
                        format!("{} — {faction}{size}{influence}", self.card_title(id))
                    })
                    .collect();
                draw_choice(frame, body, &format!("New {side:?} deck — which identity? (Enter picks, Esc goes back)"), rows, *cursor);
            }
            Mode::Name { purpose, text } => {
                self.draw_list(frame, body);
                let title = match purpose {
                    NamePurpose::New { identity } => format!("Name the new {} deck", self.card_title(identity)),
                    NamePurpose::Copy { from } => format!("Name the copy of {}", from.name),
                };
                let modal = Modal::new(&title, &format!("{text}▏"), "Enter creates it, Esc cancels");
                super::draw_modal(frame, &modal);
            }
        }
        if let Mode::ConfirmDelete { index } = &self.mode {
            let name = &self.picker.rows[*index].deck.name;
            super::draw_modal(frame, &Modal::new("Delete deck", &format!("Delete {name}? This cannot be undone."), "y deletes it, any other key keeps it"));
        }
        let help = match &self.mode {
            Mode::List | Mode::ConfirmDelete { .. } if self.searching => "Type to search names and identities · Enter keeps it · Esc clears it",
            Mode::List | Mode::ConfirmDelete { .. } => "Enter opens · / search · l legal in · n new deck · c copy · d delete · Esc back",
            Mode::View { .. } => "Up/Down scroll · c copy it to edit · Esc back",
            Mode::Edit(editor) => editor.help(),
            _ => "",
        };
        let line = match &self.notice {
            Some(notice) => Line::from(Span::styled(notice.clone(), Style::default().fg(Color::Red))),
            None => Line::from(Span::styled(help, Style::default().fg(Color::DarkGray))),
        };
        frame.render_widget(Paragraph::new(line), footer);
    }

    /// The list: the rows the search and the format filter leave, each
    /// as the picker tells it — the side, the name, the identity, the
    /// size, whose it is, its standing here, and where it is legal.
    fn draw_list(&self, frame: &mut Frame, area: Rect) {
        let visible = self.picker.visible();
        let items: Vec<ListItem> = visible
            .iter()
            .map(|&index| {
                let row = &self.picker.rows[index];
                let deck = &row.deck;
                let origin = if row.saved { "yours" } else { "built-in" };
                let (mark, color) = match &row.status.standing {
                    Standing::Legal => ("legal", Color::Green),
                    Standing::Illegal(_) => ("not legal", Color::Yellow),
                    Standing::Unplayable(_) => ("not playable", Color::Red),
                };
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{:<7}{} · {} · {} cards · {origin} · ", format!("{:?}", deck.side), deck.name, row.identity, deck.size())),
                    Span::styled(mark, Style::default().fg(color)),
                    Span::styled(format!("  legal in {}", row.legal_words()), Style::default().fg(Color::DarkGray)),
                ]))
            })
            .collect();
        let mut state = ListState::default();
        if let Some(at) = visible.iter().position(|index| *index == self.picker.cursor) {
            state.select(Some(at));
        }
        let mut title = format!("Decks — checked against {}", format_label(self.format));
        if let Some(legal) = self.picker.legal {
            title.push_str(&format!(" · legal in {}", format_label(legal)));
        }
        if !self.picker.query.is_empty() || self.searching {
            title.push_str(&format!(" · \"{}{}\"", self.picker.query, if self.searching { "▏" } else { "" }));
        }
        frame.render_stateful_widget(
            List::new(items)
                .block(Block::default().borders(Borders::ALL).title(title))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
            area,
            &mut state,
        );
    }

    fn draw_view(&self, frame: &mut Frame, area: Rect, row: &Row, scroll: u16) {
        let deck = &row.deck;
        let book = self.book();
        let mut lines = vec![
            Line::from(Span::styled(deck.name.clone(), Style::default().add_modifier(Modifier::BOLD))),
            Line::from(format!("{:?} · {} · style: {}", deck.side, row.identity, deck.style_label())),
            verdict_line(&row.status.standing, self.format, &row.status.legal_in),
        ];
        if let Some(description) = &deck.description {
            lines.push(Line::from(""));
            lines.push(Line::from(description.clone()));
        }
        lines.push(Line::from(""));
        for (id, count) in deck_builder::entries(deck, book) {
            let card = book.get(&id);
            lines.push(Line::from(format!("{count}× {}  ({})", book.title(&id), card.map_or("?", |card| type_group(&card.card_type)))));
        }
        if let Some(how_to_play) = &deck.how_to_play {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("How to play", Style::default().add_modifier(Modifier::BOLD))));
            lines.extend(how_to_play.lines().map(|line| Line::from(line.to_string())));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0))
                .block(Block::default().borders(Borders::ALL).title(format!("{} — built-in, read-only", deck.name))),
            area,
        );
    }
}

/// Which list the editor's keys move in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Pool,
    Deck,
}

/// The terminal's builder over the shared editor: which pane the keys
/// are in, where its cursors are, and whether a box has the keys.
struct Editor {
    state: State,
    focus: Focus,
    /// The search box has the keys.
    searching: bool,
    /// The rename box has the keys, holding what has been typed.
    renaming: Option<String>,
    pool_cursor: usize,
    deck_cursor: usize,
    /// A card's text, open over the editor.
    inspect: Option<Modal>,
}

#[derive(Debug, PartialEq, Eq)]
enum EditorKey {
    Continue,
    /// The deck changed; the screen saves it.
    Changed,
    Back,
}

impl Editor {
    fn new(state: State) -> Self {
        Editor { state, focus: Focus::Pool, searching: false, renaming: None, pool_cursor: 0, deck_cursor: 0, inspect: None }
    }

    fn deck_rows(&self, book: CardBook) -> Vec<(CardId, u32)> {
        deck_builder::entries(self.state.deck(), book)
    }

    fn selected(&self, book: CardBook) -> Option<CardId> {
        match self.focus {
            Focus::Pool => self.state.pool(book).get(self.pool_cursor).map(|card| card.id.clone()),
            Focus::Deck => self.deck_rows(book).get(self.deck_cursor).map(|(id, _)| id.clone()),
        }
    }

    fn move_cursor(&mut self, delta: i32, book: CardBook) {
        let (cursor, len) = match self.focus {
            Focus::Pool => {
                let len = self.state.pool(book).len();
                (&mut self.pool_cursor, len)
            }
            Focus::Deck => {
                let len = self.deck_rows(book).len();
                (&mut self.deck_cursor, len)
            }
        };
        if len > 0 {
            *cursor = (*cursor as i32 + delta).clamp(0, len as i32 - 1) as usize;
        }
    }

    /// The next of `all` after `current`, or the first with none, or
    /// none after the last: how one key cycles a filter.
    fn next_of<T: Copy + PartialEq>(all: &[T], current: Option<T>) -> Option<T> {
        match current.and_then(|current| all.iter().position(|item| *item == current)) {
            None => all.first().copied(),
            Some(index) => all.get(index + 1).copied(),
        }
    }

    /// Applies an intent, keeping the cursors on the lists it changed.
    fn apply(&mut self, intent: Intent, book: CardBook) -> EditorKey {
        let filters = matches!(intent, Intent::Faction(_) | Intent::Kind(_) | Intent::Query(_) | Intent::Format(_) | Intent::Set(_) | Intent::Sort(_) | Intent::ClearFilters);
        match self.state.apply(intent, book) {
            Outcome::Nothing | Outcome::View => {
                if filters {
                    self.pool_cursor = 0;
                }
                EditorKey::Continue
            }
            Outcome::Save => {
                self.deck_cursor = self.deck_cursor.min(self.deck_rows(book).len().saturating_sub(1));
                EditorKey::Changed
            }
        }
    }

    fn key(&mut self, key: KeyCode, registry: &CardRegistry, book: CardBook) -> EditorKey {
        if self.inspect.is_some() {
            if matches!(key, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('i') | KeyCode::Char(' ') | KeyCode::Char('q')) {
                self.inspect = None;
            }
            return EditorKey::Continue;
        }
        if let Some(name) = &mut self.renaming {
            match key {
                KeyCode::Char(c) if !c.is_control() && name.chars().count() < MAX_NAME => name.push(c),
                KeyCode::Backspace => {
                    name.pop();
                }
                KeyCode::Enter => {
                    let name = self.renaming.take().unwrap_or_default().trim().to_string();
                    if !name.is_empty() {
                        return self.apply(Intent::Rename(name), book);
                    }
                }
                KeyCode::Esc => self.renaming = None,
                _ => {}
            }
            return EditorKey::Continue;
        }
        if self.searching {
            let query = self.state.filter.query.clone();
            match key {
                KeyCode::Char(c) if !c.is_control() => return self.apply(Intent::Query(format!("{query}{c}")), book),
                KeyCode::Backspace => {
                    let mut query = query;
                    query.pop();
                    return self.apply(Intent::Query(query), book);
                }
                KeyCode::Enter | KeyCode::Down => self.searching = false,
                KeyCode::Esc => {
                    self.searching = false;
                    return self.apply(Intent::Query(String::new()), book);
                }
                _ => {}
            }
            return EditorKey::Continue;
        }
        let side = self.state.deck().side;
        match key {
            KeyCode::Esc | KeyCode::Char('q') => return EditorKey::Back,
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = if self.focus == Focus::Pool { Focus::Deck } else { Focus::Pool };
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1, book),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1, book),
            KeyCode::PageUp => self.move_cursor(-10, book),
            KeyCode::PageDown => self.move_cursor(10, book),
            KeyCode::Enter | KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Right | KeyCode::Char('l') => {
                if let Some(id) = self.selected(book) {
                    return self.apply(Intent::Add(id), book);
                }
            }
            KeyCode::Char('-') | KeyCode::Backspace | KeyCode::Delete | KeyCode::Left | KeyCode::Char('h') => {
                if let Some(id) = self.selected(book) {
                    return self.apply(Intent::Remove(id), book);
                }
            }
            KeyCode::Char('/') => {
                self.searching = true;
                self.focus = Focus::Pool;
            }
            KeyCode::Char('t') => {
                let next = Self::next_of(deck_builder::kinds(side), self.state.filter.kind);
                return self.apply(Intent::Kind(next), book);
            }
            KeyCode::Char('f') => {
                let next = Self::next_of(&deck_builder::factions(book, side), self.state.filter.faction);
                return self.apply(Intent::Faction(next), book);
            }
            KeyCode::Char('g') => {
                // The format cycles through every one, never to "none":
                // the pool is always some format's.
                let at = FORMATS.iter().position(|format| *format == self.state.filter.format).unwrap_or(0);
                return self.apply(Intent::Format(FORMATS[(at + 1) % FORMATS.len()]), book);
            }
            KeyCode::Char('s') => {
                let at = PoolSort::ALL.iter().position(|sort| *sort == self.state.filter.sort).unwrap_or(0);
                return self.apply(Intent::Sort(PoolSort::ALL[(at + 1) % PoolSort::ALL.len()]), book);
            }
            KeyCode::Char('x') => return self.apply(Intent::ClearFilters, book),
            KeyCode::Char('i') | KeyCode::Char(' ') => {
                if let Some(id) = self.selected(book) {
                    self.inspect = Some(card_modal(&id, registry));
                }
            }
            KeyCode::Char('r') => self.renaming = Some(self.state.deck().name.clone()),
            KeyCode::Char('y') => {
                // Balanced, then each of the side's plans: a stack is
                // written into the file by hand, and cycling away from
                // one puts a single plan in its place.
                let styles: Vec<Option<String>> = self.state.styles().into_iter().map(|style| style.map(|plan| plan.name().to_string())).collect();
                let current = self.state.deck().style.clone();
                let at = styles.iter().position(|style| style.iter().cloned().collect::<Vec<_>>() == current).unwrap_or(0);
                return self.apply(Intent::Style(styles[(at + 1) % styles.len()].clone()), book);
            }
            _ => {}
        }
        EditorKey::Continue
    }

    fn help(&self) -> &'static str {
        if self.searching {
            "Type to search titles, types and text · Enter keeps it · Esc clears it"
        } else if self.renaming.is_some() {
            "Type the deck's name · Enter renames · Esc cancels"
        } else {
            "Enter/+ add · -/Backspace remove · Tab pool/deck · / search · g legal in · t type · f faction · s sort · x clear · i card text · r rename · y style · Esc back"
        }
    }

    fn draw(&self, frame: &mut Frame, area: Rect, book: CardBook) {
        let [header, body] =
            Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(4), Constraint::Min(0)]).areas(area);
        let deck = self.state.deck();
        let identity = book.get(&deck.identity);
        let name = match &self.renaming {
            Some(text) => format!("{text}▏"),
            None => deck.name.clone(),
        };
        let mut lines = vec![
            Line::from(vec![
                Span::styled(name, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("  {} · bot style: {} · saved as you go", book.title(&deck.identity), deck.style_label())),
            ]),
            tally_line(self.state.draft.tally(book).as_ref()),
            verdict_line(&self.state.status.standing, self.state.format, &self.state.status.legal_in),
        ];
        if let Some(note) = &self.state.note {
            lines.push(Line::from(Span::styled(note.clone(), Style::default().fg(Color::Red))));
        }
        frame.render_widget(Paragraph::new(lines), header);

        let [deck_area, pool_area] =
            Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(40), Constraint::Percentage(60)]).areas(body);
        let active = Style::default().fg(Color::Yellow);
        let deck_items: Vec<ListItem> = self
            .deck_rows(book)
            .into_iter()
            .map(|(id, count)| {
                let card = book.get(&id);
                let pips = match (card, identity) {
                    (Some(card), Some(identity)) => "●".repeat((influence_per_copy(card, identity) * count) as usize),
                    _ => String::new(),
                };
                let group = card.map_or("?", |card| type_group(&card.card_type));
                let playable = if book.is_playable(&id) { "" } else { " · not playable yet" };
                ListItem::new(format!("{count}× {}  ({group}) {pips}{playable}", book.title(&id)))
            })
            .collect();
        let mut deck_state = ListState::default();
        if !deck_items.is_empty() && self.focus == Focus::Deck {
            deck_state.select(Some(self.deck_cursor));
        }
        frame.render_stateful_widget(
            List::new(deck_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Deck — {} cards", deck.size()))
                        .border_style(if self.focus == Focus::Deck { active } else { Style::default() }),
                )
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
            deck_area,
            &mut deck_state,
        );

        let pool_items: Vec<ListItem> = self
            .state
            .pool(book)
            .into_iter()
            .map(|card| {
                let pips = identity.map_or(0, |identity| influence_per_copy(card, identity));
                let pips = if pips == 0 { String::new() } else { "●".repeat(pips as usize) };
                let copies = self.state.draft.copies(&card.id);
                let copies = if copies == 0 { String::new() } else { format!("  ×{copies}") };
                ListItem::new(format!(
                    "{:<30} {:<9} {:<12} {:>2}c {:<5}{copies}",
                    card.title,
                    type_group(&card.card_type),
                    card.faction.map_or("", faction_label),
                    card.cost,
                    pips
                ))
            })
            .collect();
        let mut pool_state = ListState::default();
        if !pool_items.is_empty() && self.focus == Focus::Pool {
            pool_state.select(Some(self.pool_cursor));
        }
        let filter = &self.state.filter;
        let mut filters = vec![format_label(filter.format).to_string()];
        filters.extend(filter.kind.map(str::to_string));
        filters.extend(filter.faction.map(|faction| faction_label(faction).to_string()));
        if filter.sort != PoolSort::Type {
            filters.push(format!("by {}", filter.sort.label().to_lowercase()));
        }
        if !filter.query.is_empty() || self.searching {
            filters.push(format!("\"{}{}\"", filter.query, if self.searching { "▏" } else { "" }));
        }
        frame.render_stateful_widget(
            List::new(pool_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Cards — {}", filters.join(" · ")))
                        .border_style(if self.focus == Focus::Pool { active } else { Style::default() }),
                )
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
            pool_area,
            &mut pool_state,
        );
        if let Some(modal) = &self.inspect {
            super::draw_modal(frame, modal);
        }
    }
}

fn draw_choice(frame: &mut Frame, area: Rect, title: &str, rows: Vec<String>, cursor: usize) {
    let items: Vec<ListItem> = rows.into_iter().map(ListItem::new).collect();
    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(cursor));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title.to_string()))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
        &mut state,
    );
}

/// "Cards 38/40 · Influence 12/15 · Agenda points 16 of 18–20", each part
/// yellow while it is short or over and plain once it is in range.
fn tally_line(tally: Option<&DeckTally>) -> Line<'static> {
    let Some(tally) = tally else { return Line::from("") };
    let colored = |text: String, ok: bool| Span::styled(text, Style::default().fg(if ok { Color::Green } else { Color::Yellow }));
    let mut spans = vec![colored(format!("Cards {}/{}", tally.size, tally.min_size), tally.size >= tally.min_size)];
    spans.push(Span::raw(" · "));
    spans.push(match tally.influence_limit {
        Some(limit) => colored(format!("Influence {}/{limit}", tally.influence_spent), tally.influence_spent <= limit),
        None => colored(format!("Influence {} (no limit)", tally.influence_spent), true),
    });
    if let Some(agenda) = tally.agenda {
        spans.push(Span::raw(" · "));
        spans.push(colored(
            format!("Agenda points {} of {}–{}", agenda.points, agenda.min, agenda.max),
            (agenda.min..=agenda.max).contains(&agenda.points),
        ));
    }
    Line::from(spans)
}

/// The verdict in `format`, and every other format the deck is legal in.
fn verdict_line(standing: &Standing, format: NsgFormat, legal_in: &[NsgFormat]) -> Line<'static> {
    let others: Vec<&str> = legal_in.iter().filter(|other| **other != format).map(|other| format_label(*other)).collect();
    let also = if others.is_empty() { String::new() } else { format!(" · also legal in {}", others.join(", ")) };
    match standing {
        Standing::Legal => Line::from(Span::styled(format!("Legal in {}{also}", format_label(format)), Style::default().fg(Color::Green))),
        Standing::Illegal(reason) => Line::from(Span::styled(format!("Not legal yet: {reason}{also}"), Style::default().fg(Color::Yellow))),
        Standing::Unplayable(reason) => Line::from(Span::styled(format!("Can't be played: {reason}"), Style::default().fg(Color::Red))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::decks::DeckCategory;
    use netrunner_core::dsl::CardType;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("netrunner_builder_{name}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn screen(dir: &Scratch, format: NsgFormat) -> DeckScreen {
        DeckScreen::open(dir.0.clone(), netrunner_client::decks::sample_deck_registry(), format).unwrap()
    }

    fn press(screen: &mut DeckScreen, keys: &[KeyCode]) {
        for key in keys {
            screen.key(*key);
        }
    }

    fn type_text(screen: &mut DeckScreen, text: &str) {
        for c in text.chars() {
            screen.key(KeyCode::Char(c));
        }
    }

    fn editor(screen: &DeckScreen) -> &Editor {
        match &screen.mode {
            Mode::Edit(editor) => editor,
            other => panic!("not editing: {other:?}"),
        }
    }

    fn on_disk(dir: &Scratch, id: &str) -> DeckFile {
        deck_store::read_file(&dir.0.join(format!("{id}.json"))).unwrap()
    }

    fn row_index(screen: &DeckScreen, id: &str) -> usize {
        screen.picker.rows.iter().position(|row| row.deck.id == id).unwrap()
    }

    #[test]
    fn a_new_deck_is_side_then_identity_then_name_and_is_saved_empty() {
        let dir = Scratch::new("new");
        let mut screen = screen(&dir, NsgFormat::Startup);
        press(&mut screen, &[KeyCode::Char('n'), KeyCode::Down, KeyCode::Enter]);
        let Mode::PickIdentity { side, identities, .. } = &screen.mode else { panic!("{:?}", screen.mode) };
        assert_eq!(*side, Side::Runner);
        assert!(!identities.is_empty());
        let first = identities[0].clone();
        screen.key(KeyCode::Enter);
        // `q` is a letter in a name, not "back".
        type_text(&mut screen, "My quick deck");
        screen.key(KeyCode::Enter);
        let deck = editor(&screen).state.deck();
        assert_eq!((deck.id.as_str(), deck.side, &deck.identity), ("my_quick_deck", Side::Runner, &first));
        assert!(deck.cards.is_empty());
        assert_eq!(on_disk(&dir, "my_quick_deck").name, "My quick deck");
    }

    #[test]
    fn adding_stops_at_the_copy_limit_and_every_edit_is_saved() {
        let dir = Scratch::new("edit");
        let mut screen = screen(&dir, NsgFormat::Startup);
        press(&mut screen, &[KeyCode::Char('n'), KeyCode::Enter, KeyCode::Enter]);
        type_text(&mut screen, "Limit");
        press(&mut screen, &[KeyCode::Enter, KeyCode::Char('/')]);
        type_text(&mut screen, "hedge fund");
        press(&mut screen, &[KeyCode::Enter]);
        press(&mut screen, &[KeyCode::Enter, KeyCode::Enter, KeyCode::Enter, KeyCode::Enter]);
        let deck = editor(&screen).state.deck();
        assert_eq!(deck.size(), 3, "the fourth copy is refused");
        assert!(editor(&screen).state.note.as_deref().is_some_and(|note| note.contains("limit of 3")));
        assert_eq!(on_disk(&dir, "limit").size(), 3);
        press(&mut screen, &[KeyCode::Tab, KeyCode::Char('-'), KeyCode::Char('-')]);
        assert_eq!(on_disk(&dir, "limit").size(), 1);
        press(&mut screen, &[KeyCode::Char('-'), KeyCode::Char('-')]);
        assert_eq!(on_disk(&dir, "limit").size(), 0, "removing past zero is a no-op");
    }

    #[test]
    fn a_built_in_deck_opens_read_only_and_copies_into_a_legal_editable_deck() {
        let dir = Scratch::new("copy");
        let mut screen = screen(&dir, NsgFormat::Startup);
        screen.picker.cursor = row_index(&screen, "brick_stack");
        screen.key(KeyCode::Enter);
        assert!(matches!(screen.mode, Mode::View { .. }), "built-in decks are read, not edited");
        screen.key(KeyCode::Char('c'));
        screen.key(KeyCode::Enter);
        let deck = editor(&screen).state.deck().clone();
        assert_eq!(deck.id, "brick_stack_copy");
        assert_eq!(deck.category, DeckCategory::Custom);
        assert_eq!(deck.size(), 44);
        assert_eq!(deck.style, vec!["glacier".to_string(), "fast-advance".to_string()], "the copy keeps the list's style");
        deck.validate(&screen.registry, NsgFormat::Startup).expect("a copy of a legal deck is legal");
        screen.key(KeyCode::Esc);
        assert!(matches!(screen.mode, Mode::List));
        let row = screen.picker.under_cursor().expect("the cursor is on the copy");
        assert!(row.saved && row.deck.id == "brick_stack_copy");
    }

    #[test]
    fn a_saved_deck_deletes_after_confirmation_and_a_built_in_one_never_does() {
        let dir = Scratch::new("delete");
        let mut screen = screen(&dir, NsgFormat::Startup);
        let builtin = screen.picker.rows.len();
        screen.key(KeyCode::Char('d'));
        assert!(screen.notice.as_deref().is_some_and(|notice| notice.contains("cannot be deleted")));
        press(&mut screen, &[KeyCode::Char('c'), KeyCode::Enter, KeyCode::Esc]);
        assert_eq!(screen.picker.rows.len(), builtin + 1);
        assert!(screen.picker.rows[0].saved, "saved decks list first");
        screen.picker.cursor = 0;
        press(&mut screen, &[KeyCode::Char('d'), KeyCode::Char('n')]);
        assert_eq!(screen.picker.rows.len(), builtin + 1, "anything but y keeps it");
        press(&mut screen, &[KeyCode::Char('d'), KeyCode::Char('y')]);
        assert_eq!(screen.picker.rows.len(), builtin);
        assert!(deck_store::read_dir(&dir.0).unwrap().is_empty());
    }

    /// The pool is the shared editor's: the deck's side, in the format
    /// the "Legal in" filter names, and `t`, `/`, `g` and `x` are its
    /// intents.
    #[test]
    fn the_pool_is_the_decks_side_in_the_format_and_filters_narrow_it() {
        let dir = Scratch::new("pool");
        let mut screen = screen(&dir, NsgFormat::Startup);
        screen.picker.cursor = row_index(&screen, "brick_stack");
        press(&mut screen, &[KeyCode::Char('c'), KeyCode::Enter]);
        // A book of its own, so the screen stays free to take keys.
        let registry = netrunner_client::decks::sample_deck_registry();
        let catalog = netrunner_client::cards::catalog(&registry);
        let book = CardBook::new(&registry, &catalog);
        let startup = editor(&screen).state.pool(book);
        for card in &startup {
            assert_eq!(card.side, Side::Corp);
            assert!(NsgFormat::Startup.rules().in_pool(&card.id), "{} is outside Startup", card.title);
        }
        let all = startup.len();
        screen.key(KeyCode::Char('t'));
        assert_eq!(editor(&screen).state.filter.kind, Some("Agenda"));
        assert!(editor(&screen).state.pool(book).iter().all(|card| card.card_type == CardType::Agenda));
        press(&mut screen, &[KeyCode::Char('x'), KeyCode::Char('/')]);
        type_text(&mut screen, "hedge");
        let found: Vec<String> = editor(&screen).state.pool(book).iter().map(|card| card.title.clone()).collect();
        assert_eq!(found, vec!["Hedge Fund".to_string()]);
        screen.key(KeyCode::Esc);
        assert_eq!(editor(&screen).state.pool(book).len(), all, "Esc clears the search");
        screen.key(KeyCode::Char('g'));
        assert_eq!(editor(&screen).state.filter.format, NsgFormat::Standard, "g moves the format on");
        screen.key(KeyCode::Char('g'));
        assert_eq!(editor(&screen).state.filter.format, NsgFormat::Eternal, "in the settings' order");
        assert!(editor(&screen).state.pool(book).len() > all, "Eternal adds the Core Set");
        assert!(registry.get(&CardId("hedge_fund".into())).is_some());
    }

    #[test]
    fn rename_and_style_are_saved_and_the_id_stays() {
        let dir = Scratch::new("rename");
        let mut screen = screen(&dir, NsgFormat::Startup);
        press(&mut screen, &[KeyCode::Char('n'), KeyCode::Enter, KeyCode::Enter]);
        type_text(&mut screen, "Old");
        press(&mut screen, &[KeyCode::Enter, KeyCode::Char('r'), KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace]);
        type_text(&mut screen, "New name");
        press(&mut screen, &[KeyCode::Enter, KeyCode::Char('y')]);
        let saved = on_disk(&dir, "old");
        assert_eq!(saved.name, "New name");
        assert_eq!(saved.style, vec!["glacier".to_string()], "the first Corp plan after balanced");
        netrunner_bots::Style::for_deck(&saved).expect("a style the bots can read");
    }

    /// The screen draws: the list with each row's formats and the
    /// filters in its title, a built-in deck read, and the editor with
    /// its tally, verdict, deck pane and the pool's filters — nothing
    /// panics, and the words the shared editor and picker supply are on
    /// the screen.
    #[test]
    fn the_list_the_viewer_and_the_editor_draw() {
        use ratatui::backend::TestBackend;
        let dir = Scratch::new("draw");
        let mut screen = screen(&dir, NsgFormat::Startup);
        let drawn = |screen: &DeckScreen| {
            let mut terminal = ratatui::Terminal::new(TestBackend::new(160, 40)).unwrap();
            terminal.draw(|frame| screen.draw(frame, frame.area())).unwrap();
            let buffer = terminal.backend().buffer().clone();
            (0..buffer.area.height).map(|y| (0..buffer.area.width).map(|x| buffer[(x, y)].symbol().to_string()).collect::<String>()).collect::<Vec<_>>().join("\n")
        };
        screen.picker.cursor = row_index(&screen, "brick_stack");
        screen.key(KeyCode::Char('l'));
        let text = drawn(&screen);
        for wanted in ["Decks — checked against Startup · legal in Startup", "Brick Stack · Weyland Consortium: Built to Last · 44 cards · built-in · legal", "legal in Startup, Standard, Eternal, Unrestricted", "/ search · l legal in"] {
            assert!(text.contains(wanted), "the list lacks {wanted:?}:\n{text}");
        }
        screen.key(KeyCode::Enter);
        let text = drawn(&screen);
        assert!(text.contains("Brick Stack — built-in, read-only") && text.contains("Legal in Startup · also legal in"), "{text}");
        press(&mut screen, &[KeyCode::Char('c'), KeyCode::Enter, KeyCode::Char('t'), KeyCode::Char('s')]);
        let text = drawn(&screen);
        for wanted in ["Brick Stack (copy)", "bot style: glacier+fast-advance · saved as you go", "Cards 44/40 · Influence 15/15 · Agenda points 18 of 18–19", "Legal in Startup · also legal in", "Deck — 44 cards", "Cards — Startup · Agenda · by title", "g legal in · t type · f faction · s sort"] {
            assert!(text.contains(wanted), "the editor lacks {wanted:?}:\n{text}");
        }
    }

    /// The list is the picker's: no test deck is in it, `/` searches by
    /// a deck's name or its identity's, `l` narrows to a format, and a
    /// copy made while a search is on is found under the cursor after.
    #[test]
    fn the_list_is_the_pickers_with_its_search_and_no_test_deck() {
        let dir = Scratch::new("list");
        let mut screen = screen(&dir, NsgFormat::Startup);
        assert!(screen.picker.rows.iter().all(|row| row.deck.category != DeckCategory::Sweep), "test decks are not listed");
        assert!(screen.picker.rows.len() < netrunner_core::decks::embedded_decks().len());
        let all = screen.picker.visible().len();
        screen.key(KeyCode::Char('/'));
        type_text(&mut screen, "built to last");
        let shown: Vec<&str> = screen.picker.visible().iter().map(|&i| screen.picker.rows[i].deck.id.as_str()).collect();
        assert!(shown.contains(&"brick_stack") && shown.len() < all, "found by the identity: {shown:?}");
        screen.key(KeyCode::Enter);
        assert!(!screen.searching, "Enter keeps the search");
        screen.picker.cursor = row_index(&screen, "brick_stack");
        press(&mut screen, &[KeyCode::Char('c'), KeyCode::Enter, KeyCode::Esc]);
        assert_eq!(screen.picker.under_cursor().map(|row| row.deck.id.as_str()), Some("brick_stack_copy"), "the copy is under the cursor");
        assert_eq!(screen.picker.query, "built to last", "and the search is kept");
        press(&mut screen, &[KeyCode::Char('/'), KeyCode::Esc]);
        assert_eq!(screen.picker.visible().len(), all + 1, "Esc clears the search");
        screen.key(KeyCode::Char('l'));
        assert_eq!(screen.picker.legal, Some(NsgFormat::Startup));
        assert!(screen.picker.visible().iter().all(|&i| screen.picker.rows[i].status.legal_in.contains(&NsgFormat::Startup)));
    }
}
