//! A drop-down: one button that reads "Label: choice ▾" and, pressed,
//! opens the list of choices under itself. Choosing one closes the list
//! and tells the screen which; a click anywhere else, or Escape, closes
//! it without.
//!
//! Bevy 0.19 has no drop-down of its own (`bevy_ui_widgets` stops at
//! menus and list boxes, both needing a focus model the client does not
//! run), and a row of chips per filter — the browser's first cut — was
//! most of a screen's width for five filters. The list is a child of the
//! drop-down's root, positioned absolutely against it and lifted above
//! everything with `GlobalZIndex`, so it overlaps whatever the row sits
//! on; it is spawned when opened and despawned when closed rather than
//! hidden, because a hidden list would still take a hit test.
//!
//! **A list stays on the window.** It is measured against the window as
//! it opens (`layout::list_box`): below the head when the whole list
//! fits there, otherwise toward the side with more room, never taller
//! than that side. A longer list scrolls, with a bar beside it and the
//! keys: ↑ ↓, Page Up and Page Down, Home and End move a highlight that
//! the list keeps in view, and Enter chooses it. The deck lists at the
//! foot of the new-game form always opened downward, and every deck
//! past the first few was below the window, where nothing could reach
//! it. That is the report this answers.
//!
//! The screen keeps its own notion of what is chosen: it reads
//! [`DropdownChanged`], applies its intent and redraws, the same way it
//! reads a `Pressed`. The widget's `selected` is only what the head
//! shows meanwhile.

use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::models::layout::{list_box, ListBox, LIST_ROW, MENU_GAP};
use crate::nav::InputCaptured;
use crate::theme::{shape, size, Theme};
use crate::widgets::{anchor_of, Pressed, Resting, Themed};

/// How far Page Up and Page Down move the highlight.
const PAGE: usize = 8;

/// One entry: its words and, optionally, an icon drawn before them in
/// the icon font, in a colour of the screen's choosing.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub text: String,
    pub icon: Option<(String, Color)>,
}

impl Choice {
    pub fn plain(text: impl Into<String>) -> Self {
        Choice { text: text.into(), icon: None }
    }

    pub fn with_icon(text: impl Into<String>, icon: Option<String>, colour: Color) -> Self {
        Choice { text: text.into(), icon: icon.map(|glyph| (glyph, colour)) }
    }
}

/// The drop-down's state, on the entity `spawn_dropdown` returns.
#[derive(Component, Debug, Clone)]
pub struct Dropdown {
    pub label: String,
    pub choices: Vec<Choice>,
    pub selected: usize,
    pub open: bool,
    /// The row the keys have moved to while the list is open: where
    /// Enter would choose. It starts on the chosen row.
    pub highlight: usize,
}

/// `dropdown` (the entity `spawn_dropdown` returned, carrying the
/// screen's marker) now shows choice `index`.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropdownChanged {
    pub dropdown: Entity,
    pub index: usize,
}

/// The head button; its field is the drop-down's root.
#[derive(Component, Debug, Clone, Copy)]
pub struct Head(pub Entity);

/// One choice's button in an open list: the root and the index.
#[derive(Component, Debug, Clone, Copy)]
pub struct Item(pub Entity, pub usize);

/// The open list, a child of the root.
#[derive(Component, Debug, Clone, Copy)]
pub struct List(pub Entity);

/// The scrolling column inside an open list: its root, and the
/// highlighted row last brought into view (`None` until the first
/// layout, so the list opens scrolled to the chosen row).
#[derive(Component, Debug, Clone, Copy)]
pub struct ListScroll {
    pub root: Entity,
    shown: Option<usize>,
}

/// The span of the head that reads the chosen text, and the one before
/// it that carries its icon, so a choice updates the head in place.
#[derive(Component)]
pub struct HeadIcon;
#[derive(Component)]
pub struct HeadText;

