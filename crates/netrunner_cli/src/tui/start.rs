//! The new-game form: chair, rung, style and decks chosen in the TUI, for
//! a player who never touches a flag. The main menu's Play vs Computer
//! opens it, and a game it starts returns to it.
//!
//! **It is the flag path, not a second one.** Every choice here is folded
//! back into the `Config` the flags would have produced
//! (`StartChoice::apply`), and `run_local` then runs exactly as it does
//! for `--runner-level 3 --corp-deck brick_stack`. There is one seating
//! rule, one rating rule and one deck resolver, and the screen is a way
//! of filling in their inputs — which is also why the menu opens only
//! when no side flag was given: an invocation that names a side has
//! already made these choices.
//!
//! **The state machine is a plain struct and the keys are a function**,
//! the way `replay_key` is, so the whole thing is tested without a
//! terminal; `draw` is the thin layer over it, and the menu's loop owns
//! the terminal.

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use netrunner_bots::{Level, Personality};
use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::Side;

use crate::config::{BotKind, Config};
use crate::deck_store::{self, Origin};
use crate::ratings::{self, LocalRatings};

/// One deck the screen can offer, with what a player needs to tell decks
/// apart: the name, the style a bot would play it in, the identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckRow {
    pub id: String,
    pub name: String,
    pub style: Option<String>,
    pub identity: String,
    pub saved: bool,
}

impl DeckRow {
    fn label(&self) -> String {
        let style = self.style.as_deref().unwrap_or("balanced");
        let saved = if self.saved { " (saved)" } else { "" };
        format!("{} · {style} · {}{saved}", self.name, self.identity)
    }
}

/// The panes, in Tab order. Chair first because it decides what the
/// other four offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Chair,
    Level,
    Style,
    OpponentDeck,
    OwnDeck,
}

impl Pane {
    const ALL: [Pane; 5] = [Pane::Chair, Pane::Level, Pane::Style, Pane::OpponentDeck, Pane::OwnDeck];

    fn next(self) -> Pane {
        let index = Pane::ALL.iter().position(|pane| *pane == self).expect("a pane");
        Pane::ALL[(index + 1) % Pane::ALL.len()]
    }

    fn previous(self) -> Pane {
        let index = Pane::ALL.iter().position(|pane| *pane == self).expect("a pane");
        Pane::ALL[(index + Pane::ALL.len() - 1) % Pane::ALL.len()]
    }
}

/// What the player chose, in the vocabulary of the flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartChoice {
    pub human: Side,
    pub level: Level,
    /// `None` is the deck's own style, the same as an unset
    /// `--corp-personality`.
    pub style: Option<Personality>,
    pub corp_deck: String,
    pub runner_deck: String,
    /// `false` is `--unrated`.
    pub rated: bool,
}

impl StartChoice {
    /// Folds the choice into `config` so `run_local` sees the flag form:
    /// the human side `Human`, the bot side a rung (its kind is a
    /// placeholder the rung overrides), the style as the personality flag,
    /// the decks by id.
    pub fn apply(&self, config: &mut Config) {
        let bot = self.human.other();
        config.corp = if self.human == Side::Corp { BotKind::Human } else { BotKind::Heuristic };
        config.runner = if self.human == Side::Runner { BotKind::Human } else { BotKind::Heuristic };
        config.corp_level = (bot == Side::Corp).then_some(self.level);
        config.runner_level = (bot == Side::Runner).then_some(self.level);
        config.corp_personality = if bot == Side::Corp { self.style } else { None };
        config.runner_personality = if bot == Side::Runner { self.style } else { None };
        config.corp_deck = self.corp_deck.clone();
        config.runner_deck = self.runner_deck.clone();
        config.unrated = !self.rated;
    }
}

/// What one key did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartKey {
    Continue,
    Start(StartChoice),
    /// Back to the main menu.
    Back,
}

