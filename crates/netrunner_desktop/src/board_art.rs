//! The board's pictures: what a server's plate shows, in the three
//! tiers every asset in the client has (AGENTS.md §5).
//!
//! **A picture, not a frame.** A skin slot is a nine-sliced frame that a
//! box is dressed in, and it stretches its middle to whatever the box
//! is — the right shape for a border and the wrong one for a building,
//! which has to keep its proportions. So board art is its own tier of
//! files, drawn *inside* the box a slot frames: the plate's picture sits
//! under the plate's label and inside whatever skin dresses the plate.
//!
//! **Where a picture comes from**, first found wins:
//!
//! 1. the active skin's `skins/<folder>/board/<key>.png`, so a skin can
//!    carry its own buildings;
//! 2. `board/<key>.png` under the override directory, then the bundled
//!    one (`assets::read`);
//! 3. the drawn default here, for a base key — so every plate has a
//!    picture with no file anywhere.
//!
//! A state key (`server.hq.run`) is only ever a file: with none drawn it
//! falls back to its base (`server.hq`), one level deep, as a skin slot
//! does. The drawn defaults are deliberately plain — a silhouette in the
//! Corp's colour against a dusk, lit windows — so they read as places at
//! plate size and never pretend to be anybody's art.
//!
//! **Every picture is cropped to cover its box** ([`cover_rect`]): a plate
//! is 16:9 at any card width, but a file need not be, and a stretched
//! building is worse than a cropped one. `assets/board/README.md` lists
//! every key and the size to draw it at.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use netrunner_core::rules::ServerId;

use crate::theme::Theme;

/// Where board art lives, under either asset tier and inside a skin.
pub const DIR: &str = "board";

/// The drawn plates' size: 16:9, twice the plate at the widest card.
pub const PLATE_WIDTH: u32 = 512;
pub const PLATE_HEIGHT: u32 = 288;

/// Every key the board asks for, with the base a state falls back to and
/// the painter a base is drawn by when no file exists.
struct Key {
    key: &'static str,
    base: Option<&'static str>,
    paint: Option<fn(&Theme) -> Image>,
}

const KEYS: &[Key] = &[
    Key { key: "server.archives", base: None, paint: Some(|theme| paint_plate(theme, Building::Archives)) },
    Key { key: "server.rnd", base: None, paint: Some(|theme| paint_plate(theme, Building::RnD)) },
    Key { key: "server.hq", base: None, paint: Some(|theme| paint_plate(theme, Building::Hq)) },
    Key { key: "server.remote", base: None, paint: Some(|theme| paint_plate(theme, Building::Remote)) },
    Key { key: "server.archives.run", base: Some("server.archives"), paint: None },
    Key { key: "server.rnd.run", base: Some("server.rnd"), paint: None },
    Key { key: "server.hq.run", base: Some("server.hq"), paint: None },
    Key { key: "server.remote.run", base: Some("server.remote"), paint: None },
];

/// Every key the board can ask for, for the README's test and for a
/// person listing what to draw.
pub fn keys() -> impl Iterator<Item = &'static str> {
    KEYS.iter().map(|k| k.key)
}

/// A loaded picture and its size in pixels, which the crop needs.
#[derive(Debug, Clone)]
pub struct Picture {
    pub image: Handle<Image>,
    pub size: Vec2,
}

/// The pictures the board draws from, built once per match and again when
/// the skin changes. Absent in the headless tests, which have no
/// `Assets<Image>`: a plate there is its label alone.
#[derive(Resource, Debug, Default, Clone)]
pub struct BoardArt {
    pictures: HashMap<&'static str, Picture>,
    /// The skin folder these were loaded for, so a change is noticed.
    pub skin: Option<String>,
}

impl BoardArt {
    /// Loads every key through the tiers above.
    pub fn load(skin: Option<&str>, theme: &Theme, images: &mut Assets<Image>) -> Self {
        let mut pictures = HashMap::new();
        for entry in KEYS {
            let file = skin
                .and_then(|folder| crate::assets::read(&format!("{}/{folder}/{DIR}/{}.png", crate::skin::DIR, entry.key)))
                .or_else(|| crate::assets::read(&format!("{DIR}/{}.png", entry.key)))
                .and_then(|bytes| crate::card_images::decode(&bytes, "png"));
            let Some(image) = file.or_else(|| entry.paint.map(|paint| paint(theme))) else { continue };
            let size = image.size().as_vec2();
            pictures.insert(entry.key, Picture { image: images.add(image), size });
        }
        Self { pictures, skin: skin.map(str::to_string) }
    }

