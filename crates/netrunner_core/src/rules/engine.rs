use crate::cards::CardRegistry;
use crate::dsl::{CardId, CardSubtype, CardType, Cost, CounterKind, Trigger};
use crate::rules::ability;
use crate::rules::action::{PlayerAction, ServerTarget, TargetZone};
use crate::rules::dispatcher;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::paid_ability;
use crate::rules::pending_choice;
use crate::rules::run::{self, RunAction, RunPhase};
use crate::rules::setup;
use crate::rules::state::{ArchivedCard, GamePhase, GameState, InstallSlot, InstalledCard, InstalledRunnerCard, MemoryUnits, Side, WindowCheckpoint};
use crate::rules::trace;
use crate::rules::turn;
use crate::rules::win;

impl GameState {
    /// Ergonomic `state.step(registry, action)` alias for `apply_action`,
    /// for callers (MCTS/RL environments in particular) that read better as
    /// a method on the state being stepped. `apply_action` remains the one
    /// real implementation — every rejection rule lives there, not here —
    /// and `rules::legal_actions::legal_actions` is defined in terms of it
    /// (`candidate.is_ok()` under `apply_action`), so `step` rejecting
    /// exactly what's outside `legal_actions()` holds by construction, not
    /// as separately-enforced logic.
    pub fn step(
        &self,
        registry: &CardRegistry,
        action: PlayerAction,
    ) -> Result<(GameState, Vec<GameEvent>), RulesError> {
        apply_action(self, registry, action)
    }
}

pub fn apply_action(
    state: &GameState,
    registry: &CardRegistry,
    action: PlayerAction,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    // A trace admits no "stays legal during this" exceptions (unlike a
    // PaidAbilityWindow, which lets RezIce/BreakSubroutine/ActivateAbility
    // through) — nothing else should happen until both bids are in. A
    // single centralized guard here is simpler and harder to miss than
    // threading a per-handler check through ~15 existing functions.
    if let Some(trace) = &state.active_trace
        && !matches!(action, PlayerAction::SubmitCorpTraceBid { .. } | PlayerAction::SubmitRunnerTraceBid { .. })
    {
        let awaiting = if trace.corp_bid.is_none() { Side::Corp } else { Side::Runner };
        return Err(RulesError::ActionBlockedByActiveTrace { awaiting });
    }
    // A parked `PendingPaidChoice`/`PendingDecision` admits no "stays legal
    // during this" exceptions either, mirroring `active_trace`'s guard
    // above exactly — see `Effect::OfferPaidChoice`/`PresentChoice`'s doc
    // comments.
    if let Some(side) = state.pending_paid_choice.as_ref().map(|p| p.side)
        && !matches!(action, PlayerAction::AcceptPendingPaidChoice { .. } | PlayerAction::DeclinePendingPaidChoice)
    {
        return Err(RulesError::ActionBlockedByPendingPaidChoice { side });
    }
    if let Some(side) = pending_choice::pending_decision_chooser(state)
        && !matches!(
            action,
            PlayerAction::ResolvePendingChoice { .. }
                | PlayerAction::ToggleCardSelection { .. }
                | PlayerAction::ConfirmCardSelection
                | PlayerAction::ChooseServerForPendingDecision { .. }
                | PlayerAction::ChooseTriggerToResolve { .. }
        )
    {
        return Err(RulesError::ActionBlockedByPendingDecision { side });
    }
    // Classified before the match consumes `action`.
    let action_kind = classify_action(&action);
    let resolved = match action {
        PlayerAction::GainCreditClick { side } => gain_credit_click(state, side),
        PlayerAction::DrawCardClick => draw_card_click(state, registry),
        PlayerAction::InstallCard { card_id, zone, slot } => {
            install_card(state, registry, card_id, zone, slot)
        }
        PlayerAction::RezIce { ice_id } => rez_ice(state, registry, ice_id),
        PlayerAction::InitiateRun { server } => initiate_run(state, registry, server),
        PlayerAction::ContinueRun => continue_run(state, registry),
        PlayerAction::JackOut => jack_out(state, registry),
        PlayerAction::CompleteRun => complete_run(state, registry),
        PlayerAction::PlayEvent { card_id } => play_event(state, registry, card_id),
        PlayerAction::PlayOperation { card_id } => play_operation(state, registry, card_id),
        PlayerAction::InstallHardware { card_id } => install_hardware(state, registry, card_id),
        PlayerAction::InstallProgram { card_id, memory_cost } => {
            install_program(state, registry, card_id, memory_cost)
        }
        PlayerAction::InstallResource { card_id } => install_resource(state, registry, card_id),
        PlayerAction::InstallProgramOnIce { card_id, host_ice_id, memory_cost } => {
            install_program_on_ice(state, registry, card_id, host_ice_id, memory_cost)
        }
        PlayerAction::BreakSubroutine { ice_id, subroutine_index } => {
            break_subroutine(state, ice_id, subroutine_index, registry)
        }
        PlayerAction::BreakSubroutineWithClick { ice_id, subroutine_index } => {
            break_subroutine_with_click(state, ice_id, subroutine_index, registry)
        }
        PlayerAction::EndTurn => turn::end_turn(state, registry),
        PlayerAction::DiscardCard { card_id } => turn::discard_card(state, card_id, registry),
        PlayerAction::KeepHand => setup::keep_hand(state, registry),
        PlayerAction::TakeMulligan => setup::take_mulligan(state, registry),
        PlayerAction::ActivateAbility { card_id, ability_index } => {
            activate_ability(state, registry, card_id, ability_index)
        }
        PlayerAction::AdvanceCard { card_id } => advance_card(state, registry, card_id),
        PlayerAction::ScoreAgenda { card_id } => score_agenda(state, registry, card_id),
        PlayerAction::RemoveTag => remove_tag(state),
        PlayerAction::PurgeVirusCounters => purge_virus_counters(state, registry),
        PlayerAction::TrashResource { card_id } => trash_resource(state, registry, card_id),
        PlayerAction::SelectCardToAccess { card_id } => {
            select_card_to_access(state, registry, card_id)
        }
        PlayerAction::StealAgenda { card_id } => steal_agenda(state, registry, card_id),
        PlayerAction::TrashAccessedCard { card_id } => {
            trash_accessed_card(state, registry, card_id)
        }
        PlayerAction::PassAccessedCard { card_id } => {
            pass_accessed_card(state, registry, card_id)
        }
        PlayerAction::PayAccessTrigger { card_id } => {
            pay_access_trigger(state, registry, card_id)
        }
        PlayerAction::DeclineAccessTrigger { card_id } => {
            decline_access_trigger(state, registry, card_id)
        }
        PlayerAction::PassPriority { side } => pass_priority_action(state, registry, side),
        PlayerAction::SubmitCorpTraceBid { amount } => submit_corp_trace_bid(state, amount),
        PlayerAction::SubmitRunnerTraceBid { amount } => submit_runner_trace_bid(state, registry, amount),
        PlayerAction::AcceptPendingPaidChoice { cost_option_index } => {
            accept_pending_paid_choice(state, registry, cost_option_index)
        }
        PlayerAction::DeclinePendingPaidChoice => decline_pending_paid_choice(state, registry),
        PlayerAction::ResolvePendingChoice { option_index } => resolve_pending_choice(state, registry, option_index),
        PlayerAction::ToggleCardSelection { card_id } => toggle_card_selection(state, registry, card_id),
        PlayerAction::ConfirmCardSelection => confirm_card_selection(state, registry),
        PlayerAction::ChooseServerForPendingDecision { server } => {
            choose_server_for_pending_decision(state, registry, server)
        }
        PlayerAction::ChooseTriggerToResolve { card_id } => {
            choose_trigger_to_resolve(state, registry, card_id)
        }
    }?;

    // Fire anything a dispatch had to queue because a trigger parked a
    // decision partway through (see `dispatcher::fire_plan`). Placed here,
    // after every handler, for the same reason the `active_trace` guard
    // above is centralized: one call is simpler and harder to miss than
    // threading it through ~15 handlers — and this placement additionally
    // covers trace and prevention-window resolution, which a
    // `pending_choice`-only drain would miss.
    //
    // A no-op in the overwhelmingly common case: the queue is empty unless
    // a trigger actually parked something.
    let (mut next, mut events) = resolved;
    events.extend(dispatcher::drain_deferred_triggers(&mut next, registry)?);
    events.extend(open_post_action_window(&mut next, registry, &action_kind));
    Ok((next, events))
}

/// Opens a `WindowCheckpoint::PostAction` if the action just resolved was a
/// basic click action and the opponent actually has a paid ability to use.
///
/// Real Netrunner gives both players a paid-ability window after each
/// action. Only the **opponent's** half is missing here: the acting player
/// can already fire their own paid abilities throughout `Action(side)`
/// (see `activate_ability`), so a window that nobody but the acting player
/// could use would be pure overhead.
///
/// Every guard below is load-bearing:
/// - **basic click action** — run sub-actions, priority passes and decision
///   resolutions are not actions and open nothing. This is also what stops
///   a cascade: closing a window is a `PassPriority`, which is not an
///   action, so it cannot open another.
/// - **no run, no window, nothing parked, not over** — those flows own
///   their own checkpoints; layering one on top would strand them.
/// - **the opponent has something usable** — the cost guard; see
///   `paid_ability::has_usable_paid_ability`.
fn open_post_action_window(
    state: &mut GameState,
    registry: &CardRegistry,
    kind: &ActionKind,
) -> Vec<GameEvent> {
    if !matches!(kind, ActionKind::BasicClickAction) {
        return Vec::new();
    }
    let GamePhase::Action(side) = state.phase else { return Vec::new() };
    if state.active_run.is_some() || state.paid_ability_window.is_some() || state.is_resolution_blocked() {
        return Vec::new();
    }
    if !paid_ability::has_usable_paid_ability(state, registry, side.other()) {
        return Vec::new();
    }
    // Active player first, matching `open_window`'s convention.
    vec![paid_ability::open_window_for(state, side, WindowCheckpoint::PostAction { side })]
}

/// Whether a `PlayerAction` is a basic click action — the thing a
/// post-action paid-ability window follows.
///
/// Deliberately an exhaustive `match` rather than a check for a
/// `ClickSpent` event: exhaustive so that adding a `PlayerAction` fails to
/// compile here and forces a decision, and explicit so that a paid ability
/// which happens to cost a click (Regolith Mining License) doesn't get
/// mistaken for an action.
enum ActionKind {
    BasicClickAction,
    Other,
}

fn classify_action(action: &PlayerAction) -> ActionKind {
    match action {
        PlayerAction::GainCreditClick { .. }
        | PlayerAction::DrawCardClick
        | PlayerAction::InstallCard { .. }
        | PlayerAction::InstallHardware { .. }
        | PlayerAction::InstallProgram { .. }
        | PlayerAction::InstallResource { .. }
        | PlayerAction::InstallProgramOnIce { .. }
        | PlayerAction::PlayEvent { .. }
        | PlayerAction::PlayOperation { .. }
        | PlayerAction::AdvanceCard { .. }
        | PlayerAction::ScoreAgenda { .. }
        | PlayerAction::RemoveTag
        | PlayerAction::TrashResource { .. }
        | PlayerAction::PurgeVirusCounters => ActionKind::BasicClickAction,

        // `InitiateRun` is a click action, but the run it starts owns the
        // checkpoints from here on — the `active_run` guard would reject it
        // anyway; listed explicitly so the intent is recorded.
        PlayerAction::InitiateRun { .. }
        // Run sub-actions, not actions.
        | PlayerAction::ContinueRun
        | PlayerAction::JackOut
        | PlayerAction::CompleteRun
        | PlayerAction::BreakSubroutine { .. }
        | PlayerAction::BreakSubroutineWithClick { .. }
        | PlayerAction::SelectCardToAccess { .. }
        | PlayerAction::StealAgenda { .. }
        | PlayerAction::TrashAccessedCard { .. }
        | PlayerAction::PassAccessedCard { .. }
        | PlayerAction::PayAccessTrigger { .. }
        | PlayerAction::DeclineAccessTrigger { .. }
        // Rez is not an action; paid abilities are used *in* windows, not
        // followed by new ones.
        | PlayerAction::RezIce { .. }
        | PlayerAction::ActivateAbility { .. }
        // Turn structure and priority — each owns its own checkpoint.
        | PlayerAction::EndTurn
        | PlayerAction::DiscardCard { .. }
        | PlayerAction::KeepHand
        | PlayerAction::TakeMulligan
        | PlayerAction::PassPriority { .. }
        // Resolutions of something already parked.
        | PlayerAction::SubmitCorpTraceBid { .. }
        | PlayerAction::SubmitRunnerTraceBid { .. }
        | PlayerAction::AcceptPendingPaidChoice { .. }
        | PlayerAction::DeclinePendingPaidChoice
        | PlayerAction::ResolvePendingChoice { .. }
        | PlayerAction::ToggleCardSelection { .. }
        | PlayerAction::ConfirmCardSelection
        | PlayerAction::ChooseServerForPendingDecision { .. }
        | PlayerAction::ChooseTriggerToResolve { .. } => ActionKind::Other,
    }
}

fn require_phase(state: &GameState, expected: GamePhase) -> Result<(), RulesError> {
    if state.phase != expected {
        return Err(RulesError::WrongPhase { expected, actual: state.phase });
    }
    Ok(())
}

fn spend_click(state: &mut GameState, side: Side) -> Result<(), RulesError> {
    let resources = state.resources_mut(side);
    let available = resources.clicks.0;
    resources.clicks = resources
        .clicks
        .spend(1)
        .ok_or(RulesError::NotEnoughClicks {
            side,
            available,
            requested: 1,
        })?;
    Ok(())
}

fn gain_credit_click(
    state: &GameState,
    side: Side,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;
    next.resources_mut(side).credits = next.resources(side).credits.gain(1);

    Ok((
        next,
        vec![
            GameEvent::ClickSpent { side },
            GameEvent::CreditsGained { side, amount: 1 },
        ],
    ))
}

fn draw_card_click(state: &GameState, registry: &CardRegistry) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;

    let mut events = vec![GameEvent::ClickSpent { side }];
    if let Some(card) = next.runner.stack.pop() {
        next.runner.grip.push(card);
        events.push(GameEvent::CardDrawn { side });
    }

    let basic_draw_event = GameEvent::BasicDrawActionTaken { side };
    events.push(basic_draw_event.clone());
    events.extend(dispatcher::dispatch_event(&mut next, registry, &basic_draw_event)?);

    Ok((next, events))
}

fn install_card(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
    zone: TargetZone,
    slot: InstallSlot,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Corp;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;

    let position = next
        .corp
        .hq
        .iter()
        .position(|c| *c == card_id)
        .ok_or_else(|| RulesError::CardNotInHand {
            side,
            card: card_id.clone(),
        })?;
    next.corp.hq.remove(position);

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(card_def.cost), Some(&card_id))?);

    next.corp.installed.push(InstalledCard {
        card: card_id.clone(),
        server: zone,
        slot,
        rezzed: false,
        advancement_tokens: 0,
        counters: 0,
        installed_this_turn: true,
    });
    let installed_event = GameEvent::CardInstalled {
        side,
        card: card_id,
        server: zone,
    };
    events.push(installed_event.clone());

    // Haas-Bioroid: Engineering the Future-style identity reaction — gated
    // by `EffectRequirement::FirstInstallThisTurn` on the identity's own
    // `TriggeredEffect`, so this dispatch is unconditional here.
    events.extend(dispatcher::dispatch_event(&mut next, registry, &installed_event)?);

    Ok((next, events))
}

fn rez_ice(
    state: &GameState,
    registry: &CardRegistry,
    ice_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Corp;
    // Priority-independent: legal during the Corp's own Action phase, or —
    // regardless of whose turn it is — while any PaidAbilityWindow is open,
    // or (kept for states that reached ApproachIce without a window
    // literally attached, e.g. hand-built test fixtures) while a run is at
    // RunPhase::ApproachIce specifically.
    let rez_window_open = state.paid_ability_window.is_some()
        || matches!(&state.active_run, Some(run) if run.phase == RunPhase::ApproachIce);
    if !rez_window_open {
        require_phase(state, GamePhase::Action(side))?;
    }
    let mut next = state.clone();

    let server = {
        let installed = next
            .corp
            .installed
            .iter_mut()
            .find(|c| c.card == ice_id)
            .ok_or_else(|| RulesError::CardNotInstalled {
                card: ice_id.clone(),
            })?;
        if installed.rezzed {
            return Err(RulesError::AlreadyRezzed { card: ice_id });
        }
        installed.rezzed = true;
        installed.server
    };

    let card_def = registry
        .get(&ice_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(ice_id.clone()))?;
    // Tread Lightly-style "+3 credits to rez cost during this run" modifier,
    // if this ICE's server is the one being run — applied to the printed
    // cost before paying, never allowed to go negative.
    let rez_cost_modifier = next
        .active_run
        .as_ref()
        .filter(|run| run.server == server)
        .map_or(0, |run| run.ice_rez_cost_modifier);
    let rez_cost = (card_def.cost as i32 + rez_cost_modifier).max(0) as u32;
    let mut events = ability::pay_cost(&mut next, side, &Cost::Credits(rez_cost), Some(&ice_id))?;

    // If this rez happens during this ICE's own `ApproachIce` window (the
    // normal "rez window"), also flip the matching `RunIce.rezzed` so
    // `continue_run`'s upcoming `ApproachIce` transition sees it as rezzed.
    // Scoped to the ICE currently at `position` — rezzing a *different*
    // installed ICE on the same server (legal, e.g. pre-emptively, before
    // the run reaches it) must not affect a `RunIce` the run hasn't reached
    // yet; `initiate_run` already seeded that one correctly from
    // `InstalledCard::rezzed` at run start.
    if let Some(run) = next.active_run.as_mut()
        && run.phase == RunPhase::ApproachIce
        && let Some(current_ice) = run.ice.get_mut(run.position)
        && current_ice.card_id == ice_id
    {
        current_ice.rezzed = true;
    }

    // Rez stays priority-independent (either side can act regardless of
    // whose priority it currently is), but still gives the other side a
    // fresh chance to respond if a window is open — Netrunner/Null Signal
    // Games priority rule 4.
    paid_ability::note_window_action(&mut next, side);

    let rezzed_event = GameEvent::IceRezzed { card: ice_id, server };
    events.push(rezzed_event.clone());
    events.extend(dispatcher::dispatch_event(&mut next, registry, &rezzed_event)?);
    Ok((next, events))
}

fn initiate_run(
    state: &GameState,
    registry: &CardRegistry,
    server: ServerTarget,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    if state.active_run.is_some() {
        return Err(RulesError::RunAlreadyInProgress);
    }
    // Without this, a run could be initiated mid-`StartOfTurn`/`EndOfTurn`
    // window (whose checkpoint keeps `state.phase` at `Action(Runner)`/
    // unrelated), leaving it orphaned once the window closes and hands
    // control to the other side — `active_run.is_some()` alone used to be
    // enough here since only run-checkpoint windows existed, all of which
    // already implied an active run; that invariant no longer holds.
    paid_ability::require_no_window(state)?;

    let mut next = state.clone();
    spend_click(&mut next, side)?;
    run::start_run(&mut next, registry, server)?;

    let run_initiated_event = GameEvent::RunInitiated { server };
    let mut events = vec![GameEvent::ClickSpent { side }, run_initiated_event.clone()];
    events.extend(dispatcher::dispatch_event(&mut next, registry, &run_initiated_event)?);

    Ok((next, events))
}

