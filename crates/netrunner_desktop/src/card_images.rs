//! Card pictures as GPU images: the cached fronts, decoded off the main
//! thread, and the two backs — a file if there is one, the official
//! back fetched into the cache on the images opt-in, else painted.
//!
//! **A back is one handle for the run, and the picture behind it may
//! improve.** The board asks `CardImages::back` for a side and draws
//! whatever the handle holds; when the official back lands — a file
//! read at start, or a fetch finishing mid-game — the image is put in
//! place of the painted one under the same handle (`Assets::insert`),
//! so every back already on screen changes without a redraw and no
//! screen needs to know the tier moved. The fetch follows the icon
//! font's rules (`icon_font.rs`): a cached file is loaded whatever the
//! opt-in says, a missing one is fetched only when `download_images` is
//! on, the disk is looked at once a second while it is missing, and a
//! failed fetch is a notice not retried this run.
//!
//! **Bytes, not asset paths.** The cache directory is outside Bevy's
//! asset root, and the asset server refuses an absolute path by default
//! (`UnapprovedPathMode::Forbid`). Reading the file and decoding it with
//! `Image::from_buffer` needs no asset source and no plugin ordering, and
//! the decode — a 750-pixel WebP, about 57 ms, or a copy kept from an
//! earlier run, about one — runs on the compute pool, four at a time,
//! so a frame never waits for it. A face that wants its
//! picture carries [`WantsImage`]; when the handle exists the face is
//! swapped for an `ImageNode` in place, so a download finishing while
//! the browser is open fills the grid without leaving the screen.
//!
//! Every system here runs only where `Assets<Image>` exists: the
//! headless tests build the client without an asset plugin, and a face
//! there stays text.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::{AsyncComputeTaskPool, Task};
use tokio::sync::oneshot;

use netrunner_card_sync::ImageStatus;
use netrunner_core::card::CardId;
use netrunner_core::rules::Side;

use crate::assets;
use crate::card_back;
use crate::core::{ClientCore, Notices, TokioRuntime};
use crate::theme::Theme;
use crate::widgets::card_face::FaceSize;

pub struct CardImagesPlugin;

impl Plugin for CardImagesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CardImages>()
            .add_systems(Update, (track_scale, load_backs, request_wanted, poll_decoded).chain().run_if(resource_exists::<Assets<Image>>));
    }
}

/// How many fronts are decoded at once. The rest wait in
/// [`CardImages::queue`], so a card the person just asked to look at is
/// the next one started rather than the last of a browser's 271.
const IN_FLIGHT: usize = 4;

/// A text face that would rather be its picture. Removed when the
/// picture is put in its place.
#[derive(Component, Debug, Clone, Copy)]
pub struct WantsImage {
    pub code: CardId,
    pub size: FaceSize,
}

/// A front at one width: the card and its [`rung`].
type FaceKey = (CardId, u32);

#[derive(Resource)]
pub struct CardImages {
    faces: HashMap<FaceKey, Handle<Image>>,
    pending: HashMap<FaceKey, Task<Option<Image>>>,
    /// Fronts asked for and not yet started, oldest first — except a
    /// `Large` face, which goes to the front: it is the one card a person
    /// is looking at.
    queue: VecDeque<(FaceKey, PathBuf)>,
    queued: HashSet<FaceKey>,
    /// The primary window's scale factor, so a face's logical width can
    /// be turned into the pixels its picture will cover.
    scale: f32,
    backs: Option<[Handle<Image>; 2]>,
    /// Which backs still hold the painted tier and would take the
    /// official one: false once a file — dropped in, bundled or
    /// fetched — has been put under the handle.
    drawn: [bool; 2],
    /// The fetch of the official backs still drawn, if one is under way.
    fetch: Option<oneshot::Receiver<Vec<(Side, Result<Image, String>)>>>,
    /// A fetch failed, or could not be started, this run.
    gave_up: bool,
    /// When the cache is next looked at for a back that was not there.
    next_look: f64,
    /// Set by the downloader when a file may have appeared, so faces
    /// whose picture was missing ask the store again; cleared once they
    /// have.
    pub recheck: bool,
}

