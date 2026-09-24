//! The deck editor: the deck's identity, numbers and verdict along the
//! top, the card pool on the left and the deck on the right.
//!
//! **A press on a card in the pool adds a copy; the deck's rows take one
//! away or add one back.** The pool is the format's by default, as the
//! terminal builder's is, with a switch for every printing and another
//! for the printings the engine does not play yet — a deck may hold one,
//! and says it cannot be played until the card is in. Each card in the
//! pool carries its count in the deck, so the pool is also the list of
//! what is left to add. A secondary click on a card, in the pool or the
//! deck, reads it (the game's rule: the right button, or Ctrl or Cmd
//! with the primary).
//!
//! **Every edit is saved as it is made** (`models::deck_editor`), and the
//! verdict under the name is re-read with it: the validators' words, in
//! the format Settings names, and every other format the deck is legal
//! in. A deck is saved whatever it says — only starting a game refuses
//! an illegal one.
//!
//! **A built-in deck opens read-only**: the list, the verdict and how to
//! play it, with Copy to edit in place of the pool.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use netrunner_client::card_face::Face;
use netrunner_client::cards::{faction_label, set_name};
use netrunner_client::deck_builder::{self, CardBook, Playability, PoolSort};
use netrunner_client::deck_store::{self, Origin};
use netrunner_client::settings::{format_label, FORMATS};
use netrunner_core::card::Faction;
use netrunner_core::dsl::{CardDefinition, CardId};
use netrunner_core::format::NsgFormat;

use crate::card_images::CardImages;
use crate::core::ClientCore;
use crate::models::deck_editor::{Editor, Intent, Outcome};
use crate::models::decks::{Intent as ShelfIntent, Outcome as ShelfOutcome, Shelf};
use crate::nav::{screen_root, Captures, InputCaptured, Navigate};
use crate::screens::decks::{spawn_standing, write_clipboard};
use crate::screens::AppScreen;
use crate::theme::{size, Theme};
use crate::models::layout::{self, DECK_FACE};
use crate::widgets::card_face::{spawn_face, FaceSize};
use crate::widgets::reader::{secondary_click, Readable, Reading};
use crate::widgets::dropdown::{spawn_dropdown, Choice, DropdownChanged};
use crate::widgets::text_field::{TextField, TextFieldEvent};
use crate::widgets::{self, ButtonKind, Pressed};

pub struct DeckEditorPlugin;

impl Plugin for DeckEditorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::DeckEditor), spawn)
            .add_systems(Update, escape_closes_the_popup.in_set(Captures).run_if(in_state(AppScreen::DeckEditor)))
            .add_systems(Update, (controls, text_fields, rebuild, fit_pool, refresh).chain().run_if(in_state(AppScreen::DeckEditor)));
    }
}

/// The deck the editor opens: its id, set by the Decks screen before it
/// navigates here. With none — a dev hook, a test — the editor opens the
/// deck `NETRUNNER_DECK` names (`dev`), else the default Runner deck,
/// read-only.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct EditDeck(pub String);

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Done,
    Export,
    Copy,
    Rename,
    ChangeIdentity,
    Style(Option<String>),
    Search,
    Clear,
}

/// A card in the pool: a press adds a copy.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PoolCard(pub CardId);

/// The count over a pool card, updated in place.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
struct PoolBadge(CardId);

/// A row of the deck: − and + on either side of the card's name, which
/// reads the card.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub enum DeckRowButton {
    Remove(CardId),
    Add(CardId),
    Read(CardId),
}

/// A pop-up's button.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
enum PopupButton {
    Identity(CardId),
    Cancel,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    Faction,
    Kind,
    Set,
    Format,
    Playability,
    Sort,
}

#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
enum Popup {
    #[default]
    None,
    Rename,
    Identity,
}

#[derive(Component)]
struct Header;
#[derive(Component)]
struct Filters;
#[derive(Component)]
struct SearchSlot;
#[derive(Component)]
struct PoolGrid;
#[derive(Component)]
struct DeckList;
#[derive(Component)]
struct NoticeLine;
#[derive(Component)]
struct PopupLayer;
#[derive(Component)]
struct Wash;
#[derive(Component)]
struct RenameField;

#[derive(Resource)]
pub struct Model(pub Editor);

#[derive(Resource, Default)]
struct Dirty {
    /// The deck changed: the header, the list and the pool's counts.
    deck: bool,
    /// A filter changed: the pool, and the filters themselves.
    pool: bool,
    popup: bool,
    notice: bool,
}

/// Set by the Search button; `text_fields` opens the field.
#[derive(Resource, Default)]
struct SearchRequested(bool);

const MAX_QUERY_LEN: usize = 40;
/// The deck list's column.
const LIST_WIDTH: f32 = 400.0;

fn book(core: &ClientCore) -> CardBook<'_> {
    CardBook::new(&core.registry, &core.catalog)
}

fn format_of(core: &ClientCore) -> NsgFormat {
    core.settings.format.unwrap_or(NsgFormat::Startup)
}

fn decks_dir(core: &ClientCore) -> std::path::PathBuf {
    core.decks_dir.clone().unwrap_or_else(|| std::env::temp_dir().join("netrunner-no-decks"))
}

