//! A rating for local play: every game a person plays against a bot in
//! this process is recorded on `netrunner_rating`'s `HumanVsBot` track,
//! and the ladder tells them which rung to try next.
//!
//! **Why a file of its own.** `netrunner_rating` performs no I/O and says
//! so: whoever owns a file owns reading and writing it. The daemon keeps
//! one (`--ratings-file`) and `bench` keeps one; local play kept none,
//! which meant the track built for "a player's progress through the
//! difficulty tiers" was reachable only over a WebSocket. This module is
//! the third owner. The file lives beside the saved decks under the OS
//! data directory (`deck_store::resolve_decks_dir` gives the reasoning),
//! because a rating is content a player would expect to keep.
//!
//! **The file holds the book and a game log, not the book alone.** A
//! `RatingBook` keeps one record per participant and role — wins and
//! losses in total — and cannot answer "how have I done against
//! `veteran`?", which is the one question a player climbing a ladder
//! asks. The log is what answers it, and what `suggest` reads to find the
//! highest rung a player has actually faced.
//!
//! **A bot is rated under one id per rung** (`Level::rating_id`,
//! `bot:veteran`), not per style: the rung is the strength claim, and a
//! player's rating against it should span the styles it plays. A bot
//! seated by kind rather than rung is rated the way the daemon rates it
//! (`bot:heuristic`, `bot:mcts:glacier`), so the two files agree.
//!
//! **A quit is a forfeit, once the game is under way.** The daemon and
//! the crate both treat a surrender as a loss; a local player who could
//! quit out of a losing game unrated would have a ladder that only ever
//! climbs. Until both sides have had a turn nothing is recorded — a mis-seated
//! game, a wrong deck, a change of mind at the mulligan cost nothing —
//! and a stall (`SessionStep::Stalled`) is never rated, since nobody won.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use netrunner_bots::{Level, Personality};
use netrunner_core::rules::Side;
use netrunner_rating::{Outcome, Rating, RatingBook, Role, Track};

use crate::config::{BotKind, Config};

/// Overrides the default location; `--ratings-file` outranks it.
pub const RATINGS_FILE_ENV: &str = "NETRUNNER_RATINGS_FILE";

/// The player name when `--player` is not given: the login name, since a
/// local file is one person's, and a fixed word when there is none so the
/// file is still readable.
pub const DEFAULT_PLAYER: &str = "player";

/// `suggest`'s rule: the win share over the last `SUGGEST_WINDOW` games
/// against the rung being played, once there are `SUGGEST_MIN_GAMES` of
/// them. Above `UP` the rung is beaten often enough to be boring, below
/// `DOWN` it is losing often enough to teach nothing, and between the two
/// it is the rung a player "can nearly beat", which is the whole point of
/// a ladder.
///
/// **A record, not the Glicko-2 expected score, and that was measured.**
/// The first version read `Glicko2::expected_score` between the player
/// and the rung's bot as both stood on the track. Between two
/// participants who only ever play each other, both starting at 1500 ±
/// 350, the last game dominates: three wins then a loss reads 0.568, two
/// wins then two losses 0.261, a win then a loss 0.356 — every sequence
/// ending in a loss pointed down and every one ending in a win pointed
/// up. A window of results is what a person would count, and it cannot
/// flip on one game.
pub const SUGGEST_UP: f64 = 0.6;
pub const SUGGEST_DOWN: f64 = 0.4;
pub const SUGGEST_WINDOW: usize = 5;
pub const SUGGEST_MIN_GAMES: usize = 3;

/// The first turn on which a quit counts as a forfeit. `GameState::turn`
/// counts each side's turn — 1 is the Corp's first, 2 the Runner's first
/// — so 3 is the first turn after both have played once. A player who
/// quits before that has decided not to play this game, not to concede
/// it.
pub const FORFEIT_FROM_TURN: u32 = 3;