impl Default for CardImages {
    fn default() -> Self {
        CardImages {
            faces: HashMap::new(),
            pending: HashMap::new(),
            queue: VecDeque::new(),
            queued: HashSet::new(),
            scale: 1.0,
            backs: None,
            drawn: [false; 2],
            fetch: None,
            gave_up: false,
            next_look: 0.0,
            recheck: false,
        }
    }
}

impl CardImages {
    /// The front for `code` decoded for a face of `size`, if it has been.
    pub fn face(&self, code: CardId, size: FaceSize) -> Option<Handle<Image>> {
        self.faces.get(&self.key(code, size)).cloned()
    }

    /// The sharpest copy of `code` already decoded, at whatever width it
    /// was decoded for — a stand-in for a face whose own width is still
    /// decoding. A card in the decision pop-up is nearly always one the
    /// board already drew (a hand card, a card in Archives), so drawing
    /// that copy stretched for the moment beats flashing the text face
    /// first. `None` when no copy of the card has been decoded at all.
    pub fn nearest_face(&self, code: CardId) -> Option<Handle<Image>> {
        self.faces.iter().filter(|((c, _), _)| *c == code).max_by_key(|((_, width), _)| *width).map(|(_, handle)| handle.clone())
    }

    fn key(&self, code: CardId, size: FaceSize) -> FaceKey {
        (code, rung(size.width() * self.scale))
    }

    pub fn back(&self, side: Side) -> Option<Handle<Image>> {
        self.backs.as_ref().map(|backs| backs[side as usize].clone())
    }

    /// Puts `image` under the side's existing back handle, so every
    /// back on screen shows it from the next frame.
    fn set_back(&mut self, side: Side, image: Image, assets: &mut Assets<Image>) {
        // A strong handle held here cannot have been dropped, so the
        // insert cannot miss; a miss would only leave the painted back.
        if let Some(backs) = &self.backs
            && assets.insert(&backs[side as usize], image).is_ok()
        {
            self.drawn[side as usize] = false;
        }
    }

    /// Queues the file at `path` for a face of `size`, unless that width
    /// is known, under way or already queued.
    pub fn request(&mut self, code: CardId, size: FaceSize, path: PathBuf) {
        let key = self.key(code, size);
        if self.faces.contains_key(&key) || self.pending.contains_key(&key) || !self.queued.insert(key) {
            return;
        }
        if size == FaceSize::Large {
            self.queue.push_front((key, path));
        } else {
            self.queue.push_back((key, path));
        }
        self.start_queued();
    }

    /// Starts queued decodes until [`IN_FLIGHT`] are running.
    fn start_queued(&mut self) {
        while self.pending.len() < IN_FLIGHT
            && let Some((key, path)) = self.queue.pop_front()
        {
            self.queued.remove(&key);
            let task = AsyncComputeTaskPool::get().spawn(async move { load_front(&path, key) });
            self.pending.insert(key, task);
        }
    }
}

/// `bytes` as an image, `None` if the file is not one. `is_srgb` because
/// card scans are sRGB; a linear sampler because a picture is drawn near
/// its own width (a front, at its [`rung`]) but seldom exactly at it, and
/// never wants hard pixel edges.
pub(crate) fn decode(bytes: &[u8], extension: &str) -> Option<Image> {
    Image::from_buffer(bytes, ImageType::Extension(extension), CompressedImageFormats::NONE, true, ImageSampler::linear(), RenderAssetUsages::default())
        .ok()
        .map(eight_bit_srgb)
}

