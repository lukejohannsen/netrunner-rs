//! What a player sets once in the menu and expects to stay set: the name
//! they are rated under and the format their decks are checked against.
//!
//! **Settings fill in a flag the command line left unset; they never beat
//! one.** `--player` and `--format` still win, so a one-off invocation
//! behaves as it always did, and the file only replaces the defaults a
//! person would otherwise have to type every time. That is the same
//! precedence `--decks-dir` has over `NETRUNNER_DECKS_DIR`.
//!
//! **Measurements do not read this file** (`applies_to`): `bench`, `diag`
//! and `--headless` produce the numbers the roadmap quotes, and a number
//! that depends on a per-user file in the data directory is not
//! reproducible from its command line.

use std::path::{Path, PathBuf};

use clap::parser::ValueSource;
use clap::ArgMatches;
use serde::{Deserialize, Serialize};

use crate::config::{Command, Config, FormatArg};

/// Environment variable naming the settings file, for tests and for a
/// player keeping two setups apart.
pub const SETTINGS_FILE_ENV: &str = "NETRUNNER_SETTINGS_FILE";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// The name local games are rated under — `--player`'s default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<String>,
    /// The format decks are checked against — `--format`'s default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<FormatArg>,
}

/// `$NETRUNNER_SETTINGS_FILE`, else `<data dir>/netrunner/settings.json`,
/// beside the saved decks and the rating book. Not created until the first
/// save.
pub fn resolve_settings_file() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(SETTINGS_FILE_ENV).filter(|value| !value.is_empty()) {
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

    /// Fills each setting into `config` unless the command line named that
    /// flag. `flagged` answers "was this flag typed?" — `was_flagged` over
    /// the real `ArgMatches`, or a stub in tests.
    pub fn apply(&self, config: &mut Config, flagged: impl Fn(&str) -> bool) {
        if let Some(player) = &self.player
            && !flagged("player")
        {
            config.player = Some(player.clone());
        }
        if let Some(format) = self.format
            && !flagged("format")
        {
            config.format = format;
        }
    }
}

/// Whether the invocation is one a person plays, rather than one that
/// measures something. See the module comment.
pub fn applies_to(config: &Config) -> bool {
    match &config.command {
        None => !config.headless,
        Some(Command::Bench { .. } | Command::Diag { .. }) => false,
        Some(_) => true,
    }
}

/// Whether `id` was given on the command line, at the top level or after
/// any subcommand — `--player` and `--format` are global, so
/// `netrunner_cli ratings --player x` records it on the subcommand's
/// matches rather than the top level's.
pub fn was_flagged(matches: &ArgMatches, id: &str) -> bool {
    let mut current = Some(matches);
    while let Some(level) = current {
        if level.try_contains_id(id).unwrap_or(false) && level.value_source(id) == Some(ValueSource::CommandLine) {
            return true;
        }
        current = level.subcommand().map(|(_, sub)| sub);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    fn matches(args: &[&str]) -> ArgMatches {
        Config::command().try_get_matches_from(args).unwrap()
    }

    #[test]
    fn a_setting_fills_an_unset_flag_and_never_beats_a_typed_one() {
        let settings = Settings { player: Some("case".to_string()), format: Some(FormatArg::Eternal) };

        let args = ["netrunner_cli"];
        let mut config = Config::try_parse_from(args).unwrap();
        settings.apply(&mut config, |id| was_flagged(&matches(&args), id));
        assert_eq!(config.player.as_deref(), Some("case"));
        assert_eq!(config.format, FormatArg::Eternal);

        let args = ["netrunner_cli", "--player", "molly", "--format", "standard"];
        let mut config = Config::try_parse_from(args).unwrap();
        settings.apply(&mut config, |id| was_flagged(&matches(&args), id));
        assert_eq!(config.player.as_deref(), Some("molly"));
        assert_eq!(config.format, FormatArg::Standard);
    }

    #[test]
    fn a_global_flag_after_a_subcommand_counts_as_typed() {
        let args = ["netrunner_cli", "ratings", "--player", "molly"];
        assert!(was_flagged(&matches(&args), "player"));
        assert!(!was_flagged(&matches(&args), "format"));
        let args = ["netrunner_cli", "deck", "list", "--format", "eternal"];
        assert!(was_flagged(&matches(&args), "format"));
    }

    #[test]
    fn measurements_ignore_the_file() {
        let parse = |args: &[&str]| Config::try_parse_from(args).unwrap();
        assert!(applies_to(&parse(&["netrunner_cli"])));
        assert!(applies_to(&parse(&["netrunner_cli", "ratings"])));
        assert!(!applies_to(&parse(&["netrunner_cli", "--headless"])));
        assert!(!applies_to(&parse(&["netrunner_cli", "bench"])));
    }

    #[test]
    fn settings_round_trip_through_the_file_and_a_missing_file_is_the_defaults() {
        let dir = std::env::temp_dir().join(format!("netrunner_settings_{}_{:?}", std::process::id(), std::thread::current().id()));
        let path = dir.join("settings.json");
        assert_eq!(Settings::load(&path).unwrap(), Settings::default());
        let settings = Settings { player: Some("case".to_string()), format: Some(FormatArg::Standard) };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
        assert!(std::fs::read_to_string(&path).unwrap().contains("\"standard\""), "formats are stored by their flag spelling");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
