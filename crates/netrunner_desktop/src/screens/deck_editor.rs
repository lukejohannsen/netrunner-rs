//! The deck editor, in three columns (Phase 7 §16, stage (b)): the
//! search and the filters down a thin left column, the card pool as a
//! grid of faces taking the width between, and the deck down the right
//! — its identity's face, a badge per format, the metrics, the list and
//! the screen's buttons. Before (§5, §10) the identity and the numbers
//! were a header across the top, the filters a toolbar over the pool and
//! the list a column at the right; the header and the toolbar were the
//! pool's rows, and the person asked for them to be columns (11 October
//! 2026). The search is a field that is always open, filtering as it is
//! typed — it was a button that unfolded one — and the filters are pills
//! under an overline where a handful of choices fit as pills (the house
//! rule), with Set the one drop-down, since a format offers a dozen.
//!
//! **A press on a card in the pool adds a copy; the deck's rows take one
//! away or add one back.** The pool is what a match can deal in the
//! format the deck is built for — Settings' format to start with, and
//! "Legal in" is the first filter, deciding which sets the Set filter
//! offers (Phase 7 §10 Stage 4; there was a Show drop-down for the
//! cards the engine does not play yet, and the person said playable is
//! "the only reasonable option to build a deck"). A deck may still hold
//! an unplayable card from an import, and says it cannot be played until
//! the card is in. Each card in the pool carries its count in the deck,
//! so the pool is also the list of what is left to add. A secondary
//! click on a card, in the pool or the deck, reads it (the game's rule:
//! the right button, or Ctrl or Cmd with the primary). A hover shows
//! nothing (§5b, and again 11 October 2026).
//!
//! **Every edit is saved as it is made** (`models::deck_editor`), and the
//! verdict in the deck column is re-read with it: the validators' words,
//! in the format Settings names, and a badge lit for every format the
//! deck is legal in — the same badges the deck picker draws
//! (`widgets::format_badges`). A deck is saved whatever it says — only
//! starting a game refuses an illegal one.
//!
//! **A built-in deck opens read-only** in the same three columns: its
//! notes down the left where the filters would be, its own cards as the
//! spread in the middle, and Copy to edit in the deck column's foot.
//!
//! **The search field and the pop-ups take turns at the keyboard**
//! (`text_fields`): one `TextField` exists at a time, so while a pop-up
//! is open the search keeps its words and gives up its field, and takes
//! it back when the pop-up closes. Its Escape closes a card being read,
//! else clears the query, else leaves the screen — the field holds the
//! keys, so `nav`'s Escape would never see the editor.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use netrunner_client::card_face::Face;
use netrunner_client::cards::{faction_label, set_name};
use netrunner_client::deck_builder::{self, CardBook, PoolSort};
use netrunner_client::deck_store::{self, Origin};
use netrunner_client::settings::{format_label, FORMATS};
use netrunner_core::card::Faction;
use netrunner_core::dsl::{CardDefinition, CardId};
use netrunner_core::format::NsgFormat;

use crate::audio::{ButtonSound, PlaySfx, Sfx};
use crate::card_images::CardImages;
use crate::core::{ClientCore, TokioRuntime};
use crate::files::{Ask, DeckFiles, Done};
use crate::models::deck_editor::{Editor, Intent, Outcome};
use crate::models::decks::{Intent as ShelfIntent, Outcome as ShelfOutcome, Shelf};
use crate::nav::{screen_root, Captures, InputCaptured, Navigate};
use crate::screens::decks::{exported, spawn_standing};
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
            // The search field's Escape is the way out of the screen, so
            // its answer is read the frame the field gives it, inside
            // `Captures` after the field, where a Navigate still lands
            // this frame.
            .add_systems(Update, text_fields.in_set(Captures).after(crate::widgets::text_field::edit_text_fields).run_if(in_state(AppScreen::DeckEditor)))
            .add_systems(Update, (controls, file_answers, rebuild, fit_pool, redraw_new_art, refresh).chain().after(text_fields).run_if(in_state(AppScreen::DeckEditor)));
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
    Clear,
}