/// Writes the deck, or leaves a note saying why it could not.
fn save(core: &ClientCore, editor: &mut Editor) {
    let result = match &core.decks_dir {
        Some(dir) => deck_store::save(dir, editor.deck()).map(|_| ()),
        None => Err("no OS data directory is available, so the deck cannot be saved".to_string()),
    };
    if let Err(error) = result {
        editor.note = Some(format!("Not saved: {error}"));
    }
}

fn spawn(mut commands: Commands, theme: Res<Theme>, core: Res<ClientCore>, images: Res<CardImages>, wanted: Option<Res<EditDeck>>, dev: Option<Res<crate::dev::Dev>>, kept: Option<Res<Model>>) {
    let id = wanted.map(|wanted| wanted.0.clone()).or_else(|| std::env::var("NETRUNNER_DECK").ok().filter(|id| !id.trim().is_empty())).unwrap_or_else(|| netrunner_client::start::DEFAULT_RUNNER_DECK.to_string());
    // The pool's order is how the person reads it, so it follows them
    // from one deck to the next; the filters are about this deck.
    let sort = kept.map_or(PoolSort::Type, |kept| kept.0.filter.sort);
    let read_only = build(&mut commands, &theme, &core, &images, &id, sort);
    // `NETRUNNER_IDENTITIES`: the picker Change identity opens.
    if dev.is_some_and(|dev| dev.identities) && !read_only {
        commands.insert_resource(Popup::Identity);
        commands.insert_resource(Dirty { popup: true, ..default() });
    }
}

/// Set when the screen must be built again on another deck — a
/// built-in deck's Copy to edit, whose copy has a pool where the
/// published list had its notes.
#[derive(Resource, Default)]
struct Rebuild(Option<String>);

fn rebuild(mut commands: Commands, mut wanted: ResMut<Rebuild>, roots: Query<(Entity, &DespawnOnExit<AppScreen>)>, theme: Res<Theme>, core: Res<ClientCore>, images: Res<CardImages>, kept: Option<Res<Model>>) {
    let Some(id) = wanted.0.take() else { return };
    for (root, screen) in &roots {
        if screen.0 == AppScreen::DeckEditor {
            commands.entity(root).despawn();
        }
    }
    commands.insert_resource(EditDeck(id.clone()));
    build(&mut commands, &theme, &core, &images, &id, kept.map_or(PoolSort::Type, |kept| kept.0.filter.sort));
}

/// Builds the screen on deck `id`; true if the deck opened read-only.
fn build(commands: &mut Commands, theme: &Theme, core: &ClientCore, images: &CardImages, id: &str, sort: PoolSort) -> bool {
    let book = book(core);
    let (mut editor, notice) = match deck_store::load(&decks_dir(core), id) {
        Ok(stored) => {
            let read_only = matches!(stored.origin, Origin::Embedded);
            let (mut editor, resolved) = Editor::open(stored.deck, read_only, book, format_of(core));
            if resolved {
                save(core, &mut editor);
            }
            (editor, None)
        }
        Err(error) => {
            let fallback = netrunner_core::decks::by_id(netrunner_client::start::DEFAULT_RUNNER_DECK).expect("the default deck is built in");
            (Editor::open(fallback, true, book, format_of(core)).0, Some(format!("That deck could not be opened: {error}")))
        }
    };
    if let Some(notice) = notice {
        editor.note = Some(notice);
    }
    editor.filter.sort = sort;
    commands.insert_resource(Popup::None);
    commands.insert_resource(Dirty::default());
    commands.insert_resource(SearchRequested::default());
    commands.init_resource::<Rebuild>();
    commands.insert_resource(PoolFace::default());

    let header = commands.spawn((Header, Node { width: percent(100), flex_direction: FlexDirection::Row, column_gap: px(18), align_items: AlignItems::FlexStart, ..default() })).id();
    commands.entity(header).with_children(|parent| spawn_header(parent, theme, core, &editor, images));

    let left = commands.spawn(Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0), height: percent(100), flex_direction: FlexDirection::Column, row_gap: px(10), ..default() }).id();
    if editor.read_only {
        // The notes, then the deck itself as its cards, each with its
        // count: a published list is read as a spread of cards.
        commands.entity(left).with_children(|parent| spawn_notes(parent, theme, &editor));
        let grid = commands.spawn((PoolGrid, Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(layout::DECK_GAP), row_gap: px(layout::DECK_GAP), padding: UiRect::all(px(GRID_PADDING)), ..default() })).id();
        commands.entity(grid).with_children(|parent| spawn_pool(parent, theme, core, &editor, images, PoolFace::default()));
        let scroll = scroll_column(commands, grid);
        let spread = commands
            .spawn(Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(6), ..default() })
            .add_child(scroll)
            .with_children(|parent| {
                parent.spawn(widgets::scrollbar(theme, scroll));
            })
            .id();
        commands.entity(left).add_child(spread);
    } else {
        let filters = commands.spawn((Filters, Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, column_gap: px(8), row_gap: px(6), ..default() })).id();
        commands.entity(filters).with_children(|parent| spawn_filters(parent, theme, core, &editor));
        let search = commands.spawn((SearchSlot, Node { flex_direction: FlexDirection::Row, ..default() })).id();
        commands.entity(search).with_children(|parent| {
            parent.spawn(widgets::button(theme, "Search", Val::Auto, Control::Search));
        });
        let toolbar = commands
            .spawn(Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, column_gap: px(8), row_gap: px(6), ..default() })
            .add_child(filters)
            .add_child(search)
            .id();
        let grid = commands.spawn((PoolGrid, Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(layout::DECK_GAP), row_gap: px(layout::DECK_GAP), padding: UiRect::all(px(GRID_PADDING)), ..default() })).id();
        commands.entity(grid).with_children(|parent| spawn_pool(parent, theme, core, &editor, images, PoolFace::default()));
        let scroll = scroll_column(commands, grid);
        let pool = commands
            .spawn(Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(6), ..default() })
            .add_child(scroll)
            .with_children(|parent| {
                parent.spawn(widgets::scrollbar(theme, scroll));
            })
            .id();
        commands.entity(left).add_child(toolbar).add_child(pool).with_children(|parent| {
            parent.spawn(widgets::dim(theme, "A press on a card adds a copy · a secondary click (right button, or Ctrl-click) reads it"));
        });
    }

    let list = commands.spawn((DeckList, Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(4), padding: UiRect::all(px(4)), ..default() })).id();
    commands.entity(list).with_children(|parent| spawn_deck_list(parent, theme, core, &editor));
    let list_scroll = scroll_column(commands, list);
    let right = commands
        .spawn((
            Node {
                width: px(LIST_WIDTH),
                flex_shrink: 0.0,
                height: percent(100),
                flex_direction: FlexDirection::Row,
                column_gap: px(6),
                padding: UiRect::all(px(10)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(crate::theme::shape::PANEL_RADIUS)),
                ..default()
            },
            BackgroundColor(theme.glass),
            BorderColor::all(theme.glass_border),
        ))
        .add_child(list_scroll)
        .with_children(|parent| {
            parent.spawn(widgets::scrollbar(theme, list_scroll));
        })
        .id();
    let body = commands.spawn(Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(14), ..default() }).add_child(left).add_child(right).id();

    let column = commands
        .spawn(Node { width: percent(100), max_width: px(2400), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column, row_gap: px(10), ..default() })
        .add_child(header)
        .with_children(|parent| {
            parent.spawn(widgets::notice(theme, editor.note.clone().unwrap_or_default(), NoticeLine));
        })
        .add_child(body)
        .id();
    let popup = commands.spawn((PopupLayer, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), display: Display::None, ..default() })).id();
    commands.spawn(screen_root(AppScreen::DeckEditor, theme)).add_child(column).add_child(popup);
    let read_only = editor.read_only;
    commands.insert_resource(Model(editor));
    read_only
}

