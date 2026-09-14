//! The deck builder: the main menu's Decks screen.
//!
//! Everything `netrunner_cli deck new/add/remove/show/validate` does, in
//! the TUI — list every deck, read a built-in one, start a deck from an
//! identity or from a copy of any deck, then edit it against a card pool
//! with the deck's running totals and its legality always on screen.
//!
//! **Legality comes from the validators, never from here.** The verdict
//! line is `DeckFile::validate` (both of them, as starting a game runs
//! them), and the running totals are `DeckFile::tally`, which shares the
//! gate's influence rule and agenda range. The only rule this module
//! applies itself is the copy limit on the `+` key, read off the card
//! (`deck_limit`, else the validator's `MAX_COPIES_PER_CARD`) — a builder
//! that let a fourth copy in only to flag it would be a worse builder, and
//! the validator still has the last word.
//!
//! **The pool is the format's.** Only cards legal in the format Settings
//! names are offered, so a Startup player is never shown a Core Set card
//! the validator would refuse; a deck that already holds one (a copy of an
//! Eternal list) still shows and can remove it.
//!
//! **Every edit is saved as it is made**, as `deck add` saves on every
//! add: a deck under construction is legitimately illegal, so there is no
//! "valid enough to save" point to wait for, and nothing to lose on a
//! quit. Built-in decks are read-only — they are published lists — and
//! are edited by copying.
//!
//! The same shape as the menu: a plain struct, `key(KeyCode)` tested
//! without a terminal, `draw` a thin layer over it.

use std::path::PathBuf;

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use netrunner_bots::Personality;
use netrunner_core::card::Faction;
use netrunner_core::cards::CardRegistry;
use netrunner_core::deck::validator::MAX_COPIES_PER_CARD;
use netrunner_core::deck::{influence_per_copy, DeckTally};
use netrunner_core::decks::{DeckCategory, DeckEntry, DeckFile};
use netrunner_core::dsl::{CardDefinition, CardId, CardType};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

use crate::app::{card_modal, Modal};
use netrunner_client::cards::legal_in;
use netrunner_client::deck_store::{self, Origin, StoredDeck};

/// What one key did, for the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckKey {
    Continue,
    Back,
}

/// One deck in the list, with what the list shows about it.
#[derive(Debug, Clone)]
struct ListRow {
    stored: StoredDeck,
    identity: String,
    /// `Ok` if both validators pass in the current format; the first
    /// problem otherwise.
    verdict: Result<(), String>,
}

impl ListRow {
    fn saved(&self) -> bool {
        !matches!(self.stored.origin, Origin::Embedded)
    }
}

/// What a typed name is for.
#[derive(Debug, Clone)]
enum NamePurpose {
    New { side: Side, identity: CardId },
    Copy { from: Box<DeckFile> },
}

#[derive(Debug)]
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

pub struct DeckScreen {
    dir: PathBuf,
    registry: CardRegistry,
    format: NsgFormat,
    rows: Vec<ListRow>,
    cursor: usize,
    mode: Mode,
    /// One line under the screen: a save that failed, a refusal.
    notice: Option<String>,
}

/// Longest deck name the prompt accepts.
const MAX_NAME: usize = 48;

impl DeckScreen {
    pub fn open(dir: PathBuf, registry: CardRegistry, format: NsgFormat) -> Result<Self, String> {
        let mut screen = DeckScreen { dir, registry, format, rows: Vec::new(), cursor: 0, mode: Mode::List, notice: None };
        screen.reload()?;
        Ok(screen)
    }

    /// Re-reads the directory. Saved decks first — they are the player's —
    /// then the built-in ones; each group Corp before Runner, then by name.
    fn reload(&mut self) -> Result<(), String> {
        let mut rows: Vec<ListRow> = deck_store::list(&self.dir)?
            .into_iter()
            .map(|stored| {
                let identity = self.card_title(&stored.deck.identity);
                let verdict = stored.deck.validate(&self.registry, self.format).map(|_| ()).map_err(|error| error.to_string());
                ListRow { stored, identity, verdict }
            })
            .collect();
        rows.sort_by_key(|row| (!row.saved(), row.stored.deck.side == Side::Runner, row.stored.deck.name.to_lowercase()));
        self.rows = rows;
        self.cursor = self.cursor.min(self.rows.len().saturating_sub(1));
        Ok(())
    }