    /// The picture for `key`, or its base's when the state has none.
    pub fn get(&self, key: &str) -> Option<&Picture> {
        self.pictures.get(key).or_else(|| {
            let base = KEYS.iter().find(|k| k.key == key)?.base?;
            self.pictures.get(base)
        })
    }
}

/// A server's plate key, and its state's when a run is on it.
pub fn server_key(server: ServerId, under_run: bool) -> &'static str {
    match (server, under_run) {
        (ServerId::Archives, false) => "server.archives",
        (ServerId::Archives, true) => "server.archives.run",
        (ServerId::RnD, false) => "server.rnd",
        (ServerId::RnD, true) => "server.rnd.run",
        (ServerId::Hq, false) => "server.hq",
        (ServerId::Hq, true) => "server.hq.run",
        (ServerId::Remote(_), false) => "server.remote",
        (ServerId::Remote(_), true) => "server.remote.run",
    }
}

/// The part of an `image`-sized picture that covers a `target`-sized box
/// without stretching: the largest centred rectangle of the box's shape,
/// in the image's pixels, for `ImageNode::rect`.
pub fn cover_rect(image: Vec2, target: Vec2) -> Rect {
    if image.x <= 0.0 || image.y <= 0.0 || target.x <= 0.0 || target.y <= 0.0 {
        return Rect::from_corners(Vec2::ZERO, image.max(Vec2::ZERO));
    }
    let scale = (target.x / image.x).max(target.y / image.y);
    let crop = target / scale;
    let origin = (image - crop) / 2.0;
    Rect::from_corners(origin, origin + crop)
}

/// The picture as a node filling its parent's box, cropped to cover it,
/// drawn under its siblings: a plate's label is spawned after it.
pub fn backdrop(picture: &Picture, box_size: Vec2) -> impl Bundle {
    (
        ImageNode { rect: Some(cover_rect(picture.size, box_size)), image_mode: NodeImageMode::Stretch, ..ImageNode::new(picture.image.clone()) },
        Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), ..default() },
        bevy::picking::Pickable::IGNORE,
    )
}

// ---- the drawn defaults ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Building {
    Archives,
    RnD,
    Hq,
    Remote,
}

/// Where the ground meets the sky, as a fraction of the height.
const HORIZON: f32 = 0.8;

/// What a pixel of a building is.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Surface {
    Sky,
    Ground,
    /// The body of a building.
    Wall,
    /// A lit window, a data line, a light: the accent.
    Lit,
    /// A door or a recess: darker than the wall.
    Recess,
}

/// A rectangle in unit coordinates, `v` down.
#[derive(Debug, Clone, Copy)]
struct Block {
    u: (f32, f32),
    v: (f32, f32),
}

impl Block {
    const fn new(u0: f32, u1: f32, v0: f32, v1: f32) -> Self {
        Block { u: (u0, u1), v: (v0, v1) }
    }

    fn contains(&self, u: f32, v: f32) -> bool {
        u >= self.u.0 && u < self.u.1 && v >= self.v.0 && v < self.v.1
    }

    /// `(0..1, 0..1)` inside the block.
    fn local(&self, u: f32, v: f32) -> (f32, f32) {
        ((u - self.u.0) / (self.u.1 - self.u.0), (v - self.v.0) / (self.v.1 - self.v.0))
    }
}

/// A window grid: lit where `(column, row)` falls on a window and the
/// deterministic scatter says the light is on.
fn windows(x: f32, y: f32, columns: f32, rows: f32, salt: u32) -> bool {
    let (cx, cy) = (x * columns, y * rows);
    let (fx, fy) = (cx.fract(), cy.fract());
    if !(0.25..0.75).contains(&fx) || !(0.3..0.7).contains(&fy) {
        return false;
    }
    let cell = (cx as u32).wrapping_mul(73_856_093) ^ (cy as u32).wrapping_mul(19_349_663) ^ salt;
    !cell.is_multiple_of(5)
}

