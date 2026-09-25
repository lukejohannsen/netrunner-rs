//! The new-game form: chair, rung, style and the two decks, over
//! `netrunner_client::start::StartMenu` — the state machine the
//! terminal's form runs, so the two clients offer the same lists, suggest
//! the same rung and default to the same decks.
//!
//! Each choice is drawn the way it is best chosen, rather than five
//! drop-downs in a row (the first cut, which read as a settings file):
//! the side is two large cards in the sides' own colours, the rung and
//! the style are rows of pills with the chosen one filled, and the decks —
//! lists too long for pills — are drop-downs, each with the deck's
//! identity and style under it.
//!
//! Start puts the match together (`MatchHandle::start_local`, which
//! fails here rather than on the board: a deck that will not validate or
//! a record file that will not load is a notice under the form) and
//! leaves it in [`ActiveMatch`] for the game screen. When a game ends or
//! is left, the form reopens on the game just played with the rung moved
//! to the new suggestion — after a game, Start is "play again".

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;

use netrunner_client::decks::decks_for_match;
use netrunner_client::play::{LocalMatchSpec, MatchHandle, RecordFile};
use netrunner_core::tutorial::Lesson;
use netrunner_client::start::{DeckRow, Level, Pane, StartChoice, StartMenu, DEFAULT_CORP_DECK, DEFAULT_RUNNER_DECK};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

use crate::core::ClientCore;
use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::skin::{Drawn, Slot};
use crate::theme::{size, Theme};
use crate::widgets::dropdown::{spawn_dropdown, Choice, DropdownChanged};
use crate::widgets::{self, ButtonKind, Pressed};

pub struct NewGamePlugin;

impl Plugin for NewGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::NewGame), spawn).add_systems(Update, (controls, refresh).chain().run_if(in_state(AppScreen::NewGame)));
    }
}

/// The match a Start put together, for the game screen: the handle, and
/// the choice the form reopens on afterwards (`None` for a dev game and a
/// lesson).
#[derive(Resource)]
pub struct ActiveMatch {
    pub handle: MatchHandle,
    pub choice: Option<StartChoice>,
    /// The lesson being played, when this is one (`screens::learn`): the
    /// board reads its words, and leaving goes back to Learn to Play.
    pub lesson: Option<Lesson>,
    /// The starter game being played, when this is one: Play again deals
    /// it again, and leaving goes back to Learn to Play.
    pub starter: Option<Starter>,
}

/// A starter game, as Learn to Play names it: the person's chair, and
/// whether it is the booster-staged pair at 7 points rather than the
/// starter lists at 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Starter {
    pub side: Side,
    pub boosted: bool,
}

/// The last game played, left by the game screen; the form resumes from
/// it.
#[derive(Resource, Debug, Clone)]
pub struct LastGame(pub StartChoice);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Start,
    Back,
}

/// Which pane a drop-down is; on its root. The two deck lists.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneDropdown(pub Pane);

/// One choice drawn as a button of its own — a side card, a rung or a
/// style pill: its pane and its row.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneChoice {
    pub pane: Pane,
    pub index: usize,
}

/// The form's sections, respawned on every change: a choice restyles
/// its row and the chair changes every other list.
#[derive(Component)]
struct Form;
#[derive(Component)]
struct NoticeLine;

#[derive(Resource)]
struct Model(StartMenu);

#[derive(Resource, Default)]
struct Dirty {
    form: bool,
    notice: Option<String>,
}

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, last: Option<Res<LastGame>>) {
    commands.init_resource::<Dirty>();
    let (mut menu, notice) = match open_menu(&core) {
        Ok(menu) => (menu, String::new()),
        Err(error) => (StartMenu::with_decks([Vec::new(), Vec::new()], [Level::Operator; 2], defaults()), format!("Decks could not be listed: {error}")),
    };
    if let Some(last) = last {
        menu.resume_from(&last.0);
    }
    let form = commands.spawn((Form, Node { flex_direction: FlexDirection::Column, row_gap: px(22), ..default() })).id();
    commands.entity(form).with_children(|parent| spawn_form(parent, &theme, &menu));
    let buttons = commands
        .spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, justify_content: JustifyContent::FlexEnd, column_gap: px(12), margin: UiRect::top(px(8)), ..default() })
        .with_children(|parent| {
            parent.spawn(widgets::styled_button(&theme, ButtonKind::Quiet, "Back", Val::Auto, Control::Back));
            parent.spawn(widgets::styled_button(&theme, ButtonKind::Primary, "Start", px(200), Control::Start));
        })
        .id();
    let mut panel = commands.spawn(widgets::roomy_panel(&theme, px(1000)));
    let panel = panel.add_child(form).add_child(buttons).id();
    let column = commands
        .spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(16), margin: UiRect::vertical(Val::Auto), max_width: percent(100), ..default() })
        .with_children(|parent| {
            parent.spawn(widgets::title(&theme, AppScreen::NewGame.title()));
            parent.spawn(widgets::dim(&theme, "A casual game against a rung of the ladder, in the style your deck chooses. A move can always be taken back."));
        })
        .add_child(panel)
        .with_children(|parent| {
            parent.spawn(widgets::notice(&theme, notice, NoticeLine));
        })
        .id();
    commands.spawn(screen_root(AppScreen::NewGame, &theme)).add_child(column);
    commands.insert_resource(Model(menu));
}

