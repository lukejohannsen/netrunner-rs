use crate::cards::CardRegistry;
use crate::dsl::{CardId, Cost, Trigger};
use crate::rules::ability;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::state::{AccessPhase, AccessState, RunPhase, ServerId};
use crate::rules::state::{GamePhase, GameState, InstallSlot, Side};
use crate::rules::win::check_win_conditions;

/// Root (non-ICE) installs on `server` — ICE is excluded via
/// `InstalledCard::slot`, which the installing action declares explicitly
/// (see `InstallSlot`'s doc comment for why this doesn't need a full
/// `CardRegistry`). A successful run accesses these alongside whatever else
/// that server's arm below yields, since Upgrades can be installed on
/// central servers (Hq/RnD) as well as Remote ones.
fn root_installs_on(state: &GameState, server: ServerId) -> Vec<CardId> {
    state
        .corp
        .installed
        .iter()
        .filter(|installed| installed.server == server && installed.slot == InstallSlot::Root)
        .map(|installed| installed.card.clone())
        .collect()
}

/// Determine which `CardId`s become accessible when a run against `server`
/// concludes successfully. Unchanged logic from before this file's access
/// resolution became interactive — only *which* cards are accessed, not
/// what happens when they are.
fn compute_accessed_cards(state: &mut GameState, server: ServerId) -> Vec<CardId> {
    match server {
        // Real rules access one *randomly* chosen HQ card. `next_u64` is
        // `GameState`'s deterministic pseudo-random source (no external RNG,
        // per AGENTS.md's purity requirement) — the roll is reduced modulo
        // `hq.len()` to pick an index.
        ServerId::Hq => {
            let mut accessed = if state.corp.hq.is_empty() {
                Vec::new()
            } else {
                let roll = state.next_u64();
                let index = (roll as usize) % state.corp.hq.len();
                state.corp.hq.get(index).cloned().into_iter().collect()
            };
            accessed.extend(root_installs_on(state, server));
            accessed
        }
        // Real rules access one card too, but R&D isn't randomized — it's
        // drawn from a fixed deck order. `.last()` mirrors
        // `RunnerState::stack`'s "top of deck is the end of the Vec"
        // convention (see `engine.rs::draw_card_click`'s `stack.pop()`).
        ServerId::RnD => {
            let mut accessed: Vec<CardId> = state.corp.r_and_d.last().cloned().into_iter().collect();
            accessed.extend(root_installs_on(state, server));
            accessed
        }
        // Archives is fully public; a successful run accesses all of it.
        ServerId::Archives => state.corp.archives.clone(),
        ServerId::Remote(_) => root_installs_on(state, server),
    }
}

/// Builds the `AccessPhase::PendingChoice` for `card_id`, from its
/// `CardRegistry` definition (or the "unrecognized card" defaults if it
/// isn't registered — nothing stealable or trashable, so the only legal
/// resolution is `PlayerAction::PassAccessedCard`).
fn compute_pending_choice(card_id: &CardId, runner_credits: u32, registry: &CardRegistry) -> AccessPhase {
    let card_def = registry.get(card_id);
    let is_agenda = card_def.is_some_and(|c| c.agenda_points.is_some());
    let steal_cost = card_def.and_then(|c| c.steal_cost.clone());
    let mandatory_steal = is_agenda && steal_cost.is_none();
    let trash_cost = card_def.and_then(|c| c.trash_cost);
    let can_trash = trash_cost.is_some_and(|cost| runner_credits >= cost);

    AccessPhase::PendingChoice {
        card_id: card_id.clone(),
        can_trash,
        trash_cost,
        mandatory_steal,
        steal_cost,
    }
}

/// Sets `access.phase` to the `PendingChoice` computed from `card_id`'s
/// registry def, then fires its (unconditional) `Trigger::OnAccessed`
/// triggers. Does not itself emit `GameEvent::CardAccessed` — callers are
/// responsible for that, since it differs depending on whether this is a
/// card's first presentation (a fresh access) or the continuation of one
/// already announced via `AccessPhase::PendingInteractiveTrigger` (in which
/// case `CardAccessed` was already emitted once and must not repeat).
fn enter_pending_choice(
    state: &mut GameState,
    registry: &CardRegistry,
    server: ServerId,
    card_id: &CardId,
) -> Result<Vec<GameEvent>, RulesError> {
    let mut events = ability::process_card_triggers(state, registry, card_id, Trigger::OnAccessed)?;
    if let Some(finish) = finish_if_game_over(state, server, &events) {
        events.extend(finish);
        return Ok(events);
    }

    // The trigger just fired may have trashed `card_id` itself (e.g. a
    // self-trashing trap via `Effect::TrashCard(CardTarget::ThisCard)`) —
    // presenting a `PendingChoice` for a card that's already gone would let
    // the Runner "trash"/"steal" it a second time (`move_to_archives`
    // doesn't verify the card is still where it thinks, so this would
    // duplicate it into Archives). Treat a self-trash as this card's
    // resolution instead, same as an explicit `TrashAccessedCard`.
    if was_trashed(&events, card_id) {
        events.extend(advance_or_finish(state, registry, server, card_id.clone())?);
        return Ok(events);
    }

    let runner_credits = state.runner.resources.credits.0;
    let run = state.active_run.as_mut().expect("enter_pending_choice called mid-access");
    let access = run.access_state.as_mut().expect("enter_pending_choice called mid-access");
    access.phase = compute_pending_choice(card_id, runner_credits, registry);
    Ok(events)
}

/// True if `events` already trashed `card_id` — i.e. a `GameEvent::
/// CardTrashed` naming it, from a self-referencing `Effect::TrashCard(
/// CardTarget::ThisCard)`/`Cost::TrashSelf` fired while resolving this
/// card's own trigger/avoidance effects.
fn was_trashed(events: &[GameEvent], card_id: &CardId) -> bool {
    events.iter().any(|e| matches!(e, GameEvent::CardTrashed { card, .. } if card == card_id))
}

/// Like `enter_pending_choice`, but for callers (`resolve_pay_to_avoid`/
/// `resolve_decline_to_avoid`) that may have already trashed `card_id`
/// themselves (via the paid avoidance cost or the declined effects) before
/// ever reaching `enter_pending_choice` — `enter_pending_choice`'s own
/// self-trash check only sees events from *its* `Trigger::OnAccessed` call,
/// not `prior_events`.
fn enter_pending_choice_unless_self_trashed(
    state: &mut GameState,
    registry: &CardRegistry,
    server: ServerId,
    card_id: &CardId,
    prior_events: &[GameEvent],
) -> Result<Vec<GameEvent>, RulesError> {
    if was_trashed(prior_events, card_id) {
        return advance_or_finish(state, registry, server, card_id.clone());
    }
    enter_pending_choice(state, registry, server, card_id)
}

/// Presents `card_id` for access: emits `GameEvent::CardAccessed`, then
/// either parks at `AccessPhase::PendingInteractiveTrigger` (if the card's
/// registry def has an `InteractiveOnAccess` trigger — e.g. Fetal AI) or
/// goes straight to `enter_pending_choice`. The single entry point every
/// "a card is now being accessed" call site (`access_server`,
/// `resolve_select_card`, `advance_or_finish`) should use.
fn present_card_for_access(
    state: &mut GameState,
    registry: &CardRegistry,
    server: ServerId,
    card_id: &CardId,
) -> Result<Vec<GameEvent>, RulesError> {
    let mut events = vec![GameEvent::CardAccessed { card: card_id.clone(), server }];

    if let Some(interactive) = registry.get(card_id).and_then(|c| c.interactive_on_access.as_ref()) {
        let can_pay = match &interactive.cost {
            Cost::Credits(amount) => state.runner.resources.credits.0 >= *amount,
            // Other cost kinds aren't precomputed elsewhere either
            // (`resolve_steal`'s `steal_cost` handling is the same) —
            // `resolve_pay_to_avoid`'s `ability::pay_cost` call re-validates
            // affordability for every `Cost` variant regardless.
            _ => true,
        };
        let cost = interactive.cost.clone();
        let run = state.active_run.as_mut().expect("present_card_for_access called mid-access");
        let access = run.access_state.as_mut().expect("present_card_for_access called mid-access");
        access.phase = AccessPhase::PendingInteractiveTrigger { card_id: card_id.clone(), cost, can_pay };
        return Ok(events);
    }

    events.extend(enter_pending_choice(state, registry, server, card_id)?);
    Ok(events)
}

