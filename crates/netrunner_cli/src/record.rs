//! The command-line face of `netrunner_client::record`: the same log and
//! the same suggestion, reached through `Config`. What lives here is only
//! the reading of flags — `--player`, `--record-file` — so that the record
//! a game leaves is the same whether the terminal or the desktop client
//! played it.

use netrunner_bots::{Level, Personality};
use netrunner_core::rules::Side;
pub use netrunner_client::record::*;

use crate::config::{BotKind, Config};

/// The player name a local game is recorded under: `--player` or the
/// settings file, else the login name.
pub fn player_name(config: &Config) -> String {
    netrunner_client::record::player_name(config.player.as_deref())
}

/// The file is loaded now, so a corrupt one fails before the game rather
/// than after it. There was an `--unrated` that skipped this; a game
/// against a bot is always casual now, so there is no other kind to ask for.
#[allow(clippy::too_many_arguments)]
pub fn seat_record(
    config: &Config,
    human: Side,
    level: Option<Level>,
    kind: BotKind,
    personality: Personality,
    seed: u64,
    corp_deck: &str,
    runner_deck: &str,
) -> Result<SeatRecord, String> {
    let spec = SeatRecordSpec {
        path: resolve_record_file(config.record_file.as_deref())?,
        player: player_name(config),
        human,
        level,
        kind: kind.into(),
        personality,
        seed,
        corp_deck: corp_deck.to_string(),
        runner_deck: runner_deck.to_string(),
    };
    SeatRecord::open(spec)
}

/// `netrunner_cli record`: the player's record against every opponent on
/// both chairs, and the rung to try next.
pub fn print(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    for line in standing_lines(config)? {
        println!("{line}");
    }
    Ok(())
}

/// What `record` prints, as lines — the subcommand prints them and the
/// main menu's Record screen draws them, so the two cannot drift.
pub fn standing_lines(config: &Config) -> Result<Vec<String>, String> {
    let path = resolve_record_file(config.record_file.as_deref())?;
    netrunner_client::record::standing_lines(&path, &player_name(config))
}
