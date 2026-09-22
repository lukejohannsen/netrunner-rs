//! A bug report is the game, as a file that replays (Phase 7 §8 item 15).
//!
//! Most of the fixes to how the bots play began as a person describing a
//! moment in words — the matchup, the rung, what the Corp did — and
//! somebody reproducing it by hand. The match record already replays bit
//! for bit (`netrunner_session::MatchHistory::write_jsonl`, the header's
//! `setup`, and `apply_action` being pure), so a report is that record
//! written where the person can find it, and `netrunner_cli replay <file>`
//! opens it at the moment it was saved.
//!
//! **Actions, not bots.** The file holds every action either seat applied,
//! so a replay never asks a bot anything. Re-running the bot from its seed
//! would be smaller and wrong: a take-back restores the state but not the
//! bot's own random stream, so after one the bot plays a different game.
//! The rung and style are in the header for the reader.
//!
//! **The directory is the client's, like the record and the decks**:
//! `<data dir>/netrunner/reports/`, or `NETRUNNER_REPORTS_DIR`, with the
//! same precedence split as `record::resolve_record_file_with` so a test
//! never sets the process environment.

use std::fs::OpenOptions;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Re-exported so a client can read a report back — a test, a viewer —
/// without naming the session crate for the two types a report is.
pub use netrunner_session::{MatchHistory, MatchRecordHeader};

/// Overrides where reports are written.
pub const REPORTS_DIR_ENV: &str = "NETRUNNER_REPORTS_DIR";

/// Where reports go: the environment, then the OS data directory. `None`
/// when neither exists, which a client shows as "nowhere to save".
pub fn resolve_reports_dir() -> Option<PathBuf> {
    resolve_reports_dir_with(std::env::var_os(REPORTS_DIR_ENV))
}

fn resolve_reports_dir_with(env: Option<std::ffi::OsString>) -> Option<PathBuf> {
    if let Some(path) = env.filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path));
    }
    crate::data_dir().map(|base| base.join("netrunner").join("reports"))
}

/// Writes the record into `dir` and returns the file's path.
///
/// Named by the UTC time and the seed — `2026-09-22T20-15-03-seed42.jsonl`
/// — so a directory of reports sorts by when, and the seed is readable
/// without opening one. The file is created fresh, never overwritten: two
/// saves in the same second get `-2`, `-3`.
pub fn save(dir: &Path, header: &MatchRecordHeader, history: &MatchHistory) -> io::Result<PathBuf> {
    save_at(dir, header, history, SystemTime::now())
}

fn save_at(dir: &Path, header: &MatchRecordHeader, history: &MatchHistory, now: SystemTime) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stem = format!("{}-seed{}", utc_stamp(now), header.seed);
    for attempt in 1.. {
        let name = if attempt == 1 { format!("{stem}.jsonl") } else { format!("{stem}-{attempt}.jsonl") };
        let path = dir.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                let mut writer = BufWriter::new(file);
                history.write_jsonl(header, &mut writer)?;
                writer.flush()?;
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    unreachable!("an unbounded range always yields another attempt")
}

/// `YYYY-MM-DDTHH-MM-SS` in UTC, with dashes where ISO 8601 has colons,
/// because a colon is not allowed in a Windows file name. Written out
/// rather than a date crate for one line of a file name.
fn utc_stamp(now: SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let (hour, minute, second) = (rest / 3600, rest % 3600 / 60, rest % 60);
    // Days since the epoch to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}-{minute:02}-{second:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::MatchRules;
    use std::time::Duration;

    fn header(seed: u64) -> MatchRecordHeader {
        let corp = netrunner_core::decks::by_id("discretion_advised").expect("built-in deck");
        let runner = netrunner_core::decks::by_id("stolen_goods").expect("built-in deck");
        MatchRecordHeader { seed, corp_deck: corp.to_deck(), runner_deck: runner.to_deck(), rules: MatchRules::default(), bot: None }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("netrunner_client_bug_report_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_stamp_is_the_utc_date_and_time() {
        assert_eq!(utc_stamp(UNIX_EPOCH), "1970-01-01T00-00-00");
        // 2026-09-22 20:15:03 UTC.
        assert_eq!(utc_stamp(UNIX_EPOCH + Duration::from_secs(1_790_108_103)), "2026-09-22T20-15-03");
        // A leap day.
        assert_eq!(utc_stamp(UNIX_EPOCH + Duration::from_secs(1_709_164_800)), "2024-02-29T00-00-00");
    }

    #[test]
    fn a_report_reads_back_as_the_record_it_was_and_never_overwrites_another() {
        let dir = temp_dir("round_trip");
        let now = UNIX_EPOCH + Duration::from_secs(1_790_108_103);
        let (header, history) = (header(42), MatchHistory::new());
        let first = save_at(&dir, &header, &history, now).expect("saves");
        let second = save_at(&dir, &header, &history, now).expect("saves beside it");
        assert_eq!(first.file_name().unwrap(), "2026-09-22T20-15-03-seed42.jsonl");
        assert_eq!(second.file_name().unwrap(), "2026-09-22T20-15-03-seed42-2.jsonl");
        let file = std::io::BufReader::new(std::fs::File::open(&first).unwrap());
        let (read_header, read_history) = MatchHistory::read_jsonl(file).expect("reads back");
        assert_eq!((read_header, read_history), (header, history));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_environment_names_the_directory_and_the_data_directory_is_the_fallback() {
        assert_eq!(resolve_reports_dir_with(Some("/tmp/r".into())), Some(PathBuf::from("/tmp/r")));
        assert_eq!(resolve_reports_dir_with(Some("".into())), crate::data_dir().map(|d| d.join("netrunner").join("reports")));
    }
}
