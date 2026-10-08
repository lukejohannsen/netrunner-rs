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
use netrunner_client::notes::{self, Notes};
use netrunner_client::play::MatchMessage;
use netrunner_client::replay::{Replay, Start};

use crate::audio::{ButtonSound, Sfx};
use crate::core::ClientCore;
use crate::models::game::{Game, Intent, ReplayAt};
use crate::models::pace::Pacer;
use crate::models::replay::{self as model, Step};
use crate::nav::{screen_root, Navigate};
use crate::screens::game::{Dirty, Model, Pace, Rail};
use crate::screens::AppScreen;
use crate::theme::{size, Theme};
use crate::widgets::text_field::{TextField, TextFieldEvent};
use crate::widgets::{self, Pressed};

pub struct ReplayPlugin;

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::Replay), spawn)
            .add_systems(Update, pick.run_if(in_state(AppScreen::Replay)))
            // Between the board's input and its redraw: a step moves the
            // model and marks it, and the frame that read the press draws
            // it. After the text field's own system, so the N that opens
            // the note's editor is not also typed into it: that system
            // reads the frame's keys and then feeds every field that
            // exists, and the editor spawned this frame would.
            .add_systems(Update, steps.after(crate::screens::game::controls).after(crate::widgets::text_field::edit_text_fields).before(crate::screens::game::fit).run_if(in_state(AppScreen::Game)));
    }
}

/// The record on the board, with the person's notes on it. Present only
/// while one is: the list removes it on entry and the board on exit.
#[derive(Resource)]
pub struct ActiveReplay(pub Replay, pub NoteBook);

/// The notes beside a record (`netrunner_client::notes`), and the record
/// they are beside, which is where a change is written.
pub struct NoteBook {
    pub path: PathBuf,
    pub notes: Notes,
}

/// The rail's button that opens the note at this position for writing;
/// N is its key.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditNote;