/// A sixteen-bit PNG — the official backs are one — decodes to
/// `Rgba16Unorm`, which has no sRGB variant, so the renderer would read
/// its sRGB-encoded values as linear and draw the back washed out.
/// The top byte of each channel is the eight-bit value, and eight bits
/// is what a card on screen can show.
fn eight_bit_srgb(image: Image) -> Image {
    if image.texture_descriptor.format != TextureFormat::Rgba16Unorm {
        return image;
    }
    let Some(data) = image.data.as_ref() else { return image };
    let size = image.texture_descriptor.size;
    let narrowed: Vec<u8> = data.as_chunks::<2>().0.iter().map(|pair| pair[1]).collect();
    let mut eight = Image::new(
        Extent3d { width: size.width, height: size.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        narrowed,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    eight.sampler = ImageSampler::linear();
    eight
}

/// The front at `path` for `key`'s width: the copy kept from an earlier
/// run if it is newer than the scan, else the scan decoded and resampled,
/// and that copy kept for next time.
///
/// **Why keep the copy.** Decoding a 750-pixel WebP costs about 57 ms
/// and resampling it 15 ms, so a browser of 271 cards filled in over
/// seconds where the 300-pixel JPEGs it replaced had been instant. The
/// copy is raw pixels behind a small header — no codec to run — and sits
/// in `sized/` beside the scans, in the cache, where anything can be
/// deleted and made again. It is named for the scan's format as well as
/// the width, so a `.webp` that replaces a `.jpg` is never shown the
/// smaller scan's copy. A copy that cannot be written costs only the
/// decode next time.
fn load_front(path: &Path, (code, width): FaceKey) -> Option<Image> {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
    let rung_name = if width == WHOLE_SCAN { "whole".to_owned() } else { width.to_string() };
    let copy = path.parent().map(|dir| dir.join("sized").join(format!("{:05}-{extension}-{rung_name}.rgba", code.0)));
    let modified = |path: &Path| std::fs::metadata(path).and_then(|meta| meta.modified()).ok();
    if let Some(copy) = &copy
        && modified(copy) >= modified(path)
        && let Some(image) = std::fs::read(copy).ok().and_then(|bytes| from_copy(&bytes))
    {
        return Some(image);
    }
    let image = fitted(decode(&std::fs::read(path).ok()?, extension)?, width);
    if let (Some(copy), Some(bytes)) = (&copy, to_copy(&image)) {
        let _ = write_copy(copy, &bytes);
    }
    Some(image)
}

/// The header a kept copy starts with, then its width and height.
const COPY_MAGIC: &[u8; 8] = b"NRFACE1\0";

fn to_copy(image: &Image) -> Option<Vec<u8>> {
    if image.texture_descriptor.format != TextureFormat::Rgba8UnormSrgb {
        return None;
    }
    let size = image.texture_descriptor.size;
    let data = image.data.as_ref()?;
    let mut bytes = Vec::with_capacity(16 + data.len());
    bytes.extend_from_slice(COPY_MAGIC);
    bytes.extend_from_slice(&size.width.to_le_bytes());
    bytes.extend_from_slice(&size.height.to_le_bytes());
    bytes.extend_from_slice(data);
    Some(bytes)
}

/// A kept copy as an image, `None` if it is not one or is cut short.
fn from_copy(bytes: &[u8]) -> Option<Image> {
    let rest = bytes.strip_prefix(COPY_MAGIC)?;
    let (width, rest) = rest.split_first_chunk::<4>()?;
    let (height, data) = rest.split_first_chunk::<4>()?;
    let (width, height) = (u32::from_le_bytes(*width), u32::from_le_bytes(*height));
    if data.len() != width as usize * height as usize * 4 {
        return None;
    }
    let mut image = Image::new(
        Extent3d { width, height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data.to_vec(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    Some(image)
}

/// A temp file and a rename, so a copy cut off halfway is never read as
/// whole.
fn write_copy(copy: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = copy.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = copy.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, copy)
}

/// The widths a front is decoded at, each 1.25× the last. A face takes
/// the first at least as wide as the pixels it covers, so its picture is
/// never shrunk by the sampler more than 1.25×; wider than the last, it
/// takes the whole scan ([`WHOLE_SCAN`]).
///
/// **Why not the scan at every size.** A 750-pixel scan drawn at 280
/// physical pixels in the browser's grid, and under 150 on the board, is
/// read by a linear sampler four texels at a time however far it is
/// shrunk, and the printed text broke into jagged strokes — where the
/// old 300-pixel scan, at under 2×, had only softened. A mip chain is
/// the textbook answer and was the first cut, but a scan with its chain
/// holds 4 MB of GPU memory — over a gigabyte for a browser of every
/// card. A copy resampled once, on the compute pool, to the width it is
/// drawn at is as sharp as the scan allows and costs what is on screen:
/// under 0.7 MB a grid cell at a 2× display scale.
const RUNGS: [u32; 11] = [72, 90, 113, 141, 176, 220, 275, 344, 430, 537, 671];

/// The rung that stands for "the scan as NetrunnerDB served it".
const WHOLE_SCAN: u32 = u32::MAX;

/// The width to decode a front at for a face covering `pixels`.
fn rung(pixels: f32) -> u32 {
    RUNGS.into_iter().find(|&width| width as f32 >= pixels).unwrap_or(WHOLE_SCAN)
}

/// `image` resampled to `width` wide, keeping the card's shape, unless it
/// is already no wider. Catmull-Rom, from `image`.
///
/// The main-world copy stays: a face marked render-world only drew every
/// frame of the card browser black, though nothing reads its pixels
/// back. At the sizes a face is resampled to the copy is small.
pub(crate) fn fitted(mut image: Image, width: u32) -> Image {

    let size = image.texture_descriptor.size;
    if size.width <= width || image.texture_descriptor.format != TextureFormat::Rgba8UnormSrgb {
        return image;
    }
    let Some(scan) = image.data.take().and_then(|data| image::RgbaImage::from_raw(size.width, size.height, data)) else { return image };
    let height = ((width as f32 * size.height as f32 / size.width as f32).round() as u32).max(1);
    // `DynamicImage`'s method, not `imageops::resize`: that one is
    // generic, so it is compiled into this crate, unoptimised in the dev
    // profile — 446 ms a scan. This is compiled in `image`, at its own
    // optimisation level.
    let smaller = image::DynamicImage::ImageRgba8(scan).resize_exact(width, height, image::imageops::FilterType::CatmullRom).into_rgba8();
    let mut fitted = Image::new(
        Extent3d { width, height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        smaller.into_raw(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    fitted.sampler = image.sampler;
    fitted
}

/// Keeps [`CardImages::scale`] the primary window's, so a face asks for
/// the pixels it will actually cover. A window that moves to a screen of
/// another scale gets its sharper or smaller pictures as faces are next
/// spawned.
fn track_scale(mut images: ResMut<CardImages>, windows: Query<&Window, With<bevy::window::PrimaryWindow>>) {
    if let Ok(window) = windows.single()
        && images.scale != window.scale_factor()
    {
        images.scale = window.scale_factor();
    }
}

/// How often the cache is checked for a back that was not there.
const LOOK_EVERY_SECS: f64 = 1.0;

/// The back's file under the override and bundled tiers, and its name
/// in the cache: one name, wherever it sits.
fn back_file(side: Side) -> String {
    format!("cards/back-{}.png", match side { Side::Corp => "corp", Side::Runner => "runner" })
}

/// Both backs, once, from the best tier on disk — a drop-in or bundled
/// PNG, else the official back in the cache, else painted — and then
/// the official back put under the handle of any still painted, read
/// from the cache when it appears or fetched when the player has
/// opted in.
fn load_backs(
    theme: Res<Theme>,
    core: Res<ClientCore>,
    runtime: Option<Res<TokioRuntime>>,
    mut images: ResMut<CardImages>,
    mut assets: ResMut<Assets<Image>>,
    time: Res<Time>,
    mut notices: ResMut<Notices>,
) {
    if images.backs.is_none() {
        let mut drawn = [false; 2];
        let back = |side: Side, drawn: &mut bool| {
            let from_file = assets::read(&back_file(side)).and_then(|bytes| decode(&bytes, "png"));
            let from_cache = || core.images.card_back(side).and_then(|path| std::fs::read(path).ok()).and_then(|bytes| decode(&bytes, "png"));
            from_file.or_else(from_cache).unwrap_or_else(|| {
                *drawn = true;
                card_back::paint(side, &theme)
            })
        };
        let corp = back(Side::Corp, &mut drawn[0]);
        let runner = back(Side::Runner, &mut drawn[1]);
        images.backs = Some([assets.add(corp), assets.add(runner)]);
        images.drawn = drawn;
    }
    if !images.drawn.contains(&true) {
        return;
    }
    if let Some(receiver) = &mut images.fetch {
        match receiver.try_recv() {
            Ok(results) => {
                images.fetch = None;
                for (side, result) in results {
                    match result {
                        Ok(image) => {
                            info!("official {} card back fetched", side_name(side));
                            images.set_back(side, image, &mut assets);
                        }
                        Err(error) => {
                            images.gave_up = true;
                            notices.push(format!("{} card back not fetched: {error}", side_name(side)));
                        }
                    }
                }
                return;
            }
            Err(oneshot::error::TryRecvError::Empty) => return,
            Err(oneshot::error::TryRecvError::Closed) => {
                images.fetch = None;
                images.gave_up = true;
                return;
            }
        }
    }
    let now = time.elapsed_secs_f64();
    if now < images.next_look {
        return;
    }
    images.next_look = now + LOOK_EVERY_SECS;
    for side in [Side::Corp, Side::Runner] {
        if !images.drawn[side as usize] {
            continue;
        }
        if let Some(image) = core.images.card_back(side).and_then(|path| std::fs::read(path).ok()).and_then(|bytes| decode(&bytes, "png")) {
            info!("official {} card back read from the cache", side_name(side));
            images.set_back(side, image, &mut assets);
        }
    }
    let wanted: Vec<Side> = [Side::Corp, Side::Runner].into_iter().filter(|side| images.drawn[*side as usize]).collect();
    if wanted.is_empty() || images.gave_up || !core.settings.desktop.download_images {
        return;
    }
    let Some(runtime) = runtime else {
        images.gave_up = true;
        return;
    };
    let (sender, receiver) = oneshot::channel();
    let store = core.images.clone();
    runtime.0.spawn(async move {
        let mut results = Vec::with_capacity(wanted.len());
        for side in wanted {
            // Decoded here, off the main thread, so the frame the fetch
            // lands on pays for a handle swap and not a PNG decode.
            let result = match store.download_card_back(side).await {
                Ok(path) => std::fs::read(&path)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| decode(&bytes, "png").ok_or_else(|| format!("{} is not an image", path.display()))),
                Err(error) => Err(error.to_string()),
            };
            results.push((side, result));
        }
        let _ = sender.send(results);
    });
    images.fetch = Some(receiver);
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Corp => "Corp",
        Side::Runner => "Runner",
    }
}

/// Asks the store for the picture of every face that just appeared —
/// and of every face still waiting, when the downloader says something
/// may have landed.
fn request_wanted(
    core: Res<ClientCore>,
    mut images: ResMut<CardImages>,
    added: Query<&WantsImage, Added<WantsImage>>,
    all: Query<&WantsImage>,
) {
    let recheck = std::mem::take(&mut images.recheck);
    let wanted: Vec<WantsImage> = if recheck { all.iter().copied().collect() } else { added.iter().copied().collect() };
    for wants in wanted {
        if let ImageStatus::Cached(path) = core.images.status(wants.code) {
            images.request(wants.code, wants.size, path);
        }
    }
}

/// Collects finished decodes into `Assets<Image>` and puts each picture
/// in place of the text face that wanted it.
fn poll_decoded(
    mut commands: Commands,
    mut images: ResMut<CardImages>,
    mut assets: ResMut<Assets<Image>>,
    wanted: Query<(Entity, &WantsImage, Option<&Node>)>,
) {
    let finished: Vec<(FaceKey, Option<Image>)> =
        images.pending.iter_mut().filter_map(|(key, task)| check_ready(task).map(|image| (*key, image))).collect();
    for (key, image) in finished {
        images.pending.remove(&key);
        if let Some(image) = image {
            let handle = assets.add(image);
            images.faces.insert(key, handle);
        }
    }
    images.start_queued();
    for (entity, wants, old) in &wanted {
        if let Some(handle) = images.face(wants.code, wants.size) {
            let node = match old {
                Some(old) => in_place_of(old, picture_node(wants.size)),
                None => picture_node(wants.size),
            };
            commands.entity(entity).remove::<WantsImage>().despawn_children().insert(picture(handle, wants.size)).insert(node);
        }
    }
}

/// The node a picture replaces a text face with: the same footprint, the
/// scan stretched to it, and no border or padding of its own.
pub fn picture(handle: Handle<Image>, size: FaceSize) -> impl Bundle {
    (
        ImageNode { image_mode: NodeImageMode::Stretch, ..ImageNode::new(handle) },
        picture_node(size),
        BackgroundColor(Color::NONE),
        BorderColor::all(Color::NONE),
    )
}

fn picture_node(size: FaceSize) -> Node {
    Node { width: px(size.width()), height: px(size.height()), flex_shrink: 0.0, border_radius: BorderRadius::all(px(8)), ..default() }
}

/// The picture's node, keeping where the text face it replaces was put.
///
/// A face's size and border are its own, but its *placement* belongs to
/// whoever laid it out after spawning it: a hand's or rig's overlap pull
/// (`margin.left`), an ICE tile's gap (`margin.top`), a lifted copy's
/// absolute position. Swapping the whole `Node` reset those to zero, so
/// the card drawn last — the one whose scan was still decoding — slid a
/// full width right, past the hand's peek window, and was cut in half.
fn in_place_of(old: &Node, new: Node) -> Node {
    Node { margin: old.margin, position_type: old.position_type, left: old.left, right: old.right, top: old.top, bottom: old.bottom, ..new }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_keeps_the_placement_of_the_face_it_replaces() {
        let size = FaceSize::Board(200);
        let face = Node {
            width: px(200),
            height: px(280),
            border: UiRect::all(px(2)),
            margin: UiRect::left(px(-40)),
            position_type: PositionType::Absolute,
            left: px(15),
            top: px(30),
            ..default()
        };
        let node = in_place_of(&face, picture_node(size));
        assert_eq!(node.margin.left, px(-40));
        assert_eq!(node.position_type, PositionType::Absolute);
        assert_eq!((node.left, node.top), (px(15), px(30)));
        assert_eq!((node.width, node.height), (px(size.width()), px(size.height())));
        assert_eq!(node.border, UiRect::default());
    }

    /// A face gets the first rung at least as wide as it is drawn, so
    /// the sampler never shrinks it more than 1.25×; a face wider than
    /// every rung gets the scan whole.
    #[test]
    fn a_face_is_decoded_at_the_rung_that_covers_it() {
        assert_eq!(rung(72.0), 72);
        assert_eq!(rung(73.0), 90);
        assert_eq!(rung(280.0), 344, "a grid cell at a 2x display scale");
        assert_eq!(rung(672.0), WHOLE_SCAN);
        for pair in RUNGS.windows(2) {
            assert!(pair[1] as f32 <= pair[0] as f32 * 1.26, "{pair:?}");
        }
        let images = CardImages { scale: 2.0, ..CardImages::default() };
        assert_eq!(images.key(CardId(30001), FaceSize::Thumb), (CardId(30001), 344));
        assert_eq!(images.key(CardId(30001), FaceSize::Large), (CardId(30001), WHOLE_SCAN));
    }

    /// A scan wider than its rung is resampled to it, keeping the card's
    /// shape; one no wider is left as served.
    #[test]
    fn a_scan_is_resampled_down_to_its_rung_and_never_up() {
        let size = Extent3d { width: 750, height: 1050, depth_or_array_layers: 1 };
        let scan = || Image::new(size, TextureDimension::D2, vec![128; 750 * 1050 * 4], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
        let small = fitted(scan(), 344);
        assert_eq!(small.texture_descriptor.size, Extent3d { width: 344, height: 482, depth_or_array_layers: 1 });
        assert_eq!(small.data.as_ref().map(Vec::len), Some(344 * 482 * 4));
        let whole = fitted(scan(), WHOLE_SCAN);
        assert_eq!(whole.texture_descriptor.size, size);
    }

    /// A copy is kept on the first load and read back on the next, the
    /// same pixels; a scan newer than its copy is decoded again.
    #[test]
    fn a_resampled_front_is_kept_and_read_back() {
        let dir = std::env::temp_dir().join(format!("netrunner_sized_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let scan = dir.join("30001.png");
        let pixels: Vec<u8> = (0..40 * 56).flat_map(|i| [(i % 251) as u8, 40, 200, 255]).collect();
        image::RgbaImage::from_raw(40, 56, pixels).unwrap().save(&scan).unwrap();
        let first = load_front(&scan, (CardId(30001), 20)).unwrap();
        assert_eq!(first.texture_descriptor.size, Extent3d { width: 20, height: 28, depth_or_array_layers: 1 });
        let copy = dir.join("sized").join("30001-png-20.rgba");
        assert!(copy.is_file());
        std::fs::write(&copy, to_copy(&Image::new(Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }, TextureDimension::D2, vec![9, 9, 9, 9], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())).unwrap()).unwrap();
        let read_back = load_front(&scan, (CardId(30001), 20)).unwrap();
        assert_eq!(read_back.data.as_deref(), Some(&[9, 9, 9, 9][..]), "the kept copy is what is read");
        assert!(from_copy(b"NRFACE1\0\x02\0\0\0\x02\0\0\0short").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_sixteen_bit_image_is_narrowed_to_eight_bit_srgb_and_an_eight_bit_one_is_left_alone() {
        let size = Extent3d { width: 2, height: 1, depth_or_array_layers: 1 };
        // Two pixels, little-endian u16 per channel: (0x1234, 0xffff, 0x0000, 0x8000) and (0xabcd, 0, 0, 0xffff).
        let data = vec![0x34, 0x12, 0xff, 0xff, 0x00, 0x00, 0x00, 0x80, 0xcd, 0xab, 0, 0, 0, 0, 0xff, 0xff];
        let wide = Image::new(size, TextureDimension::D2, data, TextureFormat::Rgba16Unorm, RenderAssetUsages::default());
        let narrow = eight_bit_srgb(wide);
        assert_eq!(narrow.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
        assert_eq!(narrow.texture_descriptor.size, size);
        assert_eq!(narrow.data.as_deref(), Some(&[0x12, 0xff, 0x00, 0x80, 0xab, 0, 0, 0xff][..]));

        let eight = Image::new(size, TextureDimension::D2, vec![1, 2, 3, 4, 5, 6, 7, 8], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
        let same = eight_bit_srgb(eight);
        assert_eq!(same.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
        assert_eq!(same.data.as_deref(), Some(&[1, 2, 3, 4, 5, 6, 7, 8][..]));
    }
}
