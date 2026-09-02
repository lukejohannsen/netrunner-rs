//! Where saved decks live on disk, and how a deck name resolves to one.
//!
//! `netrunner_core` owns the deck *format* and both validators but performs
//! no I/O (AGENTS.md's decoupled engine rule), so directories, reads and
//! writes are this crate's job. Everything here is `std::fs`: nothing about
//! opening a deck file wants an async runtime.
//!
//! This module deliberately references no other module in this crate.
//! `netrunner_cli` has no `lib.rs`, so its integration tests reach source
//! through `#[path = "../src/..."] mod ...;`, and a module that reached for
//! `crate::` would not survive being included that way.

use std::path::{Path, PathBuf};

use netrunner_core::decks::{self, DeckFile};
use netrunner_core::rules::Side;

/// Environment variable naming the deck directory, for a persistent choice
/// that does not have to be repeated on every invocation. Outranked by
/// `--decks-dir`, so a one-off still wins over a shell profile.
pub const DECKS_DIR_ENV: &str = "NETRUNNER_DECKS_DIR";

/// Resolves the deck directory **without creating it**, so a read-only
/// command never leaves a directory behind as a side effect. Creation
/// happens in `save`, at the point something is actually written.
///
/// Precedence: `--decks-dir` beats [`DECKS_DIR_ENV`] beats the OS data
/// directory. `dirs::data_dir()` already resolves the OS-correct base
/// (`~/.local/share`, honoring `$XDG_DATA_HOME`, on Linux;
/// `~/Library/Application Support` on macOS; `%APPDATA%` on Windows), so a
/// single uniform `.join("netrunner/decks")` is right everywhere with no
/// OS-conditional branching — the same reasoning
/// `netrunner_card_sync::resolve_cache_dir` gives for the cache directory.
///
/// Decks are *data*, not configuration: they are content a player creates
/// and would expect to keep, which is why this is the data directory rather
/// than the config one.
pub fn resolve_decks_dir(flag: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(dir) = flag {
        return Ok(dir.to_path_buf());
    }
    if let Some(dir) = std::env::var_os(DECKS_DIR_ENV).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    dirs::data_dir()
        .map(|base| base.join("netrunner").join("decks"))
        .ok_or_else(|| format!("no OS data directory is available; set {DECKS_DIR_ENV} or pass --decks-dir"))
}

/// A deck, and where it came from — which is what `deck list` needs to mark
/// the built-in ones and what `save` needs to refuse to overwrite them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDeck {
    pub deck: DeckFile,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Compiled into the binary. Immutable: these are published decklists,
    /// and editing one in place would silently diverge from the published
    /// list it claims to be.
    Embedded,
    /// A file on disk.
    Disk(PathBuf),
}

impl Origin {
    pub fn is_embedded(&self) -> bool {
        matches!(self, Origin::Embedded)
    }
}

/// Reads every deck in `dir`, skipping non-`.json` entries.
///
/// A missing directory is an empty list, not an error — not having saved a
/// deck yet is the normal first-run state, and `deck list` should say "none"
/// rather than fail. A *malformed* deck file is still an error, because that
/// is a real problem the player wants told about.
pub fn read_dir(dir: &Path) -> Result<Vec<StoredDeck>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read deck directory {}: {e}", dir.display())),
    };

    let mut decks = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| format!("cannot read deck directory {}: {e}", dir.display()))?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        decks.push(StoredDeck { deck: read_file(&path)?, origin: Origin::Disk(path) });
    }
    decks.sort_by(|a, b| a.deck.id.cmp(&b.deck.id));
    Ok(decks)
}

/// Reads and parses one deck file.
pub fn read_file(path: &Path) -> Result<DeckFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    DeckFile::from_json(&text).map_err(|e| format!("{} is not a valid deck file: {e}", path.display()))
}

/// Every deck available, embedded first, then whatever is on disk.
///
/// An on-disk deck reusing an embedded id is an **error naming both**, not a
/// silent shadow: `--corp-deck party_hard` must never quietly mean something
/// other than the published deck of that name.
pub fn list(dir: &Path) -> Result<Vec<StoredDeck>, String> {
    let mut all: Vec<StoredDeck> =
        decks::embedded_decks().into_iter().map(|deck| StoredDeck { deck, origin: Origin::Embedded }).collect();

    for stored in read_dir(dir)? {
        if let Some(clash) = all.iter().find(|existing| existing.deck.id == stored.deck.id) {
            return Err(collision_message(&stored, clash));
        }
        all.push(stored);
    }
    Ok(all)
}

