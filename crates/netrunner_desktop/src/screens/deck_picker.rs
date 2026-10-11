//! The deck picker: one pop-up, over whatever screen asked, that lists a
//! side's decks and hands back the one chosen (Phase 7 §16, 11 October
//! 2026).
//!
//! **Every place a deck is chosen opens this** — the new-game form's two
//! deck slots, the Server page's seek and its tournament entry, and the
//! hub and the dashboard when they come (§14, §15) — by writing
//! [`OpenDeckPicker`] with what it is choosing for; the choice comes back
//! as [`DeckPicked`] carrying the same [`Purpose`], so a screen reads only
//! its own. It replaced a drop-down of names on each of those pages: a
//! name told nobody which deck it was, and a list long enough to need
//! the keys had nowhere to put a reason.
//!
//! **A row is the deck as a person knows it**: the identity's face, the
//! deck's name over the identity's, the faction's colour down its edge,
//! which formats it is legal in, and — dimmed, with the reason — why it
//! cannot be played at this table, where it is not a button. The model is
//! `netrunner_client::deck_picker::Picker`, which the terminal lists too;
//! this file is the drawing and the buttons. A secondary click on a row
//! reads its identity (`widgets::reader`).
//!
//! **Choosing writes the active deck** (`Settings::set_active_deck`), so
//! the next form starts from it. **A card read over the picker takes the
//! Escape first**, and the picker takes it before the screen.
//!
//! **Over every layer like the reader**: spawned under the screen root,
//! so leaving the screen takes it down, at a Z of its own above a
//! screen's pop-up (20) and below the reader (30).

use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use netrunner_client::card_face::Face;
use netrunner_client::deck_builder::CardBook;
use netrunner_client::deck_picker::{self, Intent, Outcome, Picker};
use netrunner_client::settings::{format_label, FORMATS};
use netrunner_client::start::Pane;
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

use crate::audio::{ButtonSound, Sfx};
use crate::card_images::CardImages;
use crate::core::ClientCore;
use crate::nav::{Captures, InputCaptured, Navigate};
use crate::screens::AppScreen;
use crate::theme::{size, Theme};
use crate::widgets::card_face::{spawn_face, FaceSize};
use crate::widgets::reader::Readable;
use crate::widgets::text_field::{edit_text_fields, TextField, TextFieldEvent};
use crate::widgets::{self, ButtonKind, Pressed};

/// What a screen is choosing a deck for, so it reads only its own
/// [`DeckPicked`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// The new-game form's own or opponent's deck.
    NewGame(Pane),
    /// A lobby's seek, one side.
    Lobby(Side),
    /// A tournament's entry, one side.
    Tournament(Side),
}

/// Open the picker for `side`'s decks at a table in `format`, the cursor
/// on `chosen` (the deck the asking form holds) when it names one.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct OpenDeckPicker {
    pub side: Side,
    pub format: NsgFormat,
    pub purpose: Purpose,
    pub chosen: Option<String>,
}

/// A deck was chosen, by id, for the purpose it was opened for.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct DeckPicked {
    pub purpose: Purpose,
    pub id: String,
}

/// Open a new deck in the builder for this side: the Decks screen reads
/// it on entry and goes straight to the identity picker.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewDeckFor(pub Side);

/// The picker while it is open, with what it is for and the last refusal.
#[derive(Resource, Debug, Clone)]
pub struct Picking {
    pub picker: Picker,
    pub purpose: Purpose,
    pub notice: Option<String>,
}

/// The wash the pop-up sits on; a press on it closes the picker.
#[derive(Component)]
pub struct PickerWash;

/// A row that can be chosen: its index into the picker's rows.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickerRow(pub usize);

/// The search field.
#[derive(Component)]
pub struct PickerSearch;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerControl {
    Legal(Option<NsgFormat>),
    New,
    Cancel,
}

/// The pop-up is drawn again.
#[derive(Resource, Default)]
struct Redraw(bool);

pub(crate) fn plugin(app: &mut App) {
    app.add_message::<OpenDeckPicker>()
        .add_message::<DeckPicked>()
        .init_resource::<Redraw>()
        .add_systems(Update, escape_closes.in_set(Captures).after(edit_text_fields))
        .add_systems(Update, (open, controls, search, draw).chain());
}

