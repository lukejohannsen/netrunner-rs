use crate::cards::CardRegistry;
use crate::dsl::{CardId, CardType, Effect, IceType, StrengthModifier};
use crate::rules::ability::evaluate_effect;
use crate::rules::dispatcher;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::action::RunAction;
use crate::rules::run::state::{EncounteredSubroutine, RunIce, RunPhase, RunState, ServerId, SubroutineStatus};
use crate::rules::state::CompletedRun;
use crate::rules::state::{GamePhase, GameState, InstallSlot, InstalledCard, Side, WindowCheckpoint};

/// Builds one `RunIce` from an `InstalledCard` known to be ICE (caller
/// filters by `InstallSlot::Ice`), looking up strength/subroutines from
/// `registry`. Errors with `RulesError::CardNotFoundInRegistry` if
/// `installed.card` isn't registered at all — a deck referencing a card
/// outside the loaded registry is a real authoring/setup error, not
/// something to silently paper over. A *present* but sparse `CardDefinition` (no
/// `strength`/`subroutines` set) still leniently defaults to a blank
/// 0-strength/no-subroutines ICE, which is legitimate for a vanilla ICE —
/// mirrors `run::access::compute_pending_choice`'s existing leniency there.
fn build_run_ice(installed: &InstalledCard, registry: &CardRegistry) -> Result<RunIce, RulesError> {
    let card_def = registry
        .get(&installed.card)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(installed.card.clone()))?;
    // Bakes any `StrengthModifier` into the seeded value once, at the moment
    // this ICE is first encountered during a run — its condition (server
    // type, hosted advancement count) is fixed for the run's duration in
    // every case this schema models, so a live per-query recompute isn't
    // needed here (unlike Runner breakers' `PerInstalledIcebreaker`, which
    // genuinely can change mid-game — see `ability::computed_runner_strength`).
    // `Effect::ModifyStrength`'s existing deltas (e.g. Leech) still apply
    // correctly on top, since they mutate this same `current_strength` field.
    let modifier_bonus = match card_def.strength_modifier {
        Some(StrengthModifier::WhileProtectingRemote(bonus)) if matches!(installed.server, ServerId::Remote(_)) => bonus,
        Some(StrengthModifier::WhileHostedAdvancementsAtLeast { threshold, bonus })
            if installed.advancement_tokens >= threshold =>
        {
            bonus
        }
        _ => 0,
    };
    let current_strength = card_def.strength.unwrap_or(0) + modifier_bonus;
    let ice_type = match &card_def.card_type {
        CardType::Ice(ice_type) => *ice_type,
        _ => IceType::Barrier,
    };
    let subroutines = card_def
        .subroutines
        .iter()
        .enumerate()
        .map(|(id, def)| EncounteredSubroutine { id, definition: def.clone(), status: SubroutineStatus::Pending })
        .collect();

    Ok(RunIce { card_id: installed.card.clone(), current_strength, ice_type, subroutines, rezzed: installed.rezzed })
}