    /// Re-reads and puts the cursor on `id`, after a save or a copy.
    fn reload_onto(&mut self, id: &str) {
        if let Err(error) = self.reload() {
            self.notice = Some(error);
        }
        if let Some(index) = self.rows.iter().position(|row| row.stored.deck.id == id) {
            self.cursor = index;
        }
    }

    fn card_title(&self, id: &CardId) -> String {
        self.registry.get(id).map_or_else(|| id.0.clone(), |card| card.title.clone())
    }

    fn open_editor(&mut self, deck: DeckFile) {
        self.mode = Mode::Edit(Box::new(Editor::new(deck, &self.registry, self.format)));
    }

    /// Writes `deck`, reporting a failure under the screen rather than
    /// losing the edit: the editor keeps the deck either way.
    fn save(&mut self, deck: &DeckFile) {
        if let Err(error) = deck_store::save(&self.dir, deck) {
            self.notice = Some(format!("Not saved: {error}"));
        }
    }

    /// The identities a new deck may start from: the side's, legal in the
    /// format, by faction then title.
    fn identities(&self, side: Side) -> Vec<CardId> {
        let rules = self.format.rules();
        let mut identities: Vec<&CardDefinition> = self
            .registry
            .iter()
            .filter(|card| card.card_type == CardType::Identity && card.side == side && legal_in(card, &rules))
            .collect();
        identities.sort_by_key(|card| (faction_order(card.faction), card.title.clone()));
        identities.into_iter().map(|card| card.id.clone()).collect()
    }

