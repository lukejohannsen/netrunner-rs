//! The keys, in a file of their own.
//!
//! `settings.toml` is the file a person pastes into a bug report, so a
//! key is never in it (the rule `identity` set for the signing key). Each
//! profile's key is here, under the profile's name, in
//! `<identity dir>/secrets.toml` beside `identity.key` — created readable
//! by its owner alone and rewritten the same way.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SECRETS_FILE: &str = "secrets.toml";

/// A provider's key. `Debug` never prints it, so a profile in a log, a
/// panic or a test's failure message carries no key; `expose` is the one
/// way to read it, and only the request builder calls it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(key: impl Into<String>) -> Self {
        ApiKey(key.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

/// Every key, by profile name: `[keys]` with one line per profile.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Secrets {
    #[serde(default)]
    pub keys: BTreeMap<String, ApiKey>,
}

impl Secrets {
    pub fn path_in(dir: &Path) -> PathBuf {
        dir.join(SECRETS_FILE)
    }

    /// A missing file is no keys, not an error.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("{} is not a secrets file: {e}", path.display()))
    }

    /// Written to a temp file created owner-only (`identity::write_secret`,
    /// which refuses to open a file that exists) and renamed over the
    /// target, which keeps the temp's mode — so the file is never on disk
    /// with wider access, and a rewrite is as safe as the first write. A
    /// temp left by a crash is removed first, or `create_new` would refuse
    /// it for good.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        }
        let text = toml::to_string(self).expect("Secrets serializes");
        let tmp = path.with_extension("toml.tmp");
        if tmp.exists() {
            std::fs::remove_file(&tmp).map_err(|e| format!("could not remove {}: {e}", tmp.display()))?;
        }
        crate::identity::write_secret(&tmp, &text).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))
    }

    pub fn get(&self, name: &str) -> Option<&ApiKey> {
        self.keys.get(name).filter(|key| !key.is_empty())
    }

    /// Sets or, with `None`, clears a profile's key. Says whether anything changed.
    pub fn set(&mut self, name: &str, key: Option<ApiKey>) -> bool {
        match key.filter(|key| !key.is_empty()) {
            Some(key) => {
                let changed = self.keys.get(name) != Some(&key);
                self.keys.insert(name.to_string(), key);
                changed
            }
            None => self.keys.remove(name).is_some(),
        }
    }

    /// A profile renamed keeps its key.
    pub fn rename(&mut self, from: &str, to: &str) {
        if from != to
            && let Some(key) = self.keys.remove(from)
        {
            self.keys.insert(to.to_string(), key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("netrunner_secrets_{tag}_{}_{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn debug_never_prints_the_key() {
        let key = ApiKey::new("not-a-real-key");
        let shown = format!("{key:?}");
        assert!(!shown.contains("not-a-real-key"), "{shown}");
        let mut secrets = Secrets::default();
        secrets.set("p", Some(key.clone()));
        assert!(!format!("{secrets:?}").contains("not-a-real-key"));
        assert_eq!(secrets.get("p"), Some(&key));
    }

    #[test]
    fn a_missing_file_is_no_keys_and_a_file_round_trips() {
        let dir = temp_dir("round_trip");
        let path = Secrets::path_in(&dir);
        assert_eq!(Secrets::load(&path).unwrap(), Secrets::default());
        let mut secrets = Secrets::default();
        assert!(secrets.set("claude", Some(ApiKey::new("not-a-real-key"))));
        assert!(!secrets.set("claude", Some(ApiKey::new("not-a-real-key"))), "the same key again changes nothing");
        assert!(!secrets.set("blank", Some(ApiKey::new("  "))), "a blank key is no key");
        secrets.save(&path).unwrap();
        assert_eq!(Secrets::load(&path).unwrap(), secrets);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[keys]") && text.contains("claude = \"not-a-real-key\""), "{text}");
        secrets.rename("claude", "sonnet");
        assert!(secrets.get("claude").is_none() && secrets.get("sonnet").is_some());
        assert!(secrets.set("sonnet", None));
        assert!(secrets.get("sonnet").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn secrets_are_written_for_their_owner_alone_and_rewritten_in_place() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("mode");
        let path = Secrets::path_in(&dir);
        let mut secrets = Secrets::default();
        secrets.set("a", Some(ApiKey::new("not-a-real-key")));
        secrets.save(&path).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        // A stale temp from a crash does not block the next save.
        std::fs::write(path.with_extension("toml.tmp"), "stale").unwrap();
        secrets.set("b", Some(ApiKey::new("another-fake-key")));
        secrets.save(&path).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(Secrets::load(&path).unwrap(), secrets);
        assert!(!path.with_extension("toml.tmp").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