/// A filter pill: a press applies its intent. Each group's chosen pill
/// is drawn filled, so a group reads as one choice.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct FilterPill(pub Intent);

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

/// The Set filter's drop-down, the one filter too long for pills.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct SetDropdown;

#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
enum Popup {
    #[default]
    None,
    Rename,
    Identity,
}

/// The deck column's head: the name, the identity, the badges, the
/// metrics and the style — everything over the list that an edit moves.
#[derive(Component)]
struct DeckHead;
#[derive(Component)]
struct Filters;
/// The search field's node, always drawn; its `TextField` comes and goes
/// with the pop-ups (`text_fields`).
#[derive(Component, Clone, Copy)]
pub struct SearchField;
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

const MAX_QUERY_LEN: usize = 40;
/// The filter column, thin: a search field and pills two to a row.
const FILTER_WIDTH: f32 = 236.0;
/// The deck column: wide enough for the five format badges in one row
/// beside its padding, and what the list had before the columns.
const LIST_WIDTH: f32 = 400.0;

fn book(core: &ClientCore) -> CardBook<'_> {
    CardBook::new(&core.registry, &core.catalog)
}

fn format_of(core: &ClientCore) -> NsgFormat {
    core.settings.format.unwrap_or(netrunner_client::settings::DEFAULT_FORMAT)
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
            (Editor::open(stored.deck, read_only, book, format_of(core)), None)
        }
        Err(error) => {
            let fallback = netrunner_core::decks::by_id(netrunner_client::start::DEFAULT_RUNNER_DECK).expect("the default deck is built in");
            (Editor::open(fallback, true, book, format_of(core)), Some(format!("That deck could not be opened: {error}")))
        }
    };
    if let Some(notice) = notice {
        editor.note = Some(notice);
    }
    editor.filter.sort = sort;
    commands.insert_resource(Popup::None);
    commands.insert_resource(Dirty::default());
    commands.init_resource::<Rebuild>();
    commands.insert_resource(PoolFace::default());

    // The left column: the search and the filters, or a built-in
    // deck's notes.
    let left = commands.spawn(column(theme, FILTER_WIDTH)).id();
    if editor.read_only {
        let notes = commands.spawn(Node { width: percent(100), flex_direction: FlexDirection::Column, ..default() }).id();
        commands.entity(notes).with_children(|parent| spawn_notes(parent, theme, &editor));
        let scroll = scroll_column(commands, notes);
        commands.entity(left).add_child(scroll);
    } else {
        commands.entity(left).with_children(|parent| {
            parent.spawn(widgets::overline(theme, "Search"));
            parent.spawn((
                SearchField,
                TextField::new(editor.filter.query.clone(), MAX_QUERY_LEN),
                widgets::field_node(percent(100)),
                BackgroundColor(theme.glass_strong),
                BorderColor::all(theme.accent),
                children![(Text::new(format!("{}|", editor.filter.query)), theme.font(size::SMALL), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::AnyCharacter))],
            ));
        });
        let filters = commands.spawn((Filters, Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column, row_gap: px(10), ..default() })).id();
        commands.entity(filters).with_children(|parent| spawn_filters(parent, theme, core, &editor));
        commands.entity(left).add_child(filters);
    }

    // The middle: the pool, or a built-in deck's own cards as a spread.
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
    let middle = commands
        .spawn(Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0), height: percent(100), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() })
        .add_child(pool)
        .with_children(|parent| {
            parent.spawn(widgets::notice(theme, editor.note.clone().unwrap_or_default(), NoticeLine));
            let hint = if editor.read_only { "A press on a card reads it" } else { "A press on a card adds a copy · a secondary click (right button, or Ctrl-click) reads it" };
            parent.spawn(widgets::dim(theme, hint));
        })
        .id();

    // The right column: the deck — its head, its list, the buttons.
    let head = commands.spawn((DeckHead, Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: px(6), ..default() })).id();
    commands.entity(head).with_children(|parent| spawn_head(parent, theme, core, &editor, images));
    let list = commands.spawn((DeckList, Node { width: percent(100), flex_direction: FlexDirection::Column, row_gap: px(4), padding: UiRect::all(px(4)), ..default() })).id();
    commands.entity(list).with_children(|parent| spawn_deck_list(parent, theme, core, &editor));
    let list_scroll = scroll_column(commands, list);
    let listing = commands
        .spawn(Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(6), border: UiRect::top(px(1)), padding: UiRect::top(px(6)), ..default() })
        .insert(BorderColor::all(theme.glass_border))
        .add_child(list_scroll)
        .with_children(|parent| {
            parent.spawn(widgets::scrollbar(theme, list_scroll));
        })
        .id();
    let foot = commands.spawn(Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, align_items: AlignItems::Stretch, row_gap: px(8), ..default() }).id();
    commands.entity(foot).with_children(|parent| spawn_foot(parent, theme, &editor));
    let right = commands.spawn(column(theme, LIST_WIDTH)).add_child(head).add_child(listing).add_child(foot).id();

    let body = commands
        .spawn(Node { width: percent(100), max_width: px(2600), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(14), ..default() })
        .add_child(left)
        .add_child(middle)
        .add_child(right)
        .id();
    let popup = commands.spawn((PopupLayer, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), display: Display::None, ..default() })).id();
    commands.spawn(screen_root(AppScreen::DeckEditor, theme)).add_child(body).add_child(popup);
    let read_only = editor.read_only;
    commands.insert_resource(Model(editor));
    read_only
}

