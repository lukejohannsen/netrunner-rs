//! The table: the field the board is played on, and where the board's
//! depth comes from.
//!
//! **The perspective is painted, not computed.** Every card on the board
//! is one size — `models::layout` has a single face width and the board
//! must still fit the window without scrolling — so a table with a
//! vanishing point in it is what makes the board read as a surface seen
//! from a chair. A per-row scale ramp was the alternative and was
//! rejected: `face_width` binary-searches `rows_height`, which is legal
//! only while that function is monotone in the face width, and the
//! obvious ramp is not (see the third list's preamble in
//! `docs/roadmap/phase-7-desktop-client.md` for the derivative).
//!
//! **Three tiers, like everything else here.** The painted ground below
//! needs no files and so always works; a folder under `assets/tables/`
//! is prettier; a folder under `<data dir>/netrunner/assets/tables/`
//! beats both. A table is a folder rather than a bare image so it can
//! carry a manifest and an overlay — see `netrunner_client::table`,
//! which owns the manifest because it is data and testable without a
//! window.
//!
//! **A table is chosen per match, never per frame.** [`resolve`] is pure
//! and takes the nonce that decides a random pick, so the caller draws
//! one at the start of a game and the field then stays put. A ground
//! that changed under the cards mid-game would be a distraction.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use netrunner_client::settings::{Table, TABLE_PAINTED, TABLE_RANDOM};
use netrunner_client::table::{Manifest, BASE_FILE, MANIFEST_FILE, OVERLAY_FILE};

use crate::assets;
use crate::theme::Theme;

/// Where tables live, under either asset tier.
pub const DIR: &str = "tables";

/// The painted ground's size. It is a wash and a grid with no detail
/// finer than a few pixels, so it is drawn small and stretched to the
/// window rather than repainted on every resize.
pub const PAINTED_WIDTH: u32 = 640;
pub const PAINTED_HEIGHT: u32 = 360;

/// Where the painted ground's horizon sits, as a fraction of its height.
/// Above it is the dark beyond the table's far edge; below it is the
/// surface, receding.
const HORIZON: f32 = 0.30;

/// Every installed table, in the order a settings row cycles them.
///
/// The two reserved names are filtered out rather than quietly shadowing
/// the choices they collide with: `Table` is spelled in the settings
/// file as a bare string, so a folder called `random` could never be
/// selected, and silently listing one would be a row that does nothing.
pub fn available() -> Vec<String> {
    assets::list_dirs(DIR).into_iter().filter(|name| name != TABLE_PAINTED && name != TABLE_RANDOM).collect()
}

/// Which folder a choice resolves to, or `None` for the painted ground.
///
/// Pure, so the fallbacks are testable: a named table whose folder has
/// gone falls back to the painted ground rather than to a random one —
/// a player who named a table meant that table, and surprising them with
/// a different picture is worse than showing them the plain ground and
/// letting them notice. `nonce` decides a random pick and is expected to
/// change once per match.
pub fn resolve(choice: &Table, installed: &[String], nonce: u64) -> Option<String> {
    match choice {
        Table::Painted => None,
        Table::Named(name) => installed.contains(name).then(|| name.clone()),
        Table::Random => {
            let index = usize::try_from(nonce % installed.len().max(1) as u64).ok()?;
            installed.get(index).cloned()
        }
    }
}

/// A folder's manifest, or what a folder without one is worth.
pub fn manifest(folder: &str) -> Manifest {
    match assets::read(&format!("{DIR}/{folder}/{MANIFEST_FILE}")) {
        Some(bytes) => match std::str::from_utf8(&bytes) {
            Ok(text) => Manifest::parse(folder, text),
            Err(_) => Manifest::for_folder(folder),
        },
        None => Manifest::for_folder(folder),
    }
}

/// A folder's field, decoded; `None` when it has none or it will not
/// decode, in which case the caller paints instead.
pub fn base(folder: &str) -> Option<Image> {
    decode_asset(folder, BASE_FILE)
}

/// A folder's overlay — the alpha layer that sits over the field and
/// under the cards. Optional by design; most tables will not have one.
pub fn overlay(folder: &str) -> Option<Image> {
    decode_asset(folder, OVERLAY_FILE)
}

fn decode_asset(folder: &str, file: &str) -> Option<Image> {
    let relative = format!("{DIR}/{folder}/{file}");
    let extension = file.rsplit('.').next()?;
    crate::card_images::decode(&assets::read(&relative)?, extension)
}

/// Marks the two nodes the field is drawn on, so a later item can find
/// them without knowing how the screen was built.
#[derive(Component)]
pub struct Backdrop;

/// The field, stretched over the whole screen behind everything the
/// screen spawns after it.
///
/// Absolutely positioned rather than a flex child, so it takes no part
/// in the board's layout: `models::layout` budgets the window's height
/// down to the pixel, and a backdrop that occupied a row would come
/// straight out of the cards' size.
pub fn backdrop(handle: Handle<Image>) -> impl Bundle {
    (
        Backdrop,
        ImageNode { image_mode: NodeImageMode::Stretch, ..ImageNode::new(handle) },
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::NONE),
    )
}

