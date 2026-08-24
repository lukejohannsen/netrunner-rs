use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::action::RunAction;
use crate::rules::run::state::{RunIce, RunPhase, RunState, SubroutineStatus};

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
        RunAction::ResolveSubroutine(index) => step_subroutine(run, index, true),
        RunAction::BreakSubroutine(index) => step_subroutine(run, index, false),
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
            let ice = &next.ice[position];
            let event = GameEvent::IceEncountered {
                card_id: ice.card_id.clone(),
                strength: ice.current_strength,
                subroutine_count: ice.subroutines.len(),
            };
            Ok((next, vec![event]))
        }
        RunPhase::EncounterIce => {
            let position = next.position;
            let all_handled = next
                .ice
                .get(position)
                .map(|ice| ice.subroutines.iter().all(|s| s.status != SubroutineStatus::Pending))
                .unwrap_or(true);
            if !all_handled {
                let pending = next.ice[position]
                    .subroutines
                    .iter()
                    .filter(|s| s.status == SubroutineStatus::Pending)
                    .count() as u32;
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
    index: usize,
    resolve: bool,
) -> Result<(RunState, Vec<GameEvent>), RulesError> {
    if run.phase != RunPhase::EncounterIce {
        return Err(RulesError::NotInEncounter);
    }

    let mut next = run.clone();
    let position = next.position;
    let ice = next.ice.get_mut(position).ok_or(RulesError::InvalidSubroutineIndex(index))?;
    let card_id = ice.card_id.clone();
    let subroutine = ice
        .subroutines
        .get_mut(index)
        .ok_or(RulesError::InvalidSubroutineIndex(index))?;

    if subroutine.status != SubroutineStatus::Pending {
        return Err(RulesError::SubroutineAlreadyHandled);
    }

    let event = if resolve {
        let effect = subroutine.definition.effect.clone();
        subroutine.status = SubroutineStatus::Resolved;
        GameEvent::SubroutineFired { card_id, index, effect }
    } else {
        subroutine.status = SubroutineStatus::Broken;
        GameEvent::SubroutineBroken { card_id, index }
    };
    Ok((next, vec![event]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardId, Effect, SubroutineDef};
    use crate::rules::run::state::{EncounteredSubroutine, ServerId};

    fn run_state(phase: RunPhase, ice: Vec<RunIce>, position: usize) -> RunState {
        RunState {
            server: ServerId::Hq,
            phase,
            ice,
            position,
        }
    }

    /// Builds a `RunIce` with `subroutine_count` placeholder `Pending`
    /// subroutines — identity/effect content doesn't matter for tests using
    /// this, only status transitions and counts do.
    fn test_ice(card_id: &str, strength: i32, subroutine_count: usize) -> RunIce {
        RunIce {
            card_id: CardId(card_id.to_string()),
            current_strength: strength,
            subroutines: (0..subroutine_count)
                .map(|id| EncounteredSubroutine {
                    id,
                    definition: SubroutineDef {
                        text: format!("Subroutine {id}"),
                        effect: Effect::EndTheRun,
                    },
                    status: SubroutineStatus::Pending,
                })
                .collect(),
        }
    }

    #[test]
    fn initiation_continue_with_ice_enters_approach_ice() {
        let run = run_state(RunPhase::Initiation, vec![test_ice("ice_wall", 0, 2)], 0);
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
        let run = run_state(RunPhase::ApproachIce, vec![test_ice("ice_wall", 3, 2)], 0);
        let (next, events) = advance_run(&run, RunAction::Continue).expect("should succeed");

        assert_eq!(next.phase, RunPhase::EncounterIce);
        assert_eq!(
            events,
            vec![GameEvent::IceEncountered {
                card_id: CardId("ice_wall".to_string()),
                strength: 3,
                subroutine_count: 2,
            }]
        );
    }

    #[test]
    fn encounter_ice_resolve_subroutine_fires_and_marks_resolved() {
        let run = run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 2)], 0);
        let (next, events) =
            advance_run(&run, RunAction::ResolveSubroutine(0)).expect("should succeed");

        assert_eq!(next.ice[0].subroutines[0].status, SubroutineStatus::Resolved);
        assert_eq!(next.ice[0].subroutines[1].status, SubroutineStatus::Pending);
        assert_eq!(next.phase, RunPhase::EncounterIce);
        assert_eq!(
            events,
            vec![GameEvent::SubroutineFired {
                card_id: CardId("ice_wall".to_string()),
                index: 0,
                effect: Effect::EndTheRun,
            }]
        );
    }

    #[test]
    fn encounter_ice_break_subroutine_marks_broken() {
        let run = run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 2)], 0);
        let (next, events) =
            advance_run(&run, RunAction::BreakSubroutine(0)).expect("should succeed");

        assert_eq!(next.ice[0].subroutines[0].status, SubroutineStatus::Broken);
        assert_eq!(
            events,
            vec![GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0 }]
        );
    }

    #[test]
    fn encounter_ice_continue_with_pending_subroutines_errors() {
        let run = run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 1)], 0);
        let result = advance_run(&run, RunAction::Continue);

        assert_eq!(result, Err(RulesError::SubroutinesStillPending { pending: 1 }));
    }

    #[test]
    fn encounter_ice_continue_with_no_pending_passes_to_next_ice() {
        let run = run_state(
            RunPhase::EncounterIce,
            vec![test_ice("ice_wall_0", 0, 0), test_ice("ice_wall_1", 0, 3)],
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
        let run = run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 0)], 0);
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
    fn resolve_subroutine_with_invalid_index_errors() {
        let run = run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 0)], 0);
        let result = advance_run(&run, RunAction::ResolveSubroutine(0));

        assert_eq!(result, Err(RulesError::InvalidSubroutineIndex(0)));
    }

    #[test]
    fn break_subroutine_outside_encounter_ice_errors() {
        let run = run_state(RunPhase::ApproachIce, vec![test_ice("ice_wall", 0, 2)], 0);
        let result = advance_run(&run, RunAction::BreakSubroutine(0));

        assert_eq!(result, Err(RulesError::NotInEncounter));
    }

    #[test]
    fn break_subroutine_already_handled_errors() {
        let run = run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 1)], 0);
        let (after_break, _) =
            advance_run(&run, RunAction::BreakSubroutine(0)).expect("should succeed");
        let result = advance_run(&after_break, RunAction::ResolveSubroutine(0));

        assert_eq!(result, Err(RulesError::SubroutineAlreadyHandled));
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
        let run = run_state(RunPhase::ApproachIce, vec![test_ice("ice_wall", 0, 1)], 0);
        let (next, _events) = advance_run(&run, RunAction::JackOut).expect("should succeed");

        assert_eq!(next.phase, RunPhase::Ended);
    }

    #[test]
    fn jack_out_from_encounter_ice_ends_run_even_with_pending_subroutines() {
        let run = run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 5)], 0);
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
        let run = run_state(RunPhase::Initiation, vec![test_ice("ice_wall", 0, 1)], 0);
        let _ = advance_run(&run, RunAction::Continue).expect("should succeed");

        assert_eq!(run.phase, RunPhase::Initiation);
    }
}
