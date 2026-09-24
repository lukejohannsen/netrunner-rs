//! The Decks screen: every deck as a tile — the person's own first, then
//! each kind of built-in deck — with its identity's card, its size, the
//! style a bot plays it in, and whether it can start a game.
//!
//! **A tile says where the deck stands in words and a colour** (the words
//! are what a colour-blind reader reads; the colour is for a glance):
//! "Startup-legal", "Not Startup-legal" with the validator's reason, or
//! "Can't be played" with its reason, and the other formats it is legal
//! in. The format is the one Settings names, because it is the one a game
//! is started in (`new_game`).
//!
//! **The person's deck is edited; a built-in deck is read and copied.**
//! A saved tile carries Edit, Copy, Export and Delete; a built-in one
//! carries View, Copy to edit and Export — the published lists are the
//! place to build from, which is what Copy is for. New deck asks the side
//! and then the identity, as the identity's card, and opens the editor on
//! the empty deck; the name is changed there.
//!
//! **Import and export go through the clipboard, and a file can be
//! dropped.** Import reads the clipboard as a decklist — this project's
//! deck file, or the text list NetrunnerDB, jinteki.net or a hand-typed
//! list writes (`deck_builder::import`) — and Export puts the deck on it
//! in the same text shape. A `.txt` or `.json` file dropped on the window
//! is imported the same way, which is the answer to a file picker without
//! a native dialog. Whatever an import could not read is named, and the
//! deck is saved anyway.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::window::FileDragAndDrop;

use netrunner_client::card_face::Face;
use netrunner_client::deck_builder::{self, CardBook, Standing};
use netrunner_client::settings::format_label;
use netrunner_core::dsl::{CardDefinition, CardId};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

use crate::card_images::CardImages;
use crate::core::ClientCore;
use crate::models::decks::{Intent, Outcome, Shelf, ShelfRow};
use crate::nav::{screen_root, Captures, InputCaptured, Navigate};
use crate::screens::deck_editor::EditDeck;
use crate::screens::AppScreen;
use crate::theme::{size, Theme};
use crate::widgets::card_face::{spawn_face, FaceSize};
use crate::widgets::{self, ButtonKind, Pressed};

pub struct DecksPlugin;

impl Plugin for DecksPlugin {
    fn build(&self, app: &mut App) {
        // Registered here as well as by the window plugin, which a
        // headless test does not run; `add_message` is idempotent.
        app.add_message::<FileDragAndDrop>()
            .add_systems(OnEnter(AppScreen::Decks), spawn)
            .add_systems(Update, escape_closes_the_picker.in_set(Captures).run_if(in_state(AppScreen::Decks)))
            .add_systems(Update, (controls, dropped_files, refresh).chain().run_if(in_state(AppScreen::Decks)));
    }
}

/// The toolbar's buttons.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    New,
    Import,
    Side(Option<Side>),
    Back,
}

/// A tile's button: which row, and what it does to it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileButton {
    pub row: usize,
    pub action: TileAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileAction {
    /// Edit a saved deck; view a built-in one.
    Open,
    Copy,
    Export,
    Delete,
}

/// A button in the pop-up over the shelf.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub enum PopupButton {
    Side(Side),
    Identity(CardId),
    Delete(bool),
    Cancel,
}

/// What the pop-up is asking, if anything.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub enum Popup {
    #[default]
    None,
    /// New deck: which side?
    Side,
    /// New deck: which identity?
    Identity(Side),
}

#[derive(Component)]
struct ShelfBody;
#[derive(Component)]
struct Toolbar;
#[derive(Component)]
struct NoticeLine;
#[derive(Component)]
struct PopupLayer;
/// The wash under the pop-up; a press on it is a click away.
#[derive(Component)]
struct Wash;

#[derive(Resource)]
pub struct Model(pub Shelf);

#[derive(Resource, Default)]
struct Dirty {
    shelf: bool,
    popup: bool,
}

fn book(core: &ClientCore) -> CardBook<'_> {
    CardBook::new(&core.registry, &core.catalog)
}

