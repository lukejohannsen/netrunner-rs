//! The numbers a player watches, for a HUD: credits, clicks, agenda
//! points, hand size and the side's own threats, in one fixed order.
//!
//! **Every readout is always there, at zero too.** A HUD is read by
//! where a number sits, not by its label: a Tags readout that appeared
//! with the first tag would move every number after it at the moment the
//! person most needs to find them. So each side has a fixed set — the
//! three both sides share, in the same slots for both, then the side's
//! own — and a threat that is live is *marked* ([`Readout::alarm`])
//! rather than added.
//!
//! The person asked for this after their first desktop game (Phase 7 §4
//! item 6): the same numbers were in the strip as sentences, and
//! "Credits 5 · Clicks 3 · Agenda points 2/7" is a line to read, not a
//! place to glance. Everything here is a public counter on the masked
//! `ClientView`, so a HUD built from it shows nothing the mask withheld.
//!
//! **The Agendas readout is a door.** A press opens the side's score area
//! ([`score_area`]) as a list the person reads down and opens one row of:
//! the title and points on the row, the card, its facts and its printed
//! text under it. It replaced the strip's "Stolen: …" line, which grew
//! with the game in a strip whose height is fixed.

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::CardId;
use netrunner_core::rules::{InstallId, Side};
use netrunner_core::view::ClientView;

use super::Pile;

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
    /// The zone a click on the readout opens, if it is one: the Agendas
    /// readout opens the side's score area.
    pub opens: Option<Pile>,
}

/// How many readouts a HUD row holds before it wraps: all of either
/// side's, so a strip is one row of numbers no taller than a hand's peek,
/// and both sides' shared readouts sit in the same columns.
pub const PER_ROW: usize = 6;

/// A side's readouts, in their fixed order: Credits, Clicks, Agendas, then
/// the side's own — Bad publicity for the Corp; the Grip, Tags and Core
/// damage for the Runner. Always the same length for a side.
///
/// **The Corp has no hand readout.** Its HQ is a server column whose
/// header carries the count ("HQ · 4"), so an "HQ" readout said the same
/// number twice, as the R&D and Archives line `details` dropped did. The
/// Runner's grip has no column, so its count stays.
pub fn readouts(view: &ClientView, side: Side) -> Vec<Readout> {
    let to_win = view.rules.winning_agenda_points;
    let quiet = |label, value: String| Readout { label, value, alarm: false, opens: None };
    let threat = |label, n: u32| Readout { label, value: n.to_string(), alarm: n > 0, opens: None };
    // Two points short is one agenda from the win in every sample deck
    // but a three-pointer's; close enough to be worth the colour.
    // Named for what a click opens, not for the number: the points *are*
    // the agendas, and "Points" did not say there was a pile behind it.
    let points = |side, points: u32| Readout { label: "Agendas", value: format!("{points}/{to_win}"), alarm: points + 2 >= to_win && points > 0, opens: Some(Pile::Agendas(side)) };
    match side {
        Side::Corp => {
            let corp = &view.corp;
            vec![
                quiet("Credits", corp.credits.to_string()),
                quiet("Clicks", corp.clicks.to_string()),
                points(Side::Corp, corp.agenda_points),
                threat("Bad pub.", corp.bad_publicity),
            ]
        }
        Side::Runner => {
            let runner = &view.runner;
            vec![
                quiet("Credits", runner.credits.to_string()),
                quiet("Clicks", runner.clicks.to_string()),
                points(Side::Runner, runner.agenda_points),
                quiet("Grip", runner.grip_count.to_string()),
                threat("Tags", runner.tags),
                threat("Damage", u32::try_from(runner.brain_damage).unwrap_or(u32::MAX)),
            ]
        }
    }
}

/// The smaller line under the readouts: the counts a player looks up
/// rather than watches, and only those the table does not already show.
/// The Corp has none — R&D and Archives are server columns whose headers
/// carry their counts ("R&D · 31"), and the line repeating them was
/// dropped as redundant. The Runner's stack and heap are not here either:
/// they are the pile buttons beside it, which carry their counts.
pub fn details(view: &ClientView, side: Side) -> Option<String> {
    match side {
        Side::Corp => None,
        Side::Runner => Some(format!("MU {} · Link {}", view.runner.memory_units, view.runner.link_strength)),
    }
}