fn collision_message(stored: &StoredDeck, clash: &StoredDeck) -> String {
    let where_from = match &stored.origin {
        Origin::Disk(path) => path.display().to_string(),
        Origin::Embedded => "the embedded set".to_string(),
    };
    format!(
        "deck id {:?} is used by both a built-in deck and {}; \
         rename the saved one (its \"id\" field, and the file to match)",
        clash.deck.id, where_from
    )
}

/// Resolves a deck by name: an embedded id, then `<dir>/<name>.json`, then
/// `name` as a path.
///
/// Embedded first so the published decks cannot be shadowed, and a path last
/// so the common case — a bare name — never touches the filesystem beyond
/// the deck directory. The error lists what *is* available, matching the
/// convention the deck flags already set.
pub fn load(dir: &Path, name: &str) -> Result<StoredDeck, String> {
    let available = list(dir)?;

    if let Some(found) = available.iter().find(|stored| stored.deck.id == name) {
        return Ok(found.clone());
    }

    let in_dir = dir.join(format!("{name}.json"));
    if in_dir.is_file() {
        return Ok(StoredDeck { deck: read_file(&in_dir)?, origin: Origin::Disk(in_dir) });
    }

    let as_path = Path::new(name);
    if as_path.is_file() {
        return Ok(StoredDeck { deck: read_file(as_path)?, origin: Origin::Disk(as_path.to_path_buf()) });
    }

    let ids: Vec<&str> = available.iter().map(|stored| stored.deck.id.as_str()).collect();
    Err(format!(
        "no deck named {name:?} in {} and no file at that path; available: {}",
        dir.display(),
        if ids.is_empty() { "none".to_string() } else { ids.join(", ") }
    ))
}

/// Resolves a deck by name and checks it is for `side`.
///
/// Separate from [`load`] so `deck show` can display a deck of either side
/// while `--corp-deck` still refuses a Runner deck — with a message that
/// says which it got, since the id alone does not reveal the side.
pub fn load_for_side(dir: &Path, name: &str, side: Side) -> Result<StoredDeck, String> {
    let stored = load(dir, name)?;
    if stored.deck.side != side {
        return Err(format!("deck {name:?} is a {:?} deck, not {side:?}", stored.deck.side));
    }
    Ok(stored)
}