/// The export dialog's answer, when it comes: where the file went, or
/// why there is none. A cancelled dialog is nothing, and so is a file
/// chosen to import, which only the Decks screen asks for.
fn file_answers(mut files: ResMut<DeckFiles>, mut editor: ResMut<Model>, mut dirty: ResMut<Dirty>) {
    if let Some(Done::Saved(Some(result))) = files.poll() {
        editor.0.note = Some(exported(result));
        dirty.notice = true;
    }
}

/// A column that scrolls `content`.
fn scroll_column(commands: &mut Commands, content: Entity) -> Entity {
    commands
        .spawn((bevy::ui_widgets::ScrollArea, Node { flex_grow: 1.0, min_width: px(0), height: percent(100), flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), ..default() }))
        .add_child(content)
        .id()
}

/// A side column of the three: a fixed width, the window's height, on
/// the glass, its children stacked.
fn column(theme: &Theme, width: f32) -> impl Bundle + use<> {
    (
        Node {
            width: px(width),
            flex_shrink: 0.0,
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            padding: UiRect::all(px(10)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(crate::theme::shape::PANEL_RADIUS)),
            ..default()
        },
        BackgroundColor(theme.glass),
        BorderColor::all(theme.glass_border),
    )
}

/// The identity's face in the deck column: small, because the list
/// below it is what the column is for, and a secondary click reads the
/// card large.
const HEADER_FACE: u16 = 100;

