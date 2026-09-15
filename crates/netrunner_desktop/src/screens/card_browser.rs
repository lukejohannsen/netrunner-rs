//! The card browser: every printing as a face, filters down the left,
//! the chosen card open on the right with its printed text and how the
//! engine reads it.
//!
//! Three columns under the heading. The rail holds the search field, the
//! filter chips, the image download and Back. The grid is a scrolling
//! CSS grid of thumb faces, each a button. The inspector is the large
//! face, the numbers line the terminal's `card_modal` prints, the
//! flavour, the engine's reading (`prose::engine_reading`), whether the
//! engine plays it, and whether its picture is cached. The chips and the
//! grid and the inspector are respawned on every change of the model
//! (`models::browser`), the settings screen's pattern; the search field
//! and the progress row are not, because one holds what is being typed
//! and the other moves every frame.
//!
//! **Escape.** While the search field is open it captures Escape: a
//! first press clears the text, a second closes the field, and only
//! then does Escape lead back to the menu. Search reopens it.

use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;

use netrunner_client::card_face::Face;
use netrunner_client::cards::faction_label;
use netrunner_client::prose;
use netrunner_client::settings::format_name;
use netrunner_card_sync::ImageStatus;
use netrunner_core::card::CardId;
use netrunner_core::rules::Side;

use crate::card_images::CardImages;
use crate::core::{ClientCore, Notices, TokioRuntime};
use crate::downloads::Downloads;
use crate::models::browser::{Browser, Intent};
use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::theme::{size, Theme};
use crate::widgets::card_face::{spawn_face, FaceSize};
use crate::widgets::text_field::{TextField, TextFieldEvent};
use crate::widgets::{self, Pressed};

pub struct CardBrowserPlugin;

impl Plugin for CardBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::CardBrowser), spawn).add_systems(
            Update,
            (controls, search_field, refresh, progress).chain().run_if(in_state(AppScreen::CardBrowser)),
        );
    }
}

/// The rail's buttons.
#[derive(Component, Debug, Clone, PartialEq)]
enum Control {
    Intent(Intent),
    Download,
    Search,
    Back,
}

/// A thumb in the grid; pressing it opens the card.
#[derive(Component, Debug, Clone, Copy)]
pub struct FaceButton(pub CardId);

/// The filter chips, respawned on change.
#[derive(Component)]
struct Chips;
/// The grid of thumbs, respawned on change.
#[derive(Component)]
struct Grid;
/// The inspector column, respawned on change.
#[derive(Component)]
struct Inspector;
/// Where the search field, or the Search button, lives.
#[derive(Component)]
struct SearchSlot;
#[derive(Component)]
struct ProgressBar;
#[derive(Component)]
struct ProgressLabel;

/// The model as a resource. `models::browser` has no Bevy in it, so the
/// screen wraps it rather than the model deriving `Resource`.
#[derive(Resource)]
struct Model(Browser);

impl std::ops::Deref for Model {
    type Target = Browser;
    fn deref(&self) -> &Browser {
        &self.0
    }
}

impl std::ops::DerefMut for Model {
    fn deref_mut(&mut self) -> &mut Browser {
        &mut self.0
    }
}

/// Set by a change; `refresh` rebuilds and clears it.
#[derive(Resource, Default)]
struct Dirty(bool);

/// Set by the Search button; `search_field` opens the field and clears it.
#[derive(Resource, Default)]
struct SearchRequested(bool);

/// Whether a download was running last frame, so its finishing redraws
/// the chips (the cached count on the button).
#[derive(Resource, Default)]
struct WasDownloading(bool);