fn open(mut commands: Commands, mut asked: MessageReader<OpenDeckPicker>, core: Res<ClientCore>, mut redraw: ResMut<Redraw>) {
    let Some(ask) = asked.read().last() else { return };
    let book = CardBook::new(&core.registry, &core.catalog);
    let (rows, _problems) = deck_picker::rows(core.decks_dir.as_deref(), book, ask.format);
    let picker = Picker::open(ask.side, ask.format, deck_picker::rows_for(ask.side, &rows), ask.chosen.as_deref());
    commands.insert_resource(Picking { picker, purpose: ask.purpose, notice: None });
    redraw.0 = true;
}

/// Escape closes the picker before the screen takes it, unless a card
/// is being read over it or the search field took it (the field holds
/// the capture while it exists, and cancels itself on Escape — that
/// cancel is `search`'s to turn into a close).
fn escape_closes(keys: Res<ButtonInput<KeyCode>>, mut captured: ResMut<InputCaptured>, picking: Option<Res<Picking>>, mut commands: Commands, reading: Res<crate::widgets::reader::Reading>) {
    if picking.is_none() || captured.0 || reading.is_open() {
        return;
    }
    captured.0 = true;
    if keys.just_pressed(KeyCode::Escape) {
        commands.remove_resource::<Picking>();
    }
}

#[allow(clippy::too_many_arguments)]
fn controls(
    mut commands: Commands,
    mut pressed: MessageReader<Pressed>,
    rows: Query<&PickerRow>,
    marks: Query<&PickerControl>,
    wash: Query<&Interaction, (Changed<Interaction>, With<PickerWash>)>,
    picking: Option<ResMut<Picking>>,
    mut core: ResMut<ClientCore>,
    mut picked: MessageWriter<DeckPicked>,
    mut navigate: MessageWriter<Navigate>,
    mut redraw: ResMut<Redraw>,
) {
    let Some(mut picking) = picking else {
        pressed.clear();
        return;
    };
    let mut close = wash.iter().any(|interaction| *interaction == Interaction::Pressed);
    for Pressed(entity) in pressed.read() {
        if let Ok(PickerRow(index)) = rows.get(*entity) {
            match picking.picker.apply(Intent::Choose(*index)) {
                Outcome::Chosen(id) => {
                    let side = picking.picker.side;
                    if core.settings.set_active_deck(side, &id) {
                        // A file that cannot be written loses nothing
                        // here: the deck is still picked for this game.
                        let _ = core.save_settings();
                    }
                    picked.write(DeckPicked { purpose: picking.purpose, id });
                    close = true;
                }
                Outcome::Refused(reason) => {
                    picking.notice = Some(reason);
                    redraw.0 = true;
                }
                Outcome::Changed | Outcome::Nothing => {}
            }
            continue;
        }
        match marks.get(*entity) {
            Ok(PickerControl::Legal(format)) => {
                if picking.picker.apply(Intent::Legal(*format)) == Outcome::Changed {
                    redraw.0 = true;
                }
            }
            Ok(PickerControl::New) => {
                commands.insert_resource(NewDeckFor(picking.picker.side));
                navigate.write(Navigate(AppScreen::Decks));
                close = true;
            }
            Ok(PickerControl::Cancel) => close = true,
            Err(_) => {}
        }
    }
    if close {
        commands.remove_resource::<Picking>();
        redraw.0 = true;
    }
}

/// The search field narrows the list as it is typed; Enter chooses the
/// row under the cursor, and the field's Escape closes the picker.
fn search(mut commands: Commands, fields: Query<(Entity, &TextField, Option<&TextFieldEvent>), With<PickerSearch>>, picking: Option<ResMut<Picking>>, mut core: ResMut<ClientCore>, mut picked: MessageWriter<DeckPicked>, mut redraw: ResMut<Redraw>) {
    let Some(mut picking) = picking else { return };
    for (entity, field, event) in &fields {
        if picking.picker.apply(Intent::Query(field.text.clone())) == Outcome::Changed {
            redraw.0 = true;
        }
        match event {
            Some(TextFieldEvent::Committed(_)) => {
                commands.entity(entity).remove::<TextFieldEvent>();
                match picking.picker.apply(Intent::ChooseCursor) {
                    Outcome::Chosen(id) => {
                        let side = picking.picker.side;
                        if core.settings.set_active_deck(side, &id) {
                            let _ = core.save_settings();
                        }
                        picked.write(DeckPicked { purpose: picking.purpose, id });
                        commands.remove_resource::<Picking>();
                        redraw.0 = true;
                    }
                    Outcome::Refused(reason) => {
                        picking.notice = Some(reason);
                        redraw.0 = true;
                    }
                    Outcome::Changed | Outcome::Nothing => {}
                }
            }
            Some(TextFieldEvent::Cancelled) => {
                commands.remove_resource::<Picking>();
                redraw.0 = true;
            }
            None => {}
        }
    }
}

