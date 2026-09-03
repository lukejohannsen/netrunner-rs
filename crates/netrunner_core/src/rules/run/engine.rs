use crate::cards::CardRegistry;
use crate::dsl::{CardId, CardType, Effect, StrengthModifier};
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
///
/// `Ok(None)` for a card that is not ICE at all. `install_card` and
/// `Effect::InstallFromZoneIgnoringCost` both refuse to put a non-ICE into
/// an ICE slot, so this is unreachable through `apply_action`; it exists so
/// a hand-built state cannot make the run encounter an agenda as a
/// 0-strength barrier — which is what the `_ => IceType::Barrier` arm that
/// stood here did.
fn build_run_ice(installed: &InstalledCard, registry: &CardRegistry) -> Result<Option<RunIce>, RulesError> {
    let card_def = registry
        .get(&installed.card)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(installed.card.clone()))?;
    let CardType::Ice(ice_type) = card_def.card_type else {
        return Ok(None);
    };
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
        // Ice Wall: a rate, not a threshold.
        Some(StrengthModifier::PerHostedAdvancement(per)) => per * installed.advancement_tokens as i32,
        _ => 0,
    };
    let current_strength = card_def.strength.unwrap_or(0) + modifier_bonus;
    let subroutines = card_def
        .subroutines
        .iter()
        .enumerate()
        .map(|(id, def)| EncounteredSubroutine { id, definition: def.clone(), status: SubroutineStatus::Pending })
        .collect();

    Ok(Some(RunIce {
        install_id: installed.install_id,
        card_id: installed.card.clone(),
        current_strength,
        ice_type,
        subroutines,
        rezzed: installed.rezzed,
    }))
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
    // Every run, however it was started — see `RunnerState::servers_run_this_turn`.
    state.runner.servers_run_this_turn.push(server);

    // `corp.installed`'s Vec order is install order (oldest first); installs
    // only ever `.push()`, so oldest install = outermost ICE = index 0,
    // matching `RunIce`'s outermost-to-innermost doc comment.
    let ice: Vec<RunIce> = state
        .corp
        .installed
        .iter()
        .filter(|installed| installed.server == server && installed.slot == InstallSlot::Ice)
        .map(|installed| build_run_ice(installed, registry))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();

    state.active_run = Some(RunState { agendas_stolen_this_run: 0, persistent_trashed_upgrades: Vec::new(), redirect_on_approach: None, on_end_effect: None, on_end_card: None, on_end_install: None, end_run_prevention: None, subroutine_resolved: false, initiated_by: None, ice_bypassed: false,
        on_success_effect: None,
        on_success_card: None,
        on_success_install: None,
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
    run.ice_bypassed = false;
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
        RunPhase::Success => events.push(GameEvent::ServerApproached { server }),
        _ => {}
    }
    events
}

