//! The picture behind every screen that is not the board: the splash,
//! the main menu and each screen off it, the deck builder when it comes.
//!
//! **Every screen has a slot before anybody has drawn for it.**
//! `nav::screen_root` puts a [`ScreenBackdrop`] on every screen's root,
//! so a screen gets its slot by being built the way every screen is, and
//! [`AppScreen::asset_key`] names the slot by an exhaustive match, so a
//! new screen does not compile until it has one. The guide
//! (`assets/backdrops/README.md`) is checked to list every key.
//!
//! **Where a picture comes from** is `netrunner_client::backdrop`'s rule:
//! the screen's own `backdrops/<key>.jpg` (or `.png`), then the shared
//! `backdrops/menu.jpg` that dresses every screen nobody drew for, each
//! looked up under the override directory and then the bundled one
//! (`assets::read`). With neither, the screen is its flat ground — the
//! drawn tier, which is the fallback and, under basic graphics, the whole
//! of it.
//!
//! **Cropped to cover, never stretched** (`board_art::cover_rect`). A
//! table is stretched because its perspective was painted for the window
//! and a crop would move its vanishing point; a backdrop has no vanishing
//! point, and a menu's content is centred, so a crop that keeps the
//! middle is right on every aspect. The guide says to keep the middle
//! 16:10 busy and the edges expendable.
//!
//! **Dimmed under the screen's text** by a scrim in the theme's ground,
//! as much as `backdrops/backdrops.json` asks for the key — a bright
//! picture behind light text is the failure, and the author is the one
//! who knows how bright theirs is.

use std::collections::HashMap;

use bevy::prelude::*;

use netrunner_client::backdrop::{candidates, Manifest, DIR, MANIFEST_FILE};

use crate::core::ClientCore;
use crate::screens::AppScreen;
use crate::theme::Theme;

pub struct BackdropPlugin;

impl Plugin for BackdropPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Backdrops>().add_systems(Update, (dress, fit).chain());
    }
}

/// On a screen's root: the screen whose backdrop slot this is.
#[derive(Component, Debug, Clone, Copy)]
pub struct ScreenBackdrop(pub AppScreen);

/// The picture node, with the picture's size, which the crop needs.
#[derive(Component)]
pub struct BackdropPicture(pub Vec2);

/// The pictures decoded so far, by key, so going back and forth between
/// two screens does not decode a full-window JPEG each time. Emptied when
/// basic graphics is switched, which changes every answer.
#[derive(Resource, Default)]
pub struct Backdrops {
    pictures: HashMap<String, Option<(Handle<Image>, Vec2)>>,
    manifest: Option<Manifest>,
    basic: bool,
}

impl Backdrops {
    /// `key`'s picture, loading it on first ask; `None` is the flat
    /// ground. Cached under the key whose files it came from, so two
    /// screens showing one picture (`Manifest::same_as`) decode it once.
    pub fn picture(&mut self, key: &str, basic: bool, images: &mut Assets<Image>) -> Option<(Handle<Image>, Vec2)> {
        if basic != self.basic {
            self.pictures.clear();
            self.basic = basic;
        }
        if basic {
            return None;
        }
        let key = self.manifest().picture_key(key).to_string();
        self.pictures.entry(key).or_insert_with_key(|key| load(key).map(|image| {
            let size = image.size().as_vec2();
            (images.add(image), size)
        })).clone()
    }

    /// How much `key`'s picture is dimmed, from the manifest read once.
    pub fn dim(&mut self, key: &str) -> f32 {
        self.manifest().dim(key)
    }

    /// The manifest, read on first ask.
    fn manifest(&mut self) -> &Manifest {
        self.manifest.get_or_insert_with(|| crate::assets::read(&format!("{DIR}/{MANIFEST_FILE}")).and_then(|bytes| String::from_utf8(bytes).ok()).map_or_else(Manifest::default, |text| Manifest::parse(&text)))
    }
}

/// The first of `key`'s candidate files that exists and decodes. A file
/// that will not decode is passed over for the next, so a broken
/// `cards.jpg` shows the shared picture rather than nothing.
fn load(key: &str) -> Option<Image> {
    candidates(key).into_iter().find_map(|relative| {
        let extension = relative.rsplit('.').next()?.to_string();
        crate::card_images::decode(&crate::assets::read(&relative)?, &extension)
    })
}

