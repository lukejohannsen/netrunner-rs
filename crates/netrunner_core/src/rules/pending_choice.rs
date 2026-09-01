//! Resolution for `Effect::OfferPaidChoice`/`Effect::PresentChoice`'s parked
//! decisions (`state::PendingPaidChoice`/`PendingDecision`) — the
//! non-run-scoped sibling of `run::access`'s `PendingInteractiveTrigger`
//! resolution, and structurally parallel to `trace.rs`'s bid resolution: a
//! decision parked by `ability::evaluate_effect`, resumed by a later,
//! dedicated `PlayerAction`.

use crate::cards::CardRegistry;
use crate::dsl::{card_matches_filter, CardFilter, CardId, CardZoneRef, Cost, Effect};
use crate::rules::ability;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::paid_ability;
use crate::rules::run;
use crate::rules::state::{ArchivedCard, GameState, InstallId, InstallSlot, PendingChoiceResume, PendingDecision, PendingPaidChoiceResume, Side};

/// Who `state.pending_decision` is currently awaiting a choice from, if
/// anything is parked — used by `engine::apply_action`'s blocking guard and
/// `legal_actions::action_owner`.
pub(crate) fn pending_decision_chooser(state: &GameState) -> Option<Side> {
    match state.pending_decision.as_ref()? {
        PendingDecision::ChooseEffect { chooser, .. } => Some(*chooser),
        PendingDecision::ChooseCards { side, .. } => Some(*side),
        PendingDecision::ChooseServer { chooser, .. } => Some(*chooser),
        PendingDecision::ChooseTriggerOrder { chooser, .. } => Some(*chooser),
    }
}

/// Marks a just-parked `GameState::pending_decision` so its eventual
/// resolution knows to resume `ability::resolve_unbroken_subroutines` —
/// mirrors the analogous marking `resolve_unbroken_subroutines` already
/// does for `active_trace`/`pending_prevention`/`pending_paid_choice`. A
/// no-op if nothing is currently parked.
pub(crate) fn mark_pending_decision_resume_subroutines(state: &mut GameState) {
    match state.pending_decision.as_mut() {
        Some(PendingDecision::ChooseEffect { resume, .. })
        | Some(PendingDecision::ChooseCards { resume, .. })
        | Some(PendingDecision::ChooseServer { resume, .. })
        | Some(PendingDecision::ChooseTriggerOrder { resume, .. }) => {
            *resume = PendingChoiceResume::ResumeSubroutines
        }
        None => {}
    }
}

/// Which side owns the physical zone `zone` refers to, given `chooser`
/// (relative to whom "Own"/"Opponent" are resolved).
fn owning_side(chooser: Side, zone: &CardZoneRef) -> Side {
    match zone {
        CardZoneRef::OpponentInstalled | CardZoneRef::OpponentDiscard => chooser.other(),
        _ => chooser,
    }
}

/// The raw `CardId`s physically present in `zone`, from `chooser`'s
/// perspective, unfiltered — e.g. every card in HQ regardless of
/// `CardFilter`. Used by `action_mask.rs` to positionally encode/decode
/// `ToggleCardSelection` from `state` alone (it has no `CardRegistry` to
/// apply a filter with — see that module's own doc comment on this exact
/// "encode potential positions, defer correctness to the `legal_actions`
/// probe" convention), and as `eligible_cards`'s unfiltered base.
pub(crate) fn zone_card_ids(state: &GameState, chooser: Side, zone: &CardZoneRef) -> Vec<CardId> {
    let owner = owning_side(chooser, zone);
    match zone {
        CardZoneRef::OwnHq => state.corp.hq.clone(),
        CardZoneRef::OwnArchives => state.corp.archives.iter().map(|a| a.card.clone()).collect(),
        CardZoneRef::OwnRAndD => state.corp.r_and_d.clone(),
        CardZoneRef::OwnStack => state.runner.stack.clone(),
        CardZoneRef::OwnGrip => state.runner.grip.clone(),
        CardZoneRef::OwnHeap => state.runner.heap.clone(),
        CardZoneRef::OpponentDiscard => match owner {
            Side::Corp => state.corp.archives.iter().map(|a| a.card.clone()).collect(),
            Side::Runner => state.runner.heap.clone(),
        },
        CardZoneRef::OpponentInstalled | CardZoneRef::OwnInstalled => match owner {
            Side::Corp => state.corp.installed.iter().map(|c| c.card.clone()).collect(),
            Side::Runner => state.runner.rig.iter().map(|c| c.card.clone()).collect(),
        },
    }
}

/// The `InstallId`s in `zone`, positionally aligned with `zone_card_ids`,
/// or `None` for a zone that holds no installs.
///
/// Exists so `ConfirmCardSelection` can hand an install-addressing `then`
/// effect (`SwapInstalledIce`, `RezInstalledIgnoringCost`) the *install*
/// the chooser picked, rather than re-deriving one from its `CardId` —
/// which cannot tell two copies of a card apart.
pub(crate) fn zone_install_ids(state: &GameState, chooser: Side, zone: &CardZoneRef) -> Option<Vec<InstallId>> {
    match zone {
        CardZoneRef::OpponentInstalled | CardZoneRef::OwnInstalled => match owning_side(chooser, zone) {
            Side::Corp => Some(state.corp.installed.iter().map(|c| c.install_id).collect()),
            Side::Runner => Some(state.runner.rig.iter().map(|c| c.install_id).collect()),
        },
        _ => None,
    }
}

/// The **positions** within `zone_card_ids(chooser, zone)` currently
/// eligible under `filter`. Used both to validate `ToggleCardSelection`
/// and to silently no-op `Effect::PromptChooseCards` when fewer than `min`
/// candidates exist at all.
///
/// Positions rather than `CardId`s, for the reason spelled out on
/// `PlayerAction::ToggleCardSelection`: this list is handed to the chooser
/// through `legal_actions`, and for a zone like `OpponentInstalled` it can
/// contain cards the chooser's own `ClientView` masks. A position names
/// the slot on the table without publishing what sits in it, and — unlike
/// a `CardId` — distinguishes two copies of the same card.
pub(crate) fn eligible_positions(
    state: &GameState,
    registry: &CardRegistry,
    chooser: Side,
    zone: &CardZoneRef,
    filter: &CardFilter,
) -> Vec<usize> {
    zone_card_ids(state, chooser, zone)
        .into_iter()
        .enumerate()
        .filter(|(_, id)| registry.get(id).is_some_and(|card| card_matches_filter(card, filter)))
        .filter(|(position, _)| instance_matches_filter(state, registry, chooser, zone, *position, filter))
        .map(|(position, _)| position)
        .collect()
}