/// The face a row draws its identity with, and the size.
const ROW_FACE: u16 = 72;
const PANEL_WIDTH: f32 = 900.0;

/// Spawns the pop-up when the picker opens or changes, and despawns it
/// when it closes or the screen it was drawn on has gone.
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    mut redraw: ResMut<Redraw>,
    picking: Option<Res<Picking>>,
    shown: Query<Entity, With<PickerWash>>,
    roots: Query<Entity, (With<DespawnOnExit<AppScreen>>, With<Node>)>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    images: Res<CardImages>,
    mut art_changed: MessageReader<crate::widgets::art::ArtChanged>,
) {
    let art_changed = art_changed.read().count() > 0;
    if !std::mem::take(&mut redraw.0) && !art_changed {
        return;
    }
    for entity in &shown {
        commands.entity(entity).despawn();
    }
    let (Some(picking), Some(root)) = (picking, roots.iter().next()) else { return };
    let picker = &picking.picker;
    let visible = picker.visible();
    commands.entity(root).with_children(|parent| {
        parent
            .spawn((
                PickerWash,
                Button,
                GlobalZIndex(25),
                Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                BackgroundColor(theme.wash.with_alpha(0.72)),
            ))
            .with_children(|wash| {
                let mut panel = wash.spawn((Interaction::None, FocusPolicy::Block, widgets::roomy_panel(&theme, px(PANEL_WIDTH))));
                panel.entry::<Node>().and_modify(|mut node| node.max_height = percent(90));
                panel.with_children(|panel| {
                    let (what, side) = match picking.purpose {
                        Purpose::NewGame(Pane::OwnDeck) => ("Your deck", picker.side),
                        Purpose::NewGame(_) => ("The opponent's deck", picker.side),
                        Purpose::Lobby(side) => ("Your deck for the lobby", side),
                        Purpose::Tournament(side) => ("Your deck for the tournament", side),
                    };
                    panel.spawn(widgets::heading(&theme, format!("{what} · {side:?} · {}", format_label(picker.format))));
                    panel.spawn((widgets::dim(&theme, "A deck that cannot be played at this table is greyed out with the reason. Right-click an identity to read it."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    // The search field and the format pills on one row.
                    panel.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, column_gap: px(10), row_gap: px(8), ..default() }).with_children(|row| {
                        row.spawn(Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(220), ..default() }).with_children(|slot| {
                            slot.spawn((
                                PickerSearch,
                                TextField::new(picker.query.clone(), 60),
                                widgets::field_node(percent(100)),
                                BackgroundColor(theme.glass_strong),
                                BorderColor::all(theme.accent),
                                children![(Text::new(format!("{}|", picker.query)), theme.font(size::BODY), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::AnyCharacter))],
                            ));
                        });
                        row.spawn(widgets::dim(&theme, "Legal in"));
                        for (label, format) in std::iter::once(("Any", None)).chain(FORMATS.map(|format| (format_label(format), Some(format)))) {
                            let kind = if picker.legal == format { ButtonKind::Secondary } else { ButtonKind::Quiet };
                            let mut button = row.spawn(widgets::small_button(&theme, kind, label, PickerControl::Legal(format)));
                            if picker.legal == format {
                                button.insert(BorderColor::all(theme.accent));
                            }
                        }
                    });
                    if let Some(notice) = &picking.notice {
                        panel.spawn((widgets::notice(&theme, notice.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    }
                    let list = panel
                        .spawn((bevy::ui_widgets::ScrollArea, Node { width: percent(100), flex_shrink: 1.0, min_height: px(0), flex_direction: FlexDirection::Column, row_gap: px(8), overflow: Overflow::scroll_y(), ..default() }))
                        .with_children(|list| {
                            if visible.is_empty() {
                                list.spawn(widgets::label(&theme, if picker.rows.is_empty() { "No deck for this side. Build one in the deck builder." } else { "No deck matches." }));
                            }
                            for index in visible {
                                spawn_row(list, &theme, &core, &images, picker, index);
                            }
                        })
                        .id();
                    panel.spawn(Node { width: percent(100), flex_shrink: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(4), ..default() }).add_child(list).with_children(|row| {
                        row.spawn(widgets::scrollbar(&theme, list));
                    });
                    panel.spawn(Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::FlexEnd, column_gap: px(10), margin: UiRect::top(px(8)), ..default() }).with_children(|row| {
                        row.spawn(widgets::styled_button(&theme, ButtonKind::Secondary, "New deck in the builder…", Val::Auto, PickerControl::New));
                        row.spawn(widgets::styled_button(&theme, ButtonKind::Quiet, "Cancel", Val::Auto, (PickerControl::Cancel, ButtonSound(Sfx::Back))));
                    });
                });
            });
    });
}

/// One deck: a button when it can be played here, a dimmed card with
/// its reason when it cannot. The identity's face at the left, the
/// faction's colour down the edge, the words beside.
fn spawn_row(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, picker: &Picker, index: usize) {
    let row = &picker.rows[index];
    let playable = row.playable();
    let identity = core.registry.get(&row.deck.identity).or_else(|| core.catalog.iter().find(|card| card.id == row.deck.identity));
    let faction = theme.faction(row.faction);
    let alpha = if playable { 1.0 } else { 0.45 };
    let on_cursor = picker.cursor == index;
    let node = Node {
        width: percent(100),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: px(14),
        padding: UiRect::all(px(10)),
        border: UiRect { left: px(4), right: px(1), top: px(1), bottom: px(1) },
        border_radius: BorderRadius::all(px(12)),
        ..default()
    };
    let border = BorderColor { left: faction.with_alpha(alpha), right: theme.glass_border, top: theme.glass_border, bottom: theme.glass_border };
    let mut entity = if playable {
        let mut entity = parent.spawn((Button, widgets::Themed, PickerRow(index), ButtonSound(Sfx::Toggle), node, BackgroundColor(theme.glass_strong), border));
        if on_cursor {
            entity.insert(BorderColor { left: faction, right: theme.accent, top: theme.accent, bottom: theme.accent });
        }
        entity
    } else {
        parent.spawn((node, BackgroundColor(theme.glass.with_alpha(0.5)), border))
    };
    entity.with_children(|tile| {
        match identity {
            Some(card) => {
                let drawn = Face::of(card, &core.settings.art);
                let image = images.of(&drawn, FaceSize::Board(ROW_FACE));
                spawn_face(tile, theme, &drawn, FaceSize::Board(ROW_FACE), image, Readable(card.id.clone()));
            }
            None => {
                tile.spawn((Node { width: px(ROW_FACE as f32), height: px(ROW_FACE as f32 * 1.4), flex_shrink: 0.0, ..default() }, BackgroundColor(theme.panel)));
            }
        }
        tile.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0), min_width: px(0), row_gap: px(3), ..default() }).with_children(|words| {
            let text = if playable { theme.text } else { theme.text_dim };
            let saved = if row.saved { "  ·  yours" } else { "" };
            words.spawn((Text::new(format!("{}{saved}", row.deck.name)), theme.font(size::BODY), TextColor(text)));
            words.spawn(widgets::dim(theme, row.identity.clone()));
            words.spawn(widgets::dim(theme, format!("{} cards  ·  a bot plays it {}", row.deck.size(), row.style())));
            words.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(6), row_gap: px(4), margin: UiRect::top(px(2)), ..default() }).with_children(|badges| {
                for format in FORMATS {
                    let legal = row.status.legal_in.contains(&format);
                    let colour = if legal { theme.legal } else { theme.text_dim.with_alpha(0.5) };
                    badges.spawn((
                        Text::new(format_label(format)),
                        theme.font(size::SMALL),
                        TextColor(colour),
                        Node { padding: UiRect::axes(px(6), px(1)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(8)), ..default() },
                        BorderColor::all(colour.with_alpha(0.6)),
                    ));
                }
            });
            if let Some(problem) = row.problem() {
                words.spawn((Text::new(format!("Can't be played here: {problem}")), theme.font(size::SMALL), TextColor(theme.caution)));
            }
        });
    });
}
