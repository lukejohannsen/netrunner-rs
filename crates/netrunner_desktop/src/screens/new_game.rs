//! The new-game form: chair, rung, style and the two decks, each a
//! drop-down over `netrunner_client::start::StartMenu` — the state
//! machine the terminal's form runs, so the two clients offer the same
//! lists, suggest the same rung and default to the same decks.
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
use netrunner_client::start::{Level, Pane, StartChoice, StartMenu, DEFAULT_CORP_DECK, DEFAULT_RUNNER_DECK};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

use crate::core::ClientCore;
use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::theme::Theme;
use crate::widgets::dropdown::{spawn_dropdown, Choice, DropdownChanged};
use crate::widgets::{self, Pressed};

pub struct NewGamePlugin;

impl Plugin for NewGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::NewGame), spawn).add_systems(Update, (controls, refresh).chain().run_if(in_state(AppScreen::NewGame)));
    }
}

/// The match a Start put together, for the game screen: the handle, and
/// the choice the form reopens on afterwards (`None` for a dev game).
#[derive(Resource)]
pub struct ActiveMatch {
    pub handle: MatchHandle,
    pub choice: Option<StartChoice>,
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

/// Which pane a drop-down is; on its root.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneDropdown(pub Pane);

/// The drop-downs, respawned when the chair changes (every other list
/// depends on it).
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
    let form = commands.spawn((Form, Node { flex_direction: FlexDirection::Column, row_gap: px(10), ..default() })).id();
    commands.entity(form).with_children(|parent| spawn_form(parent, &theme, &menu));
    let buttons = commands
        .spawn(widgets::row(12.0))
        .with_children(|parent| {
            parent.spawn(widgets::button(&theme, "Start", px(160), Control::Start));
            parent.spawn(widgets::button(&theme, "Back", Val::Auto, Control::Back));
        })
        .id();
    let panel = commands.spawn(widgets::panel(&theme, px(720))).add_child(form).add_child(buttons).id();
    commands
        .spawn((screen_root(AppScreen::NewGame, theme.background), children![
            widgets::heading(&theme, AppScreen::NewGame.title()),
            widgets::dim(&theme, "A casual game against a rung of the ladder, in the style your deck chooses. A move can always be taken back."),
        ]))
        .add_child(panel)
        .with_children(|parent| {
            parent.spawn(widgets::notice(&theme, notice, NoticeLine));
        });
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
    StartMenu::open(&decks_dir, core.record_path.as_deref(), &core.player_name(), &core.registry, defaults())
}

fn spawn_form(parent: &mut ChildSpawnerCommands, theme: &Theme, menu: &StartMenu) {
    for (pane, title, rows, cursor) in menu.panes() {
        // The terminal marks the suggestion with `◆`, which the text face
        // has no glyph for.
        let choices = rows.into_iter().map(|row| Choice::plain(row.replace("  ◆ suggested", " (suggested)"))).collect();
        spawn_dropdown(parent, theme, title, choices, cursor, PaneDropdown(pane));
    }
}

/// A seed off the clock: the terminal uses `rand::random`, and the
/// desktop has no reason to carry a random crate for one number a game.
fn seed_from_clock() -> u64 {
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
        record,
    };
    let handle = MatchHandle::start_local(spec)?;
    Ok(ActiveMatch { handle, choice: Some(choice.clone()) })
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
    mut menu: ResMut<Model>,
    mut dirty: ResMut<Dirty>,
    core: Res<ClientCore>,
    mut navigate: MessageWriter<Navigate>,
) {
    for DropdownChanged { dropdown, index } in chosen.read() {
        let Ok(PaneDropdown(pane)) = panes.get(*dropdown) else { continue };
        menu.0.set_cursor(*pane, *index);
        if *pane == Pane::Chair {
            dirty.form = true;
        }
    }
    for Pressed(entity) in pressed.read() {
        match marks.get(*entity) {
            Ok(Control::Back) => {
                navigate.write(Navigate(AppScreen::MainMenu));
            }
            Ok(Control::Start) => {
                let Some(choice) = menu.0.choice() else {
                    dirty.notice = Some("No deck to play: the list is empty".to_string());
                    continue;
                };
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
