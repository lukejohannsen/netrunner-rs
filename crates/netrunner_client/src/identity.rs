//! Who this player is to a server: their key, and the servers' keys they
//! have met (Phase 4 §5 stage b, `docs/identity-and-rating.md`).
//!
//! **The key has its own file, never `settings.toml`.** The settings file
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
//! **A server's key is remembered on first contact, known-hosts style**,
//! in the player's list of servers (`servers`, Phase 7 §14), when the
//! server says it keeps its key (`ServerMessage::Challenge::lasting`). A
//! later connection to the same address that meets another key is
//! refused, naming both, rather than proving this player's key to
//! whoever answered.

use std::path::{Path, PathBuf};

use netrunner_core::format::NsgFormat;
use netrunner_identity::Identity;
use netrunner_protocol::{FormatStanding, Profile};
/// Re-exported for the two clients, which name a key only to say whose a
/// tournament entry is and never depend on the identity crate themselves.
pub use netrunner_identity::{PublicKey, Signed};

/// Overrides the directory both files live in — for a test, a second
/// profile on one machine, or a dev hook that must not touch the
/// developer's own key.
pub const IDENTITY_DIR_ENV: &str = "NETRUNNER_IDENTITY_DIR";
pub const IDENTITY_FILE: &str = "identity.key";
/// Every receipt a server has handed this player (Phase 4 §5 stage d),
/// one signed `netrunner_protocol::statements::Receipt` per line: a
/// player who keeps their receipts can show their history even if a
/// server loses its own.
pub const RECEIPTS_FILE: &str = "receipts.jsonl";
/// Every tournament registration this key made, one JSON line each
/// (`Registration`): the statement it signed and the salt the hashes
/// were made with, which is what checks the list the server reveals.
pub const REGISTRATIONS_FILE: &str = "registrations.jsonl";

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

    /// The player's list of servers (`servers::ServerBook`), which holds
    /// the keys they proved.
    pub fn servers(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|dir| dir.join(crate::servers::SERVERS_FILE))
    }

    pub fn receipts(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|dir| dir.join(RECEIPTS_FILE))
    }

    pub fn registrations(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|dir| dir.join(REGISTRATIONS_FILE))
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

/// A badge's name as a file's stem: lower case, every run of anything
/// but a letter or a digit one hyphen (`first-gauntlet`, from "First
/// Gauntlet!").
pub fn badge_stem(name: &str) -> String {
    let mut stem = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            stem.extend(c.to_lowercase());
        } else if !stem.ends_with('-') && !stem.is_empty() {
            stem.push('-');
        }
    }
    stem.trim_end_matches('-').to_string()
}

/// Up to two letters for a badge drawn without a picture — the desktop's
/// ring, the terminal's brackets: the first of each of the first
/// two words, or the first two of a one-word name.
pub fn badge_initials(name: &str) -> String {
    let words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|word| !word.is_empty()).collect();
    let letters: String = match words.as_slice() {
        [] => String::new(),
        [one] => one.chars().take(2).collect(),
        [first, second, ..] => first.chars().take(1).chain(second.chars().take(1)).collect(),
    };
    letters.to_uppercase()
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
pub(crate) fn write_secret(path: &Path, text: &str) -> std::io::Result<()> {
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

/// What a server has said about this connection's profile, as the Server
/// page keeps it between the answer and the next game (Phase 4 §7 stage
/// 5, §10). The server's `Profile` reply says `None` for a server that
/// keeps nothing; the key it echoes tells "no key" apart, and the page
/// wants a state for the moment before any answer. A server's ladders are
/// one per format, so a known key has a standing per format it has
/// played in, which may be none.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ProfileHere {
    /// Not answered yet.
    #[default]
    Unasked,
    /// This connection proved no key, so nothing here counts for it.
    NoKey,
    /// A key, at a server that keeps nothing under it.
    Unkept,
    /// A key the server keeps a profile under.
    Known(Profile),
}

impl ProfileHere {
    /// The server's `Profile { key, profile }`, read.
    pub fn from_reply(key: Option<PublicKey>, profile: Option<Profile>) -> Self {
        match (key, profile) {
            (None, _) => ProfileHere::NoKey,
            (Some(_), None) => ProfileHere::Unkept,
            (Some(_), Some(profile)) => ProfileHere::Known(profile),
        }
    }

