//! Where the game is in the turn, and in a run, as a bar of steps.
//!
//! **A turn has a shape, and the board did not show it.** The status line
//! says "Turn 12 · Runner's turn", which names the phase the engine is in
//! but not what comes before or after it, and a run's step was only in the
//! prompt's sentence. A person learning the game asked for the structure
//! itself: the steps of the turn in order, with the one they are in marked.
//!
//! **Every step of a turn is always listed, and the one in play is
//! marked** — the HUD's rule ([`super::hud`]), for the same reason: a bar
//! whose steps appeared as they were reached would be a different bar
//! every time it was read. So a turn is always its three steps, and a run
//! always its four, and [`Step::state`] says which one the game is in.
//!
//! **A run is a second segment, not more steps on the first.** A run
//! happens inside the Runner's action step (and the Corp's, off a card),
//! and flattening the two would have said the turn had left its actions
//! behind, which it has not.
//!
//! Everything here is read off the masked [`ClientView`]: `phase`, `turn`,
//! `active_run` and `paid_ability_window`, all public. No engine state and
//! no rules — the bar reports where the game is, and never decides it.

use netrunner_core::rules::{GamePhase, RunPhase, Side, WindowCheckpoint};
use netrunner_core::view::ClientView;

/// Where the game is in relation to one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Done this turn, or this run.
    Past,
    /// The step the game is in.
    Now,
    /// Still to come.
    Ahead,
}

/// One step of a segment: the words on it and where the game is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub label: String,
    pub state: State,
}

/// A row of steps under a heading: the turn, or the run inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// "Corp turn 12", "Run on HQ".
    pub title: String,
    pub steps: Vec<Step>,
}

/// The phase bar: the turn, the run if one is on, and a line for a window
/// that is holding the turn open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bar {
    pub segments: Vec<Segment>,
    /// Who holds priority in an open paid-ability window, and what the
    /// window is for. `None` when no window is open: the turn's own steps
    /// say where the game is, and a line that was always there would say
    /// nothing four turns in five.
    pub note: Option<String>,
}

/// The bar for `view`, from the chair of the viewer it was built for.
pub fn bar(view: &ClientView) -> Bar {
    Bar { segments: segments(view), note: note(view) }
}

fn segments(view: &ClientView) -> Vec<Segment> {
    let mut segments = vec![turn_segment(view)];
    if let Some(run) = &view.active_run {
        segments.push(run_segment(run.phase, run.position, run.ice.len(), &super::action_map::server_name(run.server)));
    }
    segments
}

/// The turn's three steps — draw, actions, discard — with the one the
/// engine is in marked. The mulligan and the end of the game are not
/// steps of a turn, so each is a segment of its own with a single step:
/// they are the whole of what is happening.
fn turn_segment(view: &ClientView) -> Segment {
    let side = |side: Side| match side {
        Side::Corp => "Corp",
        Side::Runner => "Runner",
    };
    match view.phase {
        GamePhase::Mulligan(_) => Segment { title: "Opening hands".to_string(), steps: vec![Step { label: "Keep or mulligan".to_string(), state: State::Now }] },
        GamePhase::GameOver(winner) => {
            Segment { title: "The match is over".to_string(), steps: vec![Step { label: format!("{} wins", side(winner)), state: State::Now }] }
        }
        // Momentary: the engine advances past it on its own, and a person
        // who sees it is watching the draw happen.
        GamePhase::StartOfTurn(owner) => Segment { title: format!("{} turn {}", side(owner), view.turn), steps: turn_steps(view, 0) },
        GamePhase::Action(owner) => Segment { title: format!("{} turn {}", side(owner), view.turn), steps: turn_steps(view, 1) },
        GamePhase::Discard { side: owner, .. } => Segment { title: format!("{} turn {}", side(owner), view.turn), steps: turn_steps(view, 2) },
    }
}