/// What `building` puts at `(u, v)`.
fn surface(building: Building, u: f32, v: f32) -> Surface {
    let ground = if v >= HORIZON { Surface::Ground } else { Surface::Sky };
    match building {
        // A low, wide vault under a pediment: shelving in bands, one door.
        Building::Archives => {
            let body = Block::new(0.16, 0.84, 0.5, HORIZON);
            let door = Block::new(0.45, 0.55, 0.62, HORIZON);
            let pediment = (0.36..0.5).contains(&v) && (u - 0.5).abs() < 0.36 * (v - 0.36) / 0.14;
            if door.contains(u, v) {
                return Surface::Recess;
            }
            if body.contains(u, v) {
                let (_, y) = body.local(u, v);
                return if ((y * 6.0).fract() < 0.12) && !(0.42..0.58).contains(&u) { Surface::Lit } else { Surface::Wall };
            }
            if pediment {
                return Surface::Wall;
            }
            ground
        }
        // A tower of data lines between two shorter ones, an antenna on top.
        Building::RnD => {
            let tower = Block::new(0.42, 0.58, 0.16, HORIZON);
            let antenna = Block::new(0.495, 0.505, 0.04, 0.16);
            let left = Block::new(0.26, 0.37, 0.42, HORIZON);
            let right = Block::new(0.63, 0.74, 0.36, HORIZON);
            if antenna.contains(u, v) {
                return if v < 0.06 { Surface::Lit } else { Surface::Wall };
            }
            if tower.contains(u, v) {
                let (x, _) = tower.local(u, v);
                return if (x * 5.0).fract() < 0.18 && x > 0.1 { Surface::Lit } else { Surface::Wall };
            }
            for (block, salt) in [(left, 11), (right, 29)] {
                if block.contains(u, v) {
                    let (x, y) = block.local(u, v);
                    return if windows(x, y, 3.0, 8.0, salt) { Surface::Lit } else { Surface::Wall };
                }
            }
            ground
        }
        // An office block with a setback and a spire: many lit windows.
        Building::Hq => {
            let block = Block::new(0.3, 0.7, 0.24, HORIZON);
            let setback = Block::new(0.38, 0.62, 0.13, 0.24);
            let spire = Block::new(0.495, 0.505, 0.03, 0.13);
            let lobby = Block::new(0.44, 0.56, 0.72, HORIZON);
            if lobby.contains(u, v) {
                return Surface::Lit;
            }
            if spire.contains(u, v) {
                return Surface::Wall;
            }
            for (part, columns, rows, salt) in [(block, 8.0, 11.0, 3), (setback, 5.0, 2.0, 17)] {
                if part.contains(u, v) {
                    let (x, y) = part.local(u, v);
                    return if windows(x, y, columns, rows, salt) { Surface::Lit } else { Surface::Wall };
                }
            }
            ground
        }
        // A server rack in the open: slots with a light each, a mast.
        Building::Remote => {
            let rack = Block::new(0.37, 0.63, 0.36, HORIZON);
            let mast = Block::new(0.6, 0.61, 0.2, 0.36);
            let tip = Block::new(0.595, 0.615, 0.18, 0.2);
            if tip.contains(u, v) {
                return Surface::Lit;
            }
            if mast.contains(u, v) {
                return Surface::Wall;
            }
            if rack.contains(u, v) {
                let (x, y) = rack.local(u, v);
                let slot = (y * 8.0).fract();
                if slot < 0.14 {
                    return Surface::Recess;
                }
                return if (0.82..0.9).contains(&x) && (0.4..0.7).contains(&slot) { Surface::Lit } else { Surface::Wall };
            }
            ground
        }
    }
}

