//! The card under the pointer, shown large where it can be read.
//!
//! **A face in a grid is for finding a card; its text is read in the
//! preview.** The deck builder's grids draw cards small enough that a
//! pool of hundreds can be swept, and at that size a card's text cannot
//! be read — the person building a deck could not tell what they were
//! adding. Anything marked [`Previews`] shows its card, at
//! `layout::preview_box`'s size, in the half of the window the pointer
//! is not in, for as long as the pointer rests on it.
//!
//! The preview is a picture and nothing else: `Pickable::IGNORE`, so it
//! never takes a press or a hover from what is under it, and it carries
//! no actions — the secondary click still opens the same card to keep,
//! and the press still does what it did. It is spawned under the screen
//! root, so leaving the screen takes it down with everything else.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use netrunner_client::card_face::Face;
use netrunner_core::dsl::CardId;

use crate::card_images::CardImages;
use crate::core::ClientCore;
use crate::models::layout;
use crate::screens::AppScreen;
use crate::theme::Theme;
use crate::widgets::anchor_of;
use crate::widgets::card_face::{spawn_face, FaceSize};

/// Hovering this shows the card large.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Previews(pub CardId);

/// The preview on screen, and the entity it is showing.
#[derive(Component)]
struct Preview {
    over: Entity,
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Update, preview_hovered);
}

/// Shows the hovered [`Previews`] card, replaces it when the hover moves
/// to another, and takes it down when nothing is hovered. A press counts
/// as a hover so the preview does not blink off while a card is being
/// added. `NETRUNNER_PREVIEW` holds one card previewed for a screenshot.
fn preview_hovered(
    mut commands: Commands,
    (theme, core, images): (Option<Res<Theme>>, Option<Res<ClientCore>>, Option<Res<CardImages>>),
    windows: Query<&Window, With<PrimaryWindow>>,
    hovered: Query<(Entity, &Previews, &Interaction, &ComputedNode, &UiGlobalTransform)>,
    shown: Query<(Entity, &Preview)>,
    roots: Query<Entity, (With<DespawnOnExit<AppScreen>>, With<Node>)>,
    dev: Option<Res<crate::dev::Dev>>,
) {
    let forced = dev.and_then(|dev| dev.preview);
    let target = match forced {
        // Spawn order, which is the grid's reading order.
        Some(n) => {
            let mut all: Vec<_> = hovered.iter().collect();
            all.sort_by_key(|(entity, ..)| *entity);
            // Not before the card is laid out, or it would be placed
            // against a box at the window's corner.
            all.get(n - 1).copied().filter(|(_, _, _, node, _)| node.size().x > 0.0)
        }
        None => hovered.iter().find(|(_, _, interaction, _, _)| matches!(interaction, Interaction::Hovered | Interaction::Pressed)),
    };
    if let Some((entity, ..)) = target
        && shown.iter().any(|(_, preview)| preview.over == entity)
    {
        return;
    }
    for (entity, _) in &shown {
        commands.entity(entity).despawn();
    }
    let (Some((entity, Previews(id), _, node, transform)), Some(theme), Some(core), Some(images)) = (target, theme, core, images) else { return };
    let (Ok(window), Some(root), Some(card)) = (windows.single(), roots.iter().next(), core.registry.get(id)) else { return };
    let (left, top, width) = layout::preview_box((window.width(), window.height()), anchor_of(node, transform));
    // `Board` rather than `Large`: the width is the window's, and a board
    // face's text scales with it when no scan is cached.
    let size = FaceSize::Board(width.round() as u16);
    let image = card.numeric_id.and_then(|code| images.face(code, size));
    commands.entity(root).with_children(|parent| {
        let face = spawn_face(parent, &theme, &Face::of(card), size, image, (Preview { over: entity }, Pickable::IGNORE, GlobalZIndex(40)));
        parent.commands().entity(face).entry::<Node>().and_modify(move |mut node| {
            node.position_type = PositionType::Absolute;
            node.left = px(left);
            node.top = px(top);
        });
    });
}
