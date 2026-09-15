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
//! the decode — a JPEG at card size, a few milliseconds — runs on the
//! compute pool so a frame never waits for it. A face that wants its
//! picture carries [`WantsImage`]; when the handle exists the face is
//! swapped for an `ImageNode` in place, so a download finishing while
//! the browser is open fills the grid without leaving the screen.
//!
//! Every system here runs only where `Assets<Image>` exists: the
//! headless tests build the client without an asset plugin, and a face
//! there stays text.

use std::collections::HashMap;
use std::path::PathBuf;

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
            .add_systems(Update, (load_backs, request_wanted, poll_decoded).chain().run_if(resource_exists::<Assets<Image>>));
    }
}

/// A text face that would rather be its picture. Removed when the
/// picture is put in its place.
#[derive(Component, Debug, Clone, Copy)]
pub struct WantsImage {
    pub code: CardId,
    pub size: FaceSize,
}

#[derive(Resource, Default)]
pub struct CardImages {
    faces: HashMap<CardId, Handle<Image>>,
    pending: HashMap<CardId, Task<Option<Image>>>,
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

impl CardImages {
    /// The decoded front for `code`, if it has been.
    pub fn face(&self, code: CardId) -> Option<Handle<Image>> {
        self.faces.get(&code).cloned()
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

    /// Starts decoding the file at `path` for `code`, unless it is known
    /// or under way.
    pub fn request(&mut self, code: CardId, path: PathBuf) {
        if self.faces.contains_key(&code) || self.pending.contains_key(&code) {
            return;
        }
        let task = AsyncComputeTaskPool::get().spawn(async move { std::fs::read(&path).ok().and_then(|bytes| decode(&bytes, "jpg")) });
        self.pending.insert(code, task);
    }
}

/// `bytes` as an image, `None` if the file is not one. `is_srgb` because
/// card scans are sRGB; a linear sampler because a face is drawn smaller
/// than the scan.
fn decode(bytes: &[u8], extension: &str) -> Option<Image> {
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
    let wanted: Vec<CardId> = if recheck { all.iter().map(|w| w.code).collect() } else { added.iter().map(|w| w.code).collect() };
    for code in wanted {
        if let ImageStatus::Cached(path) = core.images.status(code) {
            images.request(code, path);
        }
    }
}

/// Collects finished decodes into `Assets<Image>` and puts each picture
/// in place of the text face that wanted it.
fn poll_decoded(
    mut commands: Commands,
    mut images: ResMut<CardImages>,
    mut assets: ResMut<Assets<Image>>,
    wanted: Query<(Entity, &WantsImage)>,
) {
    let finished: Vec<(CardId, Option<Image>)> =
        images.pending.iter_mut().filter_map(|(code, task)| check_ready(task).map(|image| (*code, image))).collect();
    for (code, image) in finished {
        images.pending.remove(&code);
        if let Some(image) = image {
            let handle = assets.add(image);
            images.faces.insert(code, handle);
        }
    }
    for (entity, wants) in &wanted {
        if let Some(handle) = images.faces.get(&wants.code) {
            commands.entity(entity).remove::<WantsImage>().despawn_children().insert(picture(handle.clone(), wants.size));
        }
    }
}

/// The node a picture replaces a text face with: the same footprint, the
/// scan stretched to it, and no border or padding of its own.
pub fn picture(handle: Handle<Image>, size: FaceSize) -> impl Bundle {
    (
        ImageNode { image_mode: NodeImageMode::Stretch, ..ImageNode::new(handle) },
        Node {
            width: px(size.width()),
            height: px(size.height()),
            flex_shrink: 0.0,
            border_radius: BorderRadius::all(px(8)),
            ..default()
        },
        BackgroundColor(Color::NONE),
        BorderColor::all(Color::NONE),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
