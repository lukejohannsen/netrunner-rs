//! The board's pictures: what a server's plate shows, the slab behind an
//! ICE's or a root card's name, the badge beside a counter and a HUD
//! readout's glyph — in the three tiers every asset in the
//! client has (AGENTS.md §5).
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
//! 2. the Corp's style, `board/corp/<faction>/<key>.png`, chosen by the
//!    faction of the Corp's identity — so a Jinteki board can stand on
//!    Jinteki buildings and a Weyland one on Weyland's. Any key may be
//!    styled, not only the plates: a faction folder holding one picture
//!    is a real style, and every key it leaves out is the generic one;
//! 3. `board/<key>.png`;
//! 4. the drawn default here — so every plate, tile and counter has a
//!    picture with no file anywhere. The drawn plates are lit in the
//!    Corp's faction colour, so the style shows before anybody draws it.
//!
//! Each of 1–3 is looked up under the override directory, then the
//! bundled one (`assets::read`). A state with no file falls back to its
//! base, one level deep, as a skin slot does — `server.hq.run` to
//! `server.hq`, `counter.virus` to `counter` — **inside a layer before
//! the next layer is tried**: a faction's HQ beats the generic HQ under a
//! run, or a Jinteki board would turn generic every time it was attacked.
//!
//! **Under basic graphics nothing is read from a file** and every key is
//! its drawn default: the no-frills client for a slow machine. A drawn
//! default is otherwise only the fallback, never the look the client
//! ships. A HUD glyph has
//! neither — they are optional, and a board without them is the words
//! it always had. The drawn defaults are deliberately plain — a
//! silhouette in the Corp's colour, a pattern in grey washed in the
//! card's own colour — so they read at board size and never pretend to
//! be anybody's art.
//!
//! **The bundled counters and HUD glyphs are Null Signal Games'
//! own game symbols** (CC BY-ND 4.0), converted to PNG and the black
//! ones recoloured light for a dark board, which NSG's terms name as not
//! a derivative. Like every committed asset they are a separately
//! licensed work with a row in `assets/CREDITS.md`
//! (`assets/board/LICENSE-NSG.txt` holds the attribution).
//!
//! **A drawn tile is grey and washed in its state's colour**
//! ([`Picture::drawn`]): the faction's for a rezzed card, the Corp's
//! dimmed for one face down — the same colour the tile's border carries.
//! A file is drawn as its author painted it.
//!
//! **Every picture is cropped to cover its box** ([`cover_rect`]): a plate
//! is 16:9 at any card width, but a file need not be, and a stretched
//! building is worse than a cropped one. `assets/board/README.md` lists
//! every key and the size to draw it at.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use netrunner_core::card::Faction;
use netrunner_core::rules::ServerId;

use crate::theme::Theme;

/// Where board art lives, under either asset tier and inside a skin.
pub const DIR: &str = "board";

/// Where the Corp's per-faction styles live, under [`DIR`].
pub const CORP_DIR: &str = "corp";

/// Every faction a Corp's style can be chosen by, for the guide's test
/// and for a person listing folders to draw.
pub const CORP_FACTIONS: [Faction; 5] = [Faction::HaasBioroid, Faction::Jinteki, Faction::Nbn, Faction::WeylandConsortium, Faction::NeutralCorp];

/// The folder a Corp faction's style lives in, `None` for a Runner's.
///
/// Spelled out rather than derived from the variant's name so a rename
/// in the engine cannot move somebody's art out from under them.
pub fn faction_slug(faction: Faction) -> Option<&'static str> {
    match faction {
        Faction::HaasBioroid => Some("haas-bioroid"),
        Faction::Jinteki => Some("jinteki"),
        Faction::Nbn => Some("nbn"),
        Faction::WeylandConsortium => Some("weyland-consortium"),
        Faction::NeutralCorp => Some("neutral-corp"),
        Faction::Anarch | Faction::Criminal | Faction::Shaper | Faction::NeutralRunner => None,
    }
}