pub fn resolve_ratings_file(flag: Option<&Path>) -> Result<PathBuf, String> {
    resolve_ratings_file_with(flag, std::env::var_os(RATINGS_FILE_ENV))
}

/// Flag, then environment, then the OS data directory — the same
/// precedence and the same split as `deck_store::resolve_decks_dir_with`,
/// for the same reason (a test must not set the process environment).
fn resolve_ratings_file_with(flag: Option<&Path>, env: Option<std::ffi::OsString>) -> Result<PathBuf, String> {
    if let Some(path) = flag {
        return Ok(path.to_path_buf());
    }
    if let Some(path) = env.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    dirs::data_dir()
        .map(|base| base.join("netrunner").join("ratings.json"))
        .ok_or_else(|| format!("no OS data directory is available; set {RATINGS_FILE_ENV} or pass --ratings-file"))
}

/// The player name a local game is rated under.
pub fn player_name(config: &Config) -> String {
    config
        .player
        .clone()
        .or_else(|| std::env::var("USER").ok().filter(|name| !name.is_empty()))
        .unwrap_or_else(|| DEFAULT_PLAYER.to_string())
}

/// The id a bot seat is rated under. A rung outranks the kind, because the
/// rung is what the player asked for and what the calibration measured.
pub fn opponent_id(level: Option<Level>, kind: BotKind, personality: Personality) -> String {
    if let Some(level) = level {
        return level.rating_id();
    }
    let kind = match kind {
        BotKind::Human => "human",
        BotKind::Random => "random",
        BotKind::Heuristic => "heuristic",
        BotKind::Mcts => "mcts",
        BotKind::Puct => "puct",
        BotKind::PuctOnnx => "puct-onnx",
        BotKind::Onnx => "onnx",
    };
    match personality {
        Personality::Balanced => format!("bot:{kind}"),
        personality => format!("bot:{kind}:{personality}"),
    }
}

/// One rated game, as the log keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameRecord {
    pub player: String,
    /// The chair the player sat in.
    pub side: Side,
    /// The bot's rating id.
    pub opponent: String,
    pub outcome: Outcome,
    pub seed: u64,
    pub corp_deck: String,
    pub runner_deck: String,
    /// Unix seconds; a log is read in order, and this is the order.
    pub recorded_at: u64,
}

/// Everything the file holds. `book` is the same `RatingBook` the daemon
/// and `bench` write, so its `HumanVsBot` track could be merged with a
/// daemon's by hand; the log is this module's own.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LocalRatings {
    #[serde(default)]
    pub book: RatingBook,
    #[serde(default)]
    pub games: Vec<GameRecord>,
}

/// What one recorded game did to the player, for the game-over modal.
#[derive(Debug, Clone, PartialEq)]
pub struct RatingReport {
    pub player: String,
    pub side: Side,
    pub opponent: String,
    pub before: Rating,
    pub after: Rating,
    /// Wins, draws, losses against this opponent on this chair, the
    /// game just recorded included.
    pub record: (u32, u32, u32),
    /// The rung the player is now pointed at, and the one just played if
    /// the opponent was a rung.
    pub suggested: Level,
    pub played: Option<Level>,
}

impl RatingReport {
    /// The lines the game-over modal appends: the rating, the record and,
    /// when it differs from what was just played, the rung to try next.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!(
                "Rating ({:?}): {:.0} ± {:.0} (was {:.0})",
                self.side, self.after.rating, self.after.deviation, self.before.rating
            ),
            format!("vs {}: {}–{}–{}", short_opponent(&self.opponent), self.record.0, self.record.1, self.record.2),
        ];
        match self.played {
            Some(played) if played == self.suggested => {}
            _ => lines.push(format!("Next: try {} ({})", self.suggested.name(), self.suggested.rung())),
        }
        lines
    }
}

/// `bot:veteran` → `veteran`; `bot:mcts:glacier` → `mcts:glacier`.
fn short_opponent(id: &str) -> &str {
    id.strip_prefix("bot:").unwrap_or(id)
}