/// A column that scrolls `content`.
fn scroll_column(commands: &mut Commands, content: Entity) -> Entity {
    commands
        .spawn((bevy::ui_widgets::ScrollArea, Node { flex_grow: 1.0, min_width: px(0), height: percent(100), flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), ..default() }))
        .add_child(content)
        .id()
}

const HEADER_FACE: u16 = 118;

fn spawn_header(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, editor: &Editor, images: &CardImages) {
    let book = book(core);
    let deck = editor.deck();
    let identity = book.get(&deck.identity);
    match identity {
        Some(card) => {
            let image = card.numeric_id.and_then(|code| images.face(code, FaceSize::Board(HEADER_FACE)));
            spawn_face(parent, theme, &Face::of(card), FaceSize::Board(HEADER_FACE), image, (Button, DeckRowButton::Read(card.id.clone()), Readable(card.id.clone())));
        }
        None => {
            parent.spawn((Node { width: px(HEADER_FACE as f32), height: px(HEADER_FACE as f32 * 1.4), flex_shrink: 0.0, ..default() }, BackgroundColor(theme.panel)));
        }
    }
    parent.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0), min_width: px(0), row_gap: px(5), ..default() }).with_children(|words| {
        words.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(12), ..default() }).with_children(|row| {
            row.spawn(widgets::heading(theme, deck.name.clone()));
            if !editor.read_only {
                row.spawn(widgets::small_button(theme, ButtonKind::Quiet, "Rename", Control::Rename));
                row.spawn(widgets::small_button(theme, ButtonKind::Quiet, "Change identity", Control::ChangeIdentity));
            }
        });
        let faction = identity.and_then(|card| card.faction).map_or("", faction_label);
        let read_only = if editor.read_only { " · built-in, read-only" } else { "" };
        words.spawn(widgets::dim(theme, format!("{:?} · {} · {faction}{read_only}", deck.side, book.title(&deck.identity))));
        words.spawn(widgets::label(theme, numbers_line(editor, book)));
        spawn_standing(words, theme, &editor.status.standing, editor.format);
        let others: Vec<&str> = editor.status.legal_in.iter().filter(|other| **other != editor.format).map(|other| format_label(*other)).collect();
        if !others.is_empty() {
            words.spawn(widgets::dim(theme, format!("Also legal in {}", others.join(", "))));
        }
        words.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, column_gap: px(6), row_gap: px(6), margin: UiRect::top(px(4)), ..default() }).with_children(|row| {
            row.spawn(widgets::overline(theme, "A bot plays it"));
            for style in editor.styles() {
                let name = style.map(|style| style.name().to_string());
                let label = name.as_deref().map_or("Balanced".to_string(), capitalised);
                let chosen = deck.style == name || (deck.style.is_none() && name.is_none());
                if editor.read_only && !chosen {
                    continue;
                }
                let kind = if chosen { ButtonKind::Secondary } else { ButtonKind::Quiet };
                let mut button = row.spawn(widgets::small_button(theme, kind, label, Control::Style(name)));
                if chosen {
                    button.insert(BorderColor::all(theme.accent));
                }
            }
        });
    });
    parent.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::FlexEnd, row_gap: px(8), flex_shrink: 0.0, ..default() }).with_children(|buttons| {
        if editor.read_only {
            buttons.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Copy to edit", px(250), Control::Copy));
        } else {
            buttons.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Done", px(250), Control::Done));
            buttons.spawn(widgets::button(theme, "Save a copy", px(250), Control::Copy));
        }
        buttons.spawn(widgets::button(theme, "Export to clipboard", px(250), Control::Export));
        if editor.read_only {
            buttons.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", px(250), Control::Done));
        } else {
            buttons.spawn(widgets::dim(theme, "Every change is saved as it is made"));
        }
    });
}

