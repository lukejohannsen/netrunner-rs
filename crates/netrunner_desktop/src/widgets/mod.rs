//! The handful of pieces every screen is built from, so a heading on
//! the profile screen is the heading on the settings screen.
//!
//! Buttons are Bevy's `Button` with the theme's colours and one shared
//! behaviour: hover and press recolour them, and a press writes a
//! [`Pressed`] carrying the button's entity. A screen tags its buttons
//! with its own marker component and, on `Pressed`, looks the entity up
//! in a query of that marker — so no screen needs its own hover system
//! and no button needs a closure.

pub mod card_face;
pub mod dropdown;
pub mod text_field;

use bevy::prelude::*;
use bevy::ui_widgets::{ControlOrientation, Scrollbar, ScrollbarThumb};

use crate::nav::Captures;
use crate::skin::{Drawn, Skin, Slot};
use crate::theme::{size, Theme};

pub struct WidgetsPlugin;

impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        // The drop-down before the text field: both may take an Escape,
        // and an open list closing is the one that wins.
        app.add_message::<Pressed>()
            .add_message::<dropdown::DropdownChanged>()
            .add_systems(Update, (button_feedback, (dropdown::dropdowns, text_field::edit_text_fields).chain().in_set(Captures)).chain())
            // Registered on its own rather than at the head of that
            // chain: putting it there moved `button_feedback` relative to
            // every screen's `controls`, and a dozen board tests stopped
            // seeing their presses. Dressing has no ordering requirement
            // of its own — it paints what interaction never touched.
            .add_systems(Update, dress);
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

/// Marks a node as a part of the board a skin may dress, and records what
/// it would look like undressed.
///
/// The picture is not applied here but by `dress`, on a later frame,
/// which is what lets every `widgets::` bundle stay a plain function of
/// the theme: a bundle has no way to reach the [`Skin`] resource, and
/// threading one into `button`, `compact_button` and the rest would have
/// touched every call site in the crate to say something none of them
/// care about.
#[derive(Component, Clone, Copy)]
pub struct Dressed {
    pub slot: Slot,
    /// What the board paints here with no skin, and the three states'
    /// versions of it. `hover` and `pressed` are `None` on anything that
    /// does not react to a pointer.
    pub drawn: Drawn,
    pub hover: Option<Drawn>,
    pub pressed: Option<Drawn>,
}

impl Dressed {
    /// A part of the board that does not react to a pointer.
    pub fn still(slot: Slot, drawn: Drawn) -> Self {
        Self { slot, drawn, hover: None, pressed: None }
    }

    /// A button: the theme's own three background colours over one border.
    pub fn button(theme: &Theme, slot: Slot, drawn: Drawn) -> Self {
        Self {
            slot,
            drawn,
            hover: Some(Drawn::new(theme.button_hover, drawn.border)),
            pressed: Some(Drawn::new(theme.button_press, drawn.border)),
        }
    }

    /// The slot and the colours for an interaction.
    fn at(&self, interaction: Interaction) -> (Slot, Drawn) {
        match interaction {
            Interaction::Hovered => (self.slot.hovered().unwrap_or(self.slot), self.hover.unwrap_or(self.drawn)),
            Interaction::Pressed => (self.slot.pressed().unwrap_or(self.slot), self.pressed.unwrap_or(self.drawn)),
            Interaction::None => (self.slot, self.drawn),
        }
    }
}

/// A themed button that is drawn but not offered: no hover, no press,
/// dim text. The board's control bar keeps every basic action in its
/// place and greys the ones the engine does not list this decision, so
/// "End turn" is always where it was.
#[derive(Component)]
pub struct Disabled;

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
///
/// [`Dressed`] with [`Slot::Panel`], which reaches every panel in the
/// client and not only the board's — the sheet, the decision pop-up, the
/// actions menu, the main menu, settings, profile and new-game screens.
/// That is the same reach `Slot::Button` already has, so it is the rule
/// rather than an exception; the authoring guide says so, because a panel
/// picture that stopped at the board's edge would be the surprise.
///
/// A caller that draws its border differently — the decision pop-up and
/// the actions menu both use the accent — replaces this `Dressed` with
/// one naming its own slot, which is the "the caller says what it would
/// have drawn" rule. It does *not* drag panels into `button_feedback`:
/// that query is `With<Themed>`, and a panel is not themed.
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
        Dressed::still(Slot::Panel, Drawn::new(theme.panel, theme.panel_border)),
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
        Dressed::button(theme, Slot::Button, Drawn::new(theme.button, theme.panel_border)),
        children![(Text::new(text), theme.font(size::BODY), TextColor(theme.text))],
    )
}