impl LocalRatings {
    /// A missing file is an empty ladder, not an error — the first game
    /// creates it.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let json = std::fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        serde_json::from_str(&json).map_err(|e| format!("{} is not a ratings file: {e}", path.display()))
    }

    /// Written whole to a sibling and renamed over the original, so a
    /// crash mid-write leaves the last good file rather than half of one.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(self).expect("LocalRatings serializes");
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))
    }

    /// Records one finished game and reports what it did.
    pub fn record(&mut self, game: GameRecord) -> RatingReport {
        let role = role_of(game.side);
        let before = self.book.standing(Track::HumanVsBot, &game.player).unwrap_or_default().role(role).rating;
        let (corp, runner) = match game.side {
            Side::Corp => (game.player.as_str(), game.opponent.as_str()),
            Side::Runner => (game.opponent.as_str(), game.player.as_str()),
        };
        self.book.record(Track::HumanVsBot, corp, runner, game.outcome);
        let after = self.book.standing(Track::HumanVsBot, &game.player).expect("just recorded").role(role).rating;
        self.games.push(game.clone());
        RatingReport {
            record: self.record_against(&game.player, game.side, &game.opponent),
            suggested: self.suggest(&game.player, game.side),
            played: level_of(&game.opponent),
            player: game.player,
            side: game.side,
            opponent: game.opponent,
            before,
            after,
        }
    }

    /// Wins, draws, losses for `player` in `side`'s chair against one
    /// opponent id, from the log.
    pub fn record_against(&self, player: &str, side: Side, opponent: &str) -> (u32, u32, u32) {
        let mut tally = (0, 0, 0);
        for game in self.games.iter().filter(|g| g.player == player && g.side == side && g.opponent == opponent) {
            match player_score(game) {
                Some(true) => tally.0 += 1,
                None => tally.1 += 1,
                Some(false) => tally.2 += 1,
            }
        }
        tally
    }

    /// The rung `player` should try next in `side`'s chair.
    ///
    /// Read off the *highest rung they have played* there, never off a
    /// rung they have not — a rule that consulted unplayed rungs sent a
    /// player who lost once at `operator` straight to `novice`. With
    /// nothing played the answer is the middle rung — one ply, played
    /// straight — because it is the cheapest game that tells the ladder
    /// something. From there the last `SUGGEST_WINDOW` results against
    /// that rung say up (`SUGGEST_UP`), down (`SUGGEST_DOWN`) or stay, one
    /// rung at a time, clamped to the ladder, and say nothing until there
    /// are `SUGGEST_MIN_GAMES` of them.
    pub fn suggest(&self, player: &str, side: Side) -> Level {
        let mine = || self.games.iter().filter(|g| g.player == player && g.side == side);
        let Some(current) = mine().filter_map(|g| level_of(&g.opponent)).max() else {
            return Level::Operator;
        };
        let recent: Vec<Option<bool>> =
            mine().filter(|g| level_of(&g.opponent) == Some(current)).map(player_score).collect();
        let window = &recent[recent.len().saturating_sub(SUGGEST_WINDOW)..];
        if window.len() < SUGGEST_MIN_GAMES {
            return current;
        }
        let share = window.iter().filter(|score| **score == Some(true)).count() as f64 / window.len() as f64;
        let index = Level::ALL.iter().position(|level| *level == current).expect("a rung is in ALL");
        let next = if share >= SUGGEST_UP {
            (index + 1).min(Level::ALL.len() - 1)
        } else if share <= SUGGEST_DOWN {
            index.saturating_sub(1)
        } else {
            index
        };
        Level::ALL[next]
    }
}

/// `Some(true)` for a win, `Some(false)` for a loss, `None` for a draw,
/// from the player's chair.
fn player_score(game: &GameRecord) -> Option<bool> {
    match (game.outcome, game.side) {
        (Outcome::Draw, _) => None,
        (Outcome::CorpWin, Side::Corp) | (Outcome::RunnerWin, Side::Runner) => Some(true),
        _ => Some(false),
    }
}