pub const MAX_QUERY_LEN: usize = 40;

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, images: Res<CardImages>, downloads: Res<Downloads>) {
    let browser = Browser::new(core.catalog.clone(), core.settings.format);
    commands.init_resource::<Dirty>();
    commands.init_resource::<SearchRequested>();
    commands.insert_resource(WasDownloading(downloads.is_running()));

    let chips = commands.spawn((Chips, Node { flex_direction: FlexDirection::Column, row_gap: px(8), ..default() })).id();
    commands.entity(chips).with_children(|parent| spawn_chips(parent, &theme, &core, &browser, &images));
    let grid = commands
        .spawn((
            Grid,
            bevy::ui_widgets::ScrollArea,
            Node {
                flex_grow: 1.0,
                min_width: px(0),
                height: percent(100),
                display: Display::Grid,
                grid_template_columns: vec![RepeatedGridTrack::px(GridTrackRepetition::AutoFill, FaceSize::Thumb.width())],
                grid_auto_rows: vec![GridTrack::px(FaceSize::Thumb.height())],
                row_gap: px(8),
                column_gap: px(8),
                align_content: AlignContent::Start,
                justify_content: JustifyContent::Center,
                overflow: Overflow::scroll_y(),
                padding: UiRect::all(px(4)),
                ..default()
            },
        ))
        .id();
    commands.entity(grid).with_children(|parent| spawn_grid(parent, &theme, &browser, &images));
    let inspector = commands
        .spawn((
            Inspector,
            bevy::ui_widgets::ScrollArea,
            BackgroundColor(theme.panel),
            BorderColor::all(theme.panel_border),
            Node {
                width: px(360),
                height: percent(100),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                padding: UiRect::all(px(12)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(8)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .id();
    commands.entity(inspector).with_children(|parent| spawn_inspector(parent, &theme, &core, &browser, &images));

    let search_slot = commands.spawn((SearchSlot, Node { flex_direction: FlexDirection::Column, ..default() })).id();
    commands.entity(search_slot).with_children(|parent| spawn_search_field(parent, &theme, ""));

    let rail = commands
        .spawn((Node { width: px(230), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: px(10), height: percent(100), overflow: Overflow::clip_y(), ..default() },))
        .id();
    commands.entity(rail).add_child(search_slot).add_child(chips).with_children(|parent| {
        parent.spawn((Node { width: percent(100), height: px(6), border_radius: BorderRadius::all(px(3)), overflow: Overflow::clip(), ..default() }, BackgroundColor(theme.button))).with_children(|bar| {
            bar.spawn((ProgressBar, Node { width: percent(0), height: percent(100), ..default() }, BackgroundColor(theme.accent)));
        });
        parent.spawn((ProgressLabel, widgets::dim(&theme, downloads.status_line())));
        parent.spawn(widgets::button(&theme, "Back", Val::Auto, Control::Back));
    });

    let columns = commands
        .spawn((Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(16), ..default() },))
        .add_child(rail)
        .add_child(grid)
        .add_child(inspector)
        .id();
    commands
        .spawn((screen_root(AppScreen::CardBrowser, theme.background), children![widgets::heading(&theme, AppScreen::CardBrowser.title())]))
        .add_child(columns);
    commands.insert_resource(Model(browser));
}

/// A chip is a button that reads as pressed while its filter is on.
/// Spawned then adjusted, because a bundle may not carry a `Node` twice
/// and the shared button helper already brings one.
fn chip<'a>(row: &'a mut ChildSpawnerCommands, theme: &Theme, text: impl Into<String>, on: bool, intent: Intent) -> EntityCommands<'a> {
    let colour = if on { theme.button_press } else { theme.button };
    let mut chip = row.spawn(widgets::button(theme, text, Val::Auto, Control::Intent(intent)));
    chip.insert((
        Node { flex_shrink: 0.0, padding: UiRect::axes(px(10), px(4)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(6)), ..default() },
        BackgroundColor(colour),
    ));
    chip
}

fn wrap_row() -> Node {
    Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, row_gap: px(4), column_gap: px(4), align_items: AlignItems::Center, ..default() }
}

fn spawn_chips(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, browser: &Browser, images: &CardImages) {
    let showing = browser.visible().len();
    parent.spawn(widgets::dim(theme, format!("{showing} of {} cards", browser.total())));
    // Side: the two backs as chips, the whole catalog as a third.
    parent.spawn(wrap_row()).with_children(|row| {
        for side in [Side::Corp, Side::Runner] {
            let on = browser.side == Some(side);
            let name = match side { Side::Corp => "Corp", Side::Runner => "Runner" };
            let mut button = chip(row, theme, name, on, Intent::Side(Some(side)));
            button.insert(BorderColor::all(if on { theme.side(side) } else { theme.panel_border }));
            if let Some(back) = images.back(side) {
                button.with_children(|b| {
                    b.spawn((ImageNode { image_mode: NodeImageMode::Stretch, ..ImageNode::new(back) }, Node { width: px(20), height: px(28), margin: UiRect::left(px(6)), ..default() }));
                });
            }
        }
        chip(row, theme, "Both", browser.side.is_none(), Intent::Side(None));
    });
    parent.spawn(wrap_row()).with_children(|row| {
        for faction in browser.factions() {
            let on = browser.faction == Some(faction);
            let mut button = chip(row, theme, faction_label(faction), on, Intent::Faction(Some(faction)));
            button.insert(BorderColor::all(if on { theme.faction(Some(faction)) } else { theme.panel_border }));
        }
    });
    parent.spawn(wrap_row()).with_children(|row| {
        for kind in browser.kinds() {
            chip(row, theme, kind, browser.kind == Some(kind), Intent::Kind(Some(kind)));
        }
    });
    parent.spawn(wrap_row()).with_children(|row| {
        chip(row, theme, format!("{} only", format_name(browser.format)), browser.format_only, Intent::ToggleFormat);
        chip(row, theme, "Clear", false, Intent::Clear);
    });
    // The download: what it would fetch, or why it will not.
    let codes: Vec<CardId> = core.catalog.iter().filter_map(|card| card.numeric_id).collect();
    let cached = core.images.cached_count(&codes);
    let label = if !core.settings.desktop.download_images {
        "Card images are off in Settings".to_string()
    } else if cached == codes.len() {
        format!("All {} images cached", codes.len())
    } else {
        format!("Download images ({cached} of {} cached)", codes.len())
    };
    parent.spawn(widgets::button(theme, label, percent(100), Control::Download));
}

