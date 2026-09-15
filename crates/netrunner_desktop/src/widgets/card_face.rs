//! A card as a node: the picture when it is cached, else the printed
//! card's layout in text — cost circle top-left, title, type line, the
//! rules text with its icons as glyphs, the numbers in their corners,
//! influence as pips.
//!
//! What goes where is decided by `netrunner_client::card_face::Face`,
//! tested without a window; this module only draws slots. The text face
//! is the procedural tier that always works — every card the engine can
//! name has one from the first frame — and the picture, when the store
//! has it, is put in its place by `card_images::poll_decoded`, which is
//! why a text face carries `WantsImage`. Which glyph a symbol is drawn
//! with is the theme's decision (`Theme::symbol`): NetrunnerDB's icon
//! font when it is cached, Noto Sans Symbols 2 when not, a word or a
//! Latin-1 stand-in when neither is loaded.

use bevy::prelude::*;

use netrunner_client::card_face::{Face, Slot};
use netrunner_client::card_text::{superscript, Segment, Symbol};

use crate::card_images::{picture, WantsImage};
use crate::theme::Theme;

/// The two sizes a face is drawn at, both 5:7 like the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceSize {
    /// A grid cell.
    Thumb,
    /// The inspector's.
    Large,
}

impl FaceSize {
    pub fn width(self) -> f32 {
        match self {
            FaceSize::Thumb => 140.0,
            FaceSize::Large => 380.0,
        }
    }

    pub fn height(self) -> f32 {
        self.width() * 1.4
    }

    fn title(self) -> f32 {
        match self {
            FaceSize::Thumb => 11.0,
            FaceSize::Large => 21.0,
        }
    }

    fn small(self) -> f32 {
        match self {
            FaceSize::Thumb => 8.0,
            FaceSize::Large => 14.0,
        }
    }

    fn body(self) -> f32 {
        match self {
            FaceSize::Thumb => 8.5,
            FaceSize::Large => 15.0,
        }
    }

    fn number(self) -> f32 {
        match self {
            FaceSize::Thumb => 11.0,
            FaceSize::Large => 19.0,
        }
    }

    fn padding(self) -> f32 {
        match self {
            FaceSize::Thumb => 5.0,
            FaceSize::Large => 12.0,
        }
    }
}

