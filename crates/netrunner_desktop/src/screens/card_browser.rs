//! The card browser: every printing as a face, filters along the top,
//! the chosen card open on the right with its printed text and how the
//! engine reads it.
//!
//! A toolbar under the heading, then two columns, then a status row.
//! The toolbar holds one drop-down per filter — side, faction, type,
//! set, format — the search field, Clear and Back; the first cut had a
//! rail of chips down the left, which was a quarter of the width for
//! five filters, and the drop-downs give that width to the grid and the
//! inspector. The status row holds the image download beside its own
//! progress bar, the count, and the keys. The grid is a scrolling CSS grid of thumb faces, each a
//! button, with a scrollbar beside it. The inspector is the large face,
//! the numbers line the terminal's `card_modal` prints, the faction and
//! set with their marks, which formats allow the card, the flavour, the
//! engine's reading (`prose::engine_reading`), whether the engine plays
//! it, and whether its picture is cached.
//!
//! **What is redrawn when.** A filter chosen from a drop-down respawns
//! the drop-downs (their choices depend on the side), the grid and the
//! inspector. A keystroke in the search respawns the grid and the
//! inspector but not the drop-downs, so a list left open stays open
//! while the grid narrows under it. A selection change — a click, an
//! arrow key — moves the outline from one thumb to another and respawns
//! only the inspector, because respawning two hundred faces for a key
//! held down is the difference between a browser and a slideshow. The
//! search field, the count and the progress row are never respawned:
//! one holds what is being typed and the others are updated in place.
//!
//! **Keys.** The arrows move the selection by one, or by a row (the
//! column count comes off the grid's laid-out width); Page Up and Page
//! Down by three rows; Home and End to the ends. The grid scrolls to
//! keep the selection in view after a key, and never after a click.
//! While the search field is open it captures Escape: a first press
//! clears the text, a second closes the field, and only then does
//! Escape lead back to the menu. Search reopens it. An open drop-down
//! takes an Escape before the field does.

use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;
use bevy::prelude::*;

use netrunner_client::card_face::Face;
use netrunner_client::cards::{faction_label, legal_formats, set_name};
use netrunner_client::prose;
use netrunner_client::settings::{format_label, FORMATS};
use netrunner_card_sync::ImageStatus;
use netrunner_core::card::CardId;
use netrunner_core::rules::Side;

use crate::card_images::CardImages;
use crate::core::{ClientCore, Notices, TokioRuntime};
use crate::downloads::Downloads;
use crate::icon_font::IconFontReady;
use crate::models::browser::{Browser, Intent};
use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::theme::{size, Theme};
use crate::widgets::card_face::{spawn_face, FaceSize};
use crate::widgets::dropdown::{spawn_dropdown, Choice, DropdownChanged};
use crate::widgets::text_field::{TextField, TextFieldEvent};
use crate::widgets::{self, Pressed};

pub struct CardBrowserPlugin;

impl Plugin for CardBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::CardBrowser), spawn).add_systems(
            Update,
            (controls, keyboard, search_field, refresh, progress).chain().run_if(in_state(AppScreen::CardBrowser)),
        );
    }
}

/// The toolbar's buttons.
#[derive(Component, Debug, Clone, PartialEq)]
enum Control {
    Clear,
    Download,
    Search,
    Back,
}

/// Which filter a drop-down is; on its root, so a `DropdownChanged`
/// resolves to an intent.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Side,
    Faction,
    Kind,
    Set,
    Format,
}

/// A thumb in the grid; pressing it opens the card.
#[derive(Component, Debug, Clone, Copy)]
pub struct FaceButton(pub CardId);

/// The filter drop-downs, respawned when a filter changes.
#[derive(Component)]
struct Filters;
/// The download button, respawned when the download state changes.
#[derive(Component)]
struct DownloadSlot;
/// "N of M cards", updated in place.
#[derive(Component)]
struct Count;
/// The grid of thumbs, respawned on a filter change.
#[derive(Component)]
struct Grid;
/// The column the grid scrolls inside. The grid itself is not the
/// scroll container: taffy reports a grid's scrollable content as one
/// cell (each item's contribution is measured from its own grid area,
/// not the container), so a scrolling grid of 271 thumbs had a scroll
/// range of nothing — the browser's first cut, which did not scroll. A
/// flex column with the grid as its one child measures its content as
/// the grid's full height, and scrolls.
#[derive(Component)]
struct GridScroll;
/// The inspector column, respawned on any change.
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

