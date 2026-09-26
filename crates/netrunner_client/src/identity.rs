//! Who this player is to a server: their key, and the servers' keys they
//! have met (Phase 4 §5 stage b, `docs/identity-and-rating.md`).
//!
//! **The key has its own file, never `settings.json`.** The settings file
//! is the one a person pastes into a bug report, and the key is the one
//! thing on this machine a rating is filed under. `identity.key` sits
//! beside the settings, is made the first time a connection wants it, and
//! is created readable by its owner alone. The "one struct, every client"
//! rule is kept the same way `settings` keeps it: both clients go through
//! this module, so neither makes a second key or reads the file its own
//! way.
//!
//! **Losing the file loses the identity; copying it is how a second
//! machine plays as the same person.** A new key starts the ladder again
//! at every server. Rotating a key (the old one signing the new) is
//! recorded in the design and not built.
//!
//! **A server's key is remembered on first contact, known-hosts style**
//! (`known_servers.json`), when the server says it keeps its key
//! (`ServerMessage::Challenge::lasting`). A later connection to the same
//! address that meets another key is refused, naming both, rather than
//! proving this player's key to whoever answered. The file is not a
//! secret; it is apart from the settings only because the connection
//! driver writes it and the settings are the screens'.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use netrunner_identity::{Identity, PublicKey};

/// Overrides the directory both files live in — for a test, a second
/// profile on one machine, or a dev hook that must not touch the
/// developer's own key.
pub const IDENTITY_DIR_ENV: &str = "NETRUNNER_IDENTITY_DIR";
pub const IDENTITY_FILE: &str = "identity.key";
pub const KNOWN_SERVERS_FILE: &str = "known_servers.json";
/// Every receipt a server has handed this player (Phase 4 §5 stage d),
/// one signed `netrunner_protocol::statements::Receipt` per line: a
/// player who keeps their receipts can show their history even if a
/// server loses its own.
pub const RECEIPTS_FILE: &str = "receipts.jsonl";

/// `$NETRUNNER_IDENTITY_DIR`, else `<data dir>/netrunner`, beside the
/// settings, the saved decks and the record.
pub fn resolve_identity_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os(IDENTITY_DIR_ENV).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    crate::data_dir().map(|base| base.join("netrunner")).ok_or_else(|| format!("no OS data directory is available; set {IDENTITY_DIR_ENV}"))
}

/// What a connection proves itself with: the player's key, and the
/// directory the servers' keys and the receipts are kept in (`None` keeps
/// neither — a test's, or a session with nowhere to write).
#[derive(Debug, Clone)]
pub struct Credentials {
    pub identity: Identity,
    pub dir: Option<PathBuf>,
}

impl Credentials {
    /// The key in `dir`, made there if there is none, and the servers
    /// remembered beside it.
    pub fn in_dir(dir: &Path) -> Result<Self, String> {
        let identity = load_or_make(&dir.join(IDENTITY_FILE))?;
        Ok(Credentials { identity, dir: Some(dir.to_path_buf()) })
    }

    pub fn known_servers(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|dir| dir.join(KNOWN_SERVERS_FILE))
    }

    pub fn receipts(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|dir| dir.join(RECEIPTS_FILE))
    }

    /// The player's own, from `resolve_identity_dir`.
    pub fn load() -> Result<Self, String> {
        Credentials::in_dir(&resolve_identity_dir()?)
    }
}

/// The key in `dir` if one has been made, without making one: what a
/// screen that only *shows* the key reads, so opening it never writes.
pub fn existing(dir: &Path) -> Result<Option<Credentials>, String> {
    if !dir.join(IDENTITY_FILE).exists() {
        return Ok(None);
    }
    Credentials::in_dir(dir).map(Some)
}

/// The key's lines where it may not exist yet: its own, or a line saying
/// when one will be made.
pub fn key_lines_in(dir: Option<&Path>) -> Vec<String> {
    match dir.map(existing) {
        Some(Ok(Some(credentials))) => key_lines(&credentials),
        Some(Ok(None)) => vec!["No key yet: one is made the first time you join a game online".to_string()],
        Some(Err(error)) => vec![format!("Your key: {error}")],
        None => vec!["No key: the OS has no data directory, so games online are unrated".to_string()],
    }
}

