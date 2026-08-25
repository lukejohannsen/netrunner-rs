use crate::cards::CardRegistry;
use crate::dsl::{
    BoostDuration, CardId, CardTarget, Cost, Effect, EffectRequirement, StackZone, SubroutineBreakCount, Trigger,
};
use crate::rules::damage;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::{self, RunPhase, ServerId, SubroutineStatus};
use crate::rules::state::{Clicks, Credits, GamePhase, GameState, Side, TraceResume, TraceState};

/// Applies a single, already-resolved `Effect` to `state` in place.
///
/// A deliberate new hybrid mutation convention: mutate-in-place like
/// `damage::apply_damage`/`run::access_server` (the caller has already
/// cloned/validated phase, so this never needs to reclone), but fallible
/// unlike them — some `Effect` arms genuinely can fail against a
/// well-formed state (`TrashCard` naming a target that isn't where it's
/// claimed to be) while others structurally cannot.
///
/// `acting_card` identifies which card is resolving this effect: for
/// `BoostStrength`/`BreakSubroutines` it's specifically "whichever Runner
/// rig card activated the ability" (`RulesError::UnresolvedCardTarget` if
/// `None`); for `TrashCard(CardTarget::ThisCard)` it's simply "the card this
/// effect is printed on" (Corp or Runner alike), resolved via
/// `trash_this_card` — `RulesError::MissingActingCardContext` if `None`.
/// Callers that aren't resolving a specific card's own ability/trigger
/// (subroutine resolution) pass `None`.
pub fn evaluate_effect(
    state: &mut GameState,
    effect: &Effect,
    acting_card: Option<&CardId>,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    match effect {
        Effect::GainCredits(side, amount) => {
            // Mirrors engine::gain_credit_click's existing pattern.
            state.resources_mut(*side).credits = state.resources(*side).credits.gain(*amount);
            Ok(vec![GameEvent::CreditsGained { side: *side, amount: *amount }])
        }

        Effect::DealDamage(damage_type, amount) => {
            // Delegates wholesale to the existing, already-infallible
            // apply_damage — no new error arm needed here.
            Ok(damage::apply_damage(state, *damage_type, *amount))
        }

        Effect::BreakSubroutine(index) => {
            let (card_id, _effect) = run::transition_subroutine(state, *index, SubroutineStatus::Broken)?;
            Ok(vec![GameEvent::SubroutineBroken { card_id, index: *index }])
        }

        Effect::ModifyStrength(delta) => {
            let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
            if run.phase != RunPhase::EncounterIce {
                return Err(RulesError::NotInEncounter);
            }
            let position = run.position;
            // `NotInEncounter` doubles as the defensive fallback here if
            // `position` were ever out of bounds — an invariant violation
            // that shouldn't happen while `phase == EncounterIce`, but
            // `.get_mut` avoids a raw-index panic regardless.
            let ice = run.ice.get_mut(position).ok_or(RulesError::NotInEncounter)?;
            ice.current_strength += delta;
            let event = GameEvent::IceStrengthModified {
                card_id: ice.card_id.clone(),
                new_strength: ice.current_strength,
                delta: *delta,
            };
            Ok(vec![event])
        }

        Effect::DrawCards(side, amount) => {
            // Mirrors engine::draw_card_click's existing per-card pattern,
            // generalized to `amount` and either side's deck. An empty
            // deck is a silent stop (fewer than `amount` cards drawn, even
            // zero) rather than an error, matching draw_card_click's
            // established precedent.
            let mut events = Vec::new();
            for _ in 0..*amount {
                let drawn = match side {
                    Side::Corp => state.corp.r_and_d.pop(),
                    Side::Runner => state.runner.stack.pop(),
                };
                match drawn {
                    Some(card) => {
                        match side {
                            Side::Corp => state.corp.hq.push(card),
                            Side::Runner => state.runner.grip.push(card),
                        }
                        events.push(GameEvent::CardDrawn { side: *side });
                    }
                    None => break,
                }
            }
            Ok(events)
        }

        Effect::EndTheRun => {
            let server = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?.server;
            state.active_run = None;
            Ok(vec![GameEvent::RunEndedByEffect { server }])
        }

        Effect::GiveTags(amount) => {
            // Always targets the Runner — see GiveTags's own doc comment.
            state.runner.tags = state.runner.tags.saturating_add(*amount);
            Ok(vec![GameEvent::TagsGiven { side: Side::Runner, amount: *amount }])
        }

        Effect::RemoveTags(amount) => {
            state.runner.tags = state.runner.tags.saturating_sub(*amount);
            Ok(vec![GameEvent::TagsRemoved { side: Side::Runner, amount: *amount }])
        }

        Effect::GiveBadPublicity(amount) => {
            state.corp.bad_publicity = state.corp.bad_publicity.saturating_add(*amount);
            Ok(vec![GameEvent::BadPublicityGiven { amount: *amount }])
        }

        Effect::RemoveBadPublicity(amount) => {
            state.corp.bad_publicity = state.corp.bad_publicity.saturating_sub(*amount);
            Ok(vec![GameEvent::BadPublicityRemoved { amount: *amount }])
        }

        Effect::TrashCard(target) => trash_card(state, target, acting_card),

        Effect::BoostStrength { amount, duration } => {
            let acting = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let card = state
                .runner
                .rig
                .iter_mut()
                .find(|c| &c.card == acting)
                .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: acting.clone() })?;
            match duration {
                BoostDuration::Encounter => card.encounter_strength_buff += *amount as i32,
                BoostDuration::Turn => card.turn_strength_buff += *amount as i32,
            }
            let new_strength = card.effective_strength();
            Ok(vec![GameEvent::StrengthBoosted {
                card_id: acting.clone(),
                new_strength,
                delta: *amount as i32,
                duration: *duration,
            }])
        }

        Effect::BreakSubroutines { count, restrict_to } => {
            let acting = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
            if run.phase != RunPhase::EncounterIce {
                return Err(RulesError::NotInEncounter);
            }

            let breaker_strength = state
                .runner
                .rig
                .iter()
                .find(|c| &c.card == acting)
                .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: acting.clone() })?
                .effective_strength();

            let run = state.active_run.as_ref().unwrap();
            let ice = &run.ice[run.position];
            let (ice_card_id, ice_strength, ice_type) =
                (ice.card_id.clone(), ice.current_strength, ice.ice_type);
            if let Some(expected) = restrict_to
                && *expected != ice_type
            {
                return Err(RulesError::InvalidBreakerSubtype {
                    breaker: acting.clone(),
                    ice: ice_card_id,
                    expected: *expected,
                });
            }
            if breaker_strength < ice_strength {
                return Err(RulesError::BreakerStrengthTooLow {
                    breaker: acting.clone(),
                    breaker_strength,
                    ice: ice_card_id,
                    ice_strength,
                });
            }

            // Collected/owned before any &mut state borrow below, so the
            // immutable `run`/`ice` reads above never overlap with
            // transition_subroutine's &mut state.
            let pending: Vec<usize> = ice
                .subroutines
                .iter()
                .filter(|s| s.status == SubroutineStatus::Pending)
                .map(|s| s.id)
                .collect();

            let take = match count {
                SubroutineBreakCount::All => pending.len(),
                SubroutineBreakCount::Fixed(n) => (*n as usize).min(pending.len()),
            };
            let mut events = Vec::new();
            for idx in pending.into_iter().take(take) {
                let (card_id, _effect) = run::transition_subroutine(state, idx, SubroutineStatus::Broken)?;
                events.push(GameEvent::SubroutineBroken { card_id, index: idx });
            }
            Ok(events)
        }

        Effect::Trace { base, on_success } => {
            if state.active_trace.is_some() {
                return Err(RulesError::TraceAlreadyActive);
            }
            state.active_trace = Some(TraceState {
                initiating_card: acting_card.cloned(),
                base_strength: *base,
                corp_bid: None,
                effect_on_success: (**on_success).clone(),
                resume: TraceResume::None,
            });
            Ok(vec![GameEvent::TraceInitiated { base: *base, initiating_card: acting_card.cloned() }])
        }

        Effect::AddAdditionalAccess { server, count } => {
            let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
            match server {
                ServerId::Hq => run.additional_hq_access = run.additional_hq_access.saturating_add(*count),
                ServerId::RnD => run.additional_rd_access = run.additional_rd_access.saturating_add(*count),
                // No per-count field exists for these — see the variant's
                // doc comment. Deliberate no-op.
                ServerId::Archives | ServerId::Remote(_) => {}
            }
            Ok(vec![GameEvent::AdditionalAccessGranted { server: *server, count: *count }])
        }

        Effect::SetAccessReplacement { server, effect } => {
            let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
            run.access_replacement = Some((*server, (**effect).clone()));
            Ok(vec![GameEvent::AccessReplacementSet { server: *server }])
        }

        Effect::Sequence(effects) => {
            let mut events = Vec::new();
            for inner in effects {
                events.extend(evaluate_effect(state, inner, acting_card, registry)?);
            }
            Ok(events)
        }

        Effect::LoseCredits(side, amount) => {
            state.resources_mut(*side).credits = Credits(state.resources(*side).credits.0.saturating_sub(*amount));
            Ok(vec![GameEvent::CreditsLost { side: *side, amount: *amount }])
        }

        Effect::LoseClicks(amount) => {
            state.runner.resources.clicks =
                Clicks(state.runner.resources.clicks.0.saturating_sub(*amount));
            Ok(vec![GameEvent::ClicksLost { side: Side::Runner, amount: *amount }])
        }

        Effect::InitiateRun(server) => {
            run::start_run(state, registry, *server)?;
            let run_initiated_event = GameEvent::RunInitiated { server: *server };
            let mut events = vec![run_initiated_event.clone()];
            events.extend(crate::rules::dispatcher::dispatch_event(state, registry, &run_initiated_event)?);
            Ok(events)
        }
    }
}

