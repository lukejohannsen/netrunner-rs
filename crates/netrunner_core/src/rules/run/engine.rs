use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::action::RunAction;
use crate::rules::run::state::{RunIce, RunPhase, RunState};

fn phase_for_position(ice: &[RunIce], position: usize) -> RunPhase {
    if position < ice.len() {
        RunPhase::ApproachIce
    } else {
        RunPhase::Success
    }
}

pub fn advance_run(
    run: &RunState,
    action: RunAction,
) -> Result<(RunState, Vec<GameEvent>), RulesError> {
    if matches!(run.phase, RunPhase::Success | RunPhase::Ended) {
        return Err(RulesError::RunAlreadyConcluded { phase: run.phase });
    }

    match action {
        RunAction::JackOut => Ok(jack_out(run)),
        RunAction::Continue => continue_run(run),
        RunAction::ResolveSubroutine => step_subroutine(run, true),
        RunAction::BreakSubroutine => step_subroutine(run, false),
    }
}

// Real NISEI rules restrict exactly when a Runner may jack out; this engine
// deliberately allows it unconditionally from any non-terminal phase, even
// mid-EncounterIce with subroutines still pending. Refining jack-out
// legality windows is future work.
fn jack_out(run: &RunState) -> (RunState, Vec<GameEvent>) {
    let mut next = run.clone();
    next.phase = RunPhase::Ended;
    (next, vec![GameEvent::RunJackedOut { server: run.server }])
}

fn continue_run(run: &RunState) -> Result<(RunState, Vec<GameEvent>), RulesError> {
    let mut next = run.clone();

    match next.phase {
        RunPhase::Initiation => {
            next.phase = phase_for_position(&next.ice, 0);
            if next.phase == RunPhase::Success {
                Ok((next, vec![GameEvent::RunSucceeded { server: run.server }]))
            } else {
                Ok((
                    next,
                    vec![GameEvent::IceApproached {
                        server: run.server,
                        position: 0,
                    }],
                ))
            }
        }
        RunPhase::ApproachIce => {
            let position = next.position;
            next.phase = RunPhase::EncounterIce;
            Ok((
                next,
                vec![GameEvent::IceEncountered {
                    server: run.server,
                    position: position as u32,
                }],
            ))
        }
        RunPhase::EncounterIce => {
            let position = next.position;
            let pending = next
                .ice
                .get(position)
                .map(|ice| ice.subroutines_pending)
                .unwrap_or(0);
            if pending > 0 {
                return Err(RulesError::SubroutinesStillPending { pending });
            }

            let mut events = vec![GameEvent::IcePassed {
                server: run.server,
                position: position as u32,
            }];
            next.position += 1;
            next.phase = phase_for_position(&next.ice, next.position);
            match next.phase {
                RunPhase::ApproachIce => events.push(GameEvent::IceApproached {
                    server: run.server,
                    position: next.position as u32,
                }),
                RunPhase::Success => events.push(GameEvent::RunSucceeded { server: run.server }),
                _ => {}
            }
            Ok((next, events))
        }
        RunPhase::Success | RunPhase::Ended => {
            Err(RulesError::RunAlreadyConcluded { phase: next.phase })
        }
    }
}

