//! `RatingBook`: every rating the system holds, by track, participant and
//! role, with the one operation that changes it — `record` a finished
//! match — and a sorted ladder to print.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::glicko2::{Glicko2, Rating, Score};

/// Which ladder a match counts toward. Two, and they never mix: a bot's
/// benchmark standing must not move because a person played it, and a
/// person's standing is against people.
///
/// **There is no people-against-bots track, and there was one.** A rating
/// is a claim to someone else, so it means something only between people
/// and only when somebody other than the rated player keeps it: a person's
/// games against a bot are practice — take-backs allowed, the file theirs
/// to delete — and what they need from them is which rung to try next,
/// which is a win/loss record (`netrunner_client::record`) and never was
/// the Glicko-2 number. `HumanVsBot` was retired for that reason; a book
/// written while it existed still loads (see [`RatingBook`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Track {
    /// People against people — the competitive ladder, kept by a server.
    HumanVsHuman,
    /// Bots against bots, offline: `netrunner_cli bench`.
    BotBenchmark,
}

/// The chair a participant sat in. The crate's own type rather than the
/// engine's `Side`, so this crate stays engine-free; consumers convert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Corp,
    Runner,
}

/// Who won, from the table's point of view. A stalled or abandoned match
/// with no winner is not an `Outcome` at all — the caller simply does not
/// `record` it, which is the only honest treatment of "nobody won".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    CorpWin,
    RunnerWin,
    Draw,
}

impl Outcome {
    fn scores(self) -> (Score, Score) {
        match self {
            Outcome::CorpWin => (Score::Win, Score::Loss),
            Outcome::RunnerWin => (Score::Loss, Score::Win),
            Outcome::Draw => (Score::Draw, Score::Draw),
        }
    }
}

/// One role's record: the rating and how it got there.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct RoleRecord {
    pub rating: Rating,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
}

impl RoleRecord {
    pub fn games(&self) -> u32 {
        self.wins + self.draws + self.losses
    }

    fn tally(&mut self, score: Score) {
        match score {
            Score::Win => self.wins += 1,
            Score::Draw => self.draws += 1,
            Score::Loss => self.losses += 1,
        }
    }
}

/// A participant's standing on one track: a record per role.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Standing {
    pub corp: RoleRecord,
    pub runner: RoleRecord,
}

impl Standing {
    pub fn role(&self, role: Role) -> &RoleRecord {
        match role {
            Role::Corp => &self.corp,
            Role::Runner => &self.runner,
        }
    }

    fn role_mut(&mut self, role: Role) -> &mut RoleRecord {
        match role {
            Role::Corp => &mut self.corp,
            Role::Runner => &mut self.runner,
        }
    }

    /// The single number a ladder sorts by: the mean of the two roles.
    /// Display only — nothing is ever *updated* from it.
    pub fn overall(&self) -> f64 {
        (self.corp.rating.rating + self.runner.rating.rating) / 2.0
    }
}

/// Every rating in the system. Serializable whole, so a consumer's
/// persistence is one file; `BTreeMap`s so that file is stable under
/// `diff`.
///
/// **A track this crate no longer knows is dropped on load, not an
/// error.** The map is keyed by the enum, so retiring a variant would
/// otherwise turn an operator's existing book into a parse failure and
/// take the tracks that are still kept down with it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RatingBook {
    #[serde(default)]
    pub system: Glicko2,
    #[serde(default, deserialize_with = "known_tracks")]
    tracks: BTreeMap<Track, BTreeMap<String, Standing>>,
}

fn known_tracks<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<BTreeMap<Track, BTreeMap<String, Standing>>, D::Error> {
    let by_name = BTreeMap::<String, BTreeMap<String, Standing>>::deserialize(deserializer)?;
    Ok(by_name
        .into_iter()
        .filter_map(|(name, standings)| {
            let track = serde_json::from_value(serde_json::Value::String(name)).ok()?;
            Some((track, standings))
        })
        .collect())
}

impl RatingBook {
    /// Records one finished match and returns the two standings after it.
    /// **A rating period of one game**: the Corp's Corp rating is updated
    /// against the Runner's Runner rating as both stood before the game,
    /// and vice versa, so the order of the two updates cannot matter. A
    /// participant playing itself (a bot benchmarked against its own kind)
    /// is fine — the two roles are two ratings.
    pub fn record(&mut self, track: Track, corp: &str, runner: &str, outcome: Outcome) -> (Standing, Standing) {
        let (corp_score, runner_score) = outcome.scores();
        let corp_before = self.standing(track, corp).unwrap_or_default().corp.rating;
        let runner_before = self.standing(track, runner).unwrap_or_default().runner.rating;
        let system = self.system;

        let corp_record = self.track_mut(track).entry(corp.to_string()).or_default().role_mut(Role::Corp);
        corp_record.rating = system.update(corp_before, &[(runner_before, corp_score)]);
        corp_record.tally(corp_score);

        let runner_record = self.track_mut(track).entry(runner.to_string()).or_default().role_mut(Role::Runner);
        runner_record.rating = system.update(runner_before, &[(corp_before, runner_score)]);
        runner_record.tally(runner_score);

        (self.standing(track, corp).unwrap(), self.standing(track, runner).unwrap())
    }

