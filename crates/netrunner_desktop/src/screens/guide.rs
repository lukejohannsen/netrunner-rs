//! The strategy guide: what to do with the rules Learn to Play teaches,
//! read one chapter at a time (`netrunner_client::guide`, the committed
//! `docs/strategy-guide.md`).
//!
//! **Reached from Learn to Play, and Back leads there** (`nav::back_from`):
//! the guide is the reading beside the lessons, not a destination of the
//! main menu's.
//!
//! **The chapters are a column of pills with the one being read filled**
//! — a handful of options drawn the way it is best chosen (AGENTS.md §5)
//! — and the chapter scrolls beside them, as About does: a reading
//! screen, not the board, so the no-scroll rule is not its.
//!
//! **A card the guide names is drawn in the accent, and the chapter ends
//! in a row of its cards**, each a button that opens the card centred
//! (`widgets::reader`), as the deck builder reads one — a primary press
//! as well as a secondary click, because on this screen reading is the
//! only thing a card can be pressed for. The names in the running text
//! are spans, which carry no `Interaction`; a button per card is the one
//! place a press can land, and the row says at a glance which cards a
//! chapter is about.

use bevy::prelude::*;
use netrunner_client::guide::{self, Block, Guide, Span};

use crate::core::ClientCore;
use crate::nav::{screen_root, Navigate};
use crate::screens::AppScreen;
use crate::theme::{size, Theme};
use crate::widgets::reader::{Readable, Reading};
use crate::skin::Skin;
use crate::widgets::{self, ButtonKind, Dressed, Pressed};

pub struct GuidePlugin;

impl Plugin for GuidePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GuideText(guide::guide()))
            .init_resource::<Chapter>()
            .add_systems(OnEnter(AppScreen::Guide), spawn)
            .add_systems(Update, (press, redraw).chain().run_if(in_state(AppScreen::Guide)));
    }
}

/// The guide, read once.
#[derive(Resource)]
pub struct GuideText(pub Guide);

/// The chapter being read. Kept across visits, so a person who goes to
/// play a lesson comes back to the page they left.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Chapter(pub usize);

/// A chapter's pill, by its index.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterButton(pub usize);

/// A card the chapter names, as a button that reads it.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct GuideCard(pub netrunner_core::dsl::CardId);

/// The column of chapter pills.
#[derive(Component)]
struct Contents;

/// The scrolling page, rebuilt when the chapter changes.
#[derive(Component)]
struct Page;

/// Every line of the chapter on screen, for a test to read what is shown.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct GuideLine(pub String);

#[derive(Component)]
struct BackButton;

fn spawn(mut commands: Commands, theme: Res<Theme>, text: Res<GuideText>, mut chapter: ResMut<Chapter>, core: Res<ClientCore>) {
    let guide = &text.0;
    if chapter.0 >= guide.chapters.len() {
        chapter.0 = 0;
    }
    let mut column = commands.spawn((Contents, widgets::roomy_panel(&theme, px(320))));
    // Its own height, at the top of the row, and never squeezed by the
    // page beside it.
    column.entry::<Node>().and_modify(|mut node| {
        node.align_self = AlignSelf::FlexStart;
        node.flex_shrink = 0.0;
    });
    let contents = column.id();
    fill_contents(&mut commands, contents, &theme, guide, chapter.0);
    let scroll = commands
        .spawn((
            bevy::ui_widgets::ScrollArea,
            Page,
            Node { flex_grow: 1.0, min_width: px(0), max_width: px(940), height: percent(100), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, overflow: Overflow::scroll_y(), ..default() },
        ))
        .id();
    fill_page(&mut commands, scroll, &theme, guide, chapter.0, &core);
    let body = commands
        .spawn(Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, justify_content: JustifyContent::Center, column_gap: px(16), ..default() })
        .add_children(&[contents, scroll])
        .with_children(|body| {
            body.spawn(widgets::scrollbar(&theme, scroll));
        })
        .id();
    let mut root = commands.spawn(screen_root(AppScreen::Guide, &theme));
    root.with_children(|root| {
        root.spawn(widgets::title(&theme, guide.title.clone()));
    });
    root.add_child(body);
    root.with_children(|root| {
        root.spawn(widgets::styled_button(&theme, ButtonKind::Quiet, "Back", Val::Auto, BackButton));
    });
}

/// The chapter pills, the one being read filled.
fn fill_contents(commands: &mut Commands, contents: Entity, theme: &Theme, guide: &Guide, at: usize) {
    commands.entity(contents).despawn_children().with_children(|panel| {
        panel.spawn(widgets::overline(theme, "Chapters"));
        for (index, chapter) in guide.chapters.iter().enumerate() {
            let kind = pill_kind(index, at);
            let mut button = panel.spawn(widgets::styled_button(theme, kind, chapter.title.clone(), percent(100), ChapterButton(index)));
            button.entry::<Node>().and_modify(|mut node| node.max_width = Val::Auto);
        }
    });
}