/// The screen's state: a cursor per pane and the lists they move over.
#[derive(Debug, Clone)]
pub struct StartMenu {
    pub pane: Pane,
    chair: usize,
    level: usize,
    style: usize,
    opponent_deck: usize,
    own_deck: usize,
    /// Corp decks, then Runner decks.
    decks: [Vec<DeckRow>; 2],
    /// The rung `ratings::LocalRatings::suggest` points the player at, as
    /// Corp and as Runner. The level cursor starts here and returns here
    /// when the chair changes.
    suggested: [Level; 2],
    /// The default deck ids from the flags, so the deck cursors start on
    /// what a flag-less run would have played.
    defaults: [String; 2],
    /// Whether the game counts — `r` toggles it. Starts from `--unrated`.
    pub rated: bool,
}

const SIDES: [Side; 2] = [Side::Corp, Side::Runner];

fn side_index(side: Side) -> usize {
    match side {
        Side::Corp => 0,
        Side::Runner => 1,
    }
}

impl StartMenu {
    /// Reads the saved-deck directory and the ratings file the flags name.
    /// A ratings file that cannot be read suggests the middle rung rather
    /// than blocking the screen; the game itself will refuse it later.
    pub fn open(config: &Config, registry: &CardRegistry) -> Result<Self, String> {
        let dir = deck_store::resolve_decks_dir(config.decks_dir.as_deref())?;
        let mut decks: [Vec<DeckRow>; 2] = [Vec::new(), Vec::new()];
        for stored in deck_store::list(&dir)? {
            let identity =
                registry.get(&stored.deck.identity).map_or_else(|| stored.deck.identity.0.clone(), |card| card.title.clone());
            decks[side_index(stored.deck.side)].push(DeckRow {
                id: stored.deck.id.clone(),
                name: stored.deck.name.clone(),
                style: stored.deck.style.clone(),
                identity,
                saved: !matches!(stored.origin, Origin::Embedded),
            });
        }
        let player = ratings::player_name(config);
        let suggested = match ratings::resolve_ratings_file(config.ratings_file.as_deref()).and_then(|path| LocalRatings::load(&path)) {
            Ok(book) => SIDES.map(|side| book.suggest(&player, side)),
            Err(_) => [Level::Operator, Level::Operator],
        };
        let mut menu = Self::with_decks(decks, suggested, [config.corp_deck.clone(), config.runner_deck.clone()]);
        menu.rated = !config.unrated;
        Ok(menu)
    }

    /// Puts the cursors back on a game just played — same chair, decks,
    /// style and rated setting — except the rung, which goes to the
    /// suggestion re-read after that game. So after a game Enter is "play
    /// again", and it is at the rung the game-over modal just named.
    pub fn resume_from(&mut self, last: &StartChoice) {
        self.chair = side_index(last.human);
        self.defaults = [last.corp_deck.clone(), last.runner_deck.clone()];
        self.reset_for_chair();
        self.style = self.styles().iter().position(|style| *style == last.style).unwrap_or(0);
        self.rated = last.rated;
    }

    /// The state without the filesystem, for tests and for `open`.
    pub fn with_decks(decks: [Vec<DeckRow>; 2], suggested: [Level; 2], defaults: [String; 2]) -> Self {
        let mut menu =
            Self { pane: Pane::Chair, chair: 0, level: 0, style: 0, opponent_deck: 0, own_deck: 0, decks, suggested, defaults, rated: true };
        menu.reset_for_chair();
        menu
    }

    pub fn human(&self) -> Side {
        SIDES[self.chair]
    }

    fn bot(&self) -> Side {
        self.human().other()
    }

    pub fn level(&self) -> Level {
        Level::ALL[self.level]
    }

    /// The styles offered for the bot's chair: the deck's own first, then
    /// every profile written for that chair, then balanced.
    fn styles(&self) -> Vec<Option<Personality>> {
        let mut styles = vec![None];
        styles.extend(Personality::ALL.iter().copied().filter(|p| p.side() == Some(self.bot())).map(Some));
        styles.push(Some(Personality::Balanced));
        styles
    }

    pub fn style(&self) -> Option<Personality> {
        self.styles()[self.style]
    }

    fn opponent_decks(&self) -> &[DeckRow] {
        &self.decks[side_index(self.bot())]
    }

    fn own_decks(&self) -> &[DeckRow] {
        &self.decks[side_index(self.human())]
    }