/// "45 cards (45 minimum) · 13 of 15 influence · 20 agenda points
/// (20–21)": the running totals against the identity's limits.
fn numbers_line(editor: &Editor, book: CardBook) -> String {
    let deck = editor.deck();
    let Some(tally) = editor.draft.tally(book) else {
        return format!("{} cards", deck.size());
    };
    let mut parts = vec![format!("{} cards ({} minimum)", tally.size, tally.min_size)];
    parts.push(match tally.influence_limit {
        Some(limit) => format!("{} of {limit} influence", tally.influence_spent),
        None => format!("{} influence, no limit", tally.influence_spent),
    });
    if let Some(agenda) = tally.agenda {
        parts.push(format!("{} agenda points ({}–{})", agenda.points, agenda.min, agenda.max));
    }
    parts.join(" · ")
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
}

/// A built-in deck's words in place of the pool: its summary and how it
/// wants to be played.
fn spawn_notes(parent: &mut ChildSpawnerCommands, theme: &Theme, editor: &Editor) {
    let deck = editor.deck();
    parent
        .spawn((
            Node { flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: px(8), padding: UiRect::all(px(16)), border_radius: BorderRadius::all(px(crate::theme::shape::PANEL_RADIUS)), ..default() },
            BackgroundColor(theme.glass),
        ))
        .with_children(|notes| {
            notes.spawn(widgets::overline(theme, "About this deck"));
            notes.spawn(widgets::label(theme, deck.description.clone().unwrap_or_else(|| "A published list.".to_string())));
            if let Some(how) = &deck.how_to_play {
                notes.spawn(widgets::overline(theme, "How to play it"));
                for paragraph in how.split("\n\n") {
                    notes.spawn((Text::new(paragraph.replace("**", "").replace("## ", "").replace("# ", "")), theme.font(size::SMALL), TextColor(theme.text)));
                }
            }
            notes.spawn(widgets::dim(theme, "Copy to edit makes this list your own deck, to change as you like."));
        });
}

/// A filter's drop-down: its choices, the intent each one applies, and
/// which is chosen now. The first four narrow the pool and start at
/// "All"; playability and the sort are always one of their values.
fn filter_entries(core: &ClientCore, editor: &Editor, filter: Filter) -> (Vec<Choice>, Vec<Intent>, usize) {
    let side = editor.deck().side;
    let mut entries: Vec<(String, Intent, bool)> = Vec::new();
    match filter {
        Filter::Faction => {
            entries.push(("All".into(), Intent::Faction(None), editor.filter.faction.is_none()));
            for faction in deck_builder::factions(book(core), side) {
                entries.push((faction_label(faction).into(), Intent::Faction(Some(faction)), editor.filter.faction == Some(faction)));
            }
        }
        Filter::Kind => {
            entries.push(("All".into(), Intent::Kind(None), editor.filter.kind.is_none()));
            for kind in deck_builder::kinds(side) {
                entries.push(((*kind).into(), Intent::Kind(Some(kind)), editor.filter.kind == Some(*kind)));
            }
        }
        Filter::Set => {
            entries.push(("All sets".into(), Intent::Set(None), editor.filter.set.is_none()));
            for set in deck_builder::sets(book(core), side) {
                let chosen = editor.filter.set.as_deref() == Some(set.as_str());
                entries.push((set_name(&set).to_string(), Intent::Set(Some(set)), chosen));
            }
        }
        Filter::Format => {
            entries.push(("Any format".into(), Intent::Format(None), editor.filter.format.is_none()));
            for format in FORMATS {
                entries.push((format_label(format).to_string(), Intent::Format(Some(format)), editor.filter.format == Some(format)));
            }
        }
        Filter::Playability => {
            for playability in Playability::ALL {
                entries.push((playability.label().into(), Intent::Playability(playability), editor.filter.playability == playability));
            }
        }
        Filter::Sort => {
            for sort in PoolSort::ALL {
                entries.push((sort.label().into(), Intent::Sort(sort), editor.filter.sort == sort));
            }
        }
    }
    let current = entries.iter().position(|(_, _, chosen)| *chosen).unwrap_or(0);
    let (choices, intents) = entries.into_iter().map(|(text, intent, _)| (Choice::plain(text), intent)).unzip();
    (choices, intents, current)
}