/// Draw, actions, discard, with `at` the one in play. The actions step
/// carries the clicks left, because that is the number a person checks
/// before they decide, and the discard step how many must go.
fn turn_steps(view: &ClientView, at: usize) -> Vec<Step> {
    let owner = match view.phase {
        GamePhase::Discard { side, .. } => side,
        GamePhase::StartOfTurn(side) | GamePhase::Action(side) | GamePhase::Mulligan(side) | GamePhase::GameOver(side) => side,
    };
    let clicks = match owner {
        Side::Corp => view.corp.clicks,
        Side::Runner => view.runner.clicks,
    };
    let actions = match (at, clicks) {
        (1, 1) => "Actions · 1 click left".to_string(),
        (1, n) => format!("Actions · {n} clicks left"),
        _ => "Actions".to_string(),
    };
    let discard = match view.phase {
        GamePhase::Discard { required, .. } if at == 2 => format!("Discard {required}"),
        _ => "Discard".to_string(),
    };
    // The Corp draws at the start of its turn and the Runner does not;
    // the Runner's first step is the clicks it gains, which the engine
    // does in the same momentary phase.
    let first = match owner {
        Side::Corp => "Draw".to_string(),
        Side::Runner => "Turn begins".to_string(),
    };
    [first, actions, discard]
        .into_iter()
        .enumerate()
        .map(|(i, label)| Step {
            label,
            state: match i.cmp(&at) {
                std::cmp::Ordering::Less => State::Past,
                std::cmp::Ordering::Equal => State::Now,
                std::cmp::Ordering::Greater => State::Ahead,
            },
        })
        .collect()
}

/// A run's four steps, with the ice counted in the approach: the engine's
/// finer moments (the rez window, a subroutine resolving, passing the
/// piece) happen inside approach and encounter as actions, so they are not
/// steps — a bar that grew a step per subroutine would be a different bar
/// on every ice.
fn run_segment(phase: RunPhase, position: usize, ice: usize, server: &str) -> Segment {
    let at = match phase {
        RunPhase::Initiation => 0,
        RunPhase::ApproachIce => 1,
        RunPhase::EncounterIce => 2,
        RunPhase::AccessingCard | RunPhase::Success => 3,
        RunPhase::Ended => 4,
    };
    let of_ice = |what: &str| match (ice, position) {
        (0, _) => what.to_string(),
        (total, at) => format!("{what} ice {} of {total}", at.min(total.saturating_sub(1)) + 1),
    };
    let labels = [
        "Initiation".to_string(),
        of_ice("Approach"),
        of_ice("Encounter"),
        "Access".to_string(),
        // The run is over but the trail is still on the board; naming the
        // last step keeps the bar the same width as it empties.
        "Run over".to_string(),
    ];
    let steps = labels
        .into_iter()
        .take(if at == 4 { 5 } else { 4 })
        .enumerate()
        .map(|(i, label)| Step {
            label,
            state: match i.cmp(&at) {
                std::cmp::Ordering::Less => State::Past,
                std::cmp::Ordering::Equal => State::Now,
                std::cmp::Ordering::Greater => State::Ahead,
            },
        })
        .collect();
    Segment { title: format!("Run on {server}"), steps }
}