    /// Creates the deck a name prompt was for, saves it and opens it.
    fn create(&mut self, purpose: NamePurpose, name: &str) {
        let taken: Vec<String> = self.rows.iter().map(|row| row.stored.deck.id.clone()).collect();
        let id = unique_id(name, &taken);
        let deck = match purpose {
            NamePurpose::New { side, identity } => DeckFile {
                id: id.clone(),
                name: name.to_string(),
                side,
                category: DeckCategory::Custom,
                description: None,
                how_to_play: None,
                style: None,
                identity,
                cards: Vec::new(),
            },
            // The copy keeps the list, its notes and its style — the point of
            // copying a published deck is to start from all of it — and is
            // `Custom`, because it is no longer the published list.
            NamePurpose::Copy { from } => DeckFile { id: id.clone(), name: name.to_string(), category: DeckCategory::Custom, ..*from },
        };
        match deck_store::save(&self.dir, &deck) {
            Ok(_) => {
                self.reload_onto(&id);
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
                    let identities = self.identities(side);
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
                    self.mode = Mode::Name { purpose: NamePurpose::New { side, identity }, text: String::new() };
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
                    let id = self.rows[index].stored.deck.id.clone();
                    match deck_store::delete(&self.dir, &id) {
                        Ok(()) => {
                            if let Err(error) = self.reload() {
                                self.notice = Some(error);
                            }
                            self.notice.get_or_insert_with(|| format!("Deleted {id}"));
                        }
                        Err(error) => self.notice = Some(error),
                    }
                }
            }
            Mode::Edit(mut editor) => match editor.key(key, &self.registry) {
                EditorKey::Continue => self.mode = Mode::Edit(editor),
                EditorKey::Changed => {
                    self.save(&editor.deck);
                    self.mode = Mode::Edit(editor);
                }
                EditorKey::Back => self.reload_onto(&editor.deck.id.clone()),
            },
        }
        DeckKey::Continue
    }

    fn list_key(&mut self, key: KeyCode) -> DeckKey {
        let len = self.rows.len();
        match key {
            KeyCode::Up | KeyCode::Char('k') if len > 0 => self.cursor = (self.cursor + len - 1) % len,
            KeyCode::Down | KeyCode::Char('j') if len > 0 => self.cursor = (self.cursor + 1) % len,
            KeyCode::Enter if len > 0 => {
                let row = &self.rows[self.cursor];
                if row.saved() {
                    self.open_editor(row.stored.deck.clone());
                } else {
                    self.mode = Mode::View { index: self.cursor, scroll: 0 };
                }
            }
            KeyCode::Char('n') => self.mode = Mode::PickSide { cursor: 0 },
            KeyCode::Char('c') if len > 0 => self.start_copy(self.cursor),
            KeyCode::Char('d') if len > 0 => {
                if self.rows[self.cursor].saved() {
                    self.mode = Mode::ConfirmDelete { index: self.cursor };
                } else {
                    self.notice = Some("Built-in decks are published lists and cannot be deleted — copy one to change it".to_string());
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => return DeckKey::Back,
            _ => {}
        }
        DeckKey::Continue
    }

    fn start_copy(&mut self, index: usize) {
        let from = self.rows[index].stored.deck.clone();
        let text = format!("{} (copy)", from.name);
        self.mode = Mode::Name { purpose: NamePurpose::Copy { from: Box::new(from) }, text };
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let [body, footer] =
            Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(0), Constraint::Length(1)]).areas(area);
        match &self.mode {
            Mode::List | Mode::ConfirmDelete { .. } => self.draw_list(frame, body),
            Mode::View { index, scroll } => self.draw_view(frame, body, &self.rows[*index], *scroll),
            Mode::Edit(editor) => editor.draw(frame, body, &self.registry, self.format),
            Mode::PickSide { cursor } => {
                let rows = vec!["Corp".to_string(), "Runner".to_string()];
                draw_choice(frame, body, "New deck — which side? (Enter picks, Esc cancels)", rows, *cursor);
            }
            Mode::PickIdentity { side, identities, cursor } => {
                let rows = identities
                    .iter()
                    .map(|id| {
                        let card = self.registry.get(id);
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
                    NamePurpose::New { identity, .. } => format!("Name the new {} deck", self.card_title(identity)),
                    NamePurpose::Copy { from } => format!("Name the copy of {}", from.name),
                };
                let modal = Modal::new(&title, &format!("{text}▏"), "Enter creates it, Esc cancels");
                super::draw_modal(frame, &modal);
            }
        }
        if let Mode::ConfirmDelete { index } = &self.mode {
            let name = &self.rows[*index].stored.deck.name;
            super::draw_modal(frame, &Modal::new("Delete deck", &format!("Delete {name}? This cannot be undone."), "y deletes it, any other key keeps it"));
        }
        let help = match &self.mode {
            Mode::List | Mode::ConfirmDelete { .. } => "Enter opens · n new deck · c copy · d delete · Esc back",
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

    fn draw_list(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .rows
            .iter()
            .map(|row| {
                let deck = &row.stored.deck;
                let origin = if row.saved() { "saved" } else { "built-in" };
                let (mark, color) = match &row.verdict {
                    Ok(()) => ("legal", Color::Green),
                    Err(_) => ("not legal", Color::Yellow),
                };
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{:<7}{} · {} · {} cards · {origin} · ", format!("{:?}", deck.side), deck.name, row.identity, deck.size())),
                    Span::styled(mark, Style::default().fg(color)),
                ]))
            })
            .collect();
        let mut state = ListState::default();
        if !self.rows.is_empty() {
            state.select(Some(self.cursor));
        }
        let title = format!("Decks — checked against {}", format_label(self.format));
        frame.render_stateful_widget(
            List::new(items)
                .block(Block::default().borders(Borders::ALL).title(title))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
            area,
            &mut state,
        );
    }

    fn draw_view(&self, frame: &mut Frame, area: Rect, row: &ListRow, scroll: u16) {
        let deck = &row.stored.deck;
        let mut lines = vec![
            Line::from(Span::styled(deck.name.clone(), Style::default().add_modifier(Modifier::BOLD))),
            Line::from(format!("{:?} · {} · style: {}", deck.side, row.identity, deck.style.as_deref().unwrap_or("balanced"))),
            verdict_line(&row.verdict.clone().map(|_| format_label(self.format))),
        ];
        if let Some(description) = &deck.description {
            lines.push(Line::from(""));
            lines.push(Line::from(description.clone()));
        }
        lines.push(Line::from(""));
        for (id, count) in sorted_entries(deck, &self.registry) {
            let card = self.registry.get(&id);
            lines.push(Line::from(format!("{count}× {}  ({})", self.card_title(&id), card.map_or("?", |card| type_group(&card.card_type)))));
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

#[derive(Debug)]
struct Editor {
    deck: DeckFile,
    focus: Focus,
    /// Every card the deck may hold in this format, by type then title.
    pool: Vec<CardId>,
    type_filter: Option<&'static str>,
    faction_filter: Option<Faction>,
    search: String,
    /// The search box has the keys.
    searching: bool,
    /// The rename box has the keys, holding what has been typed.
    renaming: Option<String>,
    pool_cursor: usize,
    deck_cursor: usize,
    /// A card's text, open over the editor.
    inspect: Option<Modal>,
    /// A refusal that is not an error: the copy limit, say.
    note: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum EditorKey {
    Continue,
    /// The deck changed; the screen saves it.
    Changed,
    Back,
}

impl Editor {
    fn new(deck: DeckFile, registry: &CardRegistry, format: NsgFormat) -> Self {
        let rules = format.rules();
        let mut pool: Vec<&CardDefinition> = registry
            .iter()
            .filter(|card| card.side == deck.side && card.card_type != CardType::Identity && legal_in(card, &rules))
            .collect();
        pool.sort_by_key(|card| (type_order(&card.card_type), card.title.clone()));
        let pool = pool.into_iter().map(|card| card.id.clone()).collect();
        Editor {
            deck,
            focus: Focus::Pool,
            pool,
            type_filter: None,
            faction_filter: None,
            search: String::new(),
            searching: false,
            renaming: None,
            pool_cursor: 0,
            deck_cursor: 0,
            inspect: None,
            note: None,
        }
    }

    /// The pool after the type, faction and search filters.
    fn visible_pool<'a>(&'a self, registry: &'a CardRegistry) -> Vec<&'a CardId> {
        let needle = self.search.to_lowercase();
        self.pool
            .iter()
            .filter(|id| {
                let Some(card) = registry.get(id) else { return false };
                self.type_filter.is_none_or(|group| type_group(&card.card_type) == group)
                    && self.faction_filter.is_none_or(|faction| card.faction == Some(faction))
                    && (needle.is_empty()
                        || card.title.to_lowercase().contains(&needle)
                        || card.type_line.as_deref().is_some_and(|line| line.to_lowercase().contains(&needle)))
            })
            .collect()
    }

    fn deck_rows(&self, registry: &CardRegistry) -> Vec<(CardId, u32)> {
        sorted_entries(&self.deck, registry)
    }

    fn copies(&self, id: &CardId) -> u32 {
        self.deck.cards.iter().filter(|entry| &entry.card == id).map(|entry| entry.count).sum()
    }

    fn selected(&self, registry: &CardRegistry) -> Option<CardId> {
        match self.focus {
            Focus::Pool => self.visible_pool(registry).get(self.pool_cursor).map(|id| (*id).clone()),
            Focus::Deck => self.deck_rows(registry).get(self.deck_cursor).map(|(id, _)| id.clone()),
        }
    }

    /// One more copy, up to the card's own limit.
    fn add(&mut self, id: &CardId, registry: &CardRegistry) -> bool {
        let limit = registry.get(id).and_then(|card| card.deck_limit).unwrap_or(MAX_COPIES_PER_CARD);
        if self.copies(id) >= limit {
            self.note = Some(format!("{} is at its limit of {limit}", registry.get(id).map_or(id.0.as_str(), |card| card.title.as_str())));
            return false;
        }
        match self.deck.cards.iter_mut().find(|entry| &entry.card == id) {
            Some(entry) => entry.count += 1,
            None => self.deck.cards.push(DeckEntry { card: id.clone(), count: 1 }),
        }
        true
    }

    fn remove(&mut self, id: &CardId, registry: &CardRegistry) -> bool {
        let Some(entry) = self.deck.cards.iter_mut().find(|entry| &entry.card == id) else { return false };
        entry.count -= 1;
        self.deck.cards.retain(|entry| entry.count > 0);
        self.deck_cursor = self.deck_cursor.min(self.deck_rows(registry).len().saturating_sub(1));
        true
    }

    fn move_cursor(&mut self, delta: i32, registry: &CardRegistry) {
        let (cursor, len) = match self.focus {
            Focus::Pool => {
                let len = self.visible_pool(registry).len();
                (&mut self.pool_cursor, len)
            }
            Focus::Deck => {
                let len = self.deck_rows(registry).len();
                (&mut self.deck_cursor, len)
            }
        };
        if len > 0 {
            *cursor = (*cursor as i32 + delta).clamp(0, len as i32 - 1) as usize;
        }
    }

    /// The type groups this side's pool has, for `t` to cycle through.
    fn type_groups(&self) -> &'static [&'static str] {
        match self.deck.side {
            Side::Corp => &["Agenda", "Asset", "Upgrade", "Operation", "ICE"],
            Side::Runner => &["Event", "Hardware", "Resource", "Program"],
        }
    }

    fn cycle_type(&mut self) {
        let groups = self.type_groups();
        self.type_filter = match self.type_filter.and_then(|current| groups.iter().position(|g| *g == current)) {
            None => groups.first().copied(),
            Some(index) => groups.get(index + 1).copied(),
        };
        self.pool_cursor = 0;
    }

    fn cycle_faction(&mut self, registry: &CardRegistry) {
        let mut factions: Vec<Faction> = self.pool.iter().filter_map(|id| registry.get(id).and_then(|card| card.faction)).collect();
        factions.sort_by_key(|faction| faction_order(Some(*faction)));
        factions.dedup();
        self.faction_filter = match self.faction_filter.and_then(|current| factions.iter().position(|f| *f == current)) {
            None => factions.first().copied(),
            Some(index) => factions.get(index + 1).copied(),
        };
        self.pool_cursor = 0;
    }

    /// The bot styles for this side, `None` (balanced) first.
    fn cycle_style(&mut self) {
        let mut styles: Vec<Option<&str>> = vec![None];
        styles.extend(Personality::ALL.iter().filter(|p| p.side() == Some(self.deck.side)).map(|p| Some(p.name())));
        let index = styles.iter().position(|style| *style == self.deck.style.as_deref()).unwrap_or(0);
        self.deck.style = styles[(index + 1) % styles.len()].map(str::to_string);
    }

    fn key(&mut self, key: KeyCode, registry: &CardRegistry) -> EditorKey {
        self.note = None;
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
                    if !name.is_empty() && name != self.deck.name {
                        self.deck.name = name;
                        return EditorKey::Changed;
                    }
                }
                KeyCode::Esc => self.renaming = None,
                _ => {}
            }
            return EditorKey::Continue;
        }
        if self.searching {
            match key {
                KeyCode::Char(c) if !c.is_control() => self.search.push(c),
                KeyCode::Backspace => {
                    self.search.pop();
                }
                KeyCode::Enter | KeyCode::Down => self.searching = false,
                KeyCode::Esc => {
                    self.searching = false;
                    self.search.clear();
                }
                _ => {}
            }
            self.pool_cursor = 0;
            return EditorKey::Continue;
        }
        match key {
            KeyCode::Esc | KeyCode::Char('q') => return EditorKey::Back,
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = if self.focus == Focus::Pool { Focus::Deck } else { Focus::Pool };
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1, registry),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1, registry),
            KeyCode::PageUp => self.move_cursor(-10, registry),
            KeyCode::PageDown => self.move_cursor(10, registry),
            KeyCode::Enter | KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Right | KeyCode::Char('l') => {
                if let Some(id) = self.selected(registry)
                    && self.add(&id, registry)
                {
                    return EditorKey::Changed;
                }
            }
            KeyCode::Char('-') | KeyCode::Backspace | KeyCode::Delete | KeyCode::Left | KeyCode::Char('h') => {
                if let Some(id) = self.selected(registry)
                    && self.remove(&id, registry)
                {
                    return EditorKey::Changed;
                }
            }
            KeyCode::Char('/') => {
                self.searching = true;
                self.focus = Focus::Pool;
            }
            KeyCode::Char('t') => self.cycle_type(),
            KeyCode::Char('f') => self.cycle_faction(registry),
            KeyCode::Char('x') => {
                self.type_filter = None;
                self.faction_filter = None;
                self.search.clear();
                self.pool_cursor = 0;
            }
            KeyCode::Char('i') | KeyCode::Char(' ') => {
                if let Some(id) = self.selected(registry) {
                    self.inspect = Some(card_modal(&id, registry));
                }
            }
            KeyCode::Char('r') => self.renaming = Some(self.deck.name.clone()),
            KeyCode::Char('y') => {
                self.cycle_style();
                return EditorKey::Changed;
            }
            _ => {}
        }
        EditorKey::Continue
    }

    fn help(&self) -> &'static str {
        if self.searching {
            "Type to search titles and types · Enter keeps it · Esc clears it"
        } else if self.renaming.is_some() {
            "Type the deck's name · Enter renames · Esc cancels"
        } else {
            "Enter/+ add · -/Backspace remove · Tab pool/deck · / search · t type · f faction · x clear · i card text · r rename · y style · Esc back"
        }
    }

    fn draw(&self, frame: &mut Frame, area: Rect, registry: &CardRegistry, format: NsgFormat) {
        let [header, body] =
            Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(4), Constraint::Min(0)]).areas(area);
        let identity = registry.get(&self.deck.identity);
        let identity_title = identity.map_or_else(|| self.deck.identity.0.clone(), |card| card.title.clone());
        let name = match &self.renaming {
            Some(text) => format!("{text}▏"),
            None => self.deck.name.clone(),
        };
        let verdict = self.deck.validate(registry, format).map(|_| format_label(format)).map_err(|error| error.to_string());
        let mut lines = vec![
            Line::from(vec![
                Span::styled(name, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("  {identity_title} · bot style: {} · saved as you go", self.deck.style.as_deref().unwrap_or("balanced"))),
            ]),
            tally_line(self.deck.tally(registry).ok().as_ref()),
            verdict_line(&verdict),
        ];
        if let Some(note) = &self.note {
            lines.push(Line::from(Span::styled(note.clone(), Style::default().fg(Color::Red))));
        }
        frame.render_widget(Paragraph::new(lines), header);

        let [deck_area, pool_area] =
            Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(40), Constraint::Percentage(60)]).areas(body);
        let active = Style::default().fg(Color::Yellow);
        let deck_items: Vec<ListItem> = self
            .deck_rows(registry)
            .into_iter()
            .map(|(id, count)| {
                let card = registry.get(&id);
                let pips = match (card, identity) {
                    (Some(card), Some(identity)) => "●".repeat((influence_per_copy(card, identity) * count) as usize),
                    _ => String::new(),
                };
                let title = card.map_or_else(|| id.0.clone(), |card| card.title.clone());
                let group = card.map_or("?", |card| type_group(&card.card_type));
                ListItem::new(format!("{count}× {title}  ({group}) {pips}"))
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
                        .title(format!("Deck — {} cards", self.deck.size()))
                        .border_style(if self.focus == Focus::Deck { active } else { Style::default() }),
                )
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
            deck_area,
            &mut deck_state,
        );

        let pool_items: Vec<ListItem> = self
            .visible_pool(registry)
            .into_iter()
            .map(|id| {
                let card = registry.get(id).expect("the pool is built from the registry");
                let pips = identity.map_or(0, |identity| influence_per_copy(card, identity));
                let pips = if pips == 0 { String::new() } else { "●".repeat(pips as usize) };
                let copies = self.copies(id);
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
        let mut filters = vec![format_label(format)];
        filters.extend(self.type_filter.map(str::to_string));
        filters.extend(self.faction_filter.map(|faction| faction_label(faction).to_string()));
        if !self.search.is_empty() || self.searching {
            filters.push(format!("\"{}{}\"", self.search, if self.searching { "▏" } else { "" }));
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

fn verdict_line(verdict: &Result<String, String>) -> Line<'static> {
    match verdict {
        Ok(format) => Line::from(Span::styled(format!("Legal in {format}"), Style::default().fg(Color::Green))),
        Err(error) => Line::from(Span::styled(format!("Not legal yet: {error}"), Style::default().fg(Color::Yellow))),
    }
}

/// The deck's entries merged by card and sorted as a decklist reads: by
/// type, then title.
fn sorted_entries(deck: &DeckFile, registry: &CardRegistry) -> Vec<(CardId, u32)> {
    let mut rows: Vec<(CardId, u32)> = Vec::new();
    for entry in &deck.cards {
        match rows.iter_mut().find(|(id, _)| *id == entry.card) {
            Some((_, count)) => *count += entry.count,
            None => rows.push((entry.card.clone(), entry.count)),
        }
    }
    rows.sort_by_key(|(id, _)| {
        let card = registry.get(id);
        (card.map_or(99, |card| type_order(&card.card_type)), card.map_or_else(|| id.0.clone(), |card| card.title.clone()))
    });
    rows
}

/// Ice of every subtype is one group: a builder filters by "ICE", not by
/// "Barrier".
fn type_group(card_type: &CardType) -> &'static str {
    match card_type {
        CardType::Agenda => "Agenda",
        CardType::Asset => "Asset",
        CardType::Upgrade => "Upgrade",
        CardType::Operation => "Operation",
        CardType::Ice(_) => "ICE",
        CardType::Event => "Event",
        CardType::Hardware => "Hardware",
        CardType::Resource => "Resource",
        CardType::Program => "Program",
        CardType::Identity => "Identity",
    }
}

fn type_order(card_type: &CardType) -> u8 {
    match card_type {
        CardType::Identity => 0,
        CardType::Agenda | CardType::Event => 1,
        CardType::Asset | CardType::Hardware => 2,
        CardType::Upgrade | CardType::Resource => 3,
        CardType::Operation => 4,
        CardType::Ice(_) | CardType::Program => 5,
    }
}

fn faction_label(faction: Faction) -> &'static str {
    match faction {
        Faction::Anarch => "Anarch",
        Faction::Criminal => "Criminal",
        Faction::Shaper => "Shaper",
        Faction::HaasBioroid => "Haas-Bioroid",
        Faction::Jinteki => "Jinteki",
        Faction::Nbn => "NBN",
        Faction::WeylandConsortium => "Weyland",
        Faction::NeutralCorp | Faction::NeutralRunner => "Neutral",
    }
}