fn spawn_filters(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, editor: &Editor) {
    let filters = [
        (Filter::Faction, "Faction"),
        (Filter::Kind, "Type"),
        (Filter::Set, "Set"),
        (Filter::Format, "Legal in"),
        (Filter::Playability, "Show"),
        (Filter::Sort, "Sort by"),
    ];
    for (filter, label) in filters {
        let (choices, _, current) = filter_entries(core, editor, filter);
        spawn_dropdown(parent, theme, label, choices, current, filter);
    }
    parent.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Clear", Val::Auto, Control::Clear));
}


/// A card in the identity picker (`layout::DECK_FACE`); its text is
/// read in the hover preview. The panel is sized to it, so it does not
/// grow to fill a row as the pool's cards do.
const IDENTITY_FACE: FaceSize = FaceSize::Board(DECK_FACE as u16);

/// The width of a card in the pool or a spread: `layout::deck_face` of
/// the grid's laid-out width, set by `fit_pool`, which respawns the grid
/// when it changes. The screen is built at `DECK_FACE` and filled on the
/// frame after, when the grid has a width to read.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
struct PoolFace(f32);

impl Default for PoolFace {
    fn default() -> Self {
        PoolFace(DECK_FACE)
    }
}

fn fit_pool(grid: Query<&ComputedNode, With<PoolGrid>>, mut face: ResMut<PoolFace>, mut dirty: ResMut<Dirty>) {
    let Ok(node) = grid.single() else { return };
    let width = node.size().x * node.inverse_scale_factor() - 2.0 * GRID_PADDING;
    if width <= 0.0 {
        return;
    }
    let fitted = layout::deck_face(width);
    if fitted != face.0 {
        face.0 = fitted;
        dirty.pool = true;
    }
}

/// The pool grid's padding, on every side.
const GRID_PADDING: f32 = 4.0;

fn spawn_pool(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, editor: &Editor, images: &CardImages, face: PoolFace) {
    let size = FaceSize::Board(face.0 as u16);
    let book = book(core);
    // A read-only deck's spread is its own cards; an editable one's is
    // the pool it is built from.
    let cards: Vec<&CardDefinition> = if editor.read_only {
        deck_builder::entries(editor.deck(), book).iter().filter_map(|(id, _)| book.get(id)).collect()
    } else {
        editor.pool(book)
    };
    if cards.is_empty() {
        parent.spawn(widgets::label(theme, "No card matches these filters."));
        return;
    }
    for card in cards {
        parent.spawn(Node { flex_direction: FlexDirection::Column, ..default() }).with_children(|cell| {
            let image = card.numeric_id.and_then(|code| images.face(code, size));
            spawn_face(cell, theme, &Face::of(card), size, image, (Button, PoolCard(card.id.clone()), Readable(card.id.clone())));
            let copies = editor.draft.copies(&card.id);
            cell.spawn((
                PoolBadge(card.id.clone()),
                Pickable::IGNORE,
                GlobalZIndex(2),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(6),
                    top: px(6),
                    padding: UiRect::axes(px(9), px(3)),
                    border_radius: BorderRadius::MAX,
                    display: if copies > 0 { Display::Flex } else { Display::None },
                    ..default()
                },
                BackgroundColor(theme.primary),
                children![(Text::new(badge(copies)), theme.font(size::BODY), TextColor(theme.on_primary))],
            ));
            if !card.is_playable {
                cell.spawn((Text::new("not playable yet"), theme.font(size::SMALL), TextColor(theme.danger)));
            }
        });
    }
}

fn badge(copies: u32) -> String {
    format!("{copies}×")
}

fn spawn_deck_list(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, editor: &Editor) {
    let book = book(core);
    let groups = deck_builder::grouped(editor.deck(), book);
    parent.spawn(widgets::overline(theme, format!("The deck · {} cards", editor.deck().size())));
    if groups.is_empty() {
        parent.spawn(widgets::dim(theme, "Empty. Press a card on the left to add it."));
        return;
    }
    for deck_builder::Group { name, total, rows } in groups {
        parent.spawn((widgets::overline(theme, format!("{name} · {total}")), Node { margin: UiRect::top(px(10)), ..default() }));
        for (id, count) in rows {
            let card = book.get(&id);
            let playable = book.is_playable(&id);
            parent.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6), ..default() }).with_children(|row| {
                if !editor.read_only {
                    row.spawn(widgets::small_button(theme, ButtonKind::Quiet, "-", DeckRowButton::Remove(id.clone())));
                }
                let colour = if playable { theme.text } else { theme.danger };
                let influence = influence_dots(card, editor, book);
                let note = if playable { String::new() } else { " · not playable yet".to_string() };
                row.spawn((
                    Button,
                    DeckRowButton::Read(id.clone()),
                    Readable(id.clone()),
                    Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0), padding: UiRect::axes(px(4), px(4)), ..default() },
                    children![(Text::new(format!("{count}× {}{influence}{note}", book.title(&id))), theme.font(size::SMALL), TextColor(colour))],
                ));
                if !editor.read_only {
                    row.spawn(widgets::small_button(theme, ButtonKind::Quiet, "+", DeckRowButton::Add(id.clone())));
                }
            });
        }
    }
}