fn continue_run(state: &GameState, registry: &CardRegistry) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    // Gabriel Santiago-style identity reaction (a run on HQ just succeeded,
    // gated by `EffectRequirement::FirstSuccessfulHqRunThisTurn` on the
    // identity's own `TriggeredEffect` — the dispatch itself is
    // unconditional, the soft-gate inside `process_card_triggers` is what
    // limits it to once per turn) and any `OnEncounter`/`OnSuccessfulRun`
    // reactions are dispatched inside `run::advance_run` itself now, so
    // every caller (this handler, and `paid_ability::close_window`'s
    // window-mediated auto-continue) gets them uniformly.
    let mut events = run::advance_run(&mut next, RunAction::Continue, registry)?;
    events.extend(paid_ability::open_window_if_at_checkpoint(&mut next));

    Ok((next, events))
}

fn jack_out(state: &GameState, registry: &CardRegistry) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    let mut next = state.clone();
    let mut events = run::advance_run(&mut next, RunAction::JackOut, registry)?;
    // Snapshotted before `active_run` is cleared, mirroring `run::access`'s
    // `RunCompleted` site — see `Trigger::OnRunEnded`'s doc comment.
    if let Some(run) = next.active_run.as_ref() {
        next.last_completed_run = Some(crate::rules::state::CompletedRun::snapshot(run));
    }
    next.active_run = None;
    // A window can be open here (e.g. mid-ApproachIce on the second+ ICE,
    // where jack_out_permitted is already true from a prior pass) — clear it
    // too, or it would survive with no active_run left to ever close it
    // against, permanently blocking every ordinary action afterward.
    next.paid_ability_window = None;

    let jacked_out_event = events
        .iter()
        .find(|e| matches!(e, GameEvent::RunJackedOut { .. }))
        .cloned()
        .expect("run::advance_run(RunAction::JackOut) always emits RunJackedOut on success");
    events.extend(dispatcher::dispatch_event(&mut next, registry, &jacked_out_event)?);

    Ok((next, events))
}

fn complete_run(
    state: &GameState,
    _registry: &CardRegistry,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    let active_run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
    if active_run.phase != RunPhase::Success {
        return Err(RulesError::RunNotConcluded { phase: active_run.phase });
    }

    let mut next = state.clone();
    // Opens the pre-access Paid Ability Window rather than accessing
    // immediately — access (`run::access_server`, `RunCompleted`) now
    // happens once both sides pass, inside `paid_ability::close_window`'s
    // `Success` arm. `access_server` clears `active_run` itself when
    // nothing was accessed; otherwise it parks the run in
    // `RunPhase::AccessingCard` and `StealAgenda`/`TrashAccessedCard`/
    // `PassAccessedCard` are what eventually finish it off.
    let event = paid_ability::open_window(&mut next);

    Ok((next, vec![event]))
}

fn take_from_grip(state: &mut GameState, side: Side, card_id: &CardId) -> Result<(), RulesError> {
    let hand = match side {
        Side::Runner => &mut state.runner.grip,
        Side::Corp => &mut state.corp.hq,
    };
    let position = hand
        .iter()
        .position(|c| c == card_id)
        .ok_or_else(|| RulesError::CardNotInHand {
            side,
            card: card_id.clone(),
        })?;
    hand.remove(position);
    Ok(())
}

fn play_event(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;
    take_from_grip(&mut next, side, &card_id)?;

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    // Hard play-legality gate (e.g. Scorched Earth's "the Runner must be
    // tagged") — checked before its credit cost is paid, mirroring
    // `activate_ability`'s identical placement for `AbilityDef::requirement`.
    if let Some(requirement) = &card_def.play_requirement {
        ability::check_requirement(&next, requirement, side, &ability::ResolutionContext::for_card(Some(&card_id)), registry)?;
    }

    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(card_def.cost), Some(&card_id))?);
    let played_event = GameEvent::EventPlayed { side, card: card_id.clone() };
    events.push(played_event.clone());
    events.extend(dispatcher::dispatch_event(&mut next, registry, &played_event)?);

    Ok((next, events))
}

/// Corp-only mirror of `play_event`: spends 1 click and the card's
/// registry-defined credit cost, moves `card_id` out of HQ into Archives
/// (Operations are trashed as part of being played, same as real
/// Netrunner/Null Signal Games rules — unlike `play_event`, which currently
/// has no Heap-placement step for played Events), then resolves its
/// `OnPlay` triggers. `card_id`'s registry `CardType` must be `Operation`
/// (`RulesError::CardNotOperation` otherwise) — checked before paying the
/// credit cost, so an ineligible card never mutates `next`'s credits.
fn play_operation(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Corp;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;
    take_from_grip(&mut next, side, &card_id)?;

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    if card_def.card_type != CardType::Operation {
        return Err(RulesError::CardNotOperation { card: card_id });
    }
    if let Some(requirement) = &card_def.play_requirement {
        ability::check_requirement(&next, requirement, side, &ability::ResolutionContext::for_card(Some(&card_id)), registry)?;
    }

    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(card_def.cost), Some(&card_id))?);
    // A played Operation resolved in the open, so the Runner has seen it.
    next.corp.archives.push(ArchivedCard::faceup(card_id.clone()));
    // `dispatch_event` resolves both `OnPlay` and, for Transaction-subtype
    // Operations, the Weyland Consortium: Building a Better World-style
    // identity reaction (unconditional — no per-turn gate, unlike
    // `OnSuccessfulRunOnHq`/`OnInstall` above) from this one event.
    let played_event = GameEvent::OperationPlayed { side, card: card_id.clone() };
    events.push(played_event.clone());
    events.extend(dispatcher::dispatch_event(&mut next, registry, &played_event)?);

    Ok((next, events))
}

/// Seeds a newly-installed rig card's `base_strength` from the registry's
/// printed `strength` — mirrors `build_run_ice`'s identical seed-once
/// pattern for `RunIce::current_strength`. `0` for Hardware/non-strength
/// Programs (`CardDefinition::strength` is `None`).
fn seed_rig_card(registry: &CardRegistry, card_id: CardId) -> Result<InstalledRunnerCard, RulesError> {
    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    Ok(InstalledRunnerCard {
        base_strength: card_def.strength.unwrap_or(0),
        card: card_id,
        encounter_strength_buff: 0,
        turn_strength_buff: 0,
        counters: 0,
        hosted_on_ice: None,
    })
}

/// Which kind of Runner install `discounted_install_cost`/
/// `applicable_first_install_discount` is pricing — Kate "Mac" McCaffrey's
/// identity discount applies to Program *or* Hardware installs, but DZMZ
/// Optimizer's rig-card discount ("the first program you install") is
/// Program-only, and neither ever applies to Resources (no baseline or
/// System Gateway card discounts those). This distinction didn't exist
/// before M5 — `install_resource` was, prior to this, incorrectly eligible
/// for the identity discount too; this fixes that alongside generalizing
/// the mechanism to also look at rig cards.
#[derive(Clone, Copy, PartialEq, Eq)]
enum InstallKind {
    Hardware,
    Program,
    Resource,
}

/// The first-install-of-the-turn discount applicable to an install of
/// `kind`, from either source: the Runner's identity (`CardDefinition::
/// first_install_discount`, applies to Hardware or Program — e.g. Kate "Mac"
/// McCaffrey), or any installed rig card declaring the same field (applies
/// to Program installs only — e.g. DZMZ Optimizer's "the first program you
/// install each turn costs 1 credit less"). Identity takes priority if
/// somehow both are present (no real deck can field both Kate and DZMZ
/// simultaneously as separate discount *sources* meaningfully stacking, so
/// this ordering is arbitrary-but-harmless). `0` if neither applies or
/// `kind` is `Resource`.
fn applicable_first_install_discount(state: &GameState, registry: &CardRegistry, kind: InstallKind) -> u32 {
    if kind != InstallKind::Resource
        && let Some(identity) = state.runner.identity.as_ref()
        && let Some(discount) = registry.get(identity).and_then(|c| c.first_install_discount)
    {
        return discount;
    }
    if kind == InstallKind::Program {
        for rig_card in &state.runner.rig {
            if let Some(discount) = registry.get(&rig_card.card).and_then(|c| c.first_install_discount) {
                return discount;
            }
        }
    }
    0
}

/// The credit cost to charge for installing a Runner card of `kind` this
/// turn: `base_cost` reduced by `applicable_first_install_discount`, if any
/// and it hasn't already been applied this turn
/// (`RunnerState::first_install_discount_used_this_turn` — shared across
/// every discount source, since e.g. Kate and DZMZ are mutually exclusive
/// in a real deck). Consumes the flag on `next` if the discount applies.
/// Not a `Trigger`/`Effect` — see `CardDefinition::first_install_discount`'s
/// doc comment for why this is a direct cost modifier instead.
fn discounted_install_cost(next: &mut GameState, registry: &CardRegistry, base_cost: u32, kind: InstallKind) -> u32 {
    if next.runner.first_install_discount_used_this_turn {
        return base_cost;
    }
    let discount = applicable_first_install_discount(next, registry, kind);
    if discount == 0 {
        return base_cost;
    }
    next.runner.first_install_discount_used_this_turn = true;
    base_cost.saturating_sub(discount)
}

fn install_hardware(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;
    take_from_grip(&mut next, side, &card_id)?;

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    // "Limit 1 console per player" (e.g. Carnivore, Pennyshaver, Pantograph)
    // — checked after the ordinary phase/click/hand checks above, matching
    // every other card-specific rejection in this function (e.g.
    // `MismatchedMemoryCost` in `install_program`) running only once the
    // action is otherwise well-formed.
    if card_def.subtypes.contains(&CardSubtype::Console)
        && next.runner.rig.iter().any(|installed| {
            registry.get(&installed.card).is_some_and(|c| c.subtypes.contains(&CardSubtype::Console))
        })
    {
        return Err(RulesError::ConsoleLimitExceeded);
    }

    let cost = discounted_install_cost(&mut next, registry, card_def.cost, InstallKind::Hardware);

    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(cost), Some(&card_id))?);
    let rig_card = seed_rig_card(registry, card_id.clone())?;
    next.runner.rig.push(rig_card);
    // MU/max-hand-size bonuses (e.g. a console's "+1[mu]", T400 Memory
    // Diamond's "+1 maximum hand size") take effect immediately on install.
    // Deliberately one-way: neither is decremented if this Hardware later
    // leaves play — see `RunnerState::max_hand_size_bonus`'s doc comment.
    // Threading a `CardRegistry` through every trash path (`ability::
    // trash_card`/`trash_this_card`/`pay_cost`) to refund these correctly
    // would be a much larger refactor than this milestone's actual cards
    // need; revisit if a future card's balance depends on the refund.
    if let Some(bonus) = card_def.memory_bonus {
        next.runner.memory_units = MemoryUnits(next.runner.memory_units.0 + bonus);
    }
    if let Some(bonus) = card_def.max_hand_size_bonus {
        next.runner.max_hand_size_bonus = next.runner.max_hand_size_bonus.saturating_add(bonus);
    }
    events.push(GameEvent::HardwareInstalled { side, card: card_id });

    Ok((next, events))
}

fn install_program(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
    memory_cost: u8,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;
    take_from_grip(&mut next, side, &card_id)?;

    let available = next.runner.memory_units.0;
    let requested = memory_cost as u32;
    next.runner.memory_units = next
        .runner
        .memory_units
        .spend(requested)
        .ok_or(RulesError::InsufficientMemory { available, requested })?;

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    // A registered `memory_cost` is authoritative — the caller-supplied
    // value must match it exactly. `None` (the common case for a card with
    // no `memory_cost` set yet) leaves the caller free to name any value,
    // preserving this action's existing behavior for every card that
    // hasn't been migrated to declare one.
    if let Some(expected) = card_def.memory_cost
        && expected != requested
    {
        return Err(RulesError::MismatchedMemoryCost { expected, actual: requested });
    }
    let mut cost = discounted_install_cost(&mut next, registry, card_def.cost, InstallKind::Program);
    // A conditional per-card discount (e.g. Carmen: "-2 to install if you
    // made a successful run this turn") stacks independently on top of the
    // once-per-turn identity/rig-card discount above — no shared
    // consumption flag, since it's re-evaluated fresh every time.
    if let Some((requirement, amount)) = &card_def.install_cost_discount_if
        && ability::check_requirement(&next, requirement, side, &ability::ResolutionContext::for_card(Some(&card_id)), registry).is_ok()
    {
        cost = cost.saturating_sub(*amount);
    }

    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(cost), Some(&card_id))?);
    let rig_card = seed_rig_card(registry, card_id.clone())?;
    next.runner.rig.push(rig_card);
    // Noise: Hacker Extraordinaire-style identity reaction (Virus-subtype
    // Programs only, unconditional otherwise — no per-turn gate) resolved by
    // `dispatch_event` from this one event.
    let installed_event = GameEvent::ProgramInstalled { side, card: card_id, memory_cost };
    events.push(installed_event.clone());
    events.extend(dispatcher::dispatch_event(&mut next, registry, &installed_event)?);

    Ok((next, events))
}

/// Resolves `PlayerAction::InstallProgramOnIce`, per its doc comment.
/// Mirrors `install_program` almost exactly (same memory/cost handling,
/// same `ProgramInstalled` event so `OnVirusInstalled`/Cookbook-style
/// dispatch keeps working uniformly for a hosted Trojan) — the only two
/// differences are the `installs_on_ice`/host-is-ICE validation up front,
/// and stamping `hosted_on_ice` on the seeded rig card afterward.
fn install_program_on_ice(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
    host_ice_id: CardId,
    memory_cost: u8,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    if !card_def.installs_on_ice {
        return Err(RulesError::NotATrojanProgram(card_id));
    }
    if !state
        .corp
        .installed
        .iter()
        .any(|c| c.card == host_ice_id && c.slot == InstallSlot::Ice)
    {
        return Err(RulesError::HostIsNotIce(host_ice_id));
    }

    let mut next = state.clone();
    spend_click(&mut next, side)?;
    take_from_grip(&mut next, side, &card_id)?;

    let available = next.runner.memory_units.0;
    let requested = memory_cost as u32;
    next.runner.memory_units = next
        .runner
        .memory_units
        .spend(requested)
        .ok_or(RulesError::InsufficientMemory { available, requested })?;

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    if let Some(expected) = card_def.memory_cost
        && expected != requested
    {
        return Err(RulesError::MismatchedMemoryCost { expected, actual: requested });
    }
    let cost = discounted_install_cost(&mut next, registry, card_def.cost, InstallKind::Program);

    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(cost), Some(&card_id))?);
    let mut rig_card = seed_rig_card(registry, card_id.clone())?;
    rig_card.hosted_on_ice = Some(host_ice_id);
    next.runner.rig.push(rig_card);
    let installed_event = GameEvent::ProgramInstalled { side, card: card_id, memory_cost };
    events.push(installed_event.clone());
    events.extend(dispatcher::dispatch_event(&mut next, registry, &installed_event)?);

    Ok((next, events))
}

/// Resolves `PlayerAction::InstallResource`, per its doc comment. Mirrors
/// `install_hardware` (no memory-unit reservation), but — like
/// `install_program` — dispatches its own installed-event afterward, since
/// a Resource is the only Runner install kind so far whose `OnInstall`
/// trigger needs to fire against the just-installed card itself (e.g. Red
/// Team/Telework Contract's "when you install this resource, load N
/// credits onto it").
fn install_resource(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;
    take_from_grip(&mut next, side, &card_id)?;

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    let cost = discounted_install_cost(&mut next, registry, card_def.cost, InstallKind::Resource);

    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(cost), Some(&card_id))?);
    let rig_card = seed_rig_card(registry, card_id.clone())?;
    next.runner.rig.push(rig_card);
    let installed_event = GameEvent::ResourceInstalled { side, card: card_id };
    events.push(installed_event.clone());
    events.extend(dispatcher::dispatch_event(&mut next, registry, &installed_event)?);

    Ok((next, events))
}

fn break_subroutine(
    state: &GameState,
    ice_id: CardId,
    subroutine_index: usize,
    registry: &CardRegistry,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;

    // Cross-check `ice_id` against the ICE actually being encountered before
    // delegating — `transition_subroutine` identifies the right `RunIce`
    // positionally (`run.position`), not by `ice_id`, so it can't catch a
    // caller-supplied mismatch on its own. See
    // `PlayerAction::BreakSubroutine`'s doc comment.
    let run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
    if run.phase != RunPhase::EncounterIce {
        return Err(RulesError::NotInEncounter);
    }
    let current_ice = run.ice.get(run.position).ok_or(RulesError::NotInEncounter)?;
    if current_ice.card_id != ice_id {
        return Err(RulesError::MismatchedIceId {
            expected: current_ice.card_id.clone(),
            actual: ice_id,
        });
    }

    // `step_subroutine` (via `advance_run`) does its own bounds/status
    // validation against `RunIce::subroutines`, so there's no need to
    // duplicate that here — just forward the index.
    let mut next = state.clone();
    let events = run::advance_run(&mut next, RunAction::BreakSubroutine(subroutine_index), registry)?;
    // Priority-independent like RezIce (not gated on whose priority it is),
    // but still gives the other side a fresh chance to respond if a window
    // is open.
    paid_ability::note_window_action(&mut next, side);

    Ok((next, events))
}

/// `PlayerAction::BreakSubroutineWithClick`'s handler — identical
/// legality/delegation shape to `break_subroutine`, plus the
/// `click_breakable` gate and a click cost instead of no cost at all.
fn break_subroutine_with_click(
    state: &GameState,
    ice_id: CardId,
    subroutine_index: usize,
    registry: &CardRegistry,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;

    let run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
    if run.phase != RunPhase::EncounterIce {
        return Err(RulesError::NotInEncounter);
    }
    let current_ice = run.ice.get(run.position).ok_or(RulesError::NotInEncounter)?;
    if current_ice.card_id != ice_id {
        return Err(RulesError::MismatchedIceId {
            expected: current_ice.card_id.clone(),
            actual: ice_id,
        });
    }
    if !registry.get(&ice_id).is_some_and(|c| c.click_breakable) {
        return Err(RulesError::IceNotClickBreakable(ice_id));
    }

    let mut next = state.clone();
    spend_click(&mut next, side)?;
    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(run::advance_run(&mut next, RunAction::BreakSubroutine(subroutine_index), registry)?);
    paid_ability::note_window_action(&mut next, side);

    Ok((next, events))
}

/// Pays and resolves the `ability_index`-th `AbilityDef` on `card_id`, per
/// `PlayerAction::ActivateAbility`'s doc comment. Symmetric like
/// `turn::end_turn`/`turn::discard_card` — the acting side is derived rather
/// than taken as a parameter. Outside a Paid Ability Window, this is
/// derived from `state.phase` exactly as before (unchanged behavior).
/// Inside one, either side may respond — *which* side is still resolved by
/// zone (Corp `installed && rezzed` vs Runner `rig`, disjoint by
/// construction, so unambiguous), but *whether* they're allowed to act now
/// is gated by `window.active_priority` instead.
fn activate_ability(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
    ability_index: usize,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let corp_active = state.corp.installed.iter().any(|c| c.card == card_id && c.rezzed);
    let runner_active = state.runner.rig.iter().any(|c| c.card == card_id);

    let side = match &state.paid_ability_window {
        Some(window) => {
            if corp_active {
                Side::Corp
            } else if runner_active {
                Side::Runner
            } else {
                // Neither zone has it — fall through to the CardNotActive
                // check below either way; `window.active_priority` here is
                // only used to shape that error's payload.
                window.active_priority
            }
        }
        None => match state.phase {
            GamePhase::Action(side) => side,
            actual => return Err(RulesError::NotInActionPhase { actual }),
        },
    };

    let active = match side {
        Side::Corp => corp_active,
        Side::Runner => runner_active,
    };
    if !active {
        return Err(RulesError::CardNotActive { side, card: card_id });
    }

    if let Some(window) = &state.paid_ability_window
        && window.active_priority != side
    {
        return Err(RulesError::NotYourPriority { expected: window.active_priority, actual: side });
    }

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    let ability = card_def
        .abilities
        .get(ability_index)
        .ok_or(RulesError::InvalidAbilityIndex(ability_index))?;
    if ability.trigger != Trigger::Paid {
        return Err(RulesError::AbilityNotManuallyActivatable(ability_index));
    }
    if let Some(requirement) = &ability.requirement {
        ability::check_requirement(state, requirement, side, &ability::ResolutionContext::for_card(Some(&card_id)), registry)?;
    }

    let mut next = state.clone();
    let mut events = Vec::new();
    if let Some(cost) = &ability.cost {
        // A conditional per-ability discount (e.g. Marjanah: "-1 to use if
        // you made a successful run this turn") only meaningfully applies
        // to `Cost::Credits` — re-evaluated fresh every activation, no
        // once-per-turn consumption, unlike `first_install_discount`.
        let discounted = match (cost, &ability.cost_discount_if) {
            (Cost::Credits(amount), Some((requirement, discount)))
                if ability::check_requirement(&next, requirement, side, &ability::ResolutionContext::for_card(Some(&card_id)), registry).is_ok() =>
            {
                Cost::Credits(amount.saturating_sub(*discount))
            }
            _ => cost.clone(),
        };
        events.extend(ability::pay_cost(&mut next, side, &discounted, Some(&card_id))?);
    }
    events.push(GameEvent::AbilityActivated { side, card_id: card_id.clone(), ability_index });
    events.extend(ability::evaluate_effect(&mut next, &ability.effect, &mut ability::ResolutionContext::for_card(Some(&card_id)), registry)?);
    // `check_requirement` above only reads — without this, a `Paid`
    // ability's `EffectRequirement::OncePerTurn` (e.g. Telework Contract's
    // click ability) would never actually get marked used and could be
    // activated any number of times per turn. Mirrors
    // `process_card_triggers`'s own check-then-consume ordering.
    if let Some(requirement) = &ability.requirement {
        ability::consume_requirement(&mut next, requirement, side);
    }
    paid_ability::note_window_action(&mut next, side);

    Ok((next, events))
}

