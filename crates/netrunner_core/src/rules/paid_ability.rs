//! Paid Ability Windows (PAWs): the priority-passing sub-loop that pauses
//! the run flow at each checkpoint (ICE approach, ICE encounter, pre-access)
//! so both sides get a chance to fire paid abilities before the engine
//! auto-advances. See `state::PaidAbilityWindow`'s doc comment for the data
//! model and `PlayerAction::PassPriority`'s for the player-facing contract.

use crate::cards::CardRegistry;
use crate::dsl::{CardId, Trigger};
use crate::rules::ability;
use crate::rules::damage;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::{self, AccessPhase, RunAction, RunPhase};
use crate::rules::state::{GamePhase, GameState, PaidAbilityWindow, PendingPreventionKind, PreventionResume, Side, WindowCheckpoint};
use crate::rules::turn;

/// Opens a PAW at `checkpoint` with `active_priority` getting priority first
/// (Netrunner/Null Signal Games priority rule 1). The general form behind
/// `open_window`/`open_window_if_at_checkpoint` (run checkpoints) and
/// `turn::end_turn`/`turn::enter_start_of_turn` (turn-boundary checkpoints).
pub(crate) fn open_window_for(state: &mut GameState, active_priority: Side, checkpoint: WindowCheckpoint) -> GameEvent {
    state.paid_ability_window =
        Some(PaidAbilityWindow { active_priority, consecutive_passes: 0, checkpoint, return_phase: Box::new(state.phase) });
    GameEvent::PaidAbilityWindowOpened { side: active_priority }
}

