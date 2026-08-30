use crate::cards::CardRegistry;
use crate::dsl::{
    card_matches_filter, Amount, BoostDuration, CardFilter, CardId, CardTarget, Cost, Effect, EffectRequirement, StackZone,
    StrengthModifier, SubroutineBreakCount, Trigger,
};
use crate::rules::damage;
use crate::rules::dispatcher;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::paid_ability;
use crate::rules::run::{self, AccessPhase, RunPhase, ServerId, SubroutineStatus};
use crate::rules::state::{
    ArchivedCard, Clicks, CompletedRun, Credits, GamePhase, GameState, InstalledRunnerCard, PendingChoiceResume, PendingDecision, PendingPaidChoice,
    PendingPaidChoiceResume, PendingPrevention, PendingPreventionKind, PreventionKind, PreventionResume, Side,
    TraceResume, TraceState, WindowCheckpoint,
};

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
            // Only parks a `PendingPrevention`/opens a window if some
            // installed/rigged card actually has a matching `Paid`
            // `PreventDamage` ability — a zero-overhead no-op for every
            // registry with no such card (the entire baseline set today),
            // so this stays a synchronous `apply_damage` call exactly as
            // before in the common case.
            if has_matching_paid_ability(state, registry, |e| matches!(e, Effect::PreventDamage(_))) {
                park_damage_prevention(state, registry, *damage_type, *amount, acting_card)
            } else {
                Ok(damage::apply_damage(state, *damage_type, *amount))
            }
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
            let run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
            let server = run.server;
            // Snapshotted before `active_run` is cleared — see
            // `Trigger::OnRunEnded`'s doc comment.
            state.last_completed_run = Some(CompletedRun::snapshot(run));
            state.active_run = None;
            let ended_event = GameEvent::RunEndedByEffect { server };
            let mut events = vec![ended_event.clone()];
            events.extend(dispatcher::dispatch_event(state, registry, &ended_event)?);
            Ok(events)
        }

        Effect::GiveTags(amount) => {
            // Always targets the Runner — see GiveTags's own doc comment.
            state.runner.tags = state.runner.tags.saturating_add(*amount);
            let tags_given_event = GameEvent::TagsGiven { side: Side::Runner, amount: *amount };
            // Dispatched recursively, same precedent as `InitiateRun`'s own
            // arm — e.g. NBN: Reality Plus's `Trigger::OnTagsGiven` needs to
            // fire no matter which card/effect actually gave the tag.
            let mut events = vec![tags_given_event.clone()];
            events.extend(dispatcher::dispatch_event(state, registry, &tags_given_event)?);
            Ok(events)
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

        Effect::TrashCard(target) => {
            // Same zero-overhead-unless-a-card-cares gating as `DealDamage`.
            if has_matching_paid_ability(state, registry, |e| matches!(e, Effect::PreventTrash)) {
                park_trash_prevention(state, registry, target.clone(), acting_card)
            } else {
                trash_card(state, target, acting_card)
            }
        }

        Effect::PreventDamage(amount) => {
            let pending = state.pending_prevention.as_mut().ok_or(RulesError::NoPendingPrevention)?;
            match &mut pending.kind {
                PendingPreventionKind::Damage { prevented, .. } => {
                    *prevented = prevented.saturating_add(*amount);
                    Ok(Vec::new())
                }
                PendingPreventionKind::Trash { .. } => Err(RulesError::PreventionKindMismatch {
                    expected: PreventionKind::Damage,
                    actual: PreventionKind::Trash,
                }),
            }
        }

        Effect::PreventTrash => {
            let pending = state.pending_prevention.as_mut().ok_or(RulesError::NoPendingPrevention)?;
            match &mut pending.kind {
                PendingPreventionKind::Trash { prevented, .. } => {
                    *prevented = true;
                    Ok(Vec::new())
                }
                PendingPreventionKind::Damage { .. } => Err(RulesError::PreventionKindMismatch {
                    expected: PreventionKind::Trash,
                    actual: PreventionKind::Damage,
                }),
            }
        }

        Effect::AddCounters(amount) => modify_counters(state, acting_card, i64::from(*amount)),

        Effect::RemoveCounters(amount) => modify_counters(state, acting_card, -i64::from(*amount)),

        Effect::TakeAllCountersAsCredits(side) => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let current = counters_of(state, card_id).ok_or_else(|| RulesError::CardNotEligibleForCounters(card_id.clone()))?;
            let mut events = modify_counters(state, acting_card, -i64::from(current))?;
            state.resources_mut(*side).credits = state.resources(*side).credits.gain(current);
            events.push(GameEvent::CreditsGained { side: *side, amount: current });
            Ok(events)
        }

        Effect::PermitJackOut => {
            let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
            run.jack_out_permitted = true;
            Ok(vec![GameEvent::JackOutPermitted { server: run.server }])
        }

        Effect::GainMaxHandSize(side, amount) => {
            match side {
                Side::Corp => state.corp.max_hand_size_bonus = state.corp.max_hand_size_bonus.saturating_add(*amount),
                Side::Runner => {
                    state.runner.max_hand_size_bonus = state.runner.max_hand_size_bonus.saturating_add(*amount)
                }
            }
            Ok(vec![GameEvent::MaxHandSizeGained { side: *side, amount: *amount }])
        }

        Effect::TrashCurrentlyAccessedCard => run::trash_currently_accessed_card_without_cost(state, registry),

        Effect::DerezCard(target) => {
            let (card_id, _server) = resolve_corp_installed_target(state, target, acting_card)?;
            let installed = state
                .corp
                .installed
                .iter_mut()
                .find(|c| c.card == card_id)
                .ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            installed.rezzed = false;
            Ok(vec![GameEvent::CardDerezzed { card: card_id }])
        }

        Effect::GainCreditsPerCounter { side, credits_per_counter } => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let current = counters_of(state, card_id).unwrap_or(0);
            let amount = current.saturating_mul(*credits_per_counter);
            state.resources_mut(*side).credits = state.resources(*side).credits.gain(amount);
            Ok(vec![GameEvent::CreditsGained { side: *side, amount }])
        }

        Effect::SwapInstalledIce(a, b) => {
            for id in [a, b] {
                if !state.corp.installed.iter().any(|c| &c.card == id) {
                    return Err(RulesError::CardNotInstalled { card: id.clone() });
                }
                if state.active_run.as_ref().is_some_and(|run| run.ice.iter().any(|i| &i.card_id == id)) {
                    return Err(RulesError::CannotSwapIceDuringActiveRun(id.clone()));
                }
            }
            let pos_a = state.corp.installed.iter().position(|c| &c.card == a).expect("checked above");
            let pos_b = state.corp.installed.iter().position(|c| &c.card == b).expect("checked above");
            let (server_a, slot_a) = (state.corp.installed[pos_a].server, state.corp.installed[pos_a].slot);
            let (server_b, slot_b) = (state.corp.installed[pos_b].server, state.corp.installed[pos_b].slot);
            state.corp.installed[pos_a].server = server_b;
            state.corp.installed[pos_a].slot = slot_b;
            state.corp.installed[pos_b].server = server_a;
            state.corp.installed[pos_b].slot = slot_a;
            Ok(vec![GameEvent::IceSwapped { a: a.clone(), b: b.clone() }])
        }

        Effect::InstallFromZoneIgnoringCost { card_id, origin_zone, into, slot, insert_after } => {
            // Archives is a `Vec<ArchivedCard>` while HQ is a plain
            // `Vec<CardId>`, so the removal is done per-zone rather than
            // through one shared `&mut Vec<CardId>` handle. Orientation is
            // irrelevant here — the card is leaving Archives entirely.
            let removed = match origin_zone {
                crate::dsl::CardZoneRef::OwnHq => {
                    state.corp.hq.iter().position(|c| c == card_id).map(|pos| {
                        state.corp.hq.remove(pos);
                    })
                }
                crate::dsl::CardZoneRef::OwnArchives => {
                    state.corp.archives.iter().position(|c| &c.card == card_id).map(|pos| {
                        state.corp.archives.remove(pos);
                    })
                }
                _ => return Err(RulesError::UnresolvedCardTarget),
            };
            removed.ok_or_else(|| RulesError::CardNotInHand { side: Side::Corp, card: card_id.clone() })?;

            let card_def =
                registry.get(card_id).ok_or_else(|| RulesError::CardNotFoundInRegistry(card_id.clone()))?;
            let resolved_slot = slot.unwrap_or(match card_def.card_type {
                crate::dsl::CardType::Ice(_) => crate::rules::InstallSlot::Ice,
                _ => crate::rules::InstallSlot::Root,
            });
            let new_card = crate::rules::InstalledCard {
                card: card_id.clone(),
                server: *into,
                slot: resolved_slot,
                rezzed: false,
                advancement_tokens: 0,
                counters: 0,
                installed_this_turn: true,
            };
            match insert_after {
                Some(host) => {
                    let host_pos = state.corp.installed.iter().position(|c| &c.card == host);
                    match host_pos {
                        Some(i) => state.corp.installed.insert(i + 1, new_card),
                        None => state.corp.installed.push(new_card),
                    }
                }
                None => state.corp.installed.push(new_card),
            }
            Ok(vec![GameEvent::CardInstalled { side: Side::Corp, card: card_id.clone(), server: *into }])
        }

        Effect::PreventStealAndTrashForRemainderOfRun => {
            let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
            run.runner_cannot_steal_or_trash = true;
            Ok(Vec::new())
        }

        Effect::PreventScoringForRemainderOfTurn => {
            state.corp.cannot_score_agendas_this_turn = true;
            Ok(Vec::new())
        }

        Effect::AddAdvancementTokens(amount) => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let installed = state
                .corp
                .installed
                .iter_mut()
                .find(|c| &c.card == card_id)
                .ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            installed.advancement_tokens = installed.advancement_tokens.saturating_add(*amount);
            let advancement_tokens = installed.advancement_tokens;
            Ok(vec![GameEvent::CardAdvanced { card: card_id.clone(), advancement_tokens }])
        }

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

            let breaker = state
                .runner
                .rig
                .iter()
                .find(|c| &c.card == acting)
                .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: acting.clone() })?;
            let breaker_strength = computed_runner_strength(breaker, state, registry);

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

        Effect::BreakSubroutinesUnconditionally { count } => {
            let run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
            if run.phase != RunPhase::EncounterIce {
                return Err(RulesError::NotInEncounter);
            }
            let ice = &run.ice[run.position];
            let pending: Vec<usize> =
                ice.subroutines.iter().filter(|s| s.status == SubroutineStatus::Pending).map(|s| s.id).collect();
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
                // Stop if that effect parked something spanning future
                // `PlayerAction`s — see `Effect::Sequence`'s doc comment.
                // Mirrors `resolve_unbroken_subroutines`'s exact "a parked
                // decision interrupts further resolution" check.
                if state.active_trace.is_some()
                    || state.pending_prevention.is_some()
                    || state.pending_paid_choice.is_some()
                    || state.pending_decision.is_some()
                {
                    break;
                }
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

        Effect::GainClicks(side, amount) => {
            let resources = match side {
                Side::Corp => &mut state.corp.resources,
                Side::Runner => &mut state.runner.resources,
            };
            resources.clicks = Clicks(resources.clicks.0.saturating_add(*amount));
            Ok(vec![GameEvent::ClicksGained { side: *side, amount: *amount }])
        }

        Effect::InitiateRun(server) => {
            run::start_run(state, registry, *server)?;
            let run_initiated_event = GameEvent::RunInitiated { server: *server };
            let mut events = vec![run_initiated_event.clone()];
            events.extend(crate::rules::dispatcher::dispatch_event(state, registry, &run_initiated_event)?);
            Ok(events)
        }

        Effect::EffectIf { condition, effect } => {
            let side = acting_side(acting_card, registry);
            if check_requirement(state, condition, side, acting_card, registry).is_ok() {
                evaluate_effect(state, effect, acting_card, registry)
            } else {
                Ok(Vec::new())
            }
        }

        Effect::OfferPaidChoice { side, cost, if_paid, if_declined } => {
            state.pending_paid_choice = Some(PendingPaidChoice {
                side: *side,
                cost: cost.clone(),
                if_paid: (**if_paid).clone(),
                if_declined: (**if_declined).clone(),
                source_card: acting_card.cloned(),
                resume: PendingPaidChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingPaidChoiceOffered { side: *side }])
        }

        Effect::PresentChoice { chooser, options } => {
            state.pending_decision = Some(PendingDecision::ChooseEffect {
                chooser: *chooser,
                options: options.clone(),
                source_card: acting_card.cloned(),
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingChoicePresented { chooser: *chooser, option_count: options.len() }])
        }

        Effect::GainCreditsPerCardAccessedThisRun(side) => {
            let amount = state.last_completed_run.as_ref().map_or(0, |run| run.cards_accessed);
            state.resources_mut(*side).credits = state.resources(*side).credits.gain(amount);
            Ok(vec![GameEvent::CreditsGained { side: *side, amount }])
        }

        Effect::PromptChooseCards { side, source, filter, min, max, reveal, shuffle_after, destination, then } => {
            let available = crate::rules::pending_choice::eligible_cards(state, registry, *side, source, filter);
            if available.len() < *min as usize {
                // Nothing to do — same "silently no-op" leniency
                // `DrawCards`/`TrashCard`'s "already gone" case establish.
                // e.g. Hansei Review's "if there are any cards in HQ".
                return Ok(Vec::new());
            }
            state.pending_decision = Some(PendingDecision::ChooseCards {
                side: *side,
                source: source.clone(),
                filter: filter.clone(),
                min: *min,
                max: *max,
                reveal: *reveal,
                shuffle_after: *shuffle_after,
                destination: destination.clone(),
                then: then.clone(),
                selected: Vec::new(),
                source_card: acting_card.cloned(),
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingCardSelectionOffered { side: *side, min: *min, max: *max }])
        }

        Effect::PromptChooseServer { chooser, rez_cost_delta, bonus_run_credits, allowed_servers, on_success } => {
            // A parked `ChooseServer` is only ever resolved by
            // `run::start_run`, which rejects a second concurrent run — so
            // parking one while a run is active creates a decision *nothing*
            // can resolve, and (since a parked decision blocks every other
            // action) deadlocks the game outright.
            //
            // Checking the precondition here rather than deferring it to
            // resolution is what makes it visible to `legal_actions`'
            // dry-run probe, which then filters out the whole activating
            // ability instead of offering a click-sink. Same reason
            // `Effect::InitiateRun` is safe already: it calls `start_run`
            // inline, so its error surfaces to the probe.
            //
            // Found by `no_panics_or_deadlocks_across_many_seeds_system_gateway`:
            // Red Team's `[click]: Run a central server…` was being offered
            // mid-run.
            if state.active_run.is_some() {
                return Err(RulesError::RunAlreadyInProgress);
            }
            state.pending_decision = Some(PendingDecision::ChooseServer {
                chooser: *chooser,
                rez_cost_delta: *rez_cost_delta,
                bonus_run_credits: *bonus_run_credits,
                allowed_servers: allowed_servers.clone(),
                on_success: on_success.clone(),
                source_card: acting_card.cloned(),
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingServerChoiceOffered { chooser: *chooser }])
        }

        Effect::RezInstalledIgnoringCost(card_id) => {
            let server = {
                let installed = state
                    .corp
                    .installed
                    .iter_mut()
                    .find(|c| &c.card == card_id)
                    .ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
                if installed.rezzed {
                    return Err(RulesError::AlreadyRezzed { card: card_id.clone() });
                }
                installed.rezzed = true;
                installed.server
            };
            // Mirrors `engine::rez_ice`'s "sync the matching RunIce if
            // mid-ApproachIce" step — see this variant's doc comment for why
            // this duplicates rather than shares `rez_ice`'s lines.
            if let Some(run) = state.active_run.as_mut()
                && run.phase == RunPhase::ApproachIce
                && let Some(current_ice) = run.ice.get_mut(run.position)
                && current_ice.card_id == *card_id
            {
                current_ice.rezzed = true;
            }
            let rezzed_event = GameEvent::IceRezzed { card: card_id.clone(), server };
            let mut events = vec![rezzed_event.clone()];
            events.extend(dispatcher::dispatch_event(state, registry, &rezzed_event)?);
            Ok(events)
        }

        Effect::DealDamageAmount(damage_type, amount) => {
            let resolved = resolve_amount(amount, acting_card, state, registry);
            evaluate_effect(state, &Effect::DealDamage(*damage_type, resolved as usize), acting_card, registry)
        }

        Effect::AddAdditionalAccessAmount { server, amount } => {
            let resolved = resolve_amount(amount, acting_card, state, registry);
            evaluate_effect(state, &Effect::AddAdditionalAccess { server: *server, count: resolved }, acting_card, registry)
        }

        Effect::BoostStrengthAmount { amount, duration } => {
            let resolved = resolve_amount(amount, acting_card, state, registry);
            evaluate_effect(state, &Effect::BoostStrength { amount: resolved, duration: *duration }, acting_card, registry)
        }
    }
}