/// Places one advancement token on `card_id`, per
/// `PlayerAction::AdvanceCard`'s doc comment. Corp-only, like `install_card`/
/// `rez_ice`.
fn advance_card(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Corp;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    spend_click(&mut next, side)?;

    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(1), Some(&card_id))?);

    let installed = next
        .corp
        .installed
        .iter_mut()
        .find(|c| c.card == card_id)
        .ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    if card_def.advancement_requirement.is_none() {
        return Err(RulesError::CardNotAdvanceable { card: card_id });
    }

    installed.advancement_tokens += 1;
    let advancement_tokens = installed.advancement_tokens;
    // No "was this the first advancement?" flag is recorded: the event
    // below already carries `advancement_tokens`, and
    // `EffectRequirement::WasFirstAdvancementThisCard` reads it from the
    // `ability::ResolutionContext` the dispatch builds.
    let advanced_event = GameEvent::CardAdvanced { card: card_id, advancement_tokens };
    events.push(advanced_event.clone());
    events.extend(dispatcher::dispatch_event(&mut next, registry, &advanced_event)?);

    Ok((next, events))
}

/// Resolves `PlayerAction::ScoreAgenda`, per its doc comment. Corp-only,
/// costs 1 click, no rez requirement.
fn score_agenda(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Corp;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    // Luminal Transubstantiation's lockout. Checked here as well as filtered
    // out of `legal_actions` so the two can't disagree.
    if state.corp.cannot_score_agendas_this_turn {
        return Err(RulesError::CannotScoreAgendasThisTurn);
    }

    let position = state
        .corp
        .installed
        .iter()
        .position(|installed| installed.card == card_id)
        .ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
    let advancement_tokens = state.corp.installed[position].advancement_tokens;
    let server = state.corp.installed[position].server;

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    if card_def.card_type != CardType::Agenda {
        return Err(RulesError::CardNotAgenda { card: card_id });
    }
    let required = card_def.advancement_requirement.unwrap_or(0);
    if advancement_tokens < required {
        return Err(RulesError::AdvancementRequirementNotMet {
            card: card_id,
            current: advancement_tokens,
            required,
        });
    }
    let agenda_points = card_def.agenda_points.unwrap_or(0);

    let mut next = state.clone();
    spend_click(&mut next, side)?;
    next.corp.installed.remove(position);
    next.corp.scored_agendas.push(card_id.clone());
    next.corp.resources.agenda_points = next.corp.resources.agenda_points.gain(agenda_points);
    next.corp.agenda_points_scored_this_turn =
        next.corp.agenda_points_scored_this_turn.saturating_add(agenda_points);

    let scored_event = GameEvent::AgendaScored { card: card_id.clone(), agenda_points, server };
    let mut events = vec![GameEvent::ClickSpent { side }, scored_event.clone()];
    // `dispatch_event` fires the agenda's own "on score" text (e.g. Hostile
    // Takeover), then the Corp identity's reactive ability if one is set
    // (e.g. Jinteki: Personal Evolution) — unconditional dispatch, no
    // per-turn gate.
    events.extend(dispatcher::dispatch_event(&mut next, registry, &scored_event)?);

    win::check_win_conditions(&mut next, registry);

    Ok((next, events))
}

/// Resolves `PlayerAction::RemoveTag`, per its doc comment. Runner-only.
fn remove_tag(state: &GameState) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    if !state.runner.is_tagged() {
        return Err(RulesError::RunnerNotTagged);
    }

    let mut next = state.clone();
    spend_click(&mut next, side)?;

    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(2), None)?);

    next.runner.tags -= 1;
    events.push(GameEvent::TagRemoved { side });

    Ok((next, events))
}

/// Resolves `PlayerAction::PurgeVirusCounters`, per its doc comment.
/// Corp-only, 3 clicks — the Corp's entire turn.
///
/// Scans both sides: `counter_kind` describes what a card's `counters`
/// field holds, so "every virus counter in play" is exactly "every
/// installed/rigged card whose registry `counter_kind` is `Virus`",
/// regardless of who controls it. No card in the current pool is a Corp
/// card with virus counters, but nothing here assumes that.
///
/// Zeroing an already-zero card is a no-op that still reports the card as
/// purged — matching the physical action (you sweep the board, not
/// individual tokens) and keeping the emitted event a straightforward
/// "these are the virus cards that were on the table."
fn purge_virus_counters(
    state: &GameState,
    registry: &CardRegistry,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Corp;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;

    let mut next = state.clone();
    let mut events = ability::pay_cost(&mut next, side, &Cost::Clicks(3), None)?;

    let holds_virus_counters =
        |card_id: &CardId| registry.get(card_id).and_then(|c| c.counter_kind) == Some(CounterKind::Virus);

    let mut purged = Vec::new();
    for installed in next.corp.installed.iter_mut().filter(|c| holds_virus_counters(&c.card)) {
        installed.counters = 0;
        purged.push(installed.card.clone());
    }
    for rigged in next.runner.rig.iter_mut().filter(|c| holds_virus_counters(&c.card)) {
        rigged.counters = 0;
        purged.push(rigged.card.clone());
    }

    events.push(GameEvent::VirusCountersPurged { cards: purged });

    Ok((next, events))
}

/// Resolves `PlayerAction::TrashResource`, per its doc comment. Corp-only.
fn trash_resource(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Corp;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    if !state.runner.is_tagged() {
        return Err(RulesError::RunnerNotTagged);
    }

    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    if card_def.card_type != CardType::Resource {
        return Err(RulesError::CardNotResource { card: card_id });
    }

    let mut next = state.clone();
    spend_click(&mut next, side)?;

    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(2), None)?);

    let position = next
        .runner
        .rig
        .iter()
        .position(|c| c.card == card_id)
        .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: card_id.clone() })?;
    let removed = next.runner.rig.remove(position);
    next.runner.heap.push(removed.card);
    events.push(GameEvent::CardTrashed { side: Side::Runner, card: card_id });

    Ok((next, events))
}

/// Resolves `PlayerAction::SelectCardToAccess`, per its doc comment.
/// Runner-only, like every other access-resolution action.
fn select_card_to_access(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    require_phase(state, GamePhase::Action(Side::Runner))?;
    // Never actually open when this is legal in practice — a window only
    // opens at `PendingChoice`/`PendingInteractiveTrigger`, never at
    // `SelectNextCard` (the only phase this action is legal in) — but kept
    // for consistency with every other access-resolution action below.
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    let mut events = run::resolve_select_card(&mut next, &card_id, registry)?;
    events.extend(paid_ability::open_window_if_at_checkpoint(&mut next));
    Ok((next, events))
}

/// Resolves `PlayerAction::StealAgenda`, per its doc comment. Runner-only,
/// like every other access-resolution action.
fn steal_agenda(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    require_phase(state, GamePhase::Action(Side::Runner))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    let mut events = run::resolve_steal(&mut next, &card_id, registry)?;
    events.extend(paid_ability::open_window_if_at_checkpoint(&mut next));
    Ok((next, events))
}

/// Resolves `PlayerAction::TrashAccessedCard`, per its doc comment.
fn trash_accessed_card(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    require_phase(state, GamePhase::Action(Side::Runner))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    let mut events = run::resolve_trash(&mut next, &card_id, registry)?;
    events.extend(paid_ability::open_window_if_at_checkpoint(&mut next));
    Ok((next, events))
}

/// Resolves `PlayerAction::PassAccessedCard`, per its doc comment.
fn pass_accessed_card(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    require_phase(state, GamePhase::Action(Side::Runner))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    let mut events = run::resolve_pass(&mut next, &card_id, registry)?;
    events.extend(paid_ability::open_window_if_at_checkpoint(&mut next));
    Ok((next, events))
}

/// Resolves `PlayerAction::PayAccessTrigger`, per its doc comment.
fn pay_access_trigger(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    require_phase(state, GamePhase::Action(Side::Runner))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    let mut events = run::resolve_pay_access_trigger(&mut next, &card_id, registry)?;
    events.extend(paid_ability::open_window_if_at_checkpoint(&mut next));
    Ok((next, events))
}

/// Resolves `PlayerAction::DeclineAccessTrigger`, per its doc comment.
fn decline_access_trigger(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    require_phase(state, GamePhase::Action(Side::Runner))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    let mut events = run::resolve_decline_access_trigger(&mut next, &card_id, registry)?;
    events.extend(paid_ability::open_window_if_at_checkpoint(&mut next));
    Ok((next, events))
}

/// Resolves `PlayerAction::PassPriority`, per its doc comment. No
/// `require_phase` call — legality is fully governed by
/// `paid_ability::pass_priority`'s own `NotInPaidAbilityWindow`/
/// `NotYourPriority` checks, since the whole point of a window is letting
/// the *non*-active-turn side (e.g. Corp during the Runner's run) act, so
/// gating on `GamePhase::Action(side)` would wrongly reject their pass.
fn pass_priority_action(
    state: &GameState,
    registry: &CardRegistry,
    side: Side,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let mut next = state.clone();
    let events = paid_ability::pass_priority(&mut next, registry, side)?;
    Ok((next, events))
}

/// Resolves `PlayerAction::SubmitCorpTraceBid`, per its doc comment.
fn submit_corp_trace_bid(state: &GameState, amount: u32) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let mut next = state.clone();
    let events = trace::submit_corp_bid(&mut next, amount)?;
    Ok((next, events))
}

/// Resolves `PlayerAction::SubmitRunnerTraceBid`, per its doc comment.
fn submit_runner_trace_bid(
    state: &GameState,
    registry: &CardRegistry,
    amount: u32,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let mut next = state.clone();
    let events = trace::submit_runner_bid(&mut next, amount, registry)?;
    Ok((next, events))
}

/// Resolves `PlayerAction::AcceptPendingPaidChoice`, per its doc comment.
fn accept_pending_paid_choice(
    state: &GameState,
    registry: &CardRegistry,
    cost_option_index: Option<usize>,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let mut next = state.clone();
    let events = pending_choice::resolve_accept(&mut next, registry, cost_option_index)?;
    Ok((next, events))
}

/// Resolves `PlayerAction::DeclinePendingPaidChoice`, per its doc comment.
fn decline_pending_paid_choice(
    state: &GameState,
    registry: &CardRegistry,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let mut next = state.clone();
    let events = pending_choice::resolve_decline(&mut next, registry)?;
    Ok((next, events))
}

/// Resolves `PlayerAction::ResolvePendingChoice`, per its doc comment.
fn resolve_pending_choice(
    state: &GameState,
    registry: &CardRegistry,
    option_index: usize,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let mut next = state.clone();
    let events = pending_choice::resolve_choice(&mut next, registry, option_index)?;
    Ok((next, events))
}

fn choose_trigger_to_resolve(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let mut next = state.clone();
    let events = pending_choice::resolve_choose_trigger_to_resolve(&mut next, registry, card_id)?;
    Ok((next, events))
}

/// Resolves `PlayerAction::ToggleCardSelection`, per its doc comment.
fn toggle_card_selection(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let mut next = state.clone();
    let events = pending_choice::resolve_toggle_card_selection(&mut next, registry, card_id)?;
    Ok((next, events))
}

/// Resolves `PlayerAction::ConfirmCardSelection`, per its doc comment.
fn confirm_card_selection(
    state: &GameState,
    registry: &CardRegistry,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let mut next = state.clone();
    let events = pending_choice::resolve_confirm_card_selection(&mut next, registry)?;
    Ok((next, events))
}

