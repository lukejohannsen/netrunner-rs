//! Card pictures as GPU images: the cached fronts, decoded off the main
//! thread, and the two backs, painted or read from a file.
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
use bevy::tasks::{AsyncComputeTaskPool, Task};

use netrunner_card_sync::ImageStatus;
use netrunner_core::card::CardId;
use netrunner_core::rules::Side;

use crate::assets;
use crate::card_back;
use crate::core::ClientCore;
use crate::theme::Theme;
use crate::widgets::card_face::FaceSize;

pub struct CardImagesPlugin;

impl Plugin for CardImagesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CardImages>()
            .add_systems(Update, (paint_backs, request_wanted, poll_decoded).chain().run_if(resource_exists::<Assets<Image>>));
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
    Image::from_buffer(bytes, ImageType::Extension(extension), CompressedImageFormats::NONE, true, ImageSampler::linear(), RenderAssetUsages::default()).ok()
}

/// Both backs, once: a PNG under `assets/cards/` if there is one, else
/// the drawn one.
fn paint_backs(theme: Res<Theme>, mut images: ResMut<CardImages>, mut assets: ResMut<Assets<Image>>) {
    if images.backs.is_some() {
        return;
    }
    let back = |side: Side| {
        let file = format!("cards/back-{}.png", match side { Side::Corp => "corp", Side::Runner => "runner" });
        assets::read(&file).and_then(|bytes| decode(&bytes, "png")).unwrap_or_else(|| card_back::paint(side, &theme))
    };
    images.backs = Some([assets.add(back(Side::Corp)), assets.add(back(Side::Runner))]);
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