fn role_of(side: Side) -> Role {
    match side {
        Side::Corp => Role::Corp,
        Side::Runner => Role::Runner,
    }
}

/// The rung a rating id names, if it names one.
fn level_of(opponent: &str) -> Option<Level> {
    opponent.strip_prefix("bot:")?.parse().ok()
}

/// What a quit records: nothing before `FORFEIT_FROM_TURN`, a loss for the
/// human from then on.
pub fn quit_outcome(turn: u32, human: Side) -> Option<Outcome> {
    if turn < FORFEIT_FROM_TURN {
        return None;
    }
    Some(match human {
        Side::Corp => Outcome::RunnerWin,
        Side::Runner => Outcome::CorpWin,
    })
}

/// The winner as an `Outcome`.
pub fn outcome_of(winner: Side) -> Outcome {
    match winner {
        Side::Corp => Outcome::CorpWin,
        Side::Runner => Outcome::RunnerWin,
    }
}

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// One local game's rating context: where the file is, who is playing
/// whom, and everything the log entry needs. Built before the game and
/// consumed by `finish` once, so a game cannot be recorded twice.
pub struct SeatRating {
    pub path: PathBuf,
    pub ratings: LocalRatings,
    pub player: String,
    pub side: Side,
    pub opponent: String,
    pub seed: u64,
    pub corp_deck: String,
    pub runner_deck: String,
}