/// Sets `state.active_run` to a freshly-initiated run on `server` — shared
/// by `engine::initiate_run` (`PlayerAction::InitiateRun`, which spends a
/// click before calling this) and `ability::evaluate_effect`'s
/// `Effect::InitiateRun` arm (a card's own "make a run" text, which doesn't
/// spend an extra click — the enclosing `PlayEvent`/`PlayOperation` already
/// did). `RulesError::RunAlreadyInProgress` if a run is already active.
/// Whether a run may begin right now — the **single** definition of that
/// precondition.
///
/// Both [`start_run`] and `Effect::PromptChooseServer`'s *park-time* check
/// call this. That pairing is load-bearing: `PromptChooseServer` parks a
/// decision that only `start_run` can resolve, so if the two preconditions
/// ever disagree, the decision parks unresolvably and — since a parked
/// decision blocks every other action — deadlocks the game outright. The
/// engine has already been bitten by exactly that once, when the park-time
/// check tested only `active_run`.
///
/// A run requires the Runner's own action phase, no run already underway,
/// and no end-of-turn window. That last clause is what a phase check alone
/// misses: `WindowCheckpoint::EndOfTurn` deliberately keeps the phase it
/// interrupted, so `Action(Runner)` stays true throughout it. Without the
/// clause, a run-initiating paid ability (*Red Team*'s) could start a run
/// during the Runner's end-of-turn window; `finish_end_turn` then handed
/// the turn over with `active_run` still set, leaving the Corp with no
/// legal action at all — `EndTurn` rejected by
/// `CannotEndTurnWhileRunActive`, and the run not theirs to advance.
/// `StartOfTurn` windows need no special case: they run under `phase ==
/// StartOfTurn(_)`, which the phase check already rejects.
///
/// Errors surface through `legal_actions`' `apply_action` probe, so an
/// ability that can't legally run right now simply isn't offered.
/// Found by `no_panics_or_deadlocks_across_many_seeds_system_gateway`.
pub(crate) fn check_run_may_begin(state: &GameState) -> Result<(), RulesError> {
    if state.active_run.is_some() {
        return Err(RulesError::RunAlreadyInProgress);
    }
    let ending_turn = matches!(
        state.paid_ability_window.as_ref().map(|w| w.checkpoint),
        Some(WindowCheckpoint::EndOfTurn { .. })
    );
    if state.phase != GamePhase::Action(Side::Runner) || ending_turn {
        return Err(RulesError::RunNotPermittedNow { phase: state.phase });
    }
    Ok(())
}

#[cfg(test)]
mod run_precondition_tests {
    use super::*;
    use crate::rules::state::PaidAbilityWindow;

    fn runner_action_phase() -> GameState {
        GameState { phase: GamePhase::Action(Side::Runner), ..Default::default() }
    }

    #[test]
    fn a_run_may_begin_in_the_runners_action_phase() {
        assert_eq!(check_run_may_begin(&runner_action_phase()), Ok(()));
    }

    /// The deadlock this precondition exists for. *Red Team*'s
    /// run-initiating paid ability is activatable during the Runner's
    /// end-of-turn window — which keeps `phase == Action(Runner)`, so a
    /// phase check alone would let it through. Starting a run there left
    /// `active_run` set across the turn handoff, and the Corp then had no
    /// legal action at all: `EndTurn` rejected by
    /// `CannotEndTurnWhileRunActive`, and the run not theirs to advance.
    #[test]
    fn a_run_may_not_begin_during_the_runners_end_of_turn_window() {
        let mut state = runner_action_phase();
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            checkpoint: WindowCheckpoint::EndOfTurn { side: Side::Runner },
            return_phase: Box::new(state.phase),
        });

        assert_eq!(
            check_run_may_begin(&state),
            Err(RulesError::RunNotPermittedNow { phase: GamePhase::Action(Side::Runner) })
        );
    }

    /// A `StartOfTurn` window needs no special case — it runs under
    /// `StartOfTurn(_)`, which the phase check already rejects.
    #[test]
    fn a_run_may_not_begin_before_the_action_phase() {
        let state = GameState { phase: GamePhase::StartOfTurn(Side::Runner), ..Default::default() };

        assert_eq!(
            check_run_may_begin(&state),
            Err(RulesError::RunNotPermittedNow { phase: GamePhase::StartOfTurn(Side::Runner) })
        );
    }

    #[test]
    fn a_run_may_not_begin_during_the_corps_turn() {
        let state = GameState { phase: GamePhase::Action(Side::Corp), ..Default::default() };

        assert!(matches!(check_run_may_begin(&state), Err(RulesError::RunNotPermittedNow { .. })));
    }
}

