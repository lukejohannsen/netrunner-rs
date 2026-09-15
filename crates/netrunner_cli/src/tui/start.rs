//! The new-game form in the terminal: the keys, the drawing, and the fold
//! into `Config`. The state machine itself is
//! `netrunner_client::start::StartMenu`, shared with the desktop (Phase 7
//! §3); this file is what a terminal adds to it.
//!
//! **It is the flag path, not a second one.** Every choice here is folded
//! back into the `Config` the flags would have produced
//! (`apply_choice`), and `run_local` then runs exactly as it does for
//! `--runner-level 3 --corp-deck brick_stack`. There is one seating
//! rule, one rating rule and one deck resolver, and the screen is a way
//! of filling in their inputs — which is also why the menu opens only
//! when no side flag was given: an invocation that names a side has
//! already made these choices.

use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::Side;

pub use netrunner_client::start::{Intent, StartChoice, StartMenu};

use crate::config::{BotKind, Config};
use crate::ratings;
use netrunner_client::deck_store;

/// `StartMenu::open` from the flags: the saved-deck directory and the
/// ratings file they name, the player they name, the decks they default
/// to, and `--unrated`.
pub fn open(config: &Config, registry: &CardRegistry) -> Result<StartMenu, String> {
    let dir = deck_store::resolve_decks_dir(config.decks_dir.as_deref())?;
    let ratings_path = ratings::resolve_ratings_file(config.ratings_file.as_deref()).ok();
    StartMenu::open(
        &dir,
        ratings_path.as_deref(),
        &ratings::player_name(config),
        registry,
        [config.corp_deck.clone(), config.runner_deck.clone()],
        !config.unrated,
    )
}

/// Folds the choice into `config` so `run_local` sees the flag form: the
/// human side `Human`, the bot side a rung (its kind is a placeholder the
/// rung overrides), the style as the personality flag, the decks by id.
pub fn apply_choice(choice: &StartChoice, config: &mut Config) {
    let bot = choice.human.other();
    config.corp = if choice.human == Side::Corp { BotKind::Human } else { BotKind::Heuristic };
    config.runner = if choice.human == Side::Runner { BotKind::Human } else { BotKind::Heuristic };
    config.corp_level = (bot == Side::Corp).then_some(choice.level);
    config.runner_level = (bot == Side::Runner).then_some(choice.level);
    config.corp_personality = if bot == Side::Corp { choice.style } else { None };
    config.runner_personality = if bot == Side::Runner { choice.style } else { None };
    config.corp_deck = choice.corp_deck.clone();
    config.runner_deck = choice.runner_deck.clone();
    config.unrated = !choice.rated;
}

/// What one key did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartKey {
    Continue,
    Start(StartChoice),
    /// Back to the main menu.
    Back,
}

/// One keypress. Tab and the arrows move between panes, Up/Down within
/// one, `r` toggles rated, Enter starts from any pane, Esc or `q` goes
/// back to the menu.
pub fn key(menu: &mut StartMenu, key: KeyCode) -> StartKey {
    let intent = match key {
        KeyCode::Esc | KeyCode::Char('q') => return StartKey::Back,
        KeyCode::Enter => return menu.choice().map_or(StartKey::Continue, StartKey::Start),
        KeyCode::Char('r') => Intent::ToggleRated,
        KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => Intent::NextPane,
        KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => Intent::PrevPane,
        KeyCode::Up | KeyCode::Char('k') => Intent::Move(-1),
        KeyCode::Down | KeyCode::Char('j') => Intent::Move(1),
        _ => return StartKey::Continue,
    };
    menu.apply(intent);
    StartKey::Continue
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
    use netrunner_bots::{Level, Personality};
    use netrunner_client::start::{DeckRow, Pane};

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

    /// The bindings over the shared state machine: Tab and the arrows
    /// move, Enter starts from any pane, Esc and `q` go back, `r` toggles.
    #[test]
    fn enter_starts_from_any_pane_and_escape_goes_back() {
        let mut menu = menu();
        key(&mut menu, KeyCode::Tab);
        assert_eq!(menu.pane, Pane::Level);
        key(&mut menu, KeyCode::Down);
        assert_eq!(menu.level(), Level::Operator);
        key(&mut menu, KeyCode::Tab);
        key(&mut menu, KeyCode::Down);
        assert_eq!(menu.style(), Some(Personality::Aggressive), "the first profile written for the Runner");
        key(&mut menu, KeyCode::Tab);
        key(&mut menu, KeyCode::Down);
        match key(&mut menu, KeyCode::Enter) {
            StartKey::Start(choice) => {
                assert_eq!(choice.level, Level::Operator);
                assert_eq!(choice.style, Some(Personality::Aggressive));
                assert_eq!(choice.runner_deck, "dashing_mad");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(key(&mut menu, KeyCode::Esc), StartKey::Back);
        assert_eq!(key(&mut menu, KeyCode::Char('q')), StartKey::Back);
        key(&mut menu, KeyCode::BackTab);
        assert_eq!(menu.pane, Pane::Style, "back one pane from the opponent's deck");
        assert!(menu.rated);
        key(&mut menu, KeyCode::Char('r'));
        assert!(!menu.choice().unwrap().rated);
    }

    #[test]
    fn an_empty_deck_list_cannot_start() {
        let mut empty = StartMenu::with_decks([Vec::new(), Vec::new()], [Level::Operator; 2], [String::new(), String::new()]);
        assert_eq!(key(&mut empty, KeyCode::Enter), StartKey::Continue);
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
        apply_choice(&choice, &mut config);
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