impl SeatRating {
    /// `None` when `--unrated`; otherwise the file is loaded now, so a
    /// corrupt one fails before the game rather than after it.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        config: &Config,
        human: Side,
        level: Option<Level>,
        kind: BotKind,
        personality: Personality,
        seed: u64,
        corp_deck: &str,
        runner_deck: &str,
    ) -> Result<Option<Self>, String> {
        if config.unrated {
            return Ok(None);
        }
        let path = resolve_ratings_file(config.ratings_file.as_deref())?;
        let ratings = LocalRatings::load(&path)?;
        Ok(Some(Self {
            path,
            ratings,
            player: player_name(config),
            side: human,
            opponent: opponent_id(level, kind, personality),
            seed,
            corp_deck: corp_deck.to_string(),
            runner_deck: runner_deck.to_string(),
        }))
    }

    pub fn finish(mut self, outcome: Outcome) -> Result<RatingReport, String> {
        let report = self.ratings.record(GameRecord {
            player: self.player,
            side: self.side,
            opponent: self.opponent,
            outcome,
            seed: self.seed,
            corp_deck: self.corp_deck,
            runner_deck: self.runner_deck,
            recorded_at: now_unix(),
        });
        self.ratings.save(&self.path)?;
        Ok(report)
    }
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
    let ratings = LocalRatings::load(&path)?;
    let player = player_name(config);
    let mut lines = vec![format!("{} — {}", player, path.display())];
    let Some(standing) = ratings.book.standing(Track::HumanVsBot, &player) else {
        lines.push("no rated games yet; play one against the computer (or --runner-level 3 / --corp-level) to start".to_string());
        return Ok(lines);
    };
    for side in [Side::Corp, Side::Runner] {
        let record = standing.role(role_of(side));
        lines.push(String::new());
        lines.push(format!(
            "As {side:?}: {:.0} ± {:.0}  ({}–{}–{})",
            record.rating.rating, record.rating.deviation, record.wins, record.draws, record.losses
        ));
        let mut opponents: Vec<&str> =
            ratings.games.iter().filter(|g| g.player == player && g.side == side).map(|g| g.opponent.as_str()).collect();
        opponents.sort_unstable();
        opponents.dedup();
        for opponent in opponents {
            let (w, d, l) = ratings.record_against(&player, side, opponent);
            lines.push(format!("  vs {:<16} {w}–{d}–{l}", short_opponent(opponent)));
        }
        let next = ratings.suggest(&player, side);
        lines.push(format!("  next: {} ({}) — {}", next.name(), next.rung(), next.spec(side).describe()));
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(player: &str, side: Side, opponent: &str, outcome: Outcome) -> GameRecord {
        GameRecord {
            player: player.to_string(),
            side,
            opponent: opponent.to_string(),
            outcome,
            seed: 1,
            corp_deck: "discretion_advised".to_string(),
            runner_deck: "stolen_goods".to_string(),
            recorded_at: 0,
        }
    }

    #[test]
    fn the_flag_outranks_the_environment_which_outranks_the_data_dir() {
        let flag = Path::new("/from/flag/r.json");
        assert_eq!(resolve_ratings_file_with(Some(flag), Some("/from/env".into())).unwrap(), PathBuf::from("/from/flag/r.json"));
        assert_eq!(resolve_ratings_file_with(None, Some("/from/env/r.json".into())).unwrap(), PathBuf::from("/from/env/r.json"));
        assert_eq!(resolve_ratings_file_with(None, Some("".into())).unwrap(), resolve_ratings_file_with(None, None).unwrap());
        assert!(resolve_ratings_file_with(None, None).unwrap().ends_with("netrunner/ratings.json"));
    }

    #[test]
    fn a_rung_is_rated_by_its_name_and_a_kind_by_the_daemons_id() {
        assert_eq!(opponent_id(Some(Level::Veteran), BotKind::Random, Personality::Rush), "bot:veteran");
        assert_eq!(opponent_id(None, BotKind::Heuristic, Personality::Balanced), "bot:heuristic");
        assert_eq!(opponent_id(None, BotKind::Mcts, Personality::Glacier), "bot:mcts:glacier");
    }

    #[test]
    fn a_game_moves_the_players_role_rating_and_the_log_keeps_the_record() {
        let mut ratings = LocalRatings::default();
        let report = ratings.record(game("luke", Side::Runner, "bot:apprentice", Outcome::RunnerWin));
        assert_eq!(report.before, Rating::default());
        assert!(report.after.rating > 1500.0);
        assert_eq!(report.record, (1, 0, 0));
        assert_eq!(report.played, Some(Level::Apprentice));
        let standing = ratings.book.standing(Track::HumanVsBot, "luke").unwrap();
        assert_eq!(standing.corp.rating, Rating::default(), "the Corp chair did not play");
        assert!(ratings.book.standing(Track::HumanVsBot, "bot:apprentice").unwrap().corp.rating.rating < 1500.0);
        ratings.record(game("luke", Side::Runner, "bot:apprentice", Outcome::CorpWin));
        assert_eq!(ratings.record_against("luke", Side::Runner, "bot:apprentice"), (1, 0, 1));
        assert_eq!(ratings.record_against("luke", Side::Corp, "bot:apprentice"), (0, 0, 0), "chairs are separate");
    }

    #[test]
    fn the_suggestion_starts_in_the_middle_and_moves_one_rung_on_the_recent_record() {
        let mut ratings = LocalRatings::default();
        assert_eq!(ratings.suggest("luke", Side::Corp), Level::Operator, "nothing played yet");
        // Two wins at apprentice are not yet a record; the third makes
        // one, and the ladder points one rung up, not two.
        ratings.record(game("luke", Side::Corp, "bot:apprentice", Outcome::CorpWin));
        ratings.record(game("luke", Side::Corp, "bot:apprentice", Outcome::CorpWin));
        assert_eq!(ratings.suggest("luke", Side::Corp), Level::Apprentice, "fewer than SUGGEST_MIN_GAMES");
        ratings.record(game("luke", Side::Corp, "bot:apprentice", Outcome::CorpWin));
        assert_eq!(ratings.suggest("luke", Side::Corp), Level::Operator);
        // Only the window counts: five losses after those wins point down.
        for _ in 0..5 {
            ratings.record(game("luke", Side::Corp, "bot:apprentice", Outcome::RunnerWin));
        }
        assert_eq!(ratings.suggest("luke", Side::Corp), Level::Novice);
        // Three losses at elite point back down one rung, and the rung
        // played is what the suggestion is read from, not the best.
        for _ in 0..3 {
            ratings.record(game("luke", Side::Corp, "bot:elite", Outcome::RunnerWin));
        }
        assert_eq!(ratings.suggest("luke", Side::Corp), Level::Veteran);
        // The Runner chair has its own answer.
        assert_eq!(ratings.suggest("luke", Side::Runner), Level::Operator);
    }

    #[test]
    fn the_suggestion_is_clamped_to_the_ladder() {
        let mut ratings = LocalRatings::default();
        for _ in 0..5 {
            ratings.record(game("a", Side::Runner, "bot:elite", Outcome::RunnerWin));
            ratings.record(game("b", Side::Runner, "bot:novice", Outcome::CorpWin));
        }
        assert_eq!(ratings.suggest("a", Side::Runner), Level::Elite);
        assert_eq!(ratings.suggest("b", Side::Runner), Level::Novice);
    }

    #[test]
    fn a_quit_is_a_forfeit_from_the_second_turn() {
        assert_eq!(quit_outcome(0, Side::Corp), None);
        assert_eq!(quit_outcome(1, Side::Corp), None, "the Corp's first turn");
        assert_eq!(quit_outcome(2, Side::Corp), None, "the Runner's first turn");
        assert_eq!(quit_outcome(3, Side::Corp), Some(Outcome::RunnerWin));
        assert_eq!(quit_outcome(7, Side::Runner), Some(Outcome::CorpWin));
    }

    #[test]
    fn the_report_names_the_next_rung_only_when_it_differs() {
        let mut ratings = LocalRatings::default();
        ratings.record(game("luke", Side::Corp, "bot:operator", Outcome::RunnerWin));
        ratings.record(game("luke", Side::Corp, "bot:operator", Outcome::RunnerWin));
        let report = ratings.record(game("luke", Side::Corp, "bot:operator", Outcome::RunnerWin));
        assert_eq!(report.suggested, Level::Apprentice);
        let lines = report.lines();
        assert!(lines[0].starts_with("Rating (Corp):"), "{lines:?}");
        assert_eq!(lines[1], "vs operator: 0–0–3");
        assert_eq!(lines[2], "Next: try apprentice (2)");
        let mut same = LocalRatings::default();
        for outcome in [Outcome::CorpWin, Outcome::RunnerWin, Outcome::CorpWin] {
            same.record(game("luke", Side::Corp, "bot:operator", outcome));
        }
        let report = same.record(game("luke", Side::Corp, "bot:operator", Outcome::RunnerWin));
        assert_eq!(report.suggested, Level::Operator, "2 of 4 is inside the band");
        assert_eq!(report.lines().len(), 2, "no 'next' line when the rung played is the one suggested");
    }

    #[test]
    fn the_file_round_trips_and_a_missing_one_is_empty() {
        let dir = std::env::temp_dir().join(format!("netrunner_ratings_{}_{}", std::process::id(), now_unix()));
        let path = dir.join("nested").join("ratings.json");
        assert_eq!(LocalRatings::load(&path).unwrap(), LocalRatings::default());
        let mut ratings = LocalRatings::default();
        ratings.record(game("luke", Side::Runner, "bot:novice", Outcome::RunnerWin));
        ratings.save(&path).unwrap();
        assert_eq!(LocalRatings::load(&path).unwrap(), ratings);
        assert!(!path.with_extension("json.tmp").exists(), "the temp file is renamed away");
        std::fs::write(&path, "not json").unwrap();
        assert!(LocalRatings::load(&path).unwrap_err().contains("not a ratings file"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