    /// This key's standing in `format` here, if it has one.
    pub fn in_format(&self, format: NsgFormat) -> Option<&netrunner_protocol::Standing> {
        match self {
            ProfileHere::Known(profile) => profile.in_format(format),
            _ => None,
        }
    }
}

/// The line over Find a game, for a lobby in `format` whose games are
/// `rated` or not (`LobbyInfo::rated`) at a server this connection stands
/// at as `standing`; `hosting` names the case where the server is the
/// person's own, which never rates. One sentence, because the page is
/// read before a game, not after one, and the lobby's format's standing
/// alone, because that is the ladder a game here moves. The word is
/// "unrated", never "casual": while a format was named Casual
/// (Unrestricted since Phase 4 §8), "Casual · casual" on its lobby's row
/// said nothing.
pub fn lobby_rating_line(rated: bool, hosting: bool, format: NsgFormat, profile: &ProfileHere) -> String {
    if hosting {
        return "Unrated: a game hosted from this machine is never rated.".to_string();
    }
    if !rated {
        return "Unrated: nothing in this lobby is rated.".to_string();
    }
    match profile {
        ProfileHere::Unasked => "Rated here.".to_string(),
        ProfileHere::NoKey => "Unrated for you: this client has no key, so nothing here counts.".to_string(),
        ProfileHere::Unkept => "Rated here. No rated games here yet.".to_string(),
        ProfileHere::Known(_) => match profile.in_format(format) {
            Some(standing) => format!("Rated here. In {format:?}: {}", role_lines(standing).join(" · ")),
            None => format!("Rated here. No rated games in {format:?} yet."),
        },
    }
}

/// A profile a server reported, as the Profile screen and
/// `netrunner_cli profile` print it: the name the server knows the key
/// by, what it wears, the awards earned, then a standing line per format
/// (`standing_lines`). One line for a server that keeps nothing.
pub fn profile_lines(profile: Option<&Profile>) -> Vec<String> {
    let Some(profile) = profile else { return vec!["Nothing kept there yet".to_string()] };
    let mut lines = vec![format!("Known there as {}", profile.name)];
    match (&profile.title, &profile.badge) {
        (Some(title), Some(badge)) => lines.push(format!("Wearing the title {title} and the badge {badge}")),
        (Some(title), None) => lines.push(format!("Wearing the title {title}")),
        (None, Some(badge)) => lines.push(format!("Wearing the badge {badge}")),
        (None, None) => {}
    }
    if !profile.titles.is_empty() {
        lines.push(format!("Titles earned: {}", profile.titles.iter().map(|award| award.name.as_str()).collect::<Vec<_>>().join(", ")));
    }
    if !profile.badges.is_empty() {
        lines.push(format!("Badges earned: {}", profile.badges.iter().map(|award| award.name.as_str()).collect::<Vec<_>>().join(", ")));
    }
    lines.extend(standing_lines(&profile.standings));
    lines
}

/// The standings a server reported, one line per format and per side
/// played in it: `Standard · As Corp: 1612 ± 120 (3–1–0)`. One line
/// saying so for a key with no rated game.
pub fn standing_lines(standings: &[FormatStanding]) -> Vec<String> {
    if standings.is_empty() {
        return vec!["No rated games there yet".to_string()];
    }
    standings.iter().flat_map(|entry| role_lines(&entry.standing).into_iter().map(move |line| format!("{:?} · {line}", entry.format))).collect()
}

/// One row of a ladder, numbered from 1: `3. ann · 1612 ± 120 overall · As
/// Corp: … · As Runner: … · Gauntlet Runner` — the key's fingerprint when
/// the server has no name for it, the title worn last when there is one.
pub fn ladder_line(place: usize, row: &netrunner_protocol::LadderRow) -> String {
    let name = if row.name.is_empty() { row.key.fingerprint() } else { row.name.clone() };
    let mut parts = vec![format!("{place}. {name}"), format!("{:.0} overall", row.standing.overall())];
    parts.extend(role_lines(&row.standing));
    if let Some(title) = &row.title {
        parts.push(title.clone());
    }
    parts.join(" · ")
}