/// A plate's drawn default: `building` against a dusk in the Corp's
/// colour, the lights in the accent.
fn paint_plate(theme: &Theme, building: Building) -> Image {
    let sky = theme.background.to_srgba();
    let corp = theme.corp.to_srgba();
    let accent = theme.accent.to_srgba();
    let mix = |a: Srgba, b: Srgba, t: f32| [a.red + (b.red - a.red) * t, a.green + (b.green - a.green) * t, a.blue + (b.blue - a.blue) * t];
    let scale = |c: Srgba, k: f32| [c.red * k, c.green * k, c.blue * k];
    let mut data = Vec::with_capacity((PLATE_WIDTH * PLATE_HEIGHT * 4) as usize);
    for y in 0..PLATE_HEIGHT {
        for x in 0..PLATE_WIDTH {
            let u = (x as f32 + 0.5) / PLATE_WIDTH as f32;
            let v = (y as f32 + 0.5) / PLATE_HEIGHT as f32;
            let rgb = match surface(building, u, v) {
                // A dusk: the background at the top, the Corp's colour
                // glowing along the horizon.
                Surface::Sky => mix(sky, corp, 0.45 * (v / HORIZON).powi(3)),
                // The ground falls away darker, with a faint grid.
                Surface::Ground => {
                    let depth = (v - HORIZON) / (1.0 - HORIZON);
                    let line = ((u - 0.5) / (0.15 + depth)).fract().abs() < 0.03 || (depth * 6.0).fract() < 0.06;
                    if line { mix(sky, accent, 0.18) } else { scale(sky, 0.8) }
                }
                Surface::Wall => scale(corp, 0.34),
                Surface::Recess => scale(corp, 0.16),
                Surface::Lit => mix(corp, accent, 0.7),
            };
            data.extend(rgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
            data.push(255);
        }
    }
    Image::new(
        Extent3d { width: PLATE_WIDTH, height: PLATE_HEIGHT, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_cropped_to_cover_its_box_and_never_stretched() {
        // Same shape: the whole picture.
        let whole = cover_rect(Vec2::new(512.0, 288.0), Vec2::new(256.0, 144.0));
        assert_eq!((whole.min, whole.max), (Vec2::ZERO, Vec2::new(512.0, 288.0)));
        // A square picture in a wide box: the full width, a centred band.
        let band = cover_rect(Vec2::new(400.0, 400.0), Vec2::new(160.0, 90.0));
        assert_eq!(band.width(), 400.0);
        assert!((band.height() - 225.0).abs() < 1e-3 && (band.min.y - 87.5).abs() < 1e-3, "{band:?}");
        // A wide picture in a tall box: the full height, a centred column.
        let column = cover_rect(Vec2::new(1000.0, 100.0), Vec2::new(50.0, 100.0));
        assert_eq!(column.height(), 100.0);
        assert!((column.width() - 50.0).abs() < 1e-3 && (column.min.x - 475.0).abs() < 1e-3, "{column:?}");
        // Degenerate sizes do not divide by zero.
        assert_eq!(cover_rect(Vec2::new(10.0, 10.0), Vec2::ZERO).max, Vec2::new(10.0, 10.0));
    }

    /// The drawn tier: every base key has a painter, at the plate's size,
    /// and no two buildings are the same picture.
    #[test]
    fn every_server_has_a_drawn_plate_and_they_differ() {
        let theme = Theme::default();
        let plates: Vec<Image> = [Building::Archives, Building::RnD, Building::Hq, Building::Remote].into_iter().map(|b| paint_plate(&theme, b)).collect();
        for plate in &plates {
            assert_eq!(plate.size(), UVec2::new(PLATE_WIDTH, PLATE_HEIGHT));
        }
        for (i, a) in plates.iter().enumerate() {
            for b in plates.iter().skip(i + 1) {
                assert_ne!(a.data, b.data, "two servers drew the same building");
            }
        }
        for key in KEYS {
            assert!(key.paint.is_some() != key.base.is_some(), "{}: a base is drawn, a state falls back", key.key);
        }
    }

    /// The guide names every key the board asks for, so a person drawing
    /// from it never misses one and a new key cannot land undocumented.
    #[test]
    fn the_guide_lists_every_key() {
        let guide = include_str!("../assets/board/README.md");
        for key in keys() {
            assert!(guide.contains(&format!("`{key}`")), "assets/board/README.md does not list `{key}`");
        }
    }

    /// With no files anywhere, a state finds its base's drawn picture.
    #[test]
    fn a_state_with_no_picture_falls_back_to_its_base() {
        let mut images = Assets::<Image>::default();
        let art = BoardArt { skin: None, pictures: HashMap::new() };
        assert!(art.get("server.hq.run").is_none());
        let loaded = BoardArt::load(Some("no-such-skin"), &Theme::default(), &mut images);
        let base = loaded.get("server.hq").expect("drawn").image.clone();
        assert_eq!(loaded.get("server.hq.run").expect("falls back").image, base);
        assert_eq!(loaded.get(server_key(ServerId::Remote(3), true)).unwrap().image, loaded.get("server.remote").unwrap().image);
        assert_eq!(loaded.get("server.remote").unwrap().size, Vec2::new(PLATE_WIDTH as f32, PLATE_HEIGHT as f32));
    }
}