/// The deck column's head: the name with Rename and Change identity,
/// the identity's face beside its words and the metrics, a badge per
/// format with the verdict's reason where there is one, and the style a
/// bot plays it in. It is kept short on purpose — the list under it is
/// what the column is for, and a head that took half the column left
/// three rows of the deck in view at 900 px.
fn spawn_head(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, editor: &Editor, images: &CardImages) {
    let book = book(core);
    let deck = editor.deck();
    let identity = book.get(&deck.identity);
    parent.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|row| {
        row.spawn((widgets::heading(theme, deck.name.clone()), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }, TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        if !editor.read_only {
            row.spawn(widgets::small_button(theme, ButtonKind::Quiet, "Rename", Control::Rename));
            row.spawn(widgets::small_button(theme, ButtonKind::Quiet, "Change identity", Control::ChangeIdentity));
        }
    });
    parent.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(12), align_items: AlignItems::FlexStart, ..default() }).with_children(|row| {
        match identity {
            Some(card) => {
                let drawn = Face::of(card, &core.settings.art);
                let image = images.of(&drawn, FaceSize::Board(HEADER_FACE));
                spawn_face(row, theme, &drawn, FaceSize::Board(HEADER_FACE), image, (Button, DeckRowButton::Read(card.id.clone()), Readable(card.id.clone())));
            }
            None => {
                row.spawn((Node { width: px(HEADER_FACE as f32), height: px(HEADER_FACE as f32 * 1.4), flex_shrink: 0.0, ..default() }, BackgroundColor(theme.panel)));
            }
        }
        row.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0), min_width: px(0), row_gap: px(3), ..default() }).with_children(|words| {
            let faction = identity.and_then(|card| card.faction).map_or("", faction_label);
            let read_only = if editor.read_only { " · built-in, read-only" } else { "" };
            words.spawn(widgets::dim(theme, format!("{:?} · {faction}{read_only}", deck.side)));
            words.spawn((widgets::label(theme, book.title(&deck.identity)), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            words.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(1), margin: UiRect::top(px(6)), ..default() }).with_children(|rows| {
                for (what, value) in metrics(editor, book) {
                    rows.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(8), ..default() }).with_children(|row| {
                        row.spawn((widgets::dim(theme, what), Node { width: px(104), flex_shrink: 0.0, ..default() }));
                        row.spawn((Text::new(value), theme.font(size::SMALL), TextColor(theme.text)));
                    });
                }
            });
        });
    });
    parent.spawn((widgets::overline(theme, "Legal in"), Node { margin: UiRect::top(px(4)), ..default() }));
    widgets::format_badges(parent, theme, &editor.status.legal_in);
    // The badges say where it is legal; the verdict's words are drawn
    // only where there is a reason to read.
    if !editor.status.standing.is_legal() {
        spawn_standing(parent, theme, &editor.status.standing, editor.format);
    }
    parent.spawn((widgets::overline(theme, "A bot plays it"), Node { margin: UiRect::top(px(4)), ..default() }));
    if editor.read_only {
        // A published list's style is read, not chosen — and may be a
        // stack ("glacier then fast advance") no single pill says.
        parent.spawn(widgets::label(theme, capitalised(&deck.style_label())));
        return;
    }
    parent.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, column_gap: px(6), row_gap: px(6), ..default() }).with_children(|row| {
        for style in editor.styles() {
            let name = style.map(|style| style.name().to_string());
            let label = name.as_deref().map_or("Balanced".to_string(), capitalised);
            let chosen = deck.style == name.iter().cloned().collect::<Vec<_>>();
            pill(row, theme, label, chosen, Control::Style(name));
        }
    });
}

/// The deck column's foot: the screen's buttons, Done the one primary
/// on an editable deck and Copy to edit on a built-in one.
fn spawn_foot(parent: &mut ChildSpawnerCommands, theme: &Theme, editor: &Editor) {
    if editor.read_only {
        parent.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(8), ..default() }).with_children(|row| {
            row.spawn(widgets::button(theme, "Export to file…", percent(50), Control::Export));
            row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Back", percent(50), (Control::Done, ButtonSound(Sfx::Back))));
        });
        parent.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Copy to edit", percent(100), Control::Copy));
    } else {
        parent.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(8), ..default() }).with_children(|row| {
            row.spawn(widgets::button(theme, "Export to file…", percent(50), Control::Export));
            row.spawn(widgets::button(theme, "Save a copy", percent(50), Control::Copy));
        });
        parent.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Done", percent(100), Control::Done));
        parent.spawn((widgets::dim(theme, "Every change is saved as it is made"), Node { align_self: AlignSelf::Center, ..default() }));
    }
}

