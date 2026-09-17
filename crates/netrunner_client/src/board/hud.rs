//! The numbers a player watches, for a HUD: credits, clicks, agenda
//! points, hand size and the side's own threats, in one fixed order.
//!
//! **Every readout is always there, at zero too.** A HUD is read by
//! where a number sits, not by its label: a Tags readout that appeared
//! with the first tag would move every number after it at the moment the
//! person most needs to find them. So each side has a fixed set — the
//! four both sides share, in the same slots for both, then the side's
//! own — and a threat that is live is *marked* ([`Readout::alarm`])
//! rather than added.
//!
//! The person asked for this after their first desktop game (Phase 7 §4
//! item 6): the same numbers were in the strip as sentences, and
//! "Credits 5 · Clicks 3 · Agenda points 2/7" is a line to read, not a
//! place to glance. Everything here is a public counter on the masked
//! `ClientView`, so a HUD built from it shows nothing the mask withheld.

use netrunner_core::rules::Side;
use netrunner_core::view::ClientView;

/// One number on the HUD and the word under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readout {
    /// The short word that fits under a large number: "Credits", "Tags".
    pub label: &'static str,
    /// The number as drawn: `5`, or `2/7` for agenda points.
    pub value: String,
    /// A threat that is live — a tag, core damage, bad publicity — or an
    /// agenda-point total one agenda from winning: drawn so it is seen.
    pub alarm: bool,
}

/// How many readouts a HUD row holds before it wraps, so both sides'
/// shared four and their own sit in the same columns.
pub const PER_ROW: usize = 3;

/// A side's readouts, in their fixed order: Credits, Clicks, Points, then
/// the hand (HQ or Grip), then the side's own — Bad publicity for the
/// Corp, Tags and Core damage for the Runner. Always the same length for
/// a side.
pub fn readouts(view: &ClientView, side: Side) -> Vec<Readout> {
    let to_win = view.rules.winning_agenda_points;
    let quiet = |label, value: String| Readout { label, value, alarm: false };
    let threat = |label, n: u32| Readout { label, value: n.to_string(), alarm: n > 0 };
    // Two points short is one agenda from the win in every sample deck
    // but a three-pointer's; close enough to be worth the colour.
    let points = |label, points: u32| Readout { label, value: format!("{points}/{to_win}"), alarm: points + 2 >= to_win && points > 0 };
    match side {
        Side::Corp => {
            let corp = &view.corp;
            vec![
                quiet("Credits", corp.credits.to_string()),
                quiet("Clicks", corp.clicks.to_string()),
                points("Points", corp.agenda_points),
                quiet("HQ", corp.hq_count.to_string()),
                threat("Bad pub.", corp.bad_publicity),
            ]
        }
        Side::Runner => {
            let runner = &view.runner;
            vec![
                quiet("Credits", runner.credits.to_string()),
                quiet("Clicks", runner.clicks.to_string()),
                points("Points", runner.agenda_points),
                quiet("Grip", runner.grip_count.to_string()),
                threat("Tags", runner.tags),
                threat("Damage", u32::try_from(runner.brain_damage).unwrap_or(u32::MAX)),
            ]
        }
    }
}

/// The smaller line under the readouts: the counts a player looks up
/// rather than watches. The Runner's stack and heap are not here — they
/// are the pile buttons beside it, which already carry their counts.
pub fn details(view: &ClientView, side: Side) -> String {
    match side {
        Side::Corp => format!("R&D {} · Archives {}", view.corp.rd_count, view.corp.archives.len()),
        Side::Runner => format!("MU {} · Link {}", view.runner.memory_units, view.runner.link_strength),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::GameState;
    use netrunner_session::{sweep_decks_for_seed, Seat, Session};

    fn view() -> ClientView {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        Session::new(state, registry, Seat::External, Seat::External).view_for(Side::Corp)
    }

    #[test]
    fn the_shared_readouts_sit_in_the_same_slots_for_both_sides() {
        let view = view();
        let corp: Vec<_> = readouts(&view, Side::Corp).iter().map(|r| r.label).collect();
        let runner: Vec<_> = readouts(&view, Side::Runner).iter().map(|r| r.label).collect();
        assert_eq!(corp[..3], runner[..3]);
        assert_eq!(corp, ["Credits", "Clicks", "Points", "HQ", "Bad pub."]);
        assert_eq!(runner, ["Credits", "Clicks", "Points", "Grip", "Tags", "Damage"]);
    }

    #[test]
    fn a_threat_is_always_shown_and_marked_only_when_live() {
        let mut view = view();
        let quiet = readouts(&view, Side::Runner);
        assert_eq!(quiet[4].value, "0");
        assert!(!quiet[4].alarm, "no tags is not an alarm");
        view.runner.tags = 2;
        view.runner.brain_damage = 1;
        let hot = readouts(&view, Side::Runner);
        assert_eq!(hot.len(), quiet.len(), "a live threat marks its slot rather than adding one");
        assert!(hot[4].alarm && hot[4].value == "2");
        assert!(hot[5].alarm && hot[5].value == "1");
    }

    #[test]
    fn points_read_against_the_total_and_warn_one_agenda_out() {
        let mut view = view();
        let to_win = view.rules.winning_agenda_points;
        assert_eq!(readouts(&view, Side::Corp)[2].value, format!("0/{to_win}"));
        assert!(!readouts(&view, Side::Corp)[2].alarm);
        view.corp.agenda_points = to_win - 2;
        assert!(readouts(&view, Side::Corp)[2].alarm);
    }
}
