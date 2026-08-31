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
use crate::rules::state::{ArchivedCard, GameState, PendingChoiceResume, PendingDecision, PendingPaidChoiceResume, Side};

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

/// The candidate `CardId`s currently eligible for `zone`/`filter`, from
/// `chooser`'s perspective — `zone_card_ids` filtered down to those
/// matching `filter`. Used both to validate `ToggleCardSelection` and to
/// silently no-op `Effect::PromptChooseCards` when fewer than `min` cards
/// exist at all.
pub(crate) fn eligible_cards(
    state: &GameState,
    registry: &CardRegistry,
    chooser: Side,
    zone: &CardZoneRef,
    filter: &CardFilter,
) -> Vec<CardId> {
    zone_card_ids(state, chooser, zone)
        .into_iter()
        .filter(|id| registry.get(id).is_some_and(|card| card_matches_filter(card, filter)))
        .filter(|id| instance_matches_filter(state, chooser, zone, id, filter))
        .collect()
}

/// The instance-level half of `CardFilter`, which `card_matches_filter`
/// can't answer from a `CardDefinition` alone. Only
/// `CardFilter::NotInstalledThisTurn` needs it today; every other variant
/// passes through.
fn instance_matches_filter(
    state: &GameState,
    chooser: Side,
    zone: &CardZoneRef,
    card_id: &CardId,
    filter: &CardFilter,
) -> bool {
    match filter {
        CardFilter::NotInstalledThisTurn => match owning_side(chooser, zone) {
            Side::Corp => state
                .corp
                .installed
                .iter()
                .find(|c| &c.card == card_id)
                .is_some_and(|c| !c.installed_this_turn),
            // No Runner card needs this restriction yet; a rig card is
            // never eligible rather than silently always-eligible.
            Side::Runner => false,
        },
        // Only an unrezzed installed Corp card can be a rez target. The
        // Runner has no rez state, so a rig card is never eligible.
        CardFilter::UnrezzedIce => match owning_side(chooser, zone) {
            Side::Corp => {
                state.corp.installed.iter().find(|c| &c.card == card_id).is_some_and(|c| !c.rezzed)
            }
            Side::Runner => false,
        },
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

/// Removes `card_id` from an installed-card zone (`OpponentInstalled`/
/// `OwnInstalled`), returning it to its owner's discard pile — the
/// `TrashCard`-shaped half of `ConfirmCardSelection`'s resolution for
/// Ballista/Retribution/Above the Law. No-ops (returns `false`) if
/// `card_id` isn't actually installed there anymore, mirroring
/// `ability::trash_this_card`'s "already gone" leniency.
fn remove_installed_card(state: &mut GameState, chooser: Side, zone: &CardZoneRef, card_id: &CardId) -> bool {
    let owner = owning_side(chooser, zone);
    match owner {
        Side::Corp => {
            if let Some(pos) = state.corp.installed.iter().position(|c| &c.card == card_id) {
                // Rezzed installs were face-up on the table; unrezzed ones
                // the Runner never saw. Same rule as `ability::trash_card`.
                let was_rezzed = state.corp.installed[pos].rezzed;
                state.corp.installed.remove(pos);
                state.corp.archives.push(if was_rezzed {
                    ArchivedCard::faceup(card_id.clone())
                } else {
                    ArchivedCard::facedown(card_id.clone())
                });
                return true;
            }
            false
        }
        Side::Runner => {
            if let Some(pos) = state.runner.rig.iter().position(|c| &c.card == card_id) {
                state.runner.rig.remove(pos);
                state.runner.heap.push(card_id.clone());
                return true;
            }
            false
        }
    }
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

    let mut events = ability::pay_cost(state, pending.side, &cost_to_pay, pending.source_card.as_ref())?;
    events.push(GameEvent::PendingPaidChoiceAccepted { side: pending.side });
    events.extend(ability::evaluate_effect(state, &pending.if_paid, pending.source_card.as_ref(), registry)?);

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
    events.extend(ability::evaluate_effect(state, &pending.if_declined, pending.source_card.as_ref(), registry)?);

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
    let PendingDecision::ChooseEffect { chooser, options, source_card, resume } =
        state.pending_decision.take().ok_or(RulesError::NoPendingDecision)?
    else {
        return Err(RulesError::NoPendingDecision);
    };

    let effect = options.get(option_index).ok_or(RulesError::InvalidChoiceIndex(option_index))?.clone();
    let mut events = vec![GameEvent::PendingChoiceResolved { chooser, option_index }];
    events.extend(ability::evaluate_effect(state, &effect, source_card.as_ref(), registry)?);

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
    card: CardId,
) -> Result<Vec<GameEvent>, RulesError> {
    let Some(PendingDecision::ChooseTriggerOrder { chooser, pending, resume }) = state.pending_decision.take() else {
        return Err(RulesError::NoPendingDecision);
    };

    let position = pending
        .iter()
        .position(|due| due.card == card)
        .ok_or_else(|| RulesError::CardNotActive { side: chooser, card: card.clone() })?;
    let mut remaining = pending;
    let chosen = remaining.remove(position);

    // Queue the remainder before firing, so a parking `chosen` can't strand
    // it: either it re-parks as a decision (2+ left) or the drain takes it.
    if remaining.len() >= 2 {
        state.pending_decision = Some(PendingDecision::ChooseTriggerOrder { chooser, pending: remaining, resume });
    } else {
        state.deferred_triggers.splice(0..0, remaining);
    }

    let mut events = vec![GameEvent::TriggerOrderChosen { chooser, card }];
    events.extend(crate::rules::dispatcher::fire_deferred(state, registry, &chosen)?);

    if resume == PendingChoiceResume::ResumeSubroutines && !state.is_resolution_blocked() {
        events.extend(paid_ability::resolve_encounter_ice(state, registry)?);
    }
    Ok(events)
}

/// Resolves `PlayerAction::ToggleCardSelection`.
pub(crate) fn resolve_toggle_card_selection(
    state: &mut GameState,
    registry: &CardRegistry,
    card_id: CardId,
) -> Result<Vec<GameEvent>, RulesError> {
    let Some(PendingDecision::ChooseCards { side, source, filter, .. }) = state.pending_decision.as_ref() else {
        return Err(RulesError::NoPendingDecision);
    };
    // Cloned out so the immutable borrow of `state.pending_decision` ends
    // here — `eligible_cards` needs `state` immutably too, and the
    // subsequent mutation needs it mutably.
    let (side, source, filter) = (*side, source.clone(), filter.clone());

    let eligible = eligible_cards(state, registry, side, &source, &filter);
    if !eligible.contains(&card_id) {
        return Err(RulesError::CardNotEligibleForSelection(card_id));
    }
    // How many physical copies of this card the zone actually holds. A zone
    // is a `Vec<CardId>`, so three copies of Botulus are three entries with
    // the same id.
    let available_copies = eligible.iter().filter(|id| *id == &card_id).count();

    let Some(PendingDecision::ChooseCards { selected, max, .. }) = state.pending_decision.as_mut() else {
        unreachable!("checked above");
    };
    let selected_copies = selected.iter().filter(|id| *id == &card_id).count();

    // Toggling cycles through copy counts rather than flipping a single
    // bit: each toggle selects one more copy until every copy the zone
    // holds is selected, and the next toggle clears them all.
    //
    // Selecting by id alone used to cap the selection at one copy per id,
    // which deadlocked any "choose N" whose zone held fewer than N
    // *distinct* cards. Carnivore ("trash 2 cards from your grip") against
    // a grip of two copies of one card is the reachable case: the
    // availability guard counts two entries and parks the decision, but the
    // selection could never reach two, so `ConfirmCardSelection` was never
    // legal while the parked decision blocked every other action.
    if selected_copies >= available_copies {
        selected.retain(|id| id != &card_id);
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
    selected.push(card_id);
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

    let mut events = vec![GameEvent::CardsSelected { side, cards: selected.clone(), revealed: reveal }];

    if let Some(dest) = &destination {
        for card_id in &selected {
            let moved = match &source {
                CardZoneRef::OpponentInstalled | CardZoneRef::OwnInstalled => {
                    remove_installed_card(state, side, &source, card_id)
                }
                _ if is_corp_archives(side, &source) => {
                    if let Some(pos) = state.corp.archives.iter().position(|a| &a.card == card_id) {
                        state.corp.archives.remove(pos);
                        true
                    } else {
                        false
                    }
                }
                _ => {
                    if let Some(zone) = plain_zone_mut(state, side, &source)
                        && let Some(pos) = zone.iter().position(|c| c == card_id)
                    {
                        zone.remove(pos);
                        true
                    } else {
                        false
                    }
                }
            };
            if moved {
                if is_corp_archives(side, dest) {
                    // Everything routed into Archives this way is the Corp
                    // trashing its own cards out of a hidden zone (HQ/R&D) —
                    // e.g. Longevity Serum, Hansei Review, Anoetic Void — so
                    // the Runner has not seen them and they land facedown.
                    state.corp.archives.push(ArchivedCard::facedown(card_id.clone()));
                } else if let Some(zone) = plain_zone_mut(state, side, dest) {
                    zone.push(card_id.clone());
                }
            }
        }
        if shuffle_after {
            shuffle_zone(state, side, dest);
        }
    }

    if let Some(effect) = then {
        let acting = selected.first().or(source_card.as_ref());
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
        // one selected card, and its `into`/`insert_after` placeholders are
        // resolved from `source_card` (the resolving ability's own card,
        // e.g. Brân 1.0) rather than from the selection — looked up here
        // since only `source_card`'s currently-installed `server` is
        // needed, not any zone-move machinery.
        let host_server = source_card.as_ref().and_then(|id| state.corp.installed.iter().find(|c| &c.card == id)).map(|c| c.server);
        let effect = match (*effect, selected.as_slice()) {
            (Effect::RezInstalledIgnoringCost(_), [chosen, ..]) => Effect::RezInstalledIgnoringCost(chosen.clone()),
            (Effect::SwapInstalledIce(_, _), [a, b, ..]) => Effect::SwapInstalledIce(a.clone(), b.clone()),
            (Effect::InstallFromZoneIgnoringCost { origin_zone, slot, insert_after, .. }, [chosen, ..]) => {
                Effect::InstallFromZoneIgnoringCost {
                    card_id: chosen.clone(),
                    origin_zone,
                    into: host_server.unwrap_or(crate::rules::ServerId::Archives),
                    slot,
                    insert_after: insert_after.and(source_card.clone()),
                }
            }
            (other, _) => other,
        };
        events.extend(ability::evaluate_effect(state, &effect, acting, registry)?);
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
    let PendingDecision::ChooseServer { rez_cost_delta, bonus_run_credits, allowed_servers, on_success, resume, .. } =
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
            resume: PendingPaidChoiceResume::None,
        });
        let registry = CardRegistry::new();

        let result = crate::rules::apply_action(&state, &registry, PlayerAction::DrawCardClick);
        assert_eq!(result, Err(RulesError::ActionBlockedByPendingPaidChoice { side: Side::Runner }));

        let (next, _) =
            crate::rules::apply_action(&state, &registry, PlayerAction::DeclinePendingPaidChoice).unwrap();
        assert!(next.pending_paid_choice.is_none());
        state = next;
        // Now unblocked.
        assert!(crate::rules::apply_action(&state, &registry, PlayerAction::DrawCardClick).is_ok());
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
            resume: PendingChoiceResume::None,
        });

        // Fill the selection to `max`.
        for index in 0..2 {
            let action = PlayerAction::ToggleCardSelection { card_id: CardId(format!("archived_{index}")) };
            let (next, _) = crate::rules::apply_action(&state, &registry, action).expect("selecting within max");
            state = next;
        }

        // A third distinct card is refused, and — the part that actually
        // prevented the stall — is no longer offered as a legal action.
        let third = PlayerAction::ToggleCardSelection { card_id: CardId("archived_2".to_string()) };
        assert!(matches!(
            crate::rules::apply_action(&state, &registry, third.clone()),
            Err(RulesError::CardSelectionFull { max: 2 })
        ));
        let legal = crate::rules::legal_actions(&state, &registry);
        assert!(!legal.contains(&third), "an at-capacity selection must not offer more cards: {legal:?}");

        // Deselecting stays legal, and frees a slot again.
        let deselect = PlayerAction::ToggleCardSelection { card_id: CardId("archived_0".to_string()) };
        assert!(legal.contains(&deselect), "deselecting an already-selected card must stay legal");
        let (state, _) = crate::rules::apply_action(&state, &registry, deselect).expect("deselecting");
        assert!(crate::rules::legal_actions(&state, &registry).contains(&third));
    }

    /// Two copies of the *same* card must both be selectable.
    ///
    /// `selected` is keyed by `CardId`, so toggling used to flip a single
    /// bit per id and cap the selection at one copy. Any "choose N" whose
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
            resume: PendingChoiceResume::None,
        });

        let toggle = PlayerAction::ToggleCardSelection { card_id: id.clone() };

        // Confirming is not legal yet: nothing is selected.
        assert!(!crate::rules::legal_actions(&state, &registry).contains(&PlayerAction::ConfirmCardSelection));

        // Two toggles select both physical copies.
        for _ in 0..2 {
            let (next, _) = crate::rules::apply_action(&state, &registry, toggle.clone()).expect("select a copy");
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
}
