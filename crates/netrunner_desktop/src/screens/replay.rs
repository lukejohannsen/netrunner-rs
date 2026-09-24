//! Replays: the saved games listed, and one stepped through on the board
//! (Phase 7 §8 item 5, §4ah).
//!
//! **The board is the game screen's, not a copy of it.** Picking a record
//! puts an [`ActiveReplay`] where an `ActiveMatch` would be and goes to
//! `AppScreen::Game`, which draws it with the same systems it draws a
//! match with: the same fit, sheets, run lane, highlights and log. What a
//! replay changes is the source — positions `netrunner_client::replay`
//! has already computed, rather than a match thread — and that nothing
//! is ever awaiting, so no entry, glow or decision is offered
//! (`models::game::Game::replay`). A second board for replays was the
//! other design, and it would have been a second place for every rule in
//! AGENTS.md §5 to be kept.
//!
//! **One step on is played as the match played it.** The step's masked
//! entry and view go through the pacer as a `MatchMessage::Applied`, so a
//! run is walked a beat at a time and the board lights what moved. Any
//! other move — back, to either end, a second step while the first is
//! still being paced, the other chair — puts the board at the new place
//! at once (`Intent::Show`): nothing moved *to* there, and a replay of
//! twenty beats queued behind a held key would be the replay deciding
//! the pace instead of the person.
//!
//! This screen itself is the list, newest first, of what
//! `netrunner_client::bug_report::list` finds where the client saves its
//! reports; a record from `--headless --record` copied there opens too.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use netrunner_client::play::MatchMessage;
use netrunner_client::replay::{Replay, Start};

use crate::core::ClientCore;
use crate::models::game::{Game, Intent, ReplayAt};
use crate::models::pace::Pacer;
use crate::models::replay::{self as model, Step};
use crate::nav::{screen_root, Navigate};
use crate::screens::game::{Dirty, Model, Pace};
use crate::screens::AppScreen;
use crate::theme::Theme;
use crate::widgets::{self, Pressed};

pub struct ReplayPlugin;

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::Replay), spawn)
            .add_systems(Update, pick.run_if(in_state(AppScreen::Replay)))
            // Between the board's input and its redraw: a step moves the
            // model and marks it, and the frame that read the press draws
            // it.
            .add_systems(Update, steps.after(crate::screens::game::controls).before(crate::screens::game::fit).run_if(in_state(AppScreen::Game)));
    }
}

/// The record on the board. Present only while one is: the list removes
/// it on entry and the board on exit.
#[derive(Resource)]
pub struct ActiveReplay(pub Replay);

/// A record to open as soon as the list is entered — the board's "Watch
/// it" beside a report it just saved, or `NETRUNNER_REPLAY` — and where in
/// it, when not where the record says.
#[derive(Resource)]
pub struct OpenReplay(pub PathBuf, pub Option<Start>);

/// A button of the replay bar.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayClick(pub Step);

/// A record in the list.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ReportRow(pub PathBuf);

/// Why the record picked last would not open, for a test to read.
#[derive(Component)]
pub struct OpenError;

#[derive(Component)]
struct BackButton;

/// Replays the record at `path` against the client's registry, opening
/// where a record says it should: a bug report at its end, from the
/// person's chair.
pub fn open(core: &ClientCore, path: &Path, at: Option<Start>) -> Result<Replay, String> {
    Replay::open(path, (*core.registry).clone(), None, at).map_err(|error| error.to_string())
}

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, waiting: Option<Res<OpenReplay>>, mut navigate: MessageWriter<Navigate>) {
    commands.remove_resource::<ActiveReplay>();
    let mut error = None;
    if let Some(waiting) = waiting {
        commands.remove_resource::<OpenReplay>();
        match open(&core, &waiting.0, waiting.1) {
            Ok(replay) => {
                commands.insert_resource(ActiveReplay(replay));
                navigate.write(Navigate(AppScreen::Game));
            }
            Err(reason) => error = Some(reason),
        }
    }
    let reports = core.reports_dir.as_deref().map(netrunner_client::bug_report::list).unwrap_or_default();
    let scroll = commands
        .spawn((
            bevy::ui_widgets::ScrollArea,
            Node { flex_grow: 1.0, min_width: px(0), height: percent(100), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, overflow: Overflow::scroll_y(), ..default() },
        ))
        .with_children(|page| {
            page.spawn(widgets::roomy_panel(&theme, px(900))).with_children(|panel| {
                if reports.is_empty() {
                    let where_ = core.reports_dir.as_ref().map_or_else(|| "nowhere: this system has no data directory".to_string(), |dir| dir.display().to_string());
                    panel.spawn((
                        widgets::dim(&theme, format!("No saved games yet. The gear's \"Save a bug report\" saves one during a game, and a stalled game saves itself; they go to {where_}.")),
                        TextLayout::new(Justify::Left, LineBreak::WordBoundary),
                    ));
                }
                for path in &reports {
                    let label = model::label(path, netrunner_client::replay::header(path).as_ref());
                    let mut row = panel.spawn(widgets::button(&theme, label, percent(100), ReportRow(path.clone())));
                    row.entry::<Node>().and_modify(|mut node| node.justify_content = JustifyContent::FlexStart);
                }
            });
        })
        .id();
    let body = commands
        .spawn(Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, justify_content: JustifyContent::Center, column_gap: px(8), ..default() })
        .add_child(scroll)
        .with_children(|body| {
            body.spawn(widgets::scrollbar(&theme, scroll));
        })
        .id();
    let mut root = commands.spawn(screen_root(AppScreen::Replay, &theme));
    root.with_children(|root| {
        root.spawn(widgets::heading(&theme, "Replays"));
        let text = error.unwrap_or_default();
        root.spawn((OpenError, widgets::dim(&theme, text), TextLayout::new(Justify::Center, LineBreak::WordBoundary)));
    });
    root.add_child(body);
    root.with_children(|root| {
        root.spawn(widgets::button(&theme, "Back", Val::Auto, BackButton));
    });
}