/// The line under the steps while a paid-ability window is open: who the
/// game is waiting on, and which window it is. Both players may act in
/// these windows, so who holds priority is the thing a person cannot work
/// out from the board.
fn note(view: &ClientView) -> Option<String> {
    let window = view.paid_ability_window.as_ref()?;
    let who = match window.active_priority {
        Side::Corp => "the Corp",
        Side::Runner => "the Runner",
    };
    let what = match window.checkpoint {
        WindowCheckpoint::Run => "during the run",
        WindowCheckpoint::StartOfTurn { .. } => "at the start of the turn",
        WindowCheckpoint::EndOfTurn { .. } => "at the end of the turn",
        WindowCheckpoint::Prevention => "to prevent what is happening",
        WindowCheckpoint::PostAction { .. } => "after the action",
    };
    Some(format!("Paid ability window {what} — {who} has priority"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{GameState, ServerId};
    use netrunner_session::{sweep_decks_for_seed, Seat, Session};

    fn view() -> ClientView {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        Session::new(state, registry, Seat::External, Seat::External).view_for(Side::Corp)
    }

    fn labels(segment: &Segment) -> Vec<&str> {
        segment.steps.iter().map(|s| s.label.as_str()).collect()
    }

    fn now(segment: &Segment) -> Vec<&str> {
        segment.steps.iter().filter(|s| s.state == State::Now).map(|s| s.label.as_str()).collect()
    }

    #[test]
    fn a_turn_is_always_its_three_steps_with_the_one_in_play_marked() {
        let mut view = view();
        // The opening hands are their own segment: no turn has begun.
        let opening = bar(&view);
        assert_eq!(opening.segments.len(), 1);
        assert_eq!(opening.segments[0].title, "Opening hands");
        assert_eq!(opening.note, None);

        view.phase = GamePhase::Action(Side::Corp);
        view.turn = 3;
        view.corp.clicks = 2;
        let segment = &bar(&view).segments[0];
        assert_eq!(segment.title, "Corp turn 3");
        assert_eq!(labels(segment), ["Draw", "Actions · 2 clicks left", "Discard"]);
        assert_eq!(segment.steps.iter().map(|s| s.state).collect::<Vec<_>>(), [State::Past, State::Now, State::Ahead]);

        view.corp.clicks = 1;
        assert_eq!(now(&bar(&view).segments[0]), ["Actions · 1 click left"]);

        view.phase = GamePhase::Discard { side: Side::Corp, required: 2 };
        let segment = &bar(&view).segments[0];
        assert_eq!(labels(segment), ["Draw", "Actions", "Discard 2"]);
        assert_eq!(now(segment), ["Discard 2"]);

        // The Runner has no mandatory draw, so its first step is not one.
        view.phase = GamePhase::Action(Side::Runner);
        view.runner.clicks = 4;
        let segment = &bar(&view).segments[0];
        assert_eq!(segment.title, "Runner turn 3");
        assert_eq!(labels(segment), ["Turn begins", "Actions · 4 clicks left", "Discard"]);

        view.phase = GamePhase::GameOver(Side::Runner);
        assert_eq!(labels(&bar(&view).segments[0]), ["Runner wins"]);
    }

    #[test]
    fn a_run_is_a_second_segment_that_counts_the_ice() {
        let mut view = view();
        view.phase = GamePhase::Action(Side::Runner);
        let mut run = netrunner_core::rules::PublicRunState {
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: Vec::new(),
            position: 0,
            access_state: None,
            jack_out_permitted: true,
            bad_publicity_credits: 0,
            bonus_run_credits: 0,
            runner_cannot_steal_or_trash: false,
            redirect_on_approach: None,
        };
        // No ice: the steps are there, and none of them counts a piece.
        view.active_run = Some(run.clone());
        let approaching = bar(&view);
        assert_eq!(approaching.segments.len(), 2, "the turn keeps its own steps");
        assert_eq!(approaching.segments[1].title, "Run on HQ");
        assert_eq!(labels(&approaching.segments[1]), ["Initiation", "Approach", "Encounter", "Access"]);
        assert_eq!(now(&approaching.segments[1]), ["Approach"]);

        run.ice = vec![ice(), ice(), ice()];
        run.position = 1;
        run.phase = RunPhase::EncounterIce;
        view.active_run = Some(run.clone());
        let segment = &bar(&view).segments[1];
        assert_eq!(labels(segment), ["Initiation", "Approach ice 2 of 3", "Encounter ice 2 of 3", "Access"]);
        assert_eq!(now(segment), ["Encounter ice 2 of 3"]);

        run.phase = RunPhase::AccessingCard;
        view.active_run = Some(run.clone());
        assert_eq!(now(&bar(&view).segments[1]), ["Access"]);

        // An ended run keeps its trail on the board, and the bar says so
        // rather than vanishing under the person's eyes.
        run.phase = RunPhase::Ended;
        view.active_run = Some(run);
        let segment = &bar(&view).segments[1];
        assert_eq!(labels(segment), ["Initiation", "Approach ice 2 of 3", "Encounter ice 2 of 3", "Access", "Run over"]);
        assert_eq!(now(segment), ["Run over"]);
    }

    #[test]
    fn an_open_window_says_who_is_holding_the_game() {
        let mut view = view();
        view.phase = GamePhase::Action(Side::Runner);
        view.paid_ability_window = Some(netrunner_core::rules::PaidAbilityWindow {
            active_priority: Side::Corp,
            consecutive_passes: 0,
            checkpoint: WindowCheckpoint::Run,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
        });
        assert_eq!(bar(&view).note.as_deref(), Some("Paid ability window during the run — the Corp has priority"));
    }

    fn ice() -> netrunner_core::rules::PublicRunIce {
        netrunner_core::rules::PublicRunIce { install_id: netrunner_core::rules::InstallId(0), rezzed: false, identity: None }
    }
}
