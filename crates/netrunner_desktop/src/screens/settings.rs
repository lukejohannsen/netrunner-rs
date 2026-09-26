//! The settings screen: one row per `models::settings::Row`, each with
//! its control, saved after every change.
//!
//! The rows are rebuilt whenever a value changes rather than patched in
//! place — eight rows of text are cheap to respawn, and one spawn function
//! is one place the layout can be wrong. The board's gear menu draws a
//! subset of the same rows with [`spawn_rows`], so a row has one shape
//! and one control wherever it appears.

use bevy::prelude::*;

use netrunner_client::record::player_name;
use netrunner_client::standing::describe;

use crate::core::{ClientCore, Notices};
use crate::models::settings::{self as model, Intent, Row, MAX_NAME_LEN};
use netrunner_client::settings::Table;
use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::skin;
use crate::table;
use crate::theme::Theme;
use crate::widgets::text_field::{TextField, TextFieldEvent};
use crate::widgets::{self, Pressed};

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::Settings), spawn)
            .add_systems(Update, (controls, open_field, field_edits, refresh).chain().run_if(in_state(AppScreen::Settings)));
    }
}

/// The controls, on their buttons. `pub` because the board's gear menu
/// spawns the same rows and applies `Intent` presses itself.
#[derive(Component, Debug, Clone, PartialEq)]
pub enum Control {
    Intent(Intent),
    /// Edit a text row: the player's name or the relay.
    Edit(Row),
    Back,
}

/// The panel the rows live in; respawned on every change.
#[derive(Component)]
struct Rows;

/// The panel the remembered answers live in; respawned with the rows.
#[derive(Component)]
struct Answers;

/// Set by a change; `refresh` rebuilds the rows and clears it.
#[derive(Resource, Default)]
struct Dirty(bool);

/// Set by an Edit button; `open_field` spawns that row's field and
/// clears it.
#[derive(Resource, Default)]
struct EditRequested(Option<Row>);

/// Which row an open field edits.
#[derive(Component, Clone, Copy)]
struct Editing(Row);

/// A relay's URL is long; a name is `MAX_NAME_LEN`.
const MAX_RELAY_LEN: usize = 200;

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>) {
    commands.init_resource::<Dirty>();
    commands.init_resource::<EditRequested>();
    let saved_where = core.settings_path.as_ref().map_or_else(
        || "Not saved: the OS has no data directory, so these apply to this session only".to_string(),
        |path| format!("Saved to {}", path.display()),
    );
    let rows = commands.spawn((Rows, widgets::roomy_panel(&theme, px(720)))).id();
    let answers = commands.spawn((Answers, widgets::roomy_panel(&theme, px(420)))).id();
    // The answers beside the rows rather than under them: twelve rows
    // already take most of a window's height, and the list grows with
    // every card answered.
    let columns = commands.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, align_items: AlignItems::FlexStart, column_gap: px(16), row_gap: px(16), ..default() }).add_children(&[rows, answers]).id();
    commands.spawn((screen_root(AppScreen::Settings, &theme), children![
        widgets::heading(&theme, AppScreen::Settings.title()),
        widgets::dim(&theme, saved_where),
        widgets::button(&theme, "Back", Val::Auto, Control::Back),
    ])).add_child(columns);
    commands.entity(rows).with_children(|parent| spawn_rows(parent, &theme, &core, &Row::ALL));
    commands.entity(answers).with_children(|parent| spawn_answers(parent, &theme, &core));
}