/// The chapter at `at`: the guide's introduction above the first, the
/// chapter's blocks, then its cards.
fn fill_page(commands: &mut Commands, page: Entity, theme: &Theme, guide: &Guide, at: usize, core: &ClientCore) {
    let Some(chapter) = guide.chapters.get(at) else { return };
    commands.entity(page).despawn_children().with_children(|page| {
        page.spawn(widgets::roomy_panel(theme, percent(100))).with_children(|panel| {
            panel.spawn((GuideLine(chapter.title.clone()), widgets::heading(theme, chapter.title.clone())));
            if at == 0 {
                for block in &guide.intro {
                    block_node(panel, theme, block);
                }
            }
            for block in &chapter.blocks {
                block_node(panel, theme, block);
            }
            let mut named: Vec<(String, netrunner_core::dsl::CardId)> = Vec::new();
            for (title, code) in guide::cards(&Guide { title: String::new(), intro: Vec::new(), chapters: vec![chapter.clone()] }) {
                if let Some(card) = core.registry.get_by_numeric_id(code)
                    && !named.iter().any(|(_, id)| *id == card.id)
                {
                    named.push((title.to_string(), card.id.clone()));
                }
            }
            if !named.is_empty() {
                panel.spawn((widgets::overline(theme, "Cards in this chapter"), Node { margin: UiRect::top(px(12)), ..default() }));
                panel.spawn((widgets::dim(theme, "Press a card to read it."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                panel.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(8), row_gap: px(8), ..default() }).with_children(|row| {
                    for (title, id) in named {
                        row.spawn((widgets::small_button(theme, ButtonKind::Secondary, title, GuideCard(id.clone())), Readable(id)));
                    }
                });
            }
        });
    });
}

/// One block: a section heading, a paragraph, or a bullet, its card
/// names in the accent and its strong words in the text's own colour
/// over a paragraph drawn a shade quieter.
fn block_node(panel: &mut ChildSpawnerCommands, theme: &Theme, block: &Block) {
    let layout = TextLayout::new(Justify::Left, LineBreak::WordBoundary);
    match block {
        Block::Heading(text) => {
            panel.spawn((GuideLine(text.clone()), widgets::overline(theme, text.clone()), Node { margin: UiRect::top(px(14)), ..default() }));
        }
        Block::Paragraph(spans) | Block::Bullet(spans) => {
            let bullet = matches!(block, Block::Bullet(_));
            let prefix = if bullet { "•  " } else { "" };
            panel
                .spawn((
                    GuideLine(block.plain()),
                    Text::new(prefix),
                    theme.font(size::BODY),
                    TextColor(theme.text),
                    layout,
                    Node { margin: if bullet { UiRect::left(px(12)) } else { UiRect::top(px(4)) }, ..default() },
                ))
                .with_children(|text| {
                    for span in spans {
                        let colour = match span {
                            Span::Card { .. } => theme.accent,
                            Span::Strong(_) => theme.text,
                            Span::Text(_) | Span::Link { .. } => theme.text.with_alpha(0.88),
                        };
                        text.spawn((TextSpan::new(span.text()), theme.font(size::BODY), TextColor(colour)));
                    }
                });
        }
    }
}

fn press(
    mut pressed: MessageReader<Pressed>,
    chapters: Query<&ChapterButton>,
    cards: Query<&GuideCard>,
    back: Query<(), With<BackButton>>,
    mut chapter: ResMut<Chapter>,
    mut reading: ResMut<Reading>,
    mut navigate: MessageWriter<Navigate>,
) {
    for Pressed(entity) in pressed.read() {
        if back.contains(*entity) {
            navigate.write(Navigate(AppScreen::Learn));
        } else if let Ok(ChapterButton(index)) = chapters.get(*entity) {
            if chapter.0 != *index {
                chapter.0 = *index;
            }
        } else if let Ok(GuideCard(id)) = cards.get(*entity) {
            reading.0 = Some(id.clone());
        }
    }
}

/// Draws the chapter again when it changes: the page, new and so
/// scrolled to its top, and the pills restyled where they stand.
///
/// **The pills are restyled, never rebuilt.** The pill just pressed still
/// has a dressing queued by `widgets::button_feedback` for its release,
/// and despawning it under that command panics the app — which the
/// navigation test found on its first press.
#[allow(clippy::type_complexity)]
fn redraw(
    mut commands: Commands,
    chapter: Res<Chapter>,
    theme: Res<Theme>,
    skin: Res<Skin>,
    text: Res<GuideText>,
    core: Res<ClientCore>,
    pages: Query<Entity, With<Page>>,
    mut pills: Query<(Entity, &ChapterButton, &mut Dressed, &mut BackgroundColor, &mut BorderColor, &Children)>,
    mut labels: Query<&mut TextColor>,
    mut scroll: Query<&mut ScrollPosition, With<Page>>,
) {
    if !chapter.is_changed() || chapter.is_added() {
        return;
    }
    for (entity, ChapterButton(index), mut dressed, mut background, mut border, children) in &mut pills {
        let ([rest, hover, press], ink) = pill_kind(*index, chapter.0).looks(&theme);
        *dressed = Dressed { slot: dressed.slot, drawn: rest, hover: Some(hover), pressed: Some(press) };
        *background = BackgroundColor(rest.bg);
        *border = BorderColor::all(rest.border);
        skin.dress(dressed.slot, rest).apply(&mut commands.entity(entity));
        for child in children.iter() {
            if let Ok(mut colour) = labels.get_mut(child) {
                *colour = TextColor(ink);
            }
        }
    }
    for entity in &pages {
        fill_page(&mut commands, entity, &theme, &text.0, chapter.0, &core);
    }
    for mut position in &mut scroll {
        *position = ScrollPosition::default();
    }
}

/// The chapter being read is the filled pill.
fn pill_kind(index: usize, at: usize) -> ButtonKind {
    if index == at { ButtonKind::Primary } else { ButtonKind::Secondary }
}