/// Writes `deck` to `<dir>/<id>.json`, creating the directory if needed.
///
/// Temp-file-plus-rename in the *same* directory, so the rename is
/// same-filesystem and therefore atomic — an interrupted write cannot leave
/// a truncated deck behind. Copied from
/// `netrunner_card_sync::NetrunnerDbSync::write_cache_atomically`, which
/// made the same call for the same reason.
pub fn save(dir: &Path, deck: &DeckFile) -> Result<PathBuf, String> {
    if decks::by_id(&deck.id).is_some() {
        return Err(format!(
            "deck id {:?} belongs to a built-in deck; built-in decks are immutable, so pick another id",
            deck.id
        ));
    }

    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create deck directory {}: {e}", dir.display()))?;

    let target = dir.join(format!("{}.json", deck.id));
    let temp = dir.join(format!("{}.json.tmp.{}", deck.id, std::process::id()));
    let mut json = deck.to_json().map_err(|e| format!("cannot serialize deck {:?}: {e}", deck.id))?;
    json.push('\n');

    std::fs::write(&temp, json).map_err(|e| format!("cannot write {}: {e}", temp.display()))?;
    std::fs::rename(&temp, &target).map_err(|e| format!("cannot save {}: {e}", target.display()))?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal std-only stand-in for a `tempfile` temp directory, matching
    /// `netrunner_card_sync`'s: neither crate depends on `tempfile`, and one
    /// struct is cheaper than a dependency.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "netrunner_deck_store_test_{}_{}",
                std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
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

    fn custom_deck(id: &str) -> DeckFile {
        let mut deck = decks::by_id("party_hard").expect("party_hard is embedded");
        deck.id = id.to_string();
        deck.name = format!("Copy of {id}");
        deck.category = netrunner_core::decks::DeckCategory::Custom;
        deck
    }

    #[test]
    fn an_explicit_directory_outranks_the_environment() {
        // SAFETY: single-threaded test process; the var is restored below.
        unsafe { std::env::set_var(DECKS_DIR_ENV, "/from/env") };
        let resolved = resolve_decks_dir(Some(Path::new("/from/flag"))).expect("flag resolves");
        unsafe { std::env::remove_var(DECKS_DIR_ENV) };

        assert_eq!(resolved, PathBuf::from("/from/flag"));
    }

    #[test]
    fn the_environment_outranks_the_default() {
        unsafe { std::env::set_var(DECKS_DIR_ENV, "/from/env") };
        let resolved = resolve_decks_dir(None).expect("env resolves");
        unsafe { std::env::remove_var(DECKS_DIR_ENV) };

        assert_eq!(resolved, PathBuf::from("/from/env"));
    }

    #[test]
    fn the_default_lands_under_the_os_data_directory() {
        let resolved = resolve_decks_dir(None).expect("a data dir exists in the test environment");
        assert!(resolved.ends_with("netrunner/decks"), "{resolved:?}");
    }

    /// Not having saved a deck yet is the normal first-run state, not a
    /// failure — `deck list` must still show the built-in decks.
    #[test]
    fn a_missing_deck_directory_reads_as_empty() {
        let dir = TempDir::new();
        let missing = dir.path().join("nope");

        assert_eq!(read_dir(&missing).expect("missing dir is not an error"), Vec::new());
        assert_eq!(list(&missing).expect("listing still works").len(), netrunner_core::decks::embedded_decks().len(), "every built-in deck");
    }

    #[test]
    fn a_saved_deck_round_trips_and_is_listed_alongside_the_built_in_ones() {
        let dir = TempDir::new();
        let deck = custom_deck("my_deck");

        let path = save(dir.path(), &deck).expect("saves");
        assert_eq!(path, dir.path().join("my_deck.json"));
        assert_eq!(read_file(&path).expect("reads back"), deck);

        let all = list(dir.path()).expect("lists");
        assert_eq!(all.len(), netrunner_core::decks::embedded_decks().len() + 1, "the built-in decks plus one saved");
        let saved = all.iter().find(|stored| stored.deck.id == "my_deck").expect("saved deck is listed");
        assert!(!saved.origin.is_embedded());
    }

    #[test]
    fn saving_leaves_no_temp_file_behind() {
        let dir = TempDir::new();
        save(dir.path(), &custom_deck("my_deck")).expect("saves");

        let names: Vec<String> = std::fs::read_dir(dir.path())
            .expect("dir exists")
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["my_deck.json".to_string()]);
    }

    #[test]
    fn a_saved_deck_may_not_shadow_a_built_in_id() {
        let dir = TempDir::new();
        let mut clash = custom_deck("party_hard");
        clash.id = "party_hard".to_string();

        let err = save(dir.path(), &clash).expect_err("built-in ids are reserved");
        assert!(err.contains("built-in"), "{err}");
    }

    /// A file written by hand can still collide, since `save` is not the
    /// only way a file gets into the directory. Listing must catch it rather
    /// than silently preferring one.
    #[test]
    fn a_hand_written_file_colliding_with_a_built_in_id_is_an_error() {
        let dir = TempDir::new();
        let mut clash = custom_deck("party_hard");
        clash.id = "party_hard".to_string();
        std::fs::write(dir.path().join("party_hard.json"), clash.to_json().unwrap()).expect("write");

        let err = list(dir.path()).expect_err("a shadowed built-in id must be reported");
        assert!(err.contains("party_hard"), "{err}");
    }

    #[test]
    fn load_resolves_embedded_then_directory_then_path() {
        let dir = TempDir::new();
        let saved = save(dir.path(), &custom_deck("my_deck")).expect("saves");

        assert!(load(dir.path(), "party_hard").expect("embedded").origin.is_embedded());
        assert_eq!(load(dir.path(), "my_deck").expect("in dir").deck.id, "my_deck");

        let elsewhere = dir.path().join("moved.json");
        std::fs::rename(&saved, &elsewhere).expect("move out of the id-named slot");
        let by_path = load(dir.path(), elsewhere.to_str().unwrap()).expect("by path");
        assert_eq!(by_path.deck.id, "my_deck");
    }

    #[test]
    fn an_unknown_deck_name_lists_what_is_available() {
        let dir = TempDir::new();
        let err = load(dir.path(), "no_such_deck").expect_err("unknown name");
        assert!(err.contains("party_hard"), "the error should list real decks: {err}");
    }

    #[test]
    fn asking_for_a_runner_deck_in_the_corp_slot_is_rejected() {
        let dir = TempDir::new();
        let err = load_for_side(dir.path(), "party_hard", Side::Corp).expect_err("party_hard is a Runner deck");
        assert!(err.contains("Runner"), "{err}");
    }

    #[test]
    fn a_malformed_deck_file_is_reported_rather_than_skipped() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("broken.json"), "{ not json").expect("write");

        let err = read_dir(dir.path()).expect_err("a malformed deck file is a real problem");
        assert!(err.contains("broken.json"), "the error should name the file: {err}");
    }

    #[test]
    fn non_json_files_in_the_deck_directory_are_ignored() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("notes.txt"), "not a deck").expect("write");

        assert_eq!(read_dir(dir.path()).expect("ignores non-json").len(), 0);
    }
}