/// What changed since the last redraw; `refresh` acts on it and clears
/// it. Any of them redraws the inspector.
#[derive(Resource, Default)]
struct Dirty {
    /// A filter: the drop-downs are respawned.
    toolbar: bool,
    /// The download started or finished: its button is respawned.
    download: bool,
    /// What is visible: the grid is respawned.
    grid: bool,
    /// Which card is open: the outline moves.
    selection: bool,
    /// The selection moved by key, so the grid scrolls to show it.
    scroll: bool,
}

impl Dirty {
    fn filter_changed(&mut self) {
        self.toolbar = true;
        self.grid = true;
    }
}

/// Set by the Search button; `search_field` opens the field and clears it.
#[derive(Resource, Default)]
struct SearchRequested(bool);

/// Whether a download was running last frame, so its finishing redraws
/// the toolbar (the cached count on the button).
#[derive(Resource, Default)]
struct WasDownloading(bool);

pub const MAX_QUERY_LEN: usize = 40;

/// The grid's cell pitch and inset, which the keyboard's row count and
/// the scroll-into-view read back.
const GRID_GAP: f32 = 8.0;
const GRID_PAD: f32 = 4.0;

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, images: Res<CardImages>, downloads: Res<Downloads>) {
    let browser = Browser::new(core.catalog.clone());
    commands.init_resource::<Dirty>();
    commands.init_resource::<SearchRequested>();
    commands.insert_resource(WasDownloading(downloads.is_running()));

    let filters = commands.spawn((Filters, wrap_row())).id();
    commands.entity(filters).with_children(|parent| spawn_filters(parent, &theme, &browser));
    let search_slot = commands.spawn((SearchSlot, Node { flex_direction: FlexDirection::Row, ..default() })).id();
    commands.entity(search_slot).with_children(|parent| spawn_search_field(parent, &theme, ""));
    let toolbar = commands
        .spawn((Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, column_gap: px(8), row_gap: px(6), ..default() },))
        .add_child(filters)
        .add_child(search_slot)
        .with_children(|parent| {
            // Pushed to the right end of the row. Spawned then adjusted:
            // a bundle may not carry a `Node` twice and the button brings one.
            let mut back = parent.spawn(widgets::button(&theme, "Back", Val::Auto, Control::Back));
            back.entry::<Node>().and_modify(|mut node| node.margin = UiRect::left(Val::Auto));
        })
        .id();

    let grid = commands
        .spawn((
            Grid,
            Node {
                width: percent(100),
                display: Display::Grid,
                grid_template_columns: vec![RepeatedGridTrack::px(GridTrackRepetition::AutoFill, FaceSize::Thumb.width())],
                grid_auto_rows: vec![GridTrack::px(FaceSize::Thumb.height())],
                row_gap: px(GRID_GAP),
                column_gap: px(GRID_GAP),
                align_content: AlignContent::Start,
                justify_content: JustifyContent::Center,
                padding: UiRect::all(px(GRID_PAD)),
                ..default()
            },
        ))
        .id();
    commands.entity(grid).with_children(|parent| spawn_grid(parent, &theme, &browser, &images));
    let grid_scroll = commands
        .spawn((
            GridScroll,
            bevy::ui_widgets::ScrollArea,
            Node { flex_grow: 1.0, min_width: px(0), height: percent(100), flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), ..default() },
        ))
        .add_child(grid)
        .id();
    let inspector = commands
        .spawn((
            Inspector,
            bevy::ui_widgets::ScrollArea,
            BackgroundColor(theme.panel),
            BorderColor::all(theme.panel_border),
            Node {
                width: px(FaceSize::Large.width() + 2.0 * 12.0 + 2.0),
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
    let columns = commands
        .spawn((Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(8), ..default() },))
        .add_child(grid_scroll)
        .with_children(|parent| {
            parent.spawn(widgets::scrollbar(&theme, grid_scroll));
        })
        .add_child(inspector)
        .with_children(|parent| {
            parent.spawn(widgets::scrollbar(&theme, inspector));
        })
        .id();

    let download_slot = commands.spawn((DownloadSlot, Node { flex_shrink: 0.0, ..default() })).id();
    commands.entity(download_slot).with_children(|parent| spawn_download(parent, &theme, &core));
    let status = commands
        .spawn((Node { width: percent(100), flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(12), ..default() },))
        .add_child(download_slot)
        .with_children(|parent| {
            parent.spawn((Node { width: px(160), height: px(6), flex_shrink: 0.0, border_radius: BorderRadius::all(px(3)), overflow: Overflow::clip(), ..default() }, BackgroundColor(theme.button))).with_children(|bar| {
                bar.spawn((ProgressBar, Node { width: percent(0), height: percent(100), ..default() }, BackgroundColor(theme.accent)));
            });
            parent.spawn((ProgressLabel, widgets::dim(&theme, downloads.status_line())));
            parent.spawn((Count, widgets::dim(&theme, count_line(&browser))));
            parent.spawn((widgets::dim(&theme, "Type to search · Esc clears, then closes · arrows, Page Up/Down, Home and End move"), Node { margin: UiRect::left(Val::Auto), ..default() }));
        })
        .id();

    commands
        .spawn((screen_root(AppScreen::CardBrowser, theme.background), children![widgets::heading(&theme, AppScreen::CardBrowser.title())]))
        .add_child(toolbar)
        .add_child(columns)
        .add_child(status);
    commands.insert_resource(Model(browser));
}

fn count_line(browser: &Browser) -> String {
    format!("{} of {} cards", browser.visible().len(), browser.total())
}

fn wrap_row() -> Node {
    Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, row_gap: px(6), column_gap: px(8), align_items: AlignItems::Center, ..default() }
}

