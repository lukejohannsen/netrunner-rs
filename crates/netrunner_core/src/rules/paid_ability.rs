//! Paid Ability Windows (PAWs): the priority-passing sub-loop that pauses
//! the run flow at each checkpoint (ICE approach, ICE encounter, pre-access)
//! so both sides get a chance to fire paid abilities before the engine
//! auto-advances. See `state::PaidAbilityWindow`'s doc comment for the data
//! model and `PlayerAction::PassPriority`'s for the player-facing contract.

use crate::cards::CardRegistry;
use crate::rules::ability;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::{self, AccessPhase, RunAction, RunPhase};
use crate::rules::state::{GamePhase, GameState, PaidAbilityWindow, Side};

/// Opens a PAW: the active-turn side gets priority first (Netrunner/Null
/// Signal Games priority rule 1). Only ever called while `state.phase ==
/// Action(_)` — the only phase a run (and therefore a window) can exist in.
pub(crate) fn open_window(state: &mut GameState) -> GameEvent {
    let active_priority = match state.phase {
        GamePhase::Action(side) => side,
        _ => unreachable!("open_window only called during Action(_) / mid-run"),
    };
    state.paid_ability_window = Some(PaidAbilityWindow {
        active_priority,
        consecutive_passes: 0,
        return_phase: Box::new(state.phase),
    });
    GameEvent::PaidAbilityWindowOpened { side: active_priority }
}

/// Opens a fresh window if the run just landed on `ApproachIce`/
/// `EncounterIce`, or on `AccessingCard` with the access sub-state at
/// `PendingChoice`/`PendingInteractiveTrigger`. Called both after a
/// Runner-driven `ContinueRun`/access-resolution action and from
/// `close_window`'s own auto-advance (arriving at the *next* ICE or the
/// *next* accessed card). The `Success` checkpoint's window is opened
/// explicitly by `CompleteRun` instead, not automatically here — the Runner
/// should still get to choose `CompleteRun` vs `JackOut` before a window
/// commits them to accessing. `AccessPhase::SelectNextCard` is deliberately
/// *not* a checkpoint either — unlike `PendingChoice`/`PendingInteractiveTrigger`,
/// which each gate a costed decision (steal/trash/avoidance cost) worth
/// reacting to, picking resolution order among already-accessed cards risks
/// nothing.
pub(crate) fn open_window_if_at_checkpoint(state: &mut GameState) -> Option<GameEvent> {
    let is_checkpoint = match state.active_run.as_ref().map(|r| &r.phase) {
        Some(RunPhase::ApproachIce) | Some(RunPhase::EncounterIce) => true,
        Some(RunPhase::AccessingCard) => matches!(
            state.active_run.as_ref().and_then(|r| r.access_state.as_ref()).map(|a| &a.phase),
            Some(AccessPhase::PendingChoice { .. }) | Some(AccessPhase::PendingInteractiveTrigger { .. })
        ),
        _ => false,
    };
    is_checkpoint.then(|| open_window(state))
}

/// Guard for handlers that must be blocked while a window is open. Called
/// explicitly per-handler rather than folded into `engine::require_phase`,
/// so `jack_out`/`rez_ice`/`break_subroutine`/`activate_ability` — which
/// stay legal (or independently gated) during a window — don't need to be
/// special-cased out of a shared check.
pub(crate) fn require_no_window(state: &GameState) -> Result<(), RulesError> {
    match &state.paid_ability_window {
        Some(w) => Err(RulesError::BlockedByPaidAbilityWindow { priority: w.active_priority }),
        None => Ok(()),
    }
}

/// Priority rule 4: any window-legal action that resolves resets the pass
/// counter and toggles priority to the other side, ensuring both players
/// always get a chance to respond. Called from `RezIce`'s, `BreakSubroutine`'s,
/// and `ActivateAbility`'s success paths — a no-op if no window is open.
///
/// Also defensively clears a now-stale window if the action ended the run
/// out from under it (e.g. a `Trigger::Paid` ability's `Effect::EndTheRun`
/// firing mid-window) — leaving a window open with no active run would
/// permanently block every ordinary action afterward, since it could never
/// be closed (`close_window` only resumes a run step; there'd be none).
pub(crate) fn note_window_action(state: &mut GameState, side: Side) {
    if state.active_run.is_none() {
        state.paid_ability_window = None;
        return;
    }
    if let Some(window) = state.paid_ability_window.as_mut() {
        window.consecutive_passes = 0;
        window.active_priority = side.other();
    }
}

