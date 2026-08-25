use crate::cards::CardRegistry;
use crate::dsl::{CardId, CardType, Cost, IceType, Trigger};
use crate::rules::ability;
use crate::rules::action::{PlayerAction, ServerTarget, TargetZone};
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::paid_ability;
use crate::rules::run::{self, EncounteredSubroutine, RunAction, RunIce, RunPhase, RunState, SubroutineStatus};
use crate::rules::state::{GamePhase, GameState, InstallSlot, InstalledCard, InstalledRunnerCard, Side};
use crate::rules::trace;
use crate::rules::turn;

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
    match action {
        PlayerAction::GainCreditClick { side } => gain_credit_click(state, side),
        PlayerAction::DrawCardClick => draw_card_click(state),
        PlayerAction::InstallCard { card_id, zone, slot } => {
            install_card(state, registry, card_id, zone, slot)
        }
        PlayerAction::RezIce { ice_id } => rez_ice(state, ice_id),
        PlayerAction::InitiateRun { server } => initiate_run(state, registry, server),
        PlayerAction::ContinueRun => continue_run(state),
        PlayerAction::JackOut => jack_out(state),
        PlayerAction::CompleteRun => complete_run(state, registry),
        PlayerAction::PlayEvent { card_id } => play_event(state, registry, card_id),
        PlayerAction::PlayOperation { card_id } => play_operation(state, registry, card_id),
        PlayerAction::InstallHardware { card_id } => install_hardware(state, registry, card_id),
        PlayerAction::InstallProgram { card_id, memory_cost } => {
            install_program(state, registry, card_id, memory_cost)
        }
        PlayerAction::BreakSubroutine { ice_id, subroutine_index } => {
            break_subroutine(state, ice_id, subroutine_index)
        }
        PlayerAction::EndTurn => turn::end_turn(state),
        PlayerAction::DiscardCard { card_id } => turn::discard_card(state, card_id),
        PlayerAction::ActivateAbility { card_id, ability_index } => {
            activate_ability(state, registry, card_id, ability_index)
        }
        PlayerAction::AdvanceCard { card_id } => advance_card(state, registry, card_id),
        PlayerAction::RemoveTag => remove_tag(state),
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
        PlayerAction::PayToAvoidAccessTrigger { card_id } => {
            pay_to_avoid_access_trigger(state, registry, card_id)
        }
        PlayerAction::DeclineAccessTrigger { card_id } => {
            decline_access_trigger(state, registry, card_id)
        }
        PlayerAction::PassPriority { side } => pass_priority_action(state, registry, side),
        PlayerAction::SubmitCorpTraceBid { amount } => submit_corp_trace_bid(state, amount),
        PlayerAction::SubmitRunnerTraceBid { amount } => submit_runner_trace_bid(state, amount),
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

fn draw_card_click(state: &GameState) -> Result<(GameState, Vec<GameEvent>), RulesError> {
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
    });
    events.push(GameEvent::CardInstalled {
        side,
        card: card_id,
        server: zone,
    });

    Ok((next, events))
}

fn rez_ice(state: &GameState, ice_id: CardId) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Corp;
    let rez_window_open =
        matches!(&state.active_run, Some(run) if run.phase == RunPhase::ApproachIce);
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

    Ok((next, vec![GameEvent::IceRezzed { card: ice_id, server }]))
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

    let mut next = state.clone();
    spend_click(&mut next, side)?;

    // `corp.installed`'s Vec order is install order (oldest first);
    // installs only ever `.push()` (see `install_card`), never reorder.
    // Oldest install = outermost ICE = index 0, matching `RunIce`'s
    // outermost-to-innermost doc comment — no reversal needed.
    let ice: Vec<RunIce> = next
        .corp
        .installed
        .iter()
        .filter(|installed| installed.server == server && installed.slot == InstallSlot::Ice)
        .map(|installed| build_run_ice(installed, registry))
        .collect();

    next.active_run = Some(RunState { access_state: None,
        bad_publicity_credits: next.corp.bad_publicity,
        server,
        phase: RunPhase::Initiation,
        ice,
        position: 0,
        // Netrunner/Null Signal Games jack-out rule 1: closed until an ICE is passed (or the
        // server approach step is reached with none installed) — see
        // `run::engine::pass_current_ice`/`continue_run`'s `Initiation` arm.
        jack_out_permitted: false,
    });

    Ok((
        next,
        vec![
            GameEvent::ClickSpent { side },
            GameEvent::RunInitiated { server },
        ],
    ))
}