/// The instance-level half of `CardFilter`, which `card_matches_filter`
/// can't answer from a `CardDefinition` alone —
/// `CardFilter::NotInstalledThisTurn` and `CardFilter::UnrezzedIce`; every
/// other variant passes through.
///
/// Takes the candidate's `position` within `zone_card_ids`, not its
/// `CardId`: both filters read per-instance state, and a find-by-`CardId`
/// answered for the *first* copy — so with one *Tithe* rezzed and a second
/// unrezzed, `UnrezzedIce` reported the wrong one, and
/// `NotInstalledThisTurn` had the same flaw for a card installed twice.
fn instance_matches_filter(
    state: &GameState,
    registry: &CardRegistry,
    chooser: Side,
    zone: &CardZoneRef,
    position: usize,
    filter: &CardFilter,
) -> bool {
    // Both filters below read per-install state, so `position` may only be
    // used to index an install list. For any other zone it is a position
    // into HQ/the grip/a deck and indexes nothing here — the card is simply
    // not installed, so neither filter can match it.
    let installed_zone = matches!(zone, CardZoneRef::OpponentInstalled | CardZoneRef::OwnInstalled);
    let corp_install = (installed_zone && owning_side(chooser, zone) == Side::Corp)
        .then(|| state.corp.installed.get(position))
        .flatten();

    match filter {
        // No Runner card needs this restriction yet; a rig card is never
        // eligible rather than silently always-eligible.
        CardFilter::NotInstalledThisTurn => corp_install.is_some_and(|c| !c.installed_this_turn),
        // Only an unrezzed installed Corp card can be a rez target. The
        // Runner has no rez state, so a rig card is never eligible.
        CardFilter::UnrezzedIce => corp_install.is_some_and(|c| !c.rezzed),
        // The state-dependent half of "could the Runner install this right
        // now": affordability, memory budget, console limit — shared with
        // `Effect::InstallRunnerCardFromGrip`'s own re-check so the offer
        // and the resolution can never disagree. Grip cards only; the
        // definition-level type half already ran in `card_matches_filter`.
        CardFilter::InstallableRunnerCard => {
            matches!(zone, CardZoneRef::OwnGrip)
                && state.runner.grip.get(position).is_some_and(|card| {
                    crate::rules::engine::can_install_runner_card_from_grip(state, registry, card)
                })
        }
        _ => true,
    }
}

/// A plain `Vec<CardId>` zone's mutable handle. Returns `None` for the
/// zones that aren't plain `CardId` vecs: `OpponentInstalled`/`OwnInstalled`
/// (see `remove_installed_card`) and the Corp's Archives, which carries a
/// facedown flag per card (`ArchivedCard`) — `archives_remove`/
/// `archives_push` handle that zone instead. Used by
/// `ConfirmCardSelection`'s resolution to remove a selected card from
/// `source` and/or push it into `destination`.
fn plain_zone_mut<'a>(state: &'a mut GameState, chooser: Side, zone: &CardZoneRef) -> Option<&'a mut Vec<CardId>> {
    let owner = owning_side(chooser, zone);
    match zone {
        CardZoneRef::OwnHq => Some(&mut state.corp.hq),
        CardZoneRef::OwnArchives => None,
        CardZoneRef::OwnRAndD => Some(&mut state.corp.r_and_d),
        CardZoneRef::OwnStack => Some(&mut state.runner.stack),
        CardZoneRef::OwnGrip => Some(&mut state.runner.grip),
        CardZoneRef::OwnHeap => Some(&mut state.runner.heap),
        CardZoneRef::OpponentDiscard => match owner {
            Side::Corp => None,
            Side::Runner => Some(&mut state.runner.heap),
        },
        CardZoneRef::OpponentInstalled | CardZoneRef::OwnInstalled => None,
    }
}

/// Whether `zone` resolves to the Corp's Archives for `chooser` — the one
/// zone `plain_zone_mut` can't hand back, since it stores `ArchivedCard`
/// (card + facedown flag) rather than a bare `CardId`.
fn is_corp_archives(chooser: Side, zone: &CardZoneRef) -> bool {
    match zone {
        CardZoneRef::OwnArchives => true,
        CardZoneRef::OpponentDiscard => owning_side(chooser, zone) == Side::Corp,
        _ => false,
    }
}

/// Takes the install `install_id` off its owner's table, returning the
/// card and whether it was public there (rezzed, or any Runner card) so
/// the caller can orient it in Archives. `None` if nothing by that id is
/// installed any more, mirroring `ability::trash_this_card`'s "already
/// gone" leniency.
///
/// Removal only — the caller pushes the card into the selection's
/// `destination`, once. This used to push into the owner's discard pile
/// itself *and* the caller then pushed into `destination`, so every card
/// trashed through a selection (Ansel 1.0, Ballista, Retribution, Above the
/// Law) arrived in the Heap or Archives twice. The card-conservation sweep
/// caught it the first time a heuristic Runner installed a program for
/// Ansel to trash. Keyed by `InstallId`, not first-matching `CardId`, so
/// with two copies installed the one the chooser pointed at is the one
/// that goes.
///
/// Returns `(card, was_public, cascade)`: a piece of ICE leaving the table
/// takes the trojans hosted on it along (`ability::
/// cascade_trash_hosted_programs`), exactly as `trash_card` does — this was
/// the one removal site that did not, leaving a hosted Botulus/Tranquilizer
/// in the rig pointing at ICE that no longer existed.
fn remove_installed_card(
    state: &mut GameState,
    chooser: Side,
    zone: &CardZoneRef,
    install_id: InstallId,
) -> Option<(CardId, bool, Vec<GameEvent>)> {
    match owning_side(chooser, zone) {
        Side::Corp => {
            let pos = state.corp.installed.iter().position(|c| c.install_id == install_id)?;
            let removed = state.corp.installed.remove(pos);
            let cascade = if removed.slot == InstallSlot::Ice {
                ability::cascade_trash_hosted_programs(state, removed.install_id)
            } else {
                Vec::new()
            };
            Some((removed.card, removed.rezzed, cascade))
        }
        Side::Runner => {
            let pos = state.runner.rig.iter().position(|c| c.install_id == install_id)?;
            Some((state.runner.rig.remove(pos).card, true, Vec::new()))
        }
    }
}

