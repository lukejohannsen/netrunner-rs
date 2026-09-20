//! A person's record against the bots: every game they play against one
//! in this process is logged, and the log tells them which rung of the
//! ladder to try next.
//!
//! **A record, not a rating, and this module once kept both.** It used to
//! hold a Glicko-2 `RatingBook` beside the log and show the number as "your
//! rating". A rating is a claim to someone else: it means something only
//! between people, and only when somebody other than the rated player
//! keeps it. This file is the player's to edit or delete, so the number
//! certified nothing — and guarding it cost the one thing practice needs,
//! since an undo past what a move had taught ended the game's rating. So
//! local play is casual: nothing here is rated, a move can always be taken
//! back, and a rating is a server's to keep (`docs/identity-and-rating.md`).
//! What was load-bearing was never the number. `suggest` read the log from
//! its second version on (see `SUGGEST_UP`), and the log is what is left.
//!
//! **Why a file of its own.** It lives beside the saved decks under the OS
//! data directory (`deck_store::resolve_decks_dir` gives the reasoning),
//! because a record is content a player would expect to keep. It answers
//! the one question a player climbing a ladder asks — "how have I done
//! against `veteran`?" — and is what `suggest` reads to find the highest
//! rung a player has actually faced.
//!
//! **A bot is recorded under one id per rung** (`Level::record_id`,
//! `bot:veteran`), not per style: the rung is the strength claim, and a
//! player's record against it should span the styles it plays. A bot
//! seated by kind rather than rung is `bot:heuristic`, `bot:mcts:glacier`.
//!
//! **A quit is a loss, once the game is under way.** Nothing is at stake
//! but the suggestion's honesty: a record a player could quit out of would
//! only ever climb, and the rung it named would be one they cannot play.
//! Until both sides have had a turn nothing is recorded — a mis-seated
//! game, a wrong deck, a change of mind at the mulligan cost nothing —
//! and a stall (`SessionStep::Stalled`) is never recorded, since nobody
//! won. **A game with take-backs counts like any other**: the record is
//! the person's own aid, and a win with a move taken back still says the
//! rung is within reach.
//!
//! **Flag-free, deliberately.** This module once took the CLI's `Config`;
//! it now takes what it reads off it — a path, a name — so the desktop
//! client, which has no command line, records a game through the same
//! code and into the same file.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use netrunner_bots::{Level, Personality};
use netrunner_core::rules::Side;

/// How a game ended. This module's own type — it was `netrunner_rating`'s
/// while the file held a rating — with the same variant names, so a log
/// written then still reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    CorpWin,
    RunnerWin,
    Draw,
}

/// The bot kinds a seat can be recorded under, by name. A plain enum rather
/// than the CLI's `clap::ValueEnum`, because the desktop has no command
/// line; the CLI converts its own into this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotKind {
    Human,
    Random,
    Heuristic,
    Mcts,
    Puct,
    PuctOnnx,
    Onnx,
}

/// Overrides the default location; `--record-file` outranks it.
pub const RECORD_FILE_ENV: &str = "NETRUNNER_RECORD_FILE";

/// The file this one replaces, read once when there is no record yet
/// (`LocalRecord::load`).
const LEGACY_FILE_NAME: &str = "ratings.json";

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
/// and the rung's bot as both stood in the rating book this file then held. Between two
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

/// The first turn on which a quit counts as a loss. `GameState::turn`
/// counts each side's turn — 1 is the Corp's first, 2 the Runner's first
/// — so 3 is the first turn after both have played once. A player who
/// quits before that has decided not to play this game, not to concede
/// it.
pub const FORFEIT_FROM_TURN: u32 = 3;

pub fn resolve_record_file(flag: Option<&Path>) -> Result<PathBuf, String> {
    resolve_record_file_with(flag, std::env::var_os(RECORD_FILE_ENV))
}

/// Flag, then environment, then the OS data directory — the same
/// precedence and the same split as `deck_store::resolve_decks_dir_with`,
/// for the same reason (a test must not set the process environment).
fn resolve_record_file_with(flag: Option<&Path>, env: Option<std::ffi::OsString>) -> Result<PathBuf, String> {
    if let Some(path) = flag {
        return Ok(path.to_path_buf());
    }
    if let Some(path) = env.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    dirs::data_dir()
        .map(|base| base.join("netrunner").join("record.json"))
        .ok_or_else(|| format!("no OS data directory is available; set {RECORD_FILE_ENV} or pass --record-file"))
}

/// The player name a local game is recorded under: the one given (a flag or
/// the settings file), else the login name, else [`DEFAULT_PLAYER`].
pub fn player_name(given: Option<&str>) -> String {
    given
        .map(str::to_string)
        .or_else(|| std::env::var("USER").ok().filter(|name| !name.is_empty()))
        .unwrap_or_else(|| DEFAULT_PLAYER.to_string())
}