/// Builds one `RunIce` from an `InstalledCard` known to be ICE (caller
/// filters by `InstallSlot::Ice`), looking up strength/subroutines from
/// `registry`. Never errors: a `card_id` absent from `registry` (or missing
/// `strength`/`subroutines`) degrades to a blank 0-strength/no-subroutines
/// ICE that can't block anything, mirroring
/// `run::access::compute_pending_choice`'s existing leniency for
/// unrecognized cards, rather than failing the whole `InitiateRun` action
/// over one unregistered card elsewhere on the server.
fn build_run_ice(installed: &InstalledCard, registry: &CardRegistry) -> RunIce {
    let card_def = registry.get(&installed.card);
    let current_strength = card_def.and_then(|c| c.strength).unwrap_or(0);
    let ice_type = card_def
        .and_then(|c| match &c.card_type {
            CardType::Ice(ice_type) => Some(*ice_type),
            _ => None,
        })
        .unwrap_or(IceType::Barrier);
    let subroutines = card_def
        .map(|c| {
            c.subroutines
                .iter()
                .enumerate()
                .map(|(id, def)| EncounteredSubroutine {
                    id,
                    definition: def.clone(),
                    status: SubroutineStatus::Pending,
                })
                .collect()
        })
        .unwrap_or_default();

    RunIce { card_id: installed.card.clone(), current_strength, ice_type, subroutines, rezzed: installed.rezzed }
}

fn continue_run(state: &GameState) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    let mut events = run::advance_run(&mut next, RunAction::Continue)?;
    events.extend(paid_ability::open_window_if_at_checkpoint(&mut next));

    Ok((next, events))
}

fn jack_out(state: &GameState) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;
    let mut next = state.clone();
    let events = run::advance_run(&mut next, RunAction::JackOut)?;
    next.active_run = None;
    // A window can be open here (e.g. mid-ApproachIce on the second+ ICE,
    // where jack_out_permitted is already true from a prior pass) — clear it
    // too, or it would survive with no active_run left to ever close it
    // against, permanently blocking every ordinary action afterward.
    next.paid_ability_window = None;

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
    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(card_def.cost), Some(&card_id))?);
    events.push(GameEvent::EventPlayed { side, card: card_id.clone() });
    events.extend(ability::process_card_triggers(&mut next, registry, &card_id, Trigger::OnPlay)?);

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

    let mut events = vec![GameEvent::ClickSpent { side }];
    events.extend(ability::pay_cost(&mut next, side, &Cost::Credits(card_def.cost), Some(&card_id))?);
    next.corp.archives.push(card_id.clone());
    events.push(GameEvent::OperationPlayed { side, card: card_id.clone() });
    events.extend(ability::process_card_triggers(&mut next, registry, &card_id, Trigger::OnPlay)?);

    Ok((next, events))
}

/// Seeds a newly-installed rig card's `base_strength` from the registry's
/// printed `strength` — mirrors `build_run_ice`'s identical seed-once
/// pattern for `RunIce::current_strength`. `0` for Hardware/non-strength
/// Programs (`Card::strength` is `None`).
fn seed_rig_card(registry: &CardRegistry, card_id: CardId) -> Result<InstalledRunnerCard, RulesError> {
    let card_def = registry
        .get(&card_id)
        .ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
    Ok(InstalledRunnerCard {
        base_strength: card_def.strength.unwrap_or(0),
        card: card_id,
        encounter_strength_buff: 0,
        turn_strength_buff: 0,
    })
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
    let rig_card = seed_rig_card(registry, card_id.clone())?;
    next.runner.rig.push(rig_card);

    Ok((
        next,
        vec![
            GameEvent::ClickSpent { side },
            GameEvent::HardwareInstalled { side, card: card_id },
        ],
    ))
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
    let rig_card = seed_rig_card(registry, card_id.clone())?;
    next.runner.rig.push(rig_card);

    Ok((
        next,
        vec![
            GameEvent::ClickSpent { side },
            GameEvent::ProgramInstalled { side, card: card_id, memory_cost },
        ],
    ))
}