/// Resolves `PlayerAction::PassPriority`. Toggles priority (rule 2), or on
/// the second consecutive pass, closes the window and auto-advances the
/// paused run step (rule 3). Takes `registry` (beyond what a bare
/// `(state, acting_side)` signature would need) because closing a `Success`
/// window must call `run::access_server`, which requires one — `apply_action`
/// already has it in scope for every other handler, so this costs nothing at
/// the call site.
pub(crate) fn pass_priority(
    state: &mut GameState,
    registry: &CardRegistry,
    acting_side: Side,
) -> Result<Vec<GameEvent>, RulesError> {
    let window = state
        .paid_ability_window
        .as_ref()
        .ok_or(RulesError::NotInPaidAbilityWindow)?;
    if window.active_priority != acting_side {
        return Err(RulesError::NotYourPriority {
            expected: window.active_priority,
            actual: acting_side,
        });
    }

    let mut events = vec![GameEvent::PriorityPassed { side: acting_side }];
    let window = state
        .paid_ability_window
        .as_mut()
        .expect("checked Some above");
    window.consecutive_passes += 1;

    if window.consecutive_passes >= 2 {
        events.push(GameEvent::PaidAbilityWindowClosed);
        state.paid_ability_window = None;
        events.extend(close_window(state, registry)?);
    } else {
        window.active_priority = acting_side.other();
    }
    Ok(events)
}