/// The drawn plates' size: 16:9, twice the plate at the widest card.
pub const PLATE_WIDTH: u32 = 512;
pub const PLATE_HEIGHT: u32 = 288;
/// The drawn tiles' size: 4:1, twice the tallest tile at the widest card.
pub const TILE_WIDTH: u32 = 512;
pub const TILE_HEIGHT: u32 = 128;
/// The drawn counter badge's size: twice the badge.
pub const BADGE: u32 = 64;
/// The avatar bar's wing, at twice its logical size: [`BAR_CAP`] at
/// each end and a plain middle that stretches. Both caps are the same
/// width because the right-hand wing is the same picture mirrored, and a
/// nine-slice is mirrored exactly only when its two ends match.
pub const BAR_WIDTH: u32 = 480;
pub const BAR_HEIGHT: u32 = 96;
pub const BAR_CAP: u32 = 160;
/// The ring round the avatar, square.
pub const FRAME: u32 = 256;

/// Every key the board asks for, with the base a state falls back to and
/// the painter a base is drawn by when no file exists.
struct Key {
    key: &'static str,
    base: Option<&'static str>,
    paint: Option<fn(&Theme, Option<Faction>) -> Image>,
}

const KEYS: &[Key] = &[
    Key { key: "server.archives", base: None, paint: Some(|theme, faction| paint_plate(theme, faction, Building::Archives)) },
    Key { key: "server.rnd", base: None, paint: Some(|theme, faction| paint_plate(theme, faction, Building::RnD)) },
    Key { key: "server.hq", base: None, paint: Some(|theme, faction| paint_plate(theme, faction, Building::Hq)) },
    Key { key: "server.remote", base: None, paint: Some(|theme, faction| paint_plate(theme, faction, Building::Remote)) },
    Key { key: "server.archives.run", base: Some("server.archives"), paint: None },
    Key { key: "server.rnd.run", base: Some("server.rnd"), paint: None },
    Key { key: "server.hq.run", base: Some("server.hq"), paint: None },
    Key { key: "server.remote.run", base: Some("server.remote"), paint: None },
    Key { key: "ice.unrezzed", base: None, paint: Some(|_, _| paint_tile(Pattern::Hatch)) },
    Key { key: "ice.rezzed", base: None, paint: Some(|_, _| paint_tile(Pattern::Scanlines)) },
    Key { key: "ice.rezzed.barrier", base: Some("ice.rezzed"), paint: Some(|_, _| paint_tile(Pattern::Bricks)) },
    Key { key: "ice.rezzed.code-gate", base: Some("ice.rezzed"), paint: Some(|_, _| paint_tile(Pattern::Gate)) },
    Key { key: "ice.rezzed.sentry", base: Some("ice.rezzed"), paint: Some(|_, _| paint_tile(Pattern::Rings)) },
    Key { key: "root.unrezzed", base: None, paint: Some(|_, _| paint_tile(Pattern::BackHatch)) },
    Key { key: "root.rezzed", base: None, paint: Some(|_, _| paint_tile(Pattern::Rivets)) },
    Key { key: "root.rezzed.asset", base: Some("root.rezzed"), paint: Some(|_, _| paint_tile(Pattern::Coins)) },
    Key { key: "root.rezzed.upgrade", base: Some("root.rezzed"), paint: Some(|_, _| paint_tile(Pattern::Chevrons)) },
    Key { key: "root.agenda", base: None, paint: Some(|_, _| paint_tile(Pattern::Diamonds)) },
    Key { key: "counter", base: None, paint: Some(|_, _| paint_badge()) },
    Key { key: "counter.advancement", base: Some("counter"), paint: None },
    Key { key: "counter.virus", base: Some("counter"), paint: None },
    Key { key: "counter.power", base: Some("counter"), paint: None },
    Key { key: "counter.credit", base: Some("counter"), paint: None },
    Key { key: "hud.credits", base: None, paint: None },
    Key { key: "hud.clicks", base: None, paint: None },
    Key { key: "hud.agendas", base: None, paint: None },
    Key { key: "hud.bad-publicity", base: None, paint: None },
    Key { key: "hud.tags", base: None, paint: None },
    Key { key: "hud.damage", base: None, paint: None },
    Key { key: "avatar.bar", base: None, paint: Some(|_, _| paint_bar()) },
    Key { key: "avatar.bar.active", base: Some("avatar.bar"), paint: None },
    Key { key: "avatar.frame", base: None, paint: Some(|_, _| paint_frame()) },
    Key { key: "avatar.frame.active", base: Some("avatar.frame"), paint: None },
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
    /// Whether this is the drawn default rather than a file: a drawn
    /// tile is grey, and is washed in its state's colour.
    pub drawn: bool,
}