/// The id a bot seat is recorded under. A rung outranks the kind, because
/// the rung is what the player asked for and what the calibration measured.
pub fn opponent_id(level: Option<Level>, kind: BotKind, personality: Personality) -> String {
    if let Some(level) = level {
        return level.record_id();
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

/// One finished game, as the log keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameRecord {
    pub player: String,
    /// The chair the player sat in.
    pub side: Side,
    /// The bot's id (`opponent_id`).
    pub opponent: String,
    pub outcome: Outcome,
    pub seed: u64,
    pub corp_deck: String,
    pub runner_deck: String,
    /// Unix seconds; a log is read in order, and this is the order.
    pub recorded_at: u64,
}

/// Everything the file holds: the log. Deliberately not
/// `deny_unknown_fields` — the file this replaces held a `book` beside the
/// same `games`, and reading it is how a player keeps their record.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LocalRecord {
    #[serde(default)]
    pub games: Vec<GameRecord>,
}

/// Where one recorded game leaves the player, for the game-over modal.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordReport {
    pub player: String,
    pub side: Side,
    pub opponent: String,
    /// Wins, draws, losses against this opponent on this chair, the
    /// game just recorded included.
    pub record: (u32, u32, u32),
    /// The rung the player is now pointed at, and the one just played if
    /// the opponent was a rung.
    pub suggested: Level,
    pub played: Option<Level>,
}

impl RecordReport {
    /// The lines the game-over modal appends: the record and, when it
    /// differs from what was just played, the rung to try next.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "As {:?} vs {}: {}–{}–{}",
            self.side,
            short_opponent(&self.opponent),
            self.record.0,
            self.record.1,
            self.record.2
        )];
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

impl LocalRecord {
    /// A missing file is an empty record, not an error — the first game
    /// creates it.
    ///
    /// **With one exception: the `ratings.json` this file replaces.** When
    /// there is no record yet and that file sits beside where it would be,
    /// its games are the record — the rung a player had climbed to was
    /// read off that log, and a rename must not send them back to the
    /// middle. It is read, never written or removed: the first `save`
    /// writes the new file and this path is not taken again.
    pub fn load(path: &Path) -> Result<Self, String> {
        let legacy = path.with_file_name(LEGACY_FILE_NAME);
        let path = match (path.exists(), legacy.exists()) {
            (true, _) => path,
            (false, true) => legacy.as_path(),
            (false, false) => return Ok(Self::default()),
        };
        let json = std::fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        serde_json::from_str(&json).map_err(|e| format!("{} is not a record file: {e}", path.display()))
    }