/// Determine which cards a successful run against `server` accesses and, if
/// any, park the run in `RunPhase::AccessingCard`. A single accessed card
/// goes straight to an `AccessState` describing its `PendingChoice` —
/// `PlayerAction::StealAgenda`/`TrashAccessedCard`/`PassAccessedCard`
/// (`resolve_steal`/`resolve_trash`/`resolve_pass` below) resolve it. Two or
/// more accessed cards instead park at `AccessPhase::SelectNextCard`, so the
/// Runner picks resolution order via `PlayerAction::SelectCardToAccess`
/// (`resolve_select_card` below) before any `PendingChoice` is presented. If
/// nothing is accessed (empty zone), clears `active_run` immediately instead
/// — there's nothing to present a choice about, so the run is simply over.
///
/// Takes `&mut GameState` because HQ access needs `GameState::next_u64` to
/// pick a pseudo-random index, and every outcome mutates `active_run`.
///
/// Fallible only because presenting the first accessed card can fire its
/// `Trigger::OnAccessed` effects (`ability::process_card_triggers`), which
/// can themselves error; an empty zone or a card with no matching trigger
/// still always succeeds.
pub fn access_server(
    state: &mut GameState,
    server: ServerId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let accessed = compute_accessed_cards(state, server);
    if accessed.is_empty() {
        state.active_run = None;
        return Ok(Vec::new());
    }

    let run = state
        .active_run
        .as_mut()
        .expect("engine::complete_run confirmed active_run is Some before calling access_server");
    run.phase = RunPhase::AccessingCard;

    if accessed.len() == 1 {
        let card_id = accessed.into_iter().next().unwrap();
        // Placeholder phase — `present_card_for_access` below overwrites it
        // immediately (with either `PendingInteractiveTrigger` or, via
        // `enter_pending_choice`, the real `PendingChoice`). `AccessState`
        // must exist first since both paths borrow `run.access_state.as_mut()`.
        run.access_state = Some(AccessState {
            server,
            unaccessed_cards: Vec::new(),
            resolved_cards: Vec::new(),
            phase: AccessPhase::SelectNextCard { selectable_cards: Vec::new() },
        });

        present_card_for_access(state, registry, server, &card_id)
    } else {
        run.access_state = Some(AccessState {
            server,
            unaccessed_cards: accessed.clone(),
            resolved_cards: Vec::new(),
            phase: AccessPhase::SelectNextCard { selectable_cards: accessed },
        });
        Ok(Vec::new())
    }
}

/// The `AccessState` fields `resolve_steal`/`resolve_trash`/`resolve_pass`
/// need, pulled out by value so the borrow of `state.active_run` doesn't
/// outlive the check — each caller goes on to mutate `state` afterward.
struct PendingAccess {
    server: ServerId,
    mandatory_steal: bool,
    steal_cost: Option<Cost>,
    trash_cost: Option<u32>,
}

/// Confirms a run is parked in `RunPhase::AccessingCard` awaiting a choice
/// on exactly `card_id`, and returns that choice's context. Covers every
/// "wrong state to call this" case with a single error
/// (`RulesError::NotInAccessPhase`): no active run, the run isn't
/// `AccessingCard`, or `card_id` doesn't match what's actually pending —
/// mirroring how `RulesError::NotInEncounter` already covers several
/// "not in the right run sub-state" cases at once.
fn require_pending(state: &GameState, card_id: &CardId) -> Result<PendingAccess, RulesError> {
    let run = state.active_run.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    if run.phase != RunPhase::AccessingCard {
        return Err(RulesError::NotInAccessPhase);
    }
    let access = run.access_state.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    let AccessPhase::PendingChoice { card_id: pending, mandatory_steal, steal_cost, trash_cost, .. } =
        &access.phase
    else {
        return Err(RulesError::NotInAccessPhase);
    };
    if pending != card_id {
        return Err(RulesError::NotInAccessPhase);
    }

    Ok(PendingAccess {
        server: access.server,
        mandatory_steal: *mandatory_steal,
        steal_cost: steal_cost.clone(),
        trash_cost: *trash_cost,
    })
}

/// The `AccessState` fields `resolve_select_card` needs, pulled out by value
/// for the same borrow-scoping reason as `PendingAccess`.
struct PendingSelection {
    server: ServerId,
    selectable_cards: Vec<CardId>,
}

/// Confirms a run is parked in `RunPhase::AccessingCard` awaiting a
/// selection (`AccessPhase::SelectNextCard`), and returns that choice's
/// context. Covers every "wrong state to call this" case with a single
/// error (`RulesError::NotInAccessPhase`), mirroring `require_pending`.
fn require_selectable(state: &GameState) -> Result<PendingSelection, RulesError> {
    let run = state.active_run.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    if run.phase != RunPhase::AccessingCard {
        return Err(RulesError::NotInAccessPhase);
    }
    let access = run.access_state.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    let AccessPhase::SelectNextCard { selectable_cards } = &access.phase else {
        return Err(RulesError::NotInAccessPhase);
    };

    Ok(PendingSelection { server: access.server, selectable_cards: selectable_cards.clone() })
}

/// Resolves `PlayerAction::SelectCardToAccess`. See its doc comment for the
/// error conditions.
pub fn resolve_select_card(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_selectable(state)?;
    if !pending.selectable_cards.contains(card_id) {
        return Err(RulesError::InvalidAccessSelection { card: card_id.clone() });
    }

    let run = state.active_run.as_mut().expect("resolve_select_card called mid-access");
    let access = run.access_state.as_mut().expect("resolve_select_card called mid-access");
    if let Some(pos) = access.unaccessed_cards.iter().position(|c| c == card_id) {
        access.unaccessed_cards.remove(pos);
    }

    present_card_for_access(state, registry, pending.server, card_id)
}

/// If `state.phase` became `GameOver` (e.g. a flatline mid-trigger, or an
/// agenda-point win), clears `active_run` and returns the terminal events;
/// otherwise `None`. Shared by every place in this file that fires
/// card-trigger effects capable of ending the game out from under an
/// in-progress access.
///
/// `events_so_far` is whatever's already been collected in the caller's
/// local `events` vec: some triggers (e.g. a flatlining `Effect::
/// DealDamage`, via `damage::apply_damage`) already emit their own
/// `GameEvent::GameOver` as part of their normal return value, unlike
/// `win::check_win_conditions` (which only mutates `state.phase` and emits
/// nothing) — so this only appends a fresh `GameOver` if the caller's
/// events don't already end with one, to avoid emitting it twice.
fn finish_if_game_over(
    state: &mut GameState,
    server: ServerId,
    events_so_far: &[GameEvent],
) -> Option<Vec<GameEvent>> {
    if let GamePhase::GameOver(winner) = state.phase {
        state.active_run = None;
        let mut events = Vec::new();
        if !matches!(events_so_far.last(), Some(GameEvent::GameOver { .. })) {
            events.push(GameEvent::GameOver { winner });
        }
        events.push(GameEvent::RunCompleted { server });
        Some(events)
    } else {
        None
    }
}

/// Shared tail of `resolve_steal`/`resolve_trash`/`resolve_pass`: if a steal
/// just won the game, finalize immediately without presenting further
/// accessed cards; otherwise record `resolved_card` as resolved and either
/// auto-present the last remaining card's `PendingChoice`, offer a choice
/// among 2+ remaining cards, or finalize if none remain.
fn advance_or_finish(
    state: &mut GameState,
    registry: &CardRegistry,
    server: ServerId,
    resolved_card: CardId,
) -> Result<Vec<GameEvent>, RulesError> {
    if let Some(events) = finish_if_game_over(state, server, &[]) {
        return Ok(events);
    }

    let run = state.active_run.as_mut().expect("advance_or_finish called mid-access");
    let access = run.access_state.as_mut().expect("advance_or_finish called mid-access");
    access.resolved_cards.push(resolved_card);

    match access.unaccessed_cards.len() {
        0 => {
            state.active_run = None;
            Ok(vec![GameEvent::RunCompleted { server }])
        }
        1 => {
            let next_card = access.unaccessed_cards.remove(0);
            present_card_for_access(state, registry, server, &next_card)
        }
        _ => {
            access.phase = AccessPhase::SelectNextCard { selectable_cards: access.unaccessed_cards.clone() };
            Ok(Vec::new())
        }
    }
}