impl Picture {
    /// The colour to draw the picture in: `state` for a drawn default,
    /// white — the picture as painted — for a file.
    pub fn tint(&self, state: Color) -> Color {
        if self.drawn { state } else { Color::WHITE }
    }
}

/// The pictures the board draws from, built once per match and again when
/// the skin changes. Absent in the headless tests, which have no
/// `Assets<Image>`: a plate there is its label alone.
#[derive(Resource, Debug, Default, Clone)]
pub struct BoardArt {
    pictures: HashMap<&'static str, Picture>,
    /// What these were loaded for, so a change is noticed.
    pub loaded_for: Style,
}

/// Everything that decides which pictures the board loads: the skin
/// folder, the Corp's faction and whether graphics are basic.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Style {
    pub skin: Option<String>,
    pub faction: Option<Faction>,
    pub basic: bool,
}

impl BoardArt {
    /// Loads every key through the tiers above.
    pub fn load(style: Style, theme: &Theme, images: &mut Assets<Image>) -> Self {
        let mut pictures = HashMap::new();
        let layers = if style.basic { Vec::new() } else { layers(style.skin.as_deref(), style.faction) };
        for entry in KEYS {
            let file = layers
                .iter()
                .find_map(|layer| entry_files(entry).find_map(|key| crate::assets::read(&format!("{layer}/{key}.png"))))
                .and_then(|bytes| crate::card_images::decode(&bytes, "png"));
            let drawn = file.is_none();
            let Some(image) = file.or_else(|| entry.paint.map(|paint| paint(theme, style.faction))) else { continue };
            let size = image.size().as_vec2();
            pictures.insert(entry.key, Picture { image: images.add(image), size, drawn });
        }
        Self { pictures, loaded_for: style }
    }

    /// The picture for `key`, or its base's when the state has none.
    pub fn get(&self, key: &str) -> Option<&Picture> {
        self.pictures.get(key).or_else(|| {
            let base = KEYS.iter().find(|k| k.key == key)?.base?;
            self.pictures.get(base)
        })
    }
}

/// The folders a picture is looked for in, first found wins: the skin's,
/// the Corp faction's, then the generic one.
fn layers(skin: Option<&str>, faction: Option<Faction>) -> Vec<String> {
    let mut layers = Vec::new();
    if let Some(folder) = skin {
        layers.push(format!("{}/{folder}/{DIR}", crate::skin::DIR));
    }
    if let Some(slug) = faction.and_then(faction_slug) {
        layers.push(format!("{DIR}/{CORP_DIR}/{slug}"));
    }
    layers.push(DIR.to_string());
    layers
}

/// A key's own file name and then its base's, which is the order a state
/// is looked for inside one layer.
fn entry_files(entry: &Key) -> impl Iterator<Item = &'static str> {
    std::iter::once(entry.key).chain(entry.base)
}

/// The colour a Corp's drawn buildings are lit in: its faction's, or the
/// Corp's own for a neutral identity or none — a neutral's grey would
/// read as a board with the lights off.
fn corp_colour(theme: &Theme, faction: Option<Faction>) -> Color {
    match faction {
        Some(faction) if faction_slug(faction).is_some() && faction != Faction::NeutralCorp => theme.faction(Some(faction)),
        _ => theme.corp,
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

/// An ICE tile's key: its state, and a rezzed one's type.
pub fn ice_key(rezzed: bool, kind: Option<netrunner_core::dsl::IceType>) -> &'static str {
    use netrunner_core::dsl::IceType;
    match (rezzed, kind) {
        (false, _) => "ice.unrezzed",
        (true, Some(IceType::Barrier)) => "ice.rezzed.barrier",
        (true, Some(IceType::CodeGate)) => "ice.rezzed.code-gate",
        (true, Some(IceType::Sentry)) => "ice.rezzed.sentry",
        (true, None) => "ice.rezzed",
    }
}

/// A root card's key: an agenda is always itself, face up or not to the
/// viewer; a rezzed asset or upgrade is its type.
pub fn root_key(face_up: bool, card_type: Option<&netrunner_core::dsl::CardType>) -> &'static str {
    use netrunner_core::dsl::CardType;
    match (face_up, card_type) {
        (_, Some(CardType::Agenda)) => "root.agenda",
        (false, _) => "root.unrezzed",
        (true, Some(CardType::Asset)) => "root.rezzed.asset",
        (true, Some(CardType::Upgrade)) => "root.rezzed.upgrade",
        (true, _) => "root.rezzed",
    }
}