fn step_subroutine(
    run: &RunState,
    resolve: bool,
) -> Result<(RunState, Vec<GameEvent>), RulesError> {
    if run.phase != RunPhase::EncounterIce {
        return Err(RulesError::NoSubroutinesPending);
    }

    let mut next = run.clone();
    let position = next.position;
    let pending = next
        .ice
        .get(position)
        .map(|ice| ice.subroutines_pending)
        .unwrap_or(0);
    if pending == 0 {
        return Err(RulesError::NoSubroutinesPending);
    }

    let remaining = pending - 1;
    next.ice[position].subroutines_pending = remaining;

    let event = if resolve {
        GameEvent::SubroutineResolved {
            server: run.server,
            position: position as u32,
            remaining,
        }
    } else {
        GameEvent::SubroutineBroken {
            server: run.server,
            position: position as u32,
            remaining,
        }
    };
    Ok((next, vec![event]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::run::state::ServerId;

    fn run_state(phase: RunPhase, ice: Vec<RunIce>, position: usize) -> RunState {
        RunState {
            server: ServerId::Hq,
            phase,
            ice,
            position,
        }
    }

    #[test]
    fn initiation_continue_with_ice_enters_approach_ice() {
        let run = run_state(RunPhase::Initiation, vec![RunIce { subroutines_pending: 2 }], 0);
        let (next, events) = advance_run(&run, RunAction::Continue).expect("should succeed");

        assert_eq!(next.phase, RunPhase::ApproachIce);
        assert_eq!(next.position, 0);
        assert_eq!(
            events,
            vec![GameEvent::IceApproached { server: ServerId::Hq, position: 0 }]
        );
    }

    #[test]
    fn initiation_continue_with_no_ice_is_immediate_success() {
        let run = run_state(RunPhase::Initiation, vec![], 0);
        let (next, events) = advance_run(&run, RunAction::Continue).expect("should succeed");

        assert_eq!(next.phase, RunPhase::Success);
        assert_eq!(events, vec![GameEvent::RunSucceeded { server: ServerId::Hq }]);
    }

    #[test]
    fn approach_ice_continue_enters_encounter_ice() {
        let run = run_state(RunPhase::ApproachIce, vec![RunIce { subroutines_pending: 2 }], 0);
        let (next, events) = advance_run(&run, RunAction::Continue).expect("should succeed");

        assert_eq!(next.phase, RunPhase::EncounterIce);
        assert_eq!(
            events,
            vec![GameEvent::IceEncountered { server: ServerId::Hq, position: 0 }]
        );
    }

    #[test]
    fn encounter_ice_resolve_subroutine_decrements_pending() {
        let run = run_state(RunPhase::EncounterIce, vec![RunIce { subroutines_pending: 2 }], 0);
        let (next, events) =
            advance_run(&run, RunAction::ResolveSubroutine).expect("should succeed");

        assert_eq!(next.ice[0].subroutines_pending, 1);
        assert_eq!(next.phase, RunPhase::EncounterIce);
        assert_eq!(
            events,
            vec![GameEvent::SubroutineResolved {
                server: ServerId::Hq,
                position: 0,
                remaining: 1
            }]
        );
    }

    #[test]
    fn encounter_ice_break_subroutine_decrements_pending() {
        let run = run_state(RunPhase::EncounterIce, vec![RunIce { subroutines_pending: 2 }], 0);
        let (next, events) =
            advance_run(&run, RunAction::BreakSubroutine).expect("should succeed");

        assert_eq!(next.ice[0].subroutines_pending, 1);
        assert_eq!(
            events,
            vec![GameEvent::SubroutineBroken {
                server: ServerId::Hq,
                position: 0,
                remaining: 1
            }]
        );
    }

    #[test]
    fn encounter_ice_continue_with_pending_subroutines_errors() {
        let run = run_state(RunPhase::EncounterIce, vec![RunIce { subroutines_pending: 1 }], 0);
        let result = advance_run(&run, RunAction::Continue);

        assert_eq!(result, Err(RulesError::SubroutinesStillPending { pending: 1 }));
    }

    #[test]
    fn encounter_ice_continue_with_no_pending_passes_to_next_ice() {
        let run = run_state(
            RunPhase::EncounterIce,
            vec![
                RunIce { subroutines_pending: 0 },
                RunIce { subroutines_pending: 3 },
            ],
            0,
        );
        let (next, events) = advance_run(&run, RunAction::Continue).expect("should succeed");

        assert_eq!(next.phase, RunPhase::ApproachIce);
        assert_eq!(next.position, 1);
        assert_eq!(
            events,
            vec![
                GameEvent::IcePassed { server: ServerId::Hq, position: 0 },
                GameEvent::IceApproached { server: ServerId::Hq, position: 1 },
            ]
        );
    }

    #[test]
    fn encounter_ice_continue_after_last_ice_reaches_success() {
        let run = run_state(RunPhase::EncounterIce, vec![RunIce { subroutines_pending: 0 }], 0);
        let (next, events) = advance_run(&run, RunAction::Continue).expect("should succeed");

        assert_eq!(next.phase, RunPhase::Success);
        assert_eq!(
            events,
            vec![
                GameEvent::IcePassed { server: ServerId::Hq, position: 0 },
                GameEvent::RunSucceeded { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn resolve_subroutine_with_none_pending_errors() {
        let run = run_state(RunPhase::EncounterIce, vec![RunIce { subroutines_pending: 0 }], 0);
        let result = advance_run(&run, RunAction::ResolveSubroutine);

        assert_eq!(result, Err(RulesError::NoSubroutinesPending));
    }

    #[test]
    fn break_subroutine_outside_encounter_ice_errors() {
        let run = run_state(RunPhase::ApproachIce, vec![RunIce { subroutines_pending: 2 }], 0);
        let result = advance_run(&run, RunAction::BreakSubroutine);

        assert_eq!(result, Err(RulesError::NoSubroutinesPending));
    }

    #[test]
    fn jack_out_from_initiation_ends_run() {
        let run = run_state(RunPhase::Initiation, vec![], 0);
        let (next, events) = advance_run(&run, RunAction::JackOut).expect("should succeed");

        assert_eq!(next.phase, RunPhase::Ended);
        assert_eq!(events, vec![GameEvent::RunJackedOut { server: ServerId::Hq }]);
    }

    #[test]
    fn jack_out_from_approach_ice_ends_run() {
        let run = run_state(RunPhase::ApproachIce, vec![RunIce { subroutines_pending: 1 }], 0);
        let (next, _events) = advance_run(&run, RunAction::JackOut).expect("should succeed");

        assert_eq!(next.phase, RunPhase::Ended);
    }

    #[test]
    fn jack_out_from_encounter_ice_ends_run_even_with_pending_subroutines() {
        let run = run_state(RunPhase::EncounterIce, vec![RunIce { subroutines_pending: 5 }], 0);
        let (next, events) = advance_run(&run, RunAction::JackOut).expect("should succeed");

        assert_eq!(next.phase, RunPhase::Ended);
        assert_eq!(events, vec![GameEvent::RunJackedOut { server: ServerId::Hq }]);
    }

    #[test]
    fn continue_after_success_errors() {
        let run = run_state(RunPhase::Success, vec![], 0);
        let result = advance_run(&run, RunAction::Continue);

        assert_eq!(
            result,
            Err(RulesError::RunAlreadyConcluded { phase: RunPhase::Success })
        );
    }

    #[test]
    fn jack_out_after_success_errors() {
        let run = run_state(RunPhase::Success, vec![], 0);
        let result = advance_run(&run, RunAction::JackOut);

        assert_eq!(
            result,
            Err(RulesError::RunAlreadyConcluded { phase: RunPhase::Success })
        );
    }

    #[test]
    fn action_after_ended_errors() {
        let run = run_state(RunPhase::Ended, vec![], 0);
        let result = advance_run(&run, RunAction::Continue);

        assert_eq!(
            result,
            Err(RulesError::RunAlreadyConcluded { phase: RunPhase::Ended })
        );
    }

    #[test]
    fn advance_run_does_not_mutate_original_run_state() {
        let run = run_state(RunPhase::Initiation, vec![RunIce { subroutines_pending: 1 }], 0);
        let _ = advance_run(&run, RunAction::Continue).expect("should succeed");

        assert_eq!(run.phase, RunPhase::Initiation);
    }
}