fn format_of(core: &ClientCore) -> NsgFormat {
    core.settings.format.unwrap_or(NsgFormat::Startup)
}

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, images: Res<CardImages>, kept: Option<Res<Model>>) {
    // The notice an editor's Copy or an import left survives the trip
    // back; the rows are re-read, since the editor wrote to them.
    let notice = kept.and_then(|kept| kept.0.notice.clone());
    let mut shelf = Shelf::open(core.decks_dir.clone(), book(&core), format_of(&core));
    if notice.is_some() {
        shelf.notice = notice;
    }
    commands.insert_resource(Popup::None);
    commands.init_resource::<Dirty>();

    let toolbar = commands.spawn((Toolbar, toolbar_node())).id();
    commands.entity(toolbar).with_children(|parent| spawn_toolbar(parent, &theme, &shelf));
    let body = commands.spawn((ShelfBody, Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(26), padding: UiRect::all(px(4)), ..default() })).id();
    commands.entity(body).with_children(|parent| spawn_shelf(parent, &theme, &core, &shelf, &images));
    let scroll = commands
        .spawn((bevy::ui_widgets::ScrollArea, Node { flex_grow: 1.0, min_width: px(0), height: percent(100), flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), ..default() }))
        .add_child(body)
        .id();
    let columns = commands
        .spawn(Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(8), ..default() })
        .add_child(scroll)
        .with_children(|parent| {
            parent.spawn(widgets::scrollbar(&theme, scroll));
        })
        .id();
    let format = format_label(shelf.format());
    let column = commands
        .spawn(Node { width: percent(100), max_width: px(1500), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column, row_gap: px(12), ..default() })
        .with_children(|parent| {
            parent.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Baseline, column_gap: px(16), ..default() }).with_children(|row| {
                row.spawn(widgets::title(&theme, AppScreen::Decks.title()));
                row.spawn(widgets::dim(&theme, format!("Checked against {format}, the format Settings names. To import a file, drop a .txt or .json decklist onto the window.")));
            });
        })
        .add_child(toolbar)
        .with_children(|parent| {
            parent.spawn(widgets::notice(&theme, shelf.notice.clone().unwrap_or_default(), NoticeLine));
        })
        .add_child(columns)
        .id();
    let popup = commands.spawn((PopupLayer, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), display: Display::None, ..default() })).id();
    commands.spawn(screen_root(AppScreen::Decks, &theme)).add_child(column).add_child(popup);
    commands.insert_resource(Model(shelf));
}

fn toolbar_node() -> Node {
    Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, column_gap: px(10), row_gap: px(8), ..default() }
}

fn spawn_toolbar(parent: &mut ChildSpawnerCommands, theme: &Theme, shelf: &Shelf) {
    parent.spawn(widgets::styled_button(theme, ButtonKind::Primary, "New deck", Val::Auto, Control::New));
    parent.spawn(widgets::button(theme, "Import from clipboard", Val::Auto, Control::Import));
    parent.spawn((Node { width: px(18), ..default() },));
    for (label, side) in [("All", None), ("Corp", Some(Side::Corp)), ("Runner", Some(Side::Runner))] {
        let kind = if shelf.side == side { ButtonKind::Secondary } else { ButtonKind::Quiet };
        let mut button = parent.spawn(widgets::styled_button(theme, kind, label, Val::Auto, Control::Side(side)));
        if shelf.side == side {
            button.insert(BorderColor::all(theme.accent));
        }
    }
    let mut back = parent.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, Control::Back));
    back.entry::<Node>().and_modify(|mut node| node.margin = UiRect::left(Val::Auto));
}

fn spawn_shelf(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, shelf: &Shelf, images: &CardImages) {
    for section in shelf.sections() {
        parent.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(10), ..default() }).with_children(|block| {
            block.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Baseline, column_gap: px(12), ..default() }).with_children(|head| {
                head.spawn(widgets::overline(theme, format!("{} · {}", section.title, section.rows.len())));
                head.spawn(widgets::dim(theme, section.blurb));
            });
            if section.rows.is_empty() {
                block.spawn(widgets::label(theme, "No decks of your own yet. Start one with New deck, copy a built-in deck below, or import a list."));
                return;
            }
            block.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(14), row_gap: px(14), ..default() }).with_children(|tiles| {
                for index in &section.rows {
                    spawn_tile(tiles, theme, core, &shelf.rows[*index], *index, shelf.format(), images);
                }
            });
        });
    }
}

