//! What both clients' Learn to Play share (Phase 1.75, Phase 7 §6): the
//! starter games' decks, and which lesson comes next for a person.
//!
//! The lessons are data in `netrunner_core::tutorial`; this is only the
//! choosing around them, so the terminal's `learn` and the desktop's
//! tracks agree on the starter lists and on what "next" means.

use std::collections::BTreeSet;

use netrunner_core::decks::{self, DeckFile};
use netrunner_core::rules::Side;
use netrunner_core::tutorial::{self, Lesson};

/// The two starter decks, The Syndicate against The Catalyst: the
/// 34- and 30-card lists played to 6 points, or with `boosted` the
/// booster-staged 44 and 40 played to the standard 7. Their category
/// carries the rules (`DeckCategory::match_rules`).
pub fn starter_decks(boosted: bool) -> Result<(DeckFile, DeckFile), String> {
    let (corp, runner) =
        if boosted { ("the_syndicate_boosted", "the_catalyst_boosted") } else { ("the_syndicate_starter", "the_catalyst_starter") };
    let load = |id: &str| decks::by_id(id).ok_or_else(|| format!("embedded deck {id:?} is missing"));
    Ok((load(corp)?, load(runner)?))
}

/// The lesson after `id` in its side's track; `None` after a track's
/// last, which does not run on into the other side's.
pub fn next_after(id: &str) -> Option<Lesson> {
    let lesson = tutorial::by_id(id)?;
    let track = tutorial::track(lesson.side);
    let at = track.iter().position(|each| each.id == id)?;
    track.into_iter().nth(at + 1)
}

/// The first lesson of `side`'s track not in `done`, in track order.
/// A person may finish them in any order; this is the one to suggest.
pub fn first_unfinished(side: Side, done: &BTreeSet<String>) -> Option<Lesson> {
    tutorial::track(side).into_iter().find(|lesson| !done.contains(&lesson.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_starter_decks_are_the_starter_category_and_the_boosted_are_standard() {
        let (corp, runner) = starter_decks(false).unwrap();
        assert_eq!(corp.category.match_rules().winning_agenda_points, 6);
        assert_eq!(runner.category.match_rules().winning_agenda_points, 6);
        let (corp, _) = starter_decks(true).unwrap();
        assert_eq!(corp.category.match_rules().winning_agenda_points, 7);
    }

    #[test]
    fn a_track_goes_on_in_order_and_stops_at_its_end() {
        for side in [Side::Corp, Side::Runner] {
            let track = tutorial::track(side);
            for pair in track.windows(2) {
                assert_eq!(next_after(&pair[0].id).map(|lesson| lesson.id), Some(pair[1].id.clone()));
            }
            assert_eq!(next_after(&track.last().unwrap().id), None);
            let mut done = BTreeSet::new();
            assert_eq!(first_unfinished(side, &done).map(|l| l.id), Some(track[0].id.clone()));
            done.insert(track[0].id.clone());
            done.insert(track[2].id.clone());
            assert_eq!(first_unfinished(side, &done).map(|l| l.id), Some(track[1].id.clone()), "the earliest gap, not the one after the last done");
            done.extend(track.iter().map(|l| l.id.clone()));
            assert_eq!(first_unfinished(side, &done), None);
        }
    }
}