/// A pill: `small_button`, filled and ringed in the accent when chosen,
/// quiet otherwise — the picker's "Legal in" pills, and every filter
/// group here.
fn pill(parent: &mut ChildSpawnerCommands, theme: &Theme, label: impl Into<String>, chosen: bool, marker: impl Bundle) {
    let kind = if chosen { ButtonKind::Secondary } else { ButtonKind::Quiet };
    let mut button = parent.spawn(widgets::small_button(theme, kind, label, marker));
    if chosen {
        button.insert(BorderColor::all(theme.accent));
    }
}

/// The running totals against the identity's limits, as rows: "Cards"
/// "44 (40 minimum)", "Influence" "15 of 15", "Agenda points" "18
/// (18–19)". One sentence of them was the header's; a column reads them
/// as a table.
fn metrics(editor: &Editor, book: CardBook) -> Vec<(String, String)> {
    let deck = editor.deck();
    let Some(tally) = editor.draft.tally(book) else {
        return vec![("Cards".to_string(), format!("{}", deck.size()))];
    };
    let mut rows = vec![("Cards".to_string(), format!("{} ({} minimum)", tally.size, tally.min_size))];
    rows.push((
        "Influence".to_string(),
        match tally.influence_limit {
            Some(limit) => format!("{} of {limit}", tally.influence_spent),
            None => format!("{}, no limit", tally.influence_spent),
        },
    ));
    if let Some(agenda) = tally.agenda {
        rows.push(("Agenda points".to_string(), format!("{} ({}–{})", agenda.points, agenda.min, agenda.max)));
    }
    rows
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

/// The Set drop-down's choices: every set the format offers
/// (`deck_builder::sets`), each under its mark, after "All sets"; the
/// intent each applies, and which is chosen now.
fn set_entries(theme: &Theme, core: &ClientCore, editor: &Editor) -> (Vec<Choice>, Vec<Intent>, usize) {
    let side = editor.deck().side;
    let mut entries: Vec<(Choice, Intent, bool)> = vec![(Choice::plain("All sets"), Intent::Set(None), editor.filter.set.is_none())];
    for set in deck_builder::sets(book(core), side, editor.filter.format) {
        let chosen = editor.filter.set.as_deref() == Some(set.as_str());
        let icon = theme.set_icon(&set, size::SMALL).map(|(glyph, _)| glyph);
        entries.push((Choice::with_icon(set_name(&set), icon, theme.text), Intent::Set(Some(set)), chosen));
    }
    let current = entries.iter().position(|(_, _, chosen)| *chosen).unwrap_or(0);
    let (choices, intents) = entries.into_iter().map(|(choice, intent, _)| (choice, intent)).unzip();
    (choices, intents, current)
}

/// A group of pills under its overline, one filled.
fn pill_group(parent: &mut ChildSpawnerCommands, theme: &Theme, title: &str, choices: Vec<(String, bool, Intent)>) {
    parent.spawn(widgets::overline(theme, title));
    parent.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(6), row_gap: px(6), ..default() }).with_children(|row| {
        for (label, chosen, intent) in choices {
            pill(row, theme, label, chosen, FilterPill(intent));
        }
    });
}

