//! The timing of the turn and the run as the rules chart it, with where
//! the game is lit (Phase 7 §8 item 10, jinteki.net's timing diagrams).
//!
//! **The phase bar ([`super::phase`]) is coarse on purpose**: three steps
//! a turn and five a run, because a bar that grew a step per window would
//! be a different bar on every piece of ice. That leaves the question a
//! person learning the game asks unanswered — *why can the Corp rez now
//! and not a moment later?* — and the answer is a window, which is exactly
//! what the bar leaves out. This is the other surface: the whole chart,
//! every step and every window, opened when wanted and closed again.
//!
//! **The source is the Comprehensive Rules' own appendix** (CR 11.1.1,
//! the timing structure reference: the Corp's turn, the Runner's, a run, a
//! breach, an access), with each window's (P), (R) and (S) tags. **The
//! words are ours.** The rules are Null Signal Games' text, committed for
//! implementing the rules and nothing more (`rules/NOTICE.md`), so the
//! client never shows a sentence of it: each step is a few words of this
//! project's and the number of the rule it stands for, and every number
//! here is a `CR` citation the `rules_citations` gate holds to the
//! committed copy.
//!
//! **Every step is always listed** — the phase bar's rule, for its reason
//! — and more than one can be lit: a run happens inside the turn's "take
//! an action", and a breach inside the run's success. Read off the masked
//! [`ClientView`] only (`phase`, `active_run`, `paid_ability_window`), as
//! the phase bar is; it reports where the game is and never decides it.

use netrunner_core::rules::{GamePhase, PublicAccessPhase, RunPhase, Side, WindowCheckpoint};
use netrunner_core::view::ClientView;

/// Which of the rules' three permissions a window gives: paid abilities
/// (P), the Corp rezzing (R), the Corp scoring (S).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Windows {
    pub paid: bool,
    pub rez: bool,
    pub score: bool,
}

/// One step of a chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartStep {
    /// The step's letter or number in the rules ("b", "4").
    pub label: &'static str,
    /// What happens, in this project's words.
    pub words: &'static str,
    /// The window this step is, if it is one.
    pub windows: Option<Windows>,
    /// The rule the step stands for, as "CR 5.6.1b".
    pub cr: &'static str,
    /// Whether the game is at this step now.
    pub lit: bool,
}

/// A phase of a chart: its heading and its steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartPhase {
    /// "Draw phase", "Approach ice · 2 of 3"; empty for a chart of one
    /// phase (a breach, an access).
    pub title: String,
    pub steps: Vec<ChartStep>,
}

/// One of the rules' timing structures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chart {
    /// "Corp turn", "Run", "Breach", "Access".
    pub title: &'static str,
    /// The section the chart summarises, as "CR 5.6".
    pub cr: &'static str,
    pub phases: Vec<ChartPhase>,
}

impl Chart {
    /// Whether any step of this chart is lit.
    pub fn lit(&self) -> bool {
        self.phases.iter().flat_map(|phase| &phase.steps).any(|step| step.lit)
    }
}

/// The charts for where the game is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timing {
    pub charts: Vec<Chart>,
    /// One line said once: why a window on the chart may pass without the
    /// game stopping in it.
    pub note: &'static str,
}

/// The engine asks nobody about a window nobody could act in (Rules
/// Conformance D3, D4), so a chart's window is often passed without a
/// stop, and a person watching the lit step jump over one should know it
/// was not skipped.
const NOTE: &str = "A window opens for a player only when they could act in it; the rest pass without a stop.";

const P: Option<Windows> = Some(Windows { paid: true, rez: false, score: false });
const PR: Option<Windows> = Some(Windows { paid: true, rez: true, score: false });
const PRS: Option<Windows> = Some(Windows { paid: true, rez: true, score: true });