/// Which side's context an `EffectRequirement` check runs under, when the
/// caller only has `acting_card` (not an explicit `Side`) to go on —
/// `Effect::EffectIf`'s only source of "whose turn/state is this." Falls
/// back to `Side::Corp` for an unregistered/absent card, matching
/// `owning_side_of_target`'s own precedent for `CardTarget::ThisCard`.
fn acting_side(acting_card: Option<&CardId>, registry: &CardRegistry) -> Side {
    acting_card.and_then(|id| registry.get(id)).map(|c| c.side).unwrap_or(Side::Corp)
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

        // A subroutine we just fired parked a Trace or a PendingPrevention,
        // either of which spans future PlayerActions — stop here rather
        // than firing the next pending subroutine underneath it.
        // `rules::trace::submit_runner_bid`/`paid_ability::close_window`'s
        // `Prevention` arm call this function again once resolved, resuming
        // the loop.
        if state.active_trace.is_some()
            || state.pending_prevention.is_some()
            || state.pending_paid_choice.is_some()
            || state.pending_decision.is_some()
        {
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
        // Pass the ICE itself as `acting_card` — needed for a subroutine
        // effect that self-references its own installed position (e.g.
        // Ansel 1.0/Brân 1.0's "install ... directly inward from this
        // ice," resolved via `Effect::InstallFromZoneIgnoringCost`'s
        // `PendingDecision::ChooseCards::source_card` lookup). No existing
        // subroutine effect relied on `acting_card` being absent here.
        let fired_events = evaluate_effect(state, &effect, Some(&card_id), registry)?;
        events.push(GameEvent::SubroutineFired { card_id, index, effect });
        events.extend(fired_events);

        // If that subroutine's effect was a Trace or parked a
        // PendingPrevention, mark it so the eventual resolution knows to
        // resume this loop afterward.
        if let Some(trace) = state.active_trace.as_mut() {
            trace.resume = TraceResume::ResumeSubroutines;
        }
        if let Some(pending) = state.pending_prevention.as_mut() {
            pending.resume = PreventionResume::ResumeSubroutines;
        }
        if let Some(pending) = state.pending_paid_choice.as_mut() {
            pending.resume = PendingPaidChoiceResume::ResumeSubroutines;
        }
        // Covers all three `PendingDecision` variants (`ChooseEffect` was
        // the only one that existed when this call was first written;
        // `ChooseCards`/`ChooseServer` need the same marking — e.g. Ansel
        // 1.0's first subroutine parks a `ChooseCards`, which must resume
        // this loop once resolved so its later subroutines still fire).
        crate::rules::pending_choice::mark_pending_decision_resume_subroutines(state);
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
    let card_side = card.side;
    for triggered in card.triggers.iter().filter(|t| t.trigger == trigger) {
        if let Some(requirement) = &triggered.requirement
            && check_requirement(state, requirement, card_side, Some(card_id), registry).is_err()
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
            consume_requirement(state, requirement, card_side);
        }
    }
    Ok(events)
}