/// A card's influence cost in this deck — what a printed decklist draws
/// as dots, in words, because the client's font has no dot glyph:
/// nothing for the identity's own faction or a neutral card.
fn influence_dots(card: Option<&CardDefinition>, editor: &Editor, book: CardBook) -> String {
    let Some(card) = card else { return String::new() };
    let identity = book.get(&editor.deck().identity).and_then(|identity| identity.faction);
    let neutral = matches!(card.faction, Some(Faction::NeutralCorp) | Some(Faction::NeutralRunner) | None);
    if neutral || card.faction == identity {
        return String::new();
    }
    let cost = card.influence_cost.unwrap_or(0);
    if cost == 0 { String::new() } else { format!(" · {cost} inf") }
}

fn controls(
    mut pressed: MessageReader<Pressed>,
    mut chosen: MessageReader<DropdownChanged>,
    (marks, rows, filters, popup_buttons): (Query<&Control>, Query<&DeckRowButton>, Query<&Filter>, Query<&PopupButton>),
    (pool, reads, picks, wash): (
        Query<(&Interaction, &PoolCard), Changed<Interaction>>,
        Query<(&Interaction, &DeckRowButton), (Changed<Interaction>, Without<widgets::Themed>)>,
        Query<(&Interaction, &PopupButton), (Changed<Interaction>, Without<widgets::Themed>)>,
        Query<&Interaction, (Changed<Interaction>, With<Wash>)>,
    ),
    (keys, mouse, mut reading): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>, ResMut<Reading>),
    mut editor: ResMut<Model>,
    (mut popup, mut dirty, mut search, mut rebuild): (ResMut<Popup>, ResMut<Dirty>, ResMut<SearchRequested>, ResMut<Rebuild>),
    core: Res<ClientCore>,
    mut clipboard: Option<ResMut<bevy::clipboard::Clipboard>>,
    mut navigate: MessageWriter<Navigate>,
) {
    let book = book(&core);
    let mut intents: Vec<Intent> = Vec::new();
    if wash.iter().any(|interaction| *interaction == Interaction::Pressed) {
        *popup = Popup::None;
        dirty.popup = true;
    }
    // A secondary click is the reader's (`widgets::reader`): a Ctrl-click
    // that reads a card must not also add it or pick it.
    let secondary = secondary_click(&keys, &mouse);
    // A pool card: a press adds it — or, in a published deck's spread,
    // which adds nothing, reads it.
    for (interaction, PoolCard(id)) in &pool {
        if *interaction != Interaction::Pressed || secondary {
            continue;
        }
        if editor.0.read_only {
            reading.0 = Some(id.clone());
        } else {
            intents.push(Intent::Add(id.clone()));
        }
    }
    for (interaction, button) in &reads {
        if *interaction == Interaction::Pressed
            && !secondary
            && let DeckRowButton::Read(id) = button
        {
            reading.0 = Some(id.clone());
        }
    }
    for (interaction, button) in &picks {
        if *interaction == Interaction::Pressed
            && !secondary
            && let PopupButton::Identity(id) = button
        {
            intents.push(Intent::Identity(id.clone()));
            *popup = Popup::None;
            dirty.popup = true;
        }
    }
    for DropdownChanged { dropdown, index } in chosen.read() {
        let Ok(filter) = filters.get(*dropdown) else { continue };
        let (_, filter_intents, _) = filter_entries(&core, &editor.0, *filter);
        if let Some(intent) = filter_intents.get(*index) {
            intents.push(intent.clone());
        }
    }
    for Pressed(entity) in pressed.read() {
        if let Ok(button) = rows.get(*entity) {
            match button {
                DeckRowButton::Remove(id) => intents.push(Intent::Remove(id.clone())),
                DeckRowButton::Add(id) => intents.push(Intent::Add(id.clone())),
                DeckRowButton::Read(_) => {}
            }
            continue;
        }
        if let Ok(PopupButton::Cancel) = popup_buttons.get(*entity) {
            *popup = Popup::None;
            dirty.popup = true;
            continue;
        }
        let Ok(control) = marks.get(*entity) else { continue };
        match control {
            Control::Done => {
                navigate.write(Navigate(AppScreen::Decks));
            }
            Control::Export => {
                let text = deck_builder::export_text(editor.0.deck(), book);
                let name = editor.0.deck().name.clone();
                editor.0.note = Some(write_clipboard(clipboard.as_deref_mut(), text, &name));
                dirty.notice = true;
            }
            Control::Copy => {
                // The copy is made the way the Decks screen makes one, so
                // its name and id follow the same rule, and opens here.
                let mut shelf = Shelf::open(core.decks_dir.clone(), book, editor.0.format);
                let deck = editor.0.deck().clone();
                let index = shelf.rows.iter().position(|row| row.deck.id == deck.id);
                let outcome = match index {
                    Some(index) => shelf.apply(ShelfIntent::Copy(index), book),
                    None => shelf.create(deck_builder::copy_of(&deck, &deck_builder::copy_name(&deck.name, &[]), &shelf.taken()), book, None),
                };
                match outcome {
                    ShelfOutcome::Open(id) => rebuild.0 = Some(id),
                    _ => {
                        editor.0.note = shelf.notice.clone();
                        dirty.notice = true;
                    }
                }
            }
            Control::Rename => {
                *popup = Popup::Rename;
                dirty.popup = true;
            }
            Control::ChangeIdentity => {
                *popup = Popup::Identity;
                dirty.popup = true;
            }
            Control::Style(style) => intents.push(Intent::Style(style.clone())),
            Control::Clear => intents.push(Intent::ClearFilters),
            Control::Search => search.0 = true,
        }
    }
    for intent in intents {
        match editor.0.apply(intent, book) {
            Outcome::Nothing => {}
            Outcome::View => {
                dirty.pool = true;
                dirty.notice = true;
            }
            Outcome::Save => {
                save(&core, &mut editor.0);
                dirty.deck = true;
                dirty.notice = true;
            }
        }
    }
}