/// Opens a PAW at `WindowCheckpoint::Run`: the active-turn side gets
/// priority first. Only ever called while `state.phase == Action(_)` — the
/// only phase a run (and therefore a run-checkpoint window) can exist in.
pub(crate) fn open_window(state: &mut GameState) -> GameEvent {
    let active_priority = match state.phase {
        GamePhase::Action(side) => side,
        _ => unreachable!("open_window only called during Action(_) / mid-run"),
    };
    open_window_for(state, active_priority, WindowCheckpoint::Run)
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
///
/// **The `Action(_)` guard is load-bearing, not defensive.** A run can
/// outlive the turn it belongs to: `resolve_unbroken_subroutines` can
/// flatline the Runner (and `run::access_server` can hand over the winning
/// agenda) *mid-run*, which sets `GamePhase::GameOver` while `active_run`
/// is still `Some` at a checkpoint. Every caller here then auto-advances to
/// the next ICE or the next accessed card and asks for a window, and
/// `open_window` — which reads the active side straight off `phase` —
/// reached its `unreachable!()`. That was a genuine crash in ordinary play:
/// a subroutine flatlining the Runner with more ICE behind it panicked the
/// engine. It surfaced inside `build_client_view`, because `legal_actions`
/// probes candidates through `apply_action`, so merely *rendering* such a
/// position brought the process down.
///
/// Guarding here rather than at the three call sites is deliberate: it is
/// the one place all of them funnel through, and "the game is over, so
/// there is no checkpoint left to react at" is a property of the
/// checkpoint question itself, not of any one caller.
pub(crate) fn open_window_if_at_checkpoint(state: &mut GameState) -> Option<GameEvent> {
    if !matches!(state.phase, GamePhase::Action(_)) {
        return None;
    }
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
/// Also defensively clears a now-stale `WindowCheckpoint::Run` window if the
/// action ended the run out from under it (e.g. a `Trigger::Paid` ability's
/// `Effect::EndTheRun` firing mid-window) — leaving a run window open with no
/// active run would permanently block every ordinary action afterward, since
/// it could never be closed (`close_run_window` only resumes a run step;
/// there'd be none). Scoped to `Run` specifically: `StartOfTurn`/`EndOfTurn`
/// windows have no active run by construction, so `active_run.is_none()`
/// alone can't be the staleness signal for them the way it is for `Run`.
pub(crate) fn note_window_action(state: &mut GameState, side: Side) {
    let is_stale_run_window = state.active_run.is_none()
        && matches!(state.paid_ability_window.as_ref().map(|w| w.checkpoint), Some(WindowCheckpoint::Run));
    if is_stale_run_window {
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
        // Capture the checkpoint before clearing — `close_window` resumes
        // based on it, and nothing else remembers which checkpoint this
        // window belonged to once `paid_ability_window` is `None`.
        let checkpoint = window.checkpoint;
        events.push(GameEvent::PaidAbilityWindowClosed);
        state.paid_ability_window = None;
        events.extend(close_window(state, registry, checkpoint)?);
    } else {
        window.active_priority = acting_side.other();
    }
    Ok(events)
}

/// Auto-advances whatever step was paused, once both sides have passed
/// (rule 3), per `checkpoint`.
fn close_window(
    state: &mut GameState,
    registry: &CardRegistry,
    checkpoint: WindowCheckpoint,
) -> Result<Vec<GameEvent>, RulesError> {
    match checkpoint {
        WindowCheckpoint::Run => close_run_window(state, registry),
        WindowCheckpoint::StartOfTurn { side } => {
            state.phase = GamePhase::Action(side);
            Ok(Vec::new())
        }
        WindowCheckpoint::EndOfTurn { side } => turn::finish_end_turn(state, side, registry),
        WindowCheckpoint::Prevention => close_prevention_window(state, registry),
        WindowCheckpoint::PostAction { side } => {
            // Nothing to resume — the window never changed the phase, and
            // the acting player simply carries on with their turn. Set it
            // explicitly anyway rather than relying on that, matching the
            // `StartOfTurn` arm.
            state.phase = GamePhase::Action(side);
            Ok(Vec::new())
        }
    }
}

/// Whether `side` has any `Trigger::Paid` ability they could actually
/// activate right now — their requirement met and their cost affordable.
///
/// The gate on `WindowCheckpoint::PostAction`. Without it every basic
/// action would cost both players a `PassPriority` whether or not anyone
/// had anything to do, roughly doubling the length of a game.
///
/// Deliberately **not** implemented by probing `legal_actions`: that calls
/// `apply_action`, which is where the window is opened, so it would recur
/// without bound. This asks the same question directly instead —
/// `check_requirement` plus `ability::cost_is_affordable`, neither of which
/// mutates or re-enters the engine.
///
/// It over-approximates in one direction on purpose: it does not evaluate
/// the effect, so an ability that would fail at resolution still counts.
/// That opens an occasional empty window (harmless — two passes close it),
/// whereas under-approximating would silently deny a player a window they
/// were entitled to. The reason this is *affordable* at all is
/// `EffectRequirement::DuringEncounter`: before it, every icebreaker in the
/// rig answered "yes" here on every action of every turn.
pub(crate) fn has_usable_paid_ability(state: &GameState, registry: &CardRegistry, side: Side) -> bool {
    active_cards_of(state, side).into_iter().any(|card_id| {
        let Some(card) = registry.get(&card_id) else { return false };
        card.abilities.iter().any(|ability| {
            ability.trigger == Trigger::Paid
                && ability
                    .requirement
                    .as_ref()
                    .is_none_or(|req| ability::check_requirement(state, req, side, &ability::ResolutionContext::for_card(Some(&card_id)), registry).is_ok())
                && ability
                    .cost
                    .as_ref()
                    .is_none_or(|cost| ability::cost_is_affordable(state, side, cost, Some(&card_id)))
        })
    })
}

/// Every card `side` could activate a paid ability from: their rezzed
/// installs (Corp) or rig (Runner), plus their identity.
fn active_cards_of(state: &GameState, side: Side) -> Vec<CardId> {
    let mut cards: Vec<CardId> = match side {
        Side::Corp => state.corp.installed.iter().filter(|c| c.rezzed).map(|c| c.card.clone()).collect(),
        Side::Runner => state.runner.rig.iter().map(|c| c.card.clone()).collect(),
    };
    let identity = match side {
        Side::Corp => state.corp.identity.clone(),
        Side::Runner => state.runner.identity.clone(),
    };
    cards.extend(identity);
    cards
}

/// `close_window`'s `WindowCheckpoint::Prevention` arm: applies whatever's
/// left unprevented, emits `DamagePrevented`/`TrashPrevented` for whatever
/// was, and — if this was parked mid-subroutine-resolution — resumes
/// `resolve_encounter_ice`'s loop, mirroring `close_run_window`'s
/// `EncounterIce` arm's own resumption call.
fn close_prevention_window(state: &mut GameState, registry: &CardRegistry) -> Result<Vec<GameEvent>, RulesError> {
    let pending = state
        .pending_prevention
        .take()
        .expect("WindowCheckpoint::Prevention implies pending_prevention is Some");

    let mut events = match pending.kind {
        PendingPreventionKind::Damage { damage_type, amount, prevented } => {
            let mut events = Vec::new();
            if prevented > 0 {
                events.push(GameEvent::DamagePrevented { amount: prevented });
            }
            // Discards are dropped here rather than recorded: a prevention
            // window resumes on a later `PlayerAction`, so there is no
            // `Sequence` left for `LastDamageTrashedOddCostCard` to read
            // them from. That was already true of the old `GameState` field
            // in practice — a parked `DealDamage` breaks its `Sequence`, so
            // the requirement was never reached down this path.
            let (damage_events, _discarded) = damage::apply_damage(state, damage_type, amount.saturating_sub(prevented));
            events.extend(damage_events);
            events
        }
        PendingPreventionKind::Trash { target, prevented } => {
            if prevented {
                vec![GameEvent::TrashPrevented { target }]
            } else {
                // Calls `ability::trash_card` directly rather than
                // re-evaluating `Effect::TrashCard` through
                // `evaluate_effect` — that entry point re-checks whether to
                // park a *new* prevention window, which would loop forever
                // here (the card granting the ability doesn't get "used up"
                // by one activation).
                ability::trash_card(state, &target, pending.source_card.as_ref())?
            }
        }
    };

    if pending.resume == PreventionResume::ResumeSubroutines {
        events.extend(resolve_encounter_ice(state, registry)?);
    }
    Ok(events)
}

/// `close_window`'s `WindowCheckpoint::Run` arm. Keys off `state.active_run`'s
/// *current* `RunPhase` — untouched while the window was open, since nothing
/// a window permits (`RezIce`, `BreakSubroutine`, `ActivateAbility`) mutates
/// `RunPhase` itself — rather than needing a separate discriminant.
fn close_run_window(state: &mut GameState, registry: &CardRegistry) -> Result<Vec<GameEvent>, RulesError> {
    let Some(run_phase) = state.active_run.as_ref().map(|r| r.phase) else {
        return Ok(Vec::new());
    };

    match run_phase {
        RunPhase::ApproachIce => {
            // Rez-or-not is already decided (any window-time RezIce already
            // flipped the matching RunIce::rezzed). Reuses continue_run's
            // existing ApproachIce arm: auto-pass if unrezzed, else commit
            // to EncounterIce.
            let mut events = run::advance_run(state, RunAction::Continue, registry)?;
            events.extend(open_window_if_at_checkpoint(state));
            Ok(events)
        }
        RunPhase::EncounterIce => resolve_encounter_ice(state, registry),
        RunPhase::Success => {
            // This window was opened by `complete_run`. Now actually
            // access — the logic `complete_run` used to run inline.
            let server = state.active_run.as_ref().expect("checked Some above").server;
            let mut events = run::access_server(state, server, registry)?;
            if state.active_run.is_none() {
                // Nothing was presented — an empty server, or a replaced
                // access — so the run is over here rather than in
                // `access::advance_or_finish`. `RunCompleted` is
                // *dispatched*, not merely pushed: a run on an empty
                // Archives is the most ordinary run there is, and Mayfly's
                // "when this run ends, trash this program" never fired on
                // one because this arm only recorded the event. Skipped
                // when access already concluded with its own
                // `RunCompleted` (a flatline mid-access).
                if !events.iter().any(|e| matches!(e, GameEvent::RunCompleted { .. })) {
                    let completed = GameEvent::RunCompleted { server };
                    events.push(completed.clone());
                    events.extend(crate::rules::dispatcher::dispatch_event(state, registry, &completed)?);
                }
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

/// Fires unbroken subroutines on the ICE currently being encountered, then
/// — unless doing so just parked a fresh `Effect::Trace`
/// (`GameState::active_trace` is `Some`) — advances past the ICE and opens a
/// checkpoint window if warranted. Shared by `close_window`'s `EncounterIce`
/// arm and `rules::trace::submit_runner_bid`'s post-resolution resumption:
/// both are "the subroutine loop just became unblocked, try to finish this
/// ICE." The `active_trace.is_none()` guard is needed even for the plain
/// `close_window` path: if a subroutine fired while a window was closing
/// parks a trace, this must not advance the run before the trace resolves.
pub(crate) fn resolve_encounter_ice(
    state: &mut GameState,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    // Whatever just resolved may have removed or derezzed the ICE being
    // encountered (`run::reconcile_ice`). If so, the encounter is over: its
    // remaining subroutines do not fire, and the run already stands at its
    // next checkpoint — open that window rather than `Continue` past it.
    let moved = run::reconcile_ice(state, registry)?;
    if !moved.is_empty() {
        let mut events = moved;
        events.extend(open_window_if_at_checkpoint(state));
        return Ok(events);
    }
    let mut events = ability::resolve_unbroken_subroutines(state, registry)?;
    // A subroutine's effect can itself park a *further* pending decision
    // (e.g. Ballista's subroutine offers a choice whose "trash a program"
    // branch is itself an `Effect::PromptChooseCards`) — `resolve_choice`/
    // `resolve_accept`/`resolve_decline` all call this function assuming
    // whatever they just resolved is fully settled, but it may not be.
    // Must not advance the run out from under a decision that's still
    // awaiting a `PlayerAction`.
    //
    // The `GameOver` check is the same "a run can outlive the game it
    // belongs to" invariant `open_window_if_at_checkpoint` documents, and it
    // has to be tested *here* rather than only there. A subroutine can
    // flatline the Runner (`damage::apply_damage` sets `GameOver` and leaves
    // `active_run` set), at which point `resolve_unbroken_subroutines`
    // breaks its loop — leaving the *rest* of a multi-subroutine ICE
    // `Pending`. Advancing then hands `run::advance_run` the one thing
    // `continue_run` refuses, `SubroutinesStillPending`, and that `Err`
    // propagates out through `close_window` into `pass_priority`.
    //
    // The consequence was a deadlock, not just a failed action: it made the
    // priority holder's own `PassPriority` illegal — `legal_actions` keeps
    // only candidates `apply_action` accepts — while `current_actor` still
    // named them, so the side had no legal action at all and the match
    // could never advance. Reachable on ordinary sample decks (seed 2 of
    // `decks::matchups()[0]`), and deterministic, therefore permanent.
    if !matches!(state.phase, GamePhase::GameOver(_))
        && state.active_run.is_some()
        && state.active_trace.is_none()
        && state.pending_prevention.is_none()
        && state.pending_paid_choice.is_none()
        && state.pending_decision.is_none()
    {
        events.extend(run::advance_run(state, RunAction::Continue, registry)?);
        events.extend(open_window_if_at_checkpoint(state));
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CardId, DamageType, Effect, IceType, SubroutineDef};
    use crate::rules::run::{EncounteredSubroutine, RunIce, RunState, ServerId, SubroutineStatus};
    use crate::rules::state::{ArchivedCard, AgendaPoints, Clicks, Credits, CorpState, MemoryUnits, PendingPrevention, PlayerResources, RunnerState,
    };

    fn registry() -> CardRegistry {
        CardRegistry::new()
    }

    fn base_state() -> GameState {
        GameState {
            corp: CorpState {
                resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
                ..Default::default()
            },
            runner: RunnerState {
                resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
                memory_units: MemoryUnits(0),
                ..Default::default()
            },
            phase: GamePhase::Action(Side::Runner),
            ..Default::default()
        }
    }

    fn run_ice(rezzed: bool, subroutines: Vec<EncounteredSubroutine>) -> RunIce {
        RunIce {
            install_id: crate::rules::InstallId::PLACEHOLDER,
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

    /// Regression: a run outliving the game it belongs to.
    ///
    /// `resolve_unbroken_subroutines` can flatline the Runner mid-encounter,
    /// setting `GameOver` while `active_run` is still parked at a
    /// checkpoint with more ICE behind it. `resolve_encounter_ice` then
    /// auto-advanced and asked for a window, and `open_window` — which
    /// reads the priority side straight off `phase` — hit its
    /// `unreachable!()`. Found by driving view-based agents across sample
    /// decks; it panicked inside `build_client_view`, since `legal_actions`
    /// probes candidates through `apply_action`.
    #[test]
    fn no_window_opens_at_a_checkpoint_once_the_game_is_already_over() {
        let mut state = base_state();
        state.phase = GamePhase::GameOver(Side::Corp);
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![run_ice(true, vec![pending_subroutine()])],
            ..Default::default()
        });

        assert_eq!(open_window_if_at_checkpoint(&mut state), None);
        assert!(state.paid_ability_window.is_none(), "a finished game has no checkpoint left to react at");
    }

    /// Regression: the deadlock half of "a run outliving the game".
    ///
    /// A subroutine that flatlines the Runner stops
    /// `resolve_unbroken_subroutines`' loop, leaving the rest of a
    /// multi-subroutine ICE `Pending`. `resolve_encounter_ice` then tried to
    /// advance past the ICE anyway; `continue_run` refused with
    /// `SubroutinesStillPending`, and that `Err` propagated out through
    /// `close_window` into `pass_priority` — making the priority holder's
    /// *own* `PassPriority` illegal, since `legal_actions` keeps only
    /// candidates `apply_action` accepts. `current_actor` still named them,
    /// so that side had no legal action at all and the match was stuck for
    /// good.
    ///
    /// Sibling of `no_window_opens_at_a_checkpoint_once_the_game_is_already_over`,
    /// which covers the panic half one statement later.
    #[test]
    fn closing_a_window_succeeds_when_a_subroutine_flatlines_with_more_subroutines_behind_it() {
        let mut state = base_state();
        // Empty grip, so 1 net damage is lethal.
        state.runner.grip = Vec::new();

        let lethal = EncounteredSubroutine {
            id: 0,
            definition: SubroutineDef { text: "do 1 net damage".to_string(), effect: Effect::DealDamage(DamageType::Net, 1) },
            status: SubroutineStatus::Pending,
        };
        let behind_it = EncounteredSubroutine { id: 1, ..pending_subroutine() };
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![run_ice(true, vec![lethal, behind_it])],
            ..Default::default()
        });
        crate::rules::test_support::install_the_runs_ice(&mut state);
        // The Runner already passed, so the Corp's pass is the one that
        // closes the window — exactly how a `Run` window reaches Corp
        // priority, since `open_window` reads the active side off `phase`.
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Corp,
            consecutive_passes: 1,
            checkpoint: WindowCheckpoint::Run,
            return_phase: Box::new(state.phase),
        });

        let events = pass_priority(&mut state, &registry(), Side::Corp)
            .expect("the priority holder's own pass must never be rejected");

        assert!(events.iter().any(|e| matches!(e, GameEvent::RunnerFlatlined)));
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
        // The trailing subroutine is left unresolved rather than forced
        // through — the game is already over, so there is nothing to advance.
        let run = state.active_run.as_ref().expect("the run outlives the game, and that is the invariant");
        assert_eq!(run.ice[0].subroutines[1].status, SubroutineStatus::Pending);
    }

    #[test]
    fn single_pass_toggles_priority_and_leaves_window_open() {
        let mut state = base_state();
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![run_ice(true, Vec::new())],
            ..Default::default()
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
            phase: RunPhase::ApproachIce,
            ice: vec![run_ice(true, Vec::new())],
            ..Default::default()
        });
        crate::rules::test_support::install_the_runs_ice(&mut state);
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
            phase: RunPhase::ApproachIce,
            ice: vec![run_ice(true, Vec::new())],
            ..Default::default()
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
            phase: RunPhase::ApproachIce,
            ice: vec![run_ice(false, Vec::new())],
            ..Default::default()
        });
        crate::rules::test_support::install_the_runs_ice(&mut state);
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
                GameEvent::ServerApproached { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn encounter_ice_window_close_auto_fires_unbroken_subroutines() {
        let mut state = base_state();
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![run_ice(true, vec![pending_subroutine()])],
            ..Default::default()
        });
        crate::rules::test_support::install_the_runs_ice(&mut state);
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
                GameEvent::ServerApproached { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn success_window_close_presenting_a_single_card_opens_a_fresh_access_window() {
        let mut state = base_state();
        state.corp.hq = vec![CardId("hedge_fund".to_string())];
        state.active_run = Some(RunState {
            phase: RunPhase::Success,
            jack_out_permitted: true,
            ..Default::default()
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

    /// A run on an empty server still *ends*, and `Trigger::OnRunEnded`
    /// must hear about it. This arm used to push `RunCompleted` into the
    /// event list without dispatching it, so Mayfly's "when this run ends,
    /// trash this program" never fired on the most ordinary run there is —
    /// a run at an empty Archives.
    #[test]
    fn closing_the_success_window_on_an_empty_server_dispatches_run_ended() {
        let mayfly = CardId("mayfly".to_string());
        let registry = CardRegistry::from_cards(vec![crate::dsl::CardDefinition {
            id: mayfly.clone(),
            title: "Mayfly".to_string(),
            side: Side::Runner,
            card_type: crate::dsl::CardType::Program,
            triggers: vec![crate::dsl::TriggeredEffect {
                trigger: crate::dsl::Trigger::OnRunEnded,
                effects: vec![crate::dsl::Effect::GainCredits(Side::Runner, 1)],
                requirement: None,
            }],
            is_playable: true,
            ..Default::default()
        }]);
        let mut state = base_state();
        state.runner.rig = vec![crate::rules::state::InstalledRunnerCard { card: mayfly, ..Default::default() }];
        state.active_run = Some(RunState {
            server: ServerId::Archives,
            phase: RunPhase::Success,
            jack_out_permitted: true,
            ..Default::default()
        });
        open_window(&mut state);
        let credits_before = state.runner.resources.credits;

        pass_priority(&mut state, &registry, Side::Runner).expect("first pass should succeed");
        let events = pass_priority(&mut state, &registry, Side::Corp).expect("second pass should succeed");

        assert!(state.active_run.is_none());
        assert!(events.contains(&GameEvent::RunCompleted { server: ServerId::Archives }));
        assert_eq!(state.runner.resources.credits, credits_before.gain(1), "OnRunEnded fired");
        assert_eq!(state.last_completed_run.as_ref().map(|r| r.server), Some(ServerId::Archives));
    }

    #[test]
    fn success_window_close_presenting_select_next_card_does_not_open_a_window() {
        let mut state = base_state();
        // Archives access every card in it, so two cards there yields a
        // `SelectNextCard` choice rather than a single `PendingChoice` —
        // deliberately not a checkpoint (no cost is at stake in ordering).
        state.corp.archives =
            vec![ArchivedCard::facedown(CardId("card_1".to_string())), ArchivedCard::facedown(CardId("card_2".to_string()))];
        state.active_run = Some(RunState {
            server: ServerId::Archives,
            phase: RunPhase::Success,
            jack_out_permitted: true,
            ..Default::default()
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
            phase: RunPhase::EncounterIce,
            ice: vec![run_ice(true, vec![end_the_run_subroutine])],
            ..Default::default()
        });
        crate::rules::test_support::install_the_runs_ice(&mut state);
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

    #[test]
    fn prevention_window_close_applies_remaining_unprevented_damage_and_emits_damage_prevented() {
        let mut state = base_state();
        state.runner.grip = vec![CardId("card_0".to_string()), CardId("card_1".to_string()), CardId("card_2".to_string())];
        state.pending_prevention = Some(PendingPrevention {
            kind: PendingPreventionKind::Damage { damage_type: DamageType::Net, amount: 3, prevented: 1 },
            source_card: None,
            resume: PreventionResume::None,
        });
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            checkpoint: WindowCheckpoint::Prevention,
            return_phase: Box::new(state.phase),
        });

        pass_priority(&mut state, &registry(), Side::Runner).expect("first pass should succeed");
        let events = pass_priority(&mut state, &registry(), Side::Corp).expect("second pass should succeed");

        assert!(state.pending_prevention.is_none());
        assert!(state.paid_ability_window.is_none());
        // 3 damage parked, 1 already prevented — 2 actually land.
        assert_eq!(state.runner.grip.len(), 1);
        assert_eq!(state.runner.heap.len(), 2);
        assert!(events.contains(&GameEvent::DamagePrevented { amount: 1 }));
        assert!(events.contains(&GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 2 }));
    }

    #[test]
    fn prevention_window_parked_mid_subroutine_resolution_resumes_remaining_subroutines_on_close() {
        let mut state = base_state();
        state.runner.grip = vec![CardId("card_0".to_string())];
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce {
                install_id: crate::rules::InstallId::PLACEHOLDER,
                card_id: CardId("ice_wall".to_string()),
                current_strength: 0,
                ice_type: IceType::Barrier,
                subroutines: vec![
                    EncounteredSubroutine {
                        id: 0,
                        definition: SubroutineDef { text: "damage".to_string(), effect: Effect::DealDamage(DamageType::Net, 1) },
                        status: SubroutineStatus::Resolved,
                    },
                    EncounteredSubroutine {
                        id: 1,
                        definition: SubroutineDef { text: "end the run".to_string(), effect: Effect::EndTheRun },
                        status: SubroutineStatus::Pending,
                    },
                ],
                rezzed: true,
            }],
            ..Default::default()
        });
        crate::rules::test_support::install_the_runs_ice(&mut state);
        state.pending_prevention = Some(PendingPrevention {
            kind: PendingPreventionKind::Damage { damage_type: DamageType::Net, amount: 1, prevented: 0 },
            source_card: None,
            resume: PreventionResume::ResumeSubroutines,
        });
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            checkpoint: WindowCheckpoint::Prevention,
            return_phase: Box::new(state.phase),
        });

        pass_priority(&mut state, &registry(), Side::Runner).expect("first pass should succeed");
        pass_priority(&mut state, &registry(), Side::Corp).expect("second pass should succeed");

        assert!(state.runner.grip.is_empty(), "the parked damage should have applied");
        assert!(
            state.active_run.is_none(),
            "the remaining pending EndTheRun subroutine must fire once the prevention window closes"
        );
    }
}