/// Whether moving a card into `zone` trashes it — the three discard piles
/// a selection can name. Decides whether `resolve_confirm_card_selection`
/// records a `GameEvent::CardTrashed`.
fn is_discard_pile(zone: &CardZoneRef) -> bool {
    matches!(zone, CardZoneRef::OwnHeap | CardZoneRef::OwnArchives | CardZoneRef::OpponentDiscard)
}

/// Fisher-Yates shuffle of `zone` (relative to `chooser`) using `GameState`'s
/// deterministic PRNG — the rolls are drawn first (immutable-length-only
/// borrow), then applied, so this never needs to borrow `state` mutably
/// twice at once.
fn shuffle_zone(state: &mut GameState, chooser: Side, zone: &CardZoneRef) {
    let archives = is_corp_archives(chooser, zone);
    let len = if archives { state.corp.archives.len() } else { plain_zone_mut(state, chooser, zone).map_or(0, |z| z.len()) };
    if len < 2 {
        return;
    }
    let rolls: Vec<(usize, usize)> =
        (1..len).rev().map(|i| (i, (state.next_u64() as usize) % (i + 1))).collect();
    if archives {
        for (i, j) in rolls {
            state.corp.archives.swap(i, j);
        }
    } else if let Some(zone) = plain_zone_mut(state, chooser, zone) {
        for (i, j) in rolls {
            zone.swap(i, j);
        }
    }
}


/// Resolves `PlayerAction::AcceptPendingPaidChoice`. `cost_option_index` only
/// matters when the pending choice's `cost` is `Cost::AnyOf` — it selects
/// which alternative to pay (`RulesError::InvalidCostChoiceIndex` if out of
/// range or missing); ignored otherwise.
pub(crate) fn resolve_accept(
    state: &mut GameState,
    registry: &CardRegistry,
    cost_option_index: Option<usize>,
) -> Result<Vec<GameEvent>, RulesError> {
    let pending = state.pending_paid_choice.take().ok_or(RulesError::NoPendingPaidChoice)?;

    let cost_to_pay = match &pending.cost {
        Cost::AnyOf(options) => {
            let index = cost_option_index.ok_or(RulesError::InvalidCostChoiceIndex(0))?;
            options.get(index).ok_or(RulesError::InvalidCostChoiceIndex(index))?.clone()
        }
        other => other.clone(),
    };

    let cost_events = ability::pay_cost_ctx(state, pending.side, &cost_to_pay, &ability::ResolutionContext::for_parked(pending.source_install, pending.source_card.as_ref()))?;
    // A tag paid as a cost (`Cost::TakeTags`, Funhouse's "end the run
    // unless the Runner takes 1 tag") is still the Runner taking a tag:
    // NBN: Reality Plus's `Trigger::OnTagsGiven` must fire for it, exactly
    // as it does when `Effect::GiveTags` dispatches its own event. Only the
    // *cost's* `TagsGiven` events are dispatched here — an `if_paid` that
    // gives tags dispatches for itself inside `evaluate_effect` — and the
    // dispatch runs after `if_paid` so the choice's own effect resolves
    // first; if that effect parked something, the `TagsGiven` arm's
    // `fire_each` queues the reaction rather than firing under it.
    let cost_tag_events: Vec<GameEvent> =
        cost_events.iter().filter(|e| matches!(e, GameEvent::TagsGiven { .. })).cloned().collect();
    let mut events = cost_events;
    events.push(GameEvent::PendingPaidChoiceAccepted { side: pending.side });
    events.extend(ability::evaluate_effect(state, &pending.if_paid, &mut ability::ResolutionContext::for_parked(pending.source_install, pending.source_card.as_ref()), registry)?);
    for tag_event in cost_tag_events {
        events.extend(crate::rules::dispatcher::dispatch_event(state, registry, &tag_event)?);
    }

    if pending.resume == PendingPaidChoiceResume::ResumeSubroutines {
        // Same nested-parking propagation as `resolve_choice` — `if_paid`
        // may itself park a further decision/paid choice.
        mark_pending_decision_resume_subroutines(state);
        if let Some(new_pending) = state.pending_paid_choice.as_mut() {
            new_pending.resume = PendingPaidChoiceResume::ResumeSubroutines;
        }
        events.extend(paid_ability::resolve_encounter_ice(state, registry)?);
    }
    Ok(events)
}

/// Resolves `PlayerAction::DeclinePendingPaidChoice` — no cost is paid;
/// `if_declined` resolves instead.
pub(crate) fn resolve_decline(state: &mut GameState, registry: &CardRegistry) -> Result<Vec<GameEvent>, RulesError> {
    let pending = state.pending_paid_choice.take().ok_or(RulesError::NoPendingPaidChoice)?;

    let mut events = vec![GameEvent::PendingPaidChoiceDeclined { side: pending.side }];
    events.extend(ability::evaluate_effect(state, &pending.if_declined, &mut ability::ResolutionContext::for_parked(pending.source_install, pending.source_card.as_ref()), registry)?);

    if pending.resume == PendingPaidChoiceResume::ResumeSubroutines {
        // Same nested-parking propagation as `resolve_choice` — `if_declined`
        // may itself park a further decision/paid choice.
        mark_pending_decision_resume_subroutines(state);
        if let Some(new_pending) = state.pending_paid_choice.as_mut() {
            new_pending.resume = PendingPaidChoiceResume::ResumeSubroutines;
        }
        events.extend(paid_ability::resolve_encounter_ice(state, registry)?);
    }
    Ok(events)
}