/// The width of a tile: two across on a small window, four on a wide one.
const TILE_WIDTH: f32 = 460.0;
const TILE_FACE: u16 = 104;

fn spawn_tile(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, row: &ShelfRow, index: usize, format: NsgFormat, images: &CardImages) {
    let identity = core.registry.get(&row.deck.identity).or_else(|| core.catalog.iter().find(|card| card.id == row.deck.identity));
    let faction = theme.faction(identity.and_then(|card| card.faction));
    parent
        .spawn((
            Node {
                width: px(TILE_WIDTH),
                flex_direction: FlexDirection::Row,
                column_gap: px(14),
                padding: UiRect::all(px(12)),
                border: UiRect { left: px(4), right: px(1), top: px(1), bottom: px(1) },
                border_radius: BorderRadius::all(px(14)),
                ..default()
            },
            BackgroundColor(theme.glass),
            BorderColor { left: faction, right: theme.glass_border, top: theme.glass_border, bottom: theme.glass_border },
        ))
        .with_children(|tile| {
            match identity {
                Some(card) => {
                    let image = card.numeric_id.and_then(|code| images.face(code, FaceSize::Board(TILE_FACE)));
                    spawn_face(tile, theme, &Face::of(card), FaceSize::Board(TILE_FACE), image, ());
                }
                None => {
                    tile.spawn((Node { width: px(TILE_FACE as f32), height: px(TILE_FACE as f32 * 1.4), flex_shrink: 0.0, ..default() }, BackgroundColor(theme.panel)));
                }
            }
            tile.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0), min_width: px(0), row_gap: px(4), ..default() }).with_children(|words| {
                words.spawn((Text::new(row.deck.name.clone()), theme.font(size::BODY), TextColor(theme.text)));
                words.spawn(widgets::dim(theme, row.identity.clone()));
                let style = row.deck.style.as_deref().unwrap_or("balanced");
                words.spawn(widgets::dim(theme, format!("{:?} · {} cards · a bot plays it {style}", row.deck.side, row.deck.size())));
                spawn_standing(words, theme, &row.status.standing, format);
                let others: Vec<&str> = row.status.legal_in.iter().filter(|other| **other != format).map(|other| format_label(*other)).collect();
                if !others.is_empty() {
                    words.spawn(widgets::dim(theme, format!("Also legal in {}", others.join(", "))));
                }
                words.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(6), row_gap: px(6), margin: UiRect::top(px(6)), ..default() }).with_children(|buttons| {
                    if row.saved {
                        buttons.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Edit", TileButton { row: index, action: TileAction::Open }));
                        buttons.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Copy", TileButton { row: index, action: TileAction::Copy }));
                        buttons.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Export", TileButton { row: index, action: TileAction::Export }));
                        buttons.spawn(widgets::small_button(theme, ButtonKind::Quiet, "Delete", TileButton { row: index, action: TileAction::Delete }));
                    } else {
                        buttons.spawn(widgets::small_button(theme, ButtonKind::Secondary, "View", TileButton { row: index, action: TileAction::Open }));
                        buttons.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Copy to edit", TileButton { row: index, action: TileAction::Copy }));
                        buttons.spawn(widgets::small_button(theme, ButtonKind::Quiet, "Export", TileButton { row: index, action: TileAction::Export }));
                    }
                });
            });
        });
}

/// The standing as a coloured word and, when there is one, its reason.
pub fn spawn_standing(parent: &mut ChildSpawnerCommands, theme: &Theme, standing: &Standing, format: NsgFormat) {
    let colour = match standing {
        Standing::Legal => theme.legal,
        Standing::Illegal(_) => theme.caution,
        Standing::Unplayable(_) => theme.danger,
    };
    parent.spawn((Text::new(standing.badge(format)), theme.font(size::SMALL), TextColor(colour)));
    if let Some(reason) = standing.reason() {
        parent.spawn((Text::new(reason.to_string()), theme.font(size::SMALL), TextColor(colour.with_alpha(0.85))));
    }
}