fn defaults() -> [String; 2] {
    [DEFAULT_CORP_DECK.to_string(), DEFAULT_RUNNER_DECK.to_string()]
}

/// The form from the client's files: the saved decks beside the built-in
/// ones, and the record's suggestion for each chair.
fn open_menu(core: &ClientCore) -> Result<StartMenu, String> {
    // No data directory means no saved decks; the built-in ones are still
    // listed, so any path that does not exist will do.
    let decks_dir = core.decks_dir.clone().unwrap_or_else(|| std::env::temp_dir().join("netrunner-no-decks"));
    let format = core.settings.format.unwrap_or(NsgFormat::Startup);
    StartMenu::open(&decks_dir, core.record_path.as_deref(), &core.player_name(), &core.registry, format, defaults())
}

/// What each side does, in a line, on its card.
fn side_blurb(side: Side) -> &'static str {
    match side {
        Side::Corp => "Build servers, protect your agendas and score seven points.",
        Side::Runner => "Break in, steal agendas and trash what the Corp builds.",
    }
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
}

/// A labelled section of the form: an overline over its content.
fn section(parent: &mut ChildSpawnerCommands, theme: &Theme, name: impl Into<String>, content: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(10), ..default() }).with_children(|section| {
        section.spawn(widgets::overline(theme, name));
        content(section);
    });
}

/// A row of pills, one per choice, the chosen one filled; it wraps
/// rather than overflowing.
fn pills(parent: &mut ChildSpawnerCommands, theme: &Theme, pane: Pane, labels: Vec<String>, cursor: usize) {
    parent.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(10), ..default() }).with_children(|row| {
        for (index, label) in labels.into_iter().enumerate() {
            let kind = if index == cursor { ButtonKind::Primary } else { ButtonKind::Secondary };
            row.spawn(widgets::styled_button(theme, kind, label, Val::Auto, PaneChoice { pane, index }));
        }
    });
}

/// The side's card: its name in its own colour over what it does,
/// ringed in the accent when chosen and dimmed when not.
fn side_card(parent: &mut ChildSpawnerCommands, theme: &Theme, side: Side, index: usize, chosen: bool) {
    let colour = theme.side(side);
    let (fill, rim, hover) = if chosen {
        (colour.with_alpha(0.28), theme.accent, colour.with_alpha(0.34))
    } else {
        (theme.secondary.with_alpha(0.06), theme.glass_border, colour.with_alpha(0.16))
    };
    let drawn = Drawn::new(fill, rim);
    parent
        .spawn((
            Button,
            widgets::Themed,
            PaneChoice { pane: Pane::Chair, index },
            Node {
                flex_grow: 1.0,
                flex_basis: px(0),
                min_height: px(118),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(18),
                padding: UiRect::horizontal(px(24)),
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(16)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(fill),
            BorderColor::all(rim),
            widgets::Dressed { slot: Slot::Button, drawn, hover: Some(Drawn::new(hover, theme.border_hover)), pressed: Some(Drawn::new(colour.with_alpha(0.40), theme.accent)) },
        ))
        .with_children(|card| {
            // The side's own colour as a short bar beside its name, inset
            // from the edge: a strip down the edge itself ran over the
            // rounded corners, because `Overflow::clip` clips to the
            // rectangle and not to the radius.
            card.spawn((
                Node { width: px(6), height: px(64), flex_shrink: 0.0, border_radius: BorderRadius::all(px(3)), ..default() },
                BackgroundColor(if chosen { colour } else { colour.with_alpha(0.55) }),
            ));
            card.spawn(Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, row_gap: px(6), padding: UiRect::vertical(px(16)), ..default() })
                .with_children(|words| {
                    let name_colour = if chosen { theme.text } else { theme.text_dim };
                    words.spawn((Text::new(format!("{side:?}")), theme.font(size::HEADING), TextColor(name_colour)));
                    words.spawn((Text::new(side_blurb(side)), theme.font(size::SMALL), TextColor(theme.text_dim)));
                });
        });
}