/// A filter's choices, the intent each stands for, and which is current.
/// One function for both the drop-down's list and the change it reports,
/// so the two cannot drift.
fn entries(browser: &Browser, theme: &Theme, filter: Filter) -> (Vec<Choice>, Vec<Intent>, usize) {
    let mut choices = Vec::new();
    let mut intents = Vec::new();
    let mut current = 0;
    let mut push = |choice: Choice, intent: Intent, is_current: bool| {
        if is_current {
            current = intents.len();
        }
        choices.push(choice);
        intents.push(intent);
    };
    match filter {
        Filter::Side => {
            push(Choice::plain("Both"), Intent::Side(None), browser.side.is_none());
            for side in [Side::Corp, Side::Runner] {
                let name = match side { Side::Corp => "Corp", Side::Runner => "Runner" };
                push(Choice::plain(name), Intent::Side(Some(side)), browser.side == Some(side));
            }
        }
        Filter::Faction => {
            push(Choice::plain("All"), Intent::Faction(None), browser.faction.is_none());
            for faction in browser.factions() {
                let icon = theme.faction_icon(faction, size::SMALL).map(|(glyph, _)| glyph);
                push(Choice::with_icon(faction_label(faction), icon, theme.faction(Some(faction))), Intent::Faction(Some(faction)), browser.faction == Some(faction));
            }
        }
        Filter::Kind => {
            push(Choice::plain("All"), Intent::Kind(None), browser.kind.is_none());
            for kind in browser.kinds() {
                push(Choice::plain(kind), Intent::Kind(Some(kind)), browser.kind == Some(kind));
            }
        }
        Filter::Set => {
            push(Choice::plain("All"), Intent::Set(None), browser.set.is_none());
            for set in browser.sets() {
                let icon = theme.set_icon(&set, size::SMALL).map(|(glyph, _)| glyph);
                push(Choice::with_icon(set_name(&set), icon, theme.text), Intent::Set(Some(set.clone())), browser.set.as_deref() == Some(set.as_str()));
            }
        }
        Filter::Format => {
            push(Choice::plain("Any"), Intent::Format(None), browser.format.is_none());
            for format in FORMATS {
                push(Choice::plain(format_label(format)), Intent::Format(Some(format)), browser.format == Some(format));
            }
        }
    }
    (choices, intents, current)
}

fn spawn_filters(parent: &mut ChildSpawnerCommands, theme: &Theme, browser: &Browser) {
    for (filter, label) in [(Filter::Side, "Side"), (Filter::Faction, "Faction"), (Filter::Kind, "Type"), (Filter::Set, "Set"), (Filter::Format, "Format")] {
        let (choices, _, current) = entries(browser, theme, filter);
        spawn_dropdown(parent, theme, label, choices, current, filter);
    }
    parent.spawn(widgets::button(theme, "Clear", Val::Auto, Control::Clear));
}

/// The download button: what it would fetch, or why it will not.
fn spawn_download(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore) {
    let codes: Vec<CardId> = core.catalog.iter().filter_map(|card| card.numeric_id).collect();
    let cached = core.images.cached_count(&codes);
    let label = if !core.settings.desktop.download_images {
        "Card images are off in Settings".to_string()
    } else if cached == codes.len() {
        format!("All {} images cached", codes.len())
    } else {
        format!("Download images ({cached} of {} cached)", codes.len())
    };
    parent.spawn(widgets::button(theme, label, Val::Auto, Control::Download));
}

