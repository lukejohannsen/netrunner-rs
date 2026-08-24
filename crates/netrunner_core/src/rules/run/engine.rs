use crate::dsl::{CardId, Effect};
use crate::rules::ability::evaluate_effect;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::action::RunAction;
use crate::rules::run::state::{RunIce, RunPhase, RunState, SubroutineStatus};
use crate::rules::state::GameState;

fn phase_for_position(ice: &[RunIce], position: usize) -> RunPhase {
    if position < ice.len() {
        RunPhase::ApproachIce
    } else {
        RunPhase::Success
    }
}

/// Advances `run.position` past the ICE at `position`, updates `run.phase`
/// via `phase_for_position`, and returns the `IcePassed`-plus-next-step
/// events shared by both ways an ICE gets left behind: `EncounterIce
/// --Continue-->` after every subroutine is handled, and `ApproachIce
/// --Continue-->` auto-passing an unrezzed ICE.
fn pass_current_ice(run: &mut RunState, position: usize) -> Vec<GameEvent> {
    let server = run.server;
    let mut events = vec![GameEvent::IcePassed { server, position: position as u32 }];
    run.position = position + 1;
    run.phase = phase_for_position(&run.ice, run.position);
    match run.phase {
        RunPhase::ApproachIce => {
            events.push(GameEvent::IceApproached { server, position: run.position as u32 })
        }
        RunPhase::Success => events.push(GameEvent::RunSucceeded { server }),
        _ => {}
    }
    events
}

pub fn advance_run(state: &mut GameState, action: RunAction) -> Result<Vec<GameEvent>, RulesError> {
    let phase = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?.phase;
    if matches!(phase, RunPhase::Success | RunPhase::AccessingCard | RunPhase::Ended) {
        return Err(RulesError::RunAlreadyConcluded { phase });
    }

    match action {
        RunAction::JackOut => Ok(jack_out(state)),
        RunAction::Continue => continue_run(state),
        RunAction::ResolveSubroutine(index) => step_subroutine(state, index, true),
        RunAction::BreakSubroutine(index) => step_subroutine(state, index, false),
    }
}

// Real NISEI rules restrict exactly when a Runner may jack out; this engine
// deliberately allows it unconditionally from any non-terminal phase, even
// mid-EncounterIce with subroutines still pending. Refining jack-out
// legality windows is future work.
fn jack_out(state: &mut GameState) -> Vec<GameEvent> {
    // Safe: advance_run already confirmed active_run is Some before dispatching here.
    let run = state.active_run.as_mut().expect("active_run checked by advance_run");
    run.phase = RunPhase::Ended;
    vec![GameEvent::RunJackedOut { server: run.server }]
}

fn continue_run(state: &mut GameState) -> Result<Vec<GameEvent>, RulesError> {
    let run = state.active_run.as_mut().expect("active_run checked by advance_run");

    match run.phase {
        RunPhase::Initiation => {
            let server = run.server;
            run.phase = phase_for_position(&run.ice, 0);
            if run.phase == RunPhase::Success {
                Ok(vec![GameEvent::RunSucceeded { server }])
            } else {
                Ok(vec![GameEvent::IceApproached { server, position: 0 }])
            }
        }
        RunPhase::ApproachIce => {
            let position = run.position;
            let ice = run.ice.get(position).ok_or(RulesError::NotInEncounter)?;

            if !ice.rezzed {
                // Unrezzed ICE has no effect on a run — it presents no
                // subroutines and is simply passed. Reuses the same
                // events `EncounterIce --Continue-->` emits after every
                // subroutine is handled: from the Runner's perspective
                // "passed this ICE with nothing to break" and "passed
                // this ICE because it was never rezzed" are the same
                // observable outcome (no `IceEncountered`/
                // `SubroutineFired`/`SubroutineBroken` either way), so no
                // new `GameEvent` variant is warranted.
                return Ok(pass_current_ice(run, position));
            }

            run.phase = RunPhase::EncounterIce;
            let event = GameEvent::IceEncountered {
                card_id: ice.card_id.clone(),
                strength: ice.current_strength,
                subroutine_count: ice.subroutines.len(),
            };
            Ok(vec![event])
        }
        RunPhase::EncounterIce => {
            let position = run.position;
            let pending = run
                .ice
                .get(position)
                .map(|ice| {
                    ice.subroutines.iter().filter(|s| s.status == SubroutineStatus::Pending).count() as u32
                })
                .unwrap_or(0);
            if pending > 0 {
                return Err(RulesError::SubroutinesStillPending { pending });
            }

            Ok(pass_current_ice(run, position))
        }
        // Unreachable in practice — `advance_run`'s top-level guard already
        // rejects both phases before `continue_run` is ever called. Handled
        // here only so this match stays exhaustive.
        RunPhase::AccessingCard | RunPhase::Success | RunPhase::Ended => {
            Err(RulesError::RunAlreadyConcluded { phase: run.phase })
        }
    }
}