    /// Everything but the chair follows from the chair: the level cursor
    /// goes to that chair's suggestion, the style list is the bot's
    /// chair's, and the deck cursors go to the flag defaults.
    fn reset_for_chair(&mut self) {
        self.level = Level::ALL.iter().position(|l| *l == self.suggested[self.chair]).unwrap_or(2);
        self.style = 0;
        let default_of = |decks: &[DeckRow], id: &str| decks.iter().position(|d| d.id == id).unwrap_or(0);
        self.opponent_deck = default_of(self.opponent_decks(), &self.defaults[side_index(self.bot())]);
        self.own_deck = default_of(self.own_decks(), &self.defaults[side_index(self.human())]);
    }

    fn move_cursor(&mut self, delta: i32) {
        let (cursor, len) = match self.pane {
            Pane::Chair => (&mut self.chair, SIDES.len()),
            Pane::Level => (&mut self.level, Level::ALL.len()),
            Pane::Style => {
                let len = self.styles().len();
                (&mut self.style, len)
            }
            Pane::OpponentDeck => {
                let len = self.opponent_decks().len();
                (&mut self.opponent_deck, len)
            }
            Pane::OwnDeck => {
                let len = self.own_decks().len();
                (&mut self.own_deck, len)
            }
        };
        if len == 0 {
            return;
        }
        *cursor = (*cursor as i32 + delta).rem_euclid(len as i32) as usize;
        if self.pane == Pane::Chair {
            self.reset_for_chair();
        }
    }

    /// The choice as it stands, or `None` while a deck list is empty.
    pub fn choice(&self) -> Option<StartChoice> {
        let opponent = self.opponent_decks().get(self.opponent_deck)?;
        let own = self.own_decks().get(self.own_deck)?;
        let (corp_deck, runner_deck) = match self.human() {
            Side::Corp => (own.id.clone(), opponent.id.clone()),
            Side::Runner => (opponent.id.clone(), own.id.clone()),
        };
        Some(StartChoice { human: self.human(), level: self.level(), style: self.style(), corp_deck, runner_deck, rated: self.rated })
    }

    /// One keypress. Tab and the arrows move between panes, Up/Down
    /// within one, `r` toggles rated, Enter starts from any pane, Esc or
    /// `q` goes back to the menu.
    pub fn key(&mut self, key: KeyCode) -> StartKey {
        match key {
            KeyCode::Esc | KeyCode::Char('q') => StartKey::Back,
            KeyCode::Char('r') => {
                self.rated = !self.rated;
                StartKey::Continue
            }
            KeyCode::Enter => self.choice().map_or(StartKey::Continue, StartKey::Start),
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                self.pane = self.pane.next();
                StartKey::Continue
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                self.pane = self.pane.previous();
                StartKey::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_cursor(-1);
                StartKey::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_cursor(1);
                StartKey::Continue
            }
            _ => StartKey::Continue,
        }
    }

    /// The five lists as `(title, rows, cursor)`, for `draw` and for tests
    /// that check what a player would see.
    pub fn panes(&self) -> Vec<(Pane, String, Vec<String>, usize)> {
        let bot = self.bot();
        let chair_rows = SIDES.iter().map(|side| format!("{side:?}")).collect();
        let level_rows = Level::ALL
            .iter()
            .map(|level| {
                let spec = level.spec(bot);
                let mark = if *level == self.suggested[self.chair] { "  ◆ suggested" } else { "" };
                format!("{}. {} — {}{mark}", level.rung(), level.name(), spec.describe())
            })
            .collect();
        let style_rows = self
            .styles()
            .into_iter()
            .map(|style| match style {
                None => "the deck's own style".to_string(),
                Some(personality) => personality.name().to_string(),
            })
            .collect();
        let deck_rows = |decks: &[DeckRow]| decks.iter().map(DeckRow::label).collect::<Vec<_>>();
        vec![
            (Pane::Chair, "Your chair".to_string(), chair_rows, self.chair),
            (Pane::Level, format!("Opponent ({bot:?}) level"), level_rows, self.level),
            (Pane::Style, "Opponent style".to_string(), style_rows, self.style),
            (Pane::OpponentDeck, format!("Opponent's deck ({bot:?})"), deck_rows(self.opponent_decks()), self.opponent_deck),
            (Pane::OwnDeck, format!("Your deck ({:?})", self.human()), deck_rows(self.own_decks()), self.own_deck),
        ]
    }
}

