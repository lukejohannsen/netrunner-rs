//! A drop-down: one button that reads "Label: choice ▾" and, pressed,
//! opens the list of choices under itself. Choosing one closes the list
//! and tells the screen which; a click anywhere else, or Escape, closes
//! it without.
//!
//! Bevy 0.19 has no drop-down of its own (`bevy_ui_widgets` stops at
//! menus and list boxes, both needing a focus model the client does not
//! run), and a row of chips per filter — the browser's first cut — was
//! most of a screen's width for five filters. The list is a child of the
//! drop-down's root, positioned absolutely below it and lifted above
//! everything with `GlobalZIndex`, so it overlaps whatever the row sits
//! on; it is spawned when opened and despawned when closed rather than
//! hidden, because a hidden list would still take a hit test.
//!
//! The screen keeps its own notion of what is chosen: it reads
//! [`DropdownChanged`], applies its intent and redraws, the same way it
//! reads a `Pressed`. The widget's `selected` is only what the head
//! shows meanwhile.

use bevy::prelude::*;

use crate::nav::InputCaptured;
use crate::theme::{size, Theme};
use crate::widgets::{Pressed, Themed};

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
    let dropdown = Dropdown { label: label.into(), choices, selected, open: false };
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
                padding: UiRect::axes(px(12), px(6)),
                align_items: AlignItems::Center,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(6)),
                ..default()
            },
            BackgroundColor(theme.button),
            BorderColor::all(theme.panel_border),
        ))
        .with_children(|button| {
            button.spawn((Text::new(""), theme.font(size::SMALL), TextColor(theme.text))).with_children(|spans| {
                spans.spawn((TextSpan::new(format!("{}: ", dropdown.label)), theme.font(size::SMALL), TextColor(theme.text_dim)));
                spans.spawn((HeadIcon, TextSpan::new(icon_with_space(&icon)), theme.icon_font(size::SMALL), TextColor(colour)));
                spans.spawn((HeadText, TextSpan::new(text), theme.font(size::SMALL), TextColor(theme.text)));
                spans.spawn((TextSpan::new(format!("  {arrow}")), theme.symbol_font(size::SMALL), TextColor(theme.text_dim)));
            });
        });
}

fn icon_with_space(icon: &str) -> String {
    if icon.is_empty() { String::new() } else { format!("{icon} ") }
}

fn spawn_list(parent: &mut ChildSpawnerCommands, theme: &Theme, root: Entity, dropdown: &Dropdown) {
    parent
        .spawn((
            List(root),
            GlobalZIndex(10),
            Node {
                position_type: PositionType::Absolute,
                top: percent(100),
                left: px(0),
                min_width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(2),
                padding: UiRect::all(px(4)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(6)),
                ..default()
            },
            BackgroundColor(theme.panel),
            BorderColor::all(theme.panel_border),
        ))
        .with_children(|list| {
            for (index, choice) in dropdown.choices.iter().enumerate() {
                let current = index == dropdown.selected;
                list.spawn((
                    Button,
                    Themed,
                    Item(root, index),
                    Node {
                        flex_shrink: 0.0,
                        padding: UiRect::axes(px(10), px(5)),
                        align_items: AlignItems::Center,
                        border_radius: BorderRadius::all(px(4)),
                        ..default()
                    },
                    BackgroundColor(if current { theme.button_press } else { theme.button }),
                ))
                .with_children(|button| {
                    button.spawn((Text::new(""), theme.font(size::SMALL), TextColor(theme.text))).with_children(|spans| {
                        if let Some((glyph, colour)) = &choice.icon {
                            spans.spawn((TextSpan::new(format!("{glyph} ")), theme.icon_font(size::SMALL), TextColor(*colour)));
                        }
                        spans.spawn((TextSpan::new(choice.text.clone()), theme.font(size::SMALL), TextColor(theme.text)));
                    });
                });
            }
        });
}

/// Opens and closes lists, applies a choice, and holds
/// [`InputCaptured`] while any list is open so Escape closes it rather
/// than leaving the screen.
pub fn dropdowns(
    mut commands: Commands,
    mut pressed: MessageReader<Pressed>,
    heads: Query<(&Head, &Interaction)>,
    items: Query<(&Item, &Interaction)>,
    lists: Query<(Entity, &List)>,
    mut dropdowns: Query<(Entity, &mut Dropdown, &Children)>,
    mut head_icons: Query<(&mut TextSpan, &mut TextColor), (With<HeadIcon>, Without<HeadText>)>,
    mut head_texts: Query<&mut TextSpan, With<HeadText>>,
    all_children: Query<&Children>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut captured: ResMut<InputCaptured>,
    mut changed: MessageWriter<DropdownChanged>,
    theme: Res<Theme>,
) {
    let mut chosen: Vec<(Entity, usize)> = Vec::new();
    let mut toggled: Vec<Entity> = Vec::new();
    for Pressed(entity) in pressed.read() {
        if let Ok((Head(root), _)) = heads.get(*entity) {
            toggled.push(*root);
        } else if let Ok((Item(root, index), _)) = items.get(*entity) {
            chosen.push((*root, *index));
        }
    }
    // A click that landed on no part of any drop-down, or an Escape,
    // closes every open list.
    let on_a_part = heads.iter().any(|(_, i)| *i == Interaction::Pressed) || items.iter().any(|(_, i)| *i == Interaction::Pressed);
    let any_open = dropdowns.iter().any(|(_, d, _)| d.open);
    let close_all = any_open && ((mouse.just_pressed(MouseButton::Left) && !on_a_part) || keys.just_pressed(KeyCode::Escape));
    if any_open {
        captured.0 = true;
    }
    for (root, mut dropdown, children) in &mut dropdowns {
        let was_open = dropdown.open;
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
            let snapshot = dropdown.clone();
            commands.entity(root).with_children(|children| spawn_list(children, &theme, root, &snapshot));
        } else if !dropdown.open && was_open {
            for (list, List(owner)) in &lists {
                if *owner == root {
                    commands.entity(list).try_despawn();
                }
            }
        }
    }
}