/// Fires every still-`Pending` subroutine on the ICE currently being
/// encountered, lowest index first, stopping once none are left (or the
/// run/game ends out from under the loop — e.g. an `Effect::EndTheRun`
/// subroutine partway through). "Nothing left to resolve" is this
/// function's normal terminal condition, not a failure: it only returns
/// `Err` if `evaluate_effect` itself errors on one of the fired
/// subroutines' effects, in which case that error propagates immediately
/// and any already-fired subroutines stay fired (no rollback).
pub fn resolve_unbroken_subroutines(
    state: &mut GameState,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let mut events = Vec::new();

    loop {
        if matches!(state.phase, GamePhase::GameOver(_)) {
            break;
        }

        // A subroutine we just fired parked a Trace, which spans two future
        // PlayerActions — stop here rather than firing the next pending
        // subroutine underneath it. `rules::trace::submit_runner_bid` calls
        // this function again once the trace resolves, resuming the loop.
        if state.active_trace.is_some() {
            break;
        }

        // Immutable read only — ends before any mutation below, so it
        // never overlaps with the `&mut state` passed to transition_subroutine/evaluate_effect.
        let Some(index) = state.active_run.as_ref().and_then(|run| {
            let ice = run.ice.get(run.position)?;
            ice.subroutines.iter().position(|s| s.status == SubroutineStatus::Pending)
        }) else {
            break;
        };

        let (card_id, effect) = run::transition_subroutine(state, index, SubroutineStatus::Resolved)?;
        let fired_events = evaluate_effect(state, &effect, None, registry)?;
        events.push(GameEvent::SubroutineFired { card_id, index, effect });
        events.extend(fired_events);

        // If that subroutine's effect was a Trace, mark it so the eventual
        // resolution knows to resume this loop afterward.
        if let Some(trace) = state.active_trace.as_mut() {
            trace.resume = TraceResume::ResumeSubroutines;
        }
    }

    Ok(events)
}

/// Fires every `TriggeredEffect` on `card_id`'s `CardRegistry` definition
/// matching `trigger`, in declaration order, evaluating each contained
/// `Effect` via `evaluate_effect` and collecting events. An unregistered
/// `card_id` (or one with no matching `TriggeredEffect`) is not an error —
/// yields `Ok(Vec::new())`, mirroring `run::access::compute_pending_choice`'s
/// existing "unrecognized card" default. Errors propagate immediately with
/// no rollback (already-fired effects/triggers stay applied), matching
/// `resolve_unbroken_subroutines`'s convention.
pub fn process_card_triggers(
    state: &mut GameState,
    registry: &CardRegistry,
    card_id: &CardId,
    trigger: Trigger,
) -> Result<Vec<GameEvent>, RulesError> {
    let Some(card) = registry.get(card_id) else {
        return Ok(Vec::new());
    };
    let mut events = Vec::new();
    for triggered in card.triggers.iter().filter(|t| t.trigger == trigger) {
        if let Some(requirement) = &triggered.requirement
            && check_requirement(state, requirement).is_err()
        {
            // Soft gate (see `TriggeredEffect::requirement`'s doc comment):
            // unmet just means no bonus this time, not an error propagated
            // to the caller — and no per-turn flag is consumed, since it was
            // never available to begin with.
            continue;
        }
        for effect in &triggered.effects {
            events.extend(evaluate_effect(state, effect, Some(card_id), registry)?);
        }
        if let Some(requirement) = &triggered.requirement {
            consume_requirement(state, requirement);
        }
    }
    Ok(events)
}

fn trash_card(
    state: &mut GameState,
    target: &CardTarget,
    acting_card: Option<&CardId>,
) -> Result<Vec<GameEvent>, RulesError> {
    match target {
        CardTarget::ThisCard => {
            let card_id = acting_card.ok_or(RulesError::MissingActingCardContext)?;
            trash_this_card(state, card_id)
        }

        CardTarget::CorpInstalled { card, server } => {
            let position = state
                .corp
                .installed
                .iter()
                .position(|installed| installed.card == *card && installed.server == *server)
                .ok_or_else(|| RulesError::CardNotInstalled { card: card.clone() })?;
            state.corp.installed.remove(position);
            state.corp.archives.push(card.clone());
            Ok(vec![GameEvent::CardTrashed { side: Side::Corp, card: card.clone() }])
        }

        CardTarget::RunnerRig(card) => {
            let position = state
                .runner
                .rig
                .iter()
                .position(|c| &c.card == card)
                .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: card.clone() })?;
            let removed = state.runner.rig.remove(position);
            state.runner.heap.push(removed.card);
            Ok(vec![GameEvent::CardTrashed { side: Side::Runner, card: card.clone() }])
        }

        CardTarget::TopOfStack { side, zone } => {
            let (deck, pile) = match (side, zone) {
                (Side::Corp, StackZone::RAndD) => (&mut state.corp.r_and_d, &mut state.corp.archives),
                (Side::Runner, StackZone::Stack) => (&mut state.runner.stack, &mut state.runner.heap),
                // Corp has no Stack, Runner has no R&D — no card ever
                // occupies this mismatched combination's "top".
                _ => return Err(RulesError::EmptyZone { side: *side, zone: *zone }),
            };
            match deck.pop() {
                Some(card) => {
                    pile.push(card.clone());
                    Ok(vec![GameEvent::CardTrashed { side: *side, card }])
                }
                None => Err(RulesError::EmptyZone { side: *side, zone: *zone }),
            }
        }
    }
}