fn break_subroutine(
    state: &GameState,
    // Not cross-checked against `RunState::ice` — see `PlayerAction::BreakSubroutine`'s doc comment.
    _ice_id: CardId,
    subroutine_index: usize,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let side = Side::Runner;
    require_phase(state, GamePhase::Action(side))?;

    // `step_subroutine` (via `advance_run`) now does its own bounds/status
    // validation against `RunIce::subroutines`, so there's no need to
    // duplicate a pre-check here — just forward the index.
    let mut next = state.clone();
    let events = run::advance_run(&mut next, RunAction::BreakSubroutine(subroutine_index))?;
    // Priority-independent like RezIce (not gated on whose priority it is),
    // but still gives the other side a fresh chance to respond if a window
    // is open.
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
        ability::check_requirement(state, requirement)?;
    }

    let mut next = state.clone();
    let mut events = Vec::new();
    if let Some(cost) = &ability.cost {
        events.extend(ability::pay_cost(&mut next, side, cost, Some(&card_id))?);
    }
    events.push(GameEvent::AbilityActivated { side, card_id: card_id.clone(), ability_index });
    events.extend(ability::evaluate_effect(&mut next, &ability.effect, Some(&card_id))?);
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
    events.push(GameEvent::CardAdvanced { card: card_id, advancement_tokens });

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