/// Resolves `PlayerAction::ResolvePendingChoice`.
pub(crate) fn resolve_choice(
    state: &mut GameState,
    registry: &CardRegistry,
    option_index: usize,
) -> Result<Vec<GameEvent>, RulesError> {
    let PendingDecision::ChooseEffect { chooser, options, source_card, source_install, resume } =
        state.pending_decision.take().ok_or(RulesError::NoPendingDecision)?
    else {
        return Err(RulesError::NoPendingDecision);
    };

    let effect = options.get(option_index).ok_or(RulesError::InvalidChoiceIndex(option_index))?.clone();
    let mut events = vec![GameEvent::PendingChoiceResolved { chooser, option_index }];
    events.extend(ability::evaluate_effect(state, &effect, &mut ability::ResolutionContext::for_parked(source_install, source_card.as_ref()), registry)?);

    if resume == PendingChoiceResume::ResumeSubroutines {
        // The chosen `effect` may itself have parked a *further* pending
        // decision or paid choice (e.g. Ansel 1.0's second subroutine:
        // choose HQ-or-Archives via this `ChooseEffect`, then choose which
        // specific card via a freshly-parked `ChooseCards`) — propagate
        // the "resume subroutines once fully resolved" intent onto it
        // rather than losing it. Harmless when nothing new was parked:
        // `resolve_encounter_ice` below just no-ops in that case.
        mark_pending_decision_resume_subroutines(state);
        if let Some(pending) = state.pending_paid_choice.as_mut() {
            pending.resume = PendingPaidChoiceResume::ResumeSubroutines;
        }
        events.extend(paid_ability::resolve_encounter_ice(state, registry)?);
    }
    Ok(events)
}

/// Resolves `PlayerAction::ChooseTriggerToResolve`: fires `card`'s share
/// of a `PendingDecision::ChooseTriggerOrder` and re-parks the rest.
///
/// The remainder goes back onto `pending_decision` while 2 or more are
/// left, and onto `deferred_triggers` when only 1 is — at that point
/// there's no order left to choose, so `engine::apply_action`'s drain
/// fires it with no further decision. That's what stops a run of N
/// simultaneous triggers from costing N decisions instead of N-1.
///
/// Firing the chosen trigger may itself park something; the remainder is
/// queued *before* firing so it survives that, and the drain picks it up
/// once the new blockage clears.
pub(crate) fn resolve_choose_trigger_to_resolve(
    state: &mut GameState,
    registry: &CardRegistry,
    index: usize,
) -> Result<Vec<GameEvent>, RulesError> {
    let Some(PendingDecision::ChooseTriggerOrder { chooser, pending, resume }) = state.pending_decision.take() else {
        return Err(RulesError::NoPendingDecision);
    };

    // Range-check before `take()`'s consequences become permanent: an `Err`
    // discards the cloned `next` in `apply_action`, so the decision is
    // still parked on the caller's state. Keyed by position, not
    // `CardId` — see `PlayerAction::ChooseTriggerToResolve`.
    if index >= pending.len() {
        return Err(RulesError::TriggerChoiceOutOfRange { index, pending: pending.len() });
    }
    let mut remaining = pending;
    let chosen = remaining.remove(index);

    // Queue the remainder before firing, so a parking `chosen` can't strand
    // it: either it re-parks as a decision (2+ left) or the drain takes it.
    if remaining.len() >= 2 {
        state.pending_decision = Some(PendingDecision::ChooseTriggerOrder { chooser, pending: remaining, resume });
    } else {
        state.deferred_triggers.splice(0..0, remaining);
    }

    let mut events = vec![GameEvent::TriggerOrderChosen { chooser, card: chosen.card.clone(), trigger: chosen.trigger }];
    events.extend(crate::rules::dispatcher::fire_deferred(state, registry, &chosen)?);

    if resume == PendingChoiceResume::ResumeSubroutines && !state.resolution_halted() {
        events.extend(paid_ability::resolve_encounter_ice(state, registry)?);
    }
    Ok(events)
}

/// Resolves `PlayerAction::ToggleCardSelection`.
pub(crate) fn resolve_toggle_card_selection(
    state: &mut GameState,
    registry: &CardRegistry,
    position: usize,
) -> Result<Vec<GameEvent>, RulesError> {
    let Some(PendingDecision::ChooseCards { side, source, filter, .. }) = state.pending_decision.as_ref() else {
        return Err(RulesError::NoPendingDecision);
    };
    // Cloned out so the immutable borrow of `state.pending_decision` ends
    // here — `eligible_positions` needs `state` immutably too, and the
    // subsequent mutation needs it mutably.
    let (side, source, filter) = (*side, source.clone(), filter.clone());

    if !eligible_positions(state, registry, side, &source, &filter).contains(&position) {
        return Err(RulesError::CardNotEligibleForSelection(position));
    }

    let Some(PendingDecision::ChooseCards { selected, max, .. }) = state.pending_decision.as_mut() else {
        unreachable!("checked above");
    };

    // A plain toggle. This used to cycle through *copy counts* — each
    // toggle selecting one more copy of the same `CardId` until the zone
    // was exhausted, then clearing them all — because selecting by id
    // could otherwise never pick two copies of one card, which deadlocked
    // Carnivore ("trash 2 cards from your grip") against a grip holding two
    // of the same card. Positions are distinct by construction, so the two
    // copies are simply two selectable entries and the cycling machinery is
    // gone. `carnivore_can_select_two_copies_of_the_same_card` still pins
    // the behaviour that motivated it.
    if let Some(existing) = selected.iter().position(|p| *p == position) {
        selected.remove(existing);
        return Ok(Vec::new());
    }
    // Deselecting is always allowed (above), but selecting past `max` is
    // not: `ConfirmCardSelection` requires `min..=max`, so an
    // over-large selection can only be escaped by toggling back down.
    // Rejecting here rather than at confirm time makes the bound visible to
    // `legal_actions`' dry-run probe, so an at-capacity selection simply
    // stops offering new cards.
    //
    // Without this the mask kept offering every eligible card regardless of
    // `max`, and a bot selecting at random walked the selection far above
    // it — 11 cards against `max: 2` — then spent tens of thousands of
    // steps random-walking back down. Found by
    // `no_panics_or_deadlocks_across_many_seeds_system_gateway` (Spin
    // Doctor, "shuffle *up to 2* cards from Archives into R&D").
    if selected.len() as u32 >= *max {
        return Err(RulesError::CardSelectionFull { max: *max });
    }
    selected.push(position);
    Ok(Vec::new())
}