/// Auto-advances whatever run step was paused, once both sides have passed
/// (rule 3). Keys off `state.active_run`'s *current* `RunPhase` — untouched
/// while the window was open, since nothing a window permits (`RezIce`,
/// `BreakSubroutine`, `ActivateAbility`) mutates `RunPhase` itself — rather
/// than needing a separate discriminant on `PaidAbilityWindow`.
fn close_window(state: &mut GameState, registry: &CardRegistry) -> Result<Vec<GameEvent>, RulesError> {
    let Some(run_phase) = state.active_run.as_ref().map(|r| r.phase) else {
        return Ok(Vec::new());
    };

    match run_phase {
        RunPhase::ApproachIce => {
            // Rez-or-not is already decided (any window-time RezIce already
            // flipped the matching RunIce::rezzed). Reuses continue_run's
            // existing ApproachIce arm: auto-pass if unrezzed, else commit
            // to EncounterIce.
            let mut events = run::advance_run(state, RunAction::Continue)?;
            events.extend(open_window_if_at_checkpoint(state));
            Ok(events)
        }
        RunPhase::EncounterIce => {
            // Auto-fire anything the Runner didn't break, then pass the ICE
            // (continue_run's EncounterIce arm now sees zero pending).
            let mut events = ability::resolve_unbroken_subroutines(state)?;
            if state.active_run.is_some() {
                events.extend(run::advance_run(state, RunAction::Continue)?);
                events.extend(open_window_if_at_checkpoint(state));
            }
            Ok(events)
        }
        RunPhase::Success => {
            // This window was opened by `complete_run`. Now actually
            // access — the logic `complete_run` used to run inline.
            let server = state.active_run.as_ref().expect("checked Some above").server;
            let mut events = run::access_server(state, server, registry)?;
            if state.active_run.is_none() {
                events.push(GameEvent::RunCompleted { server });
            } else {
                // A card was just presented (or `SelectNextCard` was
                // reached, in which case this is a no-op) — open a fresh
                // window for whichever `AccessPhase` it landed on.
                events.extend(open_window_if_at_checkpoint(state));
            }
            Ok(events)
        }
        RunPhase::Initiation | RunPhase::AccessingCard | RunPhase::Ended => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardId, Effect, IceType, SubroutineDef};
    use crate::rules::run::{EncounteredSubroutine, RunIce, RunState, ServerId, SubroutineStatus};
    use crate::rules::state::{
        AgendaPoints, Clicks, Credits, CorpState, MemoryUnits, PlayerResources, RunnerState,
    };

    fn registry() -> CardRegistry {
        CardRegistry::new()
    }

    fn base_state() -> GameState {
        GameState {
            corp: CorpState {
                scored_agendas: Vec::new(),
                resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
                hq: Vec::new(),
                r_and_d: Vec::new(),
                archives: Vec::new(),
                installed: Vec::new(),
            },
            runner: RunnerState {
                scored_agendas: Vec::new(),
                resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
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
            paid_ability_window: None,
            seed: 0,
            rng_step: 0,
        }
    }

    fn run_ice(rezzed: bool, subroutines: Vec<EncounteredSubroutine>) -> RunIce {
        RunIce {
            card_id: CardId("ice_wall".to_string()),
            current_strength: 0,
            ice_type: IceType::Barrier,
            subroutines,
            rezzed,
        }
    }

    fn pending_subroutine() -> EncounteredSubroutine {
        EncounteredSubroutine {
            id: 0,
            definition: SubroutineDef { text: "give a tag".to_string(), effect: Effect::GiveTags(1) },
            status: SubroutineStatus::Pending,
        }
    }

    #[test]
    fn open_window_sets_priority_to_active_side_and_resets_passes() {
        let mut state = base_state();
        let event = open_window(&mut state);

        assert_eq!(event, GameEvent::PaidAbilityWindowOpened { side: Side::Runner });
        let window = state.paid_ability_window.expect("window should be open");
        assert_eq!(window.active_priority, Side::Runner);
        assert_eq!(window.consecutive_passes, 0);
    }

    #[test]
    fn single_pass_toggles_priority_and_leaves_window_open() {
        let mut state = base_state();
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![run_ice(true, Vec::new())],
            position: 0,
            access_state: None,
            jack_out_permitted: false,
        });
        open_window(&mut state);

        let events = pass_priority(&mut state, &registry(), Side::Runner).expect("pass should succeed");

        assert_eq!(events, vec![GameEvent::PriorityPassed { side: Side::Runner }]);
        let window = state.paid_ability_window.expect("window should stay open");
        assert_eq!(window.active_priority, Side::Corp);
        assert_eq!(window.consecutive_passes, 1);
    }

    #[test]
    fn second_consecutive_pass_closes_window_and_auto_advances() {
        let mut state = base_state();
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![run_ice(true, Vec::new())],
            position: 0,
            access_state: None,
            jack_out_permitted: false,
        });
        open_window(&mut state);

        pass_priority(&mut state, &registry(), Side::Runner).expect("first pass should succeed");
        let events = pass_priority(&mut state, &registry(), Side::Corp).expect("second pass should succeed");

        // The ApproachIce window's close re-opens a fresh EncounterIce window
        // (rezzed ICE with no subroutines commits straight through).
        let window = state.paid_ability_window.expect("EncounterIce should reopen a window");
        assert_eq!(window.active_priority, Side::Runner);
        assert_eq!(window.consecutive_passes, 0);
        assert_eq!(
            events,
            vec![
                GameEvent::PriorityPassed { side: Side::Corp },
                GameEvent::PaidAbilityWindowClosed,
                GameEvent::IceEncountered {
                    card_id: CardId("ice_wall".to_string()),
                    strength: 0,
                    subroutine_count: 0,
                },
                GameEvent::PaidAbilityWindowOpened { side: Side::Runner },
            ]
        );
    }

    #[test]
    fn pass_priority_with_no_window_open_errors() {
        let mut state = base_state();
        let result = pass_priority(&mut state, &registry(), Side::Runner);
        assert_eq!(result, Err(RulesError::NotInPaidAbilityWindow));
    }

    #[test]
    fn pass_priority_from_non_priority_side_errors() {
        let mut state = base_state();
        open_window(&mut state); // active_priority = Runner
        let result = pass_priority(&mut state, &registry(), Side::Corp);
        assert_eq!(
            result,
            Err(RulesError::NotYourPriority { expected: Side::Runner, actual: Side::Corp })
        );
    }

    #[test]
    fn note_window_action_resets_passes_and_toggles_priority() {
        let mut state = base_state();
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![run_ice(true, Vec::new())],
            position: 0,
            access_state: None,
            jack_out_permitted: false,
        });
        open_window(&mut state);
        pass_priority(&mut state, &registry(), Side::Runner).expect("pass should succeed");
        // consecutive_passes is now 1, active_priority is Corp.

        note_window_action(&mut state, Side::Corp);

        let window = state.paid_ability_window.expect("window should stay open");
        assert_eq!(window.consecutive_passes, 0);
        assert_eq!(window.active_priority, Side::Runner);
    }

    #[test]
    fn note_window_action_clears_a_stale_window_once_the_run_has_ended() {
        let mut state = base_state();
        open_window(&mut state);
        state.active_run = None; // e.g. a Trigger::Paid ability's Effect::EndTheRun fired.

        note_window_action(&mut state, Side::Runner);

        assert!(state.paid_ability_window.is_none());
    }

    #[test]
    fn approach_ice_window_close_with_unrezzed_ice_auto_passes_without_encounter() {
        let mut state = base_state();
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![run_ice(false, Vec::new())],
            position: 0,
            access_state: None,
            jack_out_permitted: false,
        });
        open_window(&mut state);

        pass_priority(&mut state, &registry(), Side::Runner).expect("first pass should succeed");
        let events = pass_priority(&mut state, &registry(), Side::Corp).expect("second pass should succeed");

        // Only one (unrezzed) ICE, so passing it reaches Success with none
        // remaining — no new window opens there (Success's window is opened
        // explicitly by `CompleteRun`, not automatically).
        assert!(state.paid_ability_window.is_none());
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::Success);
        assert_eq!(
            events,
            vec![
                GameEvent::PriorityPassed { side: Side::Corp },
                GameEvent::PaidAbilityWindowClosed,
                GameEvent::IcePassed { server: ServerId::Hq, position: 0 },
                GameEvent::RunSucceeded { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn encounter_ice_window_close_auto_fires_unbroken_subroutines() {
        let mut state = base_state();
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![run_ice(true, vec![pending_subroutine()])],
            position: 0,
            access_state: None,
            jack_out_permitted: false,
        });
        open_window(&mut state);

        pass_priority(&mut state, &registry(), Side::Runner).expect("first pass should succeed");
        let events = pass_priority(&mut state, &registry(), Side::Corp).expect("second pass should succeed");

        assert_eq!(state.runner.tags, 1, "unbroken subroutine should have auto-fired its effect");
        assert_eq!(
            events,
            vec![
                GameEvent::PriorityPassed { side: Side::Corp },
                GameEvent::PaidAbilityWindowClosed,
                GameEvent::SubroutineFired {
                    card_id: CardId("ice_wall".to_string()),
                    index: 0,
                    effect: Effect::GiveTags(1),
                },
                GameEvent::TagsGiven { side: Side::Runner, amount: 1 },
                GameEvent::IcePassed { server: ServerId::Hq, position: 0 },
                GameEvent::RunSucceeded { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn success_window_close_presenting_a_single_card_opens_a_fresh_access_window() {
        let mut state = base_state();
        state.corp.hq = vec![CardId("hedge_fund".to_string())];
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
            access_state: None,
            jack_out_permitted: true,
        });
        open_window(&mut state);

        pass_priority(&mut state, &registry(), Side::Runner).expect("first pass should succeed");
        let events = pass_priority(&mut state, &registry(), Side::Corp).expect("second pass should succeed");

        // Landing on `PendingChoice` (a single accessed card) opens a fresh
        // window for the Runner's steal/trash/pass decision.
        let window = state.paid_ability_window.expect("a fresh window should open for the presented card");
        assert_eq!(window.active_priority, Side::Runner);
        assert_eq!(window.consecutive_passes, 0);
        assert_eq!(
            events,
            vec![
                GameEvent::PriorityPassed { side: Side::Corp },
                GameEvent::PaidAbilityWindowClosed,
                GameEvent::CardAccessed { card: CardId("hedge_fund".to_string()), server: ServerId::Hq },
                GameEvent::PaidAbilityWindowOpened { side: Side::Runner },
            ]
        );
    }

    #[test]
    fn success_window_close_presenting_select_next_card_does_not_open_a_window() {
        let mut state = base_state();
        // Archives access every card in it, so two cards there yields a
        // `SelectNextCard` choice rather than a single `PendingChoice` —
        // deliberately not a checkpoint (no cost is at stake in ordering).
        state.corp.archives = vec![CardId("card_1".to_string()), CardId("card_2".to_string())];
        state.active_run = Some(RunState {
            server: ServerId::Archives,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
            access_state: None,
            jack_out_permitted: true,
        });
        open_window(&mut state);

        pass_priority(&mut state, &registry(), Side::Runner).expect("first pass should succeed");
        let events = pass_priority(&mut state, &registry(), Side::Corp).expect("second pass should succeed");

        assert!(state.paid_ability_window.is_none(), "SelectNextCard is not a checkpoint");
        assert_eq!(
            events,
            vec![GameEvent::PriorityPassed { side: Side::Corp }, GameEvent::PaidAbilityWindowClosed]
        );
    }

    #[test]
    fn encounter_ice_window_close_auto_fires_end_the_run_and_terminates_cleanly() {
        let mut state = base_state();
        let end_the_run_subroutine = EncounteredSubroutine {
            id: 0,
            definition: SubroutineDef { text: "end the run".to_string(), effect: Effect::EndTheRun },
            status: SubroutineStatus::Pending,
        };
        state.active_run = Some(RunState {
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![run_ice(true, vec![end_the_run_subroutine])],
            position: 0,
            access_state: None,
            jack_out_permitted: false,
        });
        open_window(&mut state);

        pass_priority(&mut state, &registry(), Side::Runner).expect("first pass should succeed");
        let events = pass_priority(&mut state, &registry(), Side::Corp).expect("second pass should succeed");

        assert_eq!(
            events,
            vec![
                GameEvent::PriorityPassed { side: Side::Corp },
                GameEvent::PaidAbilityWindowClosed,
                GameEvent::SubroutineFired {
                    card_id: CardId("ice_wall".to_string()),
                    index: 0,
                    effect: Effect::EndTheRun,
                },
                GameEvent::RunEndedByEffect { server: ServerId::Hq },
            ]
        );
        assert!(state.active_run.is_none(), "the run must end cleanly, not error or half-transition");
        assert!(state.paid_ability_window.is_none());
    }
}