fn spawn_grid(parent: &mut ChildSpawnerCommands, theme: &Theme, browser: &Browser, images: &CardImages) {
    for card in browser.visible() {
        let face = Face::of(card);
        let image = card.numeric_id.and_then(|code| images.face(code));
        let marker = (Button, FaceButton(card.numeric_id.unwrap_or(CardId(0))));
        let entity = spawn_face(parent, theme, &face, FaceSize::Thumb, image, marker);
        if browser.selected == card.numeric_id && card.numeric_id.is_some() {
            parent.commands().entity(entity).insert(outline(theme));
        }
    }
}

fn outline(theme: &Theme) -> Outline {
    Outline { width: px(3), offset: px(1), color: theme.accent }
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
    // The faction with its mark, the set with its, the code.
    if let Some(faction) = card.faction {
        parent.spawn((Text::new(""), theme.font(size::SMALL), TextColor(theme.text_dim))).with_children(|spans| {
            if let Some((mark, font)) = theme.faction_icon(faction, size::SMALL) {
                spans.spawn((TextSpan::new(format!("{mark} ")), font, TextColor(theme.faction(Some(faction)))));
            }
            spans.spawn((TextSpan::new(faction_label(faction)), theme.font(size::SMALL), TextColor(theme.text_dim)));
        });
    }
    if let (Some(set), Some(code)) = (card.set_code.as_deref(), card.numeric_id) {
        parent.spawn((Text::new(""), theme.font(size::SMALL), TextColor(theme.text_dim))).with_children(|spans| {
            if let Some((mark, font)) = theme.set_icon(set, size::SMALL) {
                spans.spawn((TextSpan::new(format!("{mark} ")), font, TextColor(theme.text_dim)));
            }
            spans.spawn((TextSpan::new(format!("{} · #{:05}", set_name(set), code.0)), theme.font(size::SMALL), TextColor(theme.text_dim)));
        });
    }
    parent.spawn((widgets::dim(theme, legality_line(card)), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
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
            // The prose joins a clause to its reading with `→`, which the
            // terminal draws and Noto Sans does not have (no arrows block,
            // as `Symbol::glyph` records); `›` is in the font and reads
            // the same way across.
            parent.spawn((widgets::dim(theme, line.replace('→', "›")), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
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

/// Which formats allow the card, by the tables `cards::legal_in` reads.
fn legality_line(card: &netrunner_core::dsl::CardDefinition) -> String {
    let formats = legal_formats(card);
    if formats.is_empty() {
        return "Legal in no format".to_string();
    }
    format!("Legal in {}", formats.into_iter().map(format_label).collect::<Vec<_>>().join(" · "))
}

fn spawn_search_field(parent: &mut ChildSpawnerCommands, theme: &Theme, current: &str) {
    parent.spawn((
        TextField { text: current.to_string(), max_len: MAX_QUERY_LEN },
        Node { width: px(220), padding: UiRect::axes(px(10), px(6)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(6)), ..default() },
        BorderColor::all(theme.accent),
        children![(Text::new(format!("{current}|")), theme.font(size::SMALL), TextColor(theme.text))],
    ));
}

fn controls(
    mut pressed: MessageReader<Pressed>,
    mut chosen: MessageReader<DropdownChanged>,
    mut font_ready: MessageReader<IconFontReady>,
    marks: Query<&Control>,
    filters: Query<&Filter>,
    faces: Query<(&Interaction, &FaceButton), Changed<Interaction>>,
    mut browser: ResMut<Model>,
    mut dirty: ResMut<Dirty>,
    mut search: ResMut<SearchRequested>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    runtime: Option<Res<TokioRuntime>>,
    mut downloads: ResMut<Downloads>,
    mut notices: ResMut<Notices>,
    mut navigate: MessageWriter<Navigate>,
) {
    if font_ready.read().next().is_some() {
        dirty.filter_changed();
    }
    for Pressed(entity) in pressed.read() {
        match marks.get(*entity) {
            Ok(Control::Back) => {
                navigate.write(Navigate(AppScreen::MainMenu));
            }
            Ok(Control::Search) => search.0 = true,
            Ok(Control::Clear) => {
                if browser.apply(Intent::Clear) {
                    dirty.filter_changed();
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
    for DropdownChanged { dropdown, index } in chosen.read() {
        let Ok(filter) = filters.get(*dropdown) else { continue };
        let (_, intents, _) = entries(&browser, &theme, *filter);
        if let Some(intent) = intents.get(*index)
            && browser.apply(intent.clone())
        {
            dirty.filter_changed();
        }
    }
    for (interaction, FaceButton(code)) in &faces {
        if *interaction == Interaction::Pressed && browser.apply(Intent::Select(Some(*code))) {
            dirty.selection = true;
        }
    }
}

/// How many thumbs fit across the grid as laid out; one before the
/// first layout and in a headless test.
fn columns(grid: &ComputedNode) -> usize {
    let width = grid.size().x * grid.inverse_scale_factor - 2.0 * GRID_PAD;
    (((width + GRID_GAP) / (FaceSize::Thumb.width() + GRID_GAP)).floor() as usize).max(1)
}

/// The arrows and their friends move the selection. Read as key
/// presses rather than `just_pressed`, so a held key repeats the way
/// the window manager repeats it.
fn keyboard(mut keys: MessageReader<KeyboardInput>, grid: Query<&ComputedNode, With<Grid>>, mut browser: ResMut<Model>, mut dirty: ResMut<Dirty>) {
    let columns = grid.single().map(columns).unwrap_or(1) as isize;
    for key in keys.read().filter(|key| key.state == ButtonState::Pressed) {
        let step = match key.key_code {
            KeyCode::ArrowLeft => -1,
            KeyCode::ArrowRight => 1,
            KeyCode::ArrowUp => -columns,
            KeyCode::ArrowDown => columns,
            KeyCode::PageUp => -3 * columns,
            KeyCode::PageDown => 3 * columns,
            KeyCode::Home => isize::MIN / 2,
            KeyCode::End => isize::MAX / 2,
            _ => continue,
        };
        if browser.apply(Intent::Step(step)) {
            dirty.selection = true;
            dirty.scroll = true;
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
                    parent.spawn(widgets::button(&theme, "Search", Val::Auto, Control::Search));
                });
                open = false;
            }
            None => {}
        }
        if open && field.text != browser.query && browser.apply(Intent::Query(field.text.clone())) {
            dirty.grid = true;
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
    filters: Query<Entity, With<Filters>>,
    download_slot: Query<Entity, With<DownloadSlot>>,
    mut count: Query<&mut Text, With<Count>>,
    grid: Query<(Entity, &ComputedNode), With<Grid>>,
    mut grid_scroll: Query<(&ComputedNode, &mut ScrollPosition), With<GridScroll>>,
    inspector: Query<Entity, With<Inspector>>,
    thumbs: Query<(Entity, &FaceButton, Has<Outline>)>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    browser: Res<Model>,
    images: Res<CardImages>,
) {
    let running = downloads.is_running();
    if was.0 != running {
        was.0 = running;
        dirty.download = true;
    }
    if !(dirty.toolbar || dirty.download || dirty.grid || dirty.selection) {
        return;
    }
    let Dirty { toolbar, download, grid: regrid, scroll, .. } = std::mem::take(&mut *dirty);
    let browser = &browser.0;
    let (Ok(filters), Ok(download_slot), Ok((grid, grid_node)), Ok((viewport_node, mut scroll_position)), Ok(inspector)) =
        (filters.single(), download_slot.single(), grid.single(), grid_scroll.single_mut(), inspector.single())
    else {
        return;
    };
    if toolbar {
        commands.entity(filters).despawn_children().with_children(|parent| spawn_filters(parent, &theme, browser));
    }
    if download {
        commands.entity(download_slot).despawn_children().with_children(|parent| spawn_download(parent, &theme, &core));
    }
    if regrid {
        commands.entity(grid).despawn_children().with_children(|parent| spawn_grid(parent, &theme, browser, &images));
        for mut text in &mut count {
            text.0 = count_line(browser);
        }
    } else {
        for (entity, FaceButton(code), outlined) in &thumbs {
            let selected = browser.selected == Some(*code);
            if selected && !outlined {
                commands.entity(entity).insert(outline(&theme));
            } else if !selected && outlined {
                commands.entity(entity).remove::<Outline>();
            }
        }
    }
    commands.entity(inspector).despawn_children().with_children(|parent| spawn_inspector(parent, &theme, &core, browser, &images));
    if scroll
        && let Some(index) = browser.selected_index()
    {
        let viewport = viewport_node.size().y * viewport_node.inverse_scale_factor;
        if viewport > 0.0 {
            let row = (index / columns(grid_node)) as f32;
            let top = GRID_PAD + row * (FaceSize::Thumb.height() + GRID_GAP);
            let bottom = top + FaceSize::Thumb.height() + GRID_PAD;
            if top < scroll_position.y {
                scroll_position.y = top;
            } else if bottom > scroll_position.y + viewport {
                scroll_position.y = bottom - viewport;
            }
        }
    }
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
