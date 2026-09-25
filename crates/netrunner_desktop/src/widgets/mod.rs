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
pub mod reader;
pub mod text_field;

use bevy::prelude::*;
use bevy::ui_widgets::{ControlOrientation, Scrollbar, ScrollbarThumb};

use crate::nav::Captures;
use crate::skin::{Drawn, Skin, Slot};
use crate::theme::{shape, size, Theme};

pub struct WidgetsPlugin;

impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        // The drop-down before the text field: both may take an Escape,
        // and an open list closing is the one that wins.
        app.add_message::<Pressed>()
            .add_message::<dropdown::DropdownChanged>()
            .add_systems(Update, (button_feedback, (dropdown::dropdowns, text_field::edit_text_fields).chain().in_set(Captures), dropdown::keep_highlight_in_view).chain())
            // Registered on its own rather than at the head of that
            // chain: putting it there moved `button_feedback` relative to
            // every screen's `controls`, and a dozen board tests stopped
            // seeing their presses. Dressing has no ordering requirement
            // of its own — it paints what interaction never touched.
            .add_systems(Update, dress)
            .add_plugins(reader::plugin);
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

/// What an undressed [`Themed`] node goes back to when the pointer
/// leaves it, when that is not the theme's button colour: a drop-down's
/// chosen row, which used to lose its highlight after the first hover.
#[derive(Component, Clone, Copy)]
pub struct Resting(pub Color);

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

/// A glass panel that stacks its children vertically.
///
/// **A panel is glass and a button is a pill** — the one look every menu,
/// form, pop-up and sheet shares, so the next screen looks like the last.
/// The glass is translucent (`Theme::glass`) so a menu reads as a layer
/// over the place it opens on — the drawn backdrop, a picture, the board —
/// rather than a hole cut in it, and a soft shadow lifts it off that
/// place. The board's own chrome (plates, tiles, the log) keeps the
/// opaque `Theme::panel`: it *is* the place.
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
///
/// Its padding stays 16 and its gap 8: the board's pop-up, menu and
/// sheets are sized by arithmetic that counts them (`POPUP_PADDING`,
/// `layout::SHEET_*`). A menu screen, which has the room, uses
/// [`roomy_panel`].
pub fn panel(theme: &Theme, width: Val) -> impl Bundle + use<> {
    glass_panel(theme, width, 16.0, 8.0)
}

/// [`panel`] with a menu screen's breathing room: the main menu, the
/// new-game form, settings, profile, replays, About.
pub fn roomy_panel(theme: &Theme, width: Val) -> impl Bundle + use<> {
    glass_panel(theme, width, 28.0, 12.0)
}

fn glass_panel(theme: &Theme, width: Val, padding: f32, gap: f32) -> impl Bundle + use<> {
    (
        Node {
            width,
            max_width: percent(100),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(padding)),
            row_gap: px(gap),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(shape::PANEL_RADIUS)),
            ..default()
        },
        BackgroundColor(theme.glass),
        BorderColor::all(theme.glass_border),
        lift(),
        Dressed::still(Slot::Panel, Drawn::new(theme.glass, theme.glass_border)),
    )
}

/// The soft shadow under a glass panel or an open list.
pub fn lift() -> BoxShadow {
    BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.40), px(0), px(10), px(0), px(28))
}

/// The shadow under a title that stands on a picture rather than on
/// glass — the main menu's NETRUNNER over the neon city, where the
/// accent's icy blue met the picture's lights and went thin. Bevy's
/// `TextShadow` is an offset copy with no blur, so it is dark and close
/// rather than soft and wide.
pub fn title_shadow() -> TextShadow {
    TextShadow { offset: Vec2::new(3.0, 4.0), color: Color::srgba(0.0, 0.0, 0.0, 0.85) }
}

/// The name of a group inside a panel ("YOUR SIDE"): small capitals in
/// the accent — it labels what follows rather than being read.
pub fn overline<T: Into<String>>(theme: &Theme, text: T) -> impl Bundle + use<T> {
    (Text::new(text.into().to_uppercase()), theme.font(size::OVERLINE), TextColor(theme.accent.with_alpha(0.85)))
}

/// A row that lays its children out left to right.
pub fn row(gap: f32) -> impl Bundle + use<> {
    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(gap), ..default() }
}

