//! NetrunnerDB's icon font, fetched into the cache on the player's
//! opt-in and loaded as a Bevy font — the third and prettiest tier for
//! the printed symbols, and the only source of the factions' and sets'
//! marks.
//!
//! The file is forty kilobytes and is asked for once: if it is on disk
//! it is loaded at once, whatever the opt-in says, because a cached
//! file costs no network call; if it is not, and `download_images` is
//! on, one fetch is started on the tokio runtime and its result waited
//! for a frame at a time. A fetch that fails is a notice and is not
//! retried this run — the next start tries again, the way a failed
//! scan is remembered in memory only. A player who has never opted in
//! is never asked for a connection: their faces draw the Noto glyphs
//! and their factions are named in words.
//!
//! **Loaded from bytes, not through the asset server**, for the reason
//! `card_images` gives: the cache directory is outside the asset root.
//! `Font::from_bytes` is what the font loader itself does with a file.

use std::path::PathBuf;

use bevy::prelude::*;
use tokio::sync::oneshot;

use crate::core::{ClientCore, Notices, TokioRuntime};
use crate::theme::Theme;

pub struct IconFontPlugin;

impl Plugin for IconFontPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IconFont>()
            .add_message::<IconFontReady>()
            // Only where fonts are assets: the headless tests have no
            // text plugin, and there the theme's icon font stays `None`.
            .add_systems(Update, load_icon_font.run_if(resource_exists::<Assets<Font>>));
    }
}

/// Written the frame the icon font becomes drawable, so a screen with
/// symbols on it can redraw them as icons.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconFontReady;

#[derive(Resource, Default)]
pub struct IconFont {
    fetch: Option<oneshot::Receiver<Result<PathBuf, String>>>,
    /// A fetch failed, or could not be started, this run.
    gave_up: bool,
    /// The disk is looked at once a second while the font is absent,
    /// not every frame: a stat per frame for a file that is not there
    /// is the wrong trade, and a second is sooner than a player can
    /// toggle the setting and come back.
    next_look: f64,
}

/// How often the cache is checked for a font that was not there.
const LOOK_EVERY_SECS: f64 = 1.0;

fn load_icon_font(
    mut icon: ResMut<IconFont>,
    mut theme: ResMut<Theme>,
    core: Res<ClientCore>,
    runtime: Option<Res<TokioRuntime>>,
    mut fonts: ResMut<Assets<Font>>,
    time: Res<Time>,
    mut ready: MessageWriter<IconFontReady>,
    mut notices: ResMut<Notices>,
) {
    if theme.icon_font.is_some() {
        return;
    }
    if let Some(receiver) = &mut icon.fetch {
        match receiver.try_recv() {
            Ok(Ok(_)) => {
                icon.fetch = None;
                icon.next_look = 0.0;
            }
            Ok(Err(error)) => {
                icon.fetch = None;
                icon.gave_up = true;
                notices.push(format!("Icon font not fetched: {error}"));
                return;
            }
            Err(oneshot::error::TryRecvError::Empty) => return,
            Err(oneshot::error::TryRecvError::Closed) => {
                icon.fetch = None;
                icon.gave_up = true;
                return;
            }
        }
    }
    let now = time.elapsed_secs_f64();
    if now < icon.next_look {
        return;
    }
    icon.next_look = now + LOOK_EVERY_SECS;
    if let Some(path) = core.images.icon_font() {
        match std::fs::read(&path) {
            Ok(bytes) => {
                theme.icon_font = Some(fonts.add(Font::from_bytes(bytes)));
                ready.write(IconFontReady);
            }
            Err(error) => {
                icon.gave_up = true;
                notices.push(format!("Icon font unreadable at {}: {error}", path.display()));
            }
        }
        return;
    }
    if icon.gave_up || !core.settings.desktop.download_images {
        return;
    }
    let Some(runtime) = runtime else {
        icon.gave_up = true;
        return;
    };
    let (sender, receiver) = oneshot::channel();
    let store = core.images.clone();
    runtime.0.spawn(async move {
        let _ = sender.send(store.download_icon_font().await.map_err(|error| error.to_string()));
    });
    icon.fetch = Some(receiver);
}