/// Dresses each new screen root in its picture and the scrim over it,
/// both inserted as the root's first children so everything the screen
/// spawned paints over them.
///
/// Nothing without `Assets<Image>`, which is the headless tests: every
/// screen there is its flat ground.
fn dress(
    mut commands: Commands,
    roots: Query<(Entity, &ScreenBackdrop), Added<ScreenBackdrop>>,
    core: Res<ClientCore>,
    theme: Res<Theme>,
    mut backdrops: ResMut<Backdrops>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let Some(mut images) = images else { return };
    for (root, ScreenBackdrop(screen)) in &roots {
        let Some(key) = screen.asset_key() else { continue };
        let Some((image, size)) = backdrops.picture(key, core.settings.desktop.basic_graphics, &mut images) else { continue };
        let fill = || Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), ..default() };
        let picture = commands.spawn((BackdropPicture(size), ImageNode { image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }, fill(), bevy::picking::Pickable::IGNORE)).id();
        let scrim = commands.spawn((BackgroundColor(theme.background.with_alpha(backdrops.dim(key))), fill(), bevy::picking::Pickable::IGNORE)).id();
        commands.entity(root).insert_children(0, &[picture, scrim]);
    }
}

/// Keeps each picture cropped to its box as the window changes. Cheap: a
/// comparison per backdrop per frame, and there is one on screen.
fn fit(mut pictures: Query<(&BackdropPicture, &ComputedNode, &mut ImageNode)>) {
    for (BackdropPicture(size), node, mut image) in &mut pictures {
        let rect = crate::board_art::cover_rect(*size, node.size());
        if image.rect != Some(rect) {
            image.rect = Some(rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guide names every screen's slot, so a new screen cannot land
    /// without somewhere documented to put its picture.
    #[test]
    fn the_guide_lists_every_screen() {
        let guide = include_str!("../assets/backdrops/README.md");
        for screen in AppScreen::ALL {
            if let Some(key) = screen.asset_key() {
                assert!(guide.contains(&format!("`{key}`")), "assets/backdrops/README.md does not list `{key}` ({screen:?})");
            }
        }
        assert!(guide.contains(&format!("`{}`", netrunner_client::backdrop::SHARED_KEY)));
    }

    /// Only the two screens that are never a menu go without a slot, and
    /// no two screens share one.
    #[test]
    fn every_menu_screen_has_its_own_slot() {
        let keys: Vec<&str> = AppScreen::ALL.into_iter().filter_map(AppScreen::asset_key).collect();
        let missing: Vec<AppScreen> = AppScreen::ALL.into_iter().filter(|screen| screen.asset_key().is_none()).collect();
        assert_eq!(missing, vec![AppScreen::Boot, AppScreen::Game]);
        let mut unique = keys.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), keys.len(), "two screens share a slot: {keys:?}");
        assert!(!keys.contains(&netrunner_client::backdrop::SHARED_KEY), "the shared key is not a screen's");
    }

    /// Basic graphics is the flat ground, and switching it back loads
    /// again rather than remembering the answer.
    #[test]
    fn basic_graphics_is_the_flat_ground() {
        let mut backdrops = Backdrops::default();
        let mut images = Assets::<Image>::default();
        assert!(backdrops.picture("main-menu", true, &mut images).is_none());
        assert!(backdrops.basic);
        let _ = backdrops.picture("main-menu", false, &mut images);
        assert!(!backdrops.basic && backdrops.pictures.contains_key("main-menu"));
    }

    /// About, Settings and the deck editor show another screen's picture,
    /// and that picture's file is committed.
    #[test]
    fn a_borrowed_picture_is_a_committed_one() {
        let manifest = Manifest::parse(include_str!("../assets/backdrops/backdrops.json"));
        for (screen, picture) in [("about", "main-menu"), ("settings", "main-menu"), ("deck-editor", "decks")] {
            assert_eq!(manifest.picture_key(screen), picture);
            assert!(crate::assets::read(&format!("{DIR}/{picture}.jpg")).is_some(), "{picture}.jpg is committed");
        }
    }
}
