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
            .add_systems(Update, (controls, open_name_field, name_edits, refresh).chain().run_if(in_state(AppScreen::Settings)));
    }
}

/// The controls, on their buttons. `pub` because the board's gear menu
/// spawns the same rows and applies `Intent` presses itself.
#[derive(Component, Debug, Clone, PartialEq)]
pub enum Control {
    Intent(Intent),
    EditName,
    Back,
}

/// The panel the rows live in; respawned on every change.
#[derive(Component)]
struct Rows;

/// Set by a change; `refresh` rebuilds the rows and clears it.
#[derive(Resource, Default)]
struct Dirty(bool);

/// Set by the Edit button; `open_name_field` spawns the field and
/// clears it.
#[derive(Resource, Default)]
struct EditRequested(bool);

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>) {
    commands.init_resource::<Dirty>();
    commands.init_resource::<EditRequested>();
    let saved_where = core.settings_path.as_ref().map_or_else(
        || "Not saved: the OS has no data directory, so these apply to this session only".to_string(),
        |path| format!("Saved to {}", path.display()),
    );
    let rows = commands.spawn((Rows, widgets::panel(&theme, px(720)))).id();
    commands.spawn((screen_root(AppScreen::Settings, theme.background), children![
        widgets::heading(&theme, AppScreen::Settings.title()),
        widgets::dim(&theme, saved_where),
        widgets::button(&theme, "Back", Val::Auto, Control::Back),
    ])).add_child(rows);
    commands.entity(rows).with_children(|parent| spawn_rows(parent, &theme, &core, &Row::ALL));
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
                controls.spawn(widgets::button(theme, "<", px(44), Control::Intent(Intent::Step(row, -1))));
                controls.spawn(widgets::button(theme, ">", px(44), Control::Intent(Intent::Step(row, 1))));
            } else if row == Row::Player {
                controls.spawn(widgets::button(theme, "Edit", Val::Auto, Control::EditName));
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
            Ok(Control::EditName) => edit.0 = true,
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

/// Spawns the name field under the rows when Edit was pressed and no
/// field is open. Its own system so `controls` stays a dispatcher.
fn open_name_field(
    mut commands: Commands,
    mut edit: ResMut<EditRequested>,
    fields: Query<Entity, With<TextField>>,
    rows: Query<Entity, With<Rows>>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
) {
    if !edit.0 {
        return;
    }
    edit.0 = false;
    if !fields.is_empty() {
        return;
    }
    let Ok(rows) = rows.single() else { return };
    let current = core.settings.player.clone().unwrap_or_default();
    commands.entity(rows).with_children(|parent| {
        parent.spawn((
            TextField { text: current.clone(), max_len: MAX_NAME_LEN },
            Node { padding: UiRect::all(px(8)), border: UiRect::all(px(1)), ..default() },
            BorderColor::all(theme.accent),
            children![widgets::label(&theme, format!("{current}|")), widgets::dim(&theme, "  Enter saves, Escape cancels")],
        ));
    });
}

fn name_edits(
    mut commands: Commands,
    fields: Query<(Entity, &TextFieldEvent)>,
    mut core: ResMut<ClientCore>,
    mut dirty: ResMut<Dirty>,
    mut notices: ResMut<Notices>,
) {
    for (entity, event) in &fields {
        let intent = match event {
            TextFieldEvent::Committed(name) => Intent::NameEdited(Some(name.clone())),
            TextFieldEvent::Cancelled => Intent::NameEdited(None),
        };
        if model::apply(&mut core.settings, intent, &table::available(), &skin::available()) {
            persist(&core, &mut notices);
        }
        commands.entity(entity).despawn();
        dirty.0 = true;
    }
}

fn refresh(mut commands: Commands, mut dirty: ResMut<Dirty>, rows: Query<Entity, With<Rows>>, theme: Res<Theme>, core: Res<ClientCore>) {
    if !dirty.0 {
        return;
    }
    dirty.0 = false;
    let Ok(rows) = rows.single() else { return };
    commands.entity(rows).despawn_children();
    commands.entity(rows).with_children(|parent| spawn_rows(parent, &theme, &core, &Row::ALL));
}

fn persist(core: &ClientCore, notices: &mut Notices) {
    if let Err(error) = core.save_settings() {
        notices.push(format!("Settings not saved: {error}"));
    }
}
