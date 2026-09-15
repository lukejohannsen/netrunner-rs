//! The three-tier asset rule, in one place: a file under the player's
//! data directory wins, a file under the crate's `assets/` is next, and
//! whatever is procedural is the caller's fallback when neither exists.
//!
//! Bevy's `AssetServer` reads only its own root and refuses an absolute
//! path (`UnapprovedPathMode::Forbid`), and registering a second source
//! for the data directory has to happen before `DefaultPlugins`, which
//! would tie `main.rs` to a path resolved at boot. Reading the bytes and
//! decoding them ourselves (`Image::from_buffer`) needs neither, so that
//! is what the card backs and the cached card fronts do, and this module
//! only says where to look.

use std::path::PathBuf;

/// `<data dir>/netrunner/assets`, where a player's overrides go.
pub fn override_dir() -> Option<PathBuf> {
    netrunner_client::data_dir().map(|dir| dir.join("netrunner").join("assets"))
}

/// The crate's own `assets/` — the directory Bevy's asset server reads,
/// found the way it finds it: beside the executable, else where cargo
/// built from.
pub fn bundled_dir() -> PathBuf {
    let beside_exe = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.join("assets")));
    match beside_exe {
        Some(dir) if dir.is_dir() => dir,
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
    }
}

/// The first existing file for `relative` (`cards/back-corp.png`),
/// override before bundled; `None` means "draw it".
pub fn resolve(relative: &str) -> Option<PathBuf> {
    override_dir()
        .map(|dir| dir.join(relative))
        .into_iter()
        .chain(std::iter::once(bundled_dir().join(relative)))
        .find(|path| path.is_file())
}

/// The bytes of `resolve(relative)`, if any; an unreadable file is
/// treated as absent, since the procedural tier is always there.
pub fn read(relative: &str) -> Option<Vec<u8>> {
    std::fs::read(resolve(relative)?).ok()
}