fn controls(
    mut commands: Commands,
    mut pressed: MessageReader<Pressed>,
    marks: Query<&Control>,
    tiles: Query<&TileButton>,
    popup_buttons: Query<&PopupButton>,
    picks: Query<(&Interaction, &PopupButton), (Changed<Interaction>, Without<widgets::Themed>)>,
    wash: Query<&Interaction, (Changed<Interaction>, With<Wash>)>,
    mut shelf: ResMut<Model>,
    mut popup: ResMut<Popup>,
    mut dirty: ResMut<Dirty>,
    core: Res<ClientCore>,
    mut clipboard: Option<ResMut<bevy::clipboard::Clipboard>>,
    mut navigate: MessageWriter<Navigate>,
) {
    let book = book(&core);
    let mut outcome = Outcome::Nothing;
    if wash.iter().any(|interaction| *interaction == Interaction::Pressed) {
        close_popup(&mut shelf.0, &mut popup, book);
        dirty.popup = true;
    }
    // An identity's card is a button with no theme (a picture has no
    // pill to recolour), so it reports through its own `Interaction`.
    let picked: Vec<PopupButton> = picks.iter().filter(|(interaction, _)| **interaction == Interaction::Pressed).map(|(_, button)| button.clone()).collect();
    let presses: Vec<Entity> = pressed.read().map(|Pressed(entity)| *entity).collect();
    for button in picked.iter().chain(presses.iter().filter_map(|entity| popup_buttons.get(*entity).ok())) {
        dirty.popup = true;
        match button {
            PopupButton::Cancel => close_popup(&mut shelf.0, &mut popup, book),
            PopupButton::Side(side) => *popup = Popup::Identity(*side),
            PopupButton::Identity(id) => {
                *popup = Popup::None;
                if let Some(identity) = core.registry.get(id) {
                    let name = format!("{} deck", short_title(identity));
                    let deck = deck_builder::new_deck(&name, identity, &shelf.0.taken());
                    outcome = shelf.0.create(deck, book, None);
                }
            }
            PopupButton::Delete(yes) => outcome = shelf.0.apply(Intent::ConfirmDelete(*yes), book),
        }
    }
    for entity in &presses {
        if let Ok(control) = marks.get(*entity) {
            match control {
                Control::Back => {
                    navigate.write(Navigate(AppScreen::MainMenu));
                }
                Control::New => {
                    *popup = Popup::Side;
                    dirty.popup = true;
                }
                Control::Import => {
                    outcome = match read_clipboard(clipboard.as_deref_mut()) {
                        Ok(text) => shelf.0.apply(Intent::Import(text), book),
                        Err(error) => {
                            shelf.0.notice = Some(error);
                            Outcome::Changed
                        }
                    };
                }
                Control::Side(side) => outcome = shelf.0.apply(Intent::Side(*side), book),
            }
        } else if let Ok(TileButton { row, action }) = tiles.get(*entity) {
            match action {
                TileAction::Open => {
                    if let Some(row) = shelf.0.rows.get(*row) {
                        outcome = Outcome::Open(row.deck.id.clone());
                    }
                }
                TileAction::Copy => outcome = shelf.0.apply(Intent::Copy(*row), book),
                TileAction::Export => {
                    if let Some(row) = shelf.0.rows.get(*row) {
                        let text = deck_builder::export_text(&row.deck, book);
                        shelf.0.notice = Some(write_clipboard(clipboard.as_deref_mut(), text, &row.deck.name));
                        outcome = Outcome::Changed;
                    }
                }
                TileAction::Delete => {
                    outcome = shelf.0.apply(Intent::AskDelete(*row), book);
                    dirty.popup = true;
                }
            }
        }
    }
    match outcome {
        Outcome::Nothing => {}
        Outcome::Changed => dirty.shelf = true,
        Outcome::Open(id) => {
            commands.insert_resource(EditDeck(id));
            navigate.write(Navigate(AppScreen::DeckEditor));
        }
    }
}