/// A step before it is lit.
type Row = (&'static str, &'static str, Option<Windows>, &'static str);

const CORP_TURN: &[(&str, &[Row])] = &[
    (
        "Draw phase",
        &[
            ("a", "Gain clicks", None, "CR 5.6.1a"),
            ("b", "Window", PRS, "CR 5.6.1b"),
            ("c", "Recurring credits refill", None, "CR 5.6.1c"),
            ("d", "The turn begins", None, "CR 5.6.1d"),
            ("e", "Draw a card", None, "CR 5.6.1e"),
        ],
    ),
    (
        "Action phase",
        &[
            ("a", "Window", PRS, "CR 5.6.2a"),
            ("b", "Take an action, while clicks remain", None, "CR 5.6.2b"),
            ("c", "Back to (a)", None, "CR 5.6.2c"),
            ("d", "The action phase ends", None, "CR 5.6.2d"),
        ],
    ),
    (
        "Discard phase",
        &[
            ("a", "Discard to hand size", None, "CR 5.6.3a"),
            ("b", "Window", PR, "CR 5.6.3b"),
            ("c", "Lose unspent clicks", None, "CR 5.6.3c"),
            ("d", "The turn ends", None, "CR 5.6.3d"),
        ],
    ),
];

const RUNNER_TURN: &[(&str, &[Row])] = &[
    (
        "Action phase",
        &[
            ("a", "Gain clicks", None, "CR 5.7.1a"),
            ("b", "Window", PR, "CR 5.7.1b"),
            ("c", "Recurring credits refill", None, "CR 5.7.1c"),
            ("d", "The turn begins", None, "CR 5.7.1d"),
            ("e", "Window", PR, "CR 5.7.1e"),
            ("f", "Take an action, while clicks remain", None, "CR 5.7.1f"),
            ("g", "Back to (e)", None, "CR 5.7.1g"),
            ("h", "The action phase ends", None, "CR 5.7.1h"),
        ],
    ),
    (
        "Discard phase",
        &[
            ("a", "Discard to hand size", None, "CR 5.7.2a"),
            ("b", "Window", PR, "CR 5.7.2b"),
            ("c", "Lose unspent clicks", None, "CR 5.7.2c"),
            ("d", "The turn ends", None, "CR 5.7.2d"),
        ],
    ),
];

const RUN: &[(&str, &[Row])] = &[
    (
        "Initiation",
        &[
            ("a", "Name the server", None, "CR 6.9.1a"),
            ("b", "Take bad publicity credits", None, "CR 6.9.1b"),
            ("c", "The run begins", None, "CR 6.9.1c"),
            ("d", "Start at the outermost ice", None, "CR 6.9.1d"),
            ("e", "Window", PR, "CR 6.9.1e"),
            ("f", "Ice here? Approach it; else movement", None, "CR 6.9.1f"),
        ],
    ),
    (
        "Approach ice",
        &[
            ("a", "Approach the ice", None, "CR 6.9.2a"),
            ("b", "Window — the Corp may rez this ice", PR, "CR 6.9.2b"),
            ("c", "Rezzed? Encounter it; else movement", None, "CR 6.9.2c"),
        ],
    ),
    (
        "Encounter ice",
        &[
            ("a", "Encounter the ice", None, "CR 6.9.3a"),
            ("b", "Window — break subroutines", P, "CR 6.9.3b"),
            ("c", "Resolve the next unbroken subroutine", None, "CR 6.9.3c"),
            ("d", "Back to (c)", None, "CR 6.9.3d"),
        ],
    ),
    (
        "Movement",
        &[
            ("a", "Pass the ice", None, "CR 6.9.4a"),
            ("b", "Window", P, "CR 6.9.4b"),
            ("c", "The Runner may jack out", None, "CR 6.9.4c"),
            ("d", "Move one position inward", None, "CR 6.9.4d"),
            ("e", "Window", PR, "CR 6.9.4e"),
            ("f", "More ice? Approach it", None, "CR 6.9.4f"),
            ("g", "Approach the server", None, "CR 6.9.4g"),
        ],
    ),
    (
        "Success",
        &[("a", "The run is successful", None, "CR 6.9.5a"), ("b", "Breach the server", None, "CR 6.9.5b")],
    ),
    (
        "Run ends",
        &[
            ("a", "Close open windows", None, "CR 6.9.6a"),
            ("b", "Bad publicity credits return", None, "CR 6.9.6b"),
            ("c", "Unsuccessful, unless it succeeded", None, "CR 6.9.6c"),
            ("d", "The run is over", None, "CR 6.9.6d"),
        ],
    ),
];

const BREACH: &[(&str, &[Row])] = &[(
    "",
    &[
        ("1", "The breach begins", None, "CR 7.5.1"),
        ("2", "Archives turns faceup", None, "CR 7.5.2"),
        ("3", "How many cards to access", None, "CR 7.5.3"),
        ("4", "Choose the next card to access", None, "CR 7.5.4"),
        ("5", "Access it", None, "CR 7.5.5"),
        ("6", "Back to (4)", None, "CR 7.5.6"),
        ("7", "The breach is over", None, "CR 7.5.7"),
    ],
)];

const ACCESS: &[(&str, &[Row])] = &[(
    "",
    &[
        ("1", "The card is accessed", None, "CR 7.2.1"),
        ("2", "One mid-access ability, such as a trash", None, "CR 7.2.2"),
        ("3", "An agenda is stolen", None, "CR 7.2.3"),
        ("4", "The access is over", None, "CR 7.2.4"),
    ],
)];

/// The charts for `view`: the active side's turn always, and on the
/// Runner's turn the run, the breach and the access below it, lit where
/// the game is. The Corp's turn has no run chart: runs are the Runner's.
pub fn timing(view: &ClientView) -> Timing {
    let lit = lit(view);
    let build = |title: &'static str, cr: &'static str, table: &[(&str, &[Row])], headings: &dyn Fn(&str) -> String| Chart {
        title,
        cr,
        phases: table
            .iter()
            .map(|(phase, rows)| ChartPhase {
                title: headings(phase),
                steps: rows.iter().map(|&(label, words, windows, cr)| ChartStep { label, words, windows, cr, lit: lit.contains(&cr) }).collect(),
            })
            .collect(),
    };
    let plain = |phase: &str| phase.to_string();
    let side = match view.phase {
        GamePhase::Mulligan(side) | GamePhase::StartOfTurn(side) | GamePhase::Action(side) | GamePhase::GameOver(side) => side,
        GamePhase::Discard { side, .. } => side,
    };
    // The mulligan and the end of the game belong to no turn; the chart of
    // whoever is active is still worth reading, with nothing lit.
    let side = if matches!(view.phase, GamePhase::Mulligan(_) | GamePhase::GameOver(_)) { view.active_player } else { side };
    let mut charts = Vec::new();
    match side {
        Side::Corp => charts.push(build("Corp turn", "CR 5.6", CORP_TURN, &plain)),
        Side::Runner => {
            charts.push(build("Runner turn", "CR 5.7", RUNNER_TURN, &plain));
            // The ice being met, counted as the phase bar counts it.
            let count = view.active_run.as_ref().filter(|run| !run.ice.is_empty()).map(|run| (run.position.min(run.ice.len() - 1) + 1, run.ice.len()));
            let ice = |phase: &str| match (phase, count) {
                ("Approach ice" | "Encounter ice", Some((n, of))) => format!("{phase} · {n} of {of}"),
                _ => phase.to_string(),
            };
            charts.push(build("Run", "CR 6.9", RUN, &ice));
            charts.push(build("Breach", "CR 7.5", BREACH, &plain));
            charts.push(build("Access", "CR 7.2", ACCESS, &plain));
        }
    }
    Timing { charts, note: NOTE }
}

/// The steps the game is at, by their citations.
fn lit(view: &ClientView) -> Vec<&'static str> {
    let window = view.paid_ability_window.as_ref().map(|window| window.checkpoint);
    let mut lit = Vec::new();
    let turn = match (&view.phase, &window) {
        (GamePhase::Mulligan(_) | GamePhase::GameOver(_), _) => None,
        (_, Some(WindowCheckpoint::TurnBeginning { side })) => Some(pick(*side, "CR 5.6.1b", "CR 5.7.1b")),
        (_, Some(WindowCheckpoint::StartOfTurn { side })) => Some(pick(*side, "CR 5.6.2a", "CR 5.7.1e")),
        (_, Some(WindowCheckpoint::EndOfTurn { side })) => Some(pick(*side, "CR 5.6.3b", "CR 5.7.2b")),
        // The window after an action is the loop's: back to its top.
        (GamePhase::Action(side), Some(WindowCheckpoint::PostAction { .. })) => Some(pick(*side, "CR 5.6.2a", "CR 5.7.1e")),
        (GamePhase::StartOfTurn(side), _) => Some(pick(*side, "CR 5.6.1d", "CR 5.7.1d")),
        (GamePhase::Discard { side, .. }, _) => Some(pick(*side, "CR 5.6.3a", "CR 5.7.2a")),
        // No clicks left and nothing under way: the rules skip to the
        // action phase's end (CR 5.6.2b, 5.7.1f's "otherwise"), which is
        // where the game waits for End turn.
        (GamePhase::Action(side), None) if view.active_run.is_none() && clicks(view, *side) == 0 => Some(pick(*side, "CR 5.6.2d", "CR 5.7.1h")),
        // An action is under way, a run inside it or a decision it asked.
        (GamePhase::Action(side), _) => Some(pick(*side, "CR 5.6.2b", "CR 5.7.1f")),
    };
    lit.extend(turn);
    let Some(run) = &view.active_run else { return lit };
    let in_window = window.is_some();
    lit.push(match run.phase {
        RunPhase::Initiation => "CR 6.9.1e",
        RunPhase::ApproachIce => "CR 6.9.2b",
        RunPhase::EncounterIce if in_window => "CR 6.9.3b",
        RunPhase::EncounterIce => "CR 6.9.3c",
        RunPhase::Movement if !run.jack_out_permitted => "CR 6.9.4e",
        RunPhase::Movement if in_window => "CR 6.9.4b",
        RunPhase::Movement => "CR 6.9.4c",
        RunPhase::Success => "CR 6.9.5a",
        RunPhase::AccessingCard => "CR 6.9.5b",
        RunPhase::Ended => "CR 6.9.6d",
    });
    if run.phase == RunPhase::AccessingCard
        && let Some(access) = &run.access_state
    {
        match access.phase {
            PublicAccessPhase::SelectNextCard { .. } => lit.push("CR 7.5.4"),
            PublicAccessPhase::PendingInteractiveTrigger { .. } => lit.extend(["CR 7.5.5", "CR 7.2.1"]),
            PublicAccessPhase::PendingChoice { .. } => lit.extend(["CR 7.5.5", "CR 7.2.2"]),
        }
    }
    lit
}