/// Like `process_card_triggers`, but the reacting card's own effects act
/// on `target` instead of `owner_id` itself — the one case where "who
/// reacts" and "what the effect acts on" differ. e.g. Cookbook's
/// "whenever you install a virus program, you may place 1 virus counter
/// on it" — Cookbook (`owner_id`) is what reacts, but "it" (`target`) is
/// the just-installed virus program, not Cookbook. `requirement`
/// checks/consumption still use `owner_id`'s own side/context; only the
/// `evaluate_effect` target changes.
pub(crate) fn process_card_triggers_targeting(
    state: &mut GameState,
    registry: &CardRegistry,
    owner_id: &CardId,
    trigger: Trigger,
    target: &CardId,
) -> Result<Vec<GameEvent>, RulesError> {
    let Some(card) = registry.get(owner_id) else {
        return Ok(Vec::new());
    };
    let mut events = Vec::new();
    let card_side = card.side;
    for triggered in card.triggers.iter().filter(|t| t.trigger == trigger) {
        if let Some(requirement) = &triggered.requirement
            && check_requirement(state, requirement, card_side, Some(owner_id), registry).is_err()
        {
            continue;
        }
        for effect in &triggered.effects {
            events.extend(evaluate_effect(state, effect, Some(target), registry)?);
        }
        if let Some(requirement) = &triggered.requirement {
            consume_requirement(state, requirement, card_side);
        }
    }
    Ok(events)
}

/// Whether any rezzed Corp install or Runner rig card has a `Trigger::Paid`
/// ability whose effect matches `predicate` — the gate `DealDamage`/
/// `TrashCard` use to decide whether to park a `PendingPrevention` and open
/// a `WindowCheckpoint::Prevention` window at all, versus resolving
/// synchronously exactly as before. Mirrors `dispatcher.rs`'s `TurnStarted`
/// arm's candidate-collection shape (rezzed Corp installs ∪ full Runner
/// rig), generalized to both sides since either could in principle carry a
/// prevention ability.
fn has_matching_paid_ability(state: &GameState, registry: &CardRegistry, predicate: impl Fn(&Effect) -> bool) -> bool {
    let corp_ids = state.corp.installed.iter().filter(|c| c.rezzed).map(|c| &c.card);
    let runner_ids = state.runner.rig.iter().map(|c| &c.card);
    corp_ids.chain(runner_ids).any(|card_id| {
        registry
            .get(card_id)
            .is_some_and(|card| card.abilities.iter().any(|a| a.trigger == Trigger::Paid && predicate(&a.effect)))
    })
}

/// Parks a `PendingPreventionKind::Damage`, fires any automatic
/// `Trigger::OnDamageAboutToResolve` reaction, then opens a
/// `WindowCheckpoint::Prevention` window with the Runner holding priority
/// first — damage in this engine's model always targets the Runner (see
/// `Effect::DealDamage`'s own doc comment), so the Runner is the side with
/// something to prevent.
fn park_damage_prevention(
    state: &mut GameState,
    registry: &CardRegistry,
    damage_type: crate::dsl::DamageType,
    amount: usize,
    acting_card: Option<&CardId>,
) -> Result<Vec<GameEvent>, RulesError> {
    state.pending_prevention = Some(PendingPrevention {
        kind: PendingPreventionKind::Damage { damage_type, amount, prevented: 0 },
        source_card: acting_card.cloned(),
        resume: PreventionResume::None,
    });
    let about_to_resolve = GameEvent::DamageAboutToResolve { damage_type, amount };
    let mut events = vec![about_to_resolve.clone()];
    events.extend(dispatcher::dispatch_event(state, registry, &about_to_resolve)?);
    events.push(paid_ability::open_window_for(state, Side::Runner, WindowCheckpoint::Prevention));
    Ok(events)
}

/// Parks a `PendingPreventionKind::Trash`, fires any automatic
/// `Trigger::OnTrashAboutToResolve` reaction, then opens a
/// `WindowCheckpoint::Prevention` window with priority given to whichever
/// side owns the targeted card (see `owning_side_of_target`) — unlike
/// damage, a trash effect can target either side's card.
fn park_trash_prevention(
    state: &mut GameState,
    registry: &CardRegistry,
    target: CardTarget,
    acting_card: Option<&CardId>,
) -> Result<Vec<GameEvent>, RulesError> {
    let priority = owning_side_of_target(&target, acting_card, registry);
    state.pending_prevention = Some(PendingPrevention {
        kind: PendingPreventionKind::Trash { target: target.clone(), prevented: false },
        source_card: acting_card.cloned(),
        resume: PreventionResume::None,
    });
    let about_to_resolve = GameEvent::TrashAboutToResolve { target };
    let mut events = vec![about_to_resolve.clone()];
    events.extend(dispatcher::dispatch_event(state, registry, &about_to_resolve)?);
    events.push(paid_ability::open_window_for(state, priority, WindowCheckpoint::Prevention));
    Ok(events)
}

/// Which side owns the card a `CardTarget` names — `CorpInstalled`/
/// `RunnerRig`/`TopOfStack` all say so directly; `ThisCard` is resolved via
/// `acting_card`'s own registry-declared `side`, defaulting to `Runner` if
/// unresolvable (the overwhelmingly common case for a trash-prevention
/// trigger).
fn owning_side_of_target(target: &CardTarget, acting_card: Option<&CardId>, registry: &CardRegistry) -> Side {
    match target {
        CardTarget::CorpInstalled { .. } => Side::Corp,
        CardTarget::RunnerRig(_) => Side::Runner,
        CardTarget::TopOfStack { side, .. } => *side,
        CardTarget::ThisCard => {
            acting_card.and_then(|id| registry.get(id)).map(|c| c.side).unwrap_or(Side::Runner)
        }
        // A Trojan's host is always a Corp installed card.
        CardTarget::HostIce => Side::Corp,
    }
}

/// Resolves a `CardTarget` to a concrete Corp installed `(CardId,
/// ServerId)` — for `Effect::DerezCard`, which (unlike `TrashCard`) only
/// ever makes sense against a Corp install. `CorpInstalled`/`HostIce`
/// resolve directly (mirroring `trash_card`'s own `HostIce` resolution);
/// every other `CardTarget` variant errors `UnresolvedCardTarget`, since
/// none of them can ever name a Corp installed card.
fn resolve_corp_installed_target(
    state: &GameState,
    target: &CardTarget,
    acting_card: Option<&CardId>,
) -> Result<(CardId, ServerId), RulesError> {
    match target {
        CardTarget::CorpInstalled { card, server } => Ok((card.clone(), *server)),
        CardTarget::HostIce => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let host = state
                .runner
                .rig
                .iter()
                .find(|c| &c.card == card_id)
                .and_then(|c| c.hosted_on_ice.clone())
                .ok_or(RulesError::UnresolvedCardTarget)?;
            let server = state
                .corp
                .installed
                .iter()
                .find(|c| c.card == host)
                .map(|c| c.server)
                .ok_or_else(|| RulesError::CardNotInstalled { card: host.clone() })?;
            Ok((host, server))
        }
        CardTarget::ThisCard | CardTarget::RunnerRig(_) | CardTarget::TopOfStack { .. } => {
            Err(RulesError::UnresolvedCardTarget)
        }
    }
}

