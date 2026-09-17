//! What a player sets once and expects to stay set: the name they are
//! rated under, the format their decks are checked against, and the
//! desktop client's preferences.
//!
//! **One file, one struct, every client.** The file has no
//! `deny_unknown_fields` — a hand-edited file with a stray key should be
//! read, not refused — which means a client that loads it into a struct
//! missing a field drops that field on its next save. So the terminal and
//! the desktop client both use *this* struct: the terminal never reads
//! `desktop`, but it round-trips it, and a name typed in either client
//! rates the next game in the other.
//!
//! **The format keeps its flag spelling on disk** (`"startup"`, not
//! `"Startup"`): the file predates this crate and was written with the
//! CLI's `FormatArg`, whose serde form is lowercase. `NsgFormat`'s own
//! serde form is the variant name, and changing the engine's
//! serialization for a client file's sake would be the wrong direction,
//! so the field carries its own (de)serializer.
//!
//! The CLI keeps the half that is about *flags* — a setting fills a flag
//! the command line left unset and never beats a typed one, and
//! measurements never read the file — because that needs `clap`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use netrunner_core::format::NsgFormat;

/// Environment variable naming the settings file, for tests and for a
/// player keeping two setups apart.
pub const SETTINGS_FILE_ENV: &str = "NETRUNNER_SETTINGS_FILE";

/// Every format, in the order a settings screen cycles them.
pub const FORMATS: [NsgFormat; 4] = [NsgFormat::Startup, NsgFormat::Standard, NsgFormat::Eternal, NsgFormat::Snapshot];

/// The lowercase name a format is stored and shown under — the CLI's
/// `--format` value.
pub fn format_name(format: NsgFormat) -> &'static str {
    match format {
        NsgFormat::Startup => "startup",
        NsgFormat::Standard => "standard",
        NsgFormat::Eternal => "eternal",
        NsgFormat::Snapshot => "snapshot",
    }
}

/// The name a format is shown under in a list: [`format_name`] with a
/// capital, so a screen never spells it twice.
pub fn format_label(format: NsgFormat) -> &'static str {
    match format {
        NsgFormat::Startup => "Startup",
        NsgFormat::Standard => "Standard",
        NsgFormat::Eternal => "Eternal",
        NsgFormat::Snapshot => "Snapshot",
    }
}

/// The inverse of [`format_name`], case-insensitive.
pub fn parse_format(name: &str) -> Option<NsgFormat> {
    FORMATS.into_iter().find(|format| format_name(*format).eq_ignore_ascii_case(name))
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// The name local games are rated under — `--player`'s default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<String>,
    /// The format decks are checked against — `--format`'s default.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "format_by_name")]
    pub format: Option<NsgFormat>,
    /// The desktop client's preferences. Skipped while untouched so a
    /// terminal-only player's file stays two fields long.
    #[serde(default, skip_serializing_if = "DesktopPrefs::is_default")]
    pub desktop: DesktopPrefs,
}

/// Preferences only the graphical client reads. `#[serde(default)]` on
/// the struct means a file from before any given field existed still
/// loads, with that field at its default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopPrefs {
    /// A multiplier on every animation's duration: 1 is the authored
    /// pace, 2 twice as fast, 0 instant.
    pub animation_speed: f32,
    /// 0 to 1.
    pub sfx_volume: f32,
    /// 0 to 1. The music tier is file-only, so this is silent until a
    /// track is dropped in.
    pub music_volume: f32,
    /// Whether the client may fetch card images from NetrunnerDB into
    /// the cache. Off until the player turns it on: it is a network call
    /// on their behalf.
    pub download_images: bool,
    /// The window size last saved, if the player resized it.
    pub window_size: Option<(u32, u32)>,
    /// Whether the board lists every legal action on a flat panel — the
    /// "play helper". Off by default: the board is played from the cards,
    /// the zones and the control bar, and the first people to play read
    /// the panel as a help menu. Every legal action is reachable without
    /// it; it is an aid, not the contract.
    pub play_helper: bool,
    /// Whether the board shows the match log — the "play history". Off by
    /// default for the same reason: the transitions and the prompt say
    /// what happened, and the log is for reading back.
    pub play_history: bool,
    /// Whether the board shows the phase bar: the turn's steps, and a
    /// run's, with the one in play marked. On by default — it says where
    /// the game is, which a person cannot work out from the cards — and
    /// off for whoever knows the turn by heart and wants the row of board
    /// height back (`L`, or the game options).
    pub phase_bar: bool,
}

impl Default for DesktopPrefs {
    fn default() -> Self {
        Self { animation_speed: 1.0, sfx_volume: 0.8, music_volume: 0.5, download_images: false, window_size: None, play_helper: false, play_history: false, phase_bar: true }
    }
}