/// The filters down the column, "Legal in" first (the person's ask, 30
/// September 2026): the format decides which sets the Set filter
/// offers, so it reads before Set; then the faction and the type, which
/// narrow within the format; then how the pool is sorted; then how many
/// cards are left, and Clear.
fn spawn_filters(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, editor: &Editor) {
    let book = book(core);
    let side = editor.deck().side;
    pill_group(parent, theme, "Legal in", FORMATS.into_iter().map(|format| (format_label(format).to_string(), editor.filter.format == format, Intent::Format(format))).collect());
    let (choices, _, current) = set_entries(theme, core, editor);
    spawn_dropdown(parent, theme, "Set", choices, current, SetDropdown);
    let mut factions = vec![("All".to_string(), editor.filter.faction.is_none(), Intent::Faction(None))];
    factions.extend(deck_builder::factions(book, side).into_iter().map(|faction| (faction_label(faction).to_string(), editor.filter.faction == Some(faction), Intent::Faction(Some(faction)))));
    pill_group(parent, theme, "Faction", factions);
    let mut kinds = vec![("All".to_string(), editor.filter.kind.is_none(), Intent::Kind(None))];
    kinds.extend(deck_builder::kinds(side).iter().map(|kind| (kind.to_string(), editor.filter.kind == Some(*kind), Intent::Kind(Some(kind)))));
    pill_group(parent, theme, "Type", kinds);
    pill_group(parent, theme, "Sort by", PoolSort::ALL.into_iter().map(|sort| (sort.label().to_string(), editor.filter.sort == sort, Intent::Sort(sort))).collect());
    parent.spawn(Node { flex_grow: 1.0, ..default() });
    parent.spawn(widgets::dim(theme, format!("{} cards", editor.pool(book).len())));
    parent.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Clear filters", percent(100), Control::Clear));
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
            let drawn = Face::of(card, &core.settings.art);
            let image = images.of(&drawn, size);
            spawn_face(cell, theme, &drawn, size, image, (Button, PoolCard(card.id.clone()), Readable(card.id.clone())));
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
    (marks, rows, pills, sets, popup_buttons): (Query<&Control>, Query<&DeckRowButton>, Query<&FilterPill>, Query<(), With<SetDropdown>>, Query<&PopupButton>),
    (pool, reads, picks, wash): (
        Query<(&Interaction, &PoolCard), Changed<Interaction>>,
        Query<(&Interaction, &DeckRowButton), (Changed<Interaction>, Without<widgets::Themed>)>,
        Query<(&Interaction, &PopupButton), (Changed<Interaction>, Without<widgets::Themed>)>,
        Query<&Interaction, (Changed<Interaction>, With<Wash>)>,
    ),
    (keys, mouse, mut reading): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>, ResMut<Reading>),
    mut editor: ResMut<Model>,
    (mut popup, mut dirty, mut rebuild): (ResMut<Popup>, ResMut<Dirty>, ResMut<Rebuild>),
    mut search_field: Query<&mut TextField, With<SearchField>>,
    (core, theme): (Res<ClientCore>, Res<Theme>),
    (mut files, runtime): (ResMut<DeckFiles>, Option<Res<TokioRuntime>>),
    (mut navigate, mut sounds): (MessageWriter<Navigate>, MessageWriter<PlaySfx>),
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
        if sets.get(*dropdown).is_err() {
            continue;
        }
        let (_, set_intents, _) = set_entries(&theme, &core, &editor.0);
        if let Some(intent) = set_intents.get(*index) {
            intents.push(intent.clone());
        }
    }
    for Pressed(entity) in pressed.read() {
        if let Ok(FilterPill(intent)) = pills.get(*entity) {
            intents.push(intent.clone());
            continue;
        }
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
                // The dialog answers later, through `file_answers`.
                let text = deck_builder::export_text(editor.0.deck(), book);
                let name = editor.0.deck().name.clone();
                if let Err(error) = files.ask(runtime.as_deref(), Ask::Save { name, text }) {
                    editor.0.note = Some(error);
                    dirty.notice = true;
                }
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
            Control::Clear => {
                // The field's words go with the filters, or `text_fields`
                // would put the query straight back.
                for mut field in &mut search_field {
                    field.text.clear();
                }
                intents.push(Intent::ClearFilters);
            }
        }
    }
    for intent in intents {
        // Heard only when the deck took the card: a refused add (a third
        // copy, influence over the limit) is a note, not a click.
        let adds = matches!(intent, Intent::Add(_));
        match editor.0.apply(intent, book) {
            Outcome::Nothing => {}
            Outcome::View => {
                dirty.pool = true;
                dirty.notice = true;
            }
            Outcome::Save => {
                if adds {
                    sounds.write(PlaySfx::now(Sfx::DeckAdd));
                }
                save(&core, &mut editor.0);
                dirty.deck = true;
                dirty.notice = true;
            }
        }
    }
}