/// Locates `card_id` wherever it currently sits — Corp installed, HQ, R&D,
/// or Runner Rig/Grip — and moves it to that side's discard pile, for
/// `CardTarget::ThisCard`/`Cost::TrashSelf` self-reference resolution
/// (unlike `CardTarget::CorpInstalled`/`RunnerRig`, the zone isn't known
/// ahead of time). Not found in any of those zones (e.g. already trashed by
/// an earlier effect in the same resolution, or accessed straight from
/// Archives) is a no-op, mirroring `run::access::move_to_archives`'s
/// existing "already there" leniency, rather than erroring.
fn trash_this_card(state: &mut GameState, card_id: &CardId) -> Result<Vec<GameEvent>, RulesError> {
    if let Some(position) = state.corp.installed.iter().position(|installed| &installed.card == card_id) {
        state.corp.installed.remove(position);
        state.corp.archives.push(card_id.clone());
        return Ok(vec![GameEvent::CardTrashed { side: Side::Corp, card: card_id.clone() }]);
    }
    if let Some(position) = state.corp.hq.iter().position(|c| c == card_id) {
        state.corp.hq.remove(position);
        state.corp.archives.push(card_id.clone());
        return Ok(vec![GameEvent::CardTrashed { side: Side::Corp, card: card_id.clone() }]);
    }
    if let Some(position) = state.corp.r_and_d.iter().position(|c| c == card_id) {
        state.corp.r_and_d.remove(position);
        state.corp.archives.push(card_id.clone());
        return Ok(vec![GameEvent::CardTrashed { side: Side::Corp, card: card_id.clone() }]);
    }
    if let Some(position) = state.runner.rig.iter().position(|c| &c.card == card_id) {
        let removed = state.runner.rig.remove(position);
        state.runner.heap.push(removed.card);
        return Ok(vec![GameEvent::CardTrashed { side: Side::Runner, card: card_id.clone() }]);
    }
    if let Some(position) = state.runner.grip.iter().position(|c| c == card_id) {
        state.runner.grip.remove(position);
        state.runner.heap.push(card_id.clone());
        return Ok(vec![GameEvent::CardTrashed { side: Side::Runner, card: card_id.clone() }]);
    }
    Ok(Vec::new())
}

/// Pays `cost` on `side`'s behalf, mutating `state` in place. Kept as a
/// function separate from `evaluate_effect` — mirroring `AbilityDef`
/// itself already modeling cost and effect as two separate fields — so a
/// future dispatch path calls `pay_cost` then, only on success,
/// `evaluate_effect`, matching real Netrunner's "costs are paid first,
/// then the ability resolves" structure.
///
/// `acting_card` identifies the card whose cost this is — only read by
/// `Cost::TrashSelf`, resolved via `trash_this_card`
/// (`RulesError::MissingActingCardContext` if `None`); every other `Cost`
/// variant ignores it.
pub fn pay_cost(
    state: &mut GameState,
    side: Side,
    cost: &Cost,
    acting_card: Option<&CardId>,
) -> Result<Vec<GameEvent>, RulesError> {
    match cost {
        Cost::Credits(amount) => {
            // During an active run, the Runner draws from their temporary
            // Bad Publicity credit pool (RunState::bad_publicity_credits)
            // before their own wallet — see RunState's doc comment. Corp
            // credit costs, and any Runner cost outside a run, are
            // unaffected: bp_available is unconditionally 0 for them, so
            // this collapses to the original wallet-only behavior.
            let bp_available = match (side, state.active_run.as_ref()) {
                (Side::Runner, Some(run)) => run.bad_publicity_credits,
                _ => 0,
            };
            // Symmetric pool for the Corp: during an active trace, the Corp
            // draws from `CorpState::recurring_credits` before their own
            // wallet (e.g. NBN: Making News). Every `Cost::Credits` payment
            // the Corp makes while a trace is active is necessarily
            // `trace::submit_corp_bid`'s — `engine::apply_action` rejects
            // every other action while `active_trace` is `Some` — so keying
            // on `state.active_trace.is_some()` here is exact, not a
            // heuristic.
            let recurring_available = match (side, state.active_trace.is_some()) {
                (Side::Corp, true) => state.corp.recurring_credits,
                _ => 0,
            };
            let wallet_available = state.resources(side).credits.0;
            let total_available = bp_available.saturating_add(recurring_available).saturating_add(wallet_available);
            if total_available < *amount {
                return Err(RulesError::NotEnoughCredits {
                    side,
                    available: total_available,
                    requested: *amount,
                });
            }

            let from_bp = bp_available.min(*amount);
            let from_recurring = recurring_available.min(*amount - from_bp);
            let from_wallet = amount - from_bp - from_recurring;

            let mut events = Vec::new();
            if from_bp > 0 {
                state
                    .active_run
                    .as_mut()
                    .expect("bp_available > 0 implies an active run")
                    .bad_publicity_credits -= from_bp;
                events.push(GameEvent::BadPublicityCreditsSpent { amount: from_bp });
            }
            if from_recurring > 0 {
                state.corp.recurring_credits -= from_recurring;
                events.push(GameEvent::RecurringCreditsSpent { amount: from_recurring });
            }
            state.resources_mut(side).credits = Credits(wallet_available - from_wallet);
            events.push(GameEvent::CreditsSpent { side, amount: *amount });
            Ok(events)
        }

        Cost::Clicks(amount) => {
            let clicks = state.resources(side).clicks;
            let spent = clicks.spend(*amount).ok_or(RulesError::NotEnoughClicks {
                side,
                available: clicks.0,
                requested: *amount,
            })?;
            state.resources_mut(side).clicks = spent;
            Ok(std::iter::repeat_n(GameEvent::ClickSpent { side }, *amount as usize).collect())
        }

        Cost::TrashSelf => {
            let card_id = acting_card.ok_or(RulesError::MissingActingCardContext)?;
            trash_this_card(state, card_id)
        }

        Cost::PurgeTags => {
            state.runner.tags = 0;
            Ok(vec![GameEvent::TagsPurged { side }])
        }
    }
}