/// The one-line editor on the rail while a note is being written. Its
/// keys are its own (`widgets::text_field`): the replay's and the
/// board's stand down while it exists.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteEditor;

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
pub fn open(core: &ClientCore, path: &Path, at: Option<Start>) -> Result<(Replay, NoteBook), String> {
    let replay = Replay::open(path, (*core.registry).clone(), None, at).map_err(|error| error.to_string())?;
    Ok((replay, NoteBook { path: path.to_path_buf(), notes: notes::load(path) }))
}

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, waiting: Option<Res<OpenReplay>>, mut navigate: MessageWriter<Navigate>) {
    commands.remove_resource::<ActiveReplay>();
    let mut error = None;
    if let Some(waiting) = waiting {
        commands.remove_resource::<OpenReplay>();
        match open(&core, &waiting.0, waiting.1) {
            Ok((replay, book)) => {
                commands.insert_resource(ActiveReplay(replay, book));
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
                    // A row of a list, not a button among buttons: it
                    // keeps the panel's width, past the pill's cap, so a
                    // saved game's long name is one line, not three.
                    let mut row = panel.spawn(widgets::button(&theme, label, percent(100), ReportRow(path.clone())));
                    row.entry::<Node>().and_modify(|mut node| node.max_width = Val::Auto);
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
        root.spawn(widgets::button(&theme, "Back", Val::Auto, (BackButton, ButtonSound(Sfx::Back))));
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
            Ok((replay, book)) => {
                commands.insert_resource(ActiveReplay(replay, book));
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
pub fn board_for(core: &ClientCore, replay: &Replay, notes: &Notes) -> Game {
    let mut game = Game::replay(core.registry.clone(), replay.side(), at(replay, notes));
    game.apply(Intent::Show { view: Box::new(replay.view().clone()), log: replay.log().to_vec() });
    game
}

fn at(replay: &Replay, notes: &Notes) -> ReplayAt {
    ReplayAt { cursor: replay.cursor(), len: replay.len(), title: replay.title().to_string(), note: notes.get(replay.cursor()).map(str::to_string), noted: notes.positions() }
}

/// Spawns the note's editor on the rail, holding the note so far.
fn spawn_editor(commands: &mut Commands, theme: &Theme, rail: Entity, current: &str) {
    commands.entity(rail).with_children(|rail| {
        rail.spawn((
            NoteEditor,
            TextField::new(current.to_string(), notes::MAX_LEN),
            widgets::field_node(percent(100)),
            BackgroundColor(theme.glass_strong),
            BorderColor::all(theme.accent),
            children![(Text::new(format!("{current}|")), theme.font(size::BODY), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::AnyCharacter))],
        ));
    });
}

/// The replay bar's buttons and its keys, applied to the record and the
/// board. The keys are the arrows, Page Up and Down, Home and End, and S
/// for the other chair — none of which the board's own keys use — and
/// they stand down while anything covers the board, as the board's do.
#[allow(clippy::too_many_arguments)]
fn steps(
    mut commands: Commands,
    mut pressed: MessageReader<Pressed>,
    marks: Query<&ReplayClick>,
    edits: Query<(), With<EditNote>>,
    editor: Query<(Entity, Option<&TextFieldEvent>), With<NoteEditor>>,
    rail: Query<Entity, With<Rail>>,
    keys: Res<ButtonInput<KeyCode>>,
    replay: Option<ResMut<ActiveReplay>>,
    model: Option<ResMut<Model>>,
    pace: Option<ResMut<Pace>>,
    dirty: Option<ResMut<Dirty>>,
    core: Res<ClientCore>,
    theme: Res<Theme>,
    mut notices: ResMut<crate::core::Notices>,
) {
    let (Some(mut replay), Some(mut model), Some(mut pace), Some(mut dirty)) = (replay, model, pace, dirty) else {
        pressed.clear();
        return;
    };
    // A note being written: its keys are the editor's, and a commit
    // writes the book beside the record (`netrunner_client::notes`).
    if let Ok((entity, event)) = editor.single() {
        pressed.clear();
        match event {
            Some(TextFieldEvent::Committed(text)) => {
                let ActiveReplay(replay, book) = &mut *replay;
                book.notes.set(replay.cursor(), text);
                if let Err(error) = notes::save(&book.path, &book.notes) {
                    notices.push(format!("Note not saved: {error}"));
                }
                commands.entity(entity).despawn();
                model.0.replay = Some(at(replay, &book.notes));
                dirty.all();
            }
            Some(TextFieldEvent::Cancelled) => {
                commands.entity(entity).despawn();
            }
            None => {}
        }
        return;
    }
    let mut asked: Vec<Step> = Vec::new();
    let mut edit = false;
    for Pressed(entity) in pressed.read() {
        if let Ok(click) = marks.get(*entity) {
            asked.push(click.0);
        } else if edits.contains(*entity) {
            edit = true;
        }
    }
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
        if keys.just_pressed(KeyCode::KeyN) {
            edit = true;
        }
    }
    if edit && let Ok(rail) = rail.single() {
        let ActiveReplay(replay, book) = &*replay;
        spawn_editor(&mut commands, &theme, rail, book.notes.get(replay.cursor()).unwrap_or_default());
        return;
    }
    for step in asked {
        let ActiveReplay(replay, book) = &mut *replay;
        let noted = book.notes.positions();
        if !step.moves(replay.cursor(), replay.len(), &noted) {
            continue;
        }
        let before = replay.cursor();
        match step {
            Step::First => replay.seek(0),
            Step::Back(by) => replay.step_back(by),
            Step::Forward(by) => replay.step_forward(by),
            Step::Last => replay.seek(usize::MAX),
            Step::PreviousNote => replay.seek(book.notes.previous_before(before).expect("moves checked")),
            Step::NextNote => replay.seek(book.notes.next_after(before).expect("moves checked")),
            Step::SwapChair => replay.set_side(replay.side().other()),
        }
        let speed = core.settings.desktop.animation_speed;
        if step == Step::SwapChair {
            // The chair is the board's whole frame of reference — which
            // hand is face up, which side is near — so the other chair is
            // a new board, as a new match would be.
            model.0 = board_for(&core, replay, &book.notes);
            pace.0 = Pacer::new(replay.side(), speed);
        } else if replay.cursor() == before + 1 && pace.0.is_empty() {
            let entry = replay.entry(replay.cursor()).expect("a step on has an entry").clone();
            pace.0.push(MatchMessage::Applied { entry, view: Box::new(replay.view().clone()) });
            model.0.replay = Some(at(replay, &book.notes));
        } else {
            pace.0 = Pacer::new(replay.side(), speed);
            model.0.apply(Intent::Show { view: Box::new(replay.view().clone()), log: replay.log().to_vec() });
            model.0.replay = Some(at(replay, &book.notes));
        }
        dirty.all();
    }
}