/// Resolves `PlayerAction::StealAgenda`. See its doc comment for the error
/// conditions.
pub fn resolve_steal(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_pending(state, card_id)?;
    if !pending.mandatory_steal && pending.steal_cost.is_none() {
        return Err(RulesError::NotInAccessPhase);
    }

    let mut events = Vec::new();
    if let Some(cost) = &pending.steal_cost {
        if let Cost::Credits(requested) = cost {
            let available = state.runner.resources.credits.0;
            if available < *requested {
                return Err(RulesError::CannotAffordStealCost {
                    card: card_id.clone(),
                    available,
                    requested: *requested,
                });
            }
        }
        events.extend(ability::pay_cost(state, Side::Runner, cost, Some(card_id))?);
    }

    state.runner.scored_agendas.push(card_id.clone());
    let agenda_points = registry.get(card_id).and_then(|c| c.agenda_points).unwrap_or(0);
    state.runner.resources.agenda_points = state.runner.resources.agenda_points.gain(agenda_points);
    events.push(GameEvent::AgendaStolen { card: card_id.clone(), agenda_points });

    check_win_conditions(state, registry);
    events.extend(advance_or_finish(state, registry, pending.server, card_id.clone())?);
    Ok(events)
}

/// Removes `card_id` from wherever it currently lives (HQ, R&D, or a
/// Root-slot Corp install) and pushes it onto Archives — unless it was
/// already being accessed *from* Archives, in which case it's already
/// there and this is a no-op.
fn move_to_archives(state: &mut GameState, card_id: &CardId, server: ServerId) {
    if server == ServerId::Archives {
        return;
    }
    if let Some(pos) = state.corp.hq.iter().position(|c| c == card_id) {
        state.corp.hq.remove(pos);
    } else if let Some(pos) = state.corp.r_and_d.iter().position(|c| c == card_id) {
        state.corp.r_and_d.remove(pos);
    } else if let Some(pos) = state.corp.installed.iter().position(|c| &c.card == card_id) {
        state.corp.installed.remove(pos);
    }
    state.corp.archives.push(card_id.clone());
}

/// Resolves `PlayerAction::TrashAccessedCard`. See its doc comment for the
/// error conditions.
pub fn resolve_trash(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_pending(state, card_id)?;
    let cost = pending.trash_cost.ok_or(RulesError::NotInAccessPhase)?;

    let available = state.runner.resources.credits.0;
    if available < cost {
        return Err(RulesError::CannotAffordTrashCost { card: card_id.clone(), available, requested: cost });
    }

    let mut events = ability::pay_cost(state, Side::Runner, &Cost::Credits(cost), Some(card_id))?;
    move_to_archives(state, card_id, pending.server);
    events.push(GameEvent::CardTrashedFromAccess { card: card_id.clone(), cost_paid: cost });
    events.extend(ability::process_card_triggers(state, registry, card_id, Trigger::OnTrashedFromAccess)?);

    events.extend(advance_or_finish(state, registry, pending.server, card_id.clone())?);
    Ok(events)
}

/// Resolves `PlayerAction::PassAccessedCard`. See its doc comment for the
/// error conditions.
pub fn resolve_pass(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_pending(state, card_id)?;
    if pending.mandatory_steal {
        return Err(RulesError::MandatoryStealViolation { card: card_id.clone() });
    }

    let mut events = vec![GameEvent::AccessPassed { card: card_id.clone() }];
    events.extend(advance_or_finish(state, registry, pending.server, card_id.clone())?);
    Ok(events)
}

/// The `AccessState` fields `resolve_pay_to_avoid`/`resolve_decline_to_avoid`
/// need, pulled out by value for the same borrow-scoping reason as
/// `PendingAccess`.
struct PendingInteractive {
    server: ServerId,
    cost: Cost,
}

/// Confirms a run is parked in `RunPhase::AccessingCard` awaiting an
/// interactive-trigger decision (`AccessPhase::PendingInteractiveTrigger`)
/// on exactly `card_id`, and returns that decision's context. Covers every
/// "wrong state to call this" case with a single error
/// (`RulesError::NotInAccessPhase`), mirroring `require_pending`.
fn require_pending_interactive(state: &GameState, card_id: &CardId) -> Result<PendingInteractive, RulesError> {
    let run = state.active_run.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    if run.phase != RunPhase::AccessingCard {
        return Err(RulesError::NotInAccessPhase);
    }
    let access = run.access_state.as_ref().ok_or(RulesError::NotInAccessPhase)?;
    let AccessPhase::PendingInteractiveTrigger { card_id: pending, cost, .. } = &access.phase else {
        return Err(RulesError::NotInAccessPhase);
    };
    if pending != card_id {
        return Err(RulesError::NotInAccessPhase);
    }

    Ok(PendingInteractive { server: access.server, cost: cost.clone() })
}

/// Resolves `PlayerAction::PayToAvoidAccessTrigger`. See its doc comment for
/// the error conditions.
pub fn resolve_pay_to_avoid(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_pending_interactive(state, card_id)?;
    if let Cost::Credits(requested) = &pending.cost {
        let available = state.runner.resources.credits.0;
        if available < *requested {
            return Err(RulesError::CannotAffordAvoidanceCost {
                card: card_id.clone(),
                available,
                requested: *requested,
            });
        }
    }

    let mut events = ability::pay_cost(state, Side::Runner, &pending.cost, Some(card_id))?;
    let choice_events =
        enter_pending_choice_unless_self_trashed(state, registry, pending.server, card_id, &events)?;
    events.extend(choice_events);
    Ok(events)
}