fn clicks(view: &ClientView, side: Side) -> u32 {
    match side {
        Side::Corp => view.corp.clicks,
        Side::Runner => view.runner.clicks,
    }
}

fn pick(side: Side, corp: &'static str, runner: &'static str) -> &'static str {
    match side {
        Side::Corp => corp,
        Side::Runner => runner,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{GameState, InstallId, MaskedZone, PaidAbilityWindow, PublicAccessState, PublicRunIce, PublicRunState, ServerId};
    use netrunner_session::{sweep_decks_for_seed, Seat, Session};

    fn view() -> ClientView {
        let registry = crate::decks::sample_deck_registry();
        let (corp_deck, runner_deck) = sweep_decks_for_seed(0);
        let (state, _) = GameState::setup(&corp_deck.to_deck(), &runner_deck.to_deck(), &registry, 0).unwrap();
        Session::new(state, registry, Seat::External, Seat::External).view_for(Side::Corp)
    }

    fn window(checkpoint: WindowCheckpoint, phase: GamePhase) -> Option<PaidAbilityWindow> {
        Some(PaidAbilityWindow { active_priority: Side::Corp, consecutive_passes: 0, checkpoint, return_phase: Box::new(phase) })
    }

    fn run(phase: RunPhase) -> PublicRunState {
        let ice = PublicRunIce { install_id: InstallId(0), rezzed: false, identity: None };
        PublicRunState {
            server: ServerId::Hq,
            phase,
            ice: vec![ice.clone(), ice],
            position: 1,
            access_state: None,
            jack_out_permitted: true,
            declared_successful: false,
            bad_publicity_credits: 0,
            bonus_run_credits: 0,
            redirect_on_approach: None,
        }
    }

    /// The lit steps' citations, chart by chart.
    fn lit_steps(view: &ClientView) -> Vec<&'static str> {
        timing(view).charts.iter().flat_map(|c| &c.phases).flat_map(|p| &p.steps).filter(|s| s.lit).map(|s| s.cr).collect()
    }

    #[test]
    fn each_turn_window_lights_its_own_step() {
        let mut view = view();
        for (side, phase, checkpoint, expected) in [
            (Side::Corp, GamePhase::StartOfTurn(Side::Corp), WindowCheckpoint::TurnBeginning { side: Side::Corp }, "CR 5.6.1b"),
            (Side::Runner, GamePhase::StartOfTurn(Side::Runner), WindowCheckpoint::TurnBeginning { side: Side::Runner }, "CR 5.7.1b"),
            (Side::Corp, GamePhase::StartOfTurn(Side::Corp), WindowCheckpoint::StartOfTurn { side: Side::Corp }, "CR 5.6.2a"),
            (Side::Runner, GamePhase::StartOfTurn(Side::Runner), WindowCheckpoint::StartOfTurn { side: Side::Runner }, "CR 5.7.1e"),
            (Side::Corp, GamePhase::Action(Side::Corp), WindowCheckpoint::PostAction { side: Side::Corp }, "CR 5.6.2a"),
            (Side::Runner, GamePhase::Action(Side::Runner), WindowCheckpoint::PostAction { side: Side::Runner }, "CR 5.7.1e"),
            // The discard phase's window runs with the phase back at
            // Action (`turn.rs`), so the checkpoint is what says where.
            (Side::Corp, GamePhase::Action(Side::Corp), WindowCheckpoint::EndOfTurn { side: Side::Corp }, "CR 5.6.3b"),
            (Side::Runner, GamePhase::Action(Side::Runner), WindowCheckpoint::EndOfTurn { side: Side::Runner }, "CR 5.7.2b"),
        ] {
            view.active_player = side;
            view.phase = phase;
            view.paid_ability_window = window(checkpoint, phase);
            assert_eq!(lit_steps(&view), [expected], "{checkpoint:?}");
        }
        view.paid_ability_window = None;
        view.phase = GamePhase::Action(Side::Corp);
        view.corp.clicks = 2;
        assert_eq!(lit_steps(&view), ["CR 5.6.2b"], "an action, no window");
        view.corp.clicks = 0;
        assert_eq!(lit_steps(&view), ["CR 5.6.2d"], "no clicks left: the action phase is ending");
        view.phase = GamePhase::Discard { side: Side::Runner, required: 1 };
        assert_eq!(lit_steps(&view), ["CR 5.7.2a"]);
        view.phase = GamePhase::StartOfTurn(Side::Corp);
        assert_eq!(lit_steps(&view), ["CR 5.6.1d"]);
        view.phase = GamePhase::GameOver(Side::Corp);
        assert!(lit_steps(&view).is_empty(), "nothing is lit once the game is over");
    }

    #[test]
    fn the_corps_turn_shows_no_run_chart() {
        let mut view = view();
        view.phase = GamePhase::Action(Side::Corp);
        assert_eq!(timing(&view).charts.iter().map(|c| c.title).collect::<Vec<_>>(), ["Corp turn"]);
        view.phase = GamePhase::Action(Side::Runner);
        assert_eq!(timing(&view).charts.iter().map(|c| c.title).collect::<Vec<_>>(), ["Runner turn", "Run", "Breach", "Access"]);
    }

    #[test]
    fn a_run_lights_its_step_and_the_turns_action_step_together() {
        let mut view = view();
        view.phase = GamePhase::Action(Side::Runner);
        view.runner.clicks = 0;
        let open = window(WindowCheckpoint::Run, GamePhase::Action(Side::Runner));
        for (phase, jack_out, in_window, expected) in [
            (RunPhase::Initiation, true, true, "CR 6.9.1e"),
            (RunPhase::ApproachIce, true, true, "CR 6.9.2b"),
            (RunPhase::EncounterIce, true, true, "CR 6.9.3b"),
            (RunPhase::EncounterIce, true, false, "CR 6.9.3c"),
            (RunPhase::Movement, true, true, "CR 6.9.4b"),
            (RunPhase::Movement, true, false, "CR 6.9.4c"),
            (RunPhase::Movement, false, true, "CR 6.9.4e"),
            (RunPhase::Success, true, false, "CR 6.9.5a"),
            (RunPhase::Ended, true, false, "CR 6.9.6d"),
        ] {
            let mut run = run(phase);
            run.jack_out_permitted = jack_out;
            view.active_run = Some(run);
            view.paid_ability_window = if in_window { open.clone() } else { None };
            assert_eq!(lit_steps(&view), ["CR 5.7.1f", expected], "{phase:?}, jack out {jack_out}, window {in_window}");
        }
        view.active_run = Some(run(RunPhase::ApproachIce));
        let run_chart = timing(&view).charts.into_iter().find(|c| c.title == "Run").unwrap();
        assert_eq!(run_chart.phases[1].title, "Approach ice · 2 of 2", "the ice is counted as the phase bar counts it");
    }

    #[test]
    fn a_breach_and_an_access_light_their_charts() {
        let mut view = view();
        view.phase = GamePhase::Action(Side::Runner);
        let mut breaching = run(RunPhase::AccessingCard);
        let access = |phase| PublicAccessState { server: ServerId::Hq, candidates: Vec::new(), from_zone: 0, resolved_cards: MaskedZone::Hidden { count: 0 }, pending_install: None, phase };
        breaching.access_state = Some(access(PublicAccessPhase::SelectNextCard { selectable_cards: Vec::new() }));
        view.active_run = Some(breaching.clone());
        assert_eq!(lit_steps(&view), ["CR 5.7.1f", "CR 6.9.5b", "CR 7.5.4"]);
        breaching.access_state = Some(access(PublicAccessPhase::PendingChoice { card: None, trash_cost: None, mandatory_steal: false, steal_cost: None }));
        view.active_run = Some(breaching);
        let timing = timing(&view);
        assert_eq!(timing.charts.iter().filter(|c| c.lit()).map(|c| c.title).collect::<Vec<_>>(), ["Runner turn", "Run", "Breach", "Access"]);
        assert_eq!(lit_steps(&view), ["CR 5.7.1f", "CR 6.9.5b", "CR 7.5.5", "CR 7.2.2"]);
    }

    /// Every window on the charts says what it permits, and none but the
    /// Corp's own action-phase windows lets it score (the rules' (S)).
    #[test]
    fn only_the_corps_draw_and_action_windows_score() {
        let mut view = view();
        let mut scoring = Vec::new();
        for side in [Side::Corp, Side::Runner] {
            view.phase = GamePhase::Action(side);
            for chart in timing(&view).charts {
                for step in chart.phases.iter().flat_map(|p| &p.steps) {
                    assert_eq!(step.windows.is_some(), step.words.starts_with("Window"), "{}", step.cr);
                    if step.windows.is_some_and(|w| w.score) {
                        scoring.push(step.cr);
                    }
                }
            }
        }
        assert_eq!(scoring, ["CR 5.6.1b", "CR 5.6.2a"]);
    }
}