/// `Effect::AddCounters`/`RemoveCounters`'s shared implementation:
/// saturating-applies `delta` (negative to remove) to `acting_card`'s
/// `counters` field, wherever it's currently installed/rigged. Mirrors
/// `trash_this_card`'s "try Corp installed, then Runner rig" search order,
/// but doesn't need `trash_this_card`'s hand/deck arms — counters only ever
/// live on an installed/rigged card, never in a hand or deck zone.
fn modify_counters(
    state: &mut GameState,
    acting_card: Option<&CardId>,
    delta: i64,
) -> Result<Vec<GameEvent>, RulesError> {
    let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;

    let counters = if let Some(installed) = state.corp.installed.iter_mut().find(|c| &c.card == card_id) {
        &mut installed.counters
    } else if let Some(rig_card) = state.runner.rig.iter_mut().find(|c| &c.card == card_id) {
        &mut rig_card.counters
    } else {
        return Err(RulesError::CardNotEligibleForCounters(card_id.clone()));
    };
    *counters = (i64::from(*counters) + delta).max(0) as u32;

    let event = if delta >= 0 {
        GameEvent::CountersAdded { card: card_id.clone(), amount: delta as u32 }
    } else {
        GameEvent::CountersRemoved { card: card_id.clone(), amount: (-delta) as u32 }
    };
    Ok(vec![event])
}

pub(crate) fn trash_card(
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
            // A rezzed install was face-up on the table, so the Runner has
            // already seen it; an unrezzed one they never did.
            let seen = state.corp.installed[position].rezzed || runner_is_accessing(state, card);
            state.corp.installed.remove(position);
            state.corp.archives.push(orient(card.clone(), seen));
            let mut events = vec![GameEvent::CardTrashed { side: Side::Corp, card: card.clone() }];
            events.extend(cascade_trash_hosted_programs(state, card));
            Ok(events)
        }

        // "The ICE I'm hosted on" — resolve `acting_card`'s host, then
        // trash it exactly like `CorpInstalled` (cascade included, since
        // it recurses into that same arm).
        CardTarget::HostIce => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let host = state
                .runner
                .rig
                .iter()
                .find(|c| &c.card == card_id)
                .and_then(|c| c.hosted_on_ice.clone())
                .ok_or(RulesError::UnresolvedCardTarget)?;
            let server = state
                .corp
                .installed
                .iter()
                .find(|c| c.card == host)
                .map(|c| c.server)
                .ok_or_else(|| RulesError::CardNotInstalled { card: host.clone() })?;
            trash_card(state, &CardTarget::CorpInstalled { card: host, server }, acting_card)
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
            // Split by side rather than sharing one `(deck, pile)` pair:
            // Archives carries a facedown flag (`ArchivedCard`) while the
            // Heap is a plain `Vec<CardId>`, so the two piles no longer have
            // the same type. A card milled off R&D was never seen by the
            // Runner, hence facedown.
            let popped = match (side, zone) {
                (Side::Corp, StackZone::RAndD) => state.corp.r_and_d.pop().inspect(|card| {
                    state.corp.archives.push(ArchivedCard::facedown(card.clone()));
                }),
                (Side::Runner, StackZone::Stack) => state.runner.stack.pop().inspect(|card| {
                    state.runner.heap.push(card.clone());
                }),
                // Corp has no Stack, Runner has no R&D — no card ever
                // occupies this mismatched combination's "top".
                _ => return Err(RulesError::EmptyZone { side: *side, zone: *zone }),
            };
            match popped {
                Some(card) => {
                    Ok(vec![GameEvent::CardTrashed { side: *side, card }])
                }
                None => Err(RulesError::EmptyZone { side: *side, zone: *zone }),
            }
        }
    }
}

/// Whenever a Corp installed card (`host_card_id`) leaves play, any
/// Trojan Program hosted on it (`InstalledRunnerCard::hosted_on_ice ==
/// Some(host_card_id)`) is trashed too — e.g. trashing the ICE Botulus is
/// hosted on takes Botulus with it. Called from every site that removes a
/// card from `CorpState::installed`. A no-op (returns no events) if
/// nothing is hosted on `host_card_id`, so this is zero-overhead for the
/// overwhelming majority of Corp cards that never host anything.
fn cascade_trash_hosted_programs(state: &mut GameState, host_card_id: &CardId) -> Vec<GameEvent> {
    let hosted: Vec<CardId> = state
        .runner
        .rig
        .iter()
        .filter(|c| c.hosted_on_ice.as_ref() == Some(host_card_id))
        .map(|c| c.card.clone())
        .collect();
    let mut events = Vec::new();
    for card in hosted {
        if let Some(position) = state.runner.rig.iter().position(|c| c.card == card) {
            let removed = state.runner.rig.remove(position);
            state.runner.heap.push(removed.card.clone());
            events.push(GameEvent::CardTrashed { side: Side::Runner, card: removed.card });
        }
    }
    events
}

/// Locates `card_id` wherever it currently sits — Corp installed, HQ, R&D,
/// or Runner Rig/Grip — and moves it to that side's discard pile, for
/// `CardTarget::ThisCard`/`Cost::TrashSelf` self-reference resolution
/// (unlike `CardTarget::CorpInstalled`/`RunnerRig`, the zone isn't known
/// ahead of time). Not found in any of those zones (e.g. already trashed by
/// an earlier effect in the same resolution, or accessed straight from
/// Archives) is a no-op, mirroring `run::access::move_to_archives`'s
/// existing "already there" leniency, rather than erroring.
/// `ArchivedCard::faceup`/`facedown` selected by whether the Runner has
/// seen the card — the one place that decision is spelled out, so every
/// trash path reads the same way.
fn orient(card: CardId, seen_by_runner: bool) -> ArchivedCard {
    if seen_by_runner { ArchivedCard::faceup(card) } else { ArchivedCard::facedown(card) }
}

/// Whether the Runner is currently accessing `card_id` (it's the pending
/// choice, or already resolved earlier in this same access). Such a card has
/// been seen even if it was never rezzed — e.g. an unrezzed ambush that
/// trashes itself on access — so it lands faceup in Archives rather than
/// following the plain rezzed-or-not rule.
fn runner_is_accessing(state: &GameState, card_id: &CardId) -> bool {
    state
        .active_run
        .as_ref()
        .and_then(|run| run.access_state.as_ref())
        .is_some_and(|access| {
            access.currently_accessing.as_ref() == Some(card_id)
                || access.resolved_cards.contains(card_id)
                || matches!(&access.phase, AccessPhase::PendingChoice { card_id: pending, .. } if pending == card_id)
        })
}