/// Resolves `PlayerAction::DeclineAccessTrigger`. See its doc comment for
/// the error conditions.
pub fn resolve_decline_to_avoid(
    state: &mut GameState,
    card_id: &CardId,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = require_pending_interactive(state, card_id)?;

    let effects = registry
        .get(card_id)
        .and_then(|c| c.interactive_on_access.as_ref())
        .map(|interactive| interactive.effects.clone())
        .unwrap_or_default();

    let mut events = Vec::new();
    for effect in &effects {
        events.extend(ability::evaluate_effect(state, effect, Some(card_id))?);
    }
    if let Some(finish) = finish_if_game_over(state, pending.server, &events) {
        events.extend(finish);
        return Ok(events);
    }

    let choice_events =
        enter_pending_choice_unless_self_trashed(state, registry, pending.server, card_id, &events)?;
    events.extend(choice_events);
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{Card, CardTarget, CardType, DamageType, Effect, InteractiveOnAccess, TriggeredEffect};
    use crate::rules::run::state::RunState;
    use crate::rules::state::{
        AgendaPoints, Clicks, Credits, InstalledCard, MemoryUnits, PlayerResources, RunnerState,
        Side,
    };
    use std::collections::HashSet;

    /// An empty registry, for every test that doesn't exercise agenda
    /// scoring and so doesn't need real card definitions.
    fn registry() -> CardRegistry {
        CardRegistry::new()
    }

    /// A minimal Agenda `Card` worth `points` — everything besides id and
    /// `agenda_points` is irrelevant to these tests.
    fn agenda_card(id: &str, points: u32) -> Card {
        Card {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Agenda,
            cost: 0,
            triggers: Vec::new(),
            abilities: Vec::new(),
            trash_cost: None,
            steal_cost: None,
            advancement_requirement: Some(points),
            agenda_points: Some(points),
            min_deck_size: None,
            strength: None,
            subroutines: Vec::new(),
            interactive_on_access: None,
        }
    }

    /// A NAPD-Contract-style Agenda: worth `points`, but costs `steal_cost`
    /// credits to steal instead of being a mandatory free steal.
    fn costed_agenda_card(id: &str, points: u32, steal_cost: u32) -> Card {
        Card { steal_cost: Some(Cost::Credits(steal_cost)), ..agenda_card(id, points) }
    }

    /// A minimal non-Agenda Asset `Card` with the given `trash_cost` —
    /// everything besides id and `trash_cost` is irrelevant to these tests.
    fn trashable_card(id: &str, trash_cost: u32) -> Card {
        Card {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Asset,
            cost: 0,
            triggers: Vec::new(),
            abilities: Vec::new(),
            trash_cost: Some(trash_cost),
            steal_cost: None,
            advancement_requirement: None,
            agenda_points: None,
            min_deck_size: None,
            strength: None,
            subroutines: Vec::new(),
            interactive_on_access: None,
        }
    }

    /// A minimal non-Agenda, non-trashable Asset `Card` with an
    /// `OnAccessed` trigger firing `effects` — Snare!/Fetal AI-style traps.
    fn card_with_on_accessed(id: &str, effects: Vec<Effect>) -> Card {
        Card {
            triggers: vec![TriggeredEffect { trigger: Trigger::OnAccessed, effects }],
            trash_cost: None,
            ..trashable_card(id, 0)
        }
    }

    /// A trashable `Card` (see `trashable_card`) with an
    /// `OnTrashedFromAccess` trigger firing `effects` — Shock!-style.
    fn trashable_card_with_on_trashed_from_access(id: &str, trash_cost: u32, effects: Vec<Effect>) -> Card {
        Card {
            triggers: vec![TriggeredEffect { trigger: Trigger::OnTrashedFromAccess, effects }],
            ..trashable_card(id, trash_cost)
        }
    }

    /// A minimal non-Agenda, non-trashable Asset `Card` with an
    /// `InteractiveOnAccess` trigger — Fetal AI-style "pay `cost` to avoid
    /// `effects`."
    fn card_with_interactive_on_access(id: &str, cost: Cost, effects: Vec<Effect>) -> Card {
        Card {
            interactive_on_access: Some(InteractiveOnAccess { cost, effects }),
            trash_cost: None,
            ..trashable_card(id, 0)
        }
    }

    /// A run against `server` already in `RunPhase::Success`, ready for
    /// `access_server` to park in `AccessingCard`.
    fn run_in_success(server: ServerId) -> RunState {
        RunState { bad_publicity_credits: 0, server, phase: RunPhase::Success, ice: Vec::new(), position: 0, access_state: None , jack_out_permitted: true}
    }

    fn game_state(
        hq: Vec<CardId>,
        r_and_d: Vec<CardId>,
        archives: Vec<CardId>,
        installed: Vec<InstalledCard>,
        seed: u64,
    ) -> GameState {
        GameState {
            corp: crate::rules::state::CorpState { identity: None, bad_publicity: 0,
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                hq,
                r_and_d,
                archives,
                installed,
            },
            runner: RunnerState { identity: None,
                scored_agendas: Vec::new(),
                resources: PlayerResources {
                    credits: Credits(0),
                    clicks: Clicks(0),
                    agenda_points: AgendaPoints(0),
                },
                memory_units: MemoryUnits(0),
                brain_damage: 0,
                tags: 0,
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
                heap: Vec::new(),
                link_strength: 0,
            },
            phase: crate::rules::state::GamePhase::Action(Side::Corp),
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            seed,
            rng_step: 0,
        }
    }

    #[test]
    fn accessing_hq_with_one_card_yields_that_card() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        assert_eq!(
            access_server(&mut state, ServerId::Hq, &registry()).unwrap(),
            vec![GameEvent::CardAccessed {
                card: CardId("hedge_fund".to_string()),
                server: ServerId::Hq,
            }]
        );
        // The RNG step still advances even with only one possible index.
        assert_eq!(state.rng_step, 1);
        assert_eq!(state.active_run.unwrap().phase, RunPhase::AccessingCard);
    }

    #[test]
    fn accessing_hq_is_deterministic_for_a_given_seed() {
        let hq = vec![
            CardId("card_0".to_string()),
            CardId("card_1".to_string()),
            CardId("card_2".to_string()),
            CardId("card_3".to_string()),
            CardId("card_4".to_string()),
        ];
        let mut state_a = game_state(hq.clone(), Vec::new(), Vec::new(), Vec::new(), 42);
        state_a.active_run = Some(run_in_success(ServerId::Hq));
        let mut state_b = game_state(hq, Vec::new(), Vec::new(), Vec::new(), 42);
        state_b.active_run = Some(run_in_success(ServerId::Hq));

        let events_a = access_server(&mut state_a, ServerId::Hq, &registry()).unwrap();
        let events_b = access_server(&mut state_b, ServerId::Hq, &registry()).unwrap();

        assert_eq!(events_a, events_b);
        assert_eq!(events_a.len(), 1);
    }

    #[test]
    fn accessing_hq_yields_varied_indices_across_different_seeds() {
        let hq = vec![
            CardId("card_0".to_string()),
            CardId("card_1".to_string()),
            CardId("card_2".to_string()),
            CardId("card_3".to_string()),
            CardId("card_4".to_string()),
        ];

        let accessed_cards: HashSet<CardId> = (0..20u64)
            .map(|seed| {
                let mut state = game_state(hq.clone(), Vec::new(), Vec::new(), Vec::new(), seed);
                state.active_run = Some(run_in_success(ServerId::Hq));
                match access_server(&mut state, ServerId::Hq, &registry()).unwrap().into_iter().next() {
                    Some(GameEvent::CardAccessed { card, .. }) => card,
                    other => panic!("expected a CardAccessed event, got {other:?}"),
                }
            })
            .collect();

        assert!(
            accessed_cards.len() > 1,
            "expected varied indices across seeds, got only {accessed_cards:?}"
        );
    }

    #[test]
    fn accessing_rnd_yields_the_last_card() {
        let mut state = game_state(
            Vec::new(),
            vec![CardId("enigma".to_string()), CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::RnD));
        assert_eq!(
            access_server(&mut state, ServerId::RnD, &registry()).unwrap(),
            vec![GameEvent::CardAccessed {
                card: CardId("hedge_fund".to_string()),
                server: ServerId::RnD,
            }]
        );
    }

    #[test]
    fn accessing_hq_yields_hq_card_and_root_installed_upgrades() {
        let installed = vec![
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("ice_wall".to_string()),
                server: ServerId::Hq,
                slot: InstallSlot::Ice,
                rezzed: true,
            },
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("ash_2_0".to_string()),
                server: ServerId::Hq,
                slot: InstallSlot::Root,
                rezzed: false,
            },
        ];
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            installed,
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        // Two cards are accessed (the HQ card and the Root-installed
        // Upgrade), so nothing is presented until the Runner picks which to
        // resolve first (see
        // `multi_card_sequence_advances_through_each_card_in_order`).
        assert_eq!(access_server(&mut state, ServerId::Hq, &registry()).unwrap(), Vec::new());
        let access_state = state.active_run.unwrap().access_state.unwrap();
        assert_eq!(
            access_state.unaccessed_cards,
            vec![CardId("hedge_fund".to_string()), CardId("ash_2_0".to_string())]
        );
        assert_eq!(
            access_state.phase,
            AccessPhase::SelectNextCard {
                selectable_cards: vec![
                    CardId("hedge_fund".to_string()),
                    CardId("ash_2_0".to_string())
                ]
            }
        );
    }

    #[test]
    fn accessing_rnd_yields_rnd_card_and_root_installed_upgrades() {
        let installed = vec![
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("wraparound".to_string()),
                server: ServerId::RnD,
                slot: InstallSlot::Ice,
                rezzed: true,
            },
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("crisium_grid".to_string()),
                server: ServerId::RnD,
                slot: InstallSlot::Root,
                rezzed: false,
            },
        ];
        let mut state = game_state(
            Vec::new(),
            vec![CardId("enigma".to_string()), CardId("hedge_fund".to_string())],
            Vec::new(),
            installed,
            0,
        );
        state.active_run = Some(run_in_success(ServerId::RnD));
        assert_eq!(access_server(&mut state, ServerId::RnD, &registry()).unwrap(), Vec::new());
        assert_eq!(
            state.active_run.unwrap().access_state.unwrap().unaccessed_cards,
            vec![CardId("hedge_fund".to_string()), CardId("crisium_grid".to_string())]
        );
    }

    #[test]
    fn accessing_archives_yields_every_card_in_it() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        assert_eq!(access_server(&mut state, ServerId::Archives, &registry()).unwrap(), Vec::new());
        assert_eq!(
            state.active_run.unwrap().access_state.unwrap().unaccessed_cards,
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())]
        );
    }

    #[test]
    fn accessing_remote_skips_installed_ice_and_yields_only_root_installs() {
        let installed = vec![
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("ice_wall".to_string()),
                server: ServerId::Remote(0),
                slot: InstallSlot::Ice,
                rezzed: true,
            },
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("pad_campaign".to_string()),
                server: ServerId::Remote(0),
                slot: InstallSlot::Root,
                rezzed: false,
            },
            InstalledCard {
                advancement_tokens: 0,
                card: CardId("enigma".to_string()),
                server: ServerId::Remote(1),
                slot: InstallSlot::Ice,
                rezzed: true,
            },
        ];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        assert_eq!(
            access_server(&mut state, ServerId::Remote(0), &registry()).unwrap(),
            vec![GameEvent::CardAccessed {
                card: CardId("pad_campaign".to_string()),
                server: ServerId::Remote(0)
            }]
        );
    }

    #[test]
    fn accessing_remote_with_only_ice_yields_no_events() {
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: CardId("ice_wall".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Ice,
            rezzed: true,
        }];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        assert_eq!(access_server(&mut state, ServerId::Remote(0), &registry()).unwrap(), Vec::new());
        assert_eq!(state.active_run, None);
    }

    #[test]
    fn accessing_an_empty_zone_yields_no_events() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        assert_eq!(access_server(&mut state, ServerId::Hq, &registry()).unwrap(), Vec::new());
        assert_eq!(access_server(&mut state, ServerId::RnD, &registry()).unwrap(), Vec::new());
        assert_eq!(access_server(&mut state, ServerId::Archives, &registry()).unwrap(), Vec::new());
        assert_eq!(access_server(&mut state, ServerId::Remote(0), &registry()).unwrap(), Vec::new());
        assert_eq!(state.active_run, None);
    }

    #[test]
    fn free_agenda_access_is_a_mandatory_steal() {
        let registry = CardRegistry::from_cards(vec![agenda_card("priority_requisition", 3)]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("priority_requisition".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("priority_requisition".to_string());
        assert_eq!(
            resolve_pass(&mut state, &card_id, &registry),
            Err(RulesError::MandatoryStealViolation { card: card_id.clone() })
        );

        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");
        assert_eq!(state.runner.scored_agendas, vec![card_id.clone()]);
        assert_eq!(state.runner.resources.agenda_points, AgendaPoints(3));
        assert_eq!(state.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::AgendaStolen { card: card_id, agenda_points: 3 },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn stealing_an_agenda_that_reaches_seven_points_ends_the_game_with_a_runner_win() {
        let registry = CardRegistry::from_cards(vec![
            agenda_card("priority_requisition", 3),
            agenda_card("already_scored", 4),
        ]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("priority_requisition".to_string())],
            Vec::new(),
            0,
        );
        // Simulate having already stolen 4 points' worth of Agendas earlier
        // in the game.
        state.runner.scored_agendas = vec![CardId("already_scored".to_string())];
        state.runner.resources.agenda_points = AgendaPoints(4);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("priority_requisition".to_string());
        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");

        assert_eq!(state.runner.resources.agenda_points, AgendaPoints(7));
        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(state.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::AgendaStolen { card: card_id, agenda_points: 3 },
                GameEvent::GameOver { winner: Side::Runner },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn winning_mid_sequence_never_presents_the_next_accessed_card() {
        let registry = CardRegistry::from_cards(vec![
            agenda_card("priority_requisition", 3),
            agenda_card("hostile_takeover", 1),
            agenda_card("already_scored", 4),
        ]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![
                CardId("priority_requisition".to_string()),
                CardId("hostile_takeover".to_string()),
            ],
            Vec::new(),
            0,
        );
        state.runner.scored_agendas = vec![CardId("already_scored".to_string())];
        state.runner.resources.agenda_points = AgendaPoints(4);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("priority_requisition".to_string());
        resolve_select_card(&mut state, &card_id, &registry).expect("selecting should succeed");
        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");

        // Capped at the winning threshold, not 8 — the second agenda
        // (worth 1 more point) was never reached, and never presented.
        assert_eq!(state.runner.resources.agenda_points, AgendaPoints(7));
        assert_eq!(state.phase, GamePhase::GameOver(Side::Runner));
        assert_eq!(state.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::AgendaStolen { card: card_id, agenda_points: 3 },
                GameEvent::GameOver { winner: Side::Runner },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn costed_agenda_can_be_stolen_by_paying_its_steal_cost() {
        let registry = CardRegistry::from_cards(vec![costed_agenda_card("napd_contract", 2, 4)]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("napd_contract".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(4);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("napd_contract".to_string());
        let events = resolve_steal(&mut state, &card_id, &registry).expect("steal should succeed");

        assert_eq!(state.runner.resources.credits, Credits(0));
        assert_eq!(state.runner.scored_agendas, vec![card_id.clone()]);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 4 },
                GameEvent::AgendaStolen { card: card_id, agenda_points: 2 },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn costed_agenda_can_be_declined_when_unaffordable() {
        let registry = CardRegistry::from_cards(vec![costed_agenda_card("napd_contract", 2, 4)]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("napd_contract".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(2);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("napd_contract".to_string());
        assert_eq!(
            resolve_steal(&mut state, &card_id, &registry),
            Err(RulesError::CannotAffordStealCost { card: card_id.clone(), available: 2, requested: 4 })
        );
        // Declining is legal — this Agenda isn't a mandatory steal.
        let events = resolve_pass(&mut state, &card_id, &registry).expect("passing should succeed");

        assert!(state.runner.scored_agendas.is_empty());
        assert_eq!(state.runner.resources.credits, Credits(2));
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: card_id },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn trashing_an_installed_asset_pays_its_trash_cost_and_moves_it_to_archives() {
        let registry = CardRegistry::from_cards(vec![trashable_card("pad_campaign", 2)]);
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: CardId("pad_campaign".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: true,
        }];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.runner.resources.credits = Credits(3);
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        access_server(&mut state, ServerId::Remote(0), &registry).unwrap();

        let card_id = CardId("pad_campaign".to_string());
        let events = resolve_trash(&mut state, &card_id, &registry).expect("trash should succeed");

        assert_eq!(state.runner.resources.credits, Credits(1));
        assert!(state.corp.installed.is_empty());
        assert_eq!(state.corp.archives, vec![card_id.clone()]);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 2 },
                GameEvent::CardTrashedFromAccess { card: card_id, cost_paid: 2 },
                GameEvent::RunCompleted { server: ServerId::Remote(0) },
            ]
        );
    }

    #[test]
    fn trashing_with_insufficient_credits_errors() {
        let registry = CardRegistry::from_cards(vec![trashable_card("pad_campaign", 2)]);
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: CardId("pad_campaign".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: true,
        }];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.runner.resources.credits = Credits(1);
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        access_server(&mut state, ServerId::Remote(0), &registry).unwrap();

        let card_id = CardId("pad_campaign".to_string());
        assert_eq!(
            resolve_trash(&mut state, &card_id, &registry),
            Err(RulesError::CannotAffordTrashCost { card: card_id, available: 1, requested: 2 })
        );
        assert_eq!(state.corp.installed.len(), 1);
    }

    #[test]
    fn passing_a_non_agenda_non_trashable_card_completes_the_run() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        access_server(&mut state, ServerId::Hq, &registry()).unwrap();

        let card_id = CardId("hedge_fund".to_string());
        let events = resolve_pass(&mut state, &card_id, &registry()).expect("passing should succeed");

        assert_eq!(state.active_run, None);
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: card_id },
                GameEvent::RunCompleted { server: ServerId::Hq },
            ]
        );
    }

    #[test]
    fn multi_card_sequence_advances_through_each_card_in_order() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        let first = access_server(&mut state, ServerId::Archives, &registry()).unwrap();
        assert_eq!(first, Vec::new());
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::SelectNextCard {
                selectable_cards: vec![
                    CardId("hedge_fund".to_string()),
                    CardId("ice_wall".to_string())
                ]
            }
        );

        // Pick the second card first — order is the Runner's choice, not
        // the fixed access-determination order.
        let selected = resolve_select_card(&mut state, &CardId("ice_wall".to_string()), &registry())
            .expect("selecting the second card should succeed");
        assert_eq!(
            selected,
            vec![GameEvent::CardAccessed {
                card: CardId("ice_wall".to_string()),
                server: ServerId::Archives
            }]
        );

        let second = resolve_pass(&mut state, &CardId("ice_wall".to_string()), &registry())
            .expect("passing the selected card should succeed");
        // Only one card remains, so it auto-presents rather than offering
        // another `SelectNextCard` choice.
        assert_eq!(
            second,
            vec![
                GameEvent::AccessPassed { card: CardId("ice_wall".to_string()) },
                GameEvent::CardAccessed {
                    card: CardId("hedge_fund".to_string()),
                    server: ServerId::Archives
                },
            ]
        );
        assert_eq!(state.active_run.as_ref().unwrap().phase, RunPhase::AccessingCard);

        let last = resolve_pass(&mut state, &CardId("hedge_fund".to_string()), &registry())
            .expect("passing the last card should succeed");
        assert_eq!(
            last,
            vec![
                GameEvent::AccessPassed { card: CardId("hedge_fund".to_string()) },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
        assert_eq!(state.active_run, None);
    }

    #[test]
    fn multi_card_selection_lets_runner_pick_the_second_card_first() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry()).unwrap();

        let events = resolve_select_card(&mut state, &CardId("ice_wall".to_string()), &registry())
            .expect("selecting the second card should succeed");
        assert_eq!(
            events,
            vec![GameEvent::CardAccessed {
                card: CardId("ice_wall".to_string()),
                server: ServerId::Archives
            }]
        );
        let access_state = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(access_state.unaccessed_cards, vec![CardId("hedge_fund".to_string())]);
        assert_eq!(
            access_state.phase,
            AccessPhase::PendingChoice {
                card_id: CardId("ice_wall".to_string()),
                can_trash: false,
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
            }
        );

        let resolved = resolve_pass(&mut state, &CardId("ice_wall".to_string()), &registry())
            .expect("passing the selected card should succeed");
        assert_eq!(
            resolved,
            vec![
                GameEvent::AccessPassed { card: CardId("ice_wall".to_string()) },
                GameEvent::CardAccessed {
                    card: CardId("hedge_fund".to_string()),
                    server: ServerId::Archives
                },
            ]
        );
    }

    #[test]
    fn three_card_access_supports_out_of_order_resolution() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![
                CardId("card_1".to_string()),
                CardId("card_2".to_string()),
                CardId("card_3".to_string()),
            ],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry()).unwrap();

        resolve_select_card(&mut state, &CardId("card_3".to_string()), &registry())
            .expect("selecting card_3 should succeed");
        resolve_pass(&mut state, &CardId("card_3".to_string()), &registry())
            .expect("passing card_3 should succeed");

        // Two cards remain, so the Runner is offered another choice rather
        // than auto-advancing.
        let access_state = state.active_run.as_ref().unwrap().access_state.as_ref().unwrap();
        assert_eq!(
            access_state.phase,
            AccessPhase::SelectNextCard {
                selectable_cards: vec![CardId("card_1".to_string()), CardId("card_2".to_string())]
            }
        );
        assert_eq!(access_state.resolved_cards, vec![CardId("card_3".to_string())]);

        resolve_select_card(&mut state, &CardId("card_1".to_string()), &registry())
            .expect("selecting card_1 should succeed");
        let resolved = resolve_pass(&mut state, &CardId("card_1".to_string()), &registry())
            .expect("passing card_1 should succeed");

        // Only card_2 remains, so it auto-presents.
        assert_eq!(
            resolved,
            vec![
                GameEvent::AccessPassed { card: CardId("card_1".to_string()) },
                GameEvent::CardAccessed {
                    card: CardId("card_2".to_string()),
                    server: ServerId::Archives
                },
            ]
        );
    }

    #[test]
    fn selecting_the_final_remaining_card_bypasses_select_next_card() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry()).unwrap();
        resolve_select_card(&mut state, &CardId("hedge_fund".to_string()), &registry())
            .expect("selecting should succeed");

        let events = resolve_pass(&mut state, &CardId("hedge_fund".to_string()), &registry())
            .expect("passing should succeed");

        // With exactly one card left in `unaccessed_cards`, it goes
        // straight to `PendingChoice` instead of another `SelectNextCard`.
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: CardId("hedge_fund".to_string()) },
                GameEvent::CardAccessed {
                    card: CardId("ice_wall".to_string()),
                    server: ServerId::Archives
                },
            ]
        );
        assert!(matches!(
            state.active_run.unwrap().access_state.unwrap().phase,
            AccessPhase::PendingChoice { .. }
        ));
    }

    #[test]
    fn selecting_a_card_not_in_selectable_cards_errors() {
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("ice_wall".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry()).unwrap();

        let wrong_id = CardId("wrong_card".to_string());
        assert_eq!(
            resolve_select_card(&mut state, &wrong_id, &registry()),
            Err(RulesError::InvalidAccessSelection { card: wrong_id })
        );
    }

    #[test]
    fn selecting_while_already_at_pending_choice_errors() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        access_server(&mut state, ServerId::Hq, &registry()).unwrap();

        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(
            resolve_select_card(&mut state, &card_id, &registry()),
            Err(RulesError::NotInAccessPhase)
        );
    }

    #[test]
    fn selecting_with_no_active_run_errors() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(
            resolve_select_card(&mut state, &card_id, &registry()),
            Err(RulesError::NotInAccessPhase)
        );
    }

    #[test]
    fn resolving_with_a_card_id_that_does_not_match_the_pending_card_errors() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        access_server(&mut state, ServerId::Hq, &registry()).unwrap();

        let wrong_id = CardId("wrong_card".to_string());
        assert_eq!(resolve_pass(&mut state, &wrong_id, &registry()), Err(RulesError::NotInAccessPhase));
    }

    #[test]
    fn resolving_with_no_active_run_errors() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(resolve_pass(&mut state, &card_id, &registry()), Err(RulesError::NotInAccessPhase));
    }

    #[test]
    fn stealing_a_non_agenda_errors() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        access_server(&mut state, ServerId::Hq, &registry()).unwrap();

        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(resolve_steal(&mut state, &card_id, &registry()), Err(RulesError::NotInAccessPhase));
    }

    #[test]
    fn trashing_a_non_trashable_card_errors() {
        let mut state = game_state(
            vec![CardId("hedge_fund".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            42,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));
        access_server(&mut state, ServerId::Hq, &registry()).unwrap();

        let card_id = CardId("hedge_fund".to_string());
        assert_eq!(resolve_trash(&mut state, &card_id, &registry()), Err(RulesError::NotInAccessPhase));
    }

    #[test]
    fn accessing_a_trap_card_deals_damage_via_on_accessed_trigger() {
        let registry =
            CardRegistry::from_cards(vec![card_with_on_accessed("snare", vec![Effect::DealDamage(DamageType::Net, 2)])]);
        let mut state =
            game_state(Vec::new(), Vec::new(), vec![CardId("snare".to_string())], Vec::new(), 0);
        state.runner.grip = vec![
            CardId("card_a".to_string()),
            CardId("card_b".to_string()),
            CardId("card_c".to_string()),
        ];
        state.active_run = Some(run_in_success(ServerId::Archives));

        let events = access_server(&mut state, ServerId::Archives, &registry).unwrap();

        assert_eq!(state.runner.grip.len(), 1);
        assert_eq!(state.runner.heap.len(), 2);
        assert_eq!(
            events[0],
            GameEvent::CardAccessed { card: CardId("snare".to_string()), server: ServerId::Archives }
        );
        assert_eq!(events[1], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 2 });
        assert_eq!(events.len(), 4);
    }

    #[test]
    fn trashing_a_card_fires_on_trashed_from_access_trigger() {
        let registry = CardRegistry::from_cards(vec![trashable_card_with_on_trashed_from_access(
            "shock",
            2,
            vec![Effect::DealDamage(DamageType::Net, 1)],
        )]);
        let installed = vec![InstalledCard {
            advancement_tokens: 0,
            card: CardId("shock".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: true,
        }];
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), installed, 0);
        state.runner.resources.credits = Credits(3);
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Remote(0)));
        access_server(&mut state, ServerId::Remote(0), &registry).unwrap();

        let card_id = CardId("shock".to_string());
        let events = resolve_trash(&mut state, &card_id, &registry).expect("trash should succeed");

        assert_eq!(state.runner.grip.len(), 1);
        assert_eq!(events[0], GameEvent::CreditsSpent { side: Side::Runner, amount: 2 });
        assert_eq!(
            events[1],
            GameEvent::CardTrashedFromAccess { card: card_id.clone(), cost_paid: 2 }
        );
        assert_eq!(events[2], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 1 });
        assert!(matches!(events[3], GameEvent::CardDiscarded { side: Side::Runner, .. }));
        assert_eq!(events[4], GameEvent::RunCompleted { server: ServerId::Remote(0) });
        assert_eq!(events.len(), 5);
    }

    #[test]
    fn on_trashed_from_access_trigger_does_not_fire_on_steal_or_pass() {
        // A trashable, non-Agenda card with an `OnTrashedFromAccess`
        // trigger — passing it (rather than trashing it) must not fire it.
        let registry = CardRegistry::from_cards(vec![trashable_card_with_on_trashed_from_access(
            "shock",
            2,
            vec![Effect::DealDamage(DamageType::Net, 5)],
        )]);
        let mut state =
            game_state(Vec::new(), Vec::new(), vec![CardId("shock".to_string())], Vec::new(), 0);
        state.runner.grip = vec![CardId("card_a".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("shock".to_string());
        let events = resolve_pass(&mut state, &card_id, &registry).expect("passing should succeed");

        // Had the trigger fired, 5 net damage against a 1-card grip would
        // have flatlined the Runner (and ended the game).
        assert_eq!(state.runner.grip.len(), 1);
        assert_ne!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: card_id },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn on_accessed_flatline_clears_active_run_and_halts_further_access() {
        let registry = CardRegistry::from_cards(vec![card_with_on_accessed(
            "snare",
            vec![Effect::DealDamage(DamageType::Net, 5)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("snare".to_string()), CardId("hedge_fund".to_string())],
            Vec::new(),
            0,
        );
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let events = resolve_select_card(&mut state, &CardId("snare".to_string()), &registry)
            .expect("selecting should succeed");

        assert_eq!(state.active_run, None);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(
            events,
            vec![
                GameEvent::CardAccessed { card: CardId("snare".to_string()), server: ServerId::Archives },
                GameEvent::RunnerFlatlined,
                GameEvent::GameOver { winner: Side::Corp },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );

        // The second (never-presented) card's own trigger effects never ran
        // — no leftover access state to resolve against, so any further
        // access action now fails as "no active run" rather than
        // "not in access phase" for a card that's still nominally pending.
        assert_eq!(
            resolve_pass(&mut state, &CardId("hedge_fund".to_string()), &registry),
            Err(RulesError::NotInAccessPhase)
        );
    }

    #[test]
    fn on_accessed_flatline_via_advance_or_finish_auto_bypass() {
        // Two cards; the first has no trigger and is passed normally, which
        // auto-bypasses straight to the second (only one remains) via
        // `advance_or_finish`'s `1 =>` arm — the one hook point distinct
        // from `access_server`/`resolve_select_card`.
        let registry = CardRegistry::from_cards(vec![card_with_on_accessed(
            "snare",
            vec![Effect::DealDamage(DamageType::Net, 5)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("snare".to_string())],
            Vec::new(),
            0,
        );
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();
        resolve_select_card(&mut state, &CardId("hedge_fund".to_string()), &registry)
            .expect("selecting the first card should succeed");

        let events = resolve_pass(&mut state, &CardId("hedge_fund".to_string()), &registry)
            .expect("passing the first card should succeed");

        assert_eq!(state.active_run, None);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: CardId("hedge_fund".to_string()) },
                GameEvent::CardAccessed { card: CardId("snare".to_string()), server: ServerId::Archives },
                GameEvent::RunnerFlatlined,
                GameEvent::GameOver { winner: Side::Corp },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn accessing_a_card_with_interactive_on_access_pauses_at_pending_interactive_trigger() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(5);
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));

        let events = access_server(&mut state, ServerId::Archives, &registry).unwrap();

        assert_eq!(
            events,
            vec![GameEvent::CardAccessed { card: CardId("fetal_ai".to_string()), server: ServerId::Archives }]
        );
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingInteractiveTrigger {
                card_id: CardId("fetal_ai".to_string()),
                cost: Cost::Credits(4),
                can_pay: true,
            }
        );
        // No damage taken yet — the effect hasn't resolved.
        assert_eq!(state.runner.grip.len(), 2);
    }

    #[test]
    fn pay_to_avoid_deducts_cost_skips_effects_and_proceeds_to_pending_choice() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(5);
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("fetal_ai".to_string());
        let events = resolve_pay_to_avoid(&mut state, &card_id, &registry).expect("paying should succeed");

        assert_eq!(state.runner.resources.credits, Credits(1));
        // No damage — the effect was avoided.
        assert_eq!(state.runner.grip.len(), 2);
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingChoice {
                card_id: card_id.clone(),
                can_trash: false,
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
            }
        );
        assert_eq!(events, vec![GameEvent::CreditsSpent { side: Side::Runner, amount: 4 }]);

        // The normal choice is still reachable afterward.
        let pass_events = resolve_pass(&mut state, &card_id, &registry).expect("pass should succeed");
        assert_eq!(
            pass_events,
            vec![
                GameEvent::AccessPassed { card: card_id },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn decline_to_avoid_applies_effects_and_proceeds_to_pending_choice() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(5);
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("fetal_ai".to_string());
        let events =
            resolve_decline_to_avoid(&mut state, &card_id, &registry).expect("declining should succeed");

        // Credits untouched, but the 2 net damage landed.
        assert_eq!(state.runner.resources.credits, Credits(5));
        assert_eq!(state.runner.grip.len(), 0);
        assert_eq!(state.runner.heap.len(), 2);
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingChoice {
                card_id: card_id.clone(),
                can_trash: false,
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
            }
        );
        assert!(matches!(events[0], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 2 }));
    }

    #[test]
    fn pay_to_avoid_with_insufficient_credits_errors_and_leaves_state_untouched() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(2);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("fetal_ai".to_string());
        assert_eq!(
            resolve_pay_to_avoid(&mut state, &card_id, &registry),
            Err(RulesError::CannotAffordAvoidanceCost { card: card_id.clone(), available: 2, requested: 4 })
        );

        // Untouched: still credits 2, still pending the same interactive trigger.
        assert_eq!(state.runner.resources.credits, Credits(2));
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingInteractiveTrigger { card_id, cost: Cost::Credits(4), can_pay: false }
        );
    }

    #[test]
    fn resolving_interactive_trigger_actions_against_the_wrong_state_errors_not_in_access_phase() {
        let mut state = game_state(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
        let card_id = CardId("fetal_ai".to_string());

        assert_eq!(
            resolve_pay_to_avoid(&mut state, &card_id, &registry()),
            Err(RulesError::NotInAccessPhase)
        );
        assert_eq!(
            resolve_decline_to_avoid(&mut state, &card_id, &registry()),
            Err(RulesError::NotInAccessPhase)
        );

        // Also errors when a *different* card is actually pending.
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(5);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let wrong_card = CardId("not_pending".to_string());
        assert_eq!(
            resolve_pay_to_avoid(&mut state, &wrong_card, &registry),
            Err(RulesError::NotInAccessPhase)
        );
        assert_eq!(
            resolve_decline_to_avoid(&mut state, &wrong_card, &registry),
            Err(RulesError::NotInAccessPhase)
        );
    }

    #[test]
    fn decline_to_avoid_flatlining_ends_the_game_and_skips_pending_choice() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 5)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(0);
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("fetal_ai".to_string());
        let events =
            resolve_decline_to_avoid(&mut state, &card_id, &registry).expect("declining should succeed");

        assert_eq!(state.active_run, None);
        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(
            events,
            vec![
                GameEvent::RunnerFlatlined,
                GameEvent::GameOver { winner: Side::Corp },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn ordinary_on_accessed_cards_are_unaffected_by_the_interactive_trigger_refactor() {
        let registry = CardRegistry::from_cards(vec![card_with_on_accessed(
            "snare",
            vec![Effect::GiveTags(1)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("snare".to_string())],
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Archives));

        let events = access_server(&mut state, ServerId::Archives, &registry).unwrap();

        assert_eq!(state.runner.tags, 1, "OnAccessed still fires unconditionally for non-interactive cards");
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingChoice {
                card_id: CardId("snare".to_string()),
                can_trash: false,
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
            }
        );
        assert_eq!(
            events,
            vec![
                GameEvent::CardAccessed { card: CardId("snare".to_string()), server: ServerId::Archives },
                GameEvent::TagsGiven { side: Side::Runner, amount: 1 },
            ]
        );
    }

    /// A minimal non-Agenda, non-trashable Asset `Card` with both an
    /// `InteractiveOnAccess` trigger and a normal `OnAccessed` trigger —
    /// proves the two compose (the normal trigger still fires once the
    /// interactive decision resolves).
    fn card_with_interactive_and_on_accessed(
        id: &str,
        cost: Cost,
        avoided_effects: Vec<Effect>,
        on_accessed_effects: Vec<Effect>,
    ) -> Card {
        Card {
            interactive_on_access: Some(InteractiveOnAccess { cost, effects: avoided_effects }),
            triggers: vec![TriggeredEffect { trigger: Trigger::OnAccessed, effects: on_accessed_effects }],
            trash_cost: None,
            ..trashable_card(id, 0)
        }
    }

    #[test]
    fn interactive_on_access_composes_with_a_normal_on_accessed_trigger() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_and_on_accessed(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
            vec![Effect::GiveTags(1)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(5);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("fetal_ai".to_string());
        let events = resolve_pay_to_avoid(&mut state, &card_id, &registry).expect("paying should succeed");

        // The avoided damage never landed, but the normal OnAccessed trigger
        // still fired once the interactive decision resolved.
        assert_eq!(state.runner.resources.credits, Credits(1));
        assert_eq!(state.runner.tags, 1);
        assert_eq!(
            events,
            vec![
                GameEvent::CreditsSpent { side: Side::Runner, amount: 4 },
                GameEvent::TagsGiven { side: Side::Runner, amount: 1 },
            ]
        );
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingChoice {
                card_id,
                can_trash: false,
                trash_cost: None,
                mandatory_steal: false,
                steal_cost: None,
            }
        );
    }

    /// An Agenda (see `agenda_card`) with an `InteractiveOnAccess` trigger —
    /// Fetal AI's actual card shape (a damage trap that's also an Agenda).
    fn agenda_with_interactive_on_access(id: &str, points: u32, cost: Cost, effects: Vec<Effect>) -> Card {
        Card { interactive_on_access: Some(InteractiveOnAccess { cost, effects }), ..agenda_card(id, points) }
    }

    #[test]
    fn interactive_on_access_composes_with_mandatory_steal_on_an_agenda() {
        let registry = CardRegistry::from_cards(vec![agenda_with_interactive_on_access(
            "fetal_ai",
            2,
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(0);
        state.runner.grip = vec![CardId("card_a".to_string()), CardId("card_b".to_string())];
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        let card_id = CardId("fetal_ai".to_string());
        // Can't afford to pay — decline, taking the damage.
        resolve_decline_to_avoid(&mut state, &card_id, &registry).expect("declining should succeed");
        assert_eq!(state.runner.grip.len(), 0);
        assert_eq!(state.runner.heap.len(), 2);

        // The normal Agenda choice is still reachable afterward, and is a
        // mandatory steal (no steal_cost).
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingChoice {
                card_id: card_id.clone(),
                can_trash: false,
                trash_cost: None,
                mandatory_steal: true,
                steal_cost: None,
            }
        );
        assert_eq!(
            resolve_pass(&mut state, &card_id, &registry),
            Err(RulesError::MandatoryStealViolation { card: card_id.clone() })
        );

        let events = resolve_steal(&mut state, &card_id, &registry).expect("stealing should succeed");
        assert_eq!(state.runner.scored_agendas, vec![card_id.clone()]);
        assert_eq!(
            events,
            vec![
                GameEvent::AgendaStolen { card: card_id, agenda_points: 2 },
                GameEvent::RunCompleted { server: ServerId::Archives },
            ]
        );
    }

    #[test]
    fn interactive_on_access_on_the_second_of_a_multi_card_access() {
        let registry = CardRegistry::from_cards(vec![card_with_interactive_on_access(
            "fetal_ai",
            Cost::Credits(4),
            vec![Effect::DealDamage(DamageType::Net, 2)],
        )]);
        let mut state = game_state(
            Vec::new(),
            Vec::new(),
            vec![CardId("hedge_fund".to_string()), CardId("fetal_ai".to_string())],
            Vec::new(),
            0,
        );
        state.runner.resources.credits = Credits(5);
        state.active_run = Some(run_in_success(ServerId::Archives));
        access_server(&mut state, ServerId::Archives, &registry).unwrap();

        // Pick the plain card first, then pass it — auto-advancing to the
        // second (and last) card, which carries the interactive trigger.
        resolve_select_card(&mut state, &CardId("hedge_fund".to_string()), &registry)
            .expect("selecting should succeed");
        let events = resolve_pass(&mut state, &CardId("hedge_fund".to_string()), &registry)
            .expect("passing should succeed");

        assert_eq!(
            events,
            vec![
                GameEvent::AccessPassed { card: CardId("hedge_fund".to_string()) },
                GameEvent::CardAccessed { card: CardId("fetal_ai".to_string()), server: ServerId::Archives },
            ]
        );
        assert_eq!(
            state.active_run.as_ref().unwrap().access_state.as_ref().unwrap().phase,
            AccessPhase::PendingInteractiveTrigger {
                card_id: CardId("fetal_ai".to_string()),
                cost: Cost::Credits(4),
                can_pay: true,
            }
        );
    }

    #[test]
    fn self_trashing_trap_trashes_itself_exactly_once_without_breaking_the_access_loop() {
        // HQ (not Archives) — the card must actually move zones (hq ->
        // archives) for the self-trash to be observable at all.
        let registry = CardRegistry::from_cards(vec![card_with_on_accessed(
            "shock_ish",
            vec![Effect::GiveTags(1), Effect::TrashCard(CardTarget::ThisCard)],
        )]);
        let mut state = game_state(
            vec![CardId("shock_ish".to_string())],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            0,
        );
        state.active_run = Some(run_in_success(ServerId::Hq));

        let events = access_server(&mut state, ServerId::Hq, &registry).unwrap();

        assert_eq!(state.runner.tags, 1);
        assert!(state.corp.hq.is_empty());
        // Exactly one copy — not duplicated by a stale PendingChoice being
        // acted on afterward.
        assert_eq!(state.corp.archives, vec![CardId("shock_ish".to_string())]);
        assert_eq!(state.active_run, None, "the run should complete, not hang on a phantom PendingChoice");
        assert_eq!(
            events,
            vec![
                GameEvent::CardAccessed { card: CardId("shock_ish".to_string()), server: ServerId::Hq },
                GameEvent::TagsGiven { side: Side::Runner, amount: 1 },
                GameEvent::CardTrashed { side: Side::Corp, card: CardId("shock_ish".to_string()) },
                GameEvent::RunCompleted { server: ServerId::Hq },
            ]
        );
    }
}