pub fn draw(frame: &mut Frame, area: Rect, menu: &StartMenu) {
    let [header, body] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .areas(area);
    let rated = if menu.rated {
        "Rated — the result goes on your ladder (r: unrated)"
    } else {
        "Unrated — the result is not recorded (r: rated)"
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from("Play vs Computer — Tab/arrows move between panes, Up/Down choose, Enter plays, Esc goes back"),
            Line::from(rated),
        ]),
        header,
    );
    let [left, right] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .areas(body);
    let [chair, level, style] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Length(7), Constraint::Min(4)])
        .areas(left);
    let [opponent, own] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .areas(right);
    let areas = [chair, level, style, opponent, own];
    for ((pane, title, rows, cursor), area) in menu.panes().into_iter().zip(areas) {
        draw_pane(frame, area, pane == menu.pane, &title, rows, cursor);
    }
}

fn draw_pane(frame: &mut Frame, area: Rect, active: bool, title: &str, rows: Vec<String>, cursor: usize) {
    let items: Vec<ListItem> = rows.into_iter().map(ListItem::new).collect();
    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(cursor));
    }
    let border = if active { Style::default().fg(Color::Yellow) } else { Style::default() };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title.to_string()).border_style(border))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn row(id: &str, side: Side) -> DeckRow {
        DeckRow {
            id: id.to_string(),
            name: id.replace('_', " "),
            style: Some(if side == Side::Corp { "rush" } else { "aggressive" }.to_string()),
            identity: "Someone".to_string(),
            saved: false,
        }
    }

    fn menu() -> StartMenu {
        StartMenu::with_decks(
            [vec![row("brick_stack", Side::Corp), row("discretion_advised", Side::Corp)], vec![row("stolen_goods", Side::Runner), row("dashing_mad", Side::Runner)]],
            [Level::Apprentice, Level::Veteran],
            ["discretion_advised".to_string(), "stolen_goods".to_string()],
        )
    }

    #[test]
    fn the_cursors_start_on_the_suggested_rung_and_the_default_decks() {
        let menu = menu();
        assert_eq!(menu.human(), Side::Corp);
        assert_eq!(menu.level(), Level::Apprentice, "the Corp chair's suggestion");
        let choice = menu.choice().unwrap();
        assert_eq!((choice.corp_deck.as_str(), choice.runner_deck.as_str()), ("discretion_advised", "stolen_goods"));
        assert_eq!(choice.style, None, "the deck's own style by default");
    }

    #[test]
    fn changing_chair_swaps_the_lists_and_re_reads_the_suggestion() {
        let mut menu = menu();
        assert_eq!(menu.key(KeyCode::Down), StartKey::Continue);
        assert_eq!(menu.human(), Side::Runner);
        assert_eq!(menu.level(), Level::Veteran, "the Runner chair's suggestion");
        let panes = menu.panes();
        assert!(panes[1].1.contains("Corp"), "the opponent is now the Corp: {}", panes[1].1);
        assert!(panes[2].2.contains(&"rush".to_string()) && !panes[2].2.contains(&"aggressive".to_string()), "{:?}", panes[2].2);
        let choice = menu.choice().unwrap();
        assert_eq!(choice.human, Side::Runner);
        assert_eq!((choice.corp_deck.as_str(), choice.runner_deck.as_str()), ("discretion_advised", "stolen_goods"));
    }

    #[test]
    fn enter_starts_from_any_pane_and_escape_goes_back() {
        let mut menu = menu();
        menu.key(KeyCode::Tab);
        assert_eq!(menu.pane, Pane::Level);
        menu.key(KeyCode::Down);
        assert_eq!(menu.level(), Level::Operator);
        menu.key(KeyCode::Tab);
        menu.key(KeyCode::Down);
        assert_eq!(menu.style(), Some(Personality::Aggressive), "the first profile written for the Runner");
        menu.key(KeyCode::Tab);
        menu.key(KeyCode::Down);
        match menu.key(KeyCode::Enter) {
            StartKey::Start(choice) => {
                assert_eq!(choice.level, Level::Operator);
                assert_eq!(choice.style, Some(Personality::Aggressive));
                assert_eq!(choice.runner_deck, "dashing_mad");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(menu.key(KeyCode::Esc), StartKey::Back);
        assert_eq!(menu.key(KeyCode::Char('q')), StartKey::Back);
    }

    #[test]
    fn r_toggles_rated_and_the_choice_carries_it() {
        let mut menu = menu();
        assert!(menu.choice().unwrap().rated, "rated unless asked otherwise");
        menu.key(KeyCode::Char('r'));
        assert!(!menu.choice().unwrap().rated);
    }

    #[test]
    fn resuming_keeps_the_game_just_played_but_takes_the_new_suggestion() {
        let mut menu = menu();
        let last = StartChoice {
            human: Side::Runner,
            level: Level::Novice,
            style: Some(Personality::Glacier),
            corp_deck: "brick_stack".to_string(),
            runner_deck: "dashing_mad".to_string(),
            rated: false,
        };
        menu.resume_from(&last);
        let choice = menu.choice().unwrap();
        assert_eq!(choice.level, Level::Veteran, "the suggestion, not the rung last played");
        assert_eq!(choice, StartChoice { level: Level::Veteran, ..last });
    }

    #[test]
    fn the_panes_wrap_in_both_directions() {
        let mut menu = menu();
        menu.key(KeyCode::BackTab);
        assert_eq!(menu.pane, Pane::OwnDeck);
        menu.key(KeyCode::Tab);
        assert_eq!(menu.pane, Pane::Chair);
        menu.key(KeyCode::Up);
        assert_eq!(menu.human(), Side::Runner, "the chair list wraps too");
    }

    #[test]
    fn the_suggested_rung_is_marked() {
        let panes = menu().panes();
        let marked: Vec<&String> = panes[1].2.iter().filter(|row| row.contains("suggested")).collect();
        assert_eq!(marked.len(), 1);
        assert!(marked[0].starts_with("2. apprentice"), "{}", marked[0]);
    }

    #[test]
    fn an_empty_deck_list_cannot_start() {
        let mut empty = StartMenu::with_decks([Vec::new(), Vec::new()], [Level::Operator; 2], [String::new(), String::new()]);
        assert_eq!(empty.key(KeyCode::Enter), StartKey::Continue);
        assert_eq!(empty.choice(), None);
    }

    #[test]
    fn the_choice_folds_into_the_flag_form() {
        let choice = StartChoice {
            human: Side::Runner,
            level: Level::Veteran,
            style: Some(Personality::Glacier),
            corp_deck: "brick_stack".to_string(),
            runner_deck: "dashing_mad".to_string(),
            rated: false,
        };
        let mut config = Config::try_parse_from(["netrunner_cli"]).unwrap();
        choice.apply(&mut config);
        assert!(config.unrated, "an unrated choice is --unrated");
        assert_eq!((config.corp, config.runner), (BotKind::Heuristic, BotKind::Human));
        assert_eq!((config.corp_level, config.runner_level), (Some(Level::Veteran), None));
        assert_eq!((config.corp_personality, config.runner_personality), (Some(Personality::Glacier), None));
        assert_eq!((config.corp_deck.as_str(), config.runner_deck.as_str()), ("brick_stack", "dashing_mad"));
        // The same request as the flags, so the two paths cannot diverge.
        let flags = Config::try_parse_from([
            "netrunner_cli", "--runner", "human", "--corp", "heuristic", "--corp-level", "veteran", "--corp-personality", "glacier",
            "--corp-deck", "brick_stack", "--runner-deck", "dashing_mad",
        ])
        .unwrap();
        assert_eq!((flags.corp, flags.runner, flags.corp_level, flags.corp_personality), (config.corp, config.runner, config.corp_level, config.corp_personality));
    }
}