/// An identity's title up to its colon: "Zahya Sadeghi", not "Zahya
/// Sadeghi: Versatile Smuggler", for a new deck's name.
fn short_title(identity: &CardDefinition) -> &str {
    identity.title.split(':').next().unwrap_or(&identity.title).trim()
}

fn close_popup(shelf: &mut Shelf, popup: &mut Popup, book: CardBook) {
    *popup = Popup::None;
    if shelf.confirming.is_some() {
        shelf.apply(Intent::ConfirmDelete(false), book);
    }
}

/// The clipboard's text, or why there is none to read.
pub fn read_clipboard(clipboard: Option<&mut bevy::clipboard::Clipboard>) -> Result<String, String> {
    let Some(clipboard) = clipboard else { return Err("There is no clipboard to read here".to_string()) };
    match clipboard.fetch_text() {
        bevy::clipboard::ClipboardRead::Ready(Ok(text)) => Ok(text),
        bevy::clipboard::ClipboardRead::Ready(Err(error)) => Err(format!("The clipboard could not be read: {error}")),
        _ => Err("The clipboard is not ready; try again".to_string()),
    }
}

/// Puts a decklist on the clipboard and says so.
pub fn write_clipboard(clipboard: Option<&mut bevy::clipboard::Clipboard>, text: String, name: &str) -> String {
    match clipboard.map(|clipboard| clipboard.set_text(text)) {
        Some(Ok(())) => format!("Copied {name} to the clipboard as a decklist"),
        Some(Err(error)) => format!("The clipboard could not be written: {error}"),
        None => "There is no clipboard to write here".to_string(),
    }
}

/// A file dropped on the window is imported as if pasted.
fn dropped_files(mut drops: MessageReader<FileDragAndDrop>, mut shelf: ResMut<Model>, mut dirty: ResMut<Dirty>, core: Res<ClientCore>, mut commands: Commands, mut navigate: MessageWriter<Navigate>) {
    for drop in drops.read() {
        let FileDragAndDrop::DroppedFile { path_buf, .. } = drop else { continue };
        let outcome = match std::fs::read_to_string(path_buf) {
            Ok(text) => shelf.0.apply(Intent::Import(text), book(&core)),
            Err(error) => {
                shelf.0.notice = Some(format!("{} could not be read: {error}", path_buf.display()));
                Outcome::Changed
            }
        };
        match outcome {
            Outcome::Nothing => {}
            Outcome::Changed => dirty.shelf = true,
            Outcome::Open(id) => {
                commands.insert_resource(EditDeck(id));
                navigate.write(Navigate(AppScreen::DeckEditor));
            }
        }
    }
}

/// Escape closes the pop-up before it leaves the screen.
fn escape_closes_the_picker(keys: Res<ButtonInput<KeyCode>>, mut captured: ResMut<InputCaptured>, mut popup: ResMut<Popup>, mut shelf: ResMut<Model>, mut dirty: ResMut<Dirty>, core: Res<ClientCore>) {
    let open = *popup != Popup::None || shelf.0.confirming.is_some();
    if !open || captured.0 {
        return;
    }
    captured.0 = true;
    if keys.just_pressed(KeyCode::Escape) {
        close_popup(&mut shelf.0, &mut popup, book(&core));
        dirty.popup = true;
    }
}