/// A deck list: a drop-down of its names, and the chosen deck's
/// identity and style under it.
fn deck_picker(parent: &mut ChildSpawnerCommands, theme: &Theme, pane: Pane, title: String, decks: &[DeckRow], cursor: usize) {
    parent.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0), row_gap: px(10), ..default() }).with_children(|column| {
        column.spawn(widgets::overline(theme, title));
        // A saved deck that cannot start a game is listed anyway, marked,
        // so a person finds the deck they built and reads why; Start
        // refuses it with that reason.
        let choices = decks
            .iter()
            .map(|deck| {
                let saved = if deck.saved { " (saved)" } else { "" };
                let problem = if deck.problem.is_some() { " — not playable" } else { "" };
                Choice::plain(format!("{}{saved}{problem}", deck.name))
            })
            .collect();
        spawn_dropdown(column, theme, "", choices, cursor, PaneDropdown(pane));
        if let Some(deck) = decks.get(cursor) {
            let style = deck.style.as_deref().unwrap_or("balanced");
            column.spawn((widgets::dim(theme, format!("{} · plays {style}", deck.identity)), Node { margin: UiRect::left(px(20)), ..default() }));
            if let Some(problem) = &deck.problem {
                column.spawn((
                    Text::new(format!("Can't start a game: {problem}")),
                    theme.font(size::SMALL),
                    TextColor(theme.danger),
                    Node { margin: UiRect::left(px(20)), ..default() },
                ));
            }
        }
    });
}

fn spawn_form(parent: &mut ChildSpawnerCommands, theme: &Theme, menu: &StartMenu) {
    let human = menu.human();
    let bot = menu.bot();
    section(parent, theme, "Your side", |section| {
        section.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(16), ..default() }).with_children(|row| {
            for (index, side) in [Side::Corp, Side::Runner].into_iter().enumerate() {
                side_card(row, theme, side, index, side == human);
            }
        });
    });
    let suggested = menu.suggested();
    let level = menu.level();
    section(parent, theme, format!("Opponent level · the {bot:?}"), |section| {
        let labels = Level::ALL
            .iter()
            .map(|l| {
                let mark = if *l == suggested { "  ·  suggested" } else { "" };
                format!("{}  {}{mark}", l.rung(), capitalised(l.name()))
            })
            .collect();
        pills(section, theme, Pane::Level, labels, menu.cursor(Pane::Level));
        section.spawn(widgets::dim(theme, format!("{}: {}.", capitalised(level.name()), level.spec(bot).describe())));
    });
    section(parent, theme, "Opponent style", |section| {
        let labels = menu
            .styles()
            .into_iter()
            .map(|style| match style {
                None => "Deck's own".to_string(),
                Some(personality) => capitalised(personality.name()),
            })
            .collect();
        pills(section, theme, Pane::Style, labels, menu.cursor(Pane::Style));
    });
    parent.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(24), ..default() }).with_children(|row| {
        deck_picker(row, theme, Pane::OwnDeck, format!("Your deck · {human:?}"), menu.own_decks(), menu.cursor(Pane::OwnDeck));
        deck_picker(row, theme, Pane::OpponentDeck, format!("Opponent's deck · {bot:?}"), menu.opponent_decks(), menu.cursor(Pane::OpponentDeck));
    });
}

/// A seed off the clock: the terminal uses `rand::random`, and the
/// desktop has no reason to carry a random crate for one number a game.
pub fn seed_from_clock() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64 | 1)
}

/// Puts the match together from a choice and the client's files. Every
/// failure is a `String` for the notice line.
pub fn start(core: &ClientCore, choice: &StartChoice) -> Result<ActiveMatch, String> {
    start_seeded(core, choice, seed_from_clock())
}

/// [`start`] on a given seed: the same deal and the same bot every time,
/// which is what a test needs — a board state the test depends on (an
/// unprotected central, a Corp install by turn one) otherwise holds on
/// some clock seeds and not others, and the test flakes.
pub fn start_seeded(core: &ClientCore, choice: &StartChoice, seed: u64) -> Result<ActiveMatch, String> {
    start_with(core, choice, seed, core.record_path.clone())
}