/// The cards whose "you may" the person answered for good, each with the
/// button that makes it ask again (`netrunner_client::standing`). The
/// one place an answer is undone, so it names the card and the words it
/// offers, as the pop-up did when the answer was given.
fn spawn_answers(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore) {
    parent.spawn(widgets::label(theme, "Remembered answers"));
    if core.settings.answers.is_empty() {
        parent.spawn((widgets::dim(theme, "None yet. A card that asks \"you may\" can be answered Always or Never from its prompt."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        return;
    }
    for (index, entry) in core.settings.answers.iter().enumerate() {
        let words = format!("{}: {}", entry.answer.label(), describe(&entry.key, &core.registry));
        let mut node = parent.spawn((widgets::row(12.0), children![
            (widgets::dim(theme, words), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }, TextLayout::new(Justify::Left, LineBreak::WordBoundary)),
        ]));
        node.entry::<Node>().and_modify(|mut node| node.width = percent(100));
        node.with_children(|controls| {
            controls.spawn(widgets::button(theme, "Ask", Val::Auto, Control::Intent(Intent::Forget(index))));
        });
    }
    parent.spawn(widgets::button(theme, "Forget all", Val::Auto, Control::Intent(Intent::ForgetAll)));
}

/// One row per entry of `rows`: the label, the value, and its control
/// carrying the `Intent` a press means.
///
/// A row is as wide as its panel: the label takes what the value and
/// the control leave, and the value wraps rather than pushing the
/// control out. The first cut gave the label and the value fixed
/// widths sized for the Settings screen's 720 px panel, and in the
/// board's 560 px options window the toggles landed outside it.
pub fn spawn_rows(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, rows: &[Row]) {
    let login = player_name(None);
    for &row in rows {
        let mut value = model::value(&core.settings, row, &login);
        // The one row whose shown value may come off the disk: a table
        // carrying a `table.json` with a `name` is called by it. The
        // model cannot do this — it is pure — and this is the only place
        // a row is drawn, for the settings screen and the gear menu both.
        if let Table::Named(folder) = &core.settings.desktop.table
            && row == Row::Table
        {
            value = table::manifest(folder).label(folder).to_string();
        }
        let mut node = parent.spawn((widgets::row(12.0), children![
            (widgets::label(theme, row.label()), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }, TextLayout::new(Justify::Left, LineBreak::WordBoundary)),
            (widgets::label(theme, value), Node { width: px(170), flex_shrink: 0.0, ..default() }, TextLayout::new(Justify::Left, LineBreak::WordBoundary)),
        ]));
        node.entry::<Node>().and_modify(|mut node| node.width = percent(100));
        node.with_children(|controls| {
            if row.is_stepped() {
                controls.spawn(widgets::round_button(theme, "<", Control::Intent(Intent::Step(row, -1))));
                controls.spawn(widgets::round_button(theme, ">", Control::Intent(Intent::Step(row, 1))));
            } else if matches!(row, Row::Player | Row::Relay) {
                controls.spawn(widgets::button(theme, "Edit", Val::Auto, Control::Edit(row)));
            } else {
                controls.spawn(widgets::button(theme, "Toggle", Val::Auto, Control::Intent(Intent::Toggle(row))));
            }
        });
    }
}

fn controls(
    mut pressed: MessageReader<Pressed>,
    marks: Query<&Control>,
    mut core: ResMut<ClientCore>,
    mut dirty: ResMut<Dirty>,
    mut edit: ResMut<EditRequested>,
    mut notices: ResMut<Notices>,
    mut navigate: MessageWriter<Navigate>,
) {
    for Pressed(entity) in pressed.read() {
        match marks.get(*entity) {
            Ok(Control::Back) => {
                navigate.write(Navigate(AppScreen::MainMenu));
            }
            Ok(Control::Edit(row)) => edit.0 = Some(*row),
            Ok(Control::Intent(intent)) => {
                let changed = model::apply(&mut core.settings, intent.clone(), &table::available(), &skin::available());
                if changed {
                    persist(&core, &mut notices);
                    dirty.0 = true;
                }
            }
            Err(_) => {}
        }
    }
}

/// Spawns the edited row's field under the rows when Edit was pressed
/// and no field is open. Its own system so `controls` stays a dispatcher.
fn open_field(
    mut commands: Commands,
    mut edit: ResMut<EditRequested>,
    fields: Query<Entity, With<TextField>>,
    rows: Query<Entity, With<Rows>>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
) {
    let Some(row) = edit.0.take() else { return };
    if !fields.is_empty() {
        return;
    }
    let Ok(rows) = rows.single() else { return };
    let (current, max_len, hint) = match row {
        Row::Relay => (core.settings.relay.clone().unwrap_or_default(), MAX_RELAY_LEN, "  Empty for the public relays, off for none, or a relay's URL. Enter saves, Escape cancels"),
        _ => (core.settings.player.clone().unwrap_or_default(), MAX_NAME_LEN, "  Enter saves, Escape cancels"),
    };
    commands.entity(rows).with_children(|parent| {
        parent.spawn((
            TextField { text: current.clone(), max_len },
            Editing(row),
            widgets::field_node(Val::Auto),
            BackgroundColor(theme.glass_strong),
            BorderColor::all(theme.accent),
            children![widgets::label(&theme, format!("{current}|")), (widgets::dim(&theme, hint), TextLayout::new(Justify::Left, LineBreak::WordBoundary))],
        ));
    });
}

fn field_edits(
    mut commands: Commands,
    fields: Query<(Entity, &TextFieldEvent, &Editing)>,
    mut core: ResMut<ClientCore>,
    mut dirty: ResMut<Dirty>,
    mut notices: ResMut<Notices>,
) {
    for (entity, event, Editing(row)) in &fields {
        let intent = match (event, row) {
            (TextFieldEvent::Committed(text), Row::Relay) => {
                // Refused here with the reason, rather than dropped by
                // the model in silence.
                if let Err(reason) = model::relay_setting(text) {
                    notices.push(format!("Relay not changed: {reason}"));
                }
                Intent::RelayEdited(text.clone())
            }
            (TextFieldEvent::Committed(name), _) => Intent::NameEdited(Some(name.clone())),
            (TextFieldEvent::Cancelled, _) => Intent::NameEdited(None),
        };
        if model::apply(&mut core.settings, intent, &table::available(), &skin::available()) {
            persist(&core, &mut notices);
        }
        commands.entity(entity).despawn();
        dirty.0 = true;
    }
}

fn refresh(mut commands: Commands, mut dirty: ResMut<Dirty>, rows: Query<Entity, With<Rows>>, answers: Query<Entity, With<Answers>>, theme: Res<Theme>, core: Res<ClientCore>) {
    if !dirty.0 {
        return;
    }
    dirty.0 = false;
    if let Ok(rows) = rows.single() {
        commands.entity(rows).despawn_children();
        commands.entity(rows).with_children(|parent| spawn_rows(parent, &theme, &core, &Row::ALL));
    }
    if let Ok(answers) = answers.single() {
        commands.entity(answers).despawn_children();
        commands.entity(answers).with_children(|parent| spawn_answers(parent, &theme, &core));
    }
}

fn persist(core: &ClientCore, notices: &mut Notices) {
    if let Err(error) = core.save_settings() {
        notices.push(format!("Settings not saved: {error}"));
    }
}