/// Resolves `PlayerAction::ConfirmCardSelection`.
pub(crate) fn resolve_confirm_card_selection(
    state: &mut GameState,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let PendingDecision::ChooseCards {
        side,
        source,
        min,
        max,
        reveal,
        shuffle_after,
        destination,
        then,
        selected,
        source_card,
        source_install,
        resume,
        ..
    } = state.pending_decision.take().ok_or(RulesError::NoPendingDecision)?
    else {
        return Err(RulesError::NoPendingDecision);
    };

    let count = selected.len();
    if count < min as usize || count > max as usize {
        // Selection is discarded (not re-parked) on an invalid confirm —
        // same "no partial-progress recovery" convention `AcceptPendingPaidChoice`/
        // `ResolvePendingChoice` already establish for a malformed resolution.
        return Err(RulesError::CardSelectionOutOfRange { selected: count, min, max });
    }

    // Positions are resolved to concrete cards **once, before any
    // mutation**: every branch below removes cards from `source`, which
    // would shift the positions still to be resolved.
    let zone = zone_card_ids(state, side, &source);
    let zone_installs = zone_install_ids(state, side, &source);
    let positions = selected;
    let selected: Vec<CardId> = positions
        .iter()
        .map(|p| zone.get(*p).cloned().ok_or(RulesError::CardNotEligibleForSelection(*p)))
        .collect::<Result<_, _>>()?;
    // The same selection as installs, for the `then` effects that address
    // an install rather than a card. Empty when `source` is not an
    // installed-card zone, in which case no such effect applies.
    let selected_installs: Vec<InstallId> = zone_installs
        .map(|ids| positions.iter().filter_map(|p| ids.get(*p).copied()).collect())
        .unwrap_or_default();

    let mut events = vec![GameEvent::CardsSelected { side, cards: selected.clone(), revealed: reveal }];

    if let Some(dest) = &destination {
        for (index, card_id) in selected.iter().enumerate() {
            // `Some(was_public)` once the card has left `source`: whether
            // the Runner had seen it decides its orientation if `dest` is
            // Archives. A card from a hidden zone (HQ, R&D) was not seen; a
            // rezzed install or any Runner card was.
            let mut cascade = Vec::new();
            let moved: Option<bool> = match &source {
                CardZoneRef::OpponentInstalled | CardZoneRef::OwnInstalled => selected_installs
                    .get(index)
                    .and_then(|install_id| remove_installed_card(state, side, &source, *install_id))
                    .map(|(_, was_public, hosted)| {
                        cascade = hosted;
                        was_public
                    }),
                _ if is_corp_archives(side, &source) => {
                    let pos = state.corp.archives.iter().position(|a| &a.card == card_id);
                    pos.map(|pos| !state.corp.archives.remove(pos).facedown)
                }
                _ => {
                    if let Some(zone) = plain_zone_mut(state, side, &source)
                        && let Some(pos) = zone.iter().position(|c| c == card_id)
                    {
                        zone.remove(pos);
                        Some(false)
                    } else {
                        None
                    }
                }
            };
            if let Some(was_public) = moved {
                if is_corp_archives(side, dest) {
                    // The Corp trashing its own cards out of HQ/R&D (Longevity
                    // Serum, Hansei Review, Anoetic Void) lands them facedown —
                    // the Runner has not seen them. A rezzed install it trashes
                    // was on the table and stays faceup.
                    state.corp.archives.push(if was_public {
                        ArchivedCard::faceup(card_id.clone())
                    } else {
                        ArchivedCard::facedown(card_id.clone())
                    });
                } else if let Some(zone) = plain_zone_mut(state, side, dest) {
                    zone.push(card_id.clone());
                }
                // A card moved into a discard pile was trashed, and says so
                // — `side` is the card's owner, as in `ability::trash_card`
                // (Retribution's victim is the Runner's). This path used to
                // emit only `CardsSelected`, so every selection-trash in the
                // pool (Ansel 1.0, Ballista, Retribution, Above the Law,
                // Carnivore, Longevity Serum, Hansei Review, Anoetic Void,
                // the memory-limit trash) was invisible to the coverage
                // harness's per-card `trashed` count. Nothing dispatches on
                // `CardTrashed`, so this changes no rules. Unlike
                // `Effect::TrashCard` it does not open a prevention window;
                // no card in the pool declares `OnTrashAboutToResolve`, so
                // parity is a follow-up if one ever does.
                if is_discard_pile(dest) {
                    events.push(GameEvent::CardTrashed { side: owning_side(side, dest), card: card_id.clone() });
                }
                events.extend(cascade);
            }
        }
        if shuffle_after {
            shuffle_zone(state, side, dest);
        }
    }

    if let Some(effect) = then {
        let acting = selected.first().or(source_card.as_ref());
        // …and the *install* the `then` acts as: the selected install when
        // the selection was over installs (Seamless Launch advances the
        // Offworld Office the Corp picked, not the first one), else the
        // parking card's own. Without it every `then` fell back to the
        // first copy of the card.
        let acting_install = selected_installs.first().copied().or(source_install);
        // `RezInstalledIgnoringCost`'s own embedded `CardId` is authored as
        // an unused placeholder in JSON (the real target isn't known until
        // resolution) — substitute the actual selected card here, the same
        // "acting-context substitution" convention `Effect::TrashCard(
        // CardTarget::ThisCard)` already uses, just resolved a step earlier
        // since this `Effect` variant addresses its target directly rather
        // than through `CardTarget`. e.g. Send a Message.
        // Same substitution convention, extended to two other shapes:
        // `SwapInstalledIce`'s two placeholders become the two (in this
        // case, order-independent) selected cards (e.g. Tāo Salonga); a
        // `InstallFromZoneIgnoringCost` placeholder's `card_id` becomes the
        // one selected card, and its `into`/`insert_after` placeholders mean
        // "this ice" — resolved to the ICE **being encountered** when that is
        // the resolving ability's own card (Brân 1.0/Ansel 1.0 park this
        // from their own subroutine). By install, not by first matching
        // `CardId`: with two Brâns on two servers the first-match lookup
        // installed inward of the wrong one. And when the host is gone —
        // the run ended, or Brân left play, while the choice was parked —
        // there is nothing to install "directly inward from", so the
        // install does not happen and the chosen card stays where it was.
        // This used to fall back to `ServerId::Archives`, which happened to
        // equal the JSON placeholder and so was never noticed.
        let host = state
            .active_run
            .as_ref()
            .filter(|run| run.phase == run::RunPhase::EncounterIce)
            .and_then(|run| run.ice.get(run.position))
            .filter(|ice| Some(&ice.card_id) == source_card.as_ref())
            .and_then(|ice| state.find_corp_install(ice.install_id))
            .map(|install| (install.install_id, install.server));
        // The two install-addressing effects substitute from
        // `selected_installs`, not `selected`: with two copies of one ICE
        // selected, two identical `CardId`s named the same install twice —
        // `SwapInstalledIce` swapped a card with itself and no-opped.
        let effect = match (*effect, selected.as_slice(), selected_installs.as_slice()) {
            (Effect::RezInstalledIgnoringCost(_), _, [chosen, ..]) => Some(Effect::RezInstalledIgnoringCost(*chosen)),
            (Effect::SwapInstalledIce(_, _), _, [a, b, ..]) => Some(Effect::SwapInstalledIce(*a, *b)),
            (Effect::InstallFromZoneIgnoringCost { origin_zone, slot, insert_after, .. }, [chosen, ..], _) => {
                host.map(|(host_install, into)| Effect::InstallFromZoneIgnoringCost {
                    card_id: chosen.clone(),
                    origin_zone,
                    into,
                    slot,
                    insert_after: insert_after.map(|_| host_install),
                })
            }
            (other, _, _) => Some(other),
        };
        if let Some(effect) = effect {
            events.extend(ability::evaluate_effect(state, &effect, &mut ability::ResolutionContext::for_parked(acting_install, acting), registry)?);
        }
    }

    if resume == PendingChoiceResume::ResumeSubroutines {
        // Same nested-parking propagation as `resolve_choice` — `then` may
        // itself park a further decision/paid choice.
        mark_pending_decision_resume_subroutines(state);
        if let Some(pending) = state.pending_paid_choice.as_mut() {
            pending.resume = PendingPaidChoiceResume::ResumeSubroutines;
        }
        events.extend(paid_ability::resolve_encounter_ice(state, registry)?);
    }
    Ok(events)
}

