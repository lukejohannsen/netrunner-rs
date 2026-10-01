use std::path::PathBuf;

use crate::error::SyncError;

/// Resolves the OS-appropriate cache directory for what this crate fetches,
/// WITHOUT creating it. `dirs::cache_dir()` already resolves to
/// the OS-correct base (`~/.cache`, honoring `$XDG_CACHE_HOME`, on Linux;
/// `~/Library/Caches` on macOS; `%LOCALAPPDATA%` on Windows), so a single
/// uniform `.join("netrunner")` produces `~/.cache/netrunner`,
/// `~/Library/Caches/netrunner`, and `%LOCALAPPDATA%\netrunner` respectively
/// — no OS-conditional branching needed.
pub fn resolve_cache_dir() -> Result<PathBuf, SyncError> {
    dirs::cache_dir().map(|base| base.join("netrunner")).ok_or(SyncError::CacheDirUnavailable)
}

/// Where downloaded card images live: `resolve_cache_dir()/images`, one
/// `<code>.webp` (or `<code>.jpg` where NetrunnerDB has no larger scan)
/// per NetrunnerDB printing. Under the *cache* directory, not the data
/// one, because every file in it can be fetched again and none of it is
/// the player's own.
pub fn resolve_images_dir() -> Result<PathBuf, SyncError> {
    resolve_cache_dir().map(|dir| dir.join("images"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_dir_ends_with_netrunner() {
        let dir = resolve_cache_dir().expect("cache dir should resolve in test environment");
        assert_eq!(dir.file_name().and_then(|n| n.to_str()), Some("netrunner"));
    }

    #[test]
    fn images_dir_is_under_the_cache_dir() {
        let dir = resolve_images_dir().expect("cache dir should resolve in test environment");
        assert!(dir.ends_with("netrunner/images"), "{dir:?}");
    }
}
