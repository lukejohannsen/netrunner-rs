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
//! **Import and export are files, through the native dialog** (`files`,
//! Phase 7 §10 Stage 5). Import reads a file as a decklist — this
//! project's deck file, or the text list NetrunnerDB, jinteki.net or a
//! hand-typed list writes (`deck_builder::import`) — and Export writes the
//! deck in that text shape, offered as `<name>.txt` in Downloads. A
//! `.txt` or `.json` file dropped on the window is imported the same way.
//! Whatever an import could not read is named, and the deck is saved
//! anyway. Both went through the clipboard until the person asked for
//! files (30 September 2026), so that a deck can be handed to someone;
//! the clipboard is the Online screen's now, for a ticket — and this
//! screen's Paste, for a link.
//!
//! **A published decklist comes by its link** (`netrunnerdb`, Phase 7 §10
//! Stage 6). Import from NetrunnerDB… opens a pop-up with one field for a
//! decklist's link or uuid and a Paste beside it; Enter or Fetch asks the
//! v3 API on the tokio runtime, the pop-up says "Fetching…" until the
//! answer comes, and the deck is saved and opened like an imported file,
//! its author and link kept as its description. What NetrunnerDB refuses,
//! or what is not a decklist link, is said in the pop-up with the field
//! still holding what was typed.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::window::FileDragAndDrop;

use netrunner_card_sync::DecklistRef;
use netrunner_client::card_face::Face;
use netrunner_client::deck_builder::{self, CardBook, Standing};
use netrunner_client::cards::faction_label;
use netrunner_client::settings::{format_label, FORMATS};
use netrunner_core::dsl::{CardDefinition, CardId};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

use crate::audio::{ButtonSound, Sfx};
use crate::card_images::CardImages;
use crate::core::{ClientCore, TokioRuntime};
use crate::files::{Ask, DeckFiles, Done};
use crate::models::decks::{Intent, Outcome, Shelf, ShelfRow, ShelfSort};
use crate::nav::{screen_root, Captures, InputCaptured, Navigate};
use crate::netrunnerdb::Decklists;
use crate::screens::deck_editor::EditDeck;
use crate::screens::online::read_clipboard;
use crate::screens::AppScreen;
use crate::theme::{size, Theme};
use crate::widgets::card_face::{spawn_face, FaceSize};
use crate::widgets::dropdown::{spawn_dropdown, Choice, DropdownChanged};
use crate::widgets::text_field::{edit_text_fields, TextField, TextFieldEvent};
use crate::widgets::{self, ButtonKind, Pressed};

pub struct DecksPlugin;

impl Plugin for DecksPlugin {
    fn build(&self, app: &mut App) {
        // Registered here as well as by the window plugin, which a
        // headless test does not run; `add_message` is idempotent.
        app.add_message::<FileDragAndDrop>()
            .add_systems(OnEnter(AppScreen::Decks), spawn)
            // After the text fields: with the link field open, Escape is
            // the field's (it closes the pop-up through `Cancelled`), and
            // one key does one thing.
            .add_systems(Update, escape_closes_the_picker.in_set(Captures).after(edit_text_fields).run_if(in_state(AppScreen::Decks)))
            .add_systems(Update, (controls, dropped_files, file_answers, fetch_answers, redraw_new_art, refresh).chain().run_if(in_state(AppScreen::Decks)));
    }
}

/// The toolbar's drop-downs.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum ShelfFilter {
    Faction,
    Legal,
    Sort,
}

/// A drop-down's choices, the intent each applies, and which is chosen.
fn shelf_entries(shelf: &Shelf, filter: ShelfFilter) -> (Vec<Choice>, Vec<Intent>, usize) {
    let view = &shelf.view;
    let mut entries: Vec<(String, Intent, bool)> = Vec::new();
    match filter {
        ShelfFilter::Faction => {
            entries.push(("All".into(), Intent::Faction(None), view.faction.is_none()));
            for faction in shelf.factions() {
                entries.push((faction_label(faction).into(), Intent::Faction(Some(faction)), view.faction == Some(faction)));
            }
        }
        ShelfFilter::Legal => {
            entries.push(("Any format".into(), Intent::Legal(None), view.legal.is_none()));
            for format in FORMATS {
                entries.push((format_label(format).to_string(), Intent::Legal(Some(format)), view.legal == Some(format)));
            }
        }
        ShelfFilter::Sort => {
            for sort in ShelfSort::ALL {
                entries.push((sort.label().into(), Intent::Sort(sort), view.sort == sort));
            }
        }
    }
    let current = entries.iter().position(|(_, _, chosen)| *chosen).unwrap_or(0);
    let (choices, intents) = entries.into_iter().map(|(text, intent, _)| (Choice::plain(text), intent)).unzip();
    (choices, intents, current)
}