/// Resolves `PlayerAction::ChooseServerForPendingDecision`.
/// Rewrites any `AddAdditionalAccess` inside a `PromptChooseServer::
/// on_success` effect to name the server the player actually chose — its
/// authored `server` is an ignored placeholder, since the real target isn't
/// known until resolution. Same "placeholder substituted at resolution
/// time" convention `PromptChooseCards::then` uses for
/// `RezInstalledIgnoringCost`. Recurses through `Sequence`/`EffectIf` so a
/// composed on-success list (e.g. Jailbreak's draw-then-access) is covered.
fn substitute_chosen_server(effect: Effect, server: crate::rules::run::ServerId) -> Effect {
    match effect {
        Effect::AddAdditionalAccess { count, .. } => Effect::AddAdditionalAccess { server, count },
        Effect::Sequence(effects) => {
            Effect::Sequence(effects.into_iter().map(|e| substitute_chosen_server(e, server)).collect())
        }
        Effect::EffectIf { condition, effect } => Effect::EffectIf {
            condition,
            effect: Box::new(substitute_chosen_server(*effect, server)),
        },
        other => other,
    }
}

pub(crate) fn resolve_choose_server(
    state: &mut GameState,
    registry: &CardRegistry,
    server: crate::rules::run::ServerId,
) -> Result<Vec<GameEvent>, RulesError> {
    let PendingDecision::ChooseServer { rez_cost_delta, bonus_run_credits, allowed_servers, on_success, source_card, source_install, resume, .. } =
        state.pending_decision.take().ok_or(RulesError::NoPendingDecision)?
    else {
        return Err(RulesError::NoPendingDecision);
    };

    // `legal_actions` already filters the offer down to `allowed_servers`;
    // re-checking here keeps a directly-submitted action from bypassing it.
    if let Some(allowed) = &allowed_servers
        && !allowed.contains(&server)
    {
        return Err(RulesError::ServerNotAllowedForChoice { server });
    }

    run::start_run(state, registry, server)?;
    if let Some(run) = state.active_run.as_mut() {
        run.ice_rez_cost_modifier = rez_cost_delta;
        run.bonus_run_credits = bonus_run_credits;
        run.on_success_effect = on_success.map(|effect| Box::new(substitute_chosen_server(*effect, server)));
        // The rider resolves as the card that offered the choice (Red Team
        // takes credits from *itself*), so carry its identity onto the run.
        run.on_success_card = source_card;
        run.on_success_install = source_install;
    }

    let run_initiated_event = GameEvent::RunInitiated { server };
    let mut events = vec![run_initiated_event.clone()];
    events.extend(crate::rules::dispatcher::dispatch_event(state, registry, &run_initiated_event)?);

    if resume == PendingChoiceResume::ResumeSubroutines {
        events.extend(paid_ability::resolve_encounter_ice(state, registry)?);
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::CardRegistry;
    use crate::dsl::Effect;
    use crate::rules::action::PlayerAction;
    use crate::rules::error::RulesError;
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, Credits, GamePhase, MemoryUnits, PlayerResources, RunnerState,
    };

    fn game_state() -> GameState {
        GameState {
            corp: CorpState {
                resources: PlayerResources { credits: Credits(10), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
                ..Default::default()
            },
            runner: RunnerState {
                resources: PlayerResources { credits: Credits(10), clicks: Clicks(4), agenda_points: AgendaPoints(0) },
                memory_units: MemoryUnits(0),
                ..Default::default()
            },
            phase: GamePhase::Action(Side::Runner),
            ..Default::default()
        }
    }

    #[test]
    fn accept_pays_the_cost_and_resolves_if_paid() {
        let mut state = game_state();
        state.pending_paid_choice = Some(crate::rules::state::PendingPaidChoice {
            side: Side::Runner,
            cost: Cost::Credits(4),
            if_paid: Effect::Sequence(Vec::new()),
            if_declined: Effect::GiveTags(1),
            source_card: None,
            source_install: None,
            resume: PendingPaidChoiceResume::None,
        });

        let events = resolve_accept(&mut state, &CardRegistry::new(), None).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(6));
        assert_eq!(state.runner.tags, 0);
        assert!(state.pending_paid_choice.is_none());
        assert!(events.contains(&GameEvent::PendingPaidChoiceAccepted { side: Side::Runner }));
    }

    #[test]
    fn decline_pays_nothing_and_resolves_if_declined() {
        let mut state = game_state();
        state.pending_paid_choice = Some(crate::rules::state::PendingPaidChoice {
            side: Side::Runner,
            cost: Cost::Credits(4),
            if_paid: Effect::Sequence(Vec::new()),
            if_declined: Effect::GiveTags(1),
            source_card: None,
            source_install: None,
            resume: PendingPaidChoiceResume::None,
        });

        let events = resolve_decline(&mut state, &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(10), "no cost paid");
        assert_eq!(state.runner.tags, 1);
        assert!(state.pending_paid_choice.is_none());
        assert!(events.contains(&GameEvent::PendingPaidChoiceDeclined { side: Side::Runner }));
    }

    #[test]
    fn accept_any_of_pays_the_selected_option() {
        let mut state = game_state();
        state.pending_paid_choice = Some(crate::rules::state::PendingPaidChoice {
            side: Side::Runner,
            cost: Cost::AnyOf(vec![Cost::Clicks(2), Cost::Credits(5)]),
            if_paid: Effect::Sequence(Vec::new()),
            if_declined: Effect::EndTheRun,
            source_card: None,
            source_install: None,
            resume: PendingPaidChoiceResume::None,
        });

        resolve_accept(&mut state, &CardRegistry::new(), Some(1)).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(5), "paid the credits option (index 1)");
        assert_eq!(state.runner.resources.clicks, Clicks(4), "clicks untouched");
    }

    #[test]
    fn accept_any_of_without_an_index_errors() {
        let mut state = game_state();
        state.pending_paid_choice = Some(crate::rules::state::PendingPaidChoice {
            side: Side::Runner,
            cost: Cost::AnyOf(vec![Cost::Clicks(2), Cost::Credits(5)]),
            if_paid: Effect::Sequence(Vec::new()),
            if_declined: Effect::EndTheRun,
            source_card: None,
            source_install: None,
            resume: PendingPaidChoiceResume::None,
        });

        let result = resolve_accept(&mut state, &CardRegistry::new(), None);

        assert_eq!(result, Err(RulesError::InvalidCostChoiceIndex(0)));
    }

    #[test]
    fn resolve_choice_evaluates_the_selected_option() {
        let mut state = game_state();
        state.pending_decision = Some(PendingDecision::ChooseEffect {
            chooser: Side::Corp,
            options: vec![Effect::GainCredits(Side::Corp, 2), Effect::DrawCards(Side::Corp, 2)],
            source_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });

        let events = resolve_choice(&mut state, &CardRegistry::new(), 0).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(12));
        assert!(state.pending_decision.is_none());
        assert!(events.contains(&GameEvent::CreditsGained { side: Side::Corp, amount: 2 }));
    }

    #[test]
    fn resolve_choice_out_of_range_errors() {
        let mut state = game_state();
        state.pending_decision = Some(PendingDecision::ChooseEffect {
            chooser: Side::Corp,
            options: vec![Effect::GainCredits(Side::Corp, 2)],
            source_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });

        let result = resolve_choice(&mut state, &CardRegistry::new(), 5);

        assert_eq!(result, Err(RulesError::InvalidChoiceIndex(5)));
    }

    #[test]
    fn apply_action_blocks_unrelated_actions_while_a_paid_choice_is_pending() {
        let mut state = game_state();
        state.pending_paid_choice = Some(crate::rules::state::PendingPaidChoice {
            side: Side::Runner,
            cost: Cost::Credits(4),
            if_paid: Effect::Sequence(Vec::new()),
            if_declined: Effect::GiveTags(1),
            source_card: None,
            source_install: None,
            resume: PendingPaidChoiceResume::None,
        });
        let registry = CardRegistry::new();

        let result = crate::rules::apply_action(&state, &registry, PlayerAction::DrawCardClick { side: Side::Runner });
        assert_eq!(result, Err(RulesError::ActionBlockedByPendingPaidChoice { side: Side::Runner }));

        let (next, _) =
            crate::rules::apply_action(&state, &registry, PlayerAction::DeclinePendingPaidChoice).unwrap();
        assert!(next.pending_paid_choice.is_none());
        state = next;
        // Now unblocked.
        assert!(crate::rules::apply_action(&state, &registry, PlayerAction::DrawCardClick { side: Side::Runner }).is_ok());
    }

    /// A `ChooseCards` selection must never grow past its `max`. It used to:
    /// `ToggleCardSelection` only checked eligibility, so the mask kept
    /// offering every candidate no matter how many were already picked,
    /// while `ConfirmCardSelection` still demanded `min..=max` — leaving a
    /// state escapable only by toggling back down one card at a time.
    /// A bot picking at random walked far above the bound and burned tens of
    /// thousands of steps returning (Spin Doctor, "shuffle *up to 2*").
    #[test]
    fn toggling_past_the_selection_maximum_is_rejected_and_never_offered() {
        use crate::dsl::{CardFilter, CardZoneRef};
        use crate::rules::state::{ArchivedCard, PendingChoiceResume};

        let mut registry = CardRegistry::new();
        let mut archives = Vec::new();
        for index in 0..4 {
            let id = CardId(format!("archived_{index}"));
            registry.insert(crate::dsl::CardDefinition {
                title: id.0.clone(),
                id: id.clone(),
                side: Side::Corp,
                card_type: crate::dsl::CardType::Operation,
                is_playable: true,
                ..Default::default()
            });
            archives.push(ArchivedCard::faceup(id));
        }

        let mut state = game_state();
        state.phase = GamePhase::Action(Side::Corp);
        state.corp.archives = archives;
        state.pending_decision = Some(PendingDecision::ChooseCards {
            side: Side::Corp,
            source: CardZoneRef::OwnArchives,
            filter: CardFilter::Any,
            min: 0,
            max: 2,
            reveal: false,
            shuffle_after: false,
            destination: None,
            then: None,
            selected: Vec::new(),
            source_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });

        // Fill the selection to `max`.
        for index in 0..2 {
            let action = PlayerAction::ToggleCardSelection { position: index };
            let (next, _) = crate::rules::apply_action(&state, &registry, action).expect("selecting within max");
            state = next;
        }

        // A third distinct card is refused, and — the part that actually
        // prevented the stall — is no longer offered as a legal action.
        let third = PlayerAction::ToggleCardSelection { position: 2 };
        assert!(matches!(
            crate::rules::apply_action(&state, &registry, third.clone()),
            Err(RulesError::CardSelectionFull { max: 2 })
        ));
        let legal = crate::rules::legal_actions(&state, &registry);
        assert!(!legal.contains(&third), "an at-capacity selection must not offer more cards: {legal:?}");

        // Deselecting stays legal, and frees a slot again.
        let deselect = PlayerAction::ToggleCardSelection { position: 0 };
        assert!(legal.contains(&deselect), "deselecting an already-selected card must stay legal");
        let (state, _) = crate::rules::apply_action(&state, &registry, deselect).expect("deselecting");
        assert!(crate::rules::legal_actions(&state, &registry).contains(&third));
    }

    /// Two copies of the *same* card must both be selectable.
    ///
    /// `selected` was once keyed by `CardId`, so toggling flipped a single
    /// bit per id and capped the selection at one copy. Any "choose N" whose
    /// zone held fewer than N *distinct* cards then deadlocked: the
    /// availability guard counts physical copies and parks the decision,
    /// but the selection could never reach `min`, so
    /// `ConfirmCardSelection` was never legal while the parked decision
    /// blocked every other action. Carnivore ("trash 2 cards from your
    /// grip") against a grip holding two copies of one card is the
    /// reachable case; found by self-play over the sample decks.
    #[test]
    fn duplicate_copies_of_one_card_can_fill_a_selection() {
        use crate::dsl::{CardFilter, CardZoneRef};
        use crate::rules::state::PendingChoiceResume;

        let mut registry = CardRegistry::new();
        let id = CardId("botulus".to_string());
        registry.insert(crate::dsl::CardDefinition {
            title: id.0.clone(),
            id: id.clone(),
            side: Side::Runner,
            card_type: crate::dsl::CardType::Program,
            is_playable: true,
            ..Default::default()
        });

        let mut state = game_state();
        state.phase = GamePhase::Action(Side::Runner);
        // A grip of exactly two copies of one card — one distinct id.
        state.runner.grip = vec![id.clone(), id.clone()];
        state.pending_decision = Some(PendingDecision::ChooseCards {
            side: Side::Runner,
            source: CardZoneRef::OwnGrip,
            filter: CardFilter::Any,
            min: 2,
            max: 2,
            reveal: false,
            shuffle_after: false,
            destination: Some(CardZoneRef::OwnHeap),
            then: None,
            selected: Vec::new(),
            source_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });

        // Two copies, two positions — which is the whole fix. The
        // copy-count cycling this test was written against is gone;
        // position 0 and position 1 are simply different actions.
        let _ = &id;

        // Confirming is not legal yet: nothing is selected.
        assert!(!crate::rules::legal_actions(&state, &registry).contains(&PlayerAction::ConfirmCardSelection));

        for position in 0..2 {
            let toggle = PlayerAction::ToggleCardSelection { position };
            let (next, _) = crate::rules::apply_action(&state, &registry, toggle).expect("select a copy");
            state = next;
        }
        let Some(PendingDecision::ChooseCards { selected, .. }) = &state.pending_decision else {
            panic!("decision still parked");
        };
        assert_eq!(selected.len(), 2, "both copies should be selected");

        assert!(
            crate::rules::legal_actions(&state, &registry).contains(&PlayerAction::ConfirmCardSelection),
            "a full selection must be confirmable"
        );

        let (state, _) =
            crate::rules::apply_action(&state, &registry, PlayerAction::ConfirmCardSelection).expect("confirm");
        assert!(state.pending_decision.is_none());
        assert!(state.runner.grip.is_empty(), "both copies left the grip");
        assert_eq!(state.runner.heap.len(), 2, "both copies reached the heap");
    }

    /// A card trashed through a selection must arrive in its discard pile
    /// exactly once, and with two identical installs the one at the chosen
    /// position is the one that goes. `remove_installed_card` used to push
    /// into the Heap itself and then the destination branch pushed again —
    /// found by the card-conservation sweep the first time a heuristic
    /// Runner installed a program for Ansel 1.0 to trash.
    #[test]
    fn trashing_an_installed_card_through_a_selection_moves_it_exactly_once_by_install_id() {
        use crate::dsl::{CardFilter, CardZoneRef};
        use crate::rules::state::{InstalledRunnerCard, PendingChoiceResume};

        let mut state = GameState::new(0);
        let marjanah = CardId("marjanah".to_string());
        state.runner.rig = vec![
            InstalledRunnerCard { card: marjanah.clone(), install_id: InstallId(1), counters: 1, ..Default::default() },
            InstalledRunnerCard { card: marjanah.clone(), install_id: InstallId(2), counters: 2, ..Default::default() },
        ];
        state.pending_decision = Some(PendingDecision::ChooseCards {
            side: Side::Corp,
            source: CardZoneRef::OpponentInstalled,
            filter: CardFilter::Any,
            min: 1,
            max: 1,
            reveal: false,
            shuffle_after: false,
            destination: Some(CardZoneRef::OpponentDiscard),
            then: None,
            selected: vec![1],
            source_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });

        resolve_confirm_card_selection(&mut state, &CardRegistry::new()).expect("a valid selection resolves");

        assert_eq!(state.runner.heap, vec![marjanah], "one copy in the Heap, not two");
        assert_eq!(state.runner.rig.len(), 1);
        assert_eq!(state.runner.rig[0].install_id, InstallId(1), "the copy at position 1 (id 2) was trashed");
    }
}