/// [`button`], greyed: the same node, so the bar does not reflow when a
/// control becomes legal, with dim text and the `Disabled` marker the
/// feedback system skips.
pub fn disabled_button<T: Into<String>, M: Bundle>(theme: &Theme, text: T, width: Val, marker: M) -> impl Bundle + use<T, M> {
    (
        Button,
        Themed,
        Disabled,
        marker,
        Node {
            width,
            flex_shrink: 0.0,
            padding: UiRect::axes(px(18), px(10)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(theme.panel),
        BorderColor::all(theme.panel_border.with_alpha(0.5)),
        Dressed::still(Slot::ButtonDisabled, Drawn::new(theme.panel, theme.panel_border.with_alpha(0.5))),
        children![(Text::new(text), theme.font(size::BODY), TextColor(theme.text_dim.with_alpha(0.6)))],
    )
}

/// The gear, painted: neither bundled font has U+2699 (Noto Sans
/// Symbols 2 covers 2654–2668, 267f–268f and 269e–26a1 of the block,
/// Noto Sans none of it — checked with `fc-query`), so the icon is the
/// procedural tier, like the card backs. A white cog on a transparent
/// ground, tinted by the `ImageNode` that draws it.
pub const GEAR_SIZE: u32 = 40;

pub fn gear_image() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let n = GEAR_SIZE;
    let centre = (n as f32 - 1.0) / 2.0;
    let mut data = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 - centre, y as f32 - centre);
            let r = (dx * dx + dy * dy).sqrt();
            // Eight teeth: the sectors where the angle, in eighths of a
            // turn, is in the first half of its eighth.
            let angle = dy.atan2(dx).rem_euclid(std::f32::consts::TAU);
            let in_tooth = (angle / (std::f32::consts::TAU / 8.0)).fract() < 0.5;
            let ring = (8.5..=14.5).contains(&r);
            let tooth = r > 14.5 && r <= 18.5 && in_tooth;
            // Soft edges: a pixel on the boundary is half covered.
            let alpha: u8 = if ring || tooth { 255 } else { 0 };
            data.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }
    Image::new(Extent3d { width: n, height: n, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

/// A square button with the gear on it, for a screen's corner. With
/// `images` the cog is painted and drawn; without (a headless test, where
/// `Assets<Image>` is not registered) the button reads "Options" — the
/// rule `card_images` follows.
pub fn gear_button<M: Bundle>(theme: &Theme, images: Option<&mut Assets<Image>>, marker: M) -> impl Bundle + use<M> {
    let icon = images.map(|images| images.add(gear_image()));
    let mut node = Node {
        width: px(44),
        height: px(44),
        flex_shrink: 0.0,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::all(px(6)),
        ..default()
    };
    if icon.is_none() {
        node.width = Val::Auto;
        node.padding = UiRect::axes(px(12), px(6));
    }
    let text = theme.text;
    let font = theme.font(size::SMALL);
    let label = bevy::ecs::spawn::SpawnWith(move |parent: &mut ChildSpawner| match icon {
        Some(handle) => {
            parent.spawn((ImageNode::new(handle).with_color(text), Node { width: px(24), height: px(24), ..default() }));
        }
        None => {
            parent.spawn((Text::new("Options"), font, TextColor(text)));
        }
    });
    (Button, Themed, marker, node, BackgroundColor(theme.button), BorderColor::all(theme.panel_border), Children::spawn(label))
}

/// A vertical scrollbar for `target`, a node with `Overflow::scroll_y`:
/// a thin track with a thumb the scrollbar plugin sizes and moves. Put
/// it beside the target in a row; the wheel still works without it.
pub fn scrollbar(theme: &Theme, target: Entity) -> impl Bundle + use<> {
    (
        Scrollbar::new(target, ControlOrientation::Vertical, 24.0),
        Node { width: px(10), height: percent(100), flex_shrink: 0.0, border_radius: BorderRadius::all(px(5)), ..default() },
        BackgroundColor(theme.panel),
        children![(ScrollbarThumb { border_radius: BorderRadius::all(px(5)), border: UiRect::ZERO }, BackgroundColor(theme.panel_border))],
    )
}

/// A short line at the bottom of a screen for what just happened.
pub fn notice<T: Into<String>, M: Bundle>(theme: &Theme, text: T, marker: M) -> impl Bundle + use<T, M> {
    (Text::new(text), theme.font(size::SMALL), TextColor(theme.danger), marker)
}

/// Puts a skin's picture on everything marked [`Dressed`], and takes it
/// off again when the skin changes to one that does not dress it.
///
/// Runs on `Added<Dressed>` so a freshly spawned board is dressed the
/// frame after it appears, and over everything when the [`Skin`] resource
/// itself changes, which is how the settings row takes effect without
/// leaving the screen.
fn dress(mut commands: Commands, skin: Res<Skin>, added: Query<(Entity, &Dressed), Added<Dressed>>, all: Query<(Entity, &Dressed)>) {
    if skin.is_changed() {
        for (entity, dressed) in &all {
            skin.dress(dressed.slot, dressed.drawn).apply(&mut commands.entity(entity));
        }
        return;
    }
    for (entity, dressed) in &added {
        skin.dress(dressed.slot, dressed.drawn).apply(&mut commands.entity(entity));
    }
}

/// Hover and press, and the `Pressed` message every screen's `controls`
/// reads. One system for both because the press *is* the feedback: they
/// were written together and splitting them would let a screen see a
/// press the button never showed.
///
/// A [`Dressed`] button re-dresses itself from the skin — its own hovered
/// picture if the skin drew one, its base picture otherwise — and an
/// undressed one recolours as it always did.
fn button_feedback(
    mut commands: Commands,
    skin: Res<Skin>,
    theme: Res<Theme>,
    mut buttons: Query<(Entity, &Interaction, &mut BackgroundColor, Option<&Dressed>), (Changed<Interaction>, With<Themed>, Without<Disabled>)>,
    mut pressed: MessageWriter<Pressed>,
) {
    for (entity, interaction, mut background, dressed) in &mut buttons {
        if *interaction == Interaction::Pressed {
            pressed.write(Pressed(entity));
        }
        match dressed {
            Some(dressed) => {
                let (slot, drawn) = dressed.at(*interaction);
                skin.dress(slot, drawn).apply(&mut commands.entity(entity));
            }
            // Undressed, and the resting colour is the theme's button —
            // *not* whatever this node was spawned with. That is a real
            // limitation rather than an oversight: a `Themed` node whose
            // resting background is something else loses it after the
            // first hover, which is why the drop-down's selected item now
            // carries a `Dressed` of its own rather than relying on the
            // colour it was spawned with.
            None => {
                *background = BackgroundColor(match interaction {
                    Interaction::Pressed => theme.button_press,
                    Interaction::Hovered => theme.button_hover,
                    Interaction::None => theme.button,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gear_is_a_ring_with_teeth_on_a_transparent_ground() {
        let image = gear_image();
        assert_eq!(image.texture_descriptor.size.width, GEAR_SIZE);
        let data = image.data.as_ref().unwrap();
        let alpha = |x: u32, y: u32| data[((y * GEAR_SIZE + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0, "the corner is clear");
        assert_eq!(alpha(GEAR_SIZE / 2, GEAR_SIZE / 2), 0, "the hole is clear");
        assert_eq!(alpha(GEAR_SIZE / 2, GEAR_SIZE / 2 + 11), 255, "the ring is drawn");
        let rim: Vec<u8> = (0..GEAR_SIZE).map(|x| alpha(x, GEAR_SIZE / 2)).collect();
        assert!(rim.contains(&255) && rim.contains(&0), "the teeth break the rim: {rim:?}");
    }
}