/// The key at `path`, or a new one written there. A file that is there
/// and cannot be read is an error and is left alone: replacing it would
/// throw away the player's standing at every server without a word.
pub fn load_or_make(path: &Path) -> Result<Identity, String> {
    if path.exists() {
        let text = std::fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        return Identity::from_file_text(&text).map_err(|e| format!("{}: {e}", path.display()));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    let identity = Identity::from_secret(rand::random());
    write_secret(path, &identity.to_file_text()).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(identity)
}

/// Created readable by its owner alone, rather than narrowed after, so
/// the secret is never on disk with wider access. `create_new`, so two
/// clients starting at once cannot both write a key and have the second
/// win after the first has been used.
fn write_secret(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(text.as_bytes())
}

/// The key's lines for a profile: its fingerprint, and the two facts a
/// person must know about the file (`docs/identity-and-rating.md` §1) —
/// both clients say them in these words.
pub fn key_lines(credentials: &Credentials) -> Vec<String> {
    let mut lines = vec![format!("Your key: {}", credentials.identity.public_key().fingerprint())];
    if let Some(dir) = &credentials.dir {
        lines.push(format!("Kept in {}", dir.join(IDENTITY_FILE).display()));
    }
    lines.push("A server rates you by this key, not your name. Lose the file and you start again everywhere; copy it to another machine to play as you there.".to_string());
    lines
}

/// What a server said a game did to this seat's rating, in words:
/// `Rated as the Corp at this server: 1500 → 1662 ± 290`. The numbers
/// are the server's; the client keeps none (Phase 3 §2).
pub fn rated_line(side: netrunner_core::rules::Side, before: &netrunner_protocol::Rating, after: &netrunner_protocol::Rating) -> String {
    format!("Rated as the {side:?} at this server: {:.0} → {:.0} ± {:.0}", before.rating, after.rating, after.deviation)
}

/// A standing a server reported, one line per side it has played:
/// `As Corp: 1612 ± 120 (3–1–0)`. Empty for a key with no rated game.
pub fn standing_lines(standing: Option<&netrunner_protocol::Standing>) -> Vec<String> {
    let Some(standing) = standing else { return vec!["No rated games there yet".to_string()] };
    [("Corp", &standing.corp), ("Runner", &standing.runner)]
        .into_iter()
        .filter(|(_, record)| record.games() > 0)
        .map(|(side, record)| format!("As {side}: {:.0} ± {:.0} ({}–{}–{})", record.rating.rating, record.rating.deviation, record.wins, record.draws, record.losses))
        .collect()
}

/// Appends a receipt a server handed this player. A failed write loses
/// the player's copy and nothing else: the server keeps its own.
pub fn keep_receipt(path: &Path, receipt: &netrunner_identity::Signed) -> Result<(), String> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    let line = serde_json::to_string(receipt).expect("a signed receipt serializes");
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| writeln!(file, "{line}"))
        .map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// The servers' keys this player has met, by the address they were
/// dialled at.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KnownServers(BTreeMap<String, PublicKey>);

impl KnownServers {
    /// A missing file is nobody met yet, not an error.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let json = std::fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        serde_json::from_str(&json).map_err(|e| format!("{} is not a list of servers: {e}", path.display()))
    }

    /// Every address remembered: the servers that keep their key, which
    /// are the ones that keep ratings.
    pub fn addresses(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    pub fn get(&self, address: &str) -> Option<PublicKey> {
        self.0.get(address.trim()).copied()
    }

    pub fn insert(&mut self, address: &str, key: PublicKey) {
        self.0.insert(address.trim().to_string(), key);
    }

    /// Temp file and rename, like the settings.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(self).expect("a map of strings serializes");
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("netrunner_client_identity_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_key_is_made_once_for_its_owner_and_read_back_after() {
        let dir = scratch("made");
        let first = Credentials::in_dir(&dir).unwrap();
        let second = Credentials::in_dir(&dir).unwrap();
        assert_eq!(first.identity.public_key(), second.identity.public_key());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(IDENTITY_FILE)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreadable_key_file_is_an_error_and_is_left_alone() {
        let dir = scratch("unreadable");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(IDENTITY_FILE), "garbage").unwrap();
        assert!(Credentials::in_dir(&dir).is_err());
        assert_eq!(std::fs::read_to_string(dir.join(IDENTITY_FILE)).unwrap(), "garbage");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rating_and_a_standing_read_as_lines() {
        use netrunner_protocol::{Rating, Standing};
        let rating = |rating, deviation| Rating { rating, deviation, volatility: 0.06 };
        assert_eq!(rated_line(netrunner_core::rules::Side::Corp, &rating(1500.0, 350.0), &rating(1662.4, 290.1)), "Rated as the Corp at this server: 1500 → 1662 ± 290");
        let mut standing = Standing::default();
        standing.runner.wins = 2;
        standing.runner.losses = 1;
        standing.runner.rating = rating(1580.0, 200.0);
        assert_eq!(standing_lines(Some(&standing)), vec!["As Runner: 1580 ± 200 (2–0–1)"], "a side never played is left out");
        assert_eq!(standing_lines(None), vec!["No rated games there yet"]);
    }

    #[test]
    fn a_server_remembered_is_found_by_its_address() {
        let dir = scratch("known");
        let path = dir.join(KNOWN_SERVERS_FILE);
        assert_eq!(KnownServers::load(&path).unwrap(), KnownServers::default());
        let key = Identity::from_secret([3; 32]).public_key();
        let mut known = KnownServers::default();
        known.insert(" ws://example.org:8080 ", key);
        known.save(&path).unwrap();
        let back = KnownServers::load(&path).unwrap();
        assert_eq!(back.get("ws://example.org:8080"), Some(key));
        assert_eq!(back.get("ws://example.org:9090"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