fn refresh(
    mut commands: Commands,
    mut dirty: ResMut<Dirty>,
    body: Query<Entity, With<ShelfBody>>,
    toolbar: Query<Entity, With<Toolbar>>,
    mut layer: Query<(Entity, &mut Node), With<PopupLayer>>,
    mut notice: Query<&mut Text, With<NoticeLine>>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    images: Res<CardImages>,
    shelf: Res<Model>,
    popup: Res<Popup>,
) {
    let Dirty { shelf: reshelf, popup: repopup } = std::mem::take(&mut *dirty);
    if reshelf {
        if let Ok(body) = body.single() {
            commands.entity(body).despawn_children().with_children(|parent| spawn_shelf(parent, &theme, &core, &shelf.0, &images));
        }
        if let Ok(toolbar) = toolbar.single() {
            commands.entity(toolbar).despawn_children().with_children(|parent| spawn_toolbar(parent, &theme, &shelf.0));
        }
    }
    if reshelf || repopup {
        for mut text in &mut notice {
            text.0 = shelf.0.notice.clone().unwrap_or_default();
        }
    }
    if repopup || reshelf {
        let Ok((layer, mut node)) = layer.single_mut() else { return };
        let asking = *popup != Popup::None || shelf.0.confirming.is_some();
        node.display = if asking { Display::Flex } else { Display::None };
        commands.entity(layer).despawn_children();
        if asking {
            commands.entity(layer).with_children(|parent| spawn_popup(parent, &theme, &core, &shelf.0, &popup, &images));
        }
    }
}

fn spawn_popup(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, shelf: &Shelf, popup: &Popup, images: &CardImages) {
    parent
        .spawn((
            Wash,
            Button,
            GlobalZIndex(20),
            Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(theme.wash.with_alpha(0.72)),
        ))
        .with_children(|wash| {
            let wide = matches!(popup, Popup::Identity(_));
            let mut panel = wash.spawn((Interaction::None, FocusPolicy::Block, widgets::roomy_panel(theme, if wide { px(1180) } else { px(620) })));
            panel.entry::<Node>().and_modify(|mut node| node.max_height = percent(90));
            panel.with_children(|panel| {
                if let Some(index) = shelf.confirming {
                    let name = shelf.rows.get(index).map_or(String::new(), |row| row.deck.name.clone());
                    panel.spawn(widgets::heading(theme, "Delete this deck?"));
                    panel.spawn(widgets::label(theme, format!("{name} will be deleted from your data directory. This cannot be undone.")));
                    panel.spawn(Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::FlexEnd, column_gap: px(10), margin: UiRect::top(px(8)), ..default() }).with_children(|row| {
                        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Keep it", Val::Auto, PopupButton::Delete(false)));
                        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Delete", Val::Auto, PopupButton::Delete(true)));
                    });
                    return;
                }
                match popup {
                    Popup::None => {}
                    Popup::Side => {
                        panel.spawn(widgets::heading(theme, "New deck"));
                        panel.spawn(widgets::dim(theme, "Which side is it for? The identity comes next."));
                        panel.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(12), margin: UiRect::vertical(px(8)), ..default() }).with_children(|row| {
                            for side in [Side::Corp, Side::Runner] {
                                let mut button = row.spawn(widgets::styled_button(theme, ButtonKind::Secondary, format!("{side:?} deck"), px(260), PopupButton::Side(side)));
                                button.insert(BorderColor::all(theme.side(side)));
                            }
                        });
                        panel.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Cancel", Val::Auto, PopupButton::Cancel));
                    }
                    Popup::Identity(side) => {
                        let format = shelf.format();
                        panel.spawn(widgets::heading(theme, format!("New {side:?} deck: choose its identity")));
                        panel.spawn(widgets::dim(theme, format!("{}-legal identities first. The deck is saved empty and opens in the editor, where it can be renamed.", format_label(format))));
                        panel
                            .spawn((bevy::ui_widgets::ScrollArea, Node { flex_direction: FlexDirection::Column, flex_shrink: 1.0, min_height: px(0), overflow: Overflow::scroll_y(), ..default() }))
                            .with_children(|scroll| {
                                scroll.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(10), padding: UiRect::all(px(4)), ..default() }).with_children(|grid| {
                                    for identity in deck_builder::identities(&core.registry, *side, format) {
                                        let image = identity.numeric_id.and_then(|code| images.face(code, FaceSize::Thumb));
                                        spawn_face(grid, theme, &Face::of(identity), FaceSize::Thumb, image, (Button, PopupButton::Identity(identity.id.clone())));
                                    }
                                });
                            });
                        panel.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Cancel", Val::Auto, PopupButton::Cancel));
                    }
                }
            });
        });
}