/// The toolbar's buttons.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    New,
    Import,
    /// Import from NetrunnerDB…: the link pop-up.
    Fetch,
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
    /// Import from NetrunnerDB: fetch what the field holds.
    Fetch,
    /// Import from NetrunnerDB: the clipboard into the field.
    Paste,
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
    /// Import from NetrunnerDB: which decklist? `typed` is what the field
    /// holds across a redraw, `problem` why the last try was refused.
    Fetch { typed: String, problem: Option<String> },
}

/// The most a decklist's link runs to, with room: a uuid is 36
/// characters, and a page link with its slug about 120.
const LINK_MAX: usize = 200;

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
    core.settings.format.unwrap_or(netrunner_client::settings::DEFAULT_FORMAT)
}

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, images: Res<CardImages>, kept: Option<Res<Model>>, dev: Option<Res<crate::dev::Dev>>) {
    // The notice an editor's Copy or an import left survives the trip
    // back; the rows are re-read, since the editor wrote to them.
    // So does how the shelf was being looked at: its side, filters and
    // sort are the person's until they change them.
    let notice = kept.as_ref().and_then(|kept| kept.0.notice.clone());
    let view = kept.map(|kept| kept.0.view.clone()).unwrap_or_default();
    let mut shelf = Shelf::open(core.decks_dir.clone(), book(&core), format_of(&core));
    shelf.view = view;
    if notice.is_some() {
        shelf.notice = notice;
    }
    // `NETRUNNER_FETCH`: the link pop-up open for a screenshot.
    match dev.as_ref().and_then(|dev| dev.fetch.clone()) {
        Some(typed) => {
            commands.insert_resource(Popup::Fetch { typed, problem: None });
            commands.insert_resource(Dirty { shelf: false, popup: true });
        }
        None => {
            commands.insert_resource(Popup::None);
            commands.init_resource::<Dirty>();
        }
    }

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
                row.spawn(widgets::dim(&theme, format!("Checked against {format}, the format Settings names. A .txt or .json decklist dropped onto the window is imported too.")));
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
    Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(8), ..default() }
}

fn toolbar_row() -> Node {
    Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, column_gap: px(10), row_gap: px(8), ..default() }
}