/// Spawns `face` under `parent` and returns its entity. With `image`
/// the face is the picture; without, the text layout, tagged to be
/// replaced by the picture if one turns up. `marker` goes on the root,
/// so a screen can make the face a button.
pub fn spawn_face(parent: &mut ChildSpawnerCommands, theme: &Theme, face: &Face, size: FaceSize, image: Option<Handle<Image>>, marker: impl Bundle) -> Entity {
    if let Some(handle) = image {
        return parent.spawn((marker, picture(handle, size))).id();
    }
    let faction = theme.faction(face.faction);
    let glyphs = theme.has_symbols();
    let mut root = parent.spawn((
        marker,
        Node {
            width: px(size.width()),
            height: px(size.height()),
            flex_direction: FlexDirection::Column,
            flex_shrink: 0.0,
            overflow: Overflow::clip(),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(8)),
            padding: UiRect::all(px(size.padding())),
            row_gap: px(2),
            ..default()
        },
        BackgroundColor(theme.panel),
        BorderColor::all(faction),
    ));
    if let Some(code) = face.code {
        root.insert(WantsImage { code, size });
    }
    root.with_children(|card| {
        // Title row: the cost circle, then the title, which takes the rest.
        card.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(4), ..default() },)).with_children(|row| {
            if let Some(slot) = face.cost {
                row.spawn(circle(theme, faction, &slot.value(), size));
            }
            let title_colour = if face.implemented { theme.text } else { theme.text_dim };
            row.spawn((
                Text::new(""),
                theme.font(size.title()),
                TextColor(title_colour),
                TextLayout::new(Justify::Left, LineBreak::WordBoundary),
                Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() },
            ))
            .with_children(|spans| {
                if face.unique {
                    spans.spawn((TextSpan::new(if glyphs { "◆ " } else { "• " }), theme.symbol_font(size.title()), TextColor(faction)));
                }
                spans.spawn((TextSpan::new(face.title.clone()), theme.font(size.title()), TextColor(title_colour)));
            });
        });
        card.spawn((Text::new(face.type_line.clone()), theme.font(size.small()), TextColor(theme.text_dim), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        // The rules text, symbols as glyph spans in the accent colour.
        card.spawn((Node { flex_grow: 1.0, min_height: px(0), overflow: Overflow::clip(), flex_direction: FlexDirection::Column, ..default() },)).with_children(|body| {
            body.spawn((Text::new(""), theme.font(size.body()), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::WordBoundary), BodyText))
                .with_children(|spans| {
                    for segment in &face.body {
                        match segment {
                            Segment::Text(text) => {
                                spans.spawn((TextSpan::new(text.clone()), theme.font(size.body()), TextColor(theme.text)));
                            }
                            Segment::Symbol(symbol) => {
                                let (glyph, font) = theme.symbol(*symbol, size.body());
                                spans.spawn((TextSpan::new(glyph), font, TextColor(theme.accent)));
                            }
                            Segment::Superscript(n) => {
                                spans.spawn((TextSpan::new(superscript(*n)), theme.font(size.body()), TextColor(theme.text)));
                            }
                            Segment::Break => {
                                spans.spawn((TextSpan::new("\n"), theme.font(size.body()), TextColor(theme.text)));
                            }
                        }
                    }
                });
            if size == FaceSize::Large
                && let Some(flavor) = &face.flavor
            {
                body.spawn((
                    Text::new(format!("\u{201c}{}\u{201d}", flavor.replace('\n', " "))),
                    theme.font(size.small()),
                    TextColor(theme.text_dim),
                    TextLayout::new(Justify::Left, LineBreak::WordBoundary),
                    Node { margin: UiRect::top(px(4)), ..default() },
                ));
            }
        });
        // The bottom row: strength or points left, the faction's mark and
        // the pips in the middle, trash cost, memory, limits right.
        card.spawn((Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            column_gap: px(4),
            flex_shrink: 0.0,
            ..default()
        },))
        .with_children(|row| {
            row.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(3), ..default() },)).with_children(|left| {
                if let Some(slot) = face.bottom_left {
                    left.spawn(chip(theme, faction, slot, size));
                }
            });
            row.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(4), ..default() },)).with_children(|middle| {
                if let Some((mark, font)) = face.faction.and_then(|f| theme.faction_icon(f, size.number())) {
                    middle.spawn((Text::new(mark), font, TextColor(faction)));
                }
                if let Some(influence) = face.influence.filter(|n| *n > 0) {
                    let pip = if glyphs { "●" } else { "•" };
                    middle.spawn((Text::new(pip.repeat(influence as usize)), theme.symbol_font(size.small()), TextColor(faction)));
                }
            });
            row.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(3), ..default() },)).with_children(|right| {
                for slot in &face.bottom_right {
                    right.spawn(chip(theme, faction, *slot, size));
                }
            });
        });
    });
    root.id()
}

/// The body text of a face, for tests that read what a face says.
#[derive(Component)]
pub struct BodyText;

/// The cost circle: a number on the faction's colour.
fn circle(theme: &Theme, faction: Color, value: &str, size: FaceSize) -> impl Bundle {
    let d = size.number() + 10.0;
    (
        Node {
            width: px(d),
            height: px(d),
            flex_shrink: 0.0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(d / 2.0)),
            ..default()
        },
        BackgroundColor(faction),
        children![(Text::new(value.to_string()), theme.font(size.number()), TextColor(theme.background))],
    )
}

/// A number with its icon, or its caption where there is no icon: the
/// memory and trash icons the card prints, a word for the rest.
fn chip(theme: &Theme, faction: Color, slot: Slot, size: FaceSize) -> impl Bundle {
    let icon = match slot {
        Slot::Memory(_) => Some(Symbol::Mu),
        Slot::TrashCost(_) => Some(Symbol::Trash),
        Slot::Link(_) => Some(Symbol::Link),
        _ => None,
    };
    let (label, label_font) = match icon {
        Some(symbol) => theme.symbol(symbol, size.small()),
        None => match slot {
            Slot::Strength(_) | Slot::AgendaPoints(_) | Slot::Cost(_) | Slot::Advancement(_) => (String::new(), theme.font(size.small())),
            _ => (slot.caption().to_string(), theme.font(size.small())),
        },
    };
    (
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(2),
            padding: UiRect::axes(px(4), px(1)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(theme.button),
        children![
            (Text::new(slot.value()), theme.font(size.number()), TextColor(faction)),
            (Text::new(label), label_font, TextColor(theme.text_dim)),
        ],
    )
}
