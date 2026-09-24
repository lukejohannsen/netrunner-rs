//! A card opened to read, in the middle of the window, over whatever
//! screen it was opened on.
//!
//! **A secondary click reads a card** (the right button, or Ctrl or Cmd
//! with the primary — the board's rule), and anything marked
//! [`Readable`] answers it: the deck editor's pool, spread, deck rows
//! and header, and both identity pickers. The card is drawn at
//! `FaceSize::Large`, the board's sheet's size, **centred**, over a wash;
//! a click anywhere off the card or Escape closes it, as a sheet on the
//! board closes, and it carries no buttons.
//!
//! **Centred, where the board's reading surfaces sit at the right**
//! (§4as): that placement keeps the field in view for a second chair
//! still playing, and a deck is built by one person with nothing to
//! watch. Asked for by the person (24 September 2026), with the hover
//! preview that came before it (§5a) removed as annoying and too large.
//!
//! **One reader over every layer**, rather than a pop-up per screen: an
//! identity is read from inside the picker that chooses it, and a
//! screen's pop-up has one slot. The reader has its own, above every
//! screen's (`GlobalZIndex(30)`), so closing it leaves the picker where
//! it was. It is spawned under the screen root, so leaving the screen
//! takes it down.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use netrunner_client::card_face::Face;
use netrunner_core::dsl::CardId;

use crate::card_images::CardImages;
use crate::core::ClientCore;
use crate::nav::{Captures, InputCaptured};
use crate::screens::AppScreen;
use crate::theme::{size, Theme};
use crate::widgets::card_face::{spawn_face, FaceSize};

/// A secondary click on this reads the card.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Readable(pub CardId);

/// The card being read, if any. A screen opens one by writing it (a
/// press that reads, such as a deck row's name), and asks it before
/// taking an Escape or a press of its own.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct Reading(pub Option<CardId>);

impl Reading {
    pub fn is_open(&self) -> bool {
        self.0.is_some()
    }
}

/// The wash the card sits on; a press on it closes the reader.
#[derive(Component, Clone)]
pub struct ReaderWash;

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Reading>()
        .add_systems(Update, escape_closes.in_set(Captures))
        .add_systems(Update, (open_on_secondary_click, close_on_click_away, draw).chain());
}

/// True while a secondary click is being made: the right button, or
/// Ctrl or Cmd with the primary. A screen skips its own handling of a
/// press made this way, so a Ctrl-click that reads a pool card does not
/// also add it.
pub fn secondary_click(keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
    let modifier = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight]);
    mouse.just_pressed(MouseButton::Right) || (modifier && mouse.just_pressed(MouseButton::Left))
}

fn open_on_secondary_click(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    readable: Query<(&Interaction, &Readable)>,
    mut reading: ResMut<Reading>,
    dev: Option<Res<crate::dev::Dev>>,
    mut forced: Local<bool>,
) {
    // `NETRUNNER_READ`: the nth readable card is opened once it exists.
    if !*forced
        && let Some(n) = dev.and_then(|dev| dev.read)
        && let Some((_, Readable(id))) = readable.iter().nth(n - 1)
    {
        reading.0 = Some(id.clone());
        *forced = true;
    }
    if reading.is_open() || !secondary_click(&keys, &mouse) {
        return;
    }
    if let Some((_, Readable(id))) = readable.iter().find(|(interaction, _)| matches!(interaction, Interaction::Hovered | Interaction::Pressed)) {
        reading.0 = Some(id.clone());
    }
}

fn close_on_click_away(wash: Query<&Interaction, (Changed<Interaction>, With<ReaderWash>)>, mut reading: ResMut<Reading>) {
    if wash.iter().any(|interaction| *interaction == Interaction::Pressed) {
        reading.0 = None;
    }
}

fn escape_closes(keys: Res<ButtonInput<KeyCode>>, mut captured: ResMut<InputCaptured>, mut reading: ResMut<Reading>) {
    if !reading.is_open() || captured.0 {
        return;
    }
    captured.0 = true;
    if keys.just_pressed(KeyCode::Escape) {
        reading.0 = None;
    }
}

/// Spawns the reader when a card is opened and despawns it when it is
/// closed, or when the screen it was drawn on has gone.
fn draw(
    mut commands: Commands,
    reading: Res<Reading>,
    shown: Query<Entity, With<ReaderWash>>,
    roots: Query<Entity, (With<DespawnOnExit<AppScreen>>, With<Node>)>,
    (theme, core, images): (Option<Res<Theme>>, Option<Res<ClientCore>>, Option<Res<CardImages>>),
) {
    if !reading.is_changed() {
        return;
    }
    for entity in &shown {
        commands.entity(entity).despawn();
    }
    let (Some(id), Some(theme), Some(core), Some(images), Some(root)) = (&reading.0, theme, core, images, roots.iter().next()) else { return };
    let Some(card) = core.registry.get(id) else { return };
    let image = card.numeric_id.and_then(|code| images.face(code, FaceSize::Large));
    commands.entity(root).with_children(|parent| {
        parent
            .spawn((
                ReaderWash,
                // `Block` said outright, as the board's overlay says it
                // (`screens::game`): `Node` requires `FocusPolicy` and
                // defaults it to `Pass`, which would let a press through
                // to the pool behind. The `Interaction` is what a click
                // away is read from.
                FocusPolicy::Block,
                Interaction::None,
                GlobalZIndex(30),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    width: percent(100),
                    height: percent(100),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(theme.wash.with_alpha(0.72)),
            ))
            .with_children(|wash| {
                // The card blocks the press, so only a click that misses
                // it closes the reader.
                wash.spawn((Interaction::None, FocusPolicy::Block, Node { flex_direction: FlexDirection::Column, row_gap: px(8), ..default() })).with_children(|column| {
                    spawn_face(column, &theme, &Face::of(card), FaceSize::Large, image, ());
                    if !card.is_playable {
                        column.spawn((Text::new("The engine does not play this card yet."), theme.font(size::SMALL), TextColor(theme.danger)));
                    }
                });
            });
    });
}