/// Two rows: what makes a deck (New deck, the two imports, and Back at
/// the right), then how the shelf is looked at (the side, the filters,
/// the sort). One row held all of it until Import from NetrunnerDB…
/// joined (Stage 6), when it wrapped the sort drop-down onto a second
/// row by itself at 1920 px wide; the split is where the row would
/// break anyway, between the actions and the view.
fn spawn_toolbar(parent: &mut ChildSpawnerCommands, theme: &Theme, shelf: &Shelf) {
    parent.spawn(toolbar_row()).with_children(|row| {
        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "New deck", Val::Auto, Control::New));
        row.spawn(widgets::button(theme, "Import from file…", Val::Auto, Control::Import));
        row.spawn(widgets::button(theme, "Import from NetrunnerDB…", Val::Auto, Control::Fetch));
        let mut back = row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", Val::Auto, (Control::Back, ButtonSound(Sfx::Back))));
        back.entry::<Node>().and_modify(|mut node| node.margin = UiRect::left(Val::Auto));
    });
    parent.spawn(toolbar_row()).with_children(|row| {
        for (label, side) in [("All", None), ("Corp", Some(Side::Corp)), ("Runner", Some(Side::Runner))] {
            let kind = if shelf.view.side == side { ButtonKind::Secondary } else { ButtonKind::Quiet };
            let mut button = row.spawn(widgets::styled_button(theme, kind, label, Val::Auto, Control::Side(side)));
            if shelf.view.side == side {
                button.insert(BorderColor::all(theme.accent));
            }
        }
        row.spawn((Node { width: px(18), ..default() },));
        for (filter, label) in [(ShelfFilter::Faction, "Faction"), (ShelfFilter::Legal, "Legal in"), (ShelfFilter::Sort, "Sort by")] {
            let (choices, _, current) = shelf_entries(shelf, filter);
            spawn_dropdown(row, theme, label, choices, current, filter);
        }
    });
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
                    let drawn = Face::of(card, &core.settings.art);
                    let image = images.of(&drawn, FaceSize::Board(TILE_FACE));
                    spawn_face(tile, theme, &drawn, FaceSize::Board(TILE_FACE), image, ());
                }
                None => {
                    tile.spawn((Node { width: px(TILE_FACE as f32), height: px(TILE_FACE as f32 * 1.4), flex_shrink: 0.0, ..default() }, BackgroundColor(theme.panel)));
                }
            }
            tile.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0), min_width: px(0), row_gap: px(4), ..default() }).with_children(|words| {
                words.spawn((Text::new(row.deck.name.clone()), theme.font(size::BODY), TextColor(theme.text)));
                words.spawn(widgets::dim(theme, row.identity.clone()));
                let style = row.deck.style_label();
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
                        buttons.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Export…", TileButton { row: index, action: TileAction::Export }));
                        buttons.spawn(widgets::small_button(theme, ButtonKind::Quiet, "Delete", TileButton { row: index, action: TileAction::Delete }));
                    } else {
                        buttons.spawn(widgets::small_button(theme, ButtonKind::Secondary, "View", TileButton { row: index, action: TileAction::Open }));
                        buttons.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Copy to edit", TileButton { row: index, action: TileAction::Copy }));
                        buttons.spawn(widgets::small_button(theme, ButtonKind::Quiet, "Export…", TileButton { row: index, action: TileAction::Export }));
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
    (mut chosen, filters): (MessageReader<DropdownChanged>, Query<&ShelfFilter>),
    marks: Query<&Control>,
    tiles: Query<&TileButton>,
    popup_buttons: Query<&PopupButton>,
    picks: Query<(&Interaction, &PopupButton), (Changed<Interaction>, Without<widgets::Themed>)>,
    wash: Query<&Interaction, (Changed<Interaction>, With<Wash>)>,
    (keys, mouse): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>),
    mut shelf: ResMut<Model>,
    mut popup: ResMut<Popup>,
    mut dirty: ResMut<Dirty>,
    core: Res<ClientCore>,
    (mut files, runtime): (ResMut<DeckFiles>, Option<Res<TokioRuntime>>),
    (fields, mut lists, mut clipboard): (Query<(&TextField, Option<&TextFieldEvent>)>, ResMut<Decklists>, Option<ResMut<bevy::clipboard::Clipboard>>),
    mut navigate: MessageWriter<Navigate>,
) {
    let book = book(&core);
    let mut outcome = Outcome::Nothing;
    if wash.iter().any(|interaction| *interaction == Interaction::Pressed) {
        close_popup(&mut shelf.0, &mut popup, book);
        dirty.popup = true;
    }
    // The link field: Enter fetches what it holds, Escape closes the
    // pop-up. The field is the pop-up's only one.
    let mut fetch: Option<String> = None;
    for (_, event) in &fields {
        match event {
            Some(TextFieldEvent::Committed(text)) => fetch = Some(text.clone()),
            Some(TextFieldEvent::Cancelled) => {
                close_popup(&mut shelf.0, &mut popup, book);
                dirty.popup = true;
            }
            None => {}
        }
    }
    for DropdownChanged { dropdown, index } in chosen.read() {
        let Ok(filter) = filters.get(*dropdown) else { continue };
        let (_, intents, _) = shelf_entries(&shelf.0, *filter);
        if let Some(intent) = intents.get(*index) {
            outcome = shelf.0.apply(intent.clone(), book);
        }
    }
    // An identity's card is a button with no theme (a picture has no
    // pill to recolour), so it reports through its own `Interaction`.
    // A Ctrl-click reads the identity (`widgets::reader`) and picks nothing.
    let secondary = crate::widgets::reader::secondary_click(&keys, &mouse);
    let picked: Vec<PopupButton> = picks.iter().filter(|(interaction, _)| **interaction == Interaction::Pressed && !secondary).map(|(_, button)| button.clone()).collect();
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
            PopupButton::Fetch => fetch = Some(fields.iter().map(|(field, _)| field.text.clone()).next().unwrap_or_default()),
            PopupButton::Paste => {
                if let Popup::Fetch { typed, problem } = &mut *popup {
                    match read_clipboard(clipboard.as_deref_mut()) {
                        Ok(text) => {
                            *typed = text.lines().map(str::trim).collect();
                            *problem = None;
                        }
                        Err(reason) => *problem = Some(reason),
                    }
                }
            }
        }
    }
    if let (Some(text), Popup::Fetch { typed, problem }) = (fetch, &mut *popup) {
        // The field is redrawn from `typed`, so what was typed survives
        // a refusal; the request itself answers through `fetch_answers`.
        *typed = text.clone();
        *problem = match DecklistRef::parse(&text) {
            Ok(reference) => lists.fetch(runtime.as_deref(), reference).err(),
            Err(reason) => Some(reason),
        };
        dirty.popup = true;
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
                Control::Fetch => {
                    *popup = Popup::Fetch { typed: String::new(), problem: None };
                    dirty.popup = true;
                }
                Control::Import => {
                    // The dialog answers later, through `file_answers`.
                    if let Err(error) = files.ask(runtime.as_deref(), Ask::Open) {
                        shelf.0.notice = Some(error);
                        outcome = Outcome::Changed;
                    }
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
                        if let Err(error) = files.ask(runtime.as_deref(), Ask::Save { name: row.deck.name.clone(), text }) {
                            shelf.0.notice = Some(error);
                            outcome = Outcome::Changed;
                        }
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

/// Imports the decklist in the file at `path`: the one way in for a
/// dropped file and a chosen one.
fn import_file(shelf: &mut Shelf, book: CardBook, path: &std::path::Path) -> Outcome {
    match std::fs::read_to_string(path) {
        Ok(text) => shelf.apply(Intent::Import(text), book),
        Err(error) => {
            shelf.notice = Some(format!("{} could not be read: {error}", path.display()));
            Outcome::Changed
        }
    }
}

/// Acts on what an import or export did: a new deck opens, a notice says
/// where the file went or what went wrong.
fn act_on(outcome: Outcome, dirty: &mut Dirty, commands: &mut Commands, navigate: &mut MessageWriter<Navigate>) {
    match outcome {
        Outcome::Nothing => {}
        Outcome::Changed => dirty.shelf = true,
        Outcome::Open(id) => {
            commands.insert_resource(EditDeck(id));
            navigate.write(Navigate(AppScreen::DeckEditor));
        }
    }
}

/// A file dropped on the window is imported as one chosen in the dialog.
fn dropped_files(mut drops: MessageReader<FileDragAndDrop>, mut shelf: ResMut<Model>, mut dirty: ResMut<Dirty>, core: Res<ClientCore>, mut commands: Commands, mut navigate: MessageWriter<Navigate>) {
    for drop in drops.read() {
        let FileDragAndDrop::DroppedFile { path_buf, .. } = drop else { continue };
        let outcome = import_file(&mut shelf.0, book(&core), path_buf);
        act_on(outcome, &mut dirty, &mut commands, &mut navigate);
    }
}

/// The dialog's answer, when it comes: the chosen file is imported, or
/// the export is reported. A cancelled dialog is nothing.
fn file_answers(mut files: ResMut<DeckFiles>, mut shelf: ResMut<Model>, mut dirty: ResMut<Dirty>, core: Res<ClientCore>, mut commands: Commands, mut navigate: MessageWriter<Navigate>) {
    let outcome = match files.poll() {
        None | Some(Done::Opened(None)) | Some(Done::Saved(None)) => return,
        Some(Done::Opened(Some(path))) => import_file(&mut shelf.0, book(&core), &path),
        Some(Done::Saved(Some(result))) => {
            shelf.0.notice = Some(exported(result));
            Outcome::Changed
        }
    };
    act_on(outcome, &mut dirty, &mut commands, &mut navigate);
}

/// NetrunnerDB's answer, when it comes: the list is saved and opened like
/// an imported file, or the pop-up says why there is none — and the
/// shelf does, if the pop-up was closed in the meantime.
fn fetch_answers(mut lists: ResMut<Decklists>, mut shelf: ResMut<Model>, mut popup: ResMut<Popup>, mut dirty: ResMut<Dirty>, core: Res<ClientCore>, mut commands: Commands, mut navigate: MessageWriter<Navigate>) {
    let Some(answer) = lists.poll() else { return };
    match answer {
        Ok(list) => {
            if matches!(*popup, Popup::Fetch { .. }) {
                *popup = Popup::None;
            }
            dirty.popup = true;
            let outcome = shelf.0.apply(Intent::Fetched(list), book(&core));
            act_on(outcome, &mut dirty, &mut commands, &mut navigate);
        }
        Err(reason) => match &mut *popup {
            Popup::Fetch { problem, .. } => {
                *problem = Some(reason);
                dirty.popup = true;
            }
            _ => {
                shelf.0.notice = Some(format!("Nothing fetched: {reason}"));
                dirty.shelf = true;
            }
        },
    }
}

/// The line an export leaves: where the file is, or why there is none.
pub fn exported(result: Result<std::path::PathBuf, String>) -> String {
    match result {
        Ok(path) => format!("Exported to {}", path.display()),
        Err(error) => format!("Not exported: {error}"),
    }
}

/// Escape closes the pop-up before it leaves the screen.
fn escape_closes_the_picker(keys: Res<ButtonInput<KeyCode>>, mut captured: ResMut<InputCaptured>, mut popup: ResMut<Popup>, mut shelf: ResMut<Model>, mut dirty: ResMut<Dirty>, core: Res<ClientCore>, reading: Res<crate::widgets::reader::Reading>) {
    let open = *popup != Popup::None || shelf.0.confirming.is_some();
    // A card read over the picker takes the Escape first.
    if !open || captured.0 || reading.is_open() {
        return;
    }
    captured.0 = true;
    if keys.just_pressed(KeyCode::Escape) {
        close_popup(&mut shelf.0, &mut popup, book(&core));
        dirty.popup = true;
    }
}

/// A card's art chosen in the reader (`widgets::art`): the shelf's
/// identity faces and a picker are drawn again.
fn redraw_new_art(mut changed: MessageReader<crate::widgets::art::ArtChanged>, mut dirty: ResMut<Dirty>) {
    if changed.read().count() > 0 {
        dirty.shelf = true;
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
    lists: Res<Decklists>,
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
            commands.entity(layer).with_children(|parent| spawn_popup(parent, &theme, &core, &shelf.0, &popup, &images, lists.is_fetching()));
        }
    }
}

fn spawn_popup(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, shelf: &Shelf, popup: &Popup, images: &CardImages, fetching: bool) {
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
            let mut panel = wash.spawn((Interaction::None, FocusPolicy::Block, widgets::roomy_panel(theme, if wide { px(crate::screens::deck_editor::IDENTITY_PANEL) } else { px(620) })));
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
                                        // The editor's picker's size, and read by a
                                        // secondary click: an identity is chosen by
                                        // what its text says.
                                        let size = FaceSize::Board(crate::models::layout::DECK_FACE as u16);
                                        let drawn = Face::of(identity, &core.settings.art);
                                        let image = images.of(&drawn, size);
                                        spawn_face(grid, theme, &drawn, size, image, (Button, PopupButton::Identity(identity.id.clone()), crate::widgets::reader::Readable(identity.id.clone())));
                                    }
                                });
                            });
                        panel.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Cancel", Val::Auto, PopupButton::Cancel));
                    }
                    Popup::Fetch { typed, problem } => {
                        panel.spawn(widgets::heading(theme, "Import from NetrunnerDB"));
                        panel.spawn(widgets::dim(theme, "A published decklist's link or its uuid. The deck is saved as your own and opens in the editor; the author and the link stay with it."));
                        // The field is the editor itself — there is
                        // nothing else to type here — with a Paste for
                        // the link nobody types (Ctrl+V works too).
                        panel.spawn(widgets::row(12.0)).with_children(|row| {
                            row.spawn(Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }).with_children(|slot| {
                                slot.spawn((
                                    TextField::new(typed.clone(), LINK_MAX),
                                    widgets::field_node(percent(100)),
                                    BackgroundColor(theme.glass_strong),
                                    BorderColor::all(theme.accent),
                                    children![(Text::new(format!("{typed}|")), theme.font(size::BODY), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::AnyCharacter))],
                                ));
                            });
                            row.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Paste", PopupButton::Paste));
                        });
                        if let Some(problem) = problem {
                            panel.spawn((widgets::notice(theme, problem.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                        }
                        panel.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, justify_content: JustifyContent::FlexEnd, column_gap: px(10), margin: UiRect::top(px(8)), ..default() }).with_children(|row| {
                            if fetching {
                                row.spawn(widgets::dim(theme, "Fetching…"));
                            }
                            row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Cancel", Val::Auto, PopupButton::Cancel));
                            if fetching {
                                row.spawn(widgets::disabled_button(theme, "Fetch", px(160), PopupButton::Fetch));
                            } else {
                                row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Fetch", px(160), PopupButton::Fetch));
                            }
                        });
                    }
                }
            });
        });
}