fn trash_this_card(state: &mut GameState, card_id: &CardId) -> Result<Vec<GameEvent>, RulesError> {
    if let Some(position) = state.corp.installed.iter().position(|installed| &installed.card == card_id) {
        // Same rezzed-or-not rule as `CardTarget::CorpInstalled` above,
        // widened for the access case: a card the Runner is accessing right
        // now has been seen regardless of rez state.
        let seen = state.corp.installed[position].rezzed || runner_is_accessing(state, card_id);
        state.corp.installed.remove(position);
        state.corp.archives.push(orient(card_id.clone(), seen));
        let mut events = vec![GameEvent::CardTrashed { side: Side::Corp, card: card_id.clone() }];
        events.extend(cascade_trash_hosted_programs(state, card_id));
        return Ok(events);
    }
    if let Some(position) = state.corp.hq.iter().position(|c| c == card_id) {
        // Trashed straight out of the Corp's hand — facedown, unless the
        // Runner is accessing it right now (an HQ ambush that trashes
        // itself on access, say), in which case they have seen it.
        let seen = runner_is_accessing(state, card_id);
        state.corp.hq.remove(position);
        state.corp.archives.push(orient(card_id.clone(), seen));
        return Ok(vec![GameEvent::CardTrashed { side: Side::Corp, card: card_id.clone() }]);
    }
    if let Some(position) = state.corp.r_and_d.iter().position(|c| c == card_id) {
        // Milled off R&D — facedown on the same reasoning as HQ above.
        let seen = runner_is_accessing(state, card_id);
        state.corp.r_and_d.remove(position);
        state.corp.archives.push(orient(card_id.clone(), seen));
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
            // Symmetric pool for a run given its own temporary credits by
            // whatever initiated it (e.g. Overclock's "you can spend hosted
            // credits during that run") — drawn from before the Runner's
            // wallet, same precedent as `bad_publicity_credits`, and
            // additive with it (both are legitimately spendable during the
            // same run).
            let bonus_run_available = match (side, state.active_run.as_ref()) {
                (Side::Runner, Some(run)) => run.bonus_run_credits,
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
            let total_available = bp_available
                .saturating_add(bonus_run_available)
                .saturating_add(recurring_available)
                .saturating_add(wallet_available);
            if total_available < *amount {
                return Err(RulesError::NotEnoughCredits {
                    side,
                    available: total_available,
                    requested: *amount,
                });
            }

            let from_bp = bp_available.min(*amount);
            let from_bonus_run = bonus_run_available.min(*amount - from_bp);
            let from_recurring = recurring_available.min(*amount - from_bp - from_bonus_run);
            let from_wallet = amount - from_bp - from_bonus_run - from_recurring;

            let mut events = Vec::new();
            if from_bp > 0 {
                state
                    .active_run
                    .as_mut()
                    .expect("bp_available > 0 implies an active run")
                    .bad_publicity_credits -= from_bp;
                events.push(GameEvent::BadPublicityCreditsSpent { amount: from_bp });
            }
            if from_bonus_run > 0 {
                state
                    .active_run
                    .as_mut()
                    .expect("bonus_run_available > 0 implies an active run")
                    .bonus_run_credits -= from_bonus_run;
                events.push(GameEvent::BonusRunCreditsSpent { amount: from_bonus_run });
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

        Cost::RemoveSelfFromGame => {
            let card_id = acting_card.ok_or(RulesError::MissingActingCardContext)?;
            let position = state
                .corp
                .installed
                .iter()
                .position(|c| &c.card == card_id)
                .ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            state.corp.installed.remove(position);
            // Deliberately not Archives — see `Cost::RemoveSelfFromGame`.
            state.corp.removed_from_game.push(card_id.clone());
            Ok(vec![GameEvent::CardRemovedFromGame { side, card: card_id.clone() }])
        }

        Cost::PurgeTags => {
            state.runner.tags = 0;
            Ok(vec![GameEvent::TagsPurged { side }])
        }

        Cost::TakeTags(amount) => {
            state.runner.tags = state.runner.tags.saturating_add(*amount);
            Ok(vec![GameEvent::TagsGiven { side: Side::Runner, amount: *amount }])
        }

        // `AnyOf`'s choice is resolved by the caller before `pay_cost` is
        // ever invoked (see `pending_choice::resolve_accept_pending_paid_choice`,
        // the only production caller that can encounter one) — reaching
        // here means something handed `pay_cost` a raw, unresolved `AnyOf`.
        Cost::AnyOf(_) => Err(RulesError::CostRequiresChoice),

        Cost::RemoveCounters(amount) => {
            let card_id = acting_card.ok_or(RulesError::MissingActingCardContext)?;
            let available = counters_of(state, card_id).unwrap_or(0);
            if available < *amount {
                return Err(RulesError::InsufficientCounters { card: card_id.clone(), required: *amount, available });
            }
            modify_counters(state, acting_card, -i64::from(*amount))
        }
    }
}

/// Checks an `AbilityDef::requirement` gate before its cost/effect resolve —
/// same "checked before resolution" role as `pay_cost`, but for a
/// precondition rather than a payment. Called from `engine::activate_ability`.
///
/// `side` is whose turn/ability this check is running for — only
/// `OncePerTurn` reads it (to pick which side's `once_per_turn_used` set to
/// consult); every other variant ignores it. Callers pass the side that owns
/// the card/ability/play in question (`AbilityDef`'s activating side,
/// `CardDefinition::play_requirement`'s playing side, or — from
/// `process_card_triggers` — the triggered card's own registry `side`).
pub fn check_requirement(
    state: &GameState,
    requirement: &EffectRequirement,
    side: Side,
    acting_card: Option<&CardId>,
    registry: &CardRegistry,
) -> Result<(), RulesError> {
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
        EffectRequirement::OncePerTurn(tag) => {
            let used = match side {
                Side::Corp => &state.corp.once_per_turn_used,
                Side::Runner => &state.runner.once_per_turn_used,
            };
            if used.contains(tag) {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::RunnerCreditsAtMost(amount) => {
            if state.runner.resources.credits.0 > *amount {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::RunnerClicksAtLeast(amount) => {
            if state.runner.resources.clicks.0 < *amount {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::Not(inner) => match check_requirement(state, inner, side, acting_card, registry) {
            Ok(()) => Err(RulesError::RequirementNotMet),
            Err(_) => Ok(()),
        },
        EffectRequirement::And(a, b) => {
            check_requirement(state, a, side, acting_card, registry)?;
            check_requirement(state, b, side, acting_card, registry)
        }
        EffectRequirement::RezzedDuringRunAgainstThisServer => {
            let card_id = acting_card.ok_or(RulesError::RequirementNotMet)?;
            let own_server = state
                .corp
                .installed
                .iter()
                .find(|c| &c.card == card_id)
                .map(|c| c.server)
                .ok_or(RulesError::RequirementNotMet)?;
            let matches = state.active_run.as_ref().is_some_and(|run| {
                run.server == own_server && matches!(run.phase, RunPhase::ApproachIce | RunPhase::EncounterIce)
            });
            if matches { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::RunnerMadeSuccessfulRunLastTurn => {
            if !state.runner.made_successful_run_last_turn {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::LastDamageTrashedOddCostCard => {
            let trashed_odd_cost = state
                .last_discarded_cards
                .iter()
                .any(|card_id| registry.get(card_id).is_some_and(|card| card.cost % 2 == 1));
            if trashed_odd_cost { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::LastRunWasOnHqOrRnD => match state.last_completed_run.as_ref().map(|run| run.server) {
            Some(ServerId::Hq | ServerId::RnD) => Ok(()),
            _ => Err(RulesError::RequirementNotMet),
        },
        EffectRequirement::ArchivesHasFacedownCard => {
            if state.corp.has_facedown_in_archives() { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::StoleAgendaDuringLastRun => {
            let stole = state.last_completed_run.as_ref().is_some_and(|run| run.agendas_stolen > 0);
            if stole { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::MadeSuccessfulRunThisTurn => {
            if !state.runner.made_successful_run_this_turn {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::ThisCardCountersAtMost(amount) => {
            let current = acting_card.and_then(|id| counters_of(state, id)).unwrap_or(0);
            if current > *amount {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::CurrentlyAccessingACard => {
            let accessing = state
                .active_run
                .as_ref()
                .and_then(|run| run.access_state.as_ref())
                .is_some_and(|access| matches!(access.phase, run::AccessPhase::PendingChoice { .. }));
            if accessing { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::ThisCardCountersAtLeast(amount) => {
            let current = acting_card.and_then(|id| counters_of(state, id)).unwrap_or(0);
            if current < *amount {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::EncounteringHostIce => {
            let host = acting_card
                .and_then(|id| state.runner.rig.iter().find(|c| &c.card == id))
                .and_then(|c| c.hosted_on_ice.as_ref());
            let Some(host) = host else { return Err(RulesError::RequirementNotMet) };
            let matches = state.active_run.as_ref().is_some_and(|run| {
                run.phase == RunPhase::EncounterIce && run.ice.get(run.position).is_some_and(|ice| &ice.card_id == host)
            });
            if matches { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::WasFirstAdvancementThisCard => {
            if state.last_advancement_was_first { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
    }
}

/// `acting_card`'s current generic counter total, wherever it's currently
/// installed/rigged — `None` if it's neither (already trashed, or never
/// resolvable), mirroring `modify_counters`'s "try Corp installed, then
/// Runner rig" search order but read-only.
fn counters_of(state: &GameState, card_id: &CardId) -> Option<u32> {
    if let Some(installed) = state.corp.installed.iter().find(|c| &c.card == card_id) {
        Some(installed.counters)
    } else {
        state.runner.rig.iter().find(|c| &c.card == card_id).map(|c| c.counters)
    }
}

/// `acting_card`'s current advancement token total, if it's a Corp
/// installed card — `None` otherwise (a Runner rig card, or already
/// trashed). Read-only counterpart to `advance_card`'s mutation.
fn advancement_tokens_of(state: &GameState, card_id: &CardId) -> Option<u32> {
    state.corp.installed.iter().find(|c| &c.card == card_id).map(|c| c.advancement_tokens)
}

/// Number of Runner rig cards matching `dsl::zone::CardFilter::Icebreaker`'s
/// heuristic — used by `Amount::InstalledIcebreakerCount`.
fn installed_icebreaker_count(state: &GameState, registry: &CardRegistry) -> u32 {
    state
        .runner
        .rig
        .iter()
        .filter(|c| registry.get(&c.card).is_some_and(|def| card_matches_filter(def, &CardFilter::Icebreaker)))
        .count() as u32
}

/// Resolves a `dsl::effect::Amount` to a concrete `u32` against the current
/// `state`/`acting_card` context — the shared computation every
/// `Amount`-typed `Effect` field (`DealDamageAmount`, `AddAdditionalAccessAmount`,
/// `BoostStrengthAmount`) delegates to before falling through to its
/// fixed-amount sibling's existing mutation logic. Every non-`Fixed`
/// variant that can't be resolved (no `acting_card`, card not currently
/// installed/rigged) resolves to `0` rather than erroring — the same
/// "nothing to do" leniency `Effect::DrawCards`/`TakeAllCountersAsCredits`
/// already establish for an empty/absent source.
/// Live-computed strength for a Runner rig card, layering its
/// `CardDefinition::strength_modifier` (if any) on top of its stored
/// `effective_strength()` (base + encounter/turn buffs) — unlike Corp ICE's
/// `StrengthModifier`, which is baked into `RunIce::current_strength` once
/// per run (`run::engine::build_run_ice`), a Runner breaker's
/// `PerInstalledIcebreaker` bonus can change at any moment (installing or
/// trashing another icebreaker), so it must be recomputed live at every
/// read site that needs an up-to-date value — currently `Effect::
/// BreakSubroutines`'s strength contest and `masking::PublicInstalledRunnerCard`'s
/// displayed strength. Every other Runner-side strength read (encounter/turn
/// buff bookkeeping itself) stays on the plain `effective_strength()` it
/// already used, since those don't need a live conditional bonus applied.
pub(crate) fn computed_runner_strength(card: &InstalledRunnerCard, state: &GameState, registry: &CardRegistry) -> i32 {
    let bonus = registry
        .get(&card.card)
        .and_then(|def| def.strength_modifier)
        .map(|modifier| match modifier {
            StrengthModifier::PerInstalledIcebreaker(per) => per * installed_icebreaker_count(state, registry) as i32,
            StrengthModifier::WhileProtectingRemote(_) | StrengthModifier::WhileHostedAdvancementsAtLeast { .. } => 0,
        })
        .unwrap_or(0);
    card.effective_strength() + bonus
}

fn resolve_amount(amount: &Amount, acting_card: Option<&CardId>, state: &GameState, registry: &CardRegistry) -> u32 {
    match amount {
        Amount::Fixed(n) => *n,
        Amount::AgendaPointsScoredThisTurn => state.corp.agenda_points_scored_this_turn,
        Amount::HostedCounters => acting_card.and_then(|c| counters_of(state, c)).unwrap_or(0),
        Amount::HostedAdvancementTokens => acting_card.and_then(|c| advancement_tokens_of(state, c)).unwrap_or(0),
        Amount::InstalledIcebreakerCount => installed_icebreaker_count(state, registry),
    }
}

/// Flips the per-turn tracking flag `requirement` gates, once a
/// `TriggeredEffect` it gated has actually fired — see `dsl::card::
/// TriggeredEffect::requirement`'s doc comment. A no-op for `IsTagged`
/// (nothing to consume; tag count isn't a once-per-turn resource). Kept
/// separate from `check_requirement` (which stays read-only) so
/// `activate_ability`'s existing `AbilityDef::requirement` call site is
/// unaffected — only `process_card_triggers`'s soft-gate path calls this.
pub(crate) fn consume_requirement(state: &mut GameState, requirement: &EffectRequirement, side: Side) {
    match requirement {
        EffectRequirement::IsTagged => {}
        EffectRequirement::FirstInstallThisTurn => state.corp.first_install_used_this_turn = true,
        EffectRequirement::FirstSuccessfulHqRunThisTurn => state.runner.first_hq_run_used_this_turn = true,
        EffectRequirement::OncePerTurn(tag) => {
            let used = match side {
                Side::Corp => &mut state.corp.once_per_turn_used,
                Side::Runner => &mut state.runner.once_per_turn_used,
            };
            used.insert(tag.clone());
        }
        EffectRequirement::And(a, b) => {
            consume_requirement(state, a, side);
            consume_requirement(state, b, side);
        }
        EffectRequirement::RunnerCreditsAtMost(_)
        | EffectRequirement::RunnerClicksAtLeast(_)
        | EffectRequirement::Not(_)
        | EffectRequirement::RezzedDuringRunAgainstThisServer
        | EffectRequirement::RunnerMadeSuccessfulRunLastTurn
        | EffectRequirement::LastDamageTrashedOddCostCard
        | EffectRequirement::LastRunWasOnHqOrRnD
        | EffectRequirement::StoleAgendaDuringLastRun
        | EffectRequirement::ArchivesHasFacedownCard
        | EffectRequirement::MadeSuccessfulRunThisTurn
        | EffectRequirement::ThisCardCountersAtMost(_)
        | EffectRequirement::CurrentlyAccessingACard
        | EffectRequirement::ThisCardCountersAtLeast(_)
        | EffectRequirement::EncounteringHostIce
        | EffectRequirement::WasFirstAdvancementThisCard => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{AbilityDef, CardDefinition, CardId, CardType, DamageType, IceType, SubroutineDef, TriggeredEffect};
    use crate::rules::run::{EncounteredSubroutine, RunIce, RunPhase as RP, RunState, ServerId, SubroutineStatus};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, GamePhase, InstalledCard, InstalledRunnerCard,
        MemoryUnits, PlayerResources, RunnerState,
    };

    fn installed_runner_card(id: &str, base_strength: i32) -> InstalledRunnerCard {
        InstalledRunnerCard {
            card: CardId(id.to_string()),
            base_strength,
            ..Default::default()
        }
    }

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
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            pending_prevention: None, pending_paid_choice: None, pending_decision: None, last_discarded_cards: Vec::new(), last_completed_run: None, last_advancement_was_first: false,
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

    /// A minimal card carrying one `Trigger::Paid` ability with the given
    /// `effect` — for exercising `has_matching_paid_ability`'s scan.
    fn card_with_paid_ability(id: &str, side: Side, effect: Effect) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type: CardType::Program,
            abilities: vec![AbilityDef { trigger: Trigger::Paid, cost: None, requirement: None, effect, cost_discount_if: None }],
            is_playable: true,
            ..Default::default()
        }
    }

    #[test]
    fn deal_damage_with_no_prevention_ability_in_play_resolves_immediately_unchanged() {
        let mut state = game_state();
        state.runner.grip = vec![CardId("card_0".to_string())];
        // A registered card is in play, but its Paid ability isn't a
        // prevention one — the scan must still see this as "no prevention
        // available" and resolve synchronously.
        let registry = CardRegistry::from_cards(vec![card_with_paid_ability(
            "corroder",
            Side::Runner,
            Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter },
        )]);
        state.runner.rig = vec![InstalledRunnerCard {
            card: CardId("corroder".to_string()),
            base_strength: 2,
            ..Default::default()
        }];

        let events = evaluate_effect(&mut state, &Effect::DealDamage(DamageType::Net, 1), None, &registry).unwrap();

        assert!(state.runner.grip.is_empty());
        assert!(state.pending_prevention.is_none());
        assert!(matches!(events[0], GameEvent::DamageTaken { .. }));
    }

    #[test]
    fn deal_damage_with_a_prevention_ability_in_play_parks_a_pending_prevention_and_opens_a_window() {
        let mut state = game_state();
        state.runner.grip = vec![CardId("card_0".to_string())];
        let registry = CardRegistry::from_cards(vec![card_with_paid_ability(
            "feedback_filter",
            Side::Runner,
            Effect::PreventDamage(1),
        )]);
        state.runner.rig = vec![InstalledRunnerCard {
            card: CardId("feedback_filter".to_string()),
            ..Default::default()
        }];

        evaluate_effect(&mut state, &Effect::DealDamage(DamageType::Net, 2), None, &registry).unwrap();

        // Nothing applied yet — the grip is untouched until the window closes.
        assert_eq!(state.runner.grip.len(), 1);
        assert_eq!(
            state.pending_prevention.as_ref().map(|p| &p.kind),
            Some(&PendingPreventionKind::Damage { damage_type: DamageType::Net, amount: 2, prevented: 0 })
        );
        let window = state.paid_ability_window.expect("a Prevention window should be open");
        assert_eq!(window.checkpoint, WindowCheckpoint::Prevention);
        assert_eq!(window.active_priority, Side::Runner);
    }

    #[test]
    fn prevent_damage_reduces_the_parked_amount() {
        let mut state = game_state();
        state.pending_prevention = Some(PendingPrevention {
            kind: PendingPreventionKind::Damage { damage_type: DamageType::Net, amount: 3, prevented: 0 },
            source_card: None,
            resume: PreventionResume::None,
        });

        evaluate_effect(&mut state, &Effect::PreventDamage(1), None, &CardRegistry::new()).unwrap();

        assert_eq!(
            state.pending_prevention.map(|p| p.kind),
            Some(PendingPreventionKind::Damage { damage_type: DamageType::Net, amount: 3, prevented: 1 })
        );
    }

    #[test]
    fn prevent_damage_with_no_pending_prevention_errors() {
        let mut state = game_state();
        let result = evaluate_effect(&mut state, &Effect::PreventDamage(1), None, &CardRegistry::new());
        assert_eq!(result, Err(RulesError::NoPendingPrevention));
    }

    #[test]
    fn prevent_damage_against_a_pending_trash_errors_prevention_kind_mismatch() {
        let mut state = game_state();
        state.pending_prevention = Some(PendingPrevention {
            kind: PendingPreventionKind::Trash { target: CardTarget::RunnerRig(CardId("corroder".to_string())), prevented: false },
            source_card: None,
            resume: PreventionResume::None,
        });

        let result = evaluate_effect(&mut state, &Effect::PreventDamage(1), None, &CardRegistry::new());

        assert_eq!(
            result,
            Err(RulesError::PreventionKindMismatch { expected: PreventionKind::Damage, actual: PreventionKind::Trash })
        );
    }

    #[test]
    fn trash_card_with_a_prevention_ability_in_play_parks_a_pending_prevention_and_opens_a_window() {
        let mut state = game_state();
        state.runner.rig = vec![
            InstalledRunnerCard {
                card: CardId("plascrete".to_string()),
                ..Default::default()
            },
            InstalledRunnerCard {
                card: CardId("corroder".to_string()),
                base_strength: 2,
                ..Default::default()
            },
        ];
        let registry = CardRegistry::from_cards(vec![card_with_paid_ability("plascrete", Side::Runner, Effect::PreventTrash)]);

        let target = CardTarget::RunnerRig(CardId("corroder".to_string()));
        evaluate_effect(&mut state, &Effect::TrashCard(target.clone()), None, &registry).unwrap();

        // Nothing trashed yet, and the card named by `target` is still rigged.
        assert!(state.runner.rig.iter().any(|c| c.card == CardId("corroder".to_string())));
        assert_eq!(
            state.pending_prevention.as_ref().map(|p| &p.kind),
            Some(&PendingPreventionKind::Trash { target, prevented: false })
        );
        let window = state.paid_ability_window.expect("a Prevention window should be open");
        assert_eq!(window.checkpoint, WindowCheckpoint::Prevention);
        // The targeted card is the Runner's, so the Runner holds priority.
        assert_eq!(window.active_priority, Side::Runner);
    }

    #[test]
    fn prevent_trash_marks_the_parked_trash_prevented() {
        let mut state = game_state();
        let target = CardTarget::RunnerRig(CardId("corroder".to_string()));
        state.pending_prevention = Some(PendingPrevention {
            kind: PendingPreventionKind::Trash { target: target.clone(), prevented: false },
            source_card: None,
            resume: PreventionResume::None,
        });

        evaluate_effect(&mut state, &Effect::PreventTrash, None, &CardRegistry::new()).unwrap();

        assert_eq!(state.pending_prevention.map(|p| p.kind), Some(PendingPreventionKind::Trash { target, prevented: true }));
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
        state.active_run = Some(RunState {
            phase: RP::ApproachIce,
            jack_out_permitted: true,
            ..Default::default()
        });

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
            phase: RP::ApproachIce,
            jack_out_permitted: true,
            ..Default::default()
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
            check_requirement(&state, &EffectRequirement::IsTagged, Side::Runner, None, &CardRegistry::new()),
            Err(RulesError::RunnerNotTagged)
        );

        state.runner.tags = 1;
        assert_eq!(check_requirement(&state, &EffectRequirement::IsTagged, Side::Runner, None, &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn once_per_turn_requirement_fires_once_then_is_silently_skipped_on_a_second_attempt() {
        let mut state = game_state();
        let requirement = EffectRequirement::OncePerTurn("test_tag".to_string());

        assert_eq!(check_requirement(&state, &requirement, Side::Runner, None, &CardRegistry::new()), Ok(()));
        consume_requirement(&mut state, &requirement, Side::Runner);

        assert_eq!(
            check_requirement(&state, &requirement, Side::Runner, None, &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );
        // The Corp's own set is untouched — OncePerTurn is per-side.
        assert_eq!(check_requirement(&state, &requirement, Side::Corp, None, &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn once_per_turn_requirement_resets_at_the_next_turn_start() {
        let mut state = game_state();
        state.phase = GamePhase::Action(Side::Runner);
        let requirement = EffectRequirement::OncePerTurn("docklands_pass".to_string());
        state.runner.once_per_turn_used.insert("docklands_pass".to_string());
        assert_eq!(check_requirement(&state, &requirement, Side::Runner, None, &CardRegistry::new()), Err(RulesError::RequirementNotMet));

        crate::rules::turn::enter_start_of_turn(&mut state, &mut Vec::new(), Side::Runner, &CardRegistry::new()).unwrap();

        assert_eq!(check_requirement(&state, &requirement, Side::Runner, None, &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn runner_credits_at_most_requirement() {
        let mut state = game_state();
        state.runner.resources.credits = Credits(7);
        assert_eq!(
            check_requirement(&state, &EffectRequirement::RunnerCreditsAtMost(6), Side::Runner, None, &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );

        state.runner.resources.credits = Credits(6);
        assert_eq!(check_requirement(&state, &EffectRequirement::RunnerCreditsAtMost(6), Side::Runner, None, &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn not_requirement_inverts_the_inner_result() {
        let state = game_state();
        assert_eq!(
            check_requirement(&state, &EffectRequirement::Not(Box::new(EffectRequirement::IsTagged)), Side::Runner, None, &CardRegistry::new()),
            Ok(())
        );

        let mut tagged = game_state();
        tagged.runner.tags = 1;
        assert_eq!(
            check_requirement(&tagged, &EffectRequirement::Not(Box::new(EffectRequirement::IsTagged)), Side::Runner, None, &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );
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
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 2, true)],
            jack_out_permitted: true,
            ..Default::default()
        });

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
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 1, true)],
            jack_out_permitted: true,
            ..Default::default()
        });

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
            Some(RunState {
                phase: RP::EncounterIce,
                ice: vec![ice],
                jack_out_permitted: true,
                ..Default::default()
            });

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
            Some(RunState {
                phase: RP::ApproachIce,
                jack_out_permitted: true,
                ..Default::default()
            });

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
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![ice],
            jack_out_permitted: true,
            ..Default::default()
        });

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
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![ice],
            jack_out_permitted: true,
            ..Default::default()
        });

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
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![ice],
            jack_out_permitted: true,
            ..Default::default()
        });

        let events = resolve_unbroken_subroutines(&mut state, &CardRegistry::new()).unwrap();

        assert_eq!(events.len(), 2);
        let run = state.active_run.unwrap();
        assert_eq!(run.ice[0].subroutines[0].status, SubroutineStatus::Broken);
        assert_eq!(run.ice[0].subroutines[1].status, SubroutineStatus::Resolved);
    }

    #[test]
    fn modify_strength_updates_current_strength_and_emits_event() {
        let mut state = game_state();
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![test_ice("ice_wall", 3, 0, true)],
            jack_out_permitted: true,
            ..Default::default()
        });

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
            Some(RunState {
                phase: RP::ApproachIce,
                jack_out_permitted: true,
                ..Default::default()
            });

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
            card: CardId("pad_campaign".to_string()),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
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
        // The install was rezzed, so the Runner had already seen it.
        assert_eq!(state.corp.archives, vec![ArchivedCard::faceup(CardId("pad_campaign".to_string()))]);
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
        // Milled off R&D — the Runner never saw it.
        assert_eq!(state.corp.archives, vec![ArchivedCard::facedown(CardId("hedge_fund".to_string()))]);
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
        RunState {
            bad_publicity_credits: amount,
            phase: RP::ApproachIce,
            jack_out_permitted: true,
            ..Default::default()
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

    /// A minimal `CardDefinition` carrying exactly the given `triggers` — everything
    /// else is irrelevant to `process_card_triggers` tests.
    fn card_with_triggers(id: &str, triggers: Vec<TriggeredEffect>) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side: Side::Corp,
            card_type: CardType::Asset,
            triggers,
            is_playable: true,
            ..Default::default()
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
    fn add_counters_on_a_runner_rig_card_increments_its_counters() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("gorman_drip", 0)];
        let acting = CardId("gorman_drip".to_string());

        let events = evaluate_effect(&mut state, &Effect::AddCounters(2), Some(&acting), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.rig[0].counters, 2);
        assert_eq!(events, vec![GameEvent::CountersAdded { card: acting, amount: 2 }]);
    }

    #[test]
    fn remove_counters_on_a_runner_rig_card_decrements_and_saturates_at_zero() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("gorman_drip", 0)];
        state.runner.rig[0].counters = 1;
        let acting = CardId("gorman_drip".to_string());

        let events = evaluate_effect(&mut state, &Effect::RemoveCounters(3), Some(&acting), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.rig[0].counters, 0);
        assert_eq!(events, vec![GameEvent::CountersRemoved { card: acting, amount: 3 }]);
    }

    #[test]
    fn add_counters_on_a_corp_installed_card_increments_its_counters() {
        let mut state = game_state();
        state.corp.installed = vec![InstalledCard {
            card: CardId("some_asset".to_string()),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
        }];
        let acting = CardId("some_asset".to_string());

        evaluate_effect(&mut state, &Effect::AddCounters(3), Some(&acting), &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.installed[0].counters, 3);
    }

    #[test]
    fn add_counters_without_acting_card_errors_unresolved_card_target() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::AddCounters(1), None, &CardRegistry::new()),
            Err(RulesError::UnresolvedCardTarget)
        );
    }

    #[test]
    fn add_counters_for_a_card_neither_installed_nor_rigged_errors() {
        let mut state = game_state();
        let acting = CardId("nowhere".to_string());
        assert_eq!(
            evaluate_effect(&mut state, &Effect::AddCounters(1), Some(&acting), &CardRegistry::new()),
            Err(RulesError::CardNotEligibleForCounters(acting))
        );
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
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![test_ice("ice_wall", ice_strength, subroutine_count, true)],
            jack_out_permitted: true,
            ..Default::default()
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
        state.active_run = Some(RunState {
            phase: RP::ApproachIce,
            jack_out_permitted: true,
            ..Default::default()
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
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![test_ice_of_type("ice_wall", ice_strength, subroutine_count, true, ice_type)],
            jack_out_permitted: true,
            ..Default::default()
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
        state.active_run = Some(RunState {
            phase: RP::EncounterIce,
            ice: vec![ice],
            jack_out_permitted: true,
            ..Default::default()
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
                // `resolve_unbroken_subroutines` now passes the firing ICE
                // itself as `acting_card` (needed for a subroutine effect
                // that self-references its own installed position, e.g.
                // Ansel 1.0/Brân 1.0's "directly inward from this ice") —
                // so a subroutine-initiated trace correctly records its
                // initiating card instead of always `None`.
                GameEvent::TraceInitiated { base: 2, initiating_card: Some(CardId("ice_wall".to_string())) },
            ]
        );
    }

    #[test]
    fn effect_if_evaluates_the_inner_effect_when_the_condition_holds() {
        let mut state = game_state();
        state.runner.tags = 1;
        let effect = Effect::EffectIf {
            condition: EffectRequirement::IsTagged,
            effect: Box::new(Effect::GainCredits(Side::Runner, 3)),
        };

        let events = evaluate_effect(&mut state, &effect, None, &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(8));
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Runner, amount: 3 }]);
    }

    #[test]
    fn effect_if_silently_no_ops_when_the_condition_fails() {
        let mut state = game_state();
        let effect = Effect::EffectIf {
            condition: EffectRequirement::IsTagged,
            effect: Box::new(Effect::GainCredits(Side::Runner, 3)),
        };

        let events = evaluate_effect(&mut state, &effect, None, &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(5), "no credits gained — condition wasn't met");
        assert!(events.is_empty());
    }

    #[test]
    fn offer_paid_choice_parks_pending_state_and_does_not_resolve_immediately() {
        let mut state = game_state();
        let effect = Effect::OfferPaidChoice {
            side: Side::Runner,
            cost: Cost::Credits(4),
            if_paid: Box::new(Effect::Sequence(Vec::new())),
            if_declined: Box::new(Effect::GiveTags(1)),
        };

        let events = evaluate_effect(&mut state, &effect, None, &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.tags, 0, "not resolved yet");
        assert_eq!(state.runner.resources.credits, Credits(5), "not paid yet");
        let pending = state.pending_paid_choice.expect("should be parked");
        assert_eq!(pending.side, Side::Runner);
        assert_eq!(pending.cost, Cost::Credits(4));
        assert_eq!(events, vec![GameEvent::PendingPaidChoiceOffered { side: Side::Runner }]);
    }

    #[test]
    fn present_choice_parks_pending_decision_and_does_not_resolve_immediately() {
        let mut state = game_state();
        let effect = Effect::PresentChoice {
            chooser: Side::Corp,
            options: vec![Effect::GainCredits(Side::Corp, 2), Effect::DrawCards(Side::Corp, 2)],
        };

        let events = evaluate_effect(&mut state, &effect, None, &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(5), "not resolved yet");
        let PendingDecision::ChooseEffect { chooser, options, .. } = state.pending_decision.expect("should be parked")
        else {
            panic!("expected a parked ChooseEffect");
        };
        assert_eq!(chooser, Side::Corp);
        assert_eq!(options.len(), 2);
        assert_eq!(events, vec![GameEvent::PendingChoicePresented { chooser: Side::Corp, option_count: 2 }]);
    }

    #[test]
    fn gain_credits_per_card_accessed_this_run_reads_the_last_completed_run() {
        let mut state = game_state();
        state.last_completed_run = Some(CompletedRun { server: ServerId::Hq, cards_accessed: 3, agendas_stolen: 0, persistent_trashed_upgrades: Vec::new() });

        let events = evaluate_effect(
            &mut state,
            &Effect::GainCreditsPerCardAccessedThisRun(Side::Runner),
            None,
            &CardRegistry::new(),
        )
        .unwrap();

        assert_eq!(state.runner.resources.credits, Credits(8));
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Runner, amount: 3 }]);
    }

    #[test]
    fn gain_credits_per_card_accessed_this_run_is_zero_with_no_completed_run() {
        let mut state = game_state();

        let events = evaluate_effect(
            &mut state,
            &Effect::GainCreditsPerCardAccessedThisRun(Side::Runner),
            None,
            &CardRegistry::new(),
        )
        .unwrap();

        assert_eq!(state.runner.resources.credits, Credits(5));
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Runner, amount: 0 }]);
    }

    #[test]
    fn pay_cost_take_tags_gives_the_runner_tags() {
        let mut state = game_state();

        let events = pay_cost(&mut state, Side::Runner, &Cost::TakeTags(1), None).unwrap();

        assert_eq!(state.runner.tags, 1);
        assert_eq!(events, vec![GameEvent::TagsGiven { side: Side::Runner, amount: 1 }]);
    }

    #[test]
    fn pay_cost_any_of_directly_errors_cost_requires_choice() {
        let mut state = game_state();

        let result = pay_cost(&mut state, Side::Runner, &Cost::AnyOf(vec![Cost::Clicks(1)]), None);

        assert_eq!(result, Err(RulesError::CostRequiresChoice));
    }

    #[test]
    fn rezzed_during_run_against_this_server_requirement_matches_only_the_active_run_server() {
        let mut state = game_state();
        state.corp.installed = vec![crate::rules::state::InstalledCard {
            card: CardId("ping".to_string()),
            slot: crate::rules::state::InstallSlot::Ice,
            rezzed: true,
            ..Default::default()
        }];
        let ping = CardId("ping".to_string());

        assert_eq!(
            check_requirement(
                &state,
                &EffectRequirement::RezzedDuringRunAgainstThisServer,
                Side::Corp,
                Some(&ping),
                &CardRegistry::new()
            ),
            Err(RulesError::RequirementNotMet),
            "no active run at all"
        );

        state.active_run = Some(active_run_state());
        state.active_run.as_mut().unwrap().server = ServerId::RnD;
        assert_eq!(
            check_requirement(
                &state,
                &EffectRequirement::RezzedDuringRunAgainstThisServer,
                Side::Corp,
                Some(&ping),
                &CardRegistry::new()
            ),
            Err(RulesError::RequirementNotMet),
            "run against a different server"
        );

        state.active_run.as_mut().unwrap().server = ServerId::Hq;
        assert_eq!(
            check_requirement(
                &state,
                &EffectRequirement::RezzedDuringRunAgainstThisServer,
                Side::Corp,
                Some(&ping),
                &CardRegistry::new()
            ),
            Ok(())
        );
    }

    #[test]
    fn last_damage_trashed_odd_cost_card_requirement_checks_registry_cost() {
        let mut state = game_state();
        let mut registry = CardRegistry::new();
        registry.insert(crate::cards::common::base_card(
            "odd_cost",
            "Odd Cost",
            Side::Runner,
            crate::dsl::CardType::Event,
            3,
        ));
        registry.insert(crate::cards::common::base_card(
            "even_cost",
            "Even Cost",
            Side::Runner,
            crate::dsl::CardType::Event,
            2,
        ));

        state.last_discarded_cards = vec![CardId("even_cost".to_string())];
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastDamageTrashedOddCostCard, Side::Corp, None, &registry),
            Err(RulesError::RequirementNotMet)
        );

        state.last_discarded_cards = vec![CardId("even_cost".to_string()), CardId("odd_cost".to_string())];
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastDamageTrashedOddCostCard, Side::Corp, None, &registry),
            Ok(())
        );
    }

    #[test]
    fn last_run_was_on_hq_or_rnd_requirement() {
        let mut state = game_state();
        state.last_completed_run = Some(CompletedRun { server: ServerId::Archives, cards_accessed: 0, agendas_stolen: 0, persistent_trashed_upgrades: Vec::new() });
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastRunWasOnHqOrRnD, Side::Runner, None, &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );

        state.last_completed_run = Some(CompletedRun { server: ServerId::Hq, cards_accessed: 2, agendas_stolen: 0, persistent_trashed_upgrades: Vec::new() });
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastRunWasOnHqOrRnD, Side::Runner, None, &CardRegistry::new()),
            Ok(())
        );
    }

    #[test]
    fn and_requirement_requires_both_sides() {
        let mut state = game_state();
        state.runner.tags = 1;
        let req = EffectRequirement::And(
            Box::new(EffectRequirement::IsTagged),
            Box::new(EffectRequirement::RunnerCreditsAtMost(10)),
        );
        assert_eq!(check_requirement(&state, &req, Side::Runner, None, &CardRegistry::new()), Ok(()));

        state.runner.resources.credits = Credits(11);
        assert_eq!(
            check_requirement(&state, &req, Side::Runner, None, &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );
    }
}