/// The painted ground: the tier that always works.
///
/// A vertical wash from the theme's background, a grid laid on the
/// surface in true perspective — horizontal lines at even depths, side
/// lines converging on the vanishing point — and a vignette. The grid
/// fades with depth rather than being drawn at full strength all the way
/// to the horizon, because evenly-spaced lines converge faster than the
/// pixels can hold and the result is moiré rather than distance.
pub fn paint(theme: &Theme) -> Image {
    let ground = theme.background.to_srgba();
    let accent = theme.accent.to_srgba();
    let mut data = Vec::with_capacity((PAINTED_WIDTH * PAINTED_HEIGHT * 4) as usize);
    for y in 0..PAINTED_HEIGHT {
        for x in 0..PAINTED_WIDTH {
            let u = x as f32 / (PAINTED_WIDTH - 1) as f32;
            let v = y as f32 / (PAINTED_HEIGHT - 1) as f32;
            let (lift, line) = surface(u, v);
            let vignette = vignette(u, v);
            let mix = |base: f32, accent: f32| ((base * (1.0 + lift) + accent * line * 0.30) * vignette).clamp(0.0, 1.0);
            data.extend_from_slice(&[
                (mix(ground.red, accent.red) * 255.0).round() as u8,
                (mix(ground.green, accent.green) * 255.0).round() as u8,
                (mix(ground.blue, accent.blue) * 255.0).round() as u8,
                255,
            ]);
        }
    }
    Image::new(
        Extent3d { width: PAINTED_WIDTH, height: PAINTED_HEIGHT, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// How much brighter this point is than the flat ground, and how much
/// grid line is on it, for a point at `(u, v)` in the unit square.
///
/// Above the horizon there is no surface: it darkens toward the top, so
/// the far edge of the table reads as an edge. Below it, screen height
/// maps to depth as `1 / (v - horizon)` — the standard ground-plane
/// mapping — which is what puts the grid in real perspective instead of
/// evenly-spaced stripes pretending to be one.
fn surface(u: f32, v: f32) -> (f32, f32) {
    if v <= HORIZON {
        // Above the table: fade to dark toward the top of the screen.
        return (-0.45 * (1.0 - v / HORIZON), 0.0);
    }
    let depth = 1.0 / (v - HORIZON).max(1e-4);
    let across = (u - 0.5) * depth;
    // Near ground is lit, far ground falls away.
    let lift = 0.55 / (1.0 + depth * 0.25) - 0.10;
    // Lines fade with depth, or they converge into moiré.
    let fade = 1.0 / (1.0 + depth * 0.45);
    let line = (ruling(depth * 0.55) + ruling(across * 0.55)).min(1.0) * fade;
    (lift, line)
}

/// How near `t` is to a whole number, as a soft line: 1 on the line and
/// 0 between. Soft because a hard test aliases into dashes once the
/// spacing approaches a pixel.
fn ruling(t: f32) -> f32 {
    let distance = (t - t.round()).abs();
    (1.0 - distance / 0.06).clamp(0.0, 1.0)
}

/// Darkening toward the corners, so the eye is held in the middle where
/// the cards are.
fn vignette(u: f32, v: f32) -> f32 {
    let (dx, dy) = (u - 0.5, v - 0.5);
    (1.0 - 0.75 * (dx * dx + dy * dy)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_painted_ground_is_the_right_size_and_recedes() {
        let image = paint(&Theme::default());
        assert_eq!(image.texture_descriptor.size.width, PAINTED_WIDTH);
        assert_eq!(image.data.as_ref().map(Vec::len), Some((PAINTED_WIDTH * PAINTED_HEIGHT * 4) as usize));
        let luma = |x: u32, y: u32| {
            let i = ((y * PAINTED_WIDTH + x) * 4) as usize;
            let data = image.data.as_ref().unwrap();
            data[i] as u32 + data[i + 1] as u32 + data[i + 2] as u32
        };
        let mid = PAINTED_WIDTH / 2;
        // Near ground is brighter than the far ground, and the dark
        // above the horizon is darker than either: that ordering is the
        // depth cue, and it is the thing worth asserting.
        let near = luma(mid, PAINTED_HEIGHT - 10);
        let far = luma(mid, (PAINTED_HEIGHT as f32 * (HORIZON + 0.06)) as u32);
        let beyond = luma(mid, 2);
        assert!(near > far, "near {near} brighter than far {far}");
        assert!(far > beyond, "far {far} brighter than beyond the edge {beyond}");
        // The corners are darker than the middle of the same row.
        assert!(luma(mid, PAINTED_HEIGHT - 4) > luma(2, PAINTED_HEIGHT - 4));
    }

    #[test]
    fn a_choice_resolves_to_a_folder_or_to_the_painted_ground() {
        let installed = names(&["neon-alley", "orbital"]);
        assert_eq!(resolve(&Table::Painted, &installed, 0), None);
        assert_eq!(resolve(&Table::Named("orbital".to_string()), &installed, 7), Some("orbital".to_string()));
        // A named table that is gone falls back to the painted ground,
        // never to some other picture.
        assert_eq!(resolve(&Table::Named("gone".to_string()), &installed, 7), None);
        // Random walks the installed list and never panics on an empty one.
        assert_eq!(resolve(&Table::Random, &installed, 0), Some("neon-alley".to_string()));
        assert_eq!(resolve(&Table::Random, &installed, 1), Some("orbital".to_string()));
        assert_eq!(resolve(&Table::Random, &installed, 2), Some("neon-alley".to_string()));
        assert_eq!(resolve(&Table::Random, &[], 3), None);
    }

    /// The reserved names cannot be chosen, so they are not offered.
    #[test]
    fn a_folder_named_after_a_reserved_word_is_not_a_table() {
        let installed = names(&["painted", "random", "real-one"]);
        let offered: Vec<&String> = installed.iter().filter(|n| *n != TABLE_PAINTED && *n != TABLE_RANDOM).collect();
        assert_eq!(offered, vec!["real-one"]);
    }
}
