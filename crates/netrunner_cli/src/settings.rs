//! The command-line half of the settings file. The file and its struct
//! are `netrunner_client::settings`, shared with the desktop client so a
//! name typed in either rates the next game in the other; what stays here
//! is everything that needs `clap`.
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

use clap::parser::ValueSource;
use clap::ArgMatches;

pub use netrunner_client::settings::{resolve_settings_file, Settings};
use netrunner_client::standing::{Answer, Answers, PromptKey};

use crate::config::{Command, Config};

/// Fills each setting into `config` unless the command line named that
/// flag. `flagged` answers "was this flag typed?" — `was_flagged` over
/// the real `ArgMatches`, or a stub in tests.
pub fn apply(settings: &Settings, config: &mut Config, flagged: impl Fn(&str) -> bool) {
    if let Some(player) = &settings.player
        && !flagged("player")
    {
        config.player = Some(player.clone());
    }
    if let Some(format) = settings.format
        && !flagged("format")
    {
        config.format = format.into();
    }
}

/// The answers a person gave a card's "you may" for good
/// (`netrunner_client::standing`); none when the file cannot be read, as
/// the rest of the settings are ignored then.
pub fn answers() -> Answers {
    resolve_settings_file().and_then(|path| Settings::load(&path)).map(|settings| settings.answers).unwrap_or_default()
}

/// Keeps `answer` for `key` in the file: read, changed and written back
/// whole, so a field only the desktop client reads is not lost.
pub fn remember(key: PromptKey, answer: Answer) -> Result<(), String> {
    let path = resolve_settings_file()?;
    let mut settings = Settings::load(&path)?;
    settings.answers.set(key, Some(answer));
    settings.save(&path)
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
/// `netrunner_cli record --player x` records it on the subcommand's
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

    use crate::config::FormatArg;
    use netrunner_core::format::NsgFormat;

    fn matches(args: &[&str]) -> ArgMatches {
        Config::command().try_get_matches_from(args).unwrap()
    }

    #[test]
    fn a_setting_fills_an_unset_flag_and_never_beats_a_typed_one() {
        let settings = Settings { player: Some("case".to_string()), format: Some(NsgFormat::Eternal), ..Default::default() };

        let args = ["netrunner_cli"];
        let mut config = Config::try_parse_from(args).unwrap();
        apply(&settings, &mut config, |id| was_flagged(&matches(&args), id));
        assert_eq!(config.player.as_deref(), Some("case"));
        assert_eq!(config.format, FormatArg::Eternal);

        let args = ["netrunner_cli", "--player", "molly", "--format", "standard"];
        let mut config = Config::try_parse_from(args).unwrap();
        apply(&settings, &mut config, |id| was_flagged(&matches(&args), id));
        assert_eq!(config.player.as_deref(), Some("molly"));
        assert_eq!(config.format, FormatArg::Standard);
    }

    #[test]
    fn a_global_flag_after_a_subcommand_counts_as_typed() {
        let args = ["netrunner_cli", "record", "--player", "molly"];
        assert!(was_flagged(&matches(&args), "player"));
        assert!(!was_flagged(&matches(&args), "format"));
        let args = ["netrunner_cli", "deck", "list", "--format", "eternal"];
        assert!(was_flagged(&matches(&args), "format"));
    }

    #[test]
    fn measurements_ignore_the_file() {
        let parse = |args: &[&str]| Config::try_parse_from(args).unwrap();
        assert!(applies_to(&parse(&["netrunner_cli"])));
        assert!(applies_to(&parse(&["netrunner_cli", "record"])));
        assert!(!applies_to(&parse(&["netrunner_cli", "--headless"])));
        assert!(!applies_to(&parse(&["netrunner_cli", "bench"])));
    }

}