/// Resolves `PlayerAction::PayToAvoidAccessTrigger`, per its doc comment.
fn pay_to_avoid_access_trigger(
    state: &GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    require_phase(state, GamePhase::Action(Side::Runner))?;
    paid_ability::require_no_window(state)?;
    let mut next = state.clone();
    let mut events = run::resolve_pay_to_avoid(&mut next, &card_id, registry)?;
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
    let mut events = run::resolve_decline_to_avoid(&mut next, &card_id, registry)?;
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
fn submit_runner_trace_bid(state: &GameState, amount: u32) -> Result<(GameState, Vec<GameEvent>), RulesError> {
    let mut next = state.clone();
    let events = trace::submit_runner_bid(&mut next, amount)?;
    Ok((next, events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{
        AbilityDef, BoostDuration, Card, CardType, Cost, Effect, IceType, SubroutineBreakCount,
        SubroutineDef, TriggeredEffect,
    };
    use crate::rules::run::{EncounteredSubroutine, RunIce, ServerId, SubroutineStatus};
    use crate::rules::state::{AgendaPoints, Clicks, Credits, PaidAbilityWindow, PlayerResources};

    /// An empty registry, for every test that doesn't exercise
    /// `PlayerAction::ActivateAbility` and so doesn't need real card
    /// definitions.
    fn registry() -> CardRegistry {
        CardRegistry::new()
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
            corp: crate::rules::state::CorpState { bad_publicity: 0,
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(credits),
                    clicks: Clicks(clicks),
                    agenda_points: AgendaPoints(0),
                },
                hq: Vec::new(),
                r_and_d: Vec::new(),
                archives: Vec::new(),
                installed: Vec::new(),
            },
            runner: crate::rules::state::RunnerState {
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: crate::rules::state::MemoryUnits(0),
                brain_damage: 0,
                tags: 0,
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
                heap: Vec::new(),
                link_strength: 0,
            },
            phase: GamePhase::Action(Side::Corp),
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            seed: 0,
            rng_step: 0,
        }
    }

    /// `stack_size`/`grip_size` are filled with distinct placeholder `CardId`s
    /// (identity doesn't matter for the tests using this — only counts do).
    fn runner_state(clicks: u32, stack_size: u32, grip_size: u32) -> GameState {
        GameState {
            corp: crate::rules::state::CorpState { bad_publicity: 0,
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                hq: Vec::new(),
                r_and_d: Vec::new(),
                archives: Vec::new(),
                installed: Vec::new(),
            },
            runner: crate::rules::state::RunnerState {
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(clicks),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: crate::rules::state::MemoryUnits(0),
                brain_damage: 0,
                tags: 0,
                grip: (0..grip_size).map(|i| CardId(format!("grip_card_{i}"))).collect(),
                stack: (0..stack_size).map(|i| CardId(format!("stack_card_{i}"))).collect(),
                rig: Vec::new(),
                heap: Vec::new(),
                link_strength: 0,
            },
            phase: GamePhase::Action(Side::Runner),
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            seed: 0,
            rng_step: 0,
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
        assert_eq!(events, vec![GameEvent::ClickSpent { side: Side::Runner }]);
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
                advancement_tokens: 0,
                card: card_id.clone(),
                server: ServerId::Hq,
                slot: InstallSlot::Ice,
                rezzed: false,
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
    fn corp_rez_ice_flips_installed_card_and_costs_nothing() {
        let card_id = CardId("ice_wall".to_string());
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: card_id.clone(),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: false,
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        let (next, events) = apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: card_id.clone() })
            .expect("action should succeed");

        assert!(next.corp.installed[0].rezzed);
        assert_eq!(next.corp.resources.clicks, Clicks(3));
        assert_eq!(next.corp.resources.credits, Credits(5));
        assert_eq!(
            events,
            vec![GameEvent::IceRezzed { card: card_id, server: ServerId::Hq }]
        );
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
            advancement_tokens: 0,
            card: card_id.clone(),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: true,
        }];
        let state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        let result = apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: card_id.clone() });

        assert_eq!(result, Err(RulesError::AlreadyRezzed { card: card_id }));
    }

    #[test]
    fn corp_can_rez_ice_during_run_approach_ice_even_though_phase_is_runner_action() {
        let card_id = CardId("ice_wall".to_string());
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: card_id.clone(),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: false,
        }];
        let mut state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        state.phase = GamePhase::Action(Side::Runner);
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1, false)],
            position: 0,
         jack_out_permitted: true,});

        let (next, events) = apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: card_id.clone() })
            .expect("Corp should be able to rez ICE during the Runner's run");

        assert!(next.corp.installed[0].rezzed);
        assert!(next.active_run.as_ref().unwrap().ice[0].rezzed);
        assert_eq!(next.corp.resources.clicks, Clicks(3));
        assert_eq!(next.corp.resources.credits, Credits(5));
        assert_eq!(
            events,
            vec![GameEvent::IceRezzed { card: card_id, server: ServerId::Hq }]
        );
    }

    #[test]
    fn corp_rez_ice_for_ice_not_at_current_position_does_not_affect_run_ice() {
        let outer = CardId("outer_ice".to_string());
        let inner = CardId("inner_ice".to_string());
        let installed = vec![
            InstalledCard {
                advancement_tokens: 0,
                card: outer.clone(),
                server: ServerId::Hq,
                slot: InstallSlot::Ice,
                rezzed: false,
            },
            InstalledCard {
                advancement_tokens: 0,
                card: inner.clone(),
                server: ServerId::Hq,
                slot: InstallSlot::Ice,
                rezzed: false,
            },
        ];
        let mut state = corp_state_with_hq_and_installed(3, 5, Vec::new(), installed);
        state.phase = GamePhase::Action(Side::Runner);
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("outer_ice", 0, 1, false), test_ice("inner_ice", 0, 1, false)],
            position: 0,
         jack_out_permitted: true,});

        let (next, _events) = apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: inner.clone() })
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
            Some(RunState { bad_publicity_credits: 0, access_state: None,
                server: ServerId::Hq,
                phase: RunPhase::Initiation,
                ice: Vec::new(),
                position: 0,
                jack_out_permitted: false,
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
            advancement_tokens: 0,
            card: CardId("outer_ice".to_string()),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: false,
        };
        let inner = InstalledCard {
            advancement_tokens: 0,
            card: CardId("inner_ice".to_string()),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: true,
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
                advancement_tokens: 0,
                card: CardId("some_upgrade".to_string()),
                server: ServerId::Hq,
                slot: InstallSlot::Root,
                rezzed: false,
            },
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("remote_ice".to_string()),
                server: ServerId::Remote(0),
                slot: InstallSlot::Ice,
                rezzed: true,
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
    fn runner_initiate_run_defaults_unregistered_ice_to_blank() {
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: CardId("mystery_ice".to_string()),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: false,
        }];
        let mut state = corp_state_with_hq_and_installed(0, 0, Vec::new(), installed);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = runner_state(3, 5, 3).runner;

        let (next, _events) = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::Hq })
            .expect("action should succeed");

        let ice = next.active_run.unwrap().ice;
        assert_eq!(
            ice,
            vec![RunIce {
                card_id: CardId("mystery_ice".to_string()),
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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            position: 0,
         jack_out_permitted: true,});
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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            position: 0,
         jack_out_permitted: true,});
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
            advancement_tokens: 0,
            card: CardId("ice_wall".to_string()),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: false,
        }];
        let mut state = corp_state_with_hq_and_installed(0, 0, Vec::new(), installed);
        state.phase = GamePhase::Action(Side::Runner);
        state.runner = runner_state(3, 5, 3).runner;

        let (after_initiate, _) = apply_action(&state, &registry(), PlayerAction::InitiateRun { server: ServerId::Hq })
            .expect("initiate run should succeed");
        let result = apply_action(&after_initiate, &registry(), PlayerAction::JackOut);

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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::AccessingCard,
            ice: Vec::new(),
            position: 0,
         jack_out_permitted: true,});
        let result = apply_action(&state, &registry(), PlayerAction::JackOut);

        assert_eq!(
            result,
            Err(RulesError::RunAlreadyConcluded { phase: RunPhase::AccessingCard })
        );
    }

    #[test]
    fn runner_can_initiate_run_again_after_jacking_out() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            position: 0,
         jack_out_permitted: true,});

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
            Some(RunState { bad_publicity_credits: 0, access_state: None,
                server: ServerId::RnD,
                phase: RunPhase::Initiation,
                ice: Vec::new(),
                position: 0,
                // `initiate_run` always starts a fresh run with the
                // jack-out window closed (Netrunner/Null Signal Games rule 1) — it only opens
                // via `continue_run`, which this test never calls.
                jack_out_permitted: false,
            })
        );
    }

    #[test]
    fn runner_complete_run_clears_active_run_after_success() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
         jack_out_permitted: true,});
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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            position: 0,
         jack_out_permitted: true,});
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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
         jack_out_permitted: true,});

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
            Some(RunState { bad_publicity_credits: 0, access_state: None,
                server: ServerId::RnD,
                phase: RunPhase::Initiation,
                ice: Vec::new(),
                position: 0,
                // `initiate_run` always starts a fresh run with the
                // jack-out window closed (Netrunner/Null Signal Games rule 1) — it only opens
                // via `continue_run`, which this test never calls.
                jack_out_permitted: false,
            })
        );
    }

    #[test]
    fn runner_complete_run_against_hq_parks_the_run_awaiting_an_access_choice() {
        let mut state = runner_state(3, 5, 3);
        state.corp.hq = vec![CardId("hedge_fund".to_string())];
        state.active_run = Some(RunState { bad_publicity_credits: 0,
            access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
         jack_out_permitted: true,});
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
            Some(RunState { bad_publicity_credits: 0,
                access_state: Some(run::AccessState {
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
                server: ServerId::Hq,
                phase: RunPhase::AccessingCard,
                ice: Vec::new(),
                position: 0,
             jack_out_permitted: true,})
        );
    }

    #[test]
    fn runner_complete_run_against_empty_hq_completes_immediately() {
        let mut state = runner_state(3, 5, 3);
        state.active_run = Some(RunState { bad_publicity_credits: 0,
            access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
         jack_out_permitted: true,});
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
        let (next, events) =
            apply_action(&state, &registry(), PlayerAction::EndTurn).expect("action should succeed");

        assert_eq!(next.phase, GamePhase::Action(Side::Runner));
        assert_eq!(next.runner.resources.clicks, Clicks(4));
        assert_eq!(
            events,
            vec![
                GameEvent::TurnEnded { side: Side::Corp },
                GameEvent::TurnStarted { side: Side::Runner, clicks: 4 },
            ]
        );
    }

    #[test]
    fn runner_continue_run_steps_through_phases_with_no_click_cost() {
        let mut state = runner_state(3, 0, 0);
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::Initiation,
            ice: vec![test_ice("ice_wall", 0, 0, true)],
            position: 0,
         jack_out_permitted: true,});

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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            position: 0,
         jack_out_permitted: true,});

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
        }];
        registry.insert(card);

        let (next, events) =
            apply_action(&state, &registry, PlayerAction::PlayOperation { card_id: card_id.clone() })
                .expect("action should succeed");

        assert_eq!(next.corp.resources.clicks, Clicks(2));
        // Paid 5 to play, then the OnPlay trigger grants 9 back — net +4.
        assert_eq!(next.corp.resources.credits, Credits(9));
        assert!(next.corp.hq.is_empty());
        assert_eq!(next.corp.archives, vec![card_id.clone()]);
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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 2, true)],
            position: 0,
         jack_out_permitted: true,});
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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            position: 0,
         jack_out_permitted: true,});
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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            position: 0,
         jack_out_permitted: true,});
        let result = apply_action(
            &state,
            &registry(),
            PlayerAction::BreakSubroutine { ice_id, subroutine_index: 0 },
        );

        assert_eq!(result, Err(RulesError::NotInEncounter));
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
            encounter_strength_buff: 0,
            turn_strength_buff: 0,
        }
    }

    /// A minimal `Card` with the given install/play `cost` and
    /// `advancement_requirement`, no abilities — used by the
    /// `InstallCard`/`PlayEvent`/`AdvanceCard` cost/advancement tests, which
    /// only care about those two fields.
    fn test_card(
        card_id: &str,
        side: Side,
        card_type: CardType,
        cost: u32,
        advancement_requirement: Option<u32>,
    ) -> Card {
        Card {
            id: CardId(card_id.to_string()),
            title: card_id.to_string(),
            side,
            card_type,
            cost,
            triggers: Vec::new(),
            abilities: Vec::new(),
            trash_cost: None,
            steal_cost: None,
            advancement_requirement,
            agenda_points: None,
            min_deck_size: None,
            strength: None,
            subroutines: Vec::new(),
            interactive_on_access: None,
        }
    }

    /// A minimal `Card` whose only `abilities` entry is the given
    /// `trigger`/`cost`/`effect` — everything about the card besides its id,
    /// side, and that one ability is irrelevant to `activate_ability`'s
    /// logic, so it's held to placeholder values.
    fn test_card_with_ability(
        card_id: &str,
        side: Side,
        trigger: Trigger,
        cost: Option<Cost>,
        effect: Effect,
    ) -> Card {
        Card {
            id: CardId(card_id.to_string()),
            title: card_id.to_string(),
            side,
            card_type: CardType::Program,
            cost: 0,
            triggers: Vec::new(),
            abilities: vec![AbilityDef { trigger, cost, requirement: None, effect }],
            trash_cost: None,
            steal_cost: None,
            advancement_requirement: None,
            agenda_points: None,
            min_deck_size: None,
            strength: None,
            subroutines: Vec::new(),
            interactive_on_access: None,
        }
    }

    #[test]
    fn runner_activate_ability_pumps_icebreaker_and_deducts_credits() {
        let card_id = CardId("gordian_blade".to_string());
        let mut state = runner_state(3, 0, 0);
        state.runner.resources.credits = Credits(5);
        state.runner.rig = vec![installed_runner_card("gordian_blade", 0)];
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 0, true)],
            position: 0,
         jack_out_permitted: true,});

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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 0, true)],
            position: 0,
         jack_out_permitted: true,});

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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 2, 1, true)],
            position: 0,
         jack_out_permitted: true,});

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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 3, 1, true)],
            position: 0,
         jack_out_permitted: true,});

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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice_of_type("ice_wall", 2, 1, true, IceType::Barrier)],
            position: 0,
         jack_out_permitted: true,});

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
            state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
                server: ServerId::Hq,
                phase: RunPhase::EncounterIce,
                ice: vec![test_ice_of_type("some_ice", 2, 1, true, wrong_type)],
                position: 0,
             jack_out_permitted: true,});

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
            state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
                server: ServerId::Hq,
                phase: RunPhase::EncounterIce,
                ice: vec![test_ice_of_type("some_ice", 2, 1, true, ice_type)],
                position: 0,
             jack_out_permitted: true,});

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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 0, true)],
            position: 0,
         jack_out_permitted: false,});
        // A window is open with one pass already in (Corp holds priority
        // after the Runner's first pass); the Runner activates instead.
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 1,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 0, true)],
            position: 0,
         jack_out_permitted: false,});
        // Corp currently holds priority — the Runner tries to act anyway.
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Corp,
            consecutive_passes: 1,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
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
            advancement_tokens: 0,
            card: CardId("ice_wall".to_string()),
            server: ServerId::Hq,
            slot: InstallSlot::Ice,
            rezzed: false,
        }];
        let mut state = runner_state(3, 0, 0);
        state.corp.installed = installed;
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            ice: vec![test_ice("ice_wall", 0, 0, false)],
            position: 0,
         jack_out_permitted: false,});
        // It's the Runner's priority, but Rez is priority-independent —
        // the Corp can still act, and doing so should give the Runner a
        // fresh chance to respond (reset passes, toggle priority to Runner).
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 1,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
        });

        let (next, _events) =
            apply_action(&state, &registry(), PlayerAction::RezIce { ice_id: CardId("ice_wall".to_string()) })
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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::ApproachIce,
            // Second ICE's approach — jack_out_permitted is true because
            // the first ICE has already been passed.
            ice: vec![test_ice("ice_wall", 0, 0, true), test_ice("enigma", 0, 0, true)],
            position: 1,
         jack_out_permitted: true,});
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
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
            advancement_tokens: 0,
            card: card_id.clone(),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: false,
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
        state.active_run = Some(RunState { bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 0, true)],
            position: 0,
         jack_out_permitted: true,});

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
            slot: InstallSlot::Root,
            rezzed: false,
            advancement_tokens: 1,
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
            slot: InstallSlot::Root,
            rezzed: false,
            advancement_tokens: 0,
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
            slot: InstallSlot::Root,
            rezzed: false,
            advancement_tokens: 0,
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
        RunState { bad_publicity_credits: 0,
            server,
            phase: RunPhase::AccessingCard,
            ice: Vec::new(),
            position: 0,
            access_state: Some(run::AccessState {
                server,
                unaccessed_cards: Vec::new(),
                resolved_cards: Vec::new(),
                phase,
            }),
            jack_out_permitted: true,
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
        run::AccessPhase::PendingInteractiveTrigger { card_id: card_id.clone(), cost: Cost::Credits(2), can_pay: true }
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
            advancement_tokens: 0,
            card: CardId("pad_campaign".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: true,
        }];
        state.active_run = Some(run_accessing(ServerId::Hq, pending_choice(&card_id)));
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Corp,
            consecutive_passes: 0,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
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
        };

        for action in [
            PlayerAction::PayToAvoidAccessTrigger { card_id: card_id.clone() },
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
        state.corp.archives = vec![card_a.clone(), card_b.clone()];
        state.active_run = Some(RunState { bad_publicity_credits: 0,
            server: ServerId::Archives,
            phase: RunPhase::Success,
            ice: Vec::new(),
            position: 0,
            access_state: None,
            jack_out_permitted: true,
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
        state.active_run = Some(RunState { bad_publicity_credits: 0,
            access_state: None,
            server: ServerId::Hq,
            phase: RunPhase::EncounterIce,
            ice: vec![test_ice_of_type("ice_wall", 2, 1, true, IceType::Barrier)],
            position: 0,
            jack_out_permitted: false,
        });
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
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
        state.active_run = Some(RunState { bad_publicity_credits: 0,
            access_state: None,
            server: ServerId::Hq,
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
            position: 0,
            jack_out_permitted: false,
        });
        state.paid_ability_window = Some(PaidAbilityWindow {
            active_priority: Side::Runner,
            consecutive_passes: 0,
            return_phase: Box::new(GamePhase::Action(Side::Runner)),
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