    pub fn standing(&self, track: Track, participant: &str) -> Option<Standing> {
        self.tracks.get(&track)?.get(participant).copied()
    }

    /// Every participant on `track`, best overall first, ties by name so
    /// the order is total.
    pub fn ladder(&self, track: Track) -> Vec<(&str, Standing)> {
        let mut rows: Vec<(&str, Standing)> =
            self.tracks.get(&track).into_iter().flatten().map(|(name, standing)| (name.as_str(), *standing)).collect();
        rows.sort_by(|a, b| b.1.overall().partial_cmp(&a.1.overall()).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.cmp(b.0)));
        rows
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.values().all(BTreeMap::is_empty)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a RatingBook serializes")
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    fn track_mut(&mut self, track: Track) -> &mut BTreeMap<String, Standing> {
        self.tracks.entry(track).or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_result_moves_exactly_the_two_roles_that_played() {
        let mut book = RatingBook::default();
        let (corp, runner) = book.record(Track::BotBenchmark, "bot:heuristic", "bot:random", Outcome::CorpWin);
        assert!(corp.corp.rating.rating > 1500.0);
        assert_eq!(corp.runner.rating, Rating::default(), "the winner's Runner rating did not play");
        assert!(runner.runner.rating.rating < 1500.0);
        assert_eq!(runner.corp.rating, Rating::default());
        assert_eq!((corp.corp.wins, runner.runner.losses), (1, 1));
    }

    #[test]
    fn both_updates_read_the_ratings_as_they_stood_before_the_game() {
        let mut book = RatingBook::default();
        book.record(Track::BotBenchmark, "a", "b", Outcome::CorpWin);
        let (a, b) = (book.standing(Track::BotBenchmark, "a").unwrap(), book.standing(Track::BotBenchmark, "b").unwrap());
        // Symmetric by construction: equal newcomers, one game, so the
        // winner's gain equals the loser's loss.
        assert!((a.corp.rating.rating - 1500.0 + (b.runner.rating.rating - 1500.0)).abs() < 1e-9);
    }

    #[test]
    fn tracks_never_mix() {
        let mut book = RatingBook::default();
        book.record(Track::HumanVsHuman, "ann", "bo", Outcome::CorpWin);
        assert!(book.standing(Track::BotBenchmark, "ann").is_none());
        assert!(book.standing(Track::HumanVsHuman, "bo").unwrap().runner.rating.rating < 1500.0);
    }

    #[test]
    fn a_book_holding_a_retired_track_loads_with_the_kept_tracks_intact() {
        let mut book = RatingBook::default();
        book.record(Track::HumanVsHuman, "ann", "bo", Outcome::CorpWin);
        let mut json: serde_json::Value = serde_json::from_str(&book.to_json()).unwrap();
        let ann = json["tracks"]["HumanVsHuman"].clone();
        json["tracks"]["HumanVsBot"] = ann;
        assert_eq!(RatingBook::from_json(&json.to_string()).unwrap(), book);
    }

    #[test]
    fn a_participant_may_play_itself() {
        let mut book = RatingBook::default();
        let (corp, runner) = book.record(Track::BotBenchmark, "bot:heuristic", "bot:heuristic", Outcome::RunnerWin);
        assert_eq!(corp, runner, "one participant, two roles");
        assert!(corp.corp.rating.rating < 1500.0 && corp.runner.rating.rating > 1500.0);
    }

    #[test]
    fn the_ladder_is_best_first_and_total() {
        let mut book = RatingBook::default();
        book.record(Track::BotBenchmark, "b", "a", Outcome::RunnerWin);
        book.record(Track::BotBenchmark, "c", "c", Outcome::Draw);
        let names: Vec<&str> = book.ladder(Track::BotBenchmark).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, vec!["a", "c", "b"]);
    }

    #[test]
    fn the_book_round_trips_through_json() {
        let mut book = RatingBook::default();
        book.record(Track::HumanVsHuman, "ann", "bo", Outcome::Draw);
        book.record(Track::BotBenchmark, "bot:mcts@32", "bot:random", Outcome::CorpWin);
        let json = book.to_json();
        assert_eq!(RatingBook::from_json(&json).unwrap(), book);
        assert_eq!(RatingBook::from_json("{}").unwrap(), RatingBook::default(), "an empty file is an empty book");
    }
}