/// Spawns a closed drop-down under `parent` and returns its root, which
/// carries `marker` for the screen to find it by.
pub fn spawn_dropdown(parent: &mut ChildSpawnerCommands, theme: &Theme, label: impl Into<String>, choices: Vec<Choice>, selected: usize, marker: impl Bundle) -> Entity {
    let selected = selected.min(choices.len().saturating_sub(1));
    let dropdown = Dropdown { label: label.into(), choices, selected, open: false, highlight: selected };
    let root = parent.spawn((marker, Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() })).id();
    parent.commands().entity(root).with_children(|children| spawn_head(children, theme, root, &dropdown));
    parent.commands().entity(root).insert(dropdown);
    root
}

fn spawn_head(parent: &mut ChildSpawnerCommands, theme: &Theme, root: Entity, dropdown: &Dropdown) {
    let choice = dropdown.choices.get(dropdown.selected);
    let (icon, colour) = choice.and_then(|c| c.icon.clone()).unwrap_or((String::new(), theme.text));
    let text = choice.map_or(String::new(), |c| c.text.clone());
    let arrow = if theme.has_symbols() { "▾" } else { "v" };
    parent
        .spawn((
            Button,
            Themed,
            Head(root),
            Node {
                flex_shrink: 0.0,
                min_height: px(shape::BUTTON_HEIGHT),
                padding: UiRect::axes(px(20), px(10)),
                align_items: AlignItems::Center,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(theme.secondary),
            BorderColor::all(theme.glass_border),
            Resting(theme.secondary),
        ))
        .with_children(|button| {
            button.spawn((Text::new(""), theme.font(size::BODY), TextColor(theme.text))).with_children(|spans| {
                // An empty label is a drop-down whose name the screen
                // prints above it, as the new-game form's sections do.
                if !dropdown.label.is_empty() {
                    spans.spawn((TextSpan::new(format!("{}: ", dropdown.label)), theme.font(size::BODY), TextColor(theme.text_dim)));
                }
                spans.spawn((HeadIcon, TextSpan::new(icon_with_space(&icon)), theme.icon_font(size::BODY), TextColor(colour)));
                spans.spawn((HeadText, TextSpan::new(text), theme.font(size::BODY), TextColor(theme.text)));
                spans.spawn((TextSpan::new(format!("   {arrow}")), theme.symbol_font(size::BODY), TextColor(theme.accent)));
            });
        });
}

fn icon_with_space(icon: &str) -> String {
    if icon.is_empty() { String::new() } else { format!("{icon} ") }
}

/// A row's resting colour: the chosen row keeps its highlight after
/// the pointer has passed over it, and the keys' row has the hover's.
fn row_colour(theme: &Theme, dropdown: &Dropdown, index: usize) -> Color {
    if index == dropdown.selected {
        theme.secondary_press
    } else if index == dropdown.highlight {
        theme.secondary_hover
    } else {
        Color::NONE
    }
}

/// `place` is where the list goes, or `None` before the head has been
/// laid out (a headless test), when it hangs below at most 55% of the
/// window as it always did.
fn spawn_list(parent: &mut ChildSpawnerCommands, theme: &Theme, root: Entity, dropdown: &Dropdown, place: Option<ListBox>) {
    let inner = |max: f32| (max - 2.0 * 6.0 - 2.0).max(1.0);
    let mut node = Node {
        position_type: PositionType::Absolute,
        left: px(0),
        min_width: percent(100),
        flex_direction: FlexDirection::Row,
        column_gap: px(6),
        padding: UiRect::all(px(6)),
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::all(px(shape::LIST_RADIUS)),
        ..default()
    };
    let column_height = match place {
        Some(place) if place.opens_up => {
            node.bottom = percent(100);
            node.margin = UiRect::bottom(px(MENU_GAP));
            node.max_width = px(place.max_width);
            px(inner(place.max_height))
        }
        Some(place) => {
            node.top = percent(100);
            node.margin = UiRect::top(px(MENU_GAP));
            node.max_width = px(place.max_width);
            px(inner(place.max_height))
        }
        None => {
            node.top = percent(100);
            node.margin = UiRect::top(px(MENU_GAP));
            Val::Vh(55.0)
        }
    };
    // A bar only where the list will scroll; the estimate decides only
    // whether to draw one, and the wheel and the keys work either way.
    let scrolls = place.is_none_or(|place| dropdown.choices.len() as f32 * LIST_ROW > inner(place.max_height));
    // Opaque, unlike the panels: the rows are read against it, and
    // whatever the list opened over would otherwise read through them.
    let list = parent.spawn((List(root), GlobalZIndex(10), node, BackgroundColor(theme.glass_strong.with_alpha(1.0)), BorderColor::all(theme.glass_border), super::lift())).id();
    let column = parent
        .commands()
        .spawn((
            ListScroll { root, shown: None },
            bevy::ui_widgets::ScrollArea,
            Node { flex_grow: 1.0, max_height: column_height, overflow: Overflow::scroll_y(), flex_direction: FlexDirection::Column, row_gap: px(2), ..default() },
        ))
        .with_children(|column| {
            for (index, choice) in dropdown.choices.iter().enumerate() {
                let current = index == dropdown.selected;
                let resting = row_colour(theme, dropdown, index);
                column
                    .spawn((
                        Button,
                        Themed,
                        Item(root, index),
                        Node {
                            flex_shrink: 0.0,
                            padding: UiRect::axes(px(14), px(8)),
                            align_items: AlignItems::Center,
                            border_radius: BorderRadius::all(px(shape::ROW_RADIUS)),
                            ..default()
                        },
                        BackgroundColor(resting),
                        Resting(resting),
                    ))
                    .with_children(|button| {
                        let ink = if current { theme.accent } else { theme.text };
                        button.spawn((Text::new(""), theme.font(size::BODY), TextColor(ink))).with_children(|spans| {
                            if let Some((glyph, colour)) = &choice.icon {
                                spans.spawn((TextSpan::new(format!("{glyph} ")), theme.icon_font(size::BODY), TextColor(*colour)));
                            }
                            spans.spawn((TextSpan::new(choice.text.clone()), theme.font(size::BODY), TextColor(ink)));
                        });
                    });
            }
        })
        .id();
    parent.commands().entity(list).add_child(column);
    if scrolls {
        let bar = parent.commands().spawn(super::scrollbar(theme, column)).id();
        parent.commands().entity(list).add_child(bar);
    }
}

/// Where a key moves the highlight in a list of `len` rows, or `None`
/// for a key the list does not take.
fn step(key: KeyCode, highlight: usize, len: usize) -> Option<usize> {
    let last = len.checked_sub(1)?;
    Some(match key {
        KeyCode::ArrowUp => highlight.saturating_sub(1),
        KeyCode::ArrowDown => (highlight + 1).min(last),
        KeyCode::PageUp => highlight.saturating_sub(PAGE),
        KeyCode::PageDown => (highlight + PAGE).min(last),
        KeyCode::Home => 0,
        KeyCode::End => last,
        _ => return None,
    })
}

/// Opens and closes lists, applies a choice, moves the keys' highlight,
/// and holds [`InputCaptured`] while any list is open so Escape closes
/// it rather than leaving the screen.
#[allow(clippy::too_many_arguments)]
pub fn dropdowns(
    mut commands: Commands,
    mut pressed: MessageReader<Pressed>,
    mut key_presses: MessageReader<KeyboardInput>,
    heads: Query<(&Head, &Interaction, &ComputedNode, &UiGlobalTransform)>,
    mut items: Query<(&Item, &Interaction, &mut BackgroundColor, &mut Resting)>,
    lists: Query<(Entity, &List, &ComputedNode, &UiGlobalTransform)>,
    mut dropdowns: Query<(Entity, &mut Dropdown, &Children)>,
    mut head_icons: Query<(&mut TextSpan, &mut TextColor), (With<HeadIcon>, Without<HeadText>)>,
    mut head_texts: Query<&mut TextSpan, With<HeadText>>,
    all_children: Query<&Children>,
    window: Query<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut captured: ResMut<InputCaptured>,
    mut changed: MessageWriter<DropdownChanged>,
    theme: Res<Theme>,
) {
    let mut chosen: Vec<(Entity, usize)> = Vec::new();
    let mut toggled: Vec<Entity> = Vec::new();
    for Pressed(entity) in pressed.read() {
        if let Ok((Head(root), ..)) = heads.get(*entity) {
            toggled.push(*root);
        } else if let Ok((Item(root, index), ..)) = items.get(*entity) {
            chosen.push((*root, *index));
        }
    }
    let presses: Vec<KeyCode> = key_presses.read().filter(|key| key.state == ButtonState::Pressed).map(|key| key.key_code).collect();
    // A click that landed on no part of any drop-down, or an Escape,
    // closes every open list. The list's own box counts as a part — its
    // scrollbar and the gaps between rows are not buttons, and a click
    // meant to drag the bar must not close what it scrolls.
    let pointer = window.single().ok().and_then(Window::cursor_position);
    let over_a_list = pointer.is_some_and(|at| {
        lists.iter().any(|(_, _, node, transform)| {
            let area = anchor_of(node, transform);
            (at.x - area.x).abs() <= area.width / 2.0 && (at.y - area.y).abs() <= area.height / 2.0
        })
    });
    let on_a_part = over_a_list || heads.iter().any(|(_, i, ..)| *i == Interaction::Pressed) || items.iter().any(|(_, i, ..)| *i == Interaction::Pressed);
    let any_open = dropdowns.iter().any(|(_, d, _)| d.open);
    let close_all = any_open && ((mouse.just_pressed(MouseButton::Left) && !on_a_part) || keys.just_pressed(KeyCode::Escape));
    if any_open {
        captured.0 = true;
    }
    let window_size = window.single().ok().map(|window| (window.width(), window.height()));
    for (root, mut dropdown, children) in &mut dropdowns {
        let was_open = dropdown.open;
        // The keys act on an open list: move the highlight, or choose it.
        if was_open && !close_all && !chosen.iter().any(|(r, _)| *r == root) {
            let before = dropdown.highlight;
            for key in &presses {
                if matches!(key, KeyCode::Enter | KeyCode::NumpadEnter) {
                    chosen.push((root, dropdown.highlight));
                    break;
                }
                if let Some(next) = step(*key, dropdown.highlight, dropdown.choices.len()) {
                    dropdown.highlight = next;
                }
            }
            if dropdown.highlight != before {
                for (Item(owner, index), _, mut background, mut resting) in &mut items {
                    if *owner == root {
                        let colour = row_colour(&theme, &dropdown, *index);
                        *background = BackgroundColor(colour);
                        resting.0 = colour;
                    }
                }
            }
        }
        if let Some((_, index)) = chosen.iter().find(|(r, _)| *r == root) {
            if *index < dropdown.choices.len() && *index != dropdown.selected {
                dropdown.selected = *index;
                changed.write(DropdownChanged { dropdown: root, index: *index });
                // The head's spans, in place: no respawn, so a screen that
                // redraws the whole row this frame does not race it.
                let choice = &dropdown.choices[*index];
                let (glyph, colour) = choice.icon.clone().unwrap_or((String::new(), theme.text));
                // Root → head button → its `Text` → the spans.
                for head in children.iter().filter(|child| heads.contains(*child)) {
                    for text in all_children.get(head).into_iter().flat_map(|c| c.iter()) {
                        for span in all_children.get(text).into_iter().flat_map(|c| c.iter()) {
                            if let Ok((mut icon, mut icon_colour)) = head_icons.get_mut(span) {
                                icon.0 = icon_with_space(&glyph);
                                icon_colour.0 = colour;
                            }
                            if let Ok(mut text) = head_texts.get_mut(span) {
                                text.0 = choice.text.clone();
                            }
                        }
                    }
                }
            }
            dropdown.open = false;
        } else if toggled.contains(&root) {
            dropdown.open = !was_open;
        } else if close_all {
            dropdown.open = false;
        }
        if dropdown.open && !was_open {
            dropdown.highlight = dropdown.selected;
            // Measured against the window as it opens; a head not laid out
            // yet (a headless test) has no size, and the list hangs below.
            let head = children.iter().find_map(|child| heads.get(child).ok()).map(|(_, _, node, transform)| anchor_of(node, transform));
            let place = match (window_size, head) {
                (Some(window), Some(head)) if head.height > 0.0 => Some(list_box(window, head, dropdown.choices.len())),
                _ => None,
            };
            let snapshot = dropdown.clone();
            commands.entity(root).with_children(|children| spawn_list(children, &theme, root, &snapshot, place));
        } else if !dropdown.open && was_open {
            for (list, List(owner), ..) in &lists {
                if *owner == root {
                    commands.entity(list).try_despawn();
                }
            }
        }
    }
}

/// Scrolls an open list so its highlighted row is in view: on the first
/// frame the rows are laid out, which opens a long list at the chosen
/// row rather than at the top, and whenever the keys move the highlight.
/// Read off the laid-out boxes, never a row-height estimate.
pub fn keep_highlight_in_view(
    mut scrolls: Query<(&mut ListScroll, &ComputedNode, &UiGlobalTransform, &mut ScrollPosition)>,
    items: Query<(&Item, &ComputedNode, &UiGlobalTransform)>,
    dropdowns: Query<&Dropdown>,
) {
    for (mut scroll, node, transform, mut position) in &mut scrolls {
        let Ok(dropdown) = dropdowns.get(scroll.root) else { continue };
        if scroll.shown == Some(dropdown.highlight) {
            continue;
        }
        let Some(row) = items.iter().find(|(Item(owner, index), ..)| *owner == scroll.root && *index == dropdown.highlight).map(|(_, node, transform)| anchor_of(node, transform)) else {
            continue;
        };
        let view = anchor_of(node, transform);
        if row.height <= 0.0 || view.height <= 0.0 {
            // Not laid out yet: try again next frame.
            continue;
        }
        let (view_top, view_bottom) = (view.y - view.height / 2.0, view.y + view.height / 2.0);
        let (row_top, row_bottom) = (row.y - row.height / 2.0, row.y + row.height / 2.0);
        if row_top < view_top {
            position.y -= view_top - row_top;
        } else if row_bottom > view_bottom {
            position.y += row_bottom - view_bottom;
        }
        position.y = position.y.max(0.0);
        scroll.shown = Some(dropdown.highlight);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_move_the_highlight_and_stop_at_the_ends() {
        assert_eq!(step(KeyCode::ArrowDown, 0, 3), Some(1));
        assert_eq!(step(KeyCode::ArrowDown, 2, 3), Some(2));
        assert_eq!(step(KeyCode::ArrowUp, 0, 3), Some(0));
        assert_eq!(step(KeyCode::PageDown, 2, 30), Some(10));
        assert_eq!(step(KeyCode::PageDown, 25, 30), Some(29));
        assert_eq!(step(KeyCode::PageUp, 5, 30), Some(0));
        assert_eq!(step(KeyCode::Home, 17, 30), Some(0));
        assert_eq!(step(KeyCode::End, 0, 30), Some(29));
        assert_eq!(step(KeyCode::KeyA, 0, 30), None);
        assert_eq!(step(KeyCode::ArrowDown, 0, 0), None, "an empty list has nowhere to go");
    }
}