/// A press on a record opens it on the board; one that will not open says
/// why above the list — a record written by an engine whose rules have
/// since changed names the entry it no longer replays at — and stays.
fn pick(
    mut commands: Commands,
    mut pressed: MessageReader<Pressed>,
    rows: Query<&ReportRow>,
    back: Query<(), With<BackButton>>,
    mut error: Query<&mut Text, With<OpenError>>,
    core: Res<ClientCore>,
    mut navigate: MessageWriter<Navigate>,
) {
    for Pressed(entity) in pressed.read() {
        if back.contains(*entity) {
            navigate.write(Navigate(AppScreen::MainMenu));
            return;
        }
        let Ok(ReportRow(path)) = rows.get(*entity) else { continue };
        match open(&core, path, None) {
            Ok(replay) => {
                commands.insert_resource(ActiveReplay(replay));
                navigate.write(Navigate(AppScreen::Game));
                return;
            }
            Err(reason) => {
                for mut text in &mut error {
                    text.0 = format!("{} would not open: {reason}", path.display());
                }
            }
        }
    }
}

/// The board a replay opens on: its position, from its chair.
pub fn board_for(core: &ClientCore, replay: &Replay) -> Game {
    let mut game = Game::replay(core.registry.clone(), replay.side(), at(replay));
    game.apply(Intent::Show { view: Box::new(replay.view().clone()), log: replay.log().to_vec() });
    game
}

fn at(replay: &Replay) -> ReplayAt {
    ReplayAt { cursor: replay.cursor(), len: replay.len(), title: replay.title().to_string() }
}

/// The replay bar's buttons and its keys, applied to the record and the
/// board. The keys are the arrows, Page Up and Down, Home and End, and S
/// for the other chair — none of which the board's own keys use — and
/// they stand down while anything covers the board, as the board's do.
#[allow(clippy::too_many_arguments)]
fn steps(
    mut pressed: MessageReader<Pressed>,
    marks: Query<&ReplayClick>,
    keys: Res<ButtonInput<KeyCode>>,
    replay: Option<ResMut<ActiveReplay>>,
    model: Option<ResMut<Model>>,
    pace: Option<ResMut<Pace>>,
    dirty: Option<ResMut<Dirty>>,
    core: Res<ClientCore>,
) {
    let (Some(mut replay), Some(mut model), Some(mut pace), Some(mut dirty)) = (replay, model, pace, dirty) else {
        pressed.clear();
        return;
    };
    let mut asked: Vec<Step> = pressed.read().filter_map(|Pressed(entity)| marks.get(*entity).ok()).map(|click| click.0).collect();
    if !model.0.covered() {
        for (key, step) in [
            (KeyCode::ArrowRight, Step::Forward(1)),
            (KeyCode::ArrowLeft, Step::Back(1)),
            (KeyCode::PageDown, Step::Forward(10)),
            (KeyCode::PageUp, Step::Back(10)),
            (KeyCode::Home, Step::First),
            (KeyCode::End, Step::Last),
            (KeyCode::KeyS, Step::SwapChair),
        ] {
            if keys.just_pressed(key) {
                asked.push(step);
            }
        }
    }
    for step in asked {
        let replay = &mut replay.0;
        if !step.moves(replay.cursor(), replay.len()) {
            continue;
        }
        let before = replay.cursor();
        match step {
            Step::First => replay.seek(0),
            Step::Back(by) => replay.step_back(by),
            Step::Forward(by) => replay.step_forward(by),
            Step::Last => replay.seek(usize::MAX),
            Step::SwapChair => replay.set_side(replay.side().other()),
        }
        let speed = core.settings.desktop.animation_speed;
        if step == Step::SwapChair {
            // The chair is the board's whole frame of reference — which
            // hand is face up, which side is near — so the other chair is
            // a new board, as a new match would be.
            model.0 = board_for(&core, replay);
            pace.0 = Pacer::new(replay.side(), speed);
        } else if replay.cursor() == before + 1 && pace.0.is_empty() {
            let entry = replay.entry(replay.cursor()).expect("a step on has an entry").clone();
            pace.0.push(MatchMessage::Applied { entry, view: Box::new(replay.view().clone()) });
            model.0.replay = Some(at(replay));
        } else {
            pace.0 = Pacer::new(replay.side(), speed);
            model.0.apply(Intent::Show { view: Box::new(replay.view().clone()), log: replay.log().to_vec() });
            model.0.replay = Some(at(replay));
        }
        dirty.all();
    }
}