/// One agenda in a side's score area, as its list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredCard {
    pub card: CardId,
    pub title: String,
    pub points: u32,
    /// The handle the agenda kept from its install, when the side scored
    /// it: a scored agenda's ability is activated by it. A stolen agenda
    /// has none.
    pub install: Option<InstallId>,
    /// Agenda counters on it (Dividends, spent by its own abilities).
    pub counters: u32,
}

impl ScoredCard {
    /// The row's one line: the title and what it is worth.
    pub fn line(&self) -> String {
        format!("{} · {} point{}", self.title, self.points, if self.points == 1 { "" } else { "s" })
    }

    /// What its expanded row lists above the printed text: the points,
    /// the advancement it needed and who took it, and any counters.
    pub fn facts(&self, side: Side, registry: &CardRegistry) -> Vec<String> {
        let mut lines = Vec::new();
        let def = registry.get(&self.card);
        let how = match side {
            Side::Corp => "Scored by the Corp",
            Side::Runner => "Stolen by the Runner",
        };
        match def.and_then(|d| d.advancement_requirement) {
            Some(need) => lines.push(format!("{how} · {} point{} · advancement requirement {need}", self.points, if self.points == 1 { "" } else { "s" })),
            None => lines.push(format!("{how} · {} point{}", self.points, if self.points == 1 { "" } else { "s" })),
        }
        if self.counters > 0 {
            lines.push(format!("{} agenda counter{}", self.counters, if self.counters == 1 { "" } else { "s" }));
        }
        lines
    }
}

/// A side's score area, in the order the agendas arrived. The points are
/// the printed value: the view carries only the side's total, and a card
/// that changes what an agenda is worth moves that total, not the card.
pub fn score_area(view: &ClientView, side: Side, registry: &CardRegistry) -> Vec<ScoredCard> {
    let entry = |card: &CardId, install: Option<InstallId>, counters: u32| {
        let def = registry.get(card);
        ScoredCard {
            card: card.clone(),
            title: def.map_or_else(|| card.0.replace('_', " "), |d| d.title.clone()),
            points: def.and_then(|d| d.agenda_points).unwrap_or(0),
            install,
            counters,
        }
    };
    match side {
        Side::Corp => view.corp.scored_agendas.iter().map(|a| entry(&a.card, Some(a.install_id), a.agenda_counters)).collect(),
        Side::Runner => view.runner.scored_agendas.iter().map(|c| entry(c, None, 0)).collect(),
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
        assert_eq!(corp, ["Credits", "Clicks", "Agendas", "Bad pub."]);
        assert_eq!(runner, ["Credits", "Clicks", "Agendas", "Grip", "Tags", "Damage"]);
        assert_eq!(readouts(&view, Side::Corp)[2].opens, Some(Pile::Agendas(Side::Corp)));
        assert_eq!(readouts(&view, Side::Runner)[2].opens, Some(Pile::Agendas(Side::Runner)));
        assert!(readouts(&view, Side::Runner).iter().filter(|r| r.opens.is_some()).count() == 1, "only the Agendas readout opens anything");
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

    #[test]
    fn the_score_area_lists_each_agenda_with_its_printed_points() {
        let registry = crate::decks::sample_deck_registry();
        let mut view = view();
        assert!(score_area(&view, Side::Corp, &registry).is_empty());
        let agenda = registry.iter().find(|c| c.agenda_points.is_some() && c.advancement_requirement.is_some()).expect("the sample decks hold an agenda").clone();
        view.runner.scored_agendas = vec![agenda.id.clone(), agenda.id.clone()];
        let stolen = score_area(&view, Side::Runner, &registry);
        assert_eq!(stolen.len(), 2, "two copies are two rows");
        assert_eq!(stolen[0].title, agenda.title);
        assert_eq!(stolen[0].points, agenda.agenda_points.unwrap());
        assert!(stolen[0].install.is_none(), "a stolen agenda has no ability handle");
        let facts = stolen[0].facts(Side::Runner, &registry);
        assert!(facts[0].starts_with("Stolen by the Runner"), "{facts:?}");
        assert!(stolen[0].line().contains(&agenda.title));
    }

    #[test]
    fn the_details_line_repeats_nothing_the_table_shows() {
        let view = view();
        assert_eq!(details(&view, Side::Corp), None, "R&D and Archives are server headers");
        assert!(details(&view, Side::Runner).is_some_and(|line| line.starts_with("MU ")));
    }
}
