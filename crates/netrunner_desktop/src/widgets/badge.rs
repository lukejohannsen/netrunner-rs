//! A badge's glyph (Phase 7 §14): the small picture a badge earned on a
//! server is, wherever it is drawn — the profile page's line and the
//! pop-up a badge is picked from.
//!
//! **Three tiers, as every asset** (`assets/README.md`): the player's own
//! `badges/<stem>.png` under the data directory, then a bundled one under
//! `assets/badges/`, then a drawn glyph that always works — a ring in a
//! colour the badge's name picks, the name's initials inside. A badge is
//! named by the server that grants it (Phase 4 §11), so the stem is the
//! name made safe for a file (`identity::badge_stem`), and a badge nobody has drawn is
//! still a distinct mark rather than a blank. Basic graphics draws the
//! ring alone, as it loads no pictures anywhere.
//!
//! **Decoded once a name** and kept for the session (`BadgeGlyphs`), the
//! way the card backs are, through `assets::read` and
//! `card_images::decode` rather than Bevy's asset server, which reads
//! neither the data directory nor a path built at run time.

use std::collections::HashMap;

use bevy::prelude::*;

use netrunner_client::identity::{badge_initials, badge_stem};

use crate::theme::{size, Theme};

/// Each badge's picture, by name, once looked for: `None` is "draw it".
#[derive(Resource, Default)]
pub struct BadgeGlyphs(HashMap<String, Option<Handle<Image>>>);

impl BadgeGlyphs {
    /// The picture for `name`, decoded on first asking; `None` with basic
    /// graphics, with no image store (a test under `MinimalPlugins`), or
    /// when no tier has a file for it.
    pub fn of(&mut self, name: &str, basic: bool, images: Option<&mut Assets<Image>>) -> Option<Handle<Image>> {
        let images = images.filter(|_| !basic)?;
        self.0
            .entry(name.to_string())
            .or_insert_with(|| {
                let bytes = crate::assets::read(&format!("badges/{}.png", badge_stem(name)))?;
                crate::card_images::decode(&bytes, "png").map(|image| images.add(image))
            })
            .clone()
    }
}

/// The ring's colour, picked by the name so a badge keeps its colour
/// wherever it is drawn: one of the theme's own accents.
fn ring(theme: &Theme, name: &str) -> Color {
    let palette = [theme.accent, theme.primary, theme.legal, theme.caution, theme.corp, theme.runner];
    let sum = name.bytes().fold(0usize, |sum, byte| sum.wrapping_mul(31).wrapping_add(byte as usize));
    palette[sum % palette.len()]
}

/// The glyph, `side` logical pixels square.
pub fn spawn_glyph(parent: &mut ChildSpawnerCommands, theme: &Theme, name: &str, picture: Option<Handle<Image>>, side: f32) {
    let node = Node { width: px(side), height: px(side), flex_shrink: 0.0, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() };
    match picture {
        Some(image) => {
            parent.spawn((node, ImageNode { image, image_mode: NodeImageMode::Auto, ..default() }));
        }
        None => {
            let colour = ring(theme, name);
            parent
                .spawn((
                    Node { border: UiRect::all(px((side / 16.0).max(2.0))), border_radius: BorderRadius::MAX, ..node },
                    BorderColor::all(colour),
                    BackgroundColor(colour.with_alpha(0.18)),
                ))
                .with_children(|glyph| {
                    glyph.spawn((Text::new(badge_initials(name)), theme.font((side * 0.36).max(size::SMALL)), TextColor(theme.text)));
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_a_file_stem_and_two_letters() {
        assert_eq!(badge_stem("first-gauntlet"), "first-gauntlet");
        assert_eq!(badge_stem("First Gauntlet!"), "first-gauntlet");
        assert_eq!(badge_stem("  Ten  wins: Standard "), "ten-wins-standard");
        assert_eq!(badge_initials("first-gauntlet"), "FG");
        assert_eq!(badge_initials("Founder"), "FO");
        assert_eq!(badge_initials("!!"), "");
    }
}