/// Resolves `PlayerAction::ChooseServerForPendingDecision`, per its doc comment.
fn choose_server_for_pending_decision(
    state: &GameState,
    registry: &CardRegistry,
    server: ServerTarget,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let mut next = state.clone();
    let events = pending_choice::resolve_choose_server(&mut next, registry, server)?;
    Ok((next, events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{
        AbilityDef, BoostDuration, CardDefinition, CardType, Cost, Effect, IceType, SubroutineBreakCount,
        SubroutineDef, TriggeredEffect,
    };
    use crate::rules::run::{EncounteredSubroutine, RunIce, RunState, ServerId, SubroutineStatus};
    use crate::rules::state::{AgendaPoints, Clicks, Credits, PaidAbilityWindow, PlayerResources, WindowCheckpoint};

    /// An empty registry, for every test that doesn't exercise
    /// `PlayerAction::ActivateAbility` and so doesn't need real card
    /// definitions.
    fn registry() -> CardRegistry {
        CardRegistry::new()
    }

    /// See `turn::tests::close_all_windows`'s doc comment — same helper,
    /// duplicated here since that one lives in a private `mod tests`.
    fn close_all_windows(mut state: GameState, registry: &CardRegistry) -> (GameState, Vec<GameEvent>) {
        let mut events = Vec::new();
        while let Some(window) = &state.paid_ability_window {
            let side = window.active_priority;
            let (next, ev) = apply_action(&state, registry, PlayerAction::PassPriority { side })
                .expect("pass priority should succeed");
            state = next;
            events.extend(ev);
        }
        (state, events)
    }

    /// Builds a `RunIce` with `subroutine_count` placeholder `Pending`
    /// subroutines — identity/effect content doesn't matter for tests using
    /// this, only status transitions and counts do.
    fn test_ice(card_id: &str, strength: i32, subroutine_count: usize, rezzed: bool) -> RunIce {
        test_ice_of_type(card_id, strength, subroutine_count, rezzed, IceType::Barrier)
    }

    fn test_ice_of_type(
        card_id: &str,
        strength: i32,
        subroutine_count: usize,
        rezzed: bool,
        ice_type: IceType,
    ) -> RunIce {
        RunIce {
            card_id: CardId(card_id.to_string()),
            current_strength: strength,
            ice_type,
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

    fn corp_state(clicks: u32, credits: u32) -> GameState {
        GameState {
            corp: crate::rules::state::CorpState {
                resources: PlayerResources {
                    credits: Credits(credits),
                    clicks: Clicks(clicks),
                    agenda_points: AgendaPoints(0),
                },
                ..Default::default()
            },
            runner: crate::rules::state::RunnerState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: crate::rules::state::MemoryUnits(0),
                ..Default::default()
            },
            phase: GamePhase::Action(Side::Corp),
            ..Default::default()
        }
    }

    /// `stack_size`/`grip_size` are filled with distinct placeholder `CardId`s
    /// (identity doesn't matter for the tests using this — only counts do).
    fn runner_state(clicks: u32, stack_size: u32, grip_size: u32) -> GameState {
        GameState {
            corp: crate::rules::state::CorpState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                ..Default::default()
            },
            runner: crate::rules::state::RunnerState {
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(clicks),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: crate::rules::state::MemoryUnits(0),
                grip: (0..grip_size).map(|i| CardId(format!("grip_card_{i}"))).collect(),
                stack: (0..stack_size).map(|i| CardId(format!("stack_card_{i}"))).collect(),
                ..Default::default()
            },
            phase: GamePhase::Action(Side::Runner),
            ..Default::default()
        }
    }

    fn corp_state_with_hq_and_installed(
        clicks: u32,
        credits: u32,
        hq: Vec<CardId>,
        installed: Vec<InstalledCard>,
    ) -> GameState {
        let mut state = corp_state(clicks, credits);
        state.corp.hq = hq;
        state.corp.installed = installed;
        state
    }

    fn runner_state_with_grip(
        clicks: u32,
        credits: u32,
        memory_units: u32,
        grip: Vec<CardId>,
    ) -> GameState {
        let mut state = runner_state(clicks, 0, 0);
        state.runner.resources.credits = Credits(credits);
        state.runner.memory_units = crate::rules::state::MemoryUnits(memory_units);
        state.runner.grip = grip;
        state
    }

    #[test]
    fn corp_gain_credit_click_spends_click_and_gains_credit() {
        let state = corp_state(3, 5);
        let (next, events) = apply_action(&state, &registry(), PlayerAction::GainCreditClick { side: Side::Corp })
            .expect("action should succeed");

        assert_eq!(next.corp.resources.clicks, Clicks(2));
        assert_eq!(next.corp.resources.credits, Credits(6));
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Corp },
                GameEvent::CreditsGained {
                    side: Side::Corp,
                    amount: 1
                },
            ]
        );

        // Original state is untouched.
        assert_eq!(state.corp.resources.clicks, Clicks(3));
        assert_eq!(state.corp.resources.credits, Credits(5));
    }

    #[test]
    fn runner_draw_card_click_spends_click_and_draws_card() {
        let state = runner_state(4, 10, 5);
        let (next, events) =
            apply_action(&state, &registry(), PlayerAction::DrawCardClick).expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(3));
        assert_eq!(next.runner.stack.len(), 9);
        assert_eq!(next.runner.grip.len(), 6);
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Runner },
                GameEvent::CardDrawn { side: Side::Runner },
                GameEvent::BasicDrawActionTaken { side: Side::Runner },
            ]
        );
    }

    #[test]
    fn spending_click_with_zero_clicks_returns_error() {
        let state = corp_state(0, 5);
        let result = apply_action(&state, &registry(), PlayerAction::GainCreditClick { side: Side::Corp });

        assert_eq!(
            result,
            Err(RulesError::NotEnoughClicks {
                side: Side::Corp,
                available: 0,
                requested: 1,
            })
        );
    }

    #[test]
    fn acting_out_of_turn_returns_error() {
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::GainCreditClick { side: Side::Runner });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_draw_card_click_with_empty_stack_does_not_underflow() {
        let state = runner_state(2, 0, 3);
        let (next, events) =
            apply_action(&state, &registry(), PlayerAction::DrawCardClick).expect("action should succeed");

        assert_eq!(next.runner.stack.len(), 0);
        assert_eq!(next.runner.grip.len(), 3);
        assert_eq!(next.runner.resources.clicks, Clicks(1));
        assert_eq!(
            events,
            vec![GameEvent::ClickSpent { side: Side::Runner }, GameEvent::BasicDrawActionTaken { side: Side::Runner }]
        );
    }

    #[test]
    fn corp_install_card_moves_card_from_hq_to_installed_and_spends_click_and_credits() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, vec![card_id.clone()], Vec::new());
        let mut registry = CardRegistry::new();
        registry.insert(test_card("ice_wall", Side::Corp, CardType::Ice(crate::dsl::IceType::Barrier), 1, None));

        let (next, events) = apply_action(
            &state,
            &registry,
            PlayerAction::InstallCard {
                card_id: card_id.clone(),
                zone: ServerId::Hq,
                slot: InstallSlot::Ice,
            },
        )
        .expect("action should succeed");

        assert_eq!(next.corp.resources.clicks, Clicks(2));
        assert_eq!(next.corp.resources.credits, Credits(4));
        assert!(next.corp.hq.is_empty());
        assert_eq!(
            next.corp.installed,
            vec![InstalledCard {
                card: card_id.clone(),
                slot: InstallSlot::Ice,
                // Seamless Launch's eligibility marker — set by every
                // install, cleared at the Corp's next turn start.
                installed_this_turn: true,
                ..Default::default()
            }]
        );
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Corp },
                GameEvent::CreditsSpent { side: Side::Corp, amount: 1 },
                GameEvent::CardInstalled {
                    side: Side::Corp,
                    card: card_id,
                    server: ServerId::Hq,
                },
            ]
        );

        // Original state is untouched.
        assert_eq!(state.corp.hq, vec![CardId("ice_wall".to_string())]);
        assert!(state.corp.installed.is_empty());
    }

    #[test]
    fn corp_install_card_not_in_registry_returns_card_not_found_in_registry() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, vec![card_id.clone()], Vec::new());

        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallCard { card_id: card_id.clone(), zone: ServerId::Hq, slot: InstallSlot::Ice },
        );

        assert_eq!(result, Err(RulesError::CardNotFoundInRegistry(card_id)));
    }

    #[test]
    fn corp_install_card_with_insufficient_credits_for_registry_cost_returns_not_enough_credits() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(3, 0, vec![card_id.clone()], Vec::new());
        let mut registry = CardRegistry::new();
        registry.insert(test_card("ice_wall", Side::Corp, CardType::Ice(crate::dsl::IceType::Barrier), 1, None));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::InstallCard { card_id, zone: ServerId::Hq, slot: InstallSlot::Ice },
        );

        assert_eq!(
            result,
            Err(RulesError::NotEnoughCredits { side: Side::Corp, available: 0, requested: 1 })
        );
    }

    #[test]
    fn runner_turn_install_card_returns_not_your_turn() {
        let card_id = CardId("ice_wall".to_string());
        let state = runner_state(3, 5, 3);
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallCard { card_id, zone: ServerId::Hq, slot: InstallSlot::Ice },
        );

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Corp),
                actual: GamePhase::Action(Side::Runner),
            })
        );
    }

    #[test]
    fn corp_install_card_with_card_not_in_hq_returns_card_not_in_hand() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), Vec::new());
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallCard {
                card_id: card_id.clone(),
                zone: ServerId::Hq,
                slot: InstallSlot::Ice,
            },
        );

        assert_eq!(
            result,
            Err(RulesError::CardNotInHand { side: Side::Corp, card: card_id })
        );
    }

    #[test]
    fn corp_install_card_with_zero_clicks_returns_not_enough_clicks() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(0, 5, vec![card_id.clone()], Vec::new());
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallCard { card_id, zone: ServerId::Hq, slot: InstallSlot::Ice },
        );

        assert_eq!(
            result,
            Err(RulesError::NotEnoughClicks { side: Side::Corp, available: 0, requested: 1 })
        );
    }

    #[test]
    fn corp_rez_ice_flips_installed_card_and_pays_registry_cost() {
        let card_id = CardId("ice_wall".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            slot: InstallSlot::Ice,
            ..Default::default()
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        let registry = CardRegistry::from_cards(vec![test_card("ice_wall", Side::Corp, CardType::Ice(IceType::Barrier), 1, None)]);
        let (next, events) = apply_action(&state, &registry, PlayerAction::RezIce { ice_id: card_id.clone() })
            .expect("action should succeed");

        assert!(next.corp.installed[0].rezzed);
        assert_eq!(next.corp.resources.clicks, Clicks(3));
        assert_eq!(next.corp.resources.credits, Credits(4));
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Corp, amount: 1 },
                GameEvent::IceRezzed { card: card_id, server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn corp_rez_ice_with_insufficient_credits_returns_not_enough_credits() {
        let card_id = CardId("ice_wall".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            slot: InstallSlot::Ice,
            ..Default::default()
        }];
        let state = corp_state_with_hq_and_installed(3, 0, Vec::new(), installed);
        let registry = CardRegistry::from_cards(vec![test_card("ice_wall", Side::Corp, CardType::Ice(IceType::Barrier), 1, None)]);
        let result = apply_action(&state, &registry, PlayerAction::RezIce { ice_id: card_id });

        assert_eq!(result, Err(RulesError::NotEnoughCredits { side: Side::Corp, available: 0, requested: 1 }));
    }

    #[test]
    fn corp_rez_ice_for_card_missing_from_registry_returns_card_not_found_in_registry() {
        let card_id = CardId("ice_wall".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            slot: InstallSlot::Ice,
            ..Default::default()
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        let result = apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::CardNotFoundInRegistry(card_id)));
    }

    #[test]
    fn runner_turn_rez_ice_returns_not_your_turn() {
        let card_id = CardId("ice_wall".to_string());
        let state = runner_state(3, 5, 3);
        let result = apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: card_id });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Corp),
                actual: GamePhase::Action(Side::Runner),
            })
        );
    }

    #[test]
    fn corp_rez_ice_with_card_not_installed_returns_card_not_installed() {
        let card_id = CardId("ice_wall".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), Vec::new());
        let result = apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::CardNotInstalled { card: card_id }));
    }

    #[test]
    fn corp_rez_ice_already_rezzed_returns_already_rezzed() {
        let card_id = CardId("ice_wall".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        let result = apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::AlreadyRezzed { card: card_id }));
    }

    #[test]
    fn corp_can_rez_ice_during_run_approach_ice_even_though_phase_is_runner_action() {
        let card_id = CardId("ice_wall".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            slot: InstallSlot::Ice,
            ..Default::default()
        }];
        let mut state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        state.phase = GamePhase::Action(Side::Runner);
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1, false)],
            jack_out_permitted: true,
            ..Default::default()
        });

        let registry = CardRegistry::from_cards(vec![test_card("ice_wall", Side::Corp, CardType::Ice(IceType::Barrier), 0, None)]);
        let (next, events) = apply_action(&state, &registry, PlayerAction::RezIce { ice_id: card_id.clone() })
            .expect("Corp should be able to rez ICE during the Runner's run");

        assert!(next.corp.installed[0].rezzed);
        assert!(next.active_run.as_ref().unwrap().ice[0].rezzed);
        assert_eq!(next.corp.resources.clicks, Clicks(3));
        assert_eq!(next.corp.resources.credits, Credits(5));
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Corp, amount: 0 },
                GameEvent::IceRezzed { card: card_id, server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn corp_rez_ice_for_ice_not_at_current_position_does_not_affect_run_ice() {
        let outer = CardId("outer_ice".to_string());
        let inner = CardId("inner_ice".to_string());
        let installed = vec![
            InstalledCard {
                card: outer.clone(),
                slot: InstallSlot::Ice,
                ..Default::default()
            },
            InstalledCard {
                card: inner.clone(),
                slot: InstallSlot::Ice,
                ..Default::default()
            },
        ];
        let mut state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        state.phase = GamePhase::Action(Side::Runner);
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("outer_ice", 0, 1, false), test_ice("inner_ice", 0, 1, false)],
            jack_out_permitted: true,
            ..Default::default()
        });

        let registry = CardRegistry::from_cards(vec![
            test_card("outer_ice", Side::Corp, CardType::Ice(IceType::Barrier), 0, None),
            test_card("inner_ice", Side::Corp, CardType::Ice(IceType::Barrier), 0, None),
        ]);
        let (next, _events) = apply_action(&state, &registry, PlayerAction::RezIce { ice_id: inner.clone() })
            .expect("Corp should be able to pre-emptively rez ICE the run hasn't reached yet");

        assert!(next.corp.installed.iter().find(|c| c.card == inner).unwrap().rezzed);
        let run = next.active_run.unwrap();
        assert!(!run.ice[0].rezzed, "the currently-approached ICE must be untouched");
        assert!(!run.ice[1].rezzed, "only InstalledCard::rezzed flips for ICE not at `position`");
    }

    #[test]
    fn runner_initiate_run_starts_run_and_spends_click() {
        let state = runner_state(3, 5, 3);
        let (next, events) = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::Hq })
            .expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(2));
        assert_eq!(
            next.active_run,
            Some(RunState {
                ..Default::default()
            })
        );
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Runner },
                GameEvent::RunInitiated { server: ServerId::Hq },
            ]
        );

        // Original state is untouched.
        assert_eq!(state.active_run, None);
    }

    #[test]
    fn runner_initiate_run_populates_ice_from_installed_ice_outermost_first() {
        let outer = InstalledCard {
            card: CardId("outer_ice".to_string()),
            slot: InstallSlot::Ice,
            ..Default::default()
        };
        let inner = InstalledCard {
            card: CardId("inner_ice".to_string()),
            slot: InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        };
        let state = corp_state_with_hq_and_installed(0, 0, Vec::new(), vec![outer, inner]);
        let mut state = state;
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = runner_state(3, 5, 3).runner;

        let mut registry = CardRegistry::new();
        registry.insert(test_card("outer_ice", Side::Corp, CardType::Ice(IceType::Barrier), 1, None));
        let mut inner_card = test_card("inner_ice", Side::Corp, CardType::Ice(IceType::Barrier), 1, None);
        inner_card.strength = Some(2);
        inner_card.subroutines =
            vec![SubroutineDef { text: "End the run.".to_string(), effect: Effect::EndTheRun }];
        registry.insert(inner_card);

        let (next, _events) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq })
            .expect("action should succeed");

        let ice = next.active_run.unwrap().ice;
        assert_eq!(ice.len(), 2);
        assert_eq!(ice[0].card_id, CardId("outer_ice".to_string()));
        assert_eq!(ice[0].current_strength, 0);
        assert!(ice[0].subroutines.is_empty());
        assert!(!ice[0].rezzed);
        assert_eq!(ice[1].card_id, CardId("inner_ice".to_string()));
        assert_eq!(ice[1].current_strength, 2);
        assert_eq!(ice[1].subroutines.len(), 1);
        assert!(ice[1].rezzed);
    }

    #[test]
    fn runner_initiate_run_ignores_root_installs_and_other_servers_ice() {
        let installed = vec![
            InstalledCard {
                card: CardId("some_upgrade".to_string()),
                ..Default::default()
            },
            InstalledCard {
                card: CardId("remote_ice".to_string()),
                server: ServerId::Remote(0),
                slot: InstallSlot::Ice,
                rezzed: true,
                ..Default::default()
            },
        ];
        let mut state = corp_state_with_hq_and_installed(0, 0, Vec::new(), installed);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = runner_state(3, 5, 3).runner;

        let (next, _events) = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::Hq })
            .expect("action should succeed");

        assert!(next.active_run.unwrap().ice.is_empty());
    }

    #[test]
    fn runner_initiate_run_with_unregistered_ice_returns_card_not_found_in_registry() {
        let installed = vec![InstalledCard {
            card: CardId("mystery_ice".to_string()),
            slot: InstallSlot::Ice,
            ..Default::default()
        }];
        let mut state = corp_state_with_hq_and_installed(0, 0, Vec::new(), installed);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = runner_state(3, 5, 3).runner;

        let result = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::Hq });

        assert_eq!(result, Err(RulesError::CardNotFoundInRegistry(CardId("mystery_ice".to_string()))));
    }

    #[test]
    fn runner_initiate_run_with_registered_ice_missing_strength_and_subroutines_still_builds_blank_defaults() {
        let installed = vec![InstalledCard {
            card: CardId("vanilla_ice".to_string()),
            slot: InstallSlot::Ice,
            ..Default::default()
        }];
        let mut state = corp_state_with_hq_and_installed(0, 0, Vec::new(), installed);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = runner_state(3, 5, 3).runner;
        let registry = CardRegistry::from_cards(vec![test_card("vanilla_ice", Side::Corp, CardType::Ice(IceType::Barrier), 0, None)]);

        let (next, _events) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq })
            .expect("action should succeed");

        let ice = next.active_run.unwrap().ice;
        assert_eq!(
            ice,
            vec![RunIce {
                card_id: CardId("vanilla_ice".to_string()),
                current_strength: 0,
                ice_type: IceType::Barrier,
                subroutines: Vec::new(),
                rezzed: false,
            }]
        );
    }

    #[test]
    fn corp_turn_initiate_run_returns_not_your_turn() {
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::Hq });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_initiate_run_with_run_already_active_returns_run_already_in_progress() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            jack_out_permitted: true,
            ..Default::default()
        });
        let result = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::RnD });

        assert_eq!(result, Err(RulesError::RunAlreadyInProgress));
    }

    #[test]
    fn runner_initiate_run_with_zero_clicks_returns_not_enough_clicks() {
        let state = runner_state(0, 5, 3);
        let result = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::Hq });

        assert_eq!(
            result,
            Err(RulesError::NotEnoughClicks { side: Side::Runner, available: 0, requested: 1 })
        );
    }

    #[test]
    fn runner_jack_out_ends_run_clears_active_run_no_click_cost() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            jack_out_permitted: true,
            ..Default::default()
        });
        let (next, events) =
            apply_action(&state, &registry(), PlayerAction::JackOut).expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(3));
        assert_eq!(next.active_run, None);
        assert_eq!(events, vec![GameEvent::RunJackedOut { server: ServerId::Hq }]);
    }

    #[test]
    fn corp_turn_jack_out_returns_not_your_turn() {
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::JackOut);

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_jack_out_with_no_active_run_returns_no_active_run() {
        let state = runner_state(3, 5, 3);
        let result = apply_action(&state, &registry(), PlayerAction::JackOut);

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }

    #[test]
    fn runner_jack_out_during_initial_approach_returns_illegal_jack_out_window() {
        let installed = vec![InstalledCard {
            card: CardId("ice_wall".to_string()),
            slot: InstallSlot::Ice,
            ..Default::default()
        }];
        let mut state = corp_state_with_hq_and_installed(0, 0, Vec::new(), installed);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = runner_state(3, 5, 3).runner;
        let registry = CardRegistry::from_cards(vec![test_card("ice_wall", Side::Corp, CardType::Ice(IceType::Barrier), 0, None)]);

        let (after_initiate, _) = apply_action(&state, &registry, PlayerAction::InitiateRun { server: ServerId::Hq })
            .expect("initiate run should succeed");
        let result = apply_action(&after_initiate, &registry, PlayerAction::JackOut);

        assert_eq!(result, Err(RulesError::IllegalJackOutWindow { phase: RunPhase::Initiation }));
        assert!(after_initiate.active_run.is_some());
    }

    #[test]
    fn runner_jack_out_succeeds_on_ice_less_server_before_access() {
        let state = runner_state(3, 5, 3);

        let (after_initiate, _) = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::Hq })
            .expect("initiate run should succeed");
        let (after_continue, _) = apply_action(&after_initiate, &registry(), PlayerAction::ContinueRun)
            .expect("continue run should succeed");
        assert_eq!(after_continue.active_run.as_ref().unwrap().phase, RunPhase::Success);

        let (after_jack_out, events) = apply_action(&after_continue, &registry(), PlayerAction::JackOut)
            .expect("jack out should succeed at the server approach step");

        assert_eq!(after_jack_out.active_run, None);
        assert_eq!(events, vec![GameEvent::RunJackedOut { server: ServerId::Hq }]);
    }

    #[test]
    fn runner_jack_out_on_concluded_run_propagates_run_already_concluded() {
        // `RunPhase::AccessingCard` — genuinely terminal for `JackOut`,
        // unlike `RunPhase::Success` (legal there — the "approach server"
        // jack-out window; see `runner_jack_out_succeeds_on_ice_less_server_before_access`).
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            phase: RunPhase::AccessingCard,
            jack_out_permitted: true,
            ..Default::default()
        });
        let result = apply_action(&state, &registry(), PlayerAction::JackOut);

        assert_eq!(
            result,
            Err(RulesError::RunAlreadyConcluded { phase: RunPhase::AccessingCard })
        );
    }

    #[test]
    fn runner_can_initiate_run_again_after_jacking_out() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            jack_out_permitted: true,
            ..Default::default()
        });

        let (after_jack_out, _) =
            apply_action(&state, &registry(), PlayerAction::JackOut).expect("jack out should succeed");
        let (after_initiate, _) = apply_action(
            &after_jack_out,
            &registry(),
            PlayerAction::InitiateRun { server: ServerId::RnD },
        )
        .expect("initiating a new run should succeed");

        assert_eq!(
            after_initiate.active_run,
            Some(RunState {
                server: ServerId::RnD,
                phase: RunPhase::Initiation,
                position: 0,
                // `initiate_run` always starts a fresh run with the
                // jack-out window closed (Netrunner/Null Signal Games rule 1) — it only opens
                // via `continue_run`, which this test never calls.
                jack_out_permitted: false,
                ..Default::default()
            })
        );
    }

    #[test]
    fn runner_complete_run_clears_active_run_after_success() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            phase: RunPhase::Success,
            jack_out_permitted: true,
            ..Default::default()
        });
        let (state, complete_events) =
            apply_action(&state, &registry(), PlayerAction::CompleteRun).expect("action should succeed");
        assert_eq!(complete_events, vec![GameEvent::PaidAbilityWindowOpened { side: Side::Runner }]);

        let (state, _) = apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Runner })
            .expect("pass should succeed");
        let (next, events) = apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Corp })
            .expect("pass should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(3));
        assert_eq!(next.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::PriorityPassed { side: Side::Corp },
                GameEvent::PaidAbilityWindowClosed,
                GameEvent::RunCompleted { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn runner_complete_run_before_success_returns_run_not_concluded() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            jack_out_permitted: true,
            ..Default::default()
        });
        let result = apply_action(&state, &registry(), PlayerAction::CompleteRun);

        assert_eq!(
            result,
            Err(RulesError::RunNotConcluded { phase: RunPhase::ApproachIce })
        );
    }

    #[test]
    fn corp_turn_complete_run_returns_not_your_turn() {
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::CompleteRun);

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_complete_run_with_no_active_run_returns_no_active_run() {
        let state = runner_state(3, 5, 3);
        let result = apply_action(&state, &registry(), PlayerAction::CompleteRun);

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }

    #[test]
    fn runner_can_initiate_run_again_after_completing_previous_run() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            phase: RunPhase::Success,
            jack_out_permitted: true,
            ..Default::default()
        });

        let (state, _) =
            apply_action(&state, &registry(), PlayerAction::CompleteRun).expect("complete run should succeed");
        let (state, _) = apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Runner })
            .expect("pass should succeed");
        let (after_complete, _) = apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Corp })
            .expect("pass should succeed");
        let (after_initiate, _) = apply_action(
            &after_complete,
            &registry(),
            PlayerAction::InitiateRun { server: ServerId::RnD },
        )
        .expect("initiating a new run should succeed");

        assert_eq!(
            after_initiate.active_run,
            Some(RunState {
                server: ServerId::RnD,
                phase: RunPhase::Initiation,
                position: 0,
                // `initiate_run` always starts a fresh run with the
                // jack-out window closed (Netrunner/Null Signal Games rule 1) — it only opens
                // via `continue_run`, which this test never calls.
                jack_out_permitted: false,
                ..Default::default()
            })
        );
    }

    #[test]
    fn runner_complete_run_against_hq_parks_the_run_awaiting_an_access_choice() {
        let mut state = runner_state(3, 5, 3);
        state.corp.hq = vec![CardId("hedge_fund".to_string())];
        state.active_run = Some(RunState {
            phase: RunPhase::Success,
            jack_out_permitted: true,
            ..Default::default()
        });
        let (state, complete_events) =
            apply_action(&state, &registry(), PlayerAction::CompleteRun).expect("action should succeed");
        assert_eq!(complete_events, vec![GameEvent::PaidAbilityWindowOpened { side: Side::Runner }]);

        let (state, _) = apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Runner })
            .expect("pass should succeed");
        let (next, events) = apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Corp })
            .expect("pass should succeed");

        // Not an Agenda and not in the (empty) registry, so nothing is
        // stealable/trashable — but the run still waits for
        // `PassAccessedCard` rather than completing on its own. Landing on
        // `PendingChoice` opens a fresh window for the newly-presented card.
        assert_eq!(
            events,
            vec![
                GameEvent::PriorityPassed { side: Side::Corp },
                GameEvent::PaidAbilityWindowClosed,
                GameEvent::CardAccessed {
                    card: CardId("hedge_fund".to_string()),
                    server: ServerId::Hq
                },
                GameEvent::PaidAbilityWindowOpened { side: Side::Runner },
            ]
        );
        assert_eq!(next.paid_ability_window.as_ref().unwrap().active_priority, Side::Runner);
        assert_eq!(
            next.active_run,
            Some(RunState {
                cards_accessed_count: 1,
                access_state: Some(run::AccessState {
                    // Set when the card was presented, and left in place
                    // for the rest of its `PendingChoice`.
                    currently_accessing: Some(CardId("hedge_fund".to_string())),
                    server: ServerId::Hq,
                    unaccessed_cards: Vec::new(),
                    resolved_cards: Vec::new(),
                    phase: run::AccessPhase::PendingChoice {
                        card_id: CardId("hedge_fund".to_string()),
                        can_trash: false,
                        trash_cost: None,
                        mandatory_steal: false,
                        steal_cost: None,
                    },
                }),
                phase: RunPhase::AccessingCard,
                jack_out_permitted: true,
                ..Default::default()
            })
        );
    }

    #[test]
    fn runner_complete_run_against_empty_hq_completes_immediately() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState {
            phase: RunPhase::Success,
            jack_out_permitted: true,
            ..Default::default()
        });
        let (state, _) =
            apply_action(&state, &registry(), PlayerAction::CompleteRun).expect("action should succeed");
        let (state, _) = apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Runner })
            .expect("pass should succeed");
        let (next, events) = apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Corp })
            .expect("pass should succeed");

        assert_eq!(next.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::PriorityPassed { side: Side::Corp },
                GameEvent::PaidAbilityWindowClosed,
                GameEvent::RunCompleted { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn corp_end_turn_via_apply_action_hands_control_to_runner() {
        let state = corp_state(0, 5);
        let (next, mut events) =
            apply_action(&state, &registry(), PlayerAction::EndTurn).expect("action should succeed");
        let (next, close_events) = close_all_windows(next, &registry());
        events.extend(close_events);

        assert_eq!(next.phase, GamePhase::Action(Side::Runner));
        assert_eq!(next.runner.resources.clicks, Clicks(4));
        assert!(events.contains(&GameEvent::TurnEnded { side: Side::Corp }));
        assert!(events.contains(&GameEvent::TurnStarted { side: Side::Runner, clicks: 4 }));
    }

    #[test]
    fn runner_continue_run_steps_through_phases_with_no_click_cost() {
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState {
            ice: vec![test_ice("ice_wall", 0, 0, true)],
            jack_out_permitted: true,
            ..Default::default()
        });

        // Initiation -> ApproachIce, opening a Paid Ability Window there.
        let (state, events) =
            apply_action(&state, &registry(), PlayerAction::ContinueRun).expect("continue should succeed");
        assert_eq!(state.runner.resources.clicks, Clicks(3));
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::ApproachIce);
        assert_eq!(
            events,
            vec![
                GameEvent::IceApproached { server: ServerId::Hq, position: 0 },
                GameEvent::PaidAbilityWindowOpened { side: Side::Runner },
            ]
        );

        // A second ContinueRun is blocked while the window is open.
        let blocked = apply_action(&state, &registry(), PlayerAction::ContinueRun);
        assert_eq!(
            blocked,
            Err(RulesError::BlockedByPaidAbilityWindow { priority: Side::Runner })
        );

        // Both sides pass -> window closes -> auto-commits ApproachIce ->
        // EncounterIce (ice is rezzed), opening a fresh window there.
        let (state, _) = apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Runner })
            .expect("pass should succeed");
        let (state, events) = apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Corp })
            .expect("pass should succeed");
        assert_eq!(state.runner.resources.clicks, Clicks(3));
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::EncounterIce);
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

        // Both sides pass again -> window closes -> auto-resolves (0
        // pending) -> passes the ICE -> Success, no ICE remaining so no new
        // window opens.
        let (state, _) = apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Runner })
            .expect("pass should succeed");
        let (state, events) = apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Corp })
            .expect("pass should succeed");
        assert_eq!(state.runner.resources.clicks, Clicks(3));
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
    fn runner_continue_run_with_subroutines_pending_propagates_error() {
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            jack_out_permitted: true,
            ..Default::default()
        });

        let result = apply_action(&state, &registry(), PlayerAction::ContinueRun);

        assert_eq!(result, Err(RulesError::SubroutinesStillPending { pending: 1 }));
    }

    #[test]
    fn corp_turn_continue_run_returns_not_your_turn() {
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::ContinueRun);

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_continue_run_with_no_active_run_returns_no_active_run() {
        let state = runner_state(3, 0, 0);
        let result = apply_action(&state, &registry(), PlayerAction::ContinueRun);

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }

    #[test]
    fn runner_play_event_removes_card_from_grip_and_spends_click_and_credits() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(3, 5, 0, vec![card_id.clone()]);
        let mut registry = CardRegistry::new();
        registry.insert(test_card("sure_gamble", Side::Runner, CardType::Event, 5, None));

        let (next, events) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: card_id.clone() })
            .expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(2));
        assert_eq!(next.runner.resources.credits, Credits(0));
        assert!(next.runner.grip.is_empty());
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Runner },
                GameEvent::CreditsSpent { side: Side::Runner, amount: 5 },
                GameEvent::EventPlayed { side: Side::Runner, card: card_id },
            ]
        );

        // Original state is untouched.
        assert_eq!(state.runner.grip, vec![CardId("sure_gamble".to_string())]);
    }

    #[test]
    fn runner_play_event_fires_on_play_trigger_and_grants_credits() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(3, 5, 0, vec![card_id.clone()]);
        let mut registry = CardRegistry::new();
        let mut card = test_card("sure_gamble", Side::Runner, CardType::Event, 5, None);
        card.triggers = vec![TriggeredEffect {
            trigger: Trigger::OnPlay,
            effects: vec![Effect::GainCredits(Side::Runner, 9)],
            requirement: None,
        }];
        registry.insert(card);

        let (next, events) = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id: card_id.clone() })
            .expect("action should succeed");

        // Paid 5 to play, then the OnPlay trigger grants 9 back — net +4.
        assert_eq!(next.runner.resources.credits, Credits(9));
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Runner },
                GameEvent::CreditsSpent { side: Side::Runner, amount: 5 },
                GameEvent::EventPlayed { side: Side::Runner, card: card_id },
                GameEvent::CreditsGained { side: Side::Runner, amount: 9 },
            ]
        );
    }

    #[test]
    fn runner_play_event_not_in_registry_returns_card_not_found_in_registry() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(3, 5, 0, vec![card_id.clone()]);

        let result = apply_action(&state, &registry(), PlayerAction::PlayEvent { card_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::CardNotFoundInRegistry(card_id)));
    }

    #[test]
    fn runner_play_event_with_insufficient_credits_for_registry_cost_returns_not_enough_credits() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(3, 0, 0, vec![card_id.clone()]);
        let mut registry = CardRegistry::new();
        registry.insert(test_card("sure_gamble", Side::Runner, CardType::Event, 5, None));

        let result = apply_action(&state, &registry, PlayerAction::PlayEvent { card_id });

        assert_eq!(
            result,
            Err(RulesError::NotEnoughCredits { side: Side::Runner, available: 0, requested: 5 })
        );
    }

    #[test]
    fn corp_turn_play_event_returns_not_your_turn() {
        let card_id = CardId("sure_gamble".to_string());
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::PlayEvent { card_id });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_play_event_with_card_not_in_grip_returns_card_not_in_hand() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(3, 5, 0, Vec::new());
        let result = apply_action(&state, &registry(), PlayerAction::PlayEvent { card_id: card_id.clone() });

        assert_eq!(
            result,
            Err(RulesError::CardNotInHand { side: Side::Runner, card: card_id })
        );
    }

    #[test]
    fn runner_play_event_with_zero_clicks_returns_not_enough_clicks() {
        let card_id = CardId("sure_gamble".to_string());
        let state = runner_state_with_grip(0, 5, 0, vec![card_id.clone()]);
        let result = apply_action(&state, &registry(), PlayerAction::PlayEvent { card_id });

        assert_eq!(
            result,
            Err(RulesError::NotEnoughClicks { side: Side::Runner, available: 0, requested: 1 })
        );
    }

    #[test]
    fn corp_play_operation_fires_on_play_trigger_and_moves_card_to_archives() {
        let card_id = CardId("hedge_fund".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, vec![card_id.clone()], Vec::new());
        let mut registry = CardRegistry::new();
        let mut card = test_card("hedge_fund", Side::Corp, CardType::Operation, 5, None);
        card.triggers = vec![TriggeredEffect {
            trigger: Trigger::OnPlay,
            effects: vec![Effect::GainCredits(Side::Corp, 9)],
            requirement: None,
        }];
        registry.insert(card);

        let (next, events) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: card_id.clone() })
                .expect("action should succeed");

        assert_eq!(next.corp.resources.clicks, Clicks(2));
        // Paid 5 to play, then the OnPlay trigger grants 9 back — net +4.
        assert_eq!(next.corp.resources.credits, Credits(9));
        assert!(next.corp.hq.is_empty());
        // A played Operation resolves in the open.
        assert_eq!(next.corp.archives, vec![ArchivedCard::faceup(card_id.clone())]);
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Corp },
                GameEvent::CreditsSpent { side: Side::Corp, amount: 5 },
                GameEvent::OperationPlayed { side: Side::Corp, card: card_id.clone() },
                GameEvent::CreditsGained { side: Side::Corp, amount: 9 },
            ]
        );

        // Original state is untouched.
        assert_eq!(state.corp.hq, vec![card_id]);
        assert!(state.corp.archives.is_empty());
    }

    #[test]
    fn corp_play_operation_not_in_registry_returns_card_not_found_in_registry() {
        let card_id = CardId("hedge_fund".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, vec![card_id.clone()], Vec::new());

        let result = apply_action(&state, &registry(), PlayerAction::PlayOperation { card_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::CardNotFoundInRegistry(card_id)));
    }

    #[test]
    fn corp_play_operation_with_insufficient_credits_for_registry_cost_returns_not_enough_credits() {
        let card_id = CardId("hedge_fund".to_string());
        let state = corp_state_with_hq_and_installed(3, 0, vec![card_id.clone()], Vec::new());
        let mut registry = CardRegistry::new();
        registry.insert(test_card("hedge_fund", Side::Corp, CardType::Operation, 5, None));

        let result = apply_action(&state, &registry, PlayerAction::PlayOperation { card_id });

        assert_eq!(
            result,
            Err(RulesError::NotEnoughCredits { side: Side::Corp, available: 0, requested: 5 })
        );

        // Whole-action atomicity: the failed pay_cost never lands, so
        // nothing about `next` is ever returned/observed here.
    }

    #[test]
    fn corp_play_operation_with_zero_clicks_returns_not_enough_clicks() {
        let card_id = CardId("hedge_fund".to_string());
        let state = corp_state_with_hq_and_installed(0, 5, vec![card_id.clone()], Vec::new());

        let result = apply_action(&state, &registry(), PlayerAction::PlayOperation { card_id });

        assert_eq!(
            result,
            Err(RulesError::NotEnoughClicks { side: Side::Corp, available: 0, requested: 1 })
        );
    }

    #[test]
    fn corp_play_operation_with_card_not_in_hq_returns_card_not_in_hand() {
        let card_id = CardId("hedge_fund".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), Vec::new());

        let result = apply_action(&state, &registry(), PlayerAction::PlayOperation { card_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::CardNotInHand { side: Side::Corp, card: card_id }));
    }

    #[test]
    fn corp_play_operation_with_non_operation_card_returns_card_not_operation() {
        let ice_id = CardId("ice_wall".to_string());
        let agenda_id = CardId("priority_requisition".to_string());
        let state =
            corp_state_with_hq_and_installed(3, 5, vec![ice_id.clone(), agenda_id.clone()], Vec::new());
        let mut registry = CardRegistry::new();
        registry.insert(test_card("ice_wall", Side::Corp, CardType::Ice(IceType::Barrier), 1, None));
        registry.insert(test_card("priority_requisition", Side::Corp, CardType::Agenda, 0, Some(5)));

        assert_eq!(
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: ice_id.clone() }),
            Err(RulesError::CardNotOperation { card: ice_id })
        );
        assert_eq!(
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: agenda_id.clone() }),
            Err(RulesError::CardNotOperation { card: agenda_id })
        );
    }

    #[test]
    fn runner_turn_play_operation_returns_not_your_turn() {
        let card_id = CardId("hedge_fund".to_string());
        let state = runner_state(3, 5, 3);
        let result = apply_action(&state, &registry(), PlayerAction::PlayOperation { card_id });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Corp),
                actual: GamePhase::Action(Side::Runner),
            })
        );
    }

    #[test]
    fn ordinary_actions_are_blocked_while_a_trace_is_active() {
        let mut state = corp_state(3, 5);
        state.active_trace = Some(crate::rules::state::TraceState {
            initiating_card: None,
            base_strength: 2,
            corp_bid: None,
            effect_on_success: Effect::GiveTags(1),
            resume: crate::rules::state::TraceResume::None,
        });

        let result = apply_action(&state, &registry(), PlayerAction::GainCreditClick { side: Side::Corp });

        assert_eq!(result, Err(RulesError::ActionBlockedByActiveTrace { awaiting: Side::Corp }));
    }

    #[test]
    fn trace_as_operation_end_to_end() {
        let card_id = CardId("sea_source".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, vec![card_id.clone()], Vec::new());
        let mut registry = CardRegistry::new();
        let mut card = test_card("sea_source", Side::Corp, CardType::Operation, 0, None);
        card.triggers = vec![TriggeredEffect {
            trigger: Trigger::OnPlay,
            effects: vec![Effect::Trace { base: 2, on_success: Box::new(Effect::GiveTags(1)) }],
            requirement: None,
        }];
        registry.insert(card);

        let (after_play, play_events) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: card_id.clone() })
                .expect("playing the operation should succeed");
        assert!(after_play.active_trace.is_some(), "trace should be parked awaiting the Corp's bid");
        assert_eq!(
            play_events,
            vec![
                GameEvent::ClickSpent { side: Side::Corp },
                GameEvent::CreditsSpent { side: Side::Corp, amount: 0 },
                GameEvent::OperationPlayed { side: Side::Corp, card: card_id.clone() },
                GameEvent::TraceInitiated { base: 2, initiating_card: Some(card_id) },
            ]
        );

        // Only trace-bid actions are legal while the trace is pending.
        assert_eq!(
            apply_action(&after_play, &registry, PlayerAction::GainCreditClick { side: Side::Corp }),
            Err(RulesError::ActionBlockedByActiveTrace { awaiting: Side::Corp })
        );

        let (after_corp_bid, _) = apply_action(&after_play, &registry, PlayerAction::SubmitCorpTraceBid { amount: 0 })
            .expect("corp bid should succeed");
        let (after_runner_bid, bid_events) =
            apply_action(&after_corp_bid, &registry, PlayerAction::SubmitRunnerTraceBid { amount: 0 })
                .expect("runner bid should succeed");

        assert!(after_runner_bid.active_trace.is_none());
        assert_eq!(after_runner_bid.runner.tags, 1, "runner underbid a strength-2 trace with 0 link/bid");
        assert_eq!(
            bid_events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 0 },
                GameEvent::TraceRunnerBidSubmitted { runner_bid: 0, total_strength: 0 },
                GameEvent::TraceSuccessful { corp_total: 2, runner_total: 0 },
                GameEvent::TagsGiven { side: Side::Runner, amount: 1 },
            ]
        );
    }

    #[test]
    fn runner_install_hardware_moves_card_from_grip_to_rig_and_spends_click() {
        let card_id = CardId("clone_chip".to_string());
        let state = runner_state_with_grip(3, 5, 0, vec![card_id.clone()]);
        let mut reg = registry();
        reg.insert(test_card("clone_chip", Side::Runner, CardType::Hardware, 0, None));
        let (next, events) = apply_action(
            &state,
            &reg,
            PlayerAction::InstallHardware { card_id: card_id.clone() },
        )
        .expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(2));
        assert!(next.runner.grip.is_empty());
        assert_eq!(next.runner.rig, vec![installed_runner_card("clone_chip", 0)]);
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Runner },
                GameEvent::CreditsSpent { side: Side::Runner, amount: 0 },
                GameEvent::HardwareInstalled { side: Side::Runner, card: card_id },
            ]
        );
    }

    #[test]
    fn corp_turn_install_hardware_returns_not_your_turn() {
        let card_id = CardId("clone_chip".to_string());
        let state = corp_state(3, 5);
        let result = apply_action(&state, &registry(), PlayerAction::InstallHardware { card_id });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_install_hardware_with_card_not_in_grip_returns_card_not_in_hand() {
        let card_id = CardId("clone_chip".to_string());
        let state = runner_state_with_grip(3, 5, 0, Vec::new());
        let result = apply_action(&state, &registry(), PlayerAction::InstallHardware { card_id: card_id.clone() });

        assert_eq!(
            result,
            Err(RulesError::CardNotInHand { side: Side::Runner, card: card_id })
        );
    }

    #[test]
    fn runner_install_program_moves_card_and_reserves_memory() {
        let card_id = CardId("gordian_blade".to_string());
        let state = runner_state_with_grip(3, 5, 4, vec![card_id.clone()]);
        let mut reg = registry();
        reg.insert(test_card("gordian_blade", Side::Runner, CardType::Program, 0, None));
        let (next, events) = apply_action(
            &state,
            &reg,
            PlayerAction::InstallProgram { card_id: card_id.clone(), memory_cost: 3 },
        )
        .expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(2));
        assert!(next.runner.grip.is_empty());
        assert_eq!(next.runner.rig, vec![installed_runner_card("gordian_blade", 0)]);
        assert_eq!(next.runner.memory_units, crate::rules::state::MemoryUnits(1));
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Runner },
                GameEvent::CreditsSpent { side: Side::Runner, amount: 0 },
                GameEvent::ProgramInstalled {
                    side: Side::Runner,
                    card: card_id,
                    memory_cost: 3,
                },
            ]
        );
    }

    #[test]
    fn corp_turn_install_program_returns_not_your_turn() {
        let card_id = CardId("gordian_blade".to_string());
        let state = corp_state(3, 5);
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallProgram { card_id, memory_cost: 3 },
        );

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_install_program_with_card_not_in_grip_returns_card_not_in_hand() {
        let card_id = CardId("gordian_blade".to_string());
        let state = runner_state_with_grip(3, 5, 4, Vec::new());
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallProgram { card_id: card_id.clone(), memory_cost: 3 },
        );

        assert_eq!(
            result,
            Err(RulesError::CardNotInHand { side: Side::Runner, card: card_id })
        );
    }

    #[test]
    fn runner_install_program_with_insufficient_memory_returns_insufficient_memory() {
        let card_id = CardId("gordian_blade".to_string());
        let state = runner_state_with_grip(3, 5, 2, vec![card_id.clone()]);
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::InstallProgram { card_id: card_id.clone(), memory_cost: 3 },
        );

        assert_eq!(
            result,
            Err(RulesError::InsufficientMemory { available: 2, requested: 3 })
        );

        // Original state is untouched: the card is still in the grip.
        assert_eq!(state.runner.grip, vec![card_id]);
    }

    #[test]
    fn install_program_seeds_base_strength_from_registry() {
        let card_id = CardId("corroder".to_string());
        let state = runner_state_with_grip(3, 5, 4, vec![card_id.clone()]);
        let mut reg = registry();
        let mut corroder = test_card("corroder", Side::Runner, CardType::Program, 0, None);
        corroder.strength = Some(2);
        reg.insert(corroder);

        let (next, _events) = apply_action(
            &state,
            &reg,
            PlayerAction::InstallProgram { card_id: card_id.clone(), memory_cost: 1 },
        )
        .expect("action should succeed");

        assert_eq!(next.runner.rig, vec![installed_runner_card("corroder", 2)]);
    }

    #[test]
    fn install_program_with_matching_registry_memory_cost_succeeds() {
        let card_id = CardId("corroder".to_string());
        let state = runner_state_with_grip(3, 5, 4, vec![card_id.clone()]);
        let mut reg = registry();
        let mut corroder = test_card("corroder", Side::Runner, CardType::Program, 0, None);
        corroder.memory_cost = Some(1);
        reg.insert(corroder);

        let (next, _events) = apply_action(
            &state,
            &reg,
            PlayerAction::InstallProgram { card_id: card_id.clone(), memory_cost: 1 },
        )
        .expect("action should succeed");

        assert_eq!(next.runner.memory_units, crate::rules::state::MemoryUnits(3));
    }

    #[test]
    fn install_program_with_mismatched_memory_cost_returns_mismatched_memory_cost() {
        let card_id = CardId("corroder".to_string());
        let state = runner_state_with_grip(3, 5, 4, vec![card_id.clone()]);
        let mut reg = registry();
        let mut corroder = test_card("corroder", Side::Runner, CardType::Program, 0, None);
        corroder.memory_cost = Some(1);
        reg.insert(corroder);

        let result = apply_action(
            &state,
            &reg,
            PlayerAction::InstallProgram { card_id: card_id.clone(), memory_cost: 2 },
        );

        assert_eq!(result, Err(RulesError::MismatchedMemoryCost { expected: 1, actual: 2 }));
    }

    #[test]
    fn install_hardware_seeds_zero_base_strength_for_non_strength_card() {
        let card_id = CardId("clone_chip".to_string());
        let state = runner_state_with_grip(3, 5, 0, vec![card_id.clone()]);
        let mut reg = registry();
        reg.insert(test_card("clone_chip", Side::Runner, CardType::Hardware, 0, None));

        let (next, _events) = apply_action(
            &state,
            &reg,
            PlayerAction::InstallHardware { card_id: card_id.clone() },
        )
        .expect("action should succeed");

        assert_eq!(next.runner.rig, vec![installed_runner_card("clone_chip", 0)]);
    }

    #[test]
    fn runner_break_subroutine_decrements_pending_on_current_ice() {
        let ice_id = CardId("ice_wall".to_string());
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 2, true)],
            jack_out_permitted: true,
            ..Default::default()
        });
        let (next, events) = apply_action(
            &state,
            &registry(),
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 0 },
        )
        .expect("action should succeed");

        // No click cost: breaking a subroutine isn't a click action.
        assert_eq!(next.runner.resources.clicks, Clicks(3));
        let ice = &next.active_run.as_ref().unwrap().ice[0];
        assert_eq!(ice.subroutines[0].status, SubroutineStatus::Broken);
        assert_eq!(ice.subroutines[1].status, SubroutineStatus::Pending);
        assert_eq!(
            events,
            vec![GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0 }]
        );
    }

    #[test]
    fn corp_turn_break_subroutine_returns_not_your_turn() {
        let ice_id = CardId("ice_wall".to_string());
        let state = corp_state(3, 5);
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 0 },
        );

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    #[test]
    fn runner_break_subroutine_with_no_active_run_returns_no_active_run() {
        let ice_id = CardId("ice_wall".to_string());
        let state = runner_state(3, 0, 0);
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 0 },
        );

        assert_eq!(result, Err(RulesError::NoActiveRun));
    }

    #[test]
    fn runner_break_subroutine_with_index_out_of_range_returns_invalid_subroutine_index() {
        let ice_id = CardId("ice_wall".to_string());
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            jack_out_permitted: true,
            ..Default::default()
        });
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 1 },
        );

        assert_eq!(result, Err(RulesError::InvalidSubroutineIndex(1)));
    }

    #[test]
    fn runner_break_subroutine_outside_encounter_ice_returns_not_in_encounter() {
        let ice_id = CardId("ice_wall".to_string());
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            jack_out_permitted: true,
            ..Default::default()
        });
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 0 },
        );

        assert_eq!(result, Err(RulesError::NotInEncounter));
    }

    #[test]
    fn runner_break_subroutine_with_mismatched_ice_id_returns_error() {
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            jack_out_permitted: true,
            ..Default::default()
        });
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::BreakSubroutine { ice_id: CardId("some_other_ice".to_string()), subroutine_index: 0 },
        );

        assert_eq!(
            result,
            Err(RulesError::MismatchedIceId {
                expected: CardId("ice_wall".to_string()),
                actual: CardId("some_other_ice".to_string()),
            })
        );
    }

    #[test]
    fn actions_issued_during_game_over_fail_with_wrong_phase() {
        let mut state = corp_state(3, 5);
        state.phase = GamePhase::GameOver(Side::Runner);

        let result = apply_action(&state, &registry(), PlayerAction::GainCreditClick { side: Side::Corp });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Corp),
                actual: GamePhase::GameOver(Side::Runner),
            })
        );
    }

    #[test]
    fn end_turn_issued_during_game_over_fails_with_not_in_action_phase() {
        let mut state = corp_state(3, 5);
        state.phase = GamePhase::GameOver(Side::Runner);

        let result = apply_action(&state, &registry(), PlayerAction::EndTurn);

        assert_eq!(
            result,
            Err(RulesError::NotInActionPhase { actual: GamePhase::GameOver(Side::Runner) })
        );
    }

    /// A rig entry seeded with `base_strength` and no active buffs — used
    /// by `activate_ability`/install tests that need a card already in the
    /// Runner's rig.
    fn installed_runner_card(card_id: &str, base_strength: i32) -> InstalledRunnerCard {
        InstalledRunnerCard {
            card: CardId(card_id.to_string()),
            base_strength,
            ..Default::default()
        }
    }

    /// A minimal `CardDefinition` with the given install/play `cost` and
    /// `advancement_requirement`, no abilities — used by the
    /// `InstallCard`/`PlayEvent`/`AdvanceCard` cost/advancement tests, which
    /// only care about those two fields.
    fn test_card(
        card_id: &str,
        side: Side,
        card_type: CardType,
        cost: u32,
        advancement_requirement: Option<u32>,
    ) -> CardDefinition {
        CardDefinition {
            id: CardId(card_id.to_string()),
            title: card_id.to_string(),
            side,
            card_type,
            cost,
            advancement_requirement,
            is_playable: true,
            ..Default::default()
        }
    }

    /// A minimal `CardDefinition` whose only `abilities` entry is the given
    /// `trigger`/`cost`/`effect` — everything about the card besides its id,
    /// side, and that one ability is irrelevant to `activate_ability`'s
    /// logic, so it's held to placeholder values.
    fn test_card_with_ability(
        card_id: &str,
        side: Side,
        trigger: Trigger,
        cost: Option<Cost>,
        effect: Effect,
    ) -> CardDefinition {
        CardDefinition {
            id: CardId(card_id.to_string()),
            title: card_id.to_string(),
            side,
            card_type: CardType::Program,
            abilities: vec![AbilityDef { trigger, cost, requirement: None, effect, cost_discount_if: None }],
            is_playable: true,
            ..Default::default()
        }
    }

    #[test]
    fn runner_activate_ability_pumps_icebreaker_and_deducts_credits() {
        let card_id = CardId("gordian_blade".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![installed_runner_card("gordian_blade", 0)];
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 0, true)],
            jack_out_permitted: true,
            ..Default::default()
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "gordian_blade",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::ModifyStrength(1),
        ));

        let (next, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
        )
        .expect("action should succeed");

        assert_eq!(next.runner.resources.credits, Credits(4));
        assert_eq!(next.active_run.unwrap().ice[0].current_strength, 1);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 1 },
                GameEvent::AbilityActivated { side: Side::Runner, card_id, ability_index: 0 },
                GameEvent::IceStrengthModified {
                    card_id: CardId("ice_wall".to_string()),
                    new_strength: 1,
                    delta: 1,
                },
            ]
        );
    }

    #[test]
    fn runner_activate_ability_boosts_own_rig_card_strength_and_deducts_credits() {
        let card_id = CardId("corroder".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![installed_runner_card("corroder", 2)];
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 0, true)],
            jack_out_permitted: true,
            ..Default::default()
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "corroder",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter },
        ));

        let (next, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
        )
        .expect("action should succeed");

        assert_eq!(next.runner.resources.credits, Credits(4));
        assert_eq!(next.runner.rig[0].effective_strength(), 3);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 1 },
                GameEvent::AbilityActivated { side: Side::Runner, card_id: card_id.clone(), ability_index: 0 },
                GameEvent::StrengthBoosted {
                    card_id,
                    new_strength: 3,
                    delta: 1,
                    duration: BoostDuration::Encounter,
                },
            ]
        );
    }

    #[test]
    fn runner_activate_ability_cost_trash_self_trashes_card_and_still_applies_its_effect() {
        let card_id = CardId("self_modifying_code".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(0);
        state.runner.rig = vec![installed_runner_card("self_modifying_code", 0)];

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "self_modifying_code",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::TrashSelf),
            Effect::GainCredits(Side::Runner, 5),
        ));

        let (next, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
        )
        .expect("action should succeed");

        assert!(next.runner.rig.is_empty());
        assert_eq!(next.runner.heap, vec![card_id.clone()]);
        assert_eq!(next.runner.resources.credits, Credits(5));
        assert_eq!(
            events,
            vec![
                GameEvent::CardTrashed { side: Side::Runner, card: card_id.clone() },
                GameEvent::AbilityActivated { side: Side::Runner, card_id, ability_index: 0 },
                GameEvent::CreditsGained { side: Side::Runner, amount: 5 },
            ]
        );
    }

    #[test]
    fn runner_activate_ability_breaks_subroutine_via_break_subroutines_when_strong_enough() {
        let card_id = CardId("corroder".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![installed_runner_card("corroder", 2)];
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 2, 1, true)],
            jack_out_permitted: true,
            ..Default::default()
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "corroder",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::BreakSubroutines {
                count: SubroutineBreakCount::Fixed(1),
                restrict_to: Some(IceType::Barrier),
            },
        ));

        let (next, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
        )
        .expect("action should succeed");

        assert_eq!(next.runner.resources.credits, Credits(4));
        assert_eq!(next.active_run.unwrap().ice[0].subroutines[0].status, SubroutineStatus::Broken);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 1 },
                GameEvent::AbilityActivated { side: Side::Runner, card_id, ability_index: 0 },
                GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0 },
            ]
        );
    }

    #[test]
    fn runner_activate_ability_break_subroutines_fails_and_rolls_back_cost_when_too_weak() {
        let card_id = CardId("corroder".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![installed_runner_card("corroder", 1)];
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 3, 1, true)],
            jack_out_permitted: true,
            ..Default::default()
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "corroder",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::BreakSubroutines {
                count: SubroutineBreakCount::Fixed(1),
                restrict_to: Some(IceType::Barrier),
            },
        ));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
        );

        assert_eq!(
            result,
            Err(RulesError::BreakerStrengthTooLow {
                breaker: card_id,
                breaker_strength: 1,
                ice: CardId("ice_wall".to_string()),
                ice_strength: 3,
            })
        );
        // Whole-action atomicity: `activate_ability` operates on a cloned
        // `next` and only returns it on success, so the credit spent while
        // paying the cost is rolled back along with everything else when
        // the effect itself errors afterward.
        assert_eq!(state.runner.resources.credits, Credits(5));
    }

    #[test]
    fn runner_activate_ability_corroder_breaks_subroutine_on_matching_barrier_ice() {
        let card_id = CardId("corroder".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![installed_runner_card("corroder", 2)];
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice_of_type("ice_wall", 2, 1, true, IceType::Barrier)],
            jack_out_permitted: true,
            ..Default::default()
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "corroder",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::BreakSubroutines {
                count: SubroutineBreakCount::Fixed(1),
                restrict_to: Some(IceType::Barrier),
            },
        ));

        let (next, _events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
        )
        .expect("Corroder should break a subroutine on Barrier ICE");

        assert_eq!(next.active_run.unwrap().ice[0].subroutines[0].status, SubroutineStatus::Broken);
    }

    #[test]
    fn runner_activate_ability_corroder_fails_and_rolls_back_cost_against_wrong_subtype() {
        for wrong_type in [IceType::CodeGate, IceType::Sentry] {
            let card_id = CardId("corroder".to_string());
            let mut state = runner_state(3, 0, 0);
            state.runner.resources.credits = Credits(5);
            state.runner.rig = vec![installed_runner_card("corroder", 2)];
            state.active_run = Some(RunState {
                phase: RunPhase::EncounterIce,
                ice: vec![test_ice_of_type("some_ice", 2, 1, true, wrong_type)],
                jack_out_permitted: true,
                ..Default::default()
            });

            let mut registry = CardRegistry::new();
            registry.insert(test_card_with_ability(
                "corroder",
                Side::Runner,
                Trigger::Paid,
                Some(Cost::Credits(1)),
                Effect::BreakSubroutines {
                    count: SubroutineBreakCount::Fixed(1),
                    restrict_to: Some(IceType::Barrier),
                },
            ));

            let result = apply_action(
                &state,
                &registry,
                PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
            );

            assert_eq!(
                result,
                Err(RulesError::InvalidBreakerSubtype {
                    breaker: card_id,
                    ice: CardId("some_ice".to_string()),
                    expected: IceType::Barrier,
                })
            );
            // Same whole-action atomicity as the strength-too-low case.
            assert_eq!(state.runner.resources.credits, Credits(5));
        }
    }

    #[test]
    fn runner_activate_ability_universal_breaker_breaks_subroutines_on_any_ice_type() {
        for ice_type in [IceType::Barrier, IceType::CodeGate, IceType::Sentry] {
            let card_id = CardId("mimic".to_string());
            let mut state = runner_state(3, 0, 0);
            state.runner.resources.credits = Credits(5);
            state.runner.rig = vec![installed_runner_card("mimic", 2)];
            state.active_run = Some(RunState {
                phase: RunPhase::EncounterIce,
                ice: vec![test_ice_of_type("some_ice", 2, 1, true, ice_type)],
                jack_out_permitted: true,
                ..Default::default()
            });

            let mut registry = CardRegistry::new();
            registry.insert(test_card_with_ability(
                "mimic",
                Side::Runner,
                Trigger::Paid,
                Some(Cost::Credits(1)),
                Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None },
            ));

            let (next, _events) = apply_action(
                &state,
                &registry,
                PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
            )
            .expect("a universal breaker should break subroutines regardless of ICE subtype");

            assert_eq!(next.active_run.unwrap().ice[0].subroutines[0].status, SubroutineStatus::Broken);
        }
    }

    #[test]
    fn activate_ability_during_window_by_priority_holder_succeeds_and_resets_passes() {
        let card_id = CardId("gordian_blade".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![installed_runner_card("gordian_blade", 0)];
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 0, true)],
            ..Default::default()
        });
        // A window is open with one pass already in (Corp holds priority
        // after the Runner's first pass); the Runner activates instead.
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 1,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
            checkpoint: WindowCheckpoint::Run,
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "gordian_blade",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::ModifyStrength(1),
        ));

        let (next, _events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
        )
        .expect("action should succeed");

        let window = next.paid_ability_window.expect("window should stay open");
        assert_eq!(window.consecutive_passes, 0, "firing a paid ability resets the pass counter");
        assert_eq!(window.active_priority, Side::Corp, "priority toggles to the other side");
    }

    #[test]
    fn activate_ability_during_window_by_non_priority_side_returns_not_your_priority() {
        let card_id = CardId("gordian_blade".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![installed_runner_card("gordian_blade", 0)];
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 0, true)],
            ..Default::default()
        });
        // Corp currently holds priority — the Runner tries to act anyway.
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Corp,
            consecutive_passes: 1,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
            checkpoint: WindowCheckpoint::Run,
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "gordian_blade",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::ModifyStrength(1),
        ));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id, ability_index: 0 },
        );

        assert_eq!(
            result,
            Err(RulesError::NotYourPriority { expected: Side::Corp, actual: Side::Runner })
        );
    }

    #[test]
    fn gain_credit_click_is_blocked_while_a_paid_ability_window_is_open() {
        let mut state = corp_state(3, 5);
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            return_phase: Box::new(GamePhase::Action(Side::Corp)),
            checkpoint: WindowCheckpoint::Run,
        });

        let result = apply_action(&state, &registry(), PlayerAction::GainCreditClick { side: Side::Corp });

        assert_eq!(
            result,
            Err(RulesError::BlockedByPaidAbilityWindow { priority: Side::Runner })
        );
    }

    #[test]
    fn rez_ice_by_non_priority_side_during_window_still_succeeds_and_resets_passes() {
        let installed = vec![InstalledCard {
            card: CardId("ice_wall".to_string()),
            slot: InstallSlot::Ice,
            ..Default::default()
        }];
        let mut state = runner_state(3, 0, 0);
        state.corp.installed = installed;
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 0, false)],
            ..Default::default()
        });
        // It's the Runner's priority, but Rez is priority-independent —
        // the Corp can still act, and doing so should give the Runner a
        // fresh chance to respond (reset passes, toggle priority to Runner).
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 1,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
            checkpoint: WindowCheckpoint::Run,
        });
        let registry = CardRegistry::from_cards(vec![test_card("ice_wall", Side::Corp, CardType::Ice(IceType::Barrier), 0, None)]);

        let (next, _events) =
            apply_action(&state, &registry, PlayerAction::RezIce { ice_id: CardId("ice_wall".to_string()) })
                .expect("action should succeed");

        let window = next.paid_ability_window.expect("window should stay open");
        assert_eq!(window.consecutive_passes, 0);
        assert_eq!(window.active_priority, Side::Runner);
        assert!(next.corp.installed[0].rezzed);
        assert!(next.active_run.unwrap().ice[0].rezzed);
    }

    #[test]
    fn jack_out_during_an_open_window_clears_the_window() {
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 0, true), test_ice("enigma", 0, 0, true)],
            position: 1,
            jack_out_permitted: true,
            ..Default::default()
        });
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
            checkpoint: WindowCheckpoint::Run,
        });

        let (next, _events) =
            apply_action(&state, &registry(), PlayerAction::JackOut).expect("action should succeed");

        assert_eq!(next.active_run, None);
        assert_eq!(
            next.paid_ability_window, None,
            "a stale window must not survive the run it belonged to"
        );
    }

    #[test]
    fn corp_activate_ability_on_unrezzed_asset_returns_card_not_active() {
        let card_id = CardId("pad_campaign".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            server: ServerId::Remote(0),
            ..Default::default()
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "pad_campaign",
            Side::Corp,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::GainCredits(Side::Corp, 1),
        ));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
        );

        assert_eq!(result, Err(RulesError::CardNotActive { side: Side::Corp, card: card_id }));
    }

    #[test]
    fn runner_activate_ability_with_insufficient_credits_propagates_pay_cost_error() {
        let card_id = CardId("gordian_blade".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(0);
        state.runner.rig = vec![installed_runner_card("gordian_blade", 0)];
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 0, true)],
            jack_out_permitted: true,
            ..Default::default()
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "gordian_blade",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::ModifyStrength(1),
        ));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id, ability_index: 0 },
        );

        assert_eq!(
            result,
            Err(RulesError::NotEnoughCredits { side: Side::Runner, available: 0, requested: 1 })
        );
    }

    #[test]
    fn runner_activate_ability_with_invalid_index_returns_invalid_ability_index() {
        let card_id = CardId("gordian_blade".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.rig = vec![installed_runner_card("gordian_blade", 0)];

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "gordian_blade",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::ModifyStrength(1),
        ));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id, ability_index: 1 },
        );

        assert_eq!(result, Err(RulesError::InvalidAbilityIndex(1)));
    }

    #[test]
    fn runner_activate_ability_on_non_paid_trigger_returns_not_manually_activatable() {
        let card_id = CardId("gordian_blade".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.rig = vec![installed_runner_card("gordian_blade", 0)];

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "gordian_blade",
            Side::Runner,
            Trigger::OnEncounter,
            Some(Cost::Credits(1)),
            Effect::ModifyStrength(1),
        ));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id, ability_index: 0 },
        );

        assert_eq!(result, Err(RulesError::AbilityNotManuallyActivatable(0)));
    }

    #[test]
    fn corp_advance_card_adds_advancement_token_and_charges_click_and_credit() {
        let card_id = CardId("priority_requisition".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            server: ServerId::Remote(0),
            advancement_tokens: 1,
            ..Default::default()
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        let mut registry = CardRegistry::new();
        registry.insert(test_card("priority_requisition", Side::Corp, CardType::Agenda, 0, Some(5)));

        let (next, events) = apply_action(
            &state,
            &registry,
            PlayerAction::AdvanceCard { card_id: card_id.clone() },
        )
        .expect("action should succeed");

        assert_eq!(next.corp.resources.clicks, Clicks(2));
        assert_eq!(next.corp.resources.credits, Credits(4));
        assert_eq!(next.corp.installed[0].advancement_tokens, 2);
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Corp },
                GameEvent::CreditsSpent { side: Side::Corp, amount: 1 },
                GameEvent::CardAdvanced { card: card_id, advancement_tokens: 2 },
            ]
        );
    }

    #[test]
    fn corp_advance_card_on_non_agenda_returns_card_not_advanceable() {
        let card_id = CardId("pad_campaign".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            server: ServerId::Remote(0),
            ..Default::default()
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        let mut registry = CardRegistry::new();
        registry.insert(test_card("pad_campaign", Side::Corp, CardType::Asset, 2, None));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::AdvanceCard { card_id: card_id.clone() },
        );

        assert_eq!(result, Err(RulesError::CardNotAdvanceable { card: card_id }));
    }

    #[test]
    fn corp_advance_card_on_uninstalled_card_returns_card_not_installed() {
        let card_id = CardId("priority_requisition".to_string());
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), Vec::new());
        let mut registry = CardRegistry::new();
        registry.insert(test_card("priority_requisition", Side::Corp, CardType::Agenda, 0, Some(5)));

        let result = apply_action(
            &state,
            &registry,
            PlayerAction::AdvanceCard { card_id: card_id.clone() },
        );

        assert_eq!(result, Err(RulesError::CardNotInstalled { card: card_id }));
    }

    #[test]
    fn runner_turn_advance_card_returns_not_your_turn() {
        let card_id = CardId("priority_requisition".to_string());
        let state = runner_state(3, 0, 0);

        let result = apply_action(&state, &registry(), PlayerAction::AdvanceCard { card_id });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Corp),
                actual: GamePhase::Action(Side::Runner),
            })
        );
    }

    #[test]
    fn corp_advance_card_with_insufficient_credits_returns_not_enough_credits() {
        let card_id = CardId("priority_requisition".to_string());
        let installed = vec![InstalledCard {
            card: card_id.clone(),
            server: ServerId::Remote(0),
            ..Default::default()
        }];
        let state = corp_state_with_hq_and_installed(3, 0, Vec::new(), installed);
        let mut registry = CardRegistry::new();
        registry.insert(test_card("priority_requisition", Side::Corp, CardType::Agenda, 0, Some(5)));

        let result = apply_action(&state, &registry, PlayerAction::AdvanceCard { card_id });

        assert_eq!(
            result,
            Err(RulesError::NotEnoughCredits { side: Side::Corp, available: 0, requested: 1 })
        );
    }

    #[test]
    fn runner_remove_tag_spends_click_and_credits_and_removes_one_tag() {
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(5);
        state.runner.tags = 2;

        let (next, events) = apply_action(&state, &registry(), PlayerAction::RemoveTag).expect("action should succeed");

        assert_eq!(next.runner.resources.clicks, Clicks(2));
        assert_eq!(next.runner.resources.credits, Credits(3));
        assert_eq!(next.runner.tags, 1);
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Runner },
                GameEvent::CreditsSpent { side: Side::Runner, amount: 2 },
                GameEvent::TagRemoved { side: Side::Runner },
            ]
        );
    }

    #[test]
    fn runner_remove_tag_with_zero_tags_returns_runner_not_tagged() {
        let state = runner_state(3, 0, 0);

        let result = apply_action(&state, &registry(), PlayerAction::RemoveTag);

        assert_eq!(result, Err(RulesError::RunnerNotTagged));
    }

    #[test]
    fn corp_turn_remove_tag_returns_wrong_phase() {
        let mut state = corp_state(3, 5);
        state.runner.tags = 1;

        let result = apply_action(&state, &registry(), PlayerAction::RemoveTag);

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Runner),
                actual: GamePhase::Action(Side::Corp),
            })
        );
    }

    /// A card that holds `counter_kind` counters — the only field
    /// `purge_virus_counters` reads to decide what it targets.
    fn card_with_counter_kind(card_id: &str, side: Side, counter_kind: CounterKind) -> CardDefinition {
        CardDefinition {
            counter_kind: Some(counter_kind),
            ..test_card(card_id, side, CardType::Program, 0, None)
        }
    }

    fn rig_card_with_counters(card_id: &str, counters: u32) -> InstalledRunnerCard {
        InstalledRunnerCard { counters, ..installed_runner_card(card_id, 0) }
    }

    /// Registry + state with two virus programs (counters loaded) and one
    /// credit-counter card, so every purge test can assert both that
    /// viruses are wiped and that non-viruses are untouched.
    fn purge_fixture() -> (GameState, CardRegistry) {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_counter_kind("botulus", Side::Runner, CounterKind::Virus));
        registry.insert(card_with_counter_kind("leech", Side::Runner, CounterKind::Virus));
        registry.insert(card_with_counter_kind("nico_campaign", Side::Runner, CounterKind::Credit));

        let mut state = corp_state(3, 5);
        state.runner.rig = vec![
            rig_card_with_counters("botulus", 3),
            rig_card_with_counters("leech", 2),
            rig_card_with_counters("nico_campaign", 4),
        ];
        (state, registry)
    }

    fn counters_of(state: &GameState, card_id: &str) -> u32 {
        state.runner.rig.iter().find(|c| c.card.0 == card_id).expect("card should be in the rig").counters
    }

    #[test]
    fn corp_purge_zeroes_every_virus_card_at_once_and_spends_three_clicks() {
        let (state, registry) = purge_fixture();

        let (next, events) =
            apply_action(&state, &registry, PlayerAction::PurgeVirusCounters).expect("action should succeed");

        assert_eq!(counters_of(&next, "botulus"), 0);
        assert_eq!(counters_of(&next, "leech"), 0);
        assert_eq!(next.corp.resources.clicks, Clicks(0), "purge costs the Corp's whole turn");
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Corp },
                GameEvent::ClickSpent { side: Side::Corp },
                GameEvent::ClickSpent { side: Side::Corp },
                GameEvent::VirusCountersPurged {
                    cards: vec![CardId("botulus".to_string()), CardId("leech".to_string())],
                },
            ]
        );
    }

    #[test]
    fn corp_purge_leaves_non_virus_counters_untouched() {
        let (state, registry) = purge_fixture();

        let (next, _events) =
            apply_action(&state, &registry, PlayerAction::PurgeVirusCounters).expect("action should succeed");

        assert_eq!(counters_of(&next, "nico_campaign"), 4, "a Credit-counter card is not a virus");
    }

    /// Purging an empty board is legal in real Netrunner — a pointless but
    /// permitted way to spend a turn. Deliberately not an error, unlike
    /// `RemoveTag`'s `RunnerNotTagged`; see `PlayerAction::
    /// PurgeVirusCounters`'s doc comment.
    #[test]
    fn corp_purge_with_nothing_to_purge_succeeds_and_still_costs_three_clicks() {
        let state = corp_state(3, 5);

        let (next, events) =
            apply_action(&state, &registry(), PlayerAction::PurgeVirusCounters).expect("an empty purge is still legal");

        assert_eq!(next.corp.resources.clicks, Clicks(0));
        assert_eq!(events.last(), Some(&GameEvent::VirusCountersPurged { cards: Vec::new() }));
    }

    #[test]
    fn corp_purge_without_three_clicks_returns_not_enough_clicks() {
        let (mut state, registry) = purge_fixture();
        state.corp.resources.clicks = Clicks(2);

        let result = apply_action(&state, &registry, PlayerAction::PurgeVirusCounters);

        assert_eq!(result, Err(RulesError::NotEnoughClicks { side: Side::Corp, available: 2, requested: 3 }));
        assert_eq!(counters_of(&state, "botulus"), 3, "a rejected purge must not have mutated anything");
    }

    #[test]
    fn runner_turn_purge_returns_wrong_phase() {
        let (mut state, registry) = purge_fixture();
        state.phase = GamePhase::Action(Side::Runner);

        let result = apply_action(&state, &registry, PlayerAction::PurgeVirusCounters);

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Corp),
                actual: GamePhase::Action(Side::Runner),
            })
        );
    }

    #[test]
    fn corp_purge_during_an_open_paid_ability_window_is_rejected() {
        let (mut state, registry) = purge_fixture();
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Corp,
            consecutive_passes: 0,
            checkpoint: WindowCheckpoint::Run,
            return_phase: Box::new(state.phase),
        });

        let result = apply_action(&state, &registry, PlayerAction::PurgeVirusCounters);

        assert!(result.is_err(), "purge is a basic click action, not a paid ability");
    }

    /// End-to-end proof that `apply_action`'s drain closes the loop: a
    /// trigger parks a choice, the rest of the dispatch is queued, and
    /// resolving that choice through a real `PlayerAction` fires the
    /// remainder — no separate "now drain" step from the caller.
    ///
    /// The dispatch-level halves of this live in `dispatcher::tests`; this
    /// one exists because the drain is wired at the `apply_action` choke
    /// point, which those can't reach.
    #[test]
    fn resolving_a_parked_choice_drains_the_triggers_it_deferred() {
        let mut registry = CardRegistry::new();
        registry.insert(CardDefinition {
            triggers: vec![TriggeredEffect {
                trigger: Trigger::OnTurnStart,
                effects: vec![Effect::PresentChoice {
                    chooser: Side::Corp,
                    options: vec![Effect::GainCredits(Side::Corp, 5), Effect::Sequence(Vec::new())],
                }],
                requirement: None,
            }],
            ..test_card("parks_a_choice", Side::Corp, CardType::Asset, 0, None)
        });
        registry.insert(CardDefinition {
            triggers: vec![TriggeredEffect {
                trigger: Trigger::OnTurnStart,
                effects: vec![Effect::GainCredits(Side::Corp, 1)],
                requirement: None,
            }],
            ..test_card("pad_campaign", Side::Corp, CardType::Asset, 0, None)
        });

        let mut state = corp_state(3, 0);
        let rezzed = |id: &str| InstalledCard {
            card: CardId(id.to_string()),
            rezzed: true,
            ..Default::default()
        };
        state.corp.installed = vec![rezzed("parks_a_choice"), rezzed("pad_campaign")];
        state.deferred_triggers = vec![crate::rules::state::DeferredTrigger {
            card: CardId("pad_campaign".to_string()),
            trigger: Trigger::OnTurnStart,
            target: None, event: None,
        }];
        state.pending_decision = Some(crate::rules::state::PendingDecision::ChooseEffect {
            chooser: Side::Corp,
            options: vec![Effect::GainCredits(Side::Corp, 5), Effect::Sequence(Vec::new())],
            source_card: Some(CardId("parks_a_choice".to_string())),
            resume: crate::rules::state::PendingChoiceResume::None,
        });

        // Take the do-nothing option, so the only credits gained come from
        // the deferred PAD Campaign trigger.
        let (next, _events) =
            apply_action(&state, &registry, PlayerAction::ResolvePendingChoice { option_index: 1 })
                .expect("resolving the parked choice should succeed");

        assert_eq!(next.corp.resources.credits, Credits(1), "the deferred trigger fired on resolution");
        assert!(next.deferred_triggers.is_empty(), "and the queue drained");
        assert!(next.pending_decision.is_none());
    }

    /// Picking an order resolves *both* triggers, in the order picked, and
    /// costs only one decision for two triggers — once one is left there is
    /// no order to choose, so it drains automatically.
    ///
    /// Runs through real `apply_action` calls, so it also exercises
    /// `current_actor` naming the chooser and the drain firing the
    /// remainder — the precedence invariant that has deadlocked this engine
    /// before.
    #[test]
    fn choosing_a_trigger_order_resolves_both_in_that_order_with_one_decision() {
        let mut registry = CardRegistry::new();
        let reactor = |id: &str, amount: u32| CardDefinition {
            triggers: vec![TriggeredEffect {
                trigger: Trigger::OnTurnStart,
                effects: vec![Effect::GainCredits(Side::Corp, amount)],
                requirement: None,
            }],
            ..test_card(id, Side::Corp, CardType::Asset, 0, None)
        };
        registry.insert(reactor("pad_campaign", 1));
        registry.insert(reactor("nico_campaign", 3));

        let mut state = corp_state(3, 0);
        let rezzed = |id: &str| InstalledCard {
            card: CardId(id.to_string()),
            rezzed: true,
            ..Default::default()
        };
        state.corp.installed = vec![rezzed("pad_campaign"), rezzed("nico_campaign")];
        state.pending_decision = Some(crate::rules::state::PendingDecision::ChooseTriggerOrder {
            chooser: Side::Corp,
            pending: vec![
                crate::rules::state::DeferredTrigger {
                    card: CardId("pad_campaign".to_string()),
                    trigger: Trigger::OnTurnStart,
                    target: None, event: None,
                },
                crate::rules::state::DeferredTrigger {
                    card: CardId("nico_campaign".to_string()),
                    trigger: Trigger::OnTurnStart,
                    target: None, event: None,
                },
            ],
            resume: crate::rules::state::PendingChoiceResume::None,
        });

        assert_eq!(
            crate::rules::current_actor(&state),
            Some(Side::Corp),
            "the chooser must be the one named to act"
        );

        // Pick the *second* card first — the point of the feature.
        let (next, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ChooseTriggerToResolve { card_id: CardId("nico_campaign".to_string()) },
        )
        .expect("choosing a pending trigger should succeed");

        assert!(next.pending_decision.is_none(), "one trigger left is no choice, so no second decision");
        assert!(next.deferred_triggers.is_empty(), "and it drained in the same action");
        assert_eq!(next.corp.resources.credits, Credits(4), "both fired: 3 then 1");

        let gains: Vec<&GameEvent> =
            events.iter().filter(|e| matches!(e, GameEvent::CreditsGained { .. })).collect();
        assert_eq!(
            gains,
            vec![
                &GameEvent::CreditsGained { side: Side::Corp, amount: 3 },
                &GameEvent::CreditsGained { side: Side::Corp, amount: 1 },
            ],
            "the chosen card resolved first, ahead of its install order"
        );
    }

    /// Corp state whose Runner opponent holds one usable paid ability
    /// (1 credit for 1 credit, no requirement — deliberately not an
    /// icebreaker ability, which `DuringEncounter` would gate out).
    fn corp_turn_with_runner_paid_ability(runner_credits: u32) -> (GameState, CardRegistry) {
        let mut registry = CardRegistry::new();
        registry.insert(CardDefinition {
            abilities: vec![AbilityDef {
                trigger: Trigger::Paid,
                cost: Some(Cost::Credits(1)),
                requirement: None,
                effect: Effect::GainCredits(Side::Runner, 1),
                cost_discount_if: None,
            }],
            ..test_card("pennyshaver", Side::Runner, CardType::Hardware, 0, None)
        });

        let mut state = corp_state(3, 5);
        state.runner.resources.credits = Credits(runner_credits);
        state.runner.rig = vec![installed_runner_card("pennyshaver", 0)];
        (state, registry)
    }

    #[test]
    fn a_click_action_opens_a_post_action_window_when_the_opponent_can_respond() {
        let (state, registry) = corp_turn_with_runner_paid_ability(5);

        let (next, _events) =
            apply_action(&state, &registry, PlayerAction::GainCreditClick { side: Side::Corp })
                .expect("gain credit should succeed");

        let window = next.paid_ability_window.as_ref().expect("a post-action window should be open");
        assert_eq!(window.checkpoint, WindowCheckpoint::PostAction { side: Side::Corp });
        assert_eq!(window.active_priority, Side::Corp, "active player holds priority first");
        assert_eq!(next.phase, GamePhase::Action(Side::Corp), "the window does not change the phase");
    }

    /// The cost guard, and the most important test here: with nothing for
    /// the opponent to do, no window opens and the action costs no extra
    /// `PassPriority`s. Without this, every basic action in every game
    /// would cost two.
    #[test]
    fn no_post_action_window_when_the_opponent_has_nothing_usable() {
        // Same board, but the Runner cannot afford the ability.
        let (state, registry) = corp_turn_with_runner_paid_ability(0);

        let (next, _events) =
            apply_action(&state, &registry, PlayerAction::GainCreditClick { side: Side::Corp })
                .expect("gain credit should succeed");

        assert!(next.paid_ability_window.is_none(), "an unaffordable ability is not a reason to stop play");
    }

    #[test]
    fn no_post_action_window_when_the_opponent_has_no_paid_abilities_at_all() {
        let state = corp_state(3, 5);

        let (next, _events) =
            apply_action(&state, &registry(), PlayerAction::GainCreditClick { side: Side::Corp })
                .expect("gain credit should succeed");

        assert!(next.paid_ability_window.is_none());
    }

    /// Closing is a `PassPriority`, which is not an action — so it cannot
    /// open another window. Were that wrong, the two sides would pass at
    /// each other forever.
    #[test]
    fn passing_out_of_a_post_action_window_returns_to_the_action_phase_without_reopening() {
        let (state, registry) = corp_turn_with_runner_paid_ability(5);

        let (state, _) = apply_action(&state, &registry, PlayerAction::GainCreditClick { side: Side::Corp }).unwrap();
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp }).unwrap();
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner }).unwrap();

        assert!(state.paid_ability_window.is_none(), "both passed, so the window closed and stayed closed");
        assert_eq!(state.phase, GamePhase::Action(Side::Corp));

        // And the Corp simply carries on with their turn.
        let (state, _) = apply_action(&state, &registry, PlayerAction::GainCreditClick { side: Side::Corp })
            .expect("the acting player continues after the window");
        assert_eq!(state.corp.resources.clicks, Clicks(1), "two of three clicks spent");
    }

    #[test]
    fn no_post_action_window_mid_run() {
        let (mut state, registry) = corp_turn_with_runner_paid_ability(5);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.resources.clicks = Clicks(3);
        state.active_run = Some(RunState {
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 1, 1, true)],
            jack_out_permitted: true,
            ..Default::default()
        });

        let (next, _events) =
            apply_action(&state, &registry, PlayerAction::GainCreditClick { side: Side::Runner })
                .expect("gaining a credit mid-run is legal");

        assert!(next.paid_ability_window.is_none(), "the run owns its own checkpoints");
    }

    #[test]
    fn corp_trash_resource_while_runner_tagged_moves_card_from_rig_to_heap() {
        let card_id = CardId("daily_casts".to_string());
        let mut state = corp_state(3, 5);
        state.runner.tags = 1;
        state.runner.rig = vec![installed_runner_card("daily_casts", 0)];
        let mut registry = CardRegistry::new();
        registry.insert(test_card("daily_casts", Side::Runner, CardType::Resource, 3, None));

        let (next, events) = apply_action(
            &state,
            &registry,
            PlayerAction::TrashResource { card_id: card_id.clone() },
        )
        .expect("action should succeed");

        assert_eq!(next.corp.resources.clicks, Clicks(2));
        assert_eq!(next.corp.resources.credits, Credits(3));
        assert!(next.runner.rig.is_empty());
        assert_eq!(next.runner.heap, vec![card_id.clone()]);
        assert_eq!(
            events,
            vec![
                GameEvent::ClickSpent { side: Side::Corp },
                GameEvent::CreditsSpent { side: Side::Corp, amount: 2 },
                GameEvent::CardTrashed { side: Side::Runner, card: card_id },
            ]
        );
    }

    #[test]
    fn corp_trash_resource_with_zero_tags_returns_runner_not_tagged() {
        let card_id = CardId("daily_casts".to_string());
        let mut state = corp_state(3, 5);
        state.runner.rig = vec![installed_runner_card("daily_casts", 0)];
        let mut registry = CardRegistry::new();
        registry.insert(test_card("daily_casts", Side::Runner, CardType::Resource, 3, None));

        let result = apply_action(&state, &registry, PlayerAction::TrashResource { card_id });

        assert_eq!(result, Err(RulesError::RunnerNotTagged));
    }

    #[test]
    fn corp_trash_resource_on_non_resource_card_returns_card_not_resource() {
        let card_id = CardId("gordian_blade".to_string());
        let mut state = corp_state(3, 5);
        state.runner.tags = 1;
        state.runner.rig = vec![installed_runner_card("gordian_blade", 0)];
        let mut registry = CardRegistry::new();
        registry.insert(test_card("gordian_blade", Side::Runner, CardType::Program, 3, None));

        let result = apply_action(&state, &registry, PlayerAction::TrashResource { card_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::CardNotResource { card: card_id }));
    }

    #[test]
    fn corp_trash_resource_on_card_not_in_rig_returns_card_not_in_rig() {
        let card_id = CardId("daily_casts".to_string());
        let mut state = corp_state(3, 5);
        state.runner.tags = 1;
        let mut registry = CardRegistry::new();
        registry.insert(test_card("daily_casts", Side::Runner, CardType::Resource, 3, None));

        let result = apply_action(&state, &registry, PlayerAction::TrashResource { card_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::CardNotInRig { side: Side::Runner, card: card_id }));
    }

    #[test]
    fn runner_turn_trash_resource_returns_wrong_phase() {
        let card_id = CardId("daily_casts".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.tags = 1;

        let result = apply_action(&state, &registry(), PlayerAction::TrashResource { card_id });

        assert_eq!(
            result,
            Err(RulesError::WrongPhase {
                expected: GamePhase::Action(Side::Corp),
                actual: GamePhase::Action(Side::Runner),
            })
        );
    }

    #[test]
    fn initiate_run_seeds_bad_publicity_credits_from_corp_bad_publicity_counter() {
        let mut state = runner_state(3, 5, 3);
        state.corp.bad_publicity = 4;

        let (next, _events) = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::Hq })
            .expect("action should succeed");

        assert_eq!(next.active_run.unwrap().bad_publicity_credits, 4);
    }

    #[test]
    fn bad_publicity_credits_are_discarded_when_the_run_ends_via_jack_out() {
        let mut state = runner_state(3, 5, 3);
        state.corp.bad_publicity = 4;
        let (after_initiate, _) =
            apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::Hq })
                .expect("initiate should succeed");
        assert_eq!(after_initiate.active_run.as_ref().unwrap().bad_publicity_credits, 4);

        let (after_continue, _) =
            apply_action(&after_initiate, &registry(), PlayerAction::ContinueRun).expect("continue should succeed");
        let (after_jack_out, _) =
            apply_action(&after_continue, &registry(), PlayerAction::JackOut).expect("jack out should succeed");

        assert!(after_jack_out.active_run.is_none());
    }

    #[test]
    fn runner_activate_ability_gated_by_is_tagged_requirement() {
        let card_id = CardId("wireless_net_pavilion".to_string());
        let mut card = test_card_with_ability(
            "wireless_net_pavilion",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::GainCredits(Side::Runner, 3),
        );
        card.abilities[0].requirement = Some(crate::dsl::EffectRequirement::IsTagged);
        let mut registry = CardRegistry::new();
        registry.insert(card);

        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![installed_runner_card("wireless_net_pavilion", 0)];

        // Untagged: the ability is blocked and no cost is paid.
        let result = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
        );
        assert_eq!(result, Err(RulesError::RunnerNotTagged));

        // Tagged: the ability activates normally.
        state.runner.tags = 1;
        let (next, events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
        )
        .expect("action should succeed while tagged");

        assert_eq!(next.runner.resources.credits, Credits(7)); // 5 - 1 (cost) + 3 (effect)
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 1 },
                GameEvent::AbilityActivated { side: Side::Runner, card_id, ability_index: 0 },
                GameEvent::CreditsGained { side: Side::Runner, amount: 3 },
            ]
        );
    }

    /// A `RunState` parked at `RunPhase::AccessingCard` awaiting `card_id`'s
    /// `PendingChoice`/`PendingInteractiveTrigger` decision — used by the
    /// access-time paid-ability-window tests below.
    fn run_accessing(server: ServerId, phase: run::AccessPhase) -> RunState {
        RunState {
            server,
            phase: RunPhase::AccessingCard,
            access_state: Some(run::AccessState {
                server,
                phase,
                ..Default::default()
            }),
            jack_out_permitted: true,
            ..Default::default()
        }
    }

    fn pending_choice(card_id: &CardId) -> run::AccessPhase {
        run::AccessPhase::PendingChoice {
            card_id: card_id.clone(),
            can_trash: false,
            trash_cost: None,
            mandatory_steal: false,
            steal_cost: None,
        }
    }

    fn pending_interactive_trigger(card_id: &CardId) -> run::AccessPhase {
        run::AccessPhase::PendingInteractiveTrigger {
            card_id: card_id.clone(),
            cost: Cost::Credits(2),
            decider: Side::Runner,
            can_pay: true,
        }
    }

    #[test]
    fn runner_activate_ability_succeeds_during_access_time_window_at_pending_choice() {
        let card_id = CardId("hedge_fund".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.rig = vec![installed_runner_card("investment", 0)];
        state.active_run = Some(run_accessing(ServerId::Hq, pending_choice(&card_id)));
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
            checkpoint: WindowCheckpoint::Run,
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "investment",
            Side::Runner,
            Trigger::Paid,
            None,
            Effect::GainCredits(Side::Runner, 3),
        ));

        let (next, _events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility {
                card_id: CardId("investment".to_string()),
                ability_index: 0,
            },
        )
        .expect("action should succeed");

        assert_eq!(next.runner.resources.credits, Credits(3));
        let window = next.paid_ability_window.expect("window should stay open");
        assert_eq!(window.consecutive_passes, 0, "firing a paid ability resets the pass counter");
        assert_eq!(window.active_priority, Side::Corp, "priority toggles to the other side");
    }

    #[test]
    fn runner_activate_ability_succeeds_during_access_time_window_at_pending_interactive_trigger() {
        let card_id = CardId("fetal_ai".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.rig = vec![installed_runner_card("investment", 0)];
        state.active_run = Some(run_accessing(ServerId::Hq, pending_interactive_trigger(&card_id)));
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
            checkpoint: WindowCheckpoint::Run,
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "investment",
            Side::Runner,
            Trigger::Paid,
            None,
            Effect::GainCredits(Side::Runner, 3),
        ));

        let (next, _events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility {
                card_id: CardId("investment".to_string()),
                ability_index: 0,
            },
        )
        .expect("action should succeed");

        assert_eq!(next.runner.resources.credits, Credits(3));
        let window = next.paid_ability_window.expect("window should stay open");
        assert_eq!(window.active_priority, Side::Corp);
    }

    #[test]
    fn corp_activate_ability_succeeds_during_access_time_window() {
        let card_id = CardId("hedge_fund".to_string());
        let mut state = runner_state(3, 0, 0);
        state.corp.installed = vec![InstalledCard {
            card: CardId("pad_campaign".to_string()),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
        }];
        state.active_run = Some(run_accessing(ServerId::Hq, pending_choice(&card_id)));
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Corp,
            consecutive_passes: 0,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
            checkpoint: WindowCheckpoint::Run,
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "pad_campaign",
            Side::Corp,
            Trigger::Paid,
            None,
            Effect::GainCredits(Side::Corp, 1),
        ));

        let (next, _events) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility {
                card_id: CardId("pad_campaign".to_string()),
                ability_index: 0,
            },
        )
        .expect("action should succeed");

        assert_eq!(next.corp.resources.credits, Credits(1));
        let window = next.paid_ability_window.expect("window should stay open");
        assert_eq!(window.active_priority, Side::Runner, "priority toggles to the other side");
    }

    #[test]
    fn pending_choice_actions_are_blocked_while_an_access_time_window_is_open() {
        let card_id = CardId("hedge_fund".to_string());
        let window = PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
            checkpoint: WindowCheckpoint::Run,
        };

        for action in [
            PlayerAction::StealAgenda { card_id: card_id.clone() },
            PlayerAction::TrashAccessedCard { card_id: card_id.clone() },
            PlayerAction::PassAccessedCard { card_id: card_id.clone() },
        ] {
            let mut state = runner_state(3, 0, 0);
            state.active_run = Some(run_accessing(ServerId::Hq, pending_choice(&card_id)));
            state.paid_ability_window = Some(window.clone());

            let result = apply_action(&state, &registry(), action);

            assert_eq!(result, Err(RulesError::BlockedByPaidAbilityWindow { priority: Side::Runner }));
        }
    }

    #[test]
    fn pending_interactive_trigger_actions_are_blocked_while_an_access_time_window_is_open() {
        let card_id = CardId("fetal_ai".to_string());
        let window = PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
            checkpoint: WindowCheckpoint::Run,
        };

        for action in [
            PlayerAction::PayAccessTrigger { card_id: card_id.clone() },
            PlayerAction::DeclineAccessTrigger { card_id: card_id.clone() },
        ] {
            let mut state = runner_state(3, 0, 0);
            state.active_run = Some(run_accessing(ServerId::Hq, pending_interactive_trigger(&card_id)));
            state.paid_ability_window = Some(window.clone());

            let result = apply_action(&state, &registry(), action);

            assert_eq!(result, Err(RulesError::BlockedByPaidAbilityWindow { priority: Side::Runner }));
        }
    }

    #[test]
    fn access_time_window_closes_without_disturbing_a_pending_choice() {
        let card_id = CardId("hedge_fund".to_string());
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(run_accessing(ServerId::Hq, pending_choice(&card_id)));
        // Runner already passed once; Corp's pass here is the second and
        // should close the window.
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Corp,
            consecutive_passes: 1,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
            checkpoint: WindowCheckpoint::Run,
        });

        let (next, events) =
            apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Corp })
                .expect("pass should succeed");

        assert_eq!(events, vec![GameEvent::PriorityPassed { side: Side::Corp }, GameEvent::PaidAbilityWindowClosed]);
        assert!(next.paid_ability_window.is_none());
        assert_eq!(
            next.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            pending_choice(&card_id),
            "the pending choice itself is untouched by the window closing"
        );

        // The Runner can now resolve it normally.
        let (after_pass, _events) =
            apply_action(&next, &registry(), PlayerAction::PassAccessedCard { card_id })
                .expect("passing the accessed card should succeed");
        assert_eq!(after_pass.active_run, None);
    }

    #[test]
    fn access_time_window_closes_without_disturbing_a_pending_interactive_trigger() {
        let card_id = CardId("fetal_ai".to_string());
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(run_accessing(ServerId::Hq, pending_interactive_trigger(&card_id)));
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Corp,
            consecutive_passes: 1,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
            checkpoint: WindowCheckpoint::Run,
        });

        let (next, _events) =
            apply_action(&state, &registry(), PlayerAction::PassPriority { side: Side::Corp })
                .expect("pass should succeed");

        assert!(next.paid_ability_window.is_none());
        assert_eq!(
            next.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            pending_interactive_trigger(&card_id)
        );

        // The Runner can now decline (registry has no matching card, so this
        // just falls through to the card's normal, empty `PendingChoice`).
        let (after_decline, _events) =
            apply_action(&next, &registry(), PlayerAction::DeclineAccessTrigger { card_id })
                .expect("declining should succeed");
        assert_eq!(after_decline.active_run.unwrap().phase, RunPhase::AccessingCard);
    }

    #[test]
    fn multi_card_access_sequence_opens_a_fresh_window_at_each_card() {
        let card_a = CardId("card_a".to_string());
        let card_b = CardId("card_b".to_string());
        let mut state = runner_state(3, 0, 0);
        state.corp.archives = vec![ArchivedCard::facedown(card_a.clone()), ArchivedCard::facedown(card_b.clone())];
        state.active_run = Some(RunState {
            server: ServerId::Archives,
            phase: RunPhase::Success,
            jack_out_permitted: true,
            ..Default::default()
        });
        let mut registry = CardRegistry::new();
        registry.insert(test_card("card_a", Side::Corp, CardType::Asset, 0, None));
        registry.insert(test_card("card_b", Side::Corp, CardType::Asset, 0, None));

        let (state, complete_events) =
            apply_action(&state, &registry, PlayerAction::CompleteRun).expect("action should succeed");
        assert_eq!(complete_events, vec![GameEvent::PaidAbilityWindowOpened { side: Side::Runner }]);

        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner })
            .expect("pass should succeed");
        let (state, events) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("pass should succeed");
        // Two cards land on `SelectNextCard` — not a checkpoint, so no
        // window opens here.
        assert_eq!(
            events,
            vec![GameEvent::PriorityPassed { side: Side::Corp }, GameEvent::PaidAbilityWindowClosed]
        );
        assert!(state.paid_ability_window.is_none());

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::SelectCardToAccess { card_id: card_a.clone() })
                .expect("selecting the first card should succeed");
        assert_eq!(
            events,
            vec![
                GameEvent::CardAccessed { card: card_a.clone(), server: ServerId::Archives },
                GameEvent::PaidAbilityWindowOpened { side: Side::Runner },
            ]
        );

        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner })
            .expect("pass should succeed");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("pass should succeed");
        assert!(state.paid_ability_window.is_none());

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::PassAccessedCard { card_id: card_a.clone() })
                .expect("passing the first card should succeed");
        // Presenting the second card opens *another* fresh window.
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: card_a },
                GameEvent::CardAccessed { card: card_b.clone(), server: ServerId::Archives },
                GameEvent::PaidAbilityWindowOpened { side: Side::Runner },
            ]
        );

        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner })
            .expect("pass should succeed");
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("pass should succeed");
        assert!(state.paid_ability_window.is_none());

        let (state, events) =
            apply_action(&state, &registry, PlayerAction::PassAccessedCard { card_id: card_b.clone() })
                .expect("passing the second card should succeed");
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: card_b },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
        assert_eq!(state.active_run, None);
        assert!(state.paid_ability_window.is_none());
    }

    #[test]
    fn boost_strength_persists_across_a_pass_for_a_later_break_subroutines_activation() {
        let card_id = CardId("corroder".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(5);
        // Deliberately too weak to break the ICE alone (1 < 2) — the second
        // activation below only succeeds because the first activation's
        // encounter-duration boost is still applied.
        state.runner.rig = vec![installed_runner_card("corroder", 1)];
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice_of_type("ice_wall", 2, 1, true, IceType::Barrier)],
            ..Default::default()
        });
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
            checkpoint: WindowCheckpoint::Run,
        });

        let mut registry = CardRegistry::new();
        let mut card = test_card_with_ability(
            "corroder",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter },
        );
        card.abilities.push(AbilityDef {
            trigger: Trigger::Paid,
            cost: Some(Cost::Credits(1)),
            requirement: None,
            effect: Effect::BreakSubroutines {
                count: SubroutineBreakCount::Fixed(1),
                restrict_to: Some(IceType::Barrier),
            },
            cost_discount_if: None,
        });
        registry.insert(card);

        // Runner boosts; priority passes to Corp.
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id: card_id.clone(), ability_index: 0 },
        )
        .expect("boost should succeed");
        assert_eq!(state.runner.rig[0].effective_strength(), 2);
        assert_eq!(
            state.paid_ability_window.as_ref().unwrap().active_priority,
            Side::Corp,
            "priority toggles to the other side after a window-legal action"
        );

        // Corp declines to act; priority returns to the Runner.
        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("pass should succeed");
        assert_eq!(state.paid_ability_window.as_ref().unwrap().active_priority, Side::Runner);
        // The pass alone (no encounter exit) must not have reset the boost.
        assert_eq!(state.runner.rig[0].effective_strength(), 2);

        // Runner now breaks the subroutine — only possible because the
        // boost from the first activation persisted through the pass.
        let (next, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id, ability_index: 1 },
        )
        .expect("break should succeed now that effective strength meets the ICE's");

        assert_eq!(next.active_run.unwrap().ice[0].subroutines[0].status, SubroutineStatus::Broken);
    }

    #[test]
    fn partial_break_leaves_only_the_unbroken_subroutine_to_auto_fire_on_priority_close() {
        let card_id = CardId("mimic".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![installed_runner_card("mimic", 2)];
        state.active_run = Some(RunState {
            phase: RunPhase::EncounterIce,
            ice: vec![RunIce {
                card_id: CardId("ice_wall".to_string()),
                current_strength: 2,
                ice_type: IceType::Barrier,
                subroutines: vec![
                    EncounteredSubroutine {
                        id: 0,
                        definition: SubroutineDef { text: "sub 0".to_string(), effect: Effect::GiveTags(1) },
                        status: SubroutineStatus::Pending,
                    },
                    EncounteredSubroutine {
                        id: 1,
                        definition: SubroutineDef { text: "sub 1".to_string(), effect: Effect::GiveTags(2) },
                        status: SubroutineStatus::Pending,
                    },
                ],
                rezzed: true,
            }],
            ..Default::default()
        });
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
            checkpoint: WindowCheckpoint::Run,
        });

        let mut registry = CardRegistry::new();
        registry.insert(test_card_with_ability(
            "mimic",
            Side::Runner,
            Trigger::Paid,
            Some(Cost::Credits(1)),
            Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None },
        ));

        // Break the lowest-id pending subroutine (id 0); priority passes to Corp.
        let (state, _) = apply_action(
            &state,
            &registry,
            PlayerAction::ActivateAbility { card_id, ability_index: 0 },
        )
        .expect("break should succeed");
        assert_eq!(
            state.active_run.as_ref().unwrap().ice[0].subroutines[0].status,
            SubroutineStatus::Broken
        );
        assert_eq!(
            state.active_run.as_ref().unwrap().ice[0].subroutines[1].status,
            SubroutineStatus::Pending
        );

        let (state, _) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Corp })
            .expect("pass should succeed");
        let (next, events) = apply_action(&state, &registry, PlayerAction::PassPriority { side: Side::Runner })
            .expect("pass should succeed");

        // Only the unbroken subroutine (id 1, GiveTags(2)) auto-fires; the
        // broken one (id 0, GiveTags(1)) never does.
        assert_eq!(next.runner.tags, 2);
        let ice = &next.active_run.as_ref().unwrap().ice[0];
        assert_eq!(ice.subroutines[0].status, SubroutineStatus::Broken);
        assert_eq!(ice.subroutines[1].status, SubroutineStatus::Resolved);
        let fired: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, GameEvent::SubroutineFired { .. }))
            .collect();
        assert_eq!(
            fired,
            vec![&GameEvent::SubroutineFired {
                card_id: CardId("ice_wall".to_string()),
                index: 1,
                effect: Effect::GiveTags(2),
            }]
        );
    }
}