/// The search field and the rename field. One text field exists at a
/// time (`widgets::text_field`), so opening the rename closes the search
/// — its query stays — and the search reopens from its button.
fn text_fields(
    mut commands: Commands,
    mut fields: Query<(Entity, &mut TextField, Option<&TextFieldEvent>, Has<RenameField>)>,
    slot: Query<Entity, With<SearchSlot>>,
    buttons: Query<(Entity, &Control)>,
    mut search: ResMut<SearchRequested>,
    mut editor: ResMut<Model>,
    mut popup: ResMut<Popup>,
    mut dirty: ResMut<Dirty>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
) {
    let book = book(&core);
    let mut search_open = false;
    for (entity, mut field, event, rename) in &mut fields {
        if rename {
            match event {
                Some(TextFieldEvent::Committed(name)) => {
                    if editor.0.apply(Intent::Rename(name.clone()), book) == Outcome::Save {
                        save(&core, &mut editor.0);
                        dirty.deck = true;
                        dirty.notice = true;
                    }
                    *popup = Popup::None;
                    dirty.popup = true;
                }
                Some(TextFieldEvent::Cancelled) => {
                    *popup = Popup::None;
                    dirty.popup = true;
                }
                None => {}
            }
            continue;
        }
        search_open = true;
        let close = *popup == Popup::Rename;
        match event {
            Some(TextFieldEvent::Committed(_)) => {
                commands.entity(entity).remove::<TextFieldEvent>();
            }
            Some(TextFieldEvent::Cancelled) if !field.text.is_empty() && !close => {
                field.text.clear();
                commands.entity(entity).remove::<TextFieldEvent>();
            }
            Some(TextFieldEvent::Cancelled) => {
                close_search(&mut commands, entity, &slot, &theme);
                search_open = false;
                continue;
            }
            None if close => {
                close_search(&mut commands, entity, &slot, &theme);
                search_open = false;
                continue;
            }
            None => {}
        }
        if field.text != editor.0.filter.query && editor.0.apply(Intent::Query(field.text.clone()), book) == Outcome::View {
            dirty.pool = true;
        }
    }
    if std::mem::take(&mut search.0) && !search_open && *popup == Popup::None {
        for (entity, control) in &buttons {
            if *control == Control::Search {
                commands.entity(entity).despawn();
            }
        }
        if let Ok(slot) = slot.single() {
            let current = editor.0.filter.query.clone();
            commands.entity(slot).with_children(|parent| {
                parent.spawn((
                    TextField { text: current.clone(), max_len: MAX_QUERY_LEN },
                    widgets::field_node(px(240)),
                    BackgroundColor(theme.glass_strong),
                    BorderColor::all(theme.accent),
                    children![(Text::new(format!("{current}|")), theme.font(size::SMALL), TextColor(theme.text))],
                ));
            });
        }
    }
}

fn close_search(commands: &mut Commands, field: Entity, slot: &Query<Entity, With<SearchSlot>>, theme: &Theme) {
    commands.entity(field).despawn();
    if let Ok(slot) = slot.single() {
        commands.entity(slot).with_children(|parent| {
            parent.spawn(widgets::button(theme, "Search", Val::Auto, Control::Search));
        });
    }
}

/// Escape closes a pop-up before it leaves the screen. The rename field
/// takes its own Escape (`widgets::text_field`), so this stands down
/// while it is open.
fn escape_closes_the_popup(keys: Res<ButtonInput<KeyCode>>, mut captured: ResMut<InputCaptured>, mut popup: ResMut<Popup>, mut dirty: ResMut<Dirty>, reading: Res<Reading>) {
    // A card read over the picker takes the Escape first.
    if matches!(*popup, Popup::None | Popup::Rename) || captured.0 || reading.is_open() {
        return;
    }
    captured.0 = true;
    if keys.just_pressed(KeyCode::Escape) {
        *popup = Popup::None;
        dirty.popup = true;
    }
}