impl DesktopPrefs {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// `$NETRUNNER_SETTINGS_FILE`, else `<data dir>/netrunner/settings.json`,
/// beside the saved decks and the rating book. Not created until the first
/// save.
pub fn resolve_settings_file() -> Result<PathBuf, String> {
    resolve_settings_file_with(std::env::var_os(SETTINGS_FILE_ENV))
}

/// The precedence with the environment handed in, so it can be tested
/// without touching process state (the reasoning is
/// `deck_store::resolve_decks_dir_with`'s).
fn resolve_settings_file_with(env: Option<std::ffi::OsString>) -> Result<PathBuf, String> {
    if let Some(path) = env.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    dirs::data_dir()
        .map(|base| base.join("netrunner").join("settings.json"))
        .ok_or_else(|| format!("no OS data directory is available; set {SETTINGS_FILE_ENV}"))
}

impl Settings {
    /// A missing file is the defaults, not an error.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let json = std::fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        serde_json::from_str(&json).map_err(|e| format!("{} is not a settings file: {e}", path.display()))
    }

    /// Temp file and rename, like the rating book and the deck store.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(self).expect("Settings serializes");
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))
    }
}

/// The format's flag spelling on disk (see the module comment).
mod format_by_name {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use netrunner_core::format::NsgFormat;

    pub fn serialize<S: Serializer>(format: &Option<NsgFormat>, serializer: S) -> Result<S::Ok, S::Error> {
        format.map(super::format_name).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<NsgFormat>, D::Error> {
        let name: Option<String> = Option::deserialize(deserializer)?;
        name.map(|name| super::parse_format(&name).ok_or_else(|| serde::de::Error::custom(format!("unknown format {name:?}")))).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("netrunner_settings_{tag}_{}_{:?}", std::process::id(), std::thread::current().id()));
        (dir.clone(), dir.join("settings.json"))
    }

    #[test]
    fn settings_round_trip_through_the_file_and_a_missing_file_is_the_defaults() {
        let (dir, path) = temp_path("round_trip");
        assert_eq!(Settings::load(&path).unwrap(), Settings::default());
        let settings = Settings { player: Some("case".to_string()), format: Some(NsgFormat::Standard), ..Default::default() };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains("\"standard\""), "formats are stored by their flag spelling: {json}");
        assert!(!json.contains("desktop"), "untouched desktop preferences are not written");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The reason this struct is shared: a file written by the desktop,
    /// loaded and saved by a client that never reads `desktop`, keeps it.
    #[test]
    fn settings_desktop_block_survives_a_save_by_a_client_that_ignores_it() {
        let (dir, path) = temp_path("desktop_block");
        let desktop = DesktopPrefs { animation_speed: 2.0, download_images: true, ..Default::default() };
        Settings { player: Some("case".to_string()), format: None, desktop: desktop.clone() }.save(&path).unwrap();
        let mut reloaded = Settings::load(&path).unwrap();
        reloaded.player = Some("molly".to_string());
        reloaded.save(&path).unwrap();
        let again = Settings::load(&path).unwrap();
        assert_eq!(again.desktop, desktop);
        assert_eq!(again.player.as_deref(), Some("molly"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_from_before_a_preference_existed_still_loads() {
        let settings: Settings = serde_json::from_str(r#"{"player":"case","format":"eternal","desktop":{"sfx_volume":0.1}}"#).unwrap();
        assert_eq!(settings.format, Some(NsgFormat::Eternal));
        assert_eq!(settings.desktop.sfx_volume, 0.1);
        assert_eq!(settings.desktop.animation_speed, 1.0, "unnamed fields take their defaults");
        assert!(!settings.desktop.play_helper && !settings.desktop.play_history, "the board's aids are off until turned on");
        assert!(serde_json::from_str::<Settings>(r#"{"format":"modern"}"#).is_err(), "an unknown format is an error, not a default");
    }

    #[test]
    fn every_format_has_a_name_that_parses_back() {
        for format in FORMATS {
            assert_eq!(parse_format(format_name(format)), Some(format));
            assert_eq!(parse_format(&format_name(format).to_uppercase()), Some(format));
        }
        assert_eq!(parse_format("modern"), None);
    }

    #[test]
    fn the_environment_names_the_file_and_an_empty_value_falls_through() {
        assert_eq!(resolve_settings_file_with(Some("/from/env/s.json".into())).unwrap(), PathBuf::from("/from/env/s.json"));
        assert!(resolve_settings_file_with(Some("".into())).unwrap().ends_with("netrunner/settings.json"));
        assert!(resolve_settings_file_with(None).unwrap().ends_with("netrunner/settings.json"));
    }
}