/// Which of the three buttons this is. A screen has at most one
/// `Primary` in view — the move it most expects (Start game, Continue) —
/// so the eye lands on it; `Secondary` is every other choice; `Quiet` is
/// a way out (Back, Quit) that should not compete with the choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Quiet,
}

impl ButtonKind {
    /// Resting, hovered and pressed: fill, rim, and the label's colour.
    fn looks(self, theme: &Theme) -> ([Drawn; 3], Color) {
        match self {
            ButtonKind::Primary => (
                [Drawn::new(theme.primary, theme.primary), Drawn::new(theme.primary_hover, theme.primary_hover), Drawn::new(theme.primary_press, theme.primary_press)],
                theme.on_primary,
            ),
            ButtonKind::Secondary => (
                [Drawn::new(theme.secondary, theme.glass_border), Drawn::new(theme.secondary_hover, theme.border_hover), Drawn::new(theme.secondary_press, theme.border_hover)],
                theme.text,
            ),
            ButtonKind::Quiet => (
                [Drawn::new(Color::NONE, Color::NONE), Drawn::new(theme.secondary, theme.glass_border), Drawn::new(theme.secondary_hover, theme.border_hover)],
                theme.text_dim,
            ),
        }
    }
}

/// A pill: the shape of every button in a menu. `width` `Val::Auto` fits
/// the label.
fn pill(width: Val) -> Node {
    Node {
        width,
        min_height: px(shape::BUTTON_HEIGHT),
        // A node shrinks by default when its row overflows, by an amount
        // that depends on its neighbours — which put every main-menu
        // button at a different width, each squeezed by the length of
        // its own blurb. A button's width is a decision, never a
        // neighbour's to take.
        flex_shrink: 0.0,
        padding: UiRect::axes(px(24), px(10)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::MAX,
        ..default()
    }
}

/// A themed button with `text` on it and `marker` for the screen to find
/// it by. `width` `Val::Auto` fits the text. A [`ButtonKind::Secondary`]
/// pill; [`styled_button`] names another kind.
pub fn button<T: Into<String>, M: Bundle>(theme: &Theme, text: T, width: Val, marker: M) -> impl Bundle + use<T, M> {
    styled_button(theme, ButtonKind::Secondary, text, width, marker)
}

/// A button of a given [`ButtonKind`]. The label is the button's one
/// direct `Text` child, which is how the tests find a button by what it
/// says.
pub fn styled_button<T: Into<String>, M: Bundle>(theme: &Theme, kind: ButtonKind, text: T, width: Val, marker: M) -> impl Bundle + use<T, M> {
    let ([rest, hover, press], ink) = kind.looks(theme);
    (
        Button,
        Themed,
        marker,
        pill(width),
        BackgroundColor(rest.bg),
        BorderColor::all(rest.border),
        Dressed { slot: Slot::Button, drawn: rest, hover: Some(hover), pressed: Some(press) },
        children![(Text::new(text), theme.font(size::BODY), TextColor(ink))],
    )
}

/// [`styled_button`] at the size of a row inside a panel — a deck tile's
/// Edit and Copy, the editor's − and +: the same pill and the same three
/// kinds, shorter and in the small type, because a row of full pills per
/// tile was most of the tile.
pub fn small_button<T: Into<String>, M: Bundle>(theme: &Theme, kind: ButtonKind, text: T, marker: M) -> impl Bundle + use<T, M> {
    let ([rest, hover, press], ink) = kind.looks(theme);
    (
        Button,
        Themed,
        marker,
        Node {
            min_height: px(30),
            flex_shrink: 0.0,
            padding: UiRect::axes(px(14), px(5)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(rest.bg),
        BorderColor::all(rest.border),
        Dressed { slot: Slot::Button, drawn: rest, hover: Some(hover), pressed: Some(press) },
        children![(Text::new(text), theme.font(size::SMALL), TextColor(ink))],
    )
}

/// A round button for one glyph — a stepper's `<` and `>`: a
/// [`button`] with no padding, as wide as it is tall.
pub fn round_button<T: Into<String>, M: Bundle>(theme: &Theme, text: T, marker: M) -> impl Bundle + use<T, M> {
    let ([rest, hover, press], ink) = ButtonKind::Secondary.looks(theme);
    let mut node = pill(px(shape::BUTTON_HEIGHT));
    node.height = px(shape::BUTTON_HEIGHT);
    node.padding = UiRect::ZERO;
    (
        Button,
        Themed,
        marker,
        node,
        BackgroundColor(rest.bg),
        BorderColor::all(rest.border),
        Dressed { slot: Slot::Button, drawn: rest, hover: Some(hover), pressed: Some(press) },
        children![(Text::new(text), theme.font(size::BODY), TextColor(ink))],
    )
}

/// A text field's box: a pill of glass ringed in the accent, which is how
/// a field says it is taking the keys.
pub fn field_node(width: Val) -> Node {
    Node {
        width,
        min_height: px(shape::BUTTON_HEIGHT),
        padding: UiRect::axes(px(18), px(8)),
        align_items: AlignItems::Center,
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::MAX,
        ..default()
    }
}

/// [`button`], greyed: the same pill, so the bar does not reflow when a
/// control becomes legal, with dim text and the `Disabled` marker the
/// feedback system skips.
pub fn disabled_button<T: Into<String>, M: Bundle>(theme: &Theme, text: T, width: Val, marker: M) -> impl Bundle + use<T, M> {
    let fill = theme.secondary.with_alpha(0.05);
    let rim = theme.glass_border.with_alpha(0.12);
    (
        Button,
        Themed,
        Disabled,
        marker,
        pill(width),
        BackgroundColor(fill),
        BorderColor::all(rim),
        Dressed::still(Slot::ButtonDisabled, Drawn::new(fill, rim)),
        children![(Text::new(text), theme.font(size::BODY), TextColor(theme.text_dim.with_alpha(0.5)))],
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
        width: px(shape::BUTTON_HEIGHT),
        height: px(shape::BUTTON_HEIGHT),
        flex_shrink: 0.0,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::MAX,
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
    let ([rest, hover, press], _) = ButtonKind::Secondary.looks(theme);
    let dressed = Dressed { slot: Slot::Button, drawn: rest, hover: Some(hover), pressed: Some(press) };
    (Button, Themed, marker, node, BackgroundColor(rest.bg), BorderColor::all(rest.border), dressed, Children::spawn(label))
}

/// A vertical scrollbar for `target`, a node with `Overflow::scroll_y`:
/// a thin track with a thumb the scrollbar plugin sizes and moves. Put
/// it beside the target in a row; the wheel still works without it.
pub fn scrollbar(theme: &Theme, target: Entity) -> impl Bundle + use<> {
    (
        Scrollbar::new(target, ControlOrientation::Vertical, 24.0),
        Node { width: px(8), height: percent(100), flex_shrink: 0.0, border_radius: BorderRadius::MAX, ..default() },
        BackgroundColor(theme.secondary.with_alpha(0.06)),
        children![(ScrollbarThumb { border_radius: BorderRadius::MAX, border: UiRect::ZERO }, BackgroundColor(theme.border_hover))],
    )
}

/// The box a node was laid out in, in logical window pixels: the
/// global transform's translation is its centre and the computed size
/// its extent, both physical until scaled back.
pub fn anchor_of(node: &ComputedNode, transform: &UiGlobalTransform) -> crate::models::layout::Anchor {
    let scale = node.inverse_scale_factor();
    let centre = transform.translation * scale;
    let size = node.size() * scale;
    crate::models::layout::Anchor { x: centre.x, y: centre.y, width: size.x, height: size.y }
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
    mut buttons: Query<(Entity, &Interaction, &mut BackgroundColor, Option<&Dressed>, Option<&Resting>), (Changed<Interaction>, With<Themed>, Without<Disabled>)>,
    mut pressed: MessageWriter<Pressed>,
) {
    for (entity, interaction, mut background, dressed, resting) in &mut buttons {
        if *interaction == Interaction::Pressed {
            pressed.write(Pressed(entity));
        }
        match dressed {
            Some(dressed) => {
                let (slot, drawn) = dressed.at(*interaction);
                skin.dress(slot, drawn).apply(&mut commands.entity(entity));
            }
            // A menu's own undressed node — a drop-down's head or row —
            // names its resting colour and takes the glass's hover.
            None if resting.is_some() => {
                *background = BackgroundColor(match interaction {
                    Interaction::Pressed => theme.secondary_press,
                    Interaction::Hovered => theme.secondary_hover,
                    Interaction::None => resting.map_or(theme.secondary, |r| r.0),
                });
            }
            // Undressed, and the resting colour is the theme's button:
            // the board's own few undressed buttons (the score rows).
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