/// One standing's sides, each that has played: `As Corp: 1612 ± 120 (3–1–0)`.
fn role_lines(standing: &netrunner_protocol::Standing) -> Vec<String> {
    [("Corp", &standing.corp), ("Runner", &standing.runner)]
        .into_iter()
        .filter(|(_, record)| record.games() > 0)
        .map(|(side, record)| format!("As {side}: {:.0} ± {:.0} ({}–{}–{})", record.rating.rating, record.rating.deviation, record.wins, record.draws, record.losses))
        .collect()
}

/// Appends a receipt a server handed this player. A failed write loses
/// the player's copy and nothing else: the server keeps its own.
/// One tournament registration as the player keeps it: the statement
/// signed, the salt its hashes were made with, and which saved decks they
/// were — so the player can check the list the server reveals, or show
/// their own.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Registration {
    pub statement: netrunner_protocol::statements::RegistrationStatement,
    pub salt: String,
    /// The decks' ids (`DeckFile::id`).
    pub corp: String,
    pub runner: String,
}

/// The last registration kept for `tournament` (its code, read as typed
/// either way): the one a seat at its table is checked against (Phase 4
/// §7 stage 6c). A line that does not parse is skipped, and a file that
/// cannot be read is none.
pub fn registration_for(path: &Path, tournament: &str) -> Option<Registration> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().rev().filter_map(|line| serde_json::from_str::<Registration>(line).ok()).find(|kept| kept.statement.tournament.eq_ignore_ascii_case(tournament))
}

/// Appends `registration` to the registrations file, as `keep_receipt`
/// keeps a receipt.
pub fn keep_registration(path: &Path, registration: &Registration) -> Result<(), String> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    let line = serde_json::to_string(registration).expect("a registration serializes");
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| writeln!(file, "{line}"))
        .map_err(|e| format!("could not write {}: {e}", path.display()))
}

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
        let standings = vec![FormatStanding { format: NsgFormat::Standard, standing }];
        assert_eq!(standing_lines(&standings), vec!["Standard · As Runner: 1580 ± 200 (2–0–1)"], "a side never played is left out");
        assert_eq!(standing_lines(&[]), vec!["No rated games there yet"]);
        // The lobby's line: what the lobby offers first, then what this
        // connection stands to gain there — on the lobby's own ladder.
        let profile = Profile {
            name: "ann".into(),
            first_seen: 1,
            last_seen: 2,
            standings: standings.clone(),
            titles: vec![netrunner_protocol::Award { name: "Gauntlet Runner".into(), earned_at: 2, source: "event:abc".into() }],
            badges: Vec::new(),
            title: Some("Gauntlet Runner".into()),
            badge: None,
        };
        let known = ProfileHere::Known(profile.clone());
        assert_eq!(lobby_rating_line(true, true, NsgFormat::Standard, &ProfileHere::Unasked), "Unrated: a game hosted from this machine is never rated.");
        assert_eq!(lobby_rating_line(false, false, NsgFormat::Standard, &known), "Unrated: nothing in this lobby is rated.");
        assert_eq!(lobby_rating_line(true, false, NsgFormat::Standard, &ProfileHere::NoKey), "Unrated for you: this client has no key, so nothing here counts.");
        assert_eq!(lobby_rating_line(true, false, NsgFormat::Standard, &ProfileHere::Unkept), "Rated here. No rated games here yet.");
        assert_eq!(lobby_rating_line(true, false, NsgFormat::Standard, &known), "Rated here. In Standard: As Runner: 1580 ± 200 (2–0–1)");
        assert_eq!(lobby_rating_line(true, false, NsgFormat::Startup, &known), "Rated here. No rated games in Startup yet.");
        assert_eq!(ProfileHere::from_reply(None, None), ProfileHere::NoKey);
        assert_eq!(ProfileHere::from_reply(Some(Identity::from_secret([7; 32]).public_key()), None), ProfileHere::Unkept);
        assert_eq!(ProfileHere::from_reply(Some(Identity::from_secret([7; 32]).public_key()), Some(profile.clone())), known);
        assert_eq!(
            profile_lines(Some(&profile)),
            vec!["Known there as ann", "Wearing the title Gauntlet Runner", "Titles earned: Gauntlet Runner", "Standard · As Runner: 1580 ± 200 (2–0–1)"]
        );
        assert_eq!(profile_lines(None), vec!["Nothing kept there yet"]);
    }
}