pub fn start_run(state: &mut GameState, registry: &CardRegistry, server: ServerId) -> Result<(), RulesError> {
    check_run_may_begin(state)?;

    // `corp.installed`'s Vec order is install order (oldest first); installs
    // only ever `.push()`, so oldest install = outermost ICE = index 0,
    // matching `RunIce`'s outermost-to-innermost doc comment.
    let ice: Vec<RunIce> = state
        .corp
        .installed
        .iter()
        .filter(|installed| installed.server == server && installed.slot == InstallSlot::Ice)
        .map(|installed| build_run_ice(installed, registry))
        .collect::<Result<Vec<_>, _>>()?;

    state.active_run = Some(RunState { agendas_stolen_this_run: 0, persistent_trashed_upgrades: Vec::new(),
        on_success_effect: None,
        additional_rd_access: 0,
        additional_hq_access: 0,
        access_replacement: None,
        access_state: None,
        bad_publicity_credits: state.corp.bad_publicity,
        server,
        phase: RunPhase::Initiation,
        ice,
        position: 0,
        // Netrunner/Null Signal Games jack-out rule 1: closed until an ICE
        // is passed (or the server approach step is reached with none
        // installed).
        jack_out_permitted: false,
        cards_accessed_count: 0, ice_rez_cost_modifier: 0, bonus_run_credits: 0, runner_cannot_steal_or_trash: false,
    });
    Ok(())
}

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
    // An ICE has now been left behind — including an unrezzed one, which
    // still counts as "passed" — opening the jack-out window whether the
    // Runner is about to approach the next ICE or has reached the server
    // approach step with none remaining (Netrunner/Null Signal Games
    // jack-out rules 2 & 3).
    run.jack_out_permitted = true;
    match run.phase {
        RunPhase::ApproachIce => {
            events.push(GameEvent::IceApproached { server, position: run.position as u32 })
        }
        RunPhase::Success => events.push(GameEvent::RunSucceeded { server }),
        _ => {}
    }
    events
}

pub fn advance_run(
    state: &mut GameState,
    action: RunAction,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let phase = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?.phase;
    // `JackOut` alone is legal from `RunPhase::Success` (the "approach
    // server" jack-out window, Netrunner/Null Signal Games rule 3) — every
    // other action still
    // treats `Success` as concluded, since there's nothing left to
    // continue/resolve/break through.
    let already_concluded = match action {
        RunAction::JackOut => matches!(phase, RunPhase::AccessingCard | RunPhase::Ended),
        _ => matches!(phase, RunPhase::Success | RunPhase::AccessingCard | RunPhase::Ended),
    };
    if already_concluded {
        return Err(RulesError::RunAlreadyConcluded { phase });
    }

    let mut events = match action {
        RunAction::JackOut => jack_out(state)?,
        RunAction::Continue => continue_run(state)?,
        RunAction::ResolveSubroutine(index) => step_subroutine(state, index, true, registry)?,
        RunAction::BreakSubroutine(index) => step_subroutine(state, index, false, registry)?,
    };

    // `IceEncountered`/`RunSucceeded` may have just been emitted above (only
    // ever from the `Continue` arm) — dispatched here, in the one function
    // every `Continue` step funnels through (this crate's own
    // `PlayerAction::ContinueRun` handler, and `paid_ability::close_window`'s
    // window-mediated auto-continue alike), so `Trigger::OnEncounter`/
    // `OnSuccessfulRun`/`OnSuccessfulRunOnHq` fire identically regardless of
    // which path reached this transition. Filtered to exactly these two
    // variants (not a blanket dispatch of every event returned above) so a
    // subroutine effect that happens to emit some other dispatch-relevant
    // event (e.g. `Effect::InitiateRun`'s own `RunInitiated`, which already
    // dispatches `OnRunStart` itself) is never fired twice.
    let reactive: Vec<GameEvent> = events
        .iter()
        .filter(|event| matches!(event, GameEvent::IceEncountered { .. } | GameEvent::RunSucceeded { .. }))
        .cloned()
        .collect();
    for event in reactive {
        events.extend(dispatcher::dispatch_event(state, registry, &event)?);
    }

    Ok(events)
}

/// Resolves `PlayerAction::JackOut`, per its doc comment's four
/// Netrunner/Null Signal Games-style legality windows — gated entirely by
/// `RunState::jack_out_permitted`
/// (`RulesError::IllegalJackOutWindow` if `false`), which `continue_run`/
/// `pass_current_ice` keep in sync at every transition.
fn jack_out(state: &mut GameState) -> Result<Vec<GameEvent>, RulesError> {
    // Safe: advance_run already confirmed active_run is Some before dispatching here.
    let run = state.active_run.as_mut().expect("active_run checked by advance_run");
    if !run.jack_out_permitted {
        return Err(RulesError::IllegalJackOutWindow { phase: run.phase });
    }
    run.phase = RunPhase::Ended;
    Ok(vec![GameEvent::RunJackedOut { server: run.server }])
}