/// Brings `run.ice` back into step with the attacked server's ICE in
/// `corp.installed`, and moves the run if the ICE it was standing on is
/// gone. Returns the step events (`IceApproached`/`ServerApproached`, or
/// `pass_current_ice`'s) when the run moved, and nothing otherwise.
///
/// `run.ice` was a snapshot taken by `start_run` and never revisited, which
/// was fine for as long as nothing changed a server's ICE mid-run. Things
/// do: *Brân 1.0*'s subroutine installs a piece of ICE "directly inward
/// from this ice" — into `corp.installed`, where the run never looked, so
/// the new ICE was never approached and the run reached the server one ICE
/// early (ROADMAP Rules Audit, Tier 2). `Effect::DerezCard` left the
/// snapshot's `rezzed` stale, and `Effect::SwapInstalledIce` had to refuse
/// outright during a run because the snapshot could not follow it.
///
/// Rebuilt from `corp.installed` in its order (install order = outermost
/// first, the convention `start_run` documents), keyed by `InstallId`: an
/// entry that survives keeps its per-run state — subroutine statuses and
/// `current_strength`, which carries this run's `ModifyStrength` deltas
/// and must not be recomputed — with only `rezzed` re-read from the
/// install. A new install gets a fresh `RunIce`. Then the run is
/// re-anchored on the `InstallId` it was standing on:
///
/// - still there → `position` follows it (ICE trashed outward of the
///   Runner shifts it down, ICE installed outward shifts it up — neither is
///   approached again; ICE installed inward will be, in turn). Derezzed
///   while being *encountered* → the encounter ends and the ICE counts as
///   passed, the same transition `continue_run` makes for an unrezzed
///   approach.
/// - gone → the run stands on the first ICE it has not passed, or reaches
///   the server if there is none. Netrunner: the encounter ends, nothing
///   more of that ICE resolves (`resolve_unbroken_subroutines` stops when
///   the phase is no longer `EncounterIce`), the Runner is not said to have
///   *passed* it (no `IcePassed`), and the jack-out window that a passed
///   ICE would open opens here too.
///
/// Called from the `engine::apply_action` choke point after the deferred
/// drain (so a trigger's trash or derez is seen), from `advance_run` before
/// a `Continue` step (Brân's install lands inside the same handler that
/// then passes Brân, and `pass_current_ice` must see the new length), and
/// from `paid_ability::resolve_encounter_ice` before firing subroutines.
/// When the run moved, a `WindowCheckpoint::Run` window left open belonged
/// to a step that no longer exists and is cleared — closing it normally
/// would have resumed into a phase it was never opened for — and
/// `ServerApproached` is dispatched so `Trigger::OnApproachServer` fires,
/// as `advance_run` does for the ordinary path. It opens no window itself;
/// every caller already opens one at the checkpoint it lands on.
///
/// Phases `AccessingCard`/`Ended` are left alone: the list is never read
/// again. `Initiation` re-anchors at 0 (nothing has been approached; a
/// newly-outermost ICE is approached first); `Success` re-anchors at the
/// end (ICE installed after the approach is not approached — the rules
/// agree).
pub(crate) fn reconcile_ice(state: &mut GameState, registry: &CardRegistry) -> Result<Vec<GameEvent>, RulesError> {
    // A finished game has no run to move (`win::end_game` clears it), but
    // say so explicitly: moving a run and dispatching `ServerApproached`
    // into a finished game could return an `Err` that rejected the very
    // action that ended it.
    if state.is_over() {
        return Ok(Vec::new());
    }
    let Some(run) = state.active_run.as_ref() else { return Ok(Vec::new()) };
    if matches!(run.phase, RunPhase::AccessingCard | RunPhase::Ended) {
        return Ok(Vec::new());
    }

    let mut rebuilt = Vec::new();
    for installed in state.corp.installed.iter().filter(|c| c.server == run.server && c.slot == InstallSlot::Ice) {
        match run.ice.iter().find(|ice| ice.install_id == installed.install_id) {
            Some(existing) => rebuilt.push(RunIce { rezzed: installed.rezzed, ..existing.clone() }),
            None => rebuilt.extend(build_run_ice(installed, registry)?),
        }
    }
    if rebuilt == run.ice {
        return Ok(Vec::new());
    }

    let mut run = state.active_run.take().expect("checked Some above");
    let server = run.server;
    let old_phase = run.phase;
    let anchor = run.ice.get(run.position).map(|ice| ice.install_id);
    let passed: Vec<_> = run.ice.iter().take(run.position).map(|ice| ice.install_id).collect();
    run.ice = rebuilt;

    let mut events = Vec::new();
    match old_phase {
        RunPhase::Initiation => run.position = 0,
        RunPhase::Success => run.position = run.ice.len(),
        RunPhase::ApproachIce | RunPhase::EncounterIce => {
            match anchor.and_then(|id| run.ice.iter().position(|ice| ice.install_id == id)) {
                Some(index) => {
                    run.position = index;
                    if old_phase == RunPhase::EncounterIce && !run.ice[index].rezzed {
                        state.runner.reset_encounter_strength_buffs();
                        events.extend(pass_current_ice(&mut run, index));
                    }
                }
                None => {
                    let position =
                        run.ice.iter().position(|ice| !passed.contains(&ice.install_id)).unwrap_or(run.ice.len());
                    run.position = position;
                    run.phase = phase_for_position(&run.ice, position);
                    if old_phase == RunPhase::EncounterIce {
                        state.runner.reset_encounter_strength_buffs();
                        run.jack_out_permitted = true;
                    }
                    match run.phase {
                        RunPhase::Success => {
                            run.jack_out_permitted = true;
                            events.push(GameEvent::ServerApproached { server });
                        }
                        _ => events.push(GameEvent::IceApproached { server, position: position as u32 }),
                    }
                }
            }
        }
        RunPhase::AccessingCard | RunPhase::Ended => unreachable!("returned above"),
    }
    state.active_run = Some(run);

    if !events.is_empty() {
        // Same staleness rule as `paid_ability::note_window_action`: a run
        // window is scoped to the step it was opened at, and that step is
        // gone. Cleared silently, as there; the caller opens the next one.
        if matches!(state.paid_ability_window.as_ref().map(|w| w.checkpoint), Some(WindowCheckpoint::Run)) {
            state.paid_ability_window = None;
        }
        apply_approach_redirect(state, registry, &mut events)?;
        let approached: Vec<GameEvent> =
            events.iter().filter(|e| matches!(e, GameEvent::ServerApproached { .. })).cloned().collect();
        for event in approached {
            events.extend(dispatcher::dispatch_event(state, registry, &event)?);
        }
    }
    Ok(events)
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

    // A `Continue` steps off whatever the run is standing on, so the list
    // must be current first: Brân 1.0's install lands in the same handler
    // that then passes Brân, and `pass_current_ice` computes the next phase
    // from the list's length. If reconciling itself moved the run (the
    // encountered ICE is gone), that *was* this step — taking `continue_run`
    // as well would enter the next encounter with no rez window.
    if matches!(action, RunAction::Continue) {
        let moved = reconcile_ice(state, registry)?;
        if !moved.is_empty() {
            return Ok(moved);
        }
    }

    let mut events = match action {
        RunAction::JackOut => jack_out(state)?,
        RunAction::Continue => continue_run(state)?,
        RunAction::ResolveSubroutine(index) => step_subroutine(state, index, true, registry)?,
        RunAction::BreakSubroutine(index) => step_subroutine(state, index, false, registry)?,
    };

    // `IceEncountered`/`ServerApproached` may have just been emitted above (only
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
    apply_approach_redirect(state, registry, &mut events)?;
    let reactive: Vec<GameEvent> = events
        .iter()
        .filter(|event| matches!(event, GameEvent::IceEncountered { .. } | GameEvent::ServerApproached { .. }))
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

/// `Effect::RedirectRunOnApproach` (Maintenance Access): if this step
/// reached the server approach and the run carries a redirect, the run
/// now attacks the redirect's server instead — its ice list becomes that
/// server's, all counted as passed (the printed text says *approach* HQ,
/// not run it), and the `ServerApproached` event in `events` names the
/// new server so `Trigger::OnApproachServer` fires for its root. Applied
/// at both places a step's events are dispatched, before dispatch, so no
/// reaction sees the old server. A no-op for every ordinary run.
fn apply_approach_redirect(
    state: &mut GameState,
    registry: &CardRegistry,
    events: &mut Vec<GameEvent>,
) -> Result<(), RulesError> {
    let Some(run) = state.active_run.as_ref() else { return Ok(()) };
    let Some(target) = run.redirect_on_approach else { return Ok(()) };
    if !events.iter().any(|e| matches!(e, GameEvent::ServerApproached { .. })) {
        return Ok(());
    }
    let from = run.server;
    let ice: Vec<RunIce> = state
        .corp
        .installed
        .iter()
        .filter(|installed| installed.server == target && installed.slot == InstallSlot::Ice)
        .map(|installed| build_run_ice(installed, registry))
        .collect::<Result<Vec<Option<RunIce>>, _>>()?
        .into_iter()
        .flatten()
        .collect();
    let run = state.active_run.as_mut().expect("checked above");
    run.redirect_on_approach = None;
    run.server = target;
    run.position = ice.len();
    run.ice = ice;
    run.phase = RunPhase::Success;
    for event in events.iter_mut() {
        if let GameEvent::ServerApproached { server } = event {
            *server = target;
        }
    }
    // The redirect is recorded just before the approach it changed, so the
    // log reads "redirected, then approached HQ".
    let approach = events.iter().position(|e| matches!(e, GameEvent::ServerApproached { .. })).unwrap_or(events.len());
    events.insert(approach, GameEvent::RunRedirected { from, to: target });
    state.runner.servers_run_this_turn.push(target);
    Ok(())
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
                Ok(vec![GameEvent::ServerApproached { server }])
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

/// `Effect::BypassEncounteredIce` (Fransofia Ward): every pending
/// subroutine of the encountered ice is marked `Broken` — no event per
/// subroutine, since none was broken; one `IceBypassed` instead — and
/// `RunState::ice_bypassed` is raised. The ice is *not* passed here: the
/// bypass resolves inside a parked paid choice while the encounter's own
/// window is open, and `resolve_encounter_ice` then finds nothing to fire
/// and takes the `Continue` that passes it, exactly as a fully broken
/// encounter ends. Passing it immediately would move the run out from
/// under that window. `Broken` rather than a fourth `SubroutineStatus`:
/// every consumer already treats `Broken` as "will not fire", which is
/// the whole meaning of a bypass.
pub(crate) fn bypass_encountered_ice(state: &mut GameState) -> Result<Vec<GameEvent>, RulesError> {
    let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
    if run.phase != RunPhase::EncounterIce {
        return Err(RulesError::NotInEncounter);
    }
    let position = run.position;
    let ice = run.ice.get_mut(position).ok_or(RulesError::NotInEncounter)?;
    for subroutine in ice.subroutines.iter_mut().filter(|s| s.status == SubroutineStatus::Pending) {
        subroutine.status = SubroutineStatus::Broken;
    }
    let card_id = ice.card_id.clone();
    run.ice_bypassed = true;
    Ok(vec![GameEvent::IceBypassed { card_id, position: position as u32 }])
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
        if let Some(run) = state.active_run.as_mut() {
            run.subroutine_resolved = true;
        }
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
    state.runner.reset_run_strength_buffs();
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
            install_id: crate::rules::InstallId::PLACEHOLDER,
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

    /// One piece of ICE on HQ as both its `InstalledCard` and its `RunIce`,
    /// sharing an `InstallId` so `reconcile_ice` can match them.
    fn ice_pair(card_id: &str, install: u32, rezzed: bool) -> (InstalledCard, RunIce) {
        let installed = InstalledCard {
            install_id: crate::rules::InstallId(install),
            card: CardId(card_id.to_string()),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed,
            ..Default::default()
        };
        let run_ice = RunIce { install_id: crate::rules::InstallId(install), ..test_ice(card_id, 0, 2, rezzed) };
        (installed, run_ice)
    }

    fn reconciling_state(installed: Vec<InstalledCard>, run: RunState) -> GameState {
        let mut state = game_state();
        state.corp.installed = installed;
        state.active_run = Some(run);
        state
    }

    fn hq(state: &GameState) -> &RunState {
        state.active_run.as_ref().expect("run")
    }

    #[test]
    fn reconcile_is_a_no_op_when_the_board_matches_the_run() {
        let (ia, ra) = ice_pair("a", 1, true);
        let (ib, rb) = ice_pair("b", 2, true);
        let mut state = reconciling_state(vec![ia, ib], run_state(RunPhase::EncounterIce, vec![ra, rb], 1));
        let before = state.clone();
        let events = reconcile_ice(&mut state, &CardRegistry::new()).unwrap();
        assert!(events.is_empty());
        assert_eq!(state, before);
    }

    #[test]
    fn reconcile_drops_ice_trashed_outward_without_moving_the_current_ice() {
        let (_ia, ra) = ice_pair("a", 1, true);
        let (ib, rb) = ice_pair("b", 2, true);
        let (ic, rc) = ice_pair("c", 3, true);
        // `a` (outermost, already passed) has left play.
        let mut state = reconciling_state(vec![ib, ic], run_state(RunPhase::EncounterIce, vec![ra, rb, rc], 2));
        let events = reconcile_ice(&mut state, &CardRegistry::new()).unwrap();
        assert!(events.is_empty(), "the Runner is still on c: {events:?}");
        let run = hq(&state);
        assert_eq!(run.ice.iter().map(|i| i.card_id.0.as_str()).collect::<Vec<_>>(), vec!["b", "c"]);
        assert_eq!(run.position, 1);
        assert_eq!(run.phase, RunPhase::EncounterIce);
    }

    #[test]
    fn reconcile_ends_the_encounter_when_the_encountered_ice_leaves_play_and_approaches_the_next() {
        let (_ia, ra) = ice_pair("a", 1, true);
        let (ib, rb) = ice_pair("b", 2, true);
        let mut state =
            reconciling_state(vec![ib], run_state_with_jack_out(RunPhase::EncounterIce, vec![ra, rb], 0, false));
        state.runner.rig.push(crate::rules::InstalledRunnerCard { encounter_strength_buff: 2, ..Default::default() });

        let events = reconcile_ice(&mut state, &CardRegistry::new()).unwrap();
        assert_eq!(events, vec![GameEvent::IceApproached { server: ServerId::Hq, position: 0 }], "not `IcePassed`");
        let run = hq(&state);
        assert_eq!(run.ice.len(), 1);
        assert_eq!(run.ice[0].card_id.0, "b");
        assert_eq!(run.position, 0);
        assert_eq!(run.phase, RunPhase::ApproachIce);
        assert!(run.jack_out_permitted, "an ended encounter opens the jack-out window like a passed ICE");
        assert_eq!(state.runner.rig[0].encounter_strength_buff, 0, "encounter-duration buffs end with the encounter");
    }

    #[test]
    fn reconcile_approaches_the_server_when_the_encountered_ice_was_the_last() {
        let (_ia, ra) = ice_pair("a", 1, true);
        let mut state = reconciling_state(vec![], run_state_with_jack_out(RunPhase::EncounterIce, vec![ra], 0, false));
        let events = reconcile_ice(&mut state, &CardRegistry::new()).unwrap();
        assert_eq!(events, vec![GameEvent::ServerApproached { server: ServerId::Hq }]);
        let run = hq(&state);
        assert!(run.ice.is_empty());
        assert_eq!(run.position, 0);
        assert_eq!(run.phase, RunPhase::Success);
        assert!(run.jack_out_permitted);
    }

    #[test]
    fn reconcile_inserts_ice_installed_inward_to_be_approached_later() {
        let (ia, ra) = ice_pair("a", 1, true);
        let (ib, rb) = ice_pair("b", 2, true);
        let (inew, _) = ice_pair("new_ice", 9, false);
        let mut registry = CardRegistry::new();
        registry.insert(crate::dsl::CardDefinition {
            id: CardId("new_ice".to_string()),
            card_type: CardType::Ice(IceType::Barrier),
            ..Default::default()
        });
        // Brân's shape: installed directly inward of the encountered `a`.
        let mut state = reconciling_state(vec![ia, inew, ib], run_state(RunPhase::EncounterIce, vec![ra, rb], 0));
        let events = reconcile_ice(&mut state, &registry).unwrap();
        assert!(events.is_empty(), "the Runner is still encountering a");
        let run = hq(&state);
        assert_eq!(run.ice.iter().map(|i| i.card_id.0.as_str()).collect::<Vec<_>>(), vec!["a", "new_ice", "b"]);
        assert_eq!(run.position, 0);
        assert_eq!(run.phase, RunPhase::EncounterIce);
        assert_eq!(run.ice[1].install_id, crate::rules::InstallId(9));
        assert!(!run.ice[1].rezzed);
    }

    #[test]
    fn reconcile_inserts_ice_installed_outward_without_re_approaching_it() {
        let (ia, ra) = ice_pair("a", 1, true);
        let (ib, rb) = ice_pair("b", 2, true);
        let (inew, _) = ice_pair("new_ice", 9, true);
        let mut registry = CardRegistry::new();
        registry.insert(crate::dsl::CardDefinition {
            id: CardId("new_ice".to_string()),
            card_type: CardType::Ice(IceType::Barrier),
            ..Default::default()
        });
        let mut state = reconciling_state(vec![inew, ia, ib], run_state(RunPhase::ApproachIce, vec![ra, rb], 1));
        let events = reconcile_ice(&mut state, &registry).unwrap();
        assert!(events.is_empty());
        let run = hq(&state);
        assert_eq!(run.ice.len(), 3);
        assert_eq!(run.position, 2, "still approaching b");
        assert_eq!(run.ice[2].card_id.0, "b");
    }

    #[test]
    fn reconcile_treats_a_derez_of_the_encountered_ice_as_passing_it() {
        let (mut ia, ra) = ice_pair("a", 1, true);
        ia.rezzed = false;
        let (ib, rb) = ice_pair("b", 2, true);
        let mut state = reconciling_state(vec![ia, ib], run_state(RunPhase::EncounterIce, vec![ra, rb], 0));
        let events = reconcile_ice(&mut state, &CardRegistry::new()).unwrap();
        assert_eq!(
            events,
            vec![
                GameEvent::IcePassed { server: ServerId::Hq, position: 0 },
                GameEvent::IceApproached { server: ServerId::Hq, position: 1 },
            ]
        );
        let run = hq(&state);
        assert_eq!(run.position, 1);
        assert_eq!(run.phase, RunPhase::ApproachIce);
        assert!(!run.ice[0].rezzed, "the flag follows the install");
    }

    #[test]
    fn reconcile_only_syncs_the_flag_for_a_derez_during_approach() {
        let (mut ia, ra) = ice_pair("a", 1, true);
        ia.rezzed = false;
        let mut state = reconciling_state(vec![ia], run_state(RunPhase::ApproachIce, vec![ra], 0));
        let events = reconcile_ice(&mut state, &CardRegistry::new()).unwrap();
        assert!(events.is_empty(), "`continue_run` will pass it as unrezzed");
        assert!(!hq(&state).ice[0].rezzed);
        assert_eq!(hq(&state).phase, RunPhase::ApproachIce);
    }

    #[test]
    fn advance_run_continue_takes_no_second_step_after_the_encountered_ice_vanished() {
        let (_ia, ra) = ice_pair("a", 1, true);
        let (ib, rb) = ice_pair("b", 2, true);
        let mut state =
            reconciling_state(vec![ib], run_state_with_jack_out(RunPhase::EncounterIce, vec![ra, rb], 0, false));
        let events = advance_run(&mut state, RunAction::Continue, &CardRegistry::new()).unwrap();
        assert_eq!(
            events,
            vec![GameEvent::IceApproached { server: ServerId::Hq, position: 0 }],
            "approaching b *is* the step; b must not be encountered without its rez window"
        );
        assert_eq!(hq(&state).phase, RunPhase::ApproachIce);
    }

    #[test]
    fn reconcile_clears_a_stale_run_window_when_the_run_moves() {
        let (_ia, ra) = ice_pair("a", 1, true);
        let mut state = reconciling_state(vec![], run_state_with_jack_out(RunPhase::EncounterIce, vec![ra], 0, false));
        state.paid_ability_window = Some(crate::rules::PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            checkpoint: WindowCheckpoint::Run,
            return_phase: Box::new(state.phase),
        });
        reconcile_ice(&mut state, &CardRegistry::new()).unwrap();
        assert!(state.paid_ability_window.is_none(), "the encounter window's step no longer exists");
        assert_eq!(hq(&state).phase, RunPhase::Success);
    }

    #[test]
    fn reconcile_leaves_an_access_in_progress_alone() {
        let (_ia, ra) = ice_pair("a", 1, true);
        let mut run = run_state(RunPhase::AccessingCard, vec![ra], 1);
        run.access_state = Some(Default::default());
        let mut state = reconciling_state(vec![], run);
        let before = state.clone();
        assert!(reconcile_ice(&mut state, &CardRegistry::new()).unwrap().is_empty());
        assert_eq!(state, before);
    }

    #[test]
    fn initiation_continue_with_ice_enters_approach_ice() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::Initiation, vec![test_ice("ice_wall", 0, 2, true)], 0));
        crate::rules::test_support::install_the_runs_ice(&mut state);
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
        assert_eq!(events, vec![GameEvent::ServerApproached { server: ServerId::Hq }]);
    }

    #[test]
    fn approach_ice_continue_enters_encounter_ice() {
        let mut state = game_state();
        state.active_run = Some(run_state(RunPhase::ApproachIce, vec![test_ice("ice_wall", 3, 2, true)], 0));
        crate::rules::test_support::install_the_runs_ice(&mut state);
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
        crate::rules::test_support::install_the_runs_ice(&mut state);
        let events = advance_run(&mut state, RunAction::Continue, &CardRegistry::new()).expect("should succeed");

        let run = state.active_run.unwrap();
        assert_eq!(run.phase, RunPhase::Success);
        assert_eq!(
            events,
            vec![
                GameEvent::IcePassed { server: ServerId::Hq, position: 0 },
                GameEvent::ServerApproached { server: ServerId::Hq },
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
        crate::rules::test_support::install_the_runs_ice(&mut state);
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
        crate::rules::test_support::install_the_runs_ice(&mut state);
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
        crate::rules::test_support::install_the_runs_ice(&mut state);
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
        crate::rules::test_support::install_the_runs_ice(&mut state);
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
        crate::rules::test_support::install_the_runs_ice(&mut state);
        let events = advance_run(&mut state, RunAction::Continue, &CardRegistry::new()).expect("should succeed");

        assert_eq!(state.active_run.unwrap().phase, RunPhase::Success);
        assert_eq!(
            events,
            vec![
                GameEvent::IcePassed { server: ServerId::Hq, position: 0 },
                GameEvent::ServerApproached { server: ServerId::Hq },
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
        crate::rules::test_support::install_the_runs_ice(&mut state);
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
