//! The replay screen's state that can be tested without a window: what a
//! press on the replay bar or a key means (`Step`), and the words a
//! record is listed under (`label`).

use std::path::Path;

use netrunner_client::bug_report::MatchRecordHeader;
use netrunner_core::rules::Side;

/// A move through a recorded match. The terminal client's keys, less the
/// letters it needs for its own panes: a step, ten, either end, the other
/// chair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    First,
    Back(usize),
    Forward(usize),
    Last,
    SwapChair,
}

impl Step {
    /// The bar, left to right. Ten at a time is on the keys only
    /// (Page Up, Page Down): the bar is a row of the board and its room is
    /// the hand's.
    pub const BAR: [Step; 5] = [Step::First, Step::Back(1), Step::Forward(1), Step::Last, Step::SwapChair];

    pub fn label(self) -> &'static str {
        match self {
            Step::First => "|< Start",
            Step::Back(_) => "< Back",
            Step::Forward(_) => "Next >",
            Step::Last => "End >|",
            Step::SwapChair => "Other chair",
        }
    }

    /// Whether the step would move anything at `cursor` of `len`. The
    /// chair can always be swapped.
    pub fn moves(self, cursor: usize, len: usize) -> bool {
        match self {
            Step::First | Step::Back(_) => cursor > 0,
            Step::Forward(_) | Step::Last => cursor < len,
            Step::SwapChair => true,
        }
    }
}

/// What a record is listed under: when it was saved and its seed, read
/// off the file name a bug report is given, and who the person played
/// when the header says. A file with another name is listed by its name.
pub fn label(path: &Path, header: Option<&MatchRecordHeader>) -> String {
    let name = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let when = saved_at(&name).unwrap_or(name);
    match header {
        None => format!("{when} · not a record this client can read"),
        Some(header) => match header.bot {
            Some(bot) => format!("{when} · seed {} · your {} against the {} {:?} {}", header.seed, chair(bot.side.other()), bot.level, bot.personality, chair(bot.side)),
            None => format!("{when} · seed {} · a match between two bots", header.seed),
        },
    }
}

fn chair(side: Side) -> &'static str {
    match side {
        Side::Corp => "Corp",
        Side::Runner => "Runner",
    }
}

/// `2026-09-22T20-15-03-seed42` → `2026-09-22 20:15:03 UTC`; `None` for a
/// name `bug_report::save` did not give.
fn saved_at(stem: &str) -> Option<String> {
    let stamp = stem.get(..19)?;
    let bytes = stamp.as_bytes();
    let shaped = bytes.iter().enumerate().all(|(i, b)| match i {
        4 | 7 | 13 | 16 => *b == b'-',
        10 => *b == b'T',
        _ => b.is_ascii_digit(),
    });
    shaped.then(|| format!("{} {}:{}:{} UTC", &stamp[..10], &stamp[11..13], &stamp[14..16], &stamp[17..19]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_client::start::{Level, Personality};
    use netrunner_core::rules::MatchRules;
    use std::path::PathBuf;

    fn header(bot: Option<netrunner_client::bug_report::RecordedBot>) -> MatchRecordHeader {
        let corp = netrunner_core::decks::by_id("discretion_advised").expect("built-in deck");
        let runner = netrunner_core::decks::by_id("stolen_goods").expect("built-in deck");
        MatchRecordHeader { seed: 42, corp_deck: corp.to_deck(), runner_deck: runner.to_deck(), rules: MatchRules::default(), bot, order: Default::default() }
    }

    #[test]
    fn a_report_is_listed_by_when_and_whom() {
        let path = PathBuf::from("/r/2026-09-22T20-15-03-seed42.jsonl");
        let bot = netrunner_client::bug_report::RecordedBot { side: Side::Corp, level: Level::Elite, personality: Personality::Glacier };
        let listed = label(&path, Some(&header(Some(bot))));
        assert!(listed.starts_with("2026-09-22 20:15:03 UTC · seed 42 · your Runner against the "), "{listed}");
        assert!(listed.ends_with("Glacier Corp"), "{listed}");
        assert_eq!(label(&path, Some(&header(None))), "2026-09-22 20:15:03 UTC · seed 42 · a match between two bots");
        assert_eq!(label(&PathBuf::from("game_00001.jsonl"), None), "game_00001 · not a record this client can read");
    }

    #[test]
    fn the_bar_greys_what_would_not_move() {
        assert!(!Step::Back(1).moves(0, 10));
        assert!(!Step::First.moves(0, 10));
        assert!(Step::Forward(1).moves(0, 10));
        assert!(!Step::Last.moves(10, 10));
        assert!(Step::SwapChair.moves(10, 10));
    }
}