/// The search field and the rename field. One text field exists at a
/// time (`widgets::text_field`), so the search gives up its `TextField`
/// while a pop-up is open — its words stay drawn — and takes it back
/// when the pop-up closes. The search's Escape closes a card being read,
/// else clears the query, else leaves the screen; its Enter is nothing.
#[allow(clippy::too_many_arguments)]
fn text_fields(
    mut commands: Commands,
    mut renames: Query<(Entity, Option<&TextFieldEvent>), (With<TextField>, With<RenameField>)>,
    mut searches: Query<(Entity, Option<&mut TextField>, Option<&TextFieldEvent>, &Children), With<SearchField>>,
    mut texts: Query<&mut Text>,
    mut editor: ResMut<Model>,
    mut popup: ResMut<Popup>,
    mut dirty: ResMut<Dirty>,
    mut reading: ResMut<Reading>,
    mut navigate: MessageWriter<Navigate>,
    core: Res<ClientCore>,
) {
    let book = book(&core);
    for (_, event) in &mut renames {
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
    }
    for (entity, field, event, children) in &mut searches {
        let Some(mut field) = field else {
            if *popup == Popup::None {
                let query = editor.0.filter.query.clone();
                for child in children.iter() {
                    if let Ok(mut text) = texts.get_mut(child) {
                        text.0 = format!("{query}|");
                    }
                }
                commands.entity(entity).insert(TextField::new(query, MAX_QUERY_LEN));
            }
            continue;
        };
        if *popup != Popup::None {
            for child in children.iter() {
                if let Ok(mut text) = texts.get_mut(child) {
                    text.0 = field.text.clone();
                }
            }
            commands.entity(entity).remove::<(TextField, TextFieldEvent)>();
            continue;
        }
        match event {
            Some(TextFieldEvent::Committed(_)) => {
                commands.entity(entity).remove::<TextFieldEvent>();
            }
            Some(TextFieldEvent::Cancelled) => {
                commands.entity(entity).remove::<TextFieldEvent>();
                if reading.is_open() {
                    reading.0 = None;
                } else if !field.text.is_empty() {
                    field.text.clear();
                } else {
                    navigate.write(Navigate(AppScreen::Decks));
                }
            }
            None => {}
        }
        if field.text != editor.0.filter.query && editor.0.apply(Intent::Query(field.text.clone()), book) == Outcome::View {
            dirty.pool = true;
        }
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

/// A card's art chosen in the reader (`widgets::art`): every face of it
/// on the screen is drawn again — the header, the pool and a picker.
fn redraw_new_art(mut changed: MessageReader<crate::widgets::art::ArtChanged>, mut dirty: ResMut<Dirty>) {
    if changed.read().count() > 0 {
        dirty.deck = true;
        dirty.pool = true;
        dirty.popup = true;
    }
}

fn refresh(
    mut commands: Commands,
    mut dirty: ResMut<Dirty>,
    head: Query<Entity, With<DeckHead>>,
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
        if let Ok(head) = head.single() {
            commands.entity(head).despawn_children().with_children(|parent| spawn_head(parent, &theme, &core, editor, &images));
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
                        TextField::new(current.clone(), deck_builder::MAX_NAME),
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
                                    let drawn = Face::of(identity, &core.settings.art);
                                    let image = images.of(&drawn, IDENTITY_FACE);
                                    let face = spawn_face(grid, theme, &drawn, IDENTITY_FACE, image, (Button, PopupButton::Identity(identity.id.clone()), Readable(identity.id.clone())));
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