fn refresh(
    mut commands: Commands,
    mut dirty: ResMut<Dirty>,
    header: Query<Entity, With<Header>>,
    filters: Query<Entity, With<Filters>>,
    grid: Query<Entity, With<PoolGrid>>,
    list: Query<Entity, With<DeckList>>,
    badges: Query<(&PoolBadge, &Children)>,
    mut badge_nodes: Query<&mut Node, With<PoolBadge>>,
    mut texts: Query<&mut Text, Without<NoticeLine>>,
    mut notice: Query<&mut Text, With<NoticeLine>>,
    mut layer: Query<(Entity, &mut Node), (With<PopupLayer>, Without<PoolBadge>)>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    images: Res<CardImages>,
    editor: Res<Model>,
    (popup, face): (Res<Popup>, Res<PoolFace>),
) {
    let Dirty { deck, pool, popup: repopup, notice: renotice } = std::mem::take(&mut *dirty);
    let editor = &editor.0;
    if deck {
        if let Ok(header) = header.single() {
            commands.entity(header).despawn_children().with_children(|parent| spawn_header(parent, &theme, &core, editor, &images));
        }
        if let Ok(list) = list.single() {
            commands.entity(list).despawn_children().with_children(|parent| spawn_deck_list(parent, &theme, &core, editor));
        }
        if !pool {
            // The counts over the pool, in place: respawning every face
            // for one added copy is what the browser learned not to do.
            for (PoolBadge(id), children) in &badges {
                let copies = editor.draft.copies(id);
                for child in children.iter() {
                    if let Ok(mut text) = texts.get_mut(child) {
                        text.0 = badge(copies);
                    }
                }
            }
            for (mut node, (PoolBadge(id), _)) in badge_nodes.iter_mut().zip(badges.iter()) {
                node.display = if editor.draft.copies(id) > 0 { Display::Flex } else { Display::None };
            }
        }
    }
    if pool {
        if let Ok(filters) = filters.single() {
            commands.entity(filters).despawn_children().with_children(|parent| spawn_filters(parent, &theme, &core, editor));
        }
        if let Ok(grid) = grid.single() {
            commands.entity(grid).despawn_children().with_children(|parent| spawn_pool(parent, &theme, &core, editor, &images, *face));
        }
    }
    if deck || pool || renotice || repopup {
        for mut text in &mut notice {
            text.0 = editor.note.clone().unwrap_or_default();
        }
    }
    if repopup {
        let Ok((layer, mut node)) = layer.single_mut() else { return };
        node.display = if *popup == Popup::None { Display::None } else { Display::Flex };
        commands.entity(layer).despawn_children();
        if *popup != Popup::None {
            commands.entity(layer).with_children(|parent| spawn_popup(parent, &theme, &core, editor, &popup, &images));
        }
    }
}

/// The identity picker's panel: six [`IDENTITY_FACE`] cards across on a
/// window that has the room, fewer where the panel's `max_width` gives.
pub(crate) const IDENTITY_PANEL: f32 = 6.0 * DECK_FACE + 5.0 * 10.0 + 2.0 * 28.0 + 2.0 + 16.0;

fn spawn_popup(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, editor: &Editor, popup: &Popup, images: &CardImages) {
    parent
        .spawn((
            Wash,
            Button,
            GlobalZIndex(20),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                padding: UiRect::all(px(24)),
                ..default()
            },
            BackgroundColor(theme.wash.with_alpha(0.72)),
        ))
        .with_children(|wash| match popup {
            Popup::None => {}
            Popup::Rename => {
                wash.spawn((Interaction::None, FocusPolicy::Block, widgets::roomy_panel(theme, px(620)))).with_children(|panel| {
                    panel.spawn(widgets::heading(theme, "Rename the deck"));
                    panel.spawn(widgets::dim(theme, "Enter keeps the new name, Escape keeps the old. The deck's file keeps its name."));
                    let current = editor.deck().name.clone();
                    panel.spawn((
                        RenameField,
                        TextField { text: current.clone(), max_len: deck_builder::MAX_NAME },
                        widgets::field_node(percent(100)),
                        BackgroundColor(theme.glass_strong),
                        BorderColor::all(theme.accent),
                        children![(Text::new(format!("{current}|")), theme.font(size::BODY), TextColor(theme.text))],
                    ));
                });
            }
            Popup::Identity => {
                let mut panel = wash.spawn((Interaction::None, FocusPolicy::Block, widgets::roomy_panel(theme, px(IDENTITY_PANEL))));
                panel.entry::<Node>().and_modify(|mut node| node.max_height = percent(90));
                panel.with_children(|panel| {
                    panel.spawn(widgets::heading(theme, "Change the identity"));
                    panel.spawn(widgets::dim(theme, "The cards stay; whatever the new identity makes illegal, the verdict names."));
                    panel
                        .spawn((bevy::ui_widgets::ScrollArea, Node { flex_direction: FlexDirection::Column, flex_shrink: 1.0, min_height: px(0), overflow: Overflow::scroll_y(), ..default() }))
                        .with_children(|scroll| {
                            scroll.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(10), row_gap: px(10), padding: UiRect::all(px(4)), ..default() }).with_children(|grid| {
                                for identity in deck_builder::identities(&core.registry, editor.deck().side, editor.format) {
                                    let image = identity.numeric_id.and_then(|code| images.face(code, IDENTITY_FACE));
                                    let face = spawn_face(grid, theme, &Face::of(identity), IDENTITY_FACE, image, (Button, PopupButton::Identity(identity.id.clone()), Readable(identity.id.clone())));
                                    if identity.id == editor.deck().identity {
                                        grid.commands().entity(face).insert(Outline { width: px(3), offset: px(1), color: theme.accent });
                                    }
                                }
                            });
                        });
                    panel.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Cancel", Val::Auto, PopupButton::Cancel));
                });
            }
        });
}