fn continue_run(state: &mut GameState) -> Result<Vec<GameEvent>, RulesError> {
    let run = state.active_run.as_mut().expect("active_run checked by advance_run");

    match run.phase {
        RunPhase::Initiation => {
            let server = run.server;
            run.phase = phase_for_position(&run.ice, 0);
            if run.phase == RunPhase::Success {
                // Reached the server approach step immediately (no ICE to
                // pass) — the window opens here too (Netrunner/Null Signal
                // Games rule 3).
                run.jack_out_permitted = true;
                Ok(vec![GameEvent::RunSucceeded { server }])
            } else {
                // Initial approach of the outermost ICE — window stays
                // closed (already `false` from `initiate_run`;
                // Netrunner/Null Signal Games rule 1).
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

            // Committing to the encounter closes whatever window was open
            // (Netrunner/Null Signal Games rule 4 — no jack-out
            // mid-encounter).
            run.jack_out_permitted = false;
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

            // The encounter is genuinely ending (no pending subroutines
            // left) — clear any `BoostDuration::Encounter` strength buffs
            // before advancing. Covers both a direct `ContinueRun` action
            // and `paid_ability::close_window`'s `EncounterIce` arm, which
            // itself calls into this function.
            state.runner.reset_encounter_strength_buffs();
            let run = state.active_run.as_mut().expect("active_run checked above");
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

fn step_subroutine(
    state: &mut GameState,
    index: usize,
    resolve: bool,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let to = if resolve { SubroutineStatus::Resolved } else { SubroutineStatus::Broken };
    let (card_id, effect) = transition_subroutine(state, index, to)?;

    if resolve {
        let mut events = vec![GameEvent::SubroutineFired { card_id, index, effect: effect.clone() }];
        events.extend(evaluate_effect(state, &effect, &mut crate::rules::ability::ResolutionContext::default(), registry)?);
        Ok(events)
    } else {
        Ok(vec![GameEvent::SubroutineBroken { card_id, index }])
    }
}

/// Ends the active run — the **only** way `active_run` goes from `Some` to
/// `None` outside of tests.
///
/// Three things must happen together whenever a run ends, however it
/// ends: `last_completed_run` is snapshotted so a deferred
/// `Trigger::OnRunEnded` can still see the run; `active_run` is cleared;
/// and every `BoostDuration::Encounter` strength buff is reset. Six sites
/// used to clear the run by hand and only the normal ICE pass reset the
/// buffs, so a Runner bounced by an unbroken "end the run" subroutine kept
/// every pumped point of breaker strength for the rest of the turn, and
/// into their next run, for free (ROADMAP Rules Audit T8). Same move as
/// `drain_deferred_triggers` and `memory::refresh`: one choke point rather
/// than N sites that can each forget one of the three.
///
/// Returns the run it ended, for callers that still need to read it
/// (`access_server`'s `RunCompleted`, `jack_out`'s event lookup).
pub(crate) fn end_run(state: &mut GameState) -> Option<RunState> {
    let run = state.active_run.take();
    if let Some(run) = &run {
        state.last_completed_run = Some(CompletedRun::snapshot(run));
    }
    state.runner.reset_encounter_strength_buffs();
    run
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardId, Effect, IceType, SubroutineDef};
    use crate::rules::run::state::{EncounteredSubroutine, RunState, ServerId};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, Credits, GamePhase, MemoryUnits, PlayerResources,
        RunnerState, Side,
    };

    fn game_state() -> GameState {
        GameState {
            corp: CorpState {
                resources: PlayerResources {
                    credits: Credits(5),
                    clicks: Clicks(3),
                    agenda_points: AgendaPoints(0),
                },
                ..Default::default()
            },
            runner: RunnerState {
                resources: PlayerResources {
                    credits: Credits(5),
                    clicks: Clicks(4),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                ..Default::default()
            },
            phase: GamePhase::Action(Side::Runner),
            ..Default::default()
        }
    }

    fn run_state(phase: RunPhase, ice: Vec<RunIce>, position: usize) -> RunState {
        run_state_with_jack_out(phase, ice, position, true)
    }

    fn run_state_with_jack_out(
        phase: RunPhase,
        ice: Vec<RunIce>,
        position: usize,
        jack_out_permitted: bool,
    ) -> RunState {
        RunState {
            server: ServerId::Hq,
            phase,
            ice,
            position,
            jack_out_permitted,
            ..Default::default()
        }
    }


    /// Builds a `RunIce` with `subroutine_count` placeholder `Pending`
    /// subroutines — identity/effect content doesn't matter for tests using
    /// this, only status transitions and counts do.
    fn test_ice(card_id: &str, strength: i32, subroutine_count: usize, rezzed: bool) -> RunIce {
        RunIce {
            card_id: CardId(card_id.to_string()),
            current_strength: strength,
            ice_type: IceType::Barrier,
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
        let events = advance_run(&mut state, RunAction::Continue, &CardRegistry::new()).expect("should succeed");

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
        let events = advance_run(&mut state, RunAction::Continue, &CardRegistry::new()).expect("should succeed");

        assert_eq!(state.active_run.unwrap().phase, RunPhase::Success);
        assert_eq!(events, vec![GameEvent::RunSucceeded { server: ServerId::Hq }]);
    }

    #[test]
    fn approach_ice_continue_enters_encounter_ice() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::ApproachIce, vec![test_ice("ice_wall", 3, 2, true)], 0));
        let events = advance_run(&mut state, RunAction::Continue, &CardRegistry::new()).expect("should succeed");

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
        let events = advance_run(&mut state, RunAction::Continue, &CardRegistry::new()).expect("should succeed");

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
        let events = advance_run(&mut state, RunAction::Continue, &CardRegistry::new()).expect("should succeed");

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
        let events = advance_run(&mut state, RunAction::Continue, &CardRegistry::new()).expect("should succeed");

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
            advance_run(&mut state, RunAction::ResolveSubroutine(0), &CardRegistry::new()).expect("should succeed");

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

        let events = advance_run(&mut state, RunAction::ResolveSubroutine(0), &CardRegistry::new()).expect("should succeed");

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
            advance_run(&mut state, RunAction::BreakSubroutine(0), &CardRegistry::new()).expect("should succeed");

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
        let result = advance_run(&mut state, RunAction::Continue, &CardRegistry::new());

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
        let events = advance_run(&mut state, RunAction::Continue, &CardRegistry::new()).expect("should succeed");

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
    fn continue_run_leaving_encounter_ice_resets_encounter_buff_but_not_turn_buff() {
        use crate::rules::state::InstalledRunnerCard;

        let mut state = game_state();
        state.runner.rig = vec![InstalledRunnerCard {
            card: CardId("corroder".to_string()),
            base_strength: 2,
            encounter_strength_buff: 1,
            turn_strength_buff: 3,
            ..Default::default()
        }];
        state.active_run = Some(run_state(
            RunPhase::EncounterIce,
            vec![test_ice("ice_wall_0", 0, 0, true), test_ice("ice_wall_1", 0, 3, true)],
            0,
        ));

        advance_run(&mut state, RunAction::Continue, &CardRegistry::new()).expect("should succeed");

        assert_eq!(state.runner.rig[0].encounter_strength_buff, 0);
        assert_eq!(state.runner.rig[0].turn_strength_buff, 3);
    }

    #[test]
    fn encounter_ice_continue_after_last_ice_reaches_success() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 0, true)], 0));
        let events = advance_run(&mut state, RunAction::Continue, &CardRegistry::new()).expect("should succeed");

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
        let result = advance_run(&mut state, RunAction::ResolveSubroutine(0), &CardRegistry::new());

        assert_eq!(result, Err(RulesError::InvalidSubroutineIndex(0)));
    }

    #[test]
    fn break_subroutine_outside_encounter_ice_errors() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::ApproachIce, vec![test_ice("ice_wall", 0, 2, true)], 0));
        let result = advance_run(&mut state, RunAction::BreakSubroutine(0), &CardRegistry::new());

        assert_eq!(result, Err(RulesError::NotInEncounter));
    }

    #[test]
    fn break_subroutine_already_handled_errors() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 1, true)], 0));
        advance_run(&mut state, RunAction::BreakSubroutine(0), &CardRegistry::new()).expect("should succeed");
        let result = advance_run(&mut state, RunAction::ResolveSubroutine(0), &CardRegistry::new());

        assert_eq!(result, Err(RulesError::SubroutineAlreadyHandled));
    }

    #[test]
    fn jack_out_from_initiation_fails() {
        let mut state = game_state();
        state.active_run =
            Some(run_state_with_jack_out(RunPhase::Initiation, vec![test_ice("ice_wall", 0, 1, true)], 0, false));
        let result = advance_run(&mut state, RunAction::JackOut, &CardRegistry::new());

        assert_eq!(result, Err(RulesError::IllegalJackOutWindow { phase: RunPhase::Initiation }));
        assert_eq!(state.active_run.unwrap().phase, RunPhase::Initiation);
    }

    #[test]
    fn jack_out_during_initial_approach_ice_fails() {
        let mut state = game_state();
        state.active_run = Some(run_state_with_jack_out(
            RunPhase::ApproachIce,
            vec![test_ice("ice_wall", 0, 1, true)],
            0,
            false,
        ));
        let result = advance_run(&mut state, RunAction::JackOut, &CardRegistry::new());

        assert_eq!(result, Err(RulesError::IllegalJackOutWindow { phase: RunPhase::ApproachIce }));
        assert_eq!(state.active_run.unwrap().phase, RunPhase::ApproachIce);
    }

    #[test]
    fn jack_out_after_passing_first_ice_succeeds() {
        let mut state = game_state();
        // First ICE unrezzed — `Continue` auto-passes it, opening the
        // jack-out window before the second (rezzed) ICE is approached.
        state.active_run = Some(run_state_with_jack_out(
            RunPhase::ApproachIce,
            vec![test_ice("ice_wall_0", 0, 0, false), test_ice("ice_wall_1", 0, 1, true)],
            0,
            false,
        ));
        advance_run(&mut state, RunAction::Continue, &CardRegistry::new()).expect("should auto-pass the unrezzed ICE");
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::ApproachIce);
        assert_eq!(state.active_run.as_ref().unwrap().position, 1);

        let events = advance_run(&mut state, RunAction::JackOut, &CardRegistry::new()).expect("should succeed");

        assert_eq!(state.active_run.unwrap().phase, RunPhase::Ended);
        assert_eq!(events, vec![GameEvent::RunJackedOut { server: ServerId::Hq }]);
    }

    #[test]
    fn jack_out_during_encounter_ice_fails() {
        let mut state = game_state();
        state.active_run =
            Some(run_state_with_jack_out(RunPhase::EncounterIce, vec![test_ice("ice_wall", 0, 5, true)], 0, false));
        let result = advance_run(&mut state, RunAction::JackOut, &CardRegistry::new());

        assert_eq!(result, Err(RulesError::IllegalJackOutWindow { phase: RunPhase::EncounterIce }));
        let run = state.active_run.unwrap();
        assert_eq!(run.phase, RunPhase::EncounterIce);
        assert!(run.ice[0].subroutines.iter().all(|s| s.status == SubroutineStatus::Pending));
    }

    #[test]
    fn continue_after_success_errors() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::Success, vec![], 0));
        let result = advance_run(&mut state, RunAction::Continue, &CardRegistry::new());

        assert_eq!(
            result,
            Err(RulesError::RunAlreadyConcluded { phase: RunPhase::Success })
        );
    }

    #[test]
    fn jack_out_after_reaching_success_succeeds() {
        let mut state = game_state();
        state.active_run = Some(run_state_with_jack_out(RunPhase::Success, vec![], 0, true));
        let events = advance_run(&mut state, RunAction::JackOut, &CardRegistry::new()).expect("should succeed");

        assert_eq!(state.active_run.unwrap().phase, RunPhase::Ended);
        assert_eq!(events, vec![GameEvent::RunJackedOut { server: ServerId::Hq }]);
    }

    #[test]
    fn action_after_ended_errors() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::Ended, vec![], 0));
        let result = advance_run(&mut state, RunAction::Continue, &CardRegistry::new());

        assert_eq!(
            result,
            Err(RulesError::RunAlreadyConcluded { phase: RunPhase::Ended })
        );
    }

    #[test]
    fn advance_run_with_no_active_run_errors() {
        let mut state = game_state();
        let result = advance_run(&mut state, RunAction::Continue, &CardRegistry::new());

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }
}
