//! Filesystem JSON card loader for **external** card directories —
//! feature-gated (`fs-loader`) since `netrunner_core` is otherwise I/O-free
//! (see `Cargo.toml`'s doc comment on the feature and `CardRegistry`'s own
//! doc comment).
//!
//! This is *not* how the first-party sets load: those are embedded at
//! compile time by `build.rs` and registered via
//! `cards::register_playable_cards` (see `cards::embedded`). Reach for this
//! loader instead when cards live outside the binary — user homebrew or
//! custom sets, an external card directory, or iterating on card JSON
//! without recompiling.

use std::path::{Path, PathBuf};

use crate::cards::CardRegistry;
use crate::dsl::{CardDefinition, CardValidationError};

/// What can go wrong loading a directory of card JSON files: an I/O failure
/// reading the directory or a file, a JSON parse failure, or a semantic
/// validation failure (`CardDefinition::validate`) — the latter two carry the
/// offending file's path so a bad card is easy to find.
#[derive(Debug, thiserror::Error)]
pub enum LoaderError {
    #[error("I/O error reading {path:?}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("failed to parse {path:?} as a CardDefinition: {source}")]
    Parse { path: PathBuf, source: serde_json::Error },
    #[error("{path:?} failed validation: {source}")]
    Validation { path: PathBuf, source: CardValidationError },
}

/// Walks each directory in `dirs` (non-recursively) for `*.json` files,
/// parses each as a `CardDefinition`, validates it (`CardDefinition::validate`), and inserts it
/// into a fresh `CardRegistry`. Non-`.json` files are silently skipped —
/// everything else is an error, surfaced with the offending path attached.
pub fn load_registry_from_dirs(dirs: &[&Path]) -> Result<CardRegistry, LoaderError> {
    let mut registry = CardRegistry::new();
    for dir in dirs {
        for entry in std::fs::read_dir(dir).map_err(|source| LoaderError::Io { path: dir.to_path_buf(), source })? {
            let entry = entry.map_err(|source| LoaderError::Io { path: dir.to_path_buf(), source })?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let text = std::fs::read_to_string(&path).map_err(|source| LoaderError::Io { path: path.clone(), source })?;
            let card: CardDefinition =
                serde_json::from_str(&text).map_err(|source| LoaderError::Parse { path: path.clone(), source })?;
            card.validate().map_err(|source| LoaderError::Validation { path: path.clone(), source })?;
            registry.insert(card);
        }
    }
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Write;

    /// A throwaway directory under the OS temp dir, cleaned up on `Drop` —
    /// there's no existing temp-dir helper crate in this workspace, and one
    /// isn't worth adding for a handful of loader tests.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("netrunner_core_loader_test_{name}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }

        fn write(&self, filename: &str, contents: &str) {
            let mut file = std::fs::File::create(self.0.join(filename)).expect("create temp file");
            file.write_all(contents.as_bytes()).expect("write temp file");
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const HEDGE_FUND_JSON: &str =
        r#"{"id":"hedge_fund","title":"Hedge Fund","side":"Corp","card_type":"Operation","cost":5,
            "triggers":[{"trigger":"OnPlay","effects":[{"GainCredits":["Corp",9]}]}]}"#;
    const ICE_WALL_JSON: &str = r#"{"id":"ice_wall","title":"Ice Wall","side":"Corp","card_type":{"Ice":"Barrier"},
            "cost":1,"strength":1,"subroutines":[{"text":"End the run.","effect":"EndTheRun"}],"triggers":[]}"#;

    #[test]
    fn load_registry_from_dirs_parses_every_json_file_in_both_directories() {
        let corp = TempDir::new("corp");
        corp.write("hedge_fund.json", HEDGE_FUND_JSON);
        corp.write("ice_wall.json", ICE_WALL_JSON);
        let runner = TempDir::new("runner");
        runner.write("sure_gamble.json", &HEDGE_FUND_JSON.replace("hedge_fund", "sure_gamble").replace("Corp", "Runner"));

        let registry = load_registry_from_dirs(&[corp.path(), runner.path()]).expect("should load cleanly");

        assert!(registry.get(&crate::dsl::CardId("hedge_fund".to_string())).is_some());
        assert!(registry.get(&crate::dsl::CardId("ice_wall".to_string())).is_some());
        assert!(registry.get(&crate::dsl::CardId("sure_gamble".to_string())).is_some());
    }

    #[test]
    fn load_registry_from_dirs_skips_non_json_files() {
        let dir = TempDir::new("skip_non_json");
        dir.write("hedge_fund.json", HEDGE_FUND_JSON);
        dir.write("README.md", "not a card");

        let registry = load_registry_from_dirs(&[dir.path()]).expect("should load cleanly, ignoring README.md");

        assert!(registry.get(&crate::dsl::CardId("hedge_fund".to_string())).is_some());
    }

    #[test]
    fn load_registry_from_dirs_surfaces_parse_errors_with_the_offending_file_path() {
        let dir = TempDir::new("parse_error");
        dir.write("broken.json", "{ not valid json");

        let result = load_registry_from_dirs(&[dir.path()]);

        match result {
            Err(LoaderError::Parse { path, .. }) => assert_eq!(path.file_name().unwrap(), "broken.json"),
            other => panic!("expected a Parse error, got {other:?}"),
        }
    }

    #[test]
    fn load_registry_from_dirs_rejects_a_card_failing_semantic_validation() {
        let dir = TempDir::new("validation_error");
        // An Ice card with no strength — structurally valid JSON, fails
        // `CardDefinition::validate`'s `IceMissingStrength` check.
        dir.write(
            "bad_ice.json",
            r#"{"id":"bad_ice","title":"Bad Ice","side":"Corp","card_type":{"Ice":"Barrier"},"cost":1,"triggers":[]}"#,
        );

        let result = load_registry_from_dirs(&[dir.path()]);

        match result {
            Err(LoaderError::Validation { path, source: CardValidationError::IceMissingStrength(_) }) => {
                assert_eq!(path.file_name().unwrap(), "bad_ice.json")
            }
            other => panic!("expected a Validation(IceMissingStrength) error, got {other:?}"),
        }
    }
}
