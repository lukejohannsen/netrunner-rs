//! The handful of pieces every screen is built from, so a heading on
//! the profile screen is the heading on the settings screen.
//!
//! Buttons are Bevy's `Button` with the theme's colours and one shared
//! behaviour: hover and press recolour them, and a press writes a
//! [`Pressed`] carrying the button's entity. A screen tags its buttons
//! with its own marker component and, on `Pressed`, looks the entity up
//! in a query of that marker — so no screen needs its own hover system
//! and no button needs a closure.

pub mod text_field;

use bevy::prelude::*;

use crate::theme::{size, Theme};

pub struct WidgetsPlugin;

impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Pressed>().add_systems(Update, (button_feedback, text_field::edit_text_fields));
    }
}

/// A themed button was pressed. `Pressed(entity)`: the button's entity,
/// which carries whatever marker the screen gave it.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pressed(pub Entity);

/// Marks a button as one of ours, so the feedback system recolours only
/// themed buttons.
#[derive(Component)]
pub struct Themed;

pub fn heading<T: Into<String>>(theme: &Theme, text: T) -> impl Bundle + use<T> {
    (Text::new(text), theme.font(size::HEADING), TextColor(theme.text))
}

pub fn title<T: Into<String>>(theme: &Theme, text: T) -> impl Bundle + use<T> {
    (Text::new(text), theme.font(size::TITLE), TextColor(theme.accent))
}

pub fn label<T: Into<String>>(theme: &Theme, text: T) -> impl Bundle + use<T> {
    (Text::new(text), theme.font(size::BODY), TextColor(theme.text))
}

pub fn dim<T: Into<String>>(theme: &Theme, text: T) -> impl Bundle + use<T> {
    (Text::new(text), theme.font(size::SMALL), TextColor(theme.text_dim))
}

/// A bordered panel that stacks its children vertically.
pub fn panel(theme: &Theme, width: Val) -> impl Bundle + use<> {
    (
        Node {
            width,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(16)),
            row_gap: px(8),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(8)),
            ..default()
        },
        BackgroundColor(theme.panel),
        BorderColor::all(theme.panel_border),
    )
}

/// A row that lays its children out left to right.
pub fn row(gap: f32) -> impl Bundle + use<> {
    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(gap), ..default() }
}

/// A themed button with `text` on it and `marker` for the screen to find
/// it by. `width` `Val::Auto` fits the text.
pub fn button<T: Into<String>, M: Bundle>(theme: &Theme, text: T, width: Val, marker: M) -> impl Bundle + use<T, M> {
    (
        Button,
        Themed,
        marker,
        Node {
            width,
            // A node shrinks by default when its row overflows, by an amount
            // that depends on its neighbours — which put every main-menu
            // button at a different width, each squeezed by the length of
            // its own blurb. A button's width is a decision, never a
            // neighbour's to take.
            flex_shrink: 0.0,
            padding: UiRect::axes(px(18), px(10)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(theme.button),
        BorderColor::all(theme.panel_border),
        children![(Text::new(text), theme.font(size::BODY), TextColor(theme.text))],
    )
}

/// A short line at the bottom of a screen for what just happened.
pub fn notice<T: Into<String>, M: Bundle>(theme: &Theme, text: T, marker: M) -> impl Bundle + use<T, M> {
    (Text::new(text), theme.font(size::SMALL), TextColor(theme.danger), marker)
}

fn button_feedback(
    theme: Res<Theme>,
    mut buttons: Query<(Entity, &Interaction, &mut BackgroundColor), (Changed<Interaction>, With<Themed>)>,
    mut pressed: MessageWriter<Pressed>,
) {
    for (entity, interaction, mut background) in &mut buttons {
        *background = BackgroundColor(match interaction {
            Interaction::Pressed => {
                pressed.write(Pressed(entity));
                theme.button_press
            }
            Interaction::Hovered => theme.button_hover,
            Interaction::None => theme.button,
        });
    }
}