/// `record` is where the game is logged. Every game a person starts from
/// the form is — there is no unrecorded kind to ask for, since nothing
/// rides on a game against a bot — and `None` is the dev hook's, whose
/// autoplayed games are nobody's record.
fn start_with(core: &ClientCore, choice: &StartChoice, seed: u64, record: Option<std::path::PathBuf>) -> Result<ActiveMatch, String> {
    let decks_dir = core.decks_dir.clone().unwrap_or_else(|| std::env::temp_dir().join("netrunner-no-decks"));
    let format = core.settings.format.unwrap_or(NsgFormat::Startup);
    let (corp, runner) = decks_for_match(&decks_dir, &choice.corp_deck, &choice.runner_deck, &core.registry, format)?;
    let record = record.map(|path| RecordFile { path, player: core.player_name() });
    let spec = LocalMatchSpec {
        registry: Arc::clone(&core.registry),
        corp,
        runner,
        human: choice.human,
        level: choice.level,
        style: choice.style,
        seed,
        rules: Default::default(),
        record,
    };
    let handle = MatchHandle::start_local(spec)?;
    Ok(ActiveMatch { handle, choice: Some(choice.clone()), lesson: None, starter: None })
}

/// An unrecorded game on the default decks against the middle rung, the
/// person in `side`'s chair — a test's game.
pub fn start_default(core: &ClientCore, side: Side) -> Result<ActiveMatch, String> {
    start_dev(core, side, None, None)
}

/// `start_default` with either deck replaced — the dev hook's game, so a
/// screen a card reaches (Scatter Field's install) can be shot on a deck
/// that holds the card.
pub fn start_dev(core: &ClientCore, side: Side, corp_deck: Option<&str>, runner_deck: Option<&str>) -> Result<ActiveMatch, String> {
    let choice = StartChoice {
        human: side,
        level: Level::Operator,
        style: None,
        corp_deck: corp_deck.unwrap_or(DEFAULT_CORP_DECK).to_string(),
        runner_deck: runner_deck.unwrap_or(DEFAULT_RUNNER_DECK).to_string(),
    };
    start_with(core, &choice, seed_from_clock(), None).map(|active| ActiveMatch { choice: None, ..active })
}

fn controls(
    mut commands: Commands,
    mut pressed: MessageReader<Pressed>,
    mut chosen: MessageReader<DropdownChanged>,
    marks: Query<&Control>,
    panes: Query<&PaneDropdown>,
    choices: Query<&PaneChoice>,
    mut menu: ResMut<Model>,
    mut dirty: ResMut<Dirty>,
    core: Res<ClientCore>,
    mut navigate: MessageWriter<Navigate>,
) {
    for DropdownChanged { dropdown, index } in chosen.read() {
        let Ok(PaneDropdown(pane)) = panes.get(*dropdown) else { continue };
        menu.0.set_cursor(*pane, *index);
        dirty.form = true;
    }
    for Pressed(entity) in pressed.read() {
        if let Ok(PaneChoice { pane, index }) = choices.get(*entity) {
            if menu.0.cursor(*pane) != *index {
                menu.0.set_cursor(*pane, *index);
                dirty.form = true;
            }
            continue;
        }
        match marks.get(*entity) {
            Ok(Control::Back) => {
                navigate.write(Navigate(AppScreen::MainMenu));
            }
            Ok(Control::Start) => {
                let Some(choice) = menu.0.choice() else {
                    dirty.notice = Some("No deck to play: the list is empty".to_string());
                    continue;
                };
                if let Some(problem) = menu.0.choice_problem() {
                    dirty.notice = Some(problem);
                    continue;
                }
                match start(&core, &choice) {
                    Ok(active) => {
                        commands.insert_resource(active);
                        navigate.write(Navigate(AppScreen::Game));
                    }
                    Err(error) => dirty.notice = Some(format!("The game could not start: {error}")),
                }
            }
            Err(_) => {}
        }
    }
}

fn refresh(
    mut commands: Commands,
    mut dirty: ResMut<Dirty>,
    form: Query<Entity, With<Form>>,
    mut notice: Query<&mut Text, With<NoticeLine>>,
    theme: Res<Theme>,
    menu: Res<Model>,
) {
    let Dirty { form: reform, notice: note } = std::mem::take(&mut *dirty);
    if reform && let Ok(form) = form.single() {
        commands.entity(form).despawn_children().with_children(|parent| spawn_form(parent, &theme, &menu.0));
    }
    if let Some(note) = note {
        for mut text in &mut notice {
            text.0 = note.clone();
        }
    }
}