/// Validates and applies a `Pending -> {Broken, Resolved}` transition for the
/// subroutine at `index` on the ICE currently being encountered. Shared by
/// `step_subroutine` (below) and `ability::evaluate_effect`'s
/// `Effect::BreakSubroutine` arm, so both entry points enforce identical
/// phase/bounds/status checks instead of maintaining two copies of them.
pub(crate) fn transition_subroutine(
    state: &mut GameState,
    index: usize,
    to: SubroutineStatus,
) -> Result<(CardId, Effect), RulesError> {
    let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
    if run.phase != RunPhase::EncounterIce {
        return Err(RulesError::NotInEncounter);
    }
    let position = run.position;
    let ice = run.ice.get_mut(position).ok_or(RulesError::NotInEncounter)?;
    let card_id = ice.card_id.clone();
    let subroutine = ice
        .subroutines
        .get_mut(index)
        .ok_or(RulesError::InvalidSubroutineIndex(index))?;

    if subroutine.status != SubroutineStatus::Pending {
        return Err(RulesError::SubroutineAlreadyHandled);
    }

    let effect = subroutine.definition.effect.clone();
    subroutine.status = to;
    Ok((card_id, effect))
}

fn step_subroutine(state: &mut GameState, index: usize, resolve: bool) -> Result<Vec<GameEvent>, RulesError> {
    let to = if resolve { SubroutineStatus::Resolved } else { SubroutineStatus::Broken };
    let (card_id, effect) = transition_subroutine(state, index, to)?;

    if resolve {
        let mut events = vec![GameEvent::SubroutineFired { card_id, index, effect: effect.clone() }];
        events.extend(evaluate_effect(state, &effect)?);
        Ok(events)
    } else {
        Ok(vec![GameEvent::SubroutineBroken { card_id, index }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardId, Effect, SubroutineDef};
    use crate::rules::run::state::{EncounteredSubroutine, RunState, ServerId};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, Credits, GamePhase, MemoryUnits, PlayerResources,
        RunnerState, Side,
    };

    fn game_state() -> GameState {
        GameState {
            corp: CorpState {
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(5),
                    clicks: Clicks(3),
                    agenda_points: AgendaPoints(0),
                },
                hq: Vec::new(),
                r_and_d: Vec::new(),
                archives: Vec::new(),
                installed: Vec::new(),
            },
            runner: RunnerState {
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(5),
                    clicks: Clicks(4),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                brain_damage: 0,
                tags: 0,
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
                heap: Vec::new(),
            },
            phase: GamePhase::Action(Side::Runner),
            active_run: None,
            seed: 0,
            rng_step: 0,
        }
    }

    fn run_state(phase: RunPhase, ice: Vec<RunIce>, position: usize) -> RunState {
        RunState {
            server: ServerId::Hq,
            phase,
            ice,
            position,
            access_state: None,
        }
    }

    /// Builds a `RunIce` with `subroutine_count` placeholder `Pending`
    /// subroutines — identity/effect content doesn't matter for tests using
    /// this, only status transitions and counts do.
    fn test_ice(card_id: &str, strength: i32, subroutine_count: usize, rezzed: bool) -> RunIce {
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
            rezzed,
        }
    }

    #[test]
    fn initiation_continue_with_ice_enters_approach_ice() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::Initiation, vec![test_ice("ice_wall", 0, 2, true)], 0));
        let events = advance_run(&mut state, RunAction::Continue).expect("should succeed");

        let run = state.active_run.unwrap();
        assert_eq!(run.phase, RunPhase::ApproachIce);
        assert_eq!(run.position, 0);
        assert_eq!(
            events,
            vec![GameEvent::IceApproached { server: ServerId::Hq, position: 0 }]
        );
    }

    #[test]
    fn initiation_continue_with_no_ice_is_immediate_success() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::Initiation, vec![], 0));
        let events = advance_run(&mut state, RunAction::Continue).expect("should succeed");

        assert_eq!(state.active_run.unwrap().phase, RunPhase::Success);
        assert_eq!(events, vec![GameEvent::RunSucceeded { server: ServerId::Hq }]);
    }

    #[test]
    fn approach_ice_continue_enters_encounter_ice() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::ApproachIce, vec![test_ice("ice_wall", 3, 2, true)], 0));
        let events = advance_run(&mut state, RunAction::Continue).expect("should succeed");

        assert_eq!(state.active_run.unwrap().phase, RunPhase::EncounterIce);
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
    fn approach_ice_continue_with_unrezzed_ice_auto_passes_without_encounter() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::ApproachIce, vec![test_ice("ice_wall", 3, 2, false)], 0));
        let events = advance_run(&mut state, RunAction::Continue).expect("should succeed");

        let run = state.active_run.unwrap();
        assert_eq!(run.phase, RunPhase::Success);
        assert_eq!(
            events,
            vec![
                GameEvent::IcePassed { server: ServerId::Hq, position: 0 },
                GameEvent::RunSucceeded { server: ServerId::Hq },
            ]
        );
        // Never encountered — its subroutines were never touched.
        assert!(run.ice[0].subroutines.iter().all(|s| s.status == SubroutineStatus::Pending));
    }

    #[test]
    fn approach_ice_continue_with_unrezzed_ice_advances_to_next_ice_when_more_remain() {
        let mut state = game_state();
        state.active_run = Some(run_state(
            RunPhase::ApproachIce,
            vec![test_ice("ice_wall_0", 0, 1, false), test_ice("ice_wall_1", 0, 1, true)],
            0,
        ));
        let events = advance_run(&mut state, RunAction::Continue).expect("should succeed");

        let run = state.active_run.unwrap();
        assert_eq!(run.phase, RunPhase::ApproachIce);
        assert_eq!(run.position, 1);
        assert_eq!(
            events,
            vec![
                GameEvent::IcePassed { server: ServerId::Hq, position: 0 },
                GameEvent::IceApproached { server: ServerId::Hq, position: 1 },
            ]
        );
    }

    #[test]
    fn initiation_continue_with_unrezzed_ice_still_enters_approach_ice() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::Initiation, vec![test_ice("ice_wall", 0, 2, false)], 0));
        let events = advance_run(&mut state, RunAction::Continue).expect("should succeed");

        let run = state.active_run.unwrap();
        assert_eq!(run.phase, RunPhase::ApproachIce);
        assert_eq!(run.position, 0);
        assert_eq!(
            events,
            vec![GameEvent::IceApproached { server: ServerId::Hq, position: 0 }]
        );
    }

    #[test]
    fn encounter_ice_resolve_subroutine_fires_and_marks_resolved() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 2, true)], 0));
        let events =
            advance_run(&mut state, RunAction::ResolveSubroutine(0)).expect("should succeed");

        // subroutine 0's effect (EndTheRun) fired, which clears active_run —
        // this is itself proof the effect was actually applied, not just the
        // status flip. See resolve_subroutine_applies_its_effect below for a
        // non-run-ending effect that leaves active_run intact to inspect.
        assert!(state.active_run.is_none());
        assert_eq!(
            events,
            vec![
                GameEvent::SubroutineFired {
                    card_id: CardId("ice_wall".to_string()),
                    index: 0,
                    effect: Effect::EndTheRun,
                },
                GameEvent::RunEndedByEffect { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn resolve_subroutine_applies_its_effect() {
        let mut state = game_state();
        let mut ice = test_ice("ice_wall", 0, 1, true);
        ice.subroutines[0].definition.effect = Effect::GiveTags(2);
        state.active_run = Some(run_state(RunPhase::EncounterIce, vec![ice], 0));

        let events = advance_run(&mut state, RunAction::ResolveSubroutine(0)).expect("should succeed");

        assert_eq!(state.runner.tags, 2);
        assert_eq!(
            state.active_run.unwrap().ice[0].subroutines[0].status,
            SubroutineStatus::Resolved
        );
        assert_eq!(
            events,
            vec![
                GameEvent::SubroutineFired {
                    card_id: CardId("ice_wall".to_string()),
                    index: 0,
                    effect: Effect::GiveTags(2),
                },
                GameEvent::TagsGiven { side: Side::Runner, amount: 2 },
            ]
        );
    }

    #[test]
    fn encounter_ice_break_subroutine_marks_broken() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 2, true)], 0));
        let events =
            advance_run(&mut state, RunAction::BreakSubroutine(0)).expect("should succeed");

        assert_eq!(
            state.active_run.unwrap().ice[0].subroutines[0].status,
            SubroutineStatus::Broken
        );
        assert_eq!(
            events,
            vec![GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0 }]
        );
    }

    #[test]
    fn encounter_ice_continue_with_pending_subroutines_errors() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 1, true)], 0));
        let result = advance_run(&mut state, RunAction::Continue);

        assert_eq!(result, Err(RulesError::SubroutinesStillPending { pending: 1 }));
    }

    #[test]
    fn encounter_ice_continue_with_no_pending_passes_to_next_ice() {
        let mut state = game_state();
        state.active_run = Some(run_state(
            RunPhase::EncounterIce,
            vec![test_ice("ice_wall_0", 0, 0, true), test_ice("ice_wall_1", 0, 3, true)],
            0,
        ));
        let events = advance_run(&mut state, RunAction::Continue).expect("should succeed");

        let run = state.active_run.unwrap();
        assert_eq!(run.phase, RunPhase::ApproachIce);
        assert_eq!(run.position, 1);
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
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 0, true)], 0));
        let events = advance_run(&mut state, RunAction::Continue).expect("should succeed");

        assert_eq!(state.active_run.unwrap().phase, RunPhase::Success);
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
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 0, true)], 0));
        let result = advance_run(&mut state, RunAction::ResolveSubroutine(0));

        assert_eq!(result, Err(RulesError::InvalidSubroutineIndex(0)));
    }

    #[test]
    fn break_subroutine_outside_encounter_ice_errors() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::ApproachIce, vec![test_ice("ice_wall", 0, 2, true)], 0));
        let result = advance_run(&mut state, RunAction::BreakSubroutine(0));

        assert_eq!(result, Err(RulesError::NotInEncounter));
    }

    #[test]
    fn break_subroutine_already_handled_errors() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 1, true)], 0));
        advance_run(&mut state, RunAction::BreakSubroutine(0)).expect("should succeed");
        let result = advance_run(&mut state, RunAction::ResolveSubroutine(0));

        assert_eq!(result, Err(RulesError::SubroutineAlreadyHandled));
    }

    #[test]
    fn jack_out_from_initiation_ends_run() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::Initiation, vec![], 0));
        let events = advance_run(&mut state, RunAction::JackOut).expect("should succeed");

        assert_eq!(state.active_run.unwrap().phase, RunPhase::Ended);
        assert_eq!(events, vec![GameEvent::RunJackedOut { server: ServerId::Hq }]);
    }

    #[test]
    fn jack_out_from_approach_ice_ends_run() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::ApproachIce, vec![test_ice("ice_wall", 0, 1, true)], 0));
        advance_run(&mut state, RunAction::JackOut).expect("should succeed");

        assert_eq!(state.active_run.unwrap().phase, RunPhase::Ended);
    }

    #[test]
    fn jack_out_from_encounter_ice_ends_run_even_with_pending_subroutines() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 5, true)], 0));
        let events = advance_run(&mut state, RunAction::JackOut).expect("should succeed");

        assert_eq!(state.active_run.unwrap().phase, RunPhase::Ended);
        assert_eq!(events, vec![GameEvent::RunJackedOut { server: ServerId::Hq }]);
    }

    #[test]
    fn continue_after_success_errors() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::Success, vec![], 0));
        let result = advance_run(&mut state, RunAction::Continue);

        assert_eq!(
            result,
            Err(RulesError::RunAlreadyConcluded { phase: RunPhase::Success })
        );
    }

    #[test]
    fn jack_out_after_success_errors() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::Success, vec![], 0));
        let result = advance_run(&mut state, RunAction::JackOut);

        assert_eq!(
            result,
            Err(RulesError::RunAlreadyConcluded { phase: RunPhase::Success })
        );
    }

    #[test]
    fn action_after_ended_errors() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::Ended, vec![], 0));
        let result = advance_run(&mut state, RunAction::Continue);

        assert_eq!(
            result,
            Err(RulesError::RunAlreadyConcluded { phase: RunPhase::Ended })
        );
    }

    #[test]
    fn advance_run_with_no_active_run_errors() {
        let mut state = game_state();
        let result = advance_run(&mut state, RunAction::Continue);

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }
}