fn faction_order(faction: Option<Faction>) -> u8 {
    match faction {
        Some(Faction::HaasBioroid) | Some(Faction::Anarch) => 0,
        Some(Faction::Jinteki) | Some(Faction::Criminal) => 1,
        Some(Faction::Nbn) | Some(Faction::Shaper) => 2,
        Some(Faction::WeylandConsortium) => 3,
        Some(Faction::NeutralCorp) | Some(Faction::NeutralRunner) => 4,
        None => 5,
    }
}

fn format_label(format: NsgFormat) -> String {
    format!("{format:?}").to_lowercase()
}

/// A file-safe id from a display name — lowercase, words joined by `_` —
/// made unique against `taken` with a numeric suffix. The id is the
/// filename and what `--corp-deck` takes, so it has to be typeable; the
/// name is free text.
fn unique_id(name: &str, taken: &[String]) -> String {
    let mut slug = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let slug = slug.trim_end_matches('_');
    let base = if slug.is_empty() { "deck".to_string() } else { slug.to_string() };
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}_{n}");
        n += 1;
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn ids_are_typeable_and_unique() {
        assert_eq!(unique_id("Brick Stack 2!", &[]), "brick_stack_2");
        assert_eq!(unique_id("  Rush — HB  ", &[]), "rush_hb");
        assert_eq!(unique_id("!!!", &[]), "deck");
        assert_eq!(unique_id("Brick Stack", &["brick_stack".to_string(), "brick_stack_2".to_string()]), "brick_stack_3");
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
        let deck = &editor(&screen).deck;
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
        let deck = &editor(&screen).deck;
        assert_eq!(deck.size(), 3, "the fourth copy is refused");
        assert!(editor(&screen).note.as_deref().is_some_and(|note| note.contains("limit of 3")));
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
        let index = screen.rows.iter().position(|row| row.stored.deck.id == "brick_stack").unwrap();
        screen.cursor = index;
        screen.key(KeyCode::Enter);
        assert!(matches!(screen.mode, Mode::View { .. }), "built-in decks are read, not edited");
        screen.key(KeyCode::Char('c'));
        screen.key(KeyCode::Enter);
        let deck = editor(&screen).deck.clone();
        assert_eq!(deck.id, "brick_stack_copy");
        assert_eq!(deck.category, DeckCategory::Custom);
        assert_eq!(deck.size(), 44);
        assert_eq!(deck.style.as_deref(), Some("glacier"), "the copy keeps the list's style");
        deck.validate(&screen.registry, NsgFormat::Startup).expect("a copy of a legal deck is legal");
        screen.key(KeyCode::Esc);
        assert!(matches!(screen.mode, Mode::List));
        assert!(screen.rows[screen.cursor].saved() && screen.rows[screen.cursor].stored.deck.id == "brick_stack_copy");
    }

    #[test]
    fn a_saved_deck_deletes_after_confirmation_and_a_built_in_one_never_does() {
        let dir = Scratch::new("delete");
        let mut screen = screen(&dir, NsgFormat::Startup);
        let builtin = screen.rows.len();
        screen.key(KeyCode::Char('d'));
        assert!(screen.notice.as_deref().is_some_and(|notice| notice.contains("cannot be deleted")));
        press(&mut screen, &[KeyCode::Char('c'), KeyCode::Enter, KeyCode::Esc]);
        assert_eq!(screen.rows.len(), builtin + 1);
        assert!(screen.rows[0].saved(), "saved decks list first");
        screen.cursor = 0;
        press(&mut screen, &[KeyCode::Char('d'), KeyCode::Char('n')]);
        assert_eq!(screen.rows.len(), builtin + 1, "anything but y keeps it");
        press(&mut screen, &[KeyCode::Char('d'), KeyCode::Char('y')]);
        assert_eq!(screen.rows.len(), builtin);
        assert!(deck_store::read_dir(&dir.0).unwrap().is_empty());
    }

    #[test]
    fn the_pool_is_the_decks_side_in_the_format_and_filters_narrow_it() {
        let dir = Scratch::new("pool");
        let registry = netrunner_client::decks::sample_deck_registry();
        let deck = netrunner_core::decks::by_id("brick_stack").unwrap();
        let startup = Editor::new(deck.clone(), &registry, NsgFormat::Startup);
        let eternal = Editor::new(deck.clone(), &registry, NsgFormat::Eternal);
        assert!(startup.pool.len() < eternal.pool.len(), "Eternal adds the Core Set");
        for id in &startup.pool {
            let card = registry.get(id).unwrap();
            assert_eq!(card.side, Side::Corp);
            assert!(matches!(card.set_code.as_deref(), Some("sg" | "elev")), "{} is {:?}", card.title, card.set_code);
        }
        let mut editor = startup;
        let all = editor.visible_pool(&registry).len();
        editor.key(KeyCode::Char('t'), &registry);
        assert_eq!(editor.type_filter, Some("Agenda"));
        assert!(editor.visible_pool(&registry).iter().all(|id| registry.get(id).unwrap().card_type == CardType::Agenda));
        editor.key(KeyCode::Char('x'), &registry);
        editor.key(KeyCode::Char('/'), &registry);
        for c in "hedge".chars() {
            editor.key(KeyCode::Char(c), &registry);
        }
        let found: Vec<String> = editor.visible_pool(&registry).iter().map(|id| registry.get(id).unwrap().title.clone()).collect();
        assert_eq!(found, vec!["Hedge Fund".to_string()]);
        editor.key(KeyCode::Esc, &registry);
        assert_eq!(editor.visible_pool(&registry).len(), all, "Esc clears the search");
        drop(dir);
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
        assert_eq!(saved.style.as_deref(), Some("rush"), "the first Corp style after balanced");
        Personality::for_deck(&saved).expect("a style the bots can read");
    }
}