/// Checks an `AbilityDef::requirement` gate before its cost/effect resolve —
/// same "checked before resolution" role as `pay_cost`, but for a
/// precondition rather than a payment. Called from `engine::activate_ability`.
pub fn check_requirement(state: &GameState, requirement: &EffectRequirement) -> Result<(), RulesError> {
    match requirement {
        EffectRequirement::IsTagged => {
            if !state.runner.is_tagged() {
                return Err(RulesError::RunnerNotTagged);
            }
            Ok(())
        }
        EffectRequirement::FirstInstallThisTurn => {
            if state.corp.first_install_used_this_turn {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::FirstSuccessfulHqRunThisTurn => {
            if state.runner.first_hq_run_used_this_turn {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
    }
}

/// Flips the per-turn tracking flag `requirement` gates, once a
/// `TriggeredEffect` it gated has actually fired — see `dsl::card::
/// TriggeredEffect::requirement`'s doc comment. A no-op for `IsTagged`
/// (nothing to consume; tag count isn't a once-per-turn resource). Kept
/// separate from `check_requirement` (which stays read-only) so
/// `activate_ability`'s existing `AbilityDef::requirement` call site is
/// unaffected — only `process_card_triggers`'s soft-gate path calls this.
fn consume_requirement(state: &mut GameState, requirement: &EffectRequirement) {
    match requirement {
        EffectRequirement::IsTagged => {}
        EffectRequirement::FirstInstallThisTurn => state.corp.first_install_used_this_turn = true,
        EffectRequirement::FirstSuccessfulHqRunThisTurn => state.runner.first_hq_run_used_this_turn = true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{Card, CardId, CardType, DamageType, IceType, SubroutineDef, TriggeredEffect};
    use crate::rules::run::{EncounteredSubroutine, RunIce, RunPhase as RP, RunState, ServerId, SubroutineStatus};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, GamePhase, InstallSlot, InstalledCard, InstalledRunnerCard,
        MemoryUnits, PlayerResources, RunnerState,
    };

    fn installed_runner_card(id: &str, base_strength: i32) -> InstalledRunnerCard {
        InstalledRunnerCard {
            card: CardId(id.to_string()),
            base_strength,
            encounter_strength_buff: 0,
            turn_strength_buff: 0,
        }
    }

    fn game_state() -> GameState {
        GameState {
            corp: CorpState { identity: None, bad_publicity: 0, first_install_used_this_turn: false, recurring_credits: 0, recurring_credits_max: 0,
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
            runner: RunnerState { identity: None,
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
                link_strength: 0, first_hq_run_used_this_turn: false, first_install_discount_used_this_turn: false,
            },
            phase: GamePhase::Action(Side::Runner),
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            seed: 0,
            rng_step: 0,
        }
    }

    #[test]
    fn gain_credits_targets_the_named_side() {
        let mut state = game_state();
        let events = evaluate_effect(&mut state, &Effect::GainCredits(Side::Corp, 3), None, &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(8));
        assert_eq!(state.runner.resources.credits, Credits(5));
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Corp, amount: 3 }]);
    }

    #[test]
    fn deal_damage_delegates_to_apply_damage() {
        let mut state = game_state();
        state.runner.grip = vec![CardId("card_0".to_string()), CardId("card_1".to_string())];

        let events = evaluate_effect(&mut state, &Effect::DealDamage(DamageType::Net, 1), None, &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.grip.len(), 1);
        assert_eq!(state.runner.heap.len(), 1);
        assert!(matches!(events[0], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 1 }));
    }

    #[test]
    fn draw_cards_stops_silently_on_an_empty_deck() {
        let mut state = game_state();
        state.runner.stack = vec![CardId("only_card".to_string())];

        let events = evaluate_effect(&mut state, &Effect::DrawCards(Side::Runner, 3), None, &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.grip, vec![CardId("only_card".to_string())]);
        assert!(state.runner.stack.is_empty());
        assert_eq!(events, vec![GameEvent::CardDrawn { side: Side::Runner }]);
    }

    #[test]
    fn end_the_run_clears_active_run_and_emits_event() {
        let mut state = game_state();
        state.active_run = Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0, access_state: None, server: ServerId::Hq, phase: RP::ApproachIce, ice: Vec::new(), position: 0 , jack_out_permitted: true});

        let events = evaluate_effect(&mut state, &Effect::EndTheRun, None, &CardRegistry::new()).unwrap();

        assert!(state.active_run.is_none());
        assert_eq!(events, vec![GameEvent::RunEndedByEffect { server: ServerId::Hq }]);
    }

    #[test]
    fn end_the_run_with_no_active_run_errors() {
        let mut state = game_state();
        assert_eq!(evaluate_effect(&mut state, &Effect::EndTheRun, None, &CardRegistry::new()), Err(RulesError::NoActiveRun));
    }

    fn active_run_state() -> RunState {
        RunState {
            additional_rd_access: 0,
            additional_hq_access: 0,
            access_replacement: None,
            bad_publicity_credits: 0,
            access_state: None,
            server: ServerId::Hq,
            phase: RP::ApproachIce,
            ice: Vec::new(),
            position: 0,
            jack_out_permitted: true,
        }
    }

    #[test]
    fn add_additional_access_increments_the_matching_field() {
        let mut state = game_state();
        state.active_run = Some(active_run_state());

        let events =
            evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 }, None, &CardRegistry::new())
                .unwrap();
        assert_eq!(state.active_run.as_ref().unwrap().additional_hq_access, 1);
        assert_eq!(events, vec![GameEvent::AdditionalAccessGranted { server: ServerId::Hq, count: 1 }]);

        let events =
            evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::RnD, count: 2 }, None, &CardRegistry::new())
                .unwrap();
        assert_eq!(state.active_run.as_ref().unwrap().additional_rd_access, 2);
        assert_eq!(events, vec![GameEvent::AdditionalAccessGranted { server: ServerId::RnD, count: 2 }]);
    }

    #[test]
    fn add_additional_access_stacks_additively() {
        let mut state = game_state();
        state.active_run = Some(active_run_state());

        evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 }, None, &CardRegistry::new()).unwrap();
        evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 }, None, &CardRegistry::new()).unwrap();

        assert_eq!(state.active_run.as_ref().unwrap().additional_hq_access, 2);
    }

    #[test]
    fn add_additional_access_no_ops_for_archives_and_remote() {
        let mut state = game_state();
        state.active_run = Some(active_run_state());

        evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Archives, count: 3 }, None, &CardRegistry::new())
            .unwrap();
        evaluate_effect(
            &mut state,
            &Effect::AddAdditionalAccess { server: ServerId::Remote(0), count: 3 },
            None,
            &CardRegistry::new(),
        )
        .unwrap();

        let run = state.active_run.as_ref().unwrap();
        assert_eq!(run.additional_hq_access, 0);
        assert_eq!(run.additional_rd_access, 0);
    }

    #[test]
    fn add_additional_access_without_an_active_run_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 }, None, &CardRegistry::new()),
            Err(RulesError::NoActiveRun)
        );
    }

    #[test]
    fn set_access_replacement_stores_the_pending_effect() {
        let mut state = game_state();
        state.active_run = Some(active_run_state());
        let replacement = Effect::GainCredits(Side::Runner, 8);

        let events = evaluate_effect(
            &mut state,
            &Effect::SetAccessReplacement { server: ServerId::Hq, effect: Box::new(replacement.clone()) },
            None,
            &CardRegistry::new(),
        )
        .unwrap();

        assert_eq!(
            state.active_run.as_ref().unwrap().access_replacement,
            Some((ServerId::Hq, replacement))
        );
        assert_eq!(events, vec![GameEvent::AccessReplacementSet { server: ServerId::Hq }]);
    }

    #[test]
    fn set_access_replacement_without_an_active_run_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::SetAccessReplacement {
                    server: ServerId::Hq,
                    effect: Box::new(Effect::GainCredits(Side::Runner, 8)),
                },
                None,
                &CardRegistry::new(),
            ),
            Err(RulesError::NoActiveRun)
        );
    }

    #[test]
    fn give_tags_always_targets_the_runner() {
        let mut state = game_state();
        let events = evaluate_effect(&mut state, &Effect::GiveTags(2), None, &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.tags, 2);
        assert_eq!(events, vec![GameEvent::TagsGiven { side: Side::Runner, amount: 2 }]);
    }

    #[test]
    fn remove_tags_saturates_at_zero() {
        let mut state = game_state();
        state.runner.tags = 1;
        let events = evaluate_effect(&mut state, &Effect::RemoveTags(5), None, &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.tags, 0);
        assert_eq!(events, vec![GameEvent::TagsRemoved { side: Side::Runner, amount: 5 }]);
    }

    #[test]
    fn give_bad_publicity_increases_the_counter() {
        let mut state = game_state();
        let events = evaluate_effect(&mut state, &Effect::GiveBadPublicity(2), None, &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.bad_publicity, 2);
        assert_eq!(events, vec![GameEvent::BadPublicityGiven { amount: 2 }]);
    }

    #[test]
    fn remove_bad_publicity_saturates_at_zero() {
        let mut state = game_state();
        state.corp.bad_publicity = 1;
        let events = evaluate_effect(&mut state, &Effect::RemoveBadPublicity(5), None, &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.bad_publicity, 0);
        assert_eq!(events, vec![GameEvent::BadPublicityRemoved { amount: 5 }]);
    }

    #[test]
    fn is_tagged_requirement_fails_with_zero_tags_and_succeeds_with_a_tag() {
        let mut state = game_state();
        assert_eq!(
            check_requirement(&state, &EffectRequirement::IsTagged),
            Err(RulesError::RunnerNotTagged)
        );

        state.runner.tags = 1;
        assert_eq!(check_requirement(&state, &EffectRequirement::IsTagged), Ok(()));
    }

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

    #[test]
    fn break_subroutine_breaks_the_targeted_pending_subroutine() {
        let mut state = game_state();
        state.active_run = Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 2, true)],
            position: 0,
         jack_out_permitted: true,});

        let events = evaluate_effect(&mut state, &Effect::BreakSubroutine(0), None, &CardRegistry::new()).unwrap();

        assert_eq!(
            events,
            vec![GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0 }]
        );
        let run = state.active_run.unwrap();
        assert_eq!(run.ice[0].subroutines[0].status, SubroutineStatus::Broken);
        assert_eq!(run.ice[0].subroutines[1].status, SubroutineStatus::Pending);
    }

    #[test]
    fn break_subroutine_out_of_range_index_errors() {
        let mut state = game_state();
        state.active_run = Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            position: 0,
         jack_out_permitted: true,});

        assert_eq!(
            evaluate_effect(&mut state, &Effect::BreakSubroutine(1), None, &CardRegistry::new()),
            Err(RulesError::InvalidSubroutineIndex(1))
        );
    }

    #[test]
    fn break_subroutine_already_broken_errors() {
        let mut state = game_state();
        let mut ice = test_ice("ice_wall", 0, 1, true);
        ice.subroutines[0].status = SubroutineStatus::Broken;
        state.active_run =
            Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0, access_state: None, server: ServerId::Hq, phase: RP::EncounterIce, ice: vec![ice], position: 0 , jack_out_permitted: true});

        assert_eq!(
            evaluate_effect(&mut state, &Effect::BreakSubroutine(0), None, &CardRegistry::new()),
            Err(RulesError::SubroutineAlreadyHandled)
        );
    }

    #[test]
    fn break_subroutine_with_no_active_run_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::BreakSubroutine(0), None, &CardRegistry::new()),
            Err(RulesError::NoActiveRun)
        );
    }

    #[test]
    fn break_subroutine_outside_encounter_ice_errors() {
        let mut state = game_state();
        state.active_run =
            Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0, access_state: None, server: ServerId::Hq, phase: RP::ApproachIce, ice: Vec::new(), position: 0 , jack_out_permitted: true});

        assert_eq!(
            evaluate_effect(&mut state, &Effect::BreakSubroutine(0), None, &CardRegistry::new()),
            Err(RulesError::NotInEncounter)
        );
    }

    #[test]
    fn resolve_unbroken_subroutines_resolves_each_pending_subroutine_in_order() {
        let mut state = game_state();
        let mut ice = test_ice("ice_wall", 0, 2, true);
        ice.subroutines[0].definition.effect = Effect::GiveTags(2);
        ice.subroutines[1].definition.effect = Effect::GainCredits(Side::Corp, 3);
        state.active_run = Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![ice],
            position: 0,
         jack_out_permitted: true,});

        let events = resolve_unbroken_subroutines(&mut state, &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.tags, 2);
        assert_eq!(state.corp.resources.credits, Credits(8));

        let run = state.active_run.unwrap();
        assert_eq!(run.ice[0].subroutines[0].status, SubroutineStatus::Resolved);
        assert_eq!(run.ice[0].subroutines[1].status, SubroutineStatus::Resolved);

        assert_eq!(
            events,
            vec![
                GameEvent::SubroutineFired {
                    card_id: CardId("ice_wall".to_string()),
                    index: 0,
                    effect: Effect::GiveTags(2),
                },
                GameEvent::TagsGiven { side: Side::Runner, amount: 2 },
                GameEvent::SubroutineFired {
                    card_id: CardId("ice_wall".to_string()),
                    index: 1,
                    effect: Effect::GainCredits(Side::Corp, 3),
                },
                GameEvent::CreditsGained { side: Side::Corp, amount: 3 },
            ]
        );
    }

    #[test]
    fn resolve_unbroken_subroutines_stops_at_end_the_run() {
        let mut state = game_state();
        let mut ice = test_ice("ice_wall", 0, 2, true);
        ice.subroutines[0].definition.effect = Effect::EndTheRun;
        ice.subroutines[1].definition.effect = Effect::GiveTags(5);
        state.active_run = Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![ice],
            position: 0,
         jack_out_permitted: true,});

        let events = resolve_unbroken_subroutines(&mut state, &CardRegistry::new()).unwrap();

        assert!(state.active_run.is_none());
        assert_eq!(state.runner.tags, 0);
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
    fn resolve_unbroken_subroutines_skips_already_handled_subroutines() {
        let mut state = game_state();
        let mut ice = test_ice("ice_wall", 0, 2, true);
        ice.subroutines[0].status = SubroutineStatus::Broken;
        ice.subroutines[1].definition.effect = Effect::GiveTags(1);
        state.active_run = Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![ice],
            position: 0,
         jack_out_permitted: true,});

        let events = resolve_unbroken_subroutines(&mut state, &CardRegistry::new()).unwrap();

        assert_eq!(events.len(), 2);
        let run = state.active_run.unwrap();
        assert_eq!(run.ice[0].subroutines[0].status, SubroutineStatus::Broken);
        assert_eq!(run.ice[0].subroutines[1].status, SubroutineStatus::Resolved);
    }

    #[test]
    fn modify_strength_updates_current_strength_and_emits_event() {
        let mut state = game_state();
        state.active_run = Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0, access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![test_ice("ice_wall", 3, 0, true)],
            position: 0,
         jack_out_permitted: true,});

        let events = evaluate_effect(&mut state, &Effect::ModifyStrength(2), None, &CardRegistry::new()).unwrap();

        assert_eq!(state.active_run.unwrap().ice[0].current_strength, 5);
        assert_eq!(
            events,
            vec![GameEvent::IceStrengthModified {
                card_id: CardId("ice_wall".to_string()),
                new_strength: 5,
                delta: 2,
            }]
        );
    }

    #[test]
    fn modify_strength_outside_encounter_ice_errors() {
        let mut state = game_state();
        state.active_run =
            Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0, access_state: None, server: ServerId::Hq, phase: RP::ApproachIce, ice: Vec::new(), position: 0 , jack_out_permitted: true});

        assert_eq!(
            evaluate_effect(&mut state, &Effect::ModifyStrength(2), None, &CardRegistry::new()),
            Err(RulesError::NotInEncounter)
        );
    }

    #[test]
    fn modify_strength_with_no_active_run_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::ModifyStrength(2), None, &CardRegistry::new()),
            Err(RulesError::NoActiveRun)
        );
    }

    #[test]
    fn trash_card_this_card_without_acting_card_is_rejected_not_panicked() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::TrashCard(CardTarget::ThisCard), None, &CardRegistry::new()),
            Err(RulesError::MissingActingCardContext)
        );
    }

    #[test]
    fn trash_card_this_card_with_acting_card_moves_it_to_the_heap() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("gordian_blade", 2)];
        let acting = CardId("gordian_blade".to_string());

        let events =
            evaluate_effect(&mut state, &Effect::TrashCard(CardTarget::ThisCard), Some(&acting), &CardRegistry::new()).unwrap();

        assert!(state.runner.rig.is_empty());
        assert_eq!(state.runner.heap, vec![acting.clone()]);
        assert_eq!(events, vec![GameEvent::CardTrashed { side: Side::Runner, card: acting }]);
    }

    #[test]
    fn trash_card_corp_installed_moves_card_to_archives() {
        let mut state = game_state();
        state.corp.installed.push(InstalledCard {
            advancement_tokens: 0,
            card: CardId("pad_campaign".to_string()),
            server: ServerId::Remote(0),
            slot: InstallSlot::Root,
            rezzed: true,
        });

        let events = evaluate_effect(
            &mut state,
            &Effect::TrashCard(CardTarget::CorpInstalled {
                card: CardId("pad_campaign".to_string()),
                server: ServerId::Remote(0),
            }),
            None,
            &CardRegistry::new(),
        )
        .unwrap();

        assert!(state.corp.installed.is_empty());
        assert_eq!(state.corp.archives, vec![CardId("pad_campaign".to_string())]);
        assert_eq!(
            events,
            vec![GameEvent::CardTrashed { side: Side::Corp, card: CardId("pad_campaign".to_string()) }]
        );
    }

    #[test]
    fn trash_card_runner_rig_not_found_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::TrashCard(CardTarget::RunnerRig(CardId("gordian_blade".to_string()))), None, &CardRegistry::new()),
            Err(RulesError::CardNotInRig { side: Side::Runner, card: CardId("gordian_blade".to_string()) })
        );
    }

    #[test]
    fn trash_card_runner_rig_moves_card_to_heap() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("gordian_blade", 2)];

        let events = evaluate_effect(
            &mut state,
            &Effect::TrashCard(CardTarget::RunnerRig(CardId("gordian_blade".to_string()))),
            None,
            &CardRegistry::new(),
        )
        .unwrap();

        assert!(state.runner.rig.is_empty());
        assert_eq!(state.runner.heap, vec![CardId("gordian_blade".to_string())]);
        assert_eq!(
            events,
            vec![GameEvent::CardTrashed { side: Side::Runner, card: CardId("gordian_blade".to_string()) }]
        );
    }

    #[test]
    fn trash_card_top_of_stack_mills_from_the_correct_zone() {
        let mut state = game_state();
        state.corp.r_and_d = vec![CardId("ice_wall".to_string()), CardId("hedge_fund".to_string())];

        let events = evaluate_effect(
            &mut state,
            &Effect::TrashCard(CardTarget::TopOfStack { side: Side::Corp, zone: StackZone::RAndD }),
            None,
            &CardRegistry::new(),
        )
        .unwrap();

        assert_eq!(state.corp.r_and_d, vec![CardId("ice_wall".to_string())]);
        assert_eq!(state.corp.archives, vec![CardId("hedge_fund".to_string())]);
        assert_eq!(
            events,
            vec![GameEvent::CardTrashed { side: Side::Corp, card: CardId("hedge_fund".to_string()) }]
        );
    }

    #[test]
    fn trash_card_top_of_stack_mismatched_zone_errors() {
        let mut state = game_state();
        state.corp.r_and_d = vec![CardId("hedge_fund".to_string())];

        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::TrashCard(CardTarget::TopOfStack { side: Side::Corp, zone: StackZone::Stack }),
                None,
                &CardRegistry::new(),
            ),
            Err(RulesError::EmptyZone { side: Side::Corp, zone: StackZone::Stack })
        );
    }

    #[test]
    fn trash_card_top_of_stack_runner_mills_from_the_stack() {
        let mut state = game_state();
        state.runner.stack = vec![CardId("clone_chip".to_string()), CardId("sure_gamble".to_string())];

        let events = evaluate_effect(
            &mut state,
            &Effect::TrashCard(CardTarget::TopOfStack { side: Side::Runner, zone: StackZone::Stack }),
            None,
            &CardRegistry::new(),
        )
        .unwrap();

        assert_eq!(state.runner.stack, vec![CardId("clone_chip".to_string())]);
        assert_eq!(state.runner.heap, vec![CardId("sure_gamble".to_string())]);
        assert_eq!(
            events,
            vec![GameEvent::CardTrashed { side: Side::Runner, card: CardId("sure_gamble".to_string()) }]
        );
    }

    #[test]
    fn trash_card_top_of_stack_with_empty_deck_errors() {
        let mut state = game_state();
        // corp.r_and_d is empty by default in game_state() — a valid
        // side/zone combo, unlike the mismatched-combo case above.
        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::TrashCard(CardTarget::TopOfStack { side: Side::Corp, zone: StackZone::RAndD }),
                None,
                &CardRegistry::new(),
            ),
            Err(RulesError::EmptyZone { side: Side::Corp, zone: StackZone::RAndD })
        );
    }

    #[test]
    fn pay_credits_deducts_and_errors_when_insufficient() {
        let mut state = game_state();
        let events = pay_cost(&mut state, Side::Corp, &Cost::Credits(3), None).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(2));
        assert_eq!(events, vec![GameEvent::CreditsSpent { side: Side::Corp, amount: 3 }]);

        assert_eq!(
            pay_cost(&mut state, Side::Corp, &Cost::Credits(10), None),
            Err(RulesError::NotEnoughCredits { side: Side::Corp, available: 2, requested: 10 })
        );
    }

    fn run_with_bad_publicity_credits(amount: u32) -> RunState {
        RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None,
            bad_publicity_credits: amount,
            access_state: None,
            server: ServerId::Hq,
            phase: RP::ApproachIce,
            ice: Vec::new(),
            position: 0,
            jack_out_permitted: true,
        }
    }

    #[test]
    fn pay_credits_draws_from_bad_publicity_pool_before_the_wallet_during_a_run() {
        let mut state = game_state();
        state.active_run = Some(run_with_bad_publicity_credits(3));

        let events = pay_cost(&mut state, Side::Runner, &Cost::Credits(5), None).unwrap();

        assert_eq!(state.active_run.as_ref().unwrap().bad_publicity_credits, 0);
        assert_eq!(state.runner.resources.credits, Credits(3)); // 5 wallet - (5 - 3 from BP)
        assert_eq!(
            events,
            vec![
                GameEvent::BadPublicityCreditsSpent { amount: 3 },
                GameEvent::CreditsSpent { side: Side::Runner, amount: 5 },
            ]
        );
    }

    #[test]
    fn pay_credits_with_no_active_run_ignores_bad_publicity_pool() {
        // Regression guard: behavior/events must be byte-identical to the
        // pre-Bad-Publicity path when there's no run to draw from.
        let mut state = game_state();
        let events = pay_cost(&mut state, Side::Runner, &Cost::Credits(2), None).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(3));
        assert_eq!(events, vec![GameEvent::CreditsSpent { side: Side::Runner, amount: 2 }]);
    }

    #[test]
    fn pay_credits_with_zero_bad_publicity_credits_behaves_like_wallet_only() {
        let mut state = game_state();
        state.active_run = Some(run_with_bad_publicity_credits(0));

        let events = pay_cost(&mut state, Side::Runner, &Cost::Credits(2), None).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(3));
        assert_eq!(events, vec![GameEvent::CreditsSpent { side: Side::Runner, amount: 2 }]);
    }

    #[test]
    fn pay_credits_insufficient_across_both_pools_reports_combined_available_and_leaves_state_untouched() {
        let mut state = game_state();
        state.runner.resources.credits = Credits(1);
        state.active_run = Some(run_with_bad_publicity_credits(1));

        let result = pay_cost(&mut state, Side::Runner, &Cost::Credits(5), None);

        assert_eq!(
            result,
            Err(RulesError::NotEnoughCredits { side: Side::Runner, available: 2, requested: 5 })
        );
        assert_eq!(state.runner.resources.credits, Credits(1));
        assert_eq!(state.active_run.as_ref().unwrap().bad_publicity_credits, 1);
    }

    #[test]
    fn pay_credits_corp_side_is_unaffected_by_runner_bad_publicity_pool() {
        let mut state = game_state();
        state.active_run = Some(run_with_bad_publicity_credits(10));

        let events = pay_cost(&mut state, Side::Corp, &Cost::Credits(3), None).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(2));
        assert_eq!(state.active_run.as_ref().unwrap().bad_publicity_credits, 10);
        assert_eq!(events, vec![GameEvent::CreditsSpent { side: Side::Corp, amount: 3 }]);
    }

    #[test]
    fn pay_clicks_spends_the_requested_amount() {
        let mut state = game_state();
        let events = pay_cost(&mut state, Side::Runner, &Cost::Clicks(2), None).unwrap();

        assert_eq!(state.runner.resources.clicks, Clicks(2));
        assert_eq!(events, vec![GameEvent::ClickSpent { side: Side::Runner }, GameEvent::ClickSpent { side: Side::Runner }]);
    }

    #[test]
    fn pay_purge_tags_zeroes_the_counter() {
        let mut state = game_state();
        state.runner.tags = 3;

        let events = pay_cost(&mut state, Side::Runner, &Cost::PurgeTags, None).unwrap();

        assert_eq!(state.runner.tags, 0);
        assert_eq!(events, vec![GameEvent::TagsPurged { side: Side::Runner }]);
    }

    #[test]
    fn pay_trash_self_without_acting_card_is_rejected_not_panicked() {
        let mut state = game_state();
        assert_eq!(
            pay_cost(&mut state, Side::Runner, &Cost::TrashSelf, None),
            Err(RulesError::MissingActingCardContext)
        );
    }

    #[test]
    fn pay_trash_self_with_acting_card_trashes_it_to_the_heap() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("self_modifying_code", 0)];
        let acting = CardId("self_modifying_code".to_string());

        let events = pay_cost(&mut state, Side::Runner, &Cost::TrashSelf, Some(&acting)).unwrap();

        assert!(state.runner.rig.is_empty());
        assert_eq!(state.runner.heap, vec![acting.clone()]);
        assert_eq!(events, vec![GameEvent::CardTrashed { side: Side::Runner, card: acting }]);
    }

    /// A minimal `Card` carrying exactly the given `triggers` — everything
    /// else is irrelevant to `process_card_triggers` tests.
    fn card_with_triggers(id: &str, triggers: Vec<TriggeredEffect>) -> Card {
        Card {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Asset,
            cost: 0,
            triggers,
            abilities: Vec::new(),
            trash_cost: None,
            steal_cost: None,
            advancement_requirement: None,
            agenda_points: None,
            min_deck_size: None,
            strength: None,
            subroutines: Vec::new(),
            interactive_on_access: None, subtypes: Vec::new(), play_requirement: None, recurring_credits: None, first_install_discount: None,
        }
    }

    #[test]
    fn process_card_triggers_fires_all_effects_of_a_matching_trigger_in_order() {
        let mut state = game_state();
        let registry = CardRegistry::from_cards(vec![card_with_triggers(
            "snare",
            vec![TriggeredEffect {
                trigger: Trigger::OnAccessed,
                effects: vec![Effect::GiveTags(1), Effect::GainCredits(Side::Corp, 2)],
                requirement: None,
            }],
        )]);

        let events = process_card_triggers(
            &mut state,
            &registry,
            &CardId("snare".to_string()),
            Trigger::OnAccessed,
        )
        .unwrap();

        assert_eq!(state.runner.tags, 1);
        assert_eq!(state.corp.resources.credits, Credits(7));
        assert_eq!(
            events,
            vec![
                GameEvent::TagsGiven { side: Side::Runner, amount: 1 },
                GameEvent::CreditsGained { side: Side::Corp, amount: 2 },
            ]
        );
    }

    #[test]
    fn process_card_triggers_ignores_non_matching_triggers() {
        let mut state = game_state();
        let registry = CardRegistry::from_cards(vec![card_with_triggers(
            "hedge_fund",
            vec![TriggeredEffect {
                trigger: Trigger::OnPlay,
                effects: vec![Effect::GainCredits(Side::Corp, 9)],
                requirement: None,
            }],
        )]);

        let events = process_card_triggers(
            &mut state,
            &registry,
            &CardId("hedge_fund".to_string()),
            Trigger::OnAccessed,
        )
        .unwrap();

        assert!(events.is_empty());
        assert_eq!(state.corp.resources.credits, Credits(5));
    }

    #[test]
    fn process_card_triggers_with_unregistered_card_yields_no_events() {
        let mut state = game_state();
        let registry = CardRegistry::new();

        let events = process_card_triggers(
            &mut state,
            &registry,
            &CardId("unregistered".to_string()),
            Trigger::OnAccessed,
        )
        .unwrap();

        assert!(events.is_empty());
    }

    #[test]
    fn boost_strength_encounter_increments_buff_and_effective_strength() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("corroder", 2)];
        let acting = CardId("corroder".to_string());

        let events = evaluate_effect(
            &mut state,
            &Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter },
            Some(&acting),
            &CardRegistry::new(),
        )
        .unwrap();

        assert_eq!(state.runner.rig[0].encounter_strength_buff, 1);
        assert_eq!(state.runner.rig[0].effective_strength(), 3);
        assert_eq!(
            events,
            vec![GameEvent::StrengthBoosted {
                card_id: acting,
                new_strength: 3,
                delta: 1,
                duration: BoostDuration::Encounter,
            }]
        );
    }

    #[test]
    fn boost_strength_turn_increments_turn_buff() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("corroder", 2)];
        let acting = CardId("corroder".to_string());

        evaluate_effect(
            &mut state,
            &Effect::BoostStrength { amount: 2, duration: BoostDuration::Turn },
            Some(&acting),
            &CardRegistry::new(),
        )
        .unwrap();

        assert_eq!(state.runner.rig[0].turn_strength_buff, 2);
        assert_eq!(state.runner.rig[0].encounter_strength_buff, 0);
        assert_eq!(state.runner.rig[0].effective_strength(), 4);
    }

    #[test]
    fn boost_strength_without_acting_card_errors_unresolved_card_target() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter },
                None,
                &CardRegistry::new(),
            ),
            Err(RulesError::UnresolvedCardTarget)
        );
    }

    #[test]
    fn boost_strength_acting_card_not_in_rig_errors_card_not_in_rig() {
        let mut state = game_state();
        let acting = CardId("corroder".to_string());
        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter },
                Some(&acting),
                &CardRegistry::new(),
            ),
            Err(RulesError::CardNotInRig { side: Side::Runner, card: acting })
        );
    }

    fn ice_encounter_state(rig: Vec<InstalledRunnerCard>, ice_strength: i32, subroutine_count: usize) -> GameState {
        let mut state = game_state();
        state.runner.rig = rig;
        state.active_run = Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0,
            access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![test_ice("ice_wall", ice_strength, subroutine_count, true)],
            position: 0,
            jack_out_permitted: true,
        });
        state
    }

    #[test]
    fn break_subroutines_fixed_breaks_up_to_count_pending_lowest_id_first() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 2, 3);
        let acting = CardId("corroder".to_string());

        let events = evaluate_effect(
            &mut state,
            &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(2), restrict_to: None },
            Some(&acting),
            &CardRegistry::new(),
        )
        .unwrap();

        let ice = &state.active_run.unwrap().ice[0];
        assert_eq!(ice.subroutines[0].status, SubroutineStatus::Broken);
        assert_eq!(ice.subroutines[1].status, SubroutineStatus::Broken);
        assert_eq!(ice.subroutines[2].status, SubroutineStatus::Pending);
        assert_eq!(
            events,
            vec![
                GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0 },
                GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 1 },
            ]
        );
    }

    #[test]
    fn break_subroutines_fixed_breaks_fewer_when_fewer_are_pending() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 2, 1);
        let acting = CardId("corroder".to_string());

        let events = evaluate_effect(
            &mut state,
            &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(2), restrict_to: None },
            Some(&acting),
            &CardRegistry::new(),
        )
        .unwrap();

        assert_eq!(events.len(), 1);
        let ice = &state.active_run.unwrap().ice[0];
        assert_eq!(ice.subroutines[0].status, SubroutineStatus::Broken);
    }

    #[test]
    fn break_subroutines_all_breaks_every_pending_subroutine() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 2, 3);
        let acting = CardId("corroder".to_string());

        let events = evaluate_effect(
            &mut state,
            &Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: None },
            Some(&acting),
            &CardRegistry::new(),
        )
        .unwrap();

        assert_eq!(events.len(), 3);
        let ice = state.active_run.unwrap().ice;
        assert!(ice[0].subroutines.iter().all(|s| s.status == SubroutineStatus::Broken));
    }

    #[test]
    fn break_subroutines_outside_encounter_ice_errors_not_in_encounter() {
        let mut state = game_state();
        state.active_run = Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0,
            access_state: None,
            server: ServerId::Hq,
            phase: RP::ApproachIce,
            ice: Vec::new(),
            position: 0,
            jack_out_permitted: true,
        });

        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: None },
                Some(&CardId("corroder".to_string())),
                &CardRegistry::new(),
            ),
            Err(RulesError::NotInEncounter)
        );
    }

    #[test]
    fn break_subroutines_with_insufficient_breaker_strength_errors_breaker_strength_too_low() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 1)], 3, 1);
        let acting = CardId("corroder".to_string());

        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None },
                Some(&acting),
                &CardRegistry::new(),
            ),
            Err(RulesError::BreakerStrengthTooLow {
                breaker: acting,
                breaker_strength: 1,
                ice: CardId("ice_wall".to_string()),
                ice_strength: 3,
            })
        );
        let ice = &state.active_run.unwrap().ice[0];
        assert_eq!(ice.subroutines[0].status, SubroutineStatus::Pending);
    }

    #[test]
    fn break_subroutines_after_boost_succeeds_and_marks_subroutines_broken() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 1)], 2, 1);
        let acting = CardId("corroder".to_string());

        // Too weak before boosting.
        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None },
                Some(&acting),
                &CardRegistry::new(),
            ),
            Err(RulesError::BreakerStrengthTooLow {
                breaker: acting.clone(),
                breaker_strength: 1,
                ice: CardId("ice_wall".to_string()),
                ice_strength: 2,
            })
        );

        evaluate_effect(
            &mut state,
            &Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter },
            Some(&acting),
            &CardRegistry::new(),
        )
        .unwrap();

        let events = evaluate_effect(
            &mut state,
            &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None },
            Some(&acting),
            &CardRegistry::new(),
        )
        .unwrap();

        assert_eq!(
            events,
            vec![GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0 }]
        );
        let ice = &state.active_run.unwrap().ice[0];
        assert_eq!(ice.subroutines[0].status, SubroutineStatus::Broken);
    }

    #[test]
    fn break_subroutines_skips_already_broken_subroutines() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 2, 2);
        state.active_run.as_mut().unwrap().ice[0].subroutines[0].status = SubroutineStatus::Broken;
        let acting = CardId("corroder".to_string());

        let events = evaluate_effect(
            &mut state,
            &Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: None },
            Some(&acting),
            &CardRegistry::new(),
        )
        .unwrap();

        assert_eq!(events, vec![GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 1 }]);
    }

    fn ice_encounter_state_of_type(
        rig: Vec<InstalledRunnerCard>,
        ice_strength: i32,
        subroutine_count: usize,
        ice_type: IceType,
    ) -> GameState {
        let mut state = game_state();
        state.runner.rig = rig;
        state.active_run = Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0,
            access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![test_ice_of_type("ice_wall", ice_strength, subroutine_count, true, ice_type)],
            position: 0,
            jack_out_permitted: true,
        });
        state
    }

    #[test]
    fn break_subroutines_restrict_to_matching_ice_type_succeeds() {
        let mut state =
            ice_encounter_state_of_type(vec![installed_runner_card("corroder", 2)], 2, 1, IceType::Barrier);
        let acting = CardId("corroder".to_string());

        let events = evaluate_effect(
            &mut state,
            &Effect::BreakSubroutines {
                count: SubroutineBreakCount::Fixed(1),
                restrict_to: Some(IceType::Barrier),
            },
            Some(&acting),
            &CardRegistry::new(),
        )
        .unwrap();

        assert_eq!(
            events,
            vec![GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0 }]
        );
    }

    #[test]
    fn break_subroutines_restrict_to_mismatched_ice_type_errors_invalid_breaker_subtype() {
        let mut state =
            ice_encounter_state_of_type(vec![installed_runner_card("corroder", 2)], 2, 1, IceType::CodeGate);
        let acting = CardId("corroder".to_string());

        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BreakSubroutines {
                    count: SubroutineBreakCount::Fixed(1),
                    restrict_to: Some(IceType::Barrier),
                },
                Some(&acting),
                &CardRegistry::new(),
            ),
            Err(RulesError::InvalidBreakerSubtype {
                breaker: acting,
                ice: CardId("ice_wall".to_string()),
                expected: IceType::Barrier,
            })
        );
        let ice = &state.active_run.unwrap().ice[0];
        assert_eq!(ice.subroutines[0].status, SubroutineStatus::Pending);
    }

    #[test]
    fn break_subroutines_with_no_restrict_to_breaks_any_ice_type() {
        for ice_type in [IceType::Barrier, IceType::CodeGate, IceType::Sentry] {
            let mut state =
                ice_encounter_state_of_type(vec![installed_runner_card("mimic", 2)], 2, 1, ice_type);
            let acting = CardId("mimic".to_string());

            let events = evaluate_effect(
                &mut state,
                &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None },
                Some(&acting),
                &CardRegistry::new(),
            )
            .unwrap();

            assert_eq!(
                events,
                vec![GameEvent::SubroutineBroken { card_id: CardId("ice_wall".to_string()), index: 0 }]
            );
        }
    }

    #[test]
    fn trace_effect_parks_pending_state_and_does_not_resolve_immediately() {
        let mut state = game_state();
        let effect = Effect::Trace { base: 3, on_success: Box::new(Effect::GiveTags(1)) };

        let events = evaluate_effect(&mut state, &effect, None, &CardRegistry::new()).unwrap();

        assert_eq!(events, vec![GameEvent::TraceInitiated { base: 3, initiating_card: None }]);
        assert_eq!(state.runner.tags, 0, "on_success must not fire yet");
        let trace = state.active_trace.expect("trace should be parked");
        assert_eq!(trace.base_strength, 3);
        assert_eq!(trace.corp_bid, None);
        assert_eq!(trace.effect_on_success, Effect::GiveTags(1));
        assert_eq!(trace.resume, TraceResume::None);
    }

    #[test]
    fn trace_effect_while_already_active_errors() {
        let mut state = game_state();
        evaluate_effect(&mut state, &Effect::Trace { base: 3, on_success: Box::new(Effect::GiveTags(1)) }, None, &CardRegistry::new())
            .unwrap();

        let result =
            evaluate_effect(&mut state, &Effect::Trace { base: 5, on_success: Box::new(Effect::GiveTags(2)) }, None, &CardRegistry::new());

        assert_eq!(result, Err(RulesError::TraceAlreadyActive));
        assert_eq!(state.active_trace.unwrap().base_strength, 3, "original trace must be untouched");
    }

    #[test]
    fn resolve_unbroken_subroutines_stops_at_a_trace_subroutine_and_marks_resume() {
        let mut state = game_state();
        let mut ice = test_ice("ice_wall", 0, 2, true);
        ice.subroutines[0].definition.effect =
            Effect::Trace { base: 2, on_success: Box::new(Effect::EndTheRun) };
        ice.subroutines[1].definition.effect = Effect::GiveTags(5);
        state.active_run = Some(RunState { additional_rd_access: 0, additional_hq_access: 0, access_replacement: None, bad_publicity_credits: 0,
            access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![ice],
            position: 0,
            jack_out_permitted: true,
        });

        let events = resolve_unbroken_subroutines(&mut state, &CardRegistry::new()).unwrap();

        let run = state.active_run.as_ref().unwrap();
        assert_eq!(run.ice[0].subroutines[0].status, SubroutineStatus::Resolved);
        assert_eq!(run.ice[0].subroutines[1].status, SubroutineStatus::Pending, "must not fire while trace pending");
        let trace = state.active_trace.expect("trace should be parked");
        assert_eq!(trace.resume, TraceResume::ResumeSubroutines);
        assert_eq!(
            events,
            vec![
                GameEvent::SubroutineFired {
                    card_id: CardId("ice_wall".to_string()),
                    index: 0,
                    effect: Effect::Trace { base: 2, on_success: Box::new(Effect::EndTheRun) },
                },
                GameEvent::TraceInitiated { base: 2, initiating_card: None },
            ]
        );
    }
}