    /// Written whole to a sibling and renamed over the original, so a
    /// crash mid-write leaves the last good file rather than half of one.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(self).expect("LocalRecord serializes");
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))
    }

    /// Records one finished game and reports where it leaves the player.
    pub fn record(&mut self, game: GameRecord) -> RecordReport {
        self.games.push(game.clone());
        RecordReport {
            record: self.record_against(&game.player, game.side, &game.opponent),
            suggested: self.suggest(&game.player, game.side),
            played: level_of(&game.opponent),
            player: game.player,
            side: game.side,
            opponent: game.opponent,
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

/// The rung an opponent id names, if it names one.
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

/// One local game's place in the record: where the file is, who is
/// playing whom, and everything the log entry needs. Built before the game
/// and consumed by `finish` once, so a game cannot be recorded twice.
pub struct SeatRecord {
    pub path: PathBuf,
    pub log: LocalRecord,
    pub player: String,
    pub side: Side,
    pub opponent: String,
    pub seed: u64,
    pub corp_deck: String,
    pub runner_deck: String,
}

/// Everything `SeatRecord::open` needs to know about the game about to be
/// played. A struct rather than eight arguments so the two clients that
/// build one cannot transpose the deck ids.
#[derive(Debug, Clone)]
pub struct SeatRecordSpec {
    /// The record file, from `resolve_record_file`.
    pub path: PathBuf,
    /// The name the game is recorded under, from `player_name`.
    pub player: String,
    /// The chair the player sits in.
    pub human: Side,
    pub level: Option<Level>,
    pub kind: BotKind,
    pub personality: Personality,
    pub seed: u64,
    pub corp_deck: String,
    pub runner_deck: String,
}

impl SeatRecord {
    /// The file is loaded now, so a corrupt one fails before the game
    /// rather than after it.
    pub fn open(spec: SeatRecordSpec) -> Result<Self, String> {
        let log = LocalRecord::load(&spec.path)?;
        Ok(Self {
            path: spec.path,
            log,
            player: spec.player,
            side: spec.human,
            opponent: opponent_id(spec.level, spec.kind, spec.personality),
            seed: spec.seed,
            corp_deck: spec.corp_deck,
            runner_deck: spec.runner_deck,
        })
    }

    pub fn finish(mut self, outcome: Outcome) -> Result<RecordReport, String> {
        let report = self.log.record(GameRecord {
            player: self.player,
            side: self.side,
            opponent: self.opponent,
            outcome,
            seed: self.seed,
            corp_deck: self.corp_deck,
            runner_deck: self.runner_deck,
            recorded_at: now_unix(),
        });
        self.log.save(&self.path)?;
        Ok(report)
    }
}

/// The player's record against every opponent on both chairs and the rung
/// to try next, as lines — `netrunner_cli record` prints them and both
/// clients' screens draw them, so the three cannot drift.
pub fn standing_lines(path: &Path, player: &str) -> Result<Vec<String>, String> {
    let log = LocalRecord::load(path)?;
    let mut lines = vec![format!("{} — {}", player, path.display())];
    if !log.games.iter().any(|g| g.player == player) {
        lines.push("no games yet; play one against the computer (or --runner-level 3 / --corp-level) to start".to_string());
        return Ok(lines);
    }
    for side in [Side::Corp, Side::Runner] {
        lines.push(String::new());
        lines.push(format!("As {side:?}"));
        let mut opponents: Vec<&str> =
            log.games.iter().filter(|g| g.player == player && g.side == side).map(|g| g.opponent.as_str()).collect();
        opponents.sort_unstable();
        opponents.dedup();
        for opponent in opponents {
            let (w, d, l) = log.record_against(player, side, opponent);
            lines.push(format!("  vs {:<16} {w}–{d}–{l}", short_opponent(opponent)));
        }
        let next = log.suggest(player, side);
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
        assert_eq!(resolve_record_file_with(Some(flag), Some("/from/env".into())).unwrap(), PathBuf::from("/from/flag/r.json"));
        assert_eq!(resolve_record_file_with(None, Some("/from/env/r.json".into())).unwrap(), PathBuf::from("/from/env/r.json"));
        assert_eq!(resolve_record_file_with(None, Some("".into())).unwrap(), resolve_record_file_with(None, None).unwrap());
        assert!(resolve_record_file_with(None, None).unwrap().ends_with("netrunner/record.json"));
    }

    #[test]
    fn a_rung_is_recorded_by_its_name_and_a_kind_by_its_own() {
        assert_eq!(opponent_id(Some(Level::Veteran), BotKind::Random, Personality::Rush), "bot:veteran");
        assert_eq!(opponent_id(None, BotKind::Heuristic, Personality::Balanced), "bot:heuristic");
        assert_eq!(opponent_id(None, BotKind::Mcts, Personality::Glacier), "bot:mcts:glacier");
    }

    #[test]
    fn the_log_keeps_the_record_per_chair_and_opponent() {
        let mut log = LocalRecord::default();
        let report = log.record(game("luke", Side::Runner, "bot:apprentice", Outcome::RunnerWin));
        assert_eq!(report.record, (1, 0, 0));
        assert_eq!(report.played, Some(Level::Apprentice));
        log.record(game("luke", Side::Runner, "bot:apprentice", Outcome::CorpWin));
        assert_eq!(log.record_against("luke", Side::Runner, "bot:apprentice"), (1, 0, 1));
        assert_eq!(log.record_against("luke", Side::Corp, "bot:apprentice"), (0, 0, 0), "chairs are separate");
    }

    #[test]
    fn the_suggestion_starts_in_the_middle_and_moves_one_rung_on_the_recent_record() {
        let mut log = LocalRecord::default();
        assert_eq!(log.suggest("luke", Side::Corp), Level::Operator, "nothing played yet");
        // Two wins at apprentice are not yet a record; the third makes
        // one, and the ladder points one rung up, not two.
        log.record(game("luke", Side::Corp, "bot:apprentice", Outcome::CorpWin));
        log.record(game("luke", Side::Corp, "bot:apprentice", Outcome::CorpWin));
        assert_eq!(log.suggest("luke", Side::Corp), Level::Apprentice, "fewer than SUGGEST_MIN_GAMES");
        log.record(game("luke", Side::Corp, "bot:apprentice", Outcome::CorpWin));
        assert_eq!(log.suggest("luke", Side::Corp), Level::Operator);
        // Only the window counts: five losses after those wins point down.
        for _ in 0..5 {
            log.record(game("luke", Side::Corp, "bot:apprentice", Outcome::RunnerWin));
        }
        assert_eq!(log.suggest("luke", Side::Corp), Level::Novice);
        // Three losses at elite point back down one rung, and the rung
        // played is what the suggestion is read from, not the best.
        for _ in 0..3 {
            log.record(game("luke", Side::Corp, "bot:elite", Outcome::RunnerWin));
        }
        assert_eq!(log.suggest("luke", Side::Corp), Level::Veteran);
        // The Runner chair has its own answer.
        assert_eq!(log.suggest("luke", Side::Runner), Level::Operator);
    }

    #[test]
    fn the_suggestion_is_clamped_to_the_ladder() {
        let mut log = LocalRecord::default();
        for _ in 0..5 {
            log.record(game("a", Side::Runner, "bot:elite", Outcome::RunnerWin));
            log.record(game("b", Side::Runner, "bot:novice", Outcome::CorpWin));
        }
        assert_eq!(log.suggest("a", Side::Runner), Level::Elite);
        assert_eq!(log.suggest("b", Side::Runner), Level::Novice);
    }

    #[test]
    fn a_quit_is_a_loss_once_both_sides_have_had_a_turn() {
        assert_eq!(quit_outcome(0, Side::Corp), None);
        assert_eq!(quit_outcome(1, Side::Corp), None, "the Corp's first turn");
        assert_eq!(quit_outcome(2, Side::Corp), None, "the Runner's first turn");
        assert_eq!(quit_outcome(3, Side::Corp), Some(Outcome::RunnerWin));
        assert_eq!(quit_outcome(7, Side::Runner), Some(Outcome::CorpWin));
    }

    #[test]
    fn the_report_names_the_next_rung_only_when_it_differs() {
        let mut log = LocalRecord::default();
        log.record(game("luke", Side::Corp, "bot:operator", Outcome::RunnerWin));
        log.record(game("luke", Side::Corp, "bot:operator", Outcome::RunnerWin));
        let report = log.record(game("luke", Side::Corp, "bot:operator", Outcome::RunnerWin));
        assert_eq!(report.suggested, Level::Apprentice);
        let lines = report.lines();
        assert_eq!(lines[0], "As Corp vs operator: 0–0–3");
        assert_eq!(lines[1], "Next: try apprentice (2)");
        assert!(!lines.iter().any(|line| line.contains("Rating")), "nothing local is a rating: {lines:?}");
        let mut same = LocalRecord::default();
        for outcome in [Outcome::CorpWin, Outcome::RunnerWin, Outcome::CorpWin] {
            same.record(game("luke", Side::Corp, "bot:operator", outcome));
        }
        let report = same.record(game("luke", Side::Corp, "bot:operator", Outcome::RunnerWin));
        assert_eq!(report.suggested, Level::Operator, "2 of 4 is inside the band");
        assert_eq!(report.lines().len(), 1, "no 'next' line when the rung played is the one suggested");
    }

    #[test]
    fn the_file_round_trips_and_a_missing_one_is_empty() {
        let dir = std::env::temp_dir().join(format!("netrunner_record_{}_{}", std::process::id(), now_unix()));
        let path = dir.join("nested").join("record.json");
        assert_eq!(LocalRecord::load(&path).unwrap(), LocalRecord::default());
        let mut log = LocalRecord::default();
        log.record(game("luke", Side::Runner, "bot:novice", Outcome::RunnerWin));
        log.save(&path).unwrap();
        assert_eq!(LocalRecord::load(&path).unwrap(), log);
        assert!(!path.with_extension("json.tmp").exists(), "the temp file is renamed away");
        std::fs::write(&path, "not json").unwrap();
        assert!(LocalRecord::load(&path).unwrap_err().contains("not a record file"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The file as it was written while it held a rating book: the games
    /// are the record, the book is ignored, and the rung the player had
    /// climbed to survives the rename.
    #[test]
    fn the_ratings_file_this_replaces_is_read_once_and_its_rung_survives() {
        let dir = std::env::temp_dir().join(format!("netrunner_record_legacy_{}_{}", std::process::id(), now_unix()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut old = LocalRecord::default();
        for _ in 0..3 {
            old.record(game("luke", Side::Corp, "bot:veteran", Outcome::CorpWin));
        }
        let legacy = serde_json::json!({
            "book": { "system": { "tau": 0.5 }, "tracks": { "HumanVsBot": {} } },
            "games": serde_json::to_value(&old.games).unwrap(),
        });
        std::fs::write(dir.join("ratings.json"), legacy.to_string()).unwrap();
        let path = dir.join("record.json");

        let mut log = LocalRecord::load(&path).unwrap();
        assert_eq!(log, old);
        assert_eq!(log.suggest("luke", Side::Corp), Level::Elite);

        log.record(game("luke", Side::Corp, "bot:elite", Outcome::RunnerWin));
        log.save(&path).unwrap();
        assert_eq!(LocalRecord::load(&path).unwrap().games.len(), 4, "the new file wins once it exists");
        let untouched: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("ratings.json")).unwrap()).unwrap();
        assert_eq!(untouched, legacy, "the old file is read, never rewritten");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