/// A token's badge key.
pub fn token_key(kind: netrunner_client::board::TokenKind) -> &'static str {
    use netrunner_client::board::TokenKind;
    use netrunner_core::dsl::CounterKind;
    match kind {
        TokenKind::Advancement => "counter.advancement",
        TokenKind::Counter(Some(CounterKind::Virus)) => "counter.virus",
        TokenKind::Counter(Some(CounterKind::Power)) => "counter.power",
        TokenKind::Counter(Some(CounterKind::Credit)) => "counter.credit",
        TokenKind::Counter(None) => "counter",
    }
}

/// A HUD readout's glyph, by the readout's label (`hud::readouts`).
pub fn hud_key(label: &str) -> Option<&'static str> {
    Some(match label {
        "Credits" => "hud.credits",
        "Clicks" => "hud.clicks",
        "Agendas" => "hud.agendas",
        "Bad pub." => "hud.bad-publicity",
        "Tags" => "hud.tags",
        "Core damage" => "hud.damage",
        _ => return None,
    })
}

/// A glyph at `size` logical pixels, kept to its shape.
pub fn glyph(picture: &Picture, size: f32) -> impl Bundle {
    (
        ImageNode { image_mode: NodeImageMode::Auto, ..ImageNode::new(picture.image.clone()) },
        Node { width: px(size), height: px(size), flex_shrink: 0.0, ..default() },
        bevy::picking::Pickable::IGNORE,
    )
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
/// drawn under its siblings: a plate's label is spawned after it. `tint`
/// is the state's colour, which only a drawn picture takes.
pub fn backdrop(picture: &Picture, box_size: Vec2, tint: Color) -> impl Bundle {
    (
        ImageNode { rect: Some(cover_rect(picture.size, box_size)), image_mode: NodeImageMode::Stretch, color: picture.tint(tint), ..ImageNode::new(picture.image.clone()) },
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
/// faction colour, the lights in the accent.
fn paint_plate(theme: &Theme, faction: Option<Faction>, building: Building) -> Image {
    let sky = theme.background.to_srgba();
    let corp = corp_colour(theme, faction).to_srgba();
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

/// A drawn tile's pattern: one per key, in grey, so the tile's state
/// colour washes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pattern {
    /// Face-down ICE: diagonal hatching, the back of a card.
    Hatch,
    /// Face-down root card: hatching the other way.
    BackHatch,
    /// Rezzed ICE of no type the board knows: scanlines.
    Scanlines,
    /// A barrier: a wall of bricks.
    Bricks,
    /// A code gate: bars with a lock in the middle.
    Gate,
    /// A sentry: rings around a sight.
    Rings,
    /// A rezzed root card: a riveted panel.
    Rivets,
    /// An asset: a row of coins.
    Coins,
    /// An upgrade: chevrons pointing up.
    Chevrons,
    /// An agenda: diamonds.
    Diamonds,
}

/// How bright the pattern is at pixel `(x, y)` of a tile, 0..1.
fn pattern(pattern: Pattern, x: f32, y: f32) -> f32 {
    let (w, h) = (TILE_WIDTH as f32, TILE_HEIGHT as f32);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let low = 0.28;
    let high = 0.62;
    let on = |yes: bool| if yes { high } else { low };
    match pattern {
        Pattern::Hatch => on((x + y).rem_euclid(28.0) < 7.0),
        Pattern::BackHatch => on((x - y).rem_euclid(28.0) < 7.0),
        Pattern::Scanlines => on(y.rem_euclid(10.0) < 3.0),
        Pattern::Bricks => {
            let row = (y / 32.0).floor();
            let offset = if row as i32 % 2 == 0 { 0.0 } else { 32.0 };
            on(!(y.rem_euclid(32.0) < 5.0 || (x + offset).rem_euclid(64.0) < 5.0))
        }
        Pattern::Gate => {
            let lock = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            if lock < 34.0 {
                return if lock < 12.0 || ((x - cx).abs() < 5.0 && y > cy) { 0.12 } else { 0.72 };
            }
            on(x.rem_euclid(40.0) < 10.0)
        }
        Pattern::Rings => {
            let d = ((x - cx).powi(2) + ((y - cy) * 1.0).powi(2)).sqrt();
            let sight = (x - cx).abs() < 2.0 || (y - cy).abs() < 2.0;
            on(d.rem_euclid(22.0) < 4.0 || (sight && d < 60.0))
        }
        Pattern::Rivets => {
            let (fx, fy) = (x.rem_euclid(48.0) - 24.0, y.rem_euclid(48.0) - 24.0);
            on(fx * fx + fy * fy < 36.0)
        }
        Pattern::Coins => {
            let (fx, fy) = (x.rem_euclid(64.0) - 32.0, y.rem_euclid(64.0) - 32.0);
            let r = (fx * fx + fy * fy).sqrt();
            on(r < 18.0 && r > 12.0 || r < 6.0)
        }
        Pattern::Chevrons => on((x.rem_euclid(56.0) - 28.0).abs() + y.rem_euclid(40.0) < 34.0 && (x.rem_euclid(56.0) - 28.0).abs() + y.rem_euclid(40.0) > 24.0),
        Pattern::Diamonds => {
            let (fx, fy) = (x.rem_euclid(64.0) - 32.0, y.rem_euclid(64.0) - 32.0);
            on(fx.abs() + fy.abs() < 16.0)
        }
    }
}

/// A drawn tile: `kind` in grey, darker toward the ends so a label in
/// the middle reads, for the tile's state colour to wash.
fn paint_tile(kind: Pattern) -> Image {
    let mut data = Vec::with_capacity((TILE_WIDTH * TILE_HEIGHT * 4) as usize);
    for y in 0..TILE_HEIGHT {
        for x in 0..TILE_WIDTH {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let edge = (fx.min(TILE_WIDTH as f32 - fx) / 40.0).min(1.0);
            let value = (pattern(kind, fx, fy) * (0.55 + 0.45 * edge)).clamp(0.0, 1.0);
            let byte = (value * 255.0).round() as u8;
            data.extend_from_slice(&[byte, byte, byte, 255]);
        }
    }
    Image::new(
        Extent3d { width: TILE_WIDTH, height: TILE_HEIGHT, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// The drawn counter: a light ring on a dark disc, transparent outside,
/// for a counter of a kind nobody has drawn.
fn paint_badge() -> Image {
    let mut data = Vec::with_capacity((BADGE * BADGE * 4) as usize);
    let centre = BADGE as f32 / 2.0;
    for y in 0..BADGE {
        for x in 0..BADGE {
            let d = ((x as f32 + 0.5 - centre).powi(2) + (y as f32 + 0.5 - centre).powi(2)).sqrt();
            let pixel = if d > centre - 1.0 {
                [0, 0, 0, 0]
            } else if d > centre - 8.0 {
                [228, 232, 242, 255]
            } else {
                [40, 44, 56, 255]
            };
            data.extend_from_slice(&pixel);
        }
    }
    Image::new(Extent3d { width: BADGE, height: BADGE, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

/// The drawn wing: a flat grey plate with a lit top edge and a dark
/// channel along its foot, washed in the state's colour — the side's
/// while it is that side's turn, grey otherwise. Plain on purpose: the
/// look is the committed picture, and this is what a slow machine gets.
fn paint_bar() -> Image {
    let mut data = Vec::with_capacity((BAR_WIDTH * BAR_HEIGHT * 4) as usize);
    for y in 0..BAR_HEIGHT {
        for x in 0..BAR_WIDTH {
            let pixel = if !(4..92).contains(&y) || x < 10 {
                [0, 0, 0, 0]
            } else if y < 7 {
                [236, 238, 244, 255]
            } else if (68..86).contains(&y) && x >= 44 {
                if y == 77 { [236, 238, 244, 255] } else { [36, 38, 46, 255] }
            } else {
                let shade = 150 - (y as i32 - 7).clamp(0, 60) as u8;
                [shade, shade, shade.saturating_add(8), 255]
            };
            data.extend_from_slice(&pixel);
        }
    }
    Image::new(Extent3d { width: BAR_WIDTH, height: BAR_HEIGHT, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

/// The drawn ring round the avatar: light grey, transparent inside and
/// out, washed in the state's colour like the bar.
fn paint_frame() -> Image {
    let mut data = Vec::with_capacity((FRAME * FRAME * 4) as usize);
    let centre = FRAME as f32 / 2.0;
    for y in 0..FRAME {
        for x in 0..FRAME {
            let d = ((x as f32 + 0.5 - centre).powi(2) + (y as f32 + 0.5 - centre).powi(2)).sqrt();
            let pixel = if d > centre - 2.0 || d < centre - 22.0 {
                [0, 0, 0, 0]
            } else if d < centre - 18.0 {
                [236, 238, 244, 255]
            } else {
                [150, 152, 162, 255]
            };
            data.extend_from_slice(&pixel);
        }
    }
    Image::new(Extent3d { width: FRAME, height: FRAME, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
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
        let plates: Vec<Image> = [Building::Archives, Building::RnD, Building::Hq, Building::Remote].into_iter().map(|b| paint_plate(&theme, None, b)).collect();
        for plate in &plates {
            assert_eq!(plate.size(), UVec2::new(PLATE_WIDTH, PLATE_HEIGHT));
        }
        for (i, a) in plates.iter().enumerate() {
            for b in plates.iter().skip(i + 1) {
                assert_ne!(a.data, b.data, "two servers drew the same building");
            }
        }
        for key in KEYS {
            // Every base names a key in the table; HUD glyphs are the
            // only keys allowed to have nothing at all.
            if let Some(base) = key.base {
                assert!(KEYS.iter().any(|k| k.key == base), "{}'s base {base} is not a key", key.key);
            }
            let optional = key.key.starts_with("hud.");
            let reaches_a_drawing = key.paint.is_some() || key.base.and_then(|b| KEYS.iter().find(|k| k.key == b)).is_some_and(|b| b.paint.is_some());
            assert!(optional || reaches_a_drawing, "{} has no drawn default to fall back to", key.key);
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

    /// The Corp's style is looked for between the skin's and the generic
    /// folder, and a Runner's faction has no style to look for.
    #[test]
    fn the_corp_style_sits_between_the_skin_and_the_generic_art() {
        assert_eq!(layers(Some("brass"), Some(Faction::Jinteki)), vec!["skins/brass/board", "board/corp/jinteki", "board"]);
        assert_eq!(layers(None, Some(Faction::WeylandConsortium)), vec!["board/corp/weyland-consortium", "board"]);
        assert_eq!(layers(None, Some(Faction::Shaper)), vec!["board"], "a Runner faction styles nothing");
        assert_eq!(layers(None, None), vec!["board"]);
        for faction in CORP_FACTIONS {
            assert!(faction_slug(faction).is_some(), "{faction:?} has a folder");
        }
    }

    /// The drawn plates are lit in the Corp's faction colour, so a style
    /// is visible before anybody has drawn one — and a neutral Corp keeps
    /// the Corp's own colour rather than a grey.
    #[test]
    fn a_drawn_plate_is_lit_in_the_corps_faction_colour() {
        let theme = Theme::default();
        let jinteki = paint_plate(&theme, Some(Faction::Jinteki), Building::Hq);
        let nbn = paint_plate(&theme, Some(Faction::Nbn), Building::Hq);
        let plain = paint_plate(&theme, None, Building::Hq);
        assert_ne!(jinteki.data, nbn.data);
        assert_ne!(jinteki.data, plain.data);
        assert_eq!(paint_plate(&theme, Some(Faction::NeutralCorp), Building::Hq).data, plain.data);
    }

    /// Basic graphics reads no file: every picture is a drawn one, even
    /// the bundled glyphs.
    #[test]
    fn basic_graphics_loads_only_the_drawn_tier() {
        let mut images = Assets::<Image>::default();
        let basic = BoardArt::load(Style { basic: true, faction: Some(Faction::Jinteki), ..Style::default() }, &Theme::default(), &mut images);
        for key in keys() {
            if let Some(picture) = basic.pictures.get(key) {
                assert!(picture.drawn, "{key} was read from a file under basic graphics");
            }
        }
        assert!(basic.get("hud.credits").is_none(), "a HUD glyph has no drawn tier");
    }

    /// The guide names every Corp style folder.
    #[test]
    fn the_guide_lists_every_corp_style() {
        let guide = include_str!("../assets/board/README.md");
        for faction in CORP_FACTIONS {
            let folder = format!("`corp/{}/`", faction_slug(faction).unwrap());
            assert!(guide.contains(&folder), "assets/board/README.md does not list {folder}");
        }
    }

    /// Each tile pattern is a different picture, at the tile's shape.
    #[test]
    fn every_tile_kind_has_its_own_drawn_pattern() {
        let all = [Pattern::Hatch, Pattern::BackHatch, Pattern::Scanlines, Pattern::Bricks, Pattern::Gate, Pattern::Rings, Pattern::Rivets, Pattern::Coins, Pattern::Chevrons, Pattern::Diamonds];
        let tiles: Vec<Image> = all.iter().map(|p| paint_tile(*p)).collect();
        for (i, a) in tiles.iter().enumerate() {
            assert_eq!(a.size(), UVec2::new(TILE_WIDTH, TILE_HEIGHT));
            for b in tiles.iter().skip(i + 1) {
                assert_ne!(a.data, b.data, "two tile kinds drew the same pattern");
            }
        }
        use netrunner_core::dsl::{CardType, IceType};
        assert_eq!(ice_key(false, Some(IceType::Sentry)), "ice.unrezzed", "a face-down ICE never shows its type");
        assert_eq!(ice_key(true, Some(IceType::CodeGate)), "ice.rezzed.code-gate");
        assert_eq!(root_key(false, Some(&CardType::Agenda)), "root.agenda");
        assert_eq!(root_key(false, Some(&CardType::Asset)), "root.unrezzed");
        assert_eq!(root_key(true, Some(&CardType::Upgrade)), "root.rezzed.upgrade");
    }

    /// With no files anywhere, a state finds its base's drawn picture.
    #[test]
    fn a_state_with_no_picture_falls_back_to_its_base() {
        let mut images = Assets::<Image>::default();
        let art = BoardArt::default();
        assert!(art.get("server.hq.run").is_none());
        let loaded = BoardArt::load(Style { skin: Some("no-such-skin".to_string()), ..Style::default() }, &Theme::default(), &mut images);
        let base = loaded.get("server.hq").expect("drawn").image.clone();
        assert_eq!(loaded.get("server.hq.run").expect("falls back").image, base);
        assert_eq!(loaded.get(server_key(ServerId::Remote(3), true)).unwrap().image, loaded.get("server.remote").unwrap().image);
        assert_eq!(loaded.get("server.remote").unwrap().size, Vec2::new(PLATE_WIDTH as f32, PLATE_HEIGHT as f32));
        // A drawn picture is washed in its state's colour; a file is not.
        let drawn = loaded.get("ice.unrezzed").unwrap();
        assert!(drawn.drawn);
        assert_eq!(drawn.tint(Color::BLACK), Color::BLACK);
        // The bundled glyphs load as files, and every counter reaches a
        // picture whether or not they are there.
        for kind in ["counter.virus", "counter.advancement", "counter.power", "counter.credit", "counter"] {
            assert!(loaded.get(kind).is_some(), "{kind}");
        }
        let virus = loaded.get("counter.virus").unwrap();
        if crate::assets::resolve("board/counter.virus.png").is_some() {
            assert!(!virus.drawn && virus.tint(Color::BLACK) == Color::WHITE, "a file is drawn as painted");
        }
    }
}
