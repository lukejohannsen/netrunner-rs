//! The command-line face of `netrunner_client::ratings`: the same book,
//! the same log and the same suggestion, reached through `Config`. What
//! lives here is only the reading of flags — `--player`, `--ratings-file`,
//! `--unrated` — so that the rating a game records is the same whether the
//! terminal or the desktop client played it.

use netrunner_bots::{Level, Personality};
use netrunner_core::rules::Side;
pub use netrunner_client::ratings::*;

use crate::config::{BotKind, Config};

/// The player name a local game is rated under: `--player` or the
/// settings file, else the login name.
pub fn player_name(config: &Config) -> String {
    netrunner_client::ratings::player_name(config.player.as_deref())
}

/// `None` when `--unrated`; otherwise the file is loaded now, so a corrupt
/// one fails before the game rather than after it.
#[allow(clippy::too_many_arguments)]
pub fn seat_rating(
    config: &Config,
    human: Side,
    level: Option<Level>,
    kind: BotKind,
    personality: Personality,
    seed: u64,
    corp_deck: &str,
    runner_deck: &str,
) -> Result<Option<SeatRating>, String> {
    if config.unrated {
        return Ok(None);
    }
    let spec = SeatRatingSpec {
        path: resolve_ratings_file(config.ratings_file.as_deref())?,
        player: player_name(config),
        human,
        level,
        kind: kind.into(),
        personality,
        seed,
        corp_deck: corp_deck.to_string(),
        runner_deck: runner_deck.to_string(),
    };
    SeatRating::open(spec).map(Some)
}

/// `netrunner_cli ratings`: the player's standing on both chairs, their
/// record against every opponent, and the rung to try next.
pub fn print(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    for line in standing_lines(config)? {
        println!("{line}");
    }
    Ok(())
}

/// What `ratings` prints, as lines — the subcommand prints them and the
/// main menu's Ratings screen draws them, so the two cannot drift.
pub fn standing_lines(config: &Config) -> Result<Vec<String>, String> {
    let path = resolve_ratings_file(config.ratings_file.as_deref())?;
    netrunner_client::ratings::standing_lines(&path, &player_name(config))
}