fn spawn_grid(parent: &mut ChildSpawnerCommands, theme: &Theme, browser: &Browser, images: &CardImages) {
    for card in browser.visible() {
        let face = Face::of(card);
        let image = card.numeric_id.and_then(|code| images.face(code));
        let marker = (Button, FaceButton(card.numeric_id.unwrap_or(CardId(0))));
        let entity = spawn_face(parent, theme, &face, FaceSize::Thumb, image, marker);
        if browser.selected == card.numeric_id && card.numeric_id.is_some() {
            parent.commands().entity(entity).insert(Outline { width: px(3), offset: px(1), color: theme.accent });
        }
    }
}

fn spawn_inspector(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, browser: &Browser, images: &CardImages) {
    let Some(card) = browser.selected() else {
        parent.spawn(widgets::dim(theme, "Choose a card to open it here."));
        return;
    };
    let face = Face::of(card);
    let image = card.numeric_id.and_then(|code| images.face(code));
    parent.spawn((Node { justify_content: JustifyContent::Center, ..default() },)).with_children(|centre| {
        spawn_face(centre, theme, &face, FaceSize::Large, image, ());
    });
    parent.spawn((widgets::heading(theme, card.title.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    parent.spawn(widgets::dim(theme, face.type_line.clone()));
    parent.spawn((widgets::dim(theme, numbers_line(card)), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    if let (Some(set), Some(code)) = (&card.set_code, card.numeric_id) {
        parent.spawn(widgets::dim(theme, format!("Set {set} · #{:05}", code.0)));
    }
    if !card.is_playable {
        parent.spawn(widgets::notice(theme, "Not implemented in the engine yet", ()));
    }
    parent.spawn((Text::new(face.body_text(false)), theme.font(size::SMALL), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    if let Some(flavor) = &card.flavor {
        parent.spawn((widgets::dim(theme, format!("\u{201c}{flavor}\u{201d}")), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    let engine = prose::engine_reading(card, &core.registry);
    if !engine.is_empty() {
        parent.spawn(widgets::label(theme, "Engine reads it as"));
        for line in engine {
            parent.spawn((widgets::dim(theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        }
    }
    if let Some(code) = card.numeric_id {
        let status = match core.images.status(code) {
            ImageStatus::Cached(_) => "Picture cached".to_string(),
            ImageStatus::Missing => "No picture cached".to_string(),
            ImageStatus::Failed(reason) => format!("Picture failed: {reason}"),
        };
        parent.spawn(widgets::dim(theme, status));
    }
}

/// The numbers line the terminal's `card_modal` prints, so the two
/// clients say the same thing about a card.
fn numbers_line(card: &netrunner_core::dsl::CardDefinition) -> String {
    let mut numbers: Vec<String> = vec![format!("Cost {}", card.cost)];
    if let Some(strength) = card.strength {
        numbers.push(format!("Strength {strength}"));
    }
    if let Some(required) = card.advancement_requirement {
        numbers.push(format!("Advancement {required}"));
    }
    if let Some(points) = card.agenda_points {
        numbers.push(format!("{points} agenda point{}", if points == 1 { "" } else { "s" }));
    }
    if let Some(trash) = card.trash_cost {
        numbers.push(format!("Trash {trash}"));
    }
    if let Some(mu) = card.memory_cost {
        numbers.push(format!("{mu} MU"));
    }
    if let Some(influence) = card.influence_cost {
        numbers.push(format!("Influence {influence}"));
    }
    if card.unique {
        numbers.push("Unique".to_string());
    }
    numbers.join(" · ")
}

fn spawn_search_field(parent: &mut ChildSpawnerCommands, theme: &Theme, current: &str) {
    parent.spawn((
        TextField { text: current.to_string(), max_len: MAX_QUERY_LEN },
        Node { padding: UiRect::all(px(8)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(6)), ..default() },
        BorderColor::all(theme.accent),
        children![widgets::label(theme, format!("{current}|")), widgets::dim(theme, "Type to search · Esc clears, then closes")],
    ));
}

fn controls(
    mut pressed: MessageReader<Pressed>,
    marks: Query<&Control>,
    faces: Query<(&Interaction, &FaceButton), Changed<Interaction>>,
    mut browser: ResMut<Model>,
    mut dirty: ResMut<Dirty>,
    mut search: ResMut<SearchRequested>,
    core: Res<ClientCore>,
    runtime: Option<Res<TokioRuntime>>,
    mut downloads: ResMut<Downloads>,
    mut notices: ResMut<Notices>,
    mut navigate: MessageWriter<Navigate>,
) {
    for Pressed(entity) in pressed.read() {
        match marks.get(*entity) {
            Ok(Control::Back) => {
                navigate.write(Navigate(AppScreen::MainMenu));
            }
            Ok(Control::Search) => search.0 = true,
            Ok(Control::Intent(intent)) => {
                if browser.apply(intent.clone()) {
                    dirty.0 = true;
                }
            }
            Ok(Control::Download) => {
                if !core.settings.desktop.download_images {
                    notices.push("Turn on card images in Settings to download them");
                } else if let Some(runtime) = &runtime {
                    let codes: Vec<CardId> = core.catalog.iter().filter_map(|card| card.numeric_id).collect();
                    if !downloads.start(runtime, core.images.clone(), codes) {
                        notices.push("A download is already running");
                    }
                } else {
                    notices.push("No network runtime, so no download");
                }
            }
            Err(_) => {}
        }
    }
    for (interaction, FaceButton(code)) in &faces {
        if *interaction == Interaction::Pressed && browser.apply(Intent::Select(Some(*code))) {
            dirty.0 = true;
        }
    }
}

/// Keeps the model's query in step with what is typed, handles the
/// field's Enter and Escape, and opens the field on Search.
fn search_field(
    mut commands: Commands,
    mut fields: Query<(Entity, &mut TextField, Option<&TextFieldEvent>)>,
    slot: Query<Entity, With<SearchSlot>>,
    buttons: Query<(Entity, &Control)>,
    mut search: ResMut<SearchRequested>,
    mut browser: ResMut<Model>,
    mut dirty: ResMut<Dirty>,
    theme: Res<Theme>,
) {
    let Ok(slot) = slot.single() else { return };
    let mut open = false;
    for (entity, mut field, event) in &mut fields {
        open = true;
        match event {
            Some(TextFieldEvent::Committed(_)) => {
                commands.entity(entity).remove::<TextFieldEvent>();
            }
            Some(TextFieldEvent::Cancelled) if !field.text.is_empty() => {
                field.text.clear();
                commands.entity(entity).remove::<TextFieldEvent>();
            }
            Some(TextFieldEvent::Cancelled) => {
                commands.entity(entity).despawn();
                commands.entity(slot).with_children(|parent| {
                    parent.spawn(widgets::button(&theme, "Search", percent(100), Control::Search));
                });
                open = false;
            }
            None => {}
        }
        if open && field.text != browser.query && browser.apply(Intent::Query(field.text.clone())) {
            dirty.0 = true;
        }
    }
    if std::mem::take(&mut search.0) && !open {
        for (entity, control) in &buttons {
            if *control == Control::Search {
                commands.entity(entity).despawn();
            }
        }
        let current = browser.query.clone();
        commands.entity(slot).with_children(|parent| spawn_search_field(parent, &theme, &current));
    }
}

fn refresh(
    mut commands: Commands,
    mut dirty: ResMut<Dirty>,
    mut was: ResMut<WasDownloading>,
    downloads: Res<Downloads>,
    chips: Query<Entity, With<Chips>>,
    grid: Query<Entity, With<Grid>>,
    inspector: Query<Entity, With<Inspector>>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    browser: Res<Model>,
    images: Res<CardImages>,
) {
    let running = downloads.is_running();
    if was.0 != running {
        was.0 = running;
        dirty.0 = true;
    }
    if !dirty.0 {
        return;
    }
    dirty.0 = false;
    let browser = &browser.0;
    let (Ok(chips), Ok(grid), Ok(inspector)) = (chips.single(), grid.single(), inspector.single()) else { return };
    commands.entity(chips).despawn_children().with_children(|parent| spawn_chips(parent, &theme, &core, browser, &images));
    commands.entity(grid).despawn_children().with_children(|parent| spawn_grid(parent, &theme, browser, &images));
    commands.entity(inspector).despawn_children().with_children(|parent| spawn_inspector(parent, &theme, &core, browser, &images));
}

/// The bar and its line, every frame — they are the only thing on the
/// screen that moves without a press.
fn progress(downloads: Res<Downloads>, mut bar: Query<&mut Node, With<ProgressBar>>, mut label: Query<&mut Text, With<ProgressLabel>>) {
    let fraction = match downloads.progress() {
        Some((done, total)) if total > 0 => done as f32 / total as f32,
        Some(_) => 0.0,
        None => 0.0,
    };
    for mut node in &mut bar {
        node.width = percent(fraction * 100.0);
    }
    let line = downloads.status_line();
    for mut text in &mut label {
        if text.0 != line {
            text.0 = line.clone();
        }
    }
}
