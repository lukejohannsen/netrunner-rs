use crate::cards::CardRegistry;
use crate::dsl::{
    card_matches_filter, Amount, BoostDuration, CardFilter, CardId, CardSubtype, CardTarget, CardType, Cost, Effect,
    EffectRequirement, HostedCardOrigin, IceType, StackZone, StrengthModifier, SubroutineBreakCount, Trigger,
};
use crate::rules::damage;
use crate::rules::dispatcher;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::paid_ability;
use crate::rules::run::{self, AccessPhase, RunPhase, ServerId, SubroutineStatus};
use crate::rules::state::{
    ArchivedCard, Clicks, Credits, DeferredTrigger, GameState, InstallId, InstalledCard, InstalledRunnerCard, OncePerTurnKey, PendingChoiceResume, PendingDecision, PendingPaidChoice,
    PendingPaidChoiceResume, PendingPrevention, PendingPreventionKind, PreventionKind, PreventionResume, Side,
    TraceResume, TraceState, WindowCheckpoint,
};

/// Everything about the resolution *currently in flight* that an effect or
/// requirement may need and cannot read from `GameState`.
///
/// **Transient by construction:** built at the top of a resolution, dropped
/// when it ends, never serialized, never crossing a `PlayerAction`
/// boundary. That is the whole point — it replaces
/// `GameState::last_discarded_cards` and `last_advancement_was_first`,
/// which outlived their resolutions and could be read stale.
///
/// Anything that must *survive* a parked decision belongs on `GameState`
/// instead. `GameState::last_completed_run` is exactly that case and
/// deliberately stays there: `Trigger::OnRunEnded` can be deferred into
/// `GameState::deferred_triggers` and fire on a later `PlayerAction`, by
/// which time any context built here is long gone.
///
/// This is the home AGENTS.md's State Hygiene Rule asks for. A new card
/// needing resolution context adds a field here rather than a scratchpad
/// field on `GameState`.
#[derive(Debug, Default, Clone)]
pub struct ResolutionContext<'a> {
    /// Which card is resolving this — absorbed from `evaluate_effect`'s
    /// former `acting_card` parameter rather than added alongside it, so
    /// arity is unchanged. See `evaluate_effect`'s doc comment for what it
    /// means per-effect.
    pub acting_card: Option<&'a CardId>,
    /// Which *install* of `acting_card` is resolving this, when it is an
    /// installed card. Every "this card" lookup — counters, advancement,
    /// self-trash, host, strength, once-per-turn — resolves through this
    /// when it is `Some`, and only falls back to the first install matching
    /// `acting_card` when it is `None` (an identity, an event, an
    /// operation, a subroutine). Before it existed every such lookup was
    /// first-match by `CardId`: two Fermenters shared one counter pool and
    /// cashing the second trashed the first, Nico Campaign #2 loaded its
    /// counters onto #1, a decoy Urtica Cipher dealt the other Urtica's
    /// damage (ROADMAP Rules Audit §4). A `Some` install that has since
    /// left play resolves to *nothing*, never to a sibling copy.
    pub acting_install: Option<InstallId>,
    /// The event whose triggers are being dispatched, when this resolution
    /// is a trigger rather than a directly activated ability. `None` for an
    /// ability the player activated, a subroutine, or a cost payment.
    ///
    /// Read by `EffectRequirement::WasFirstAdvancementThisCard`, which
    /// needs `GameEvent::CardAdvanced`'s `advancement_tokens` — the fact
    /// the deleted `last_advancement_was_first` field existed to carry.
    /// Generalizes: the next trigger needing its own event's payload reads
    /// it here.
    pub triggering_event: Option<&'a GameEvent>,
    /// Cards a `DealDamage` discarded earlier in this same `Sequence`,
    /// as returned by `damage::apply_damage`. Backs
    /// `EffectRequirement::LastDamageTrashedOddCostCard` (*Diviner*).
    pub damage_discarded: Vec<CardId>,
    /// Credits actually removed by the most recent `Effect::LoseCredits`
    /// in this same resolution — overwritten per `LoseCredits`, never
    /// accumulated, mirroring `damage_discarded`'s contract. Backs
    /// `Amount::CreditsLostThisResolution` (*Account Siphon*).
    pub credits_lost: u32,
    /// How many cards the `PromptChooseCards` this `then` belongs to
    /// selected — `Amount::RemainingAfterSelection` (a sabotage's R&D
    /// half). 0 outside a selection's `then`.
    pub selected_count: u32,
}

impl<'a> ResolutionContext<'a> {
    /// The common case: a resolution attributed to `acting_card`, with no
    /// triggering event and nothing accumulated yet.
    pub fn for_card(acting_card: Option<&'a CardId>) -> Self {
        ResolutionContext { acting_card, ..ResolutionContext::default() }
    }

    /// A trigger's resolution: attributed to `acting_card` and carrying the
    /// event that fired it.
    pub fn for_trigger(acting_card: Option<&'a CardId>, triggering_event: Option<&'a GameEvent>) -> Self {
        ResolutionContext { acting_card, triggering_event, ..ResolutionContext::default() }
    }

    /// A resolution attributed to one specific install of `acting_card` —
    /// an activated ability, or a trigger on an installed card.
    pub fn for_install(acting_install: InstallId, acting_card: &'a CardId) -> Self {
        ResolutionContext { acting_card: Some(acting_card), acting_install: Some(acting_install), ..ResolutionContext::default() }
    }

    /// A trigger's resolution on an installed card: `for_trigger` plus the
    /// install. `acting_install` is `None` for a source with no install.
    pub fn for_install_trigger(
        acting_install: Option<InstallId>,
        acting_card: Option<&'a CardId>,
        triggering_event: Option<&'a GameEvent>,
    ) -> Self {
        ResolutionContext { acting_card, acting_install, triggering_event, ..ResolutionContext::default() }
    }

    /// Rebuilds the context a parked resolution had when it parked —
    /// `PendingPaidChoice::source_install` and friends carry the install
    /// across the `PlayerAction` boundary for exactly this.
    pub fn for_parked(acting_install: Option<InstallId>, acting_card: Option<&'a CardId>) -> Self {
        ResolutionContext { acting_card, acting_install, ..ResolutionContext::default() }
    }
}

/// The Corp install `ctx` is acting as: by `acting_install` when it has one
/// (and `None` if that install has left play — never a sibling copy), else
/// the first install of `acting_card`. The four `acting_*` helpers below
/// are the only way effect resolution looks up "this card"; see
/// `ResolutionContext::acting_install` for why.
fn acting_corp_install<'s>(state: &'s GameState, ctx: &ResolutionContext<'_>) -> Option<&'s InstalledCard> {
    acting_corp_position(state, ctx).map(|position| &state.corp.installed[position])
}

fn acting_corp_install_mut<'s>(state: &'s mut GameState, ctx: &ResolutionContext<'_>) -> Option<&'s mut InstalledCard> {
    acting_corp_position(state, ctx).map(|position| &mut state.corp.installed[position])
}

/// The scored agenda `ctx` is acting as — by `acting_install` only, since
/// a score area can hold two copies of one agenda and only the install
/// handle tells them apart (Off the Books spending its own counters).
fn acting_scored_position(state: &GameState, ctx: &ResolutionContext<'_>) -> Option<usize> {
    let install = ctx.acting_install?;
    state.corp.scored_agendas.iter().position(|scored| scored.install_id == install)
}

/// Whether `ctx` is resolving as the Corp's identity — which has no
/// install, so every "this card" lookup that walks the table misses it.
/// The counter helpers fall through to `CorpState::identity_counters`
/// on this (AU Co.).
fn acting_is_corp_identity(state: &GameState, ctx: &ResolutionContext<'_>) -> bool {
    ctx.acting_install.is_none() && ctx.acting_card.is_some() && ctx.acting_card == state.corp.identity.as_ref()
}

fn acting_corp_position(state: &GameState, ctx: &ResolutionContext<'_>) -> Option<usize> {
    match ctx.acting_install {
        Some(install) => state.corp.installed.iter().position(|c| c.install_id == install),
        None => ctx.acting_card.and_then(|card| state.corp.installed.iter().position(|c| &c.card == card)),
    }
}

/// The rig card `ctx` is acting as — the Runner-side twin of
/// [`acting_corp_install`].
fn acting_rig_card<'s>(state: &'s GameState, ctx: &ResolutionContext<'_>) -> Option<&'s InstalledRunnerCard> {
    acting_rig_position(state, ctx).map(|position| &state.runner.rig[position])
}

fn acting_rig_position(state: &GameState, ctx: &ResolutionContext<'_>) -> Option<usize> {
    match ctx.acting_install {
        Some(install) => state.runner.rig.iter().position(|c| c.install_id == install),
        None => ctx.acting_card.and_then(|card| state.runner.rig.iter().position(|c| &c.card == card)),
    }
}

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
    ctx: &mut ResolutionContext<'_>,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let acting_card = ctx.acting_card;
    match effect {
        Effect::GainCredits(side, amount) => gain_credits_from_ability(state, registry, *side, *amount, ctx),

        Effect::DealDamage(damage_type, amount) => {
            // Only parks a `PendingPrevention`/opens a window if some
            // installed/rigged card actually has a matching `Paid`
            // `PreventDamage` ability — a zero-overhead no-op for every
            // registry with no such card (the entire baseline set today),
            // so this stays a synchronous `apply_damage` call exactly as
            // before in the common case.
            if has_matching_paid_ability(state, registry, |e| matches!(e, Effect::PreventDamage(_))) {
                park_damage_prevention(state, registry, *damage_type, *amount, ctx)
            } else {
                let (mut events, discarded) = damage::apply_damage(state, *damage_type, *amount);
                // Overwrite rather than append: the requirement reading this
                // (`LastDamageTrashedOddCostCard`) asks about the *most
                // recent* damage, so a second `DealDamage` in the same
                // `Sequence` must not be answered from the first's discards.
                ctx.damage_discarded = discarded;
                events.extend(dispatch_damage_taken(state, registry, &events)?);
                Ok(events)
            }
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
            // Ending a run that has already ended is a no-op, not an
            // error: a card's own text can reach here twice (Biawak's
            // second subroutine after its first ended the run), and an
            // `Err` there fails the whole action that got here — which,
            // when that action is the confirmation of a parked selection,
            // leaves a decision nothing can resolve. The view-path sweep
            // found exactly that at seed 19 as 10,000 fruitless toggles.
            let Some(run) = state.active_run.as_mut() else {
                return Ok(Vec::new());
            };
            // Shred: the first Corp attempt to end the run is intercepted
            // and turned into the Corp's paid choice — see
            // `EndRunPrevention`. `take()` makes it the first attempt only.
            if let Some(prevention) = run.end_run_prevention.take() {
                let server = run.server;
                match prevention {
                    crate::dsl::EndRunPrevention::UnlessCorpTrashesRootCountFromHq => {
                        let root_count = state
                            .corp
                            .installed
                            .iter()
                            .filter(|c| c.server == server && c.slot == crate::rules::InstallSlot::Root)
                            .count() as u32;
                        if root_count > 0 {
                            let mut events = vec![GameEvent::RunEndPrevented { server }];
                            events.extend(evaluate_effect(
                                state,
                                &Effect::OfferPaidChoice {
                                    side: Side::Corp,
                                    cost: Cost::TrashRandomFromHq(root_count),
                                    if_paid: Box::new(Effect::EndTheRun),
                                    if_declined: Box::new(Effect::Sequence(Vec::new())),
                                },
                                ctx,
                                registry,
                            )?);
                            return Ok(events);
                        }
                    }
                }
            }
            let run = run::end_run(state).expect("checked Some above");
            let server = run.server;
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
            // The event reports what actually came off, not what was asked
            // for: a trigger keyed on a tag being removed (Synapse Global)
            // must not fire on a request that removed nothing.
            let removed = state.runner.tags.min(*amount);
            state.runner.tags -= removed;
            let event = GameEvent::TagsRemoved { side: Side::Runner, amount: removed };
            // Dispatched here rather than from the caller, for the same
            // reason `DamageTaken` is: the event is produced deep in an
            // effect and returned, and Synapse Global: Faster than Thought
            // reacts to it.
            let mut events = vec![event.clone()];
            events.extend(dispatcher::dispatch_event(state, registry, &event)?);
            Ok(events)
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
            // Hosted, uninstalled cards (Bling's) have no prevention
            // window of their own and no single owner: each goes to its
            // own side's discard pile.
            if matches!(target, CardTarget::HostedOnThisCard) {
                return trash_hosted_cards(state, registry, ctx);
            }
            // Same zero-overhead-unless-a-card-cares gating as `DealDamage`.
            if has_matching_paid_ability(state, registry, |e| matches!(e, Effect::PreventTrash)) {
                park_trash_prevention(state, registry, target.clone(), ctx)
            } else {
                trash_card(state, target, ctx)
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

        Effect::AddCounters(amount) => modify_counters(state, ctx, i64::from(*amount)),

        Effect::RemoveCounters(amount) => modify_counters(state, ctx, -i64::from(*amount)),

        Effect::TakeAllCountersAsCredits(side) => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let current = counters_of(state, ctx).ok_or_else(|| RulesError::CardNotEligibleForCounters(card_id.clone()))?;
            let mut events = modify_counters(state, ctx, -i64::from(current))?;
            events.extend(gain_credits_from_ability(state, registry, *side, current, ctx)?);
            Ok(events)
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
            let (install, card_id, _server) = resolve_corp_installed_target(state, target, ctx)?;
            let installed = state
                .corp
                .installed
                .iter_mut()
                .find(|c| c.install_id == install)
                .ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            installed.rezzed = false;
            Ok(vec![GameEvent::CardDerezzed { card: card_id }])
        }

        Effect::GainCreditsPerCounter { side, credits_per_counter } => {
            acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let current = counters_of(state, ctx).unwrap_or(0);
            let amount = current.saturating_mul(*credits_per_counter);
            gain_credits_from_ability(state, registry, *side, amount, ctx)
        }

        Effect::SwapInstalledIce(a, b) => {
            // Legal mid-run: `run::reconcile_ice` rebuilds the run's ICE
            // list from `corp.installed` at the next step and keeps
            // `position` on the same install. This used to be refused
            // outright (`CannotSwapIceDuringActiveRun`) because the list
            // was a snapshot that could not follow a swap.
            for id in [a, b] {
                if state.find_corp_install(*id).is_none() {
                    return Err(RulesError::InstallNotFound(*id));
                }
            }
            let pos_a =
                state.corp.installed.iter().position(|c| c.install_id == *a).expect("checked above");
            let pos_b =
                state.corp.installed.iter().position(|c| c.install_id == *b).expect("checked above");
            let (card_a, card_b) =
                (state.corp.installed[pos_a].card.clone(), state.corp.installed[pos_b].card.clone());
            let (server_a, slot_a) = (state.corp.installed[pos_a].server, state.corp.installed[pos_a].slot);
            let (server_b, slot_b) = (state.corp.installed[pos_b].server, state.corp.installed[pos_b].slot);
            state.corp.installed[pos_a].server = server_b;
            state.corp.installed[pos_a].slot = slot_b;
            state.corp.installed[pos_b].server = server_a;
            state.corp.installed[pos_b].slot = slot_a;
            // The event names the cards, not the installs, because
            // `Trigger` dispatch is keyed by `CardId`. A client sees it
            // only through `masking::mask_event_for_player`, which drops
            // it for the Runner while either card is unrezzed.
            Ok(vec![GameEvent::IceSwapped { a: card_a, b: card_b }])
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
            // An authored `slot: Ice` with a filter that admits a non-ICE
            // would otherwise put an agenda in the ICE column; same guard
            // `engine::install_card` applies to the player's own installs.
            if resolved_slot == crate::rules::InstallSlot::Ice && !matches!(card_def.card_type, crate::dsl::CardType::Ice(_)) {
                return Err(RulesError::CardTypeMismatch { card: card_id.clone(), expected: "ice" });
            }
            let new_card = crate::rules::InstalledCard {
                card: card_id.clone(),
                install_id: state.allocate_install_id(),
                server: *into,
                slot: resolved_slot,
                rezzed: false,
                advancement_tokens: 0,
                counters: 0,
                installed_this_turn: true,
            };
            match insert_after {
                Some(host) => {
                    let host_pos = state.corp.installed.iter().position(|c| c.install_id == *host);
                    match host_pos {
                        Some(i) => state.corp.installed.insert(i + 1, new_card),
                        None => state.corp.installed.push(new_card),
                    }
                }
                None => state.corp.installed.push(new_card),
            }
            Ok(vec![GameEvent::CardInstalled { side: Side::Corp, card: card_id.clone(), server: *into }])
        }

        Effect::DrawCardsAmount(side, amount) => {
            let resolved = resolve_amount(amount, ctx, state, registry);
            evaluate_effect(state, &Effect::DrawCards(*side, resolved), ctx, registry)
        }

        Effect::RefillCountersTo(target) => {
            let current = counters_of(state, ctx)
                .ok_or_else(|| RulesError::CardNotEligibleForCounters(acting_card.cloned().unwrap_or(CardId(String::new()))))?;
            if current >= *target {
                return Ok(Vec::new());
            }
            modify_counters(state, ctx, i64::from(*target - current))
        }

        Effect::InstallRunnerCardFromHeap => {
            use crate::rules::engine::{can_install_runner_card_from_zone, install_runner_card_from_zone_paying_cost, RunnerCardSource};
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            // Same leniency as the grip variant: an uninstallable pick stays
            // where it is.
            if !can_install_runner_card_from_zone(state, registry, &card_id, RunnerCardSource::Heap) {
                return Ok(Vec::new());
            }
            install_runner_card_from_zone_paying_cost(state, registry, card_id, RunnerCardSource::Heap)
        }

        Effect::InstallRunnerCardFromGripWithDiscount(discount) => {
            use crate::rules::engine::{can_install_runner_card_from_zone_with_discount, install_runner_card_from_zone_with_discount, RunnerCardSource};
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            if !can_install_runner_card_from_zone_with_discount(state, registry, &card_id, RunnerCardSource::Grip, *discount) {
                return Ok(Vec::new());
            }
            install_runner_card_from_zone_with_discount(state, registry, card_id, RunnerCardSource::Grip, *discount)
        }

        Effect::InstallRunnerCardFromHost => {
            use crate::rules::engine::{can_install_runner_card_from_zone, install_runner_card_from_zone_paying_cost, RunnerCardSource};
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            let host = ctx.acting_install.ok_or(RulesError::MissingActingCardContext)?;
            if !can_install_runner_card_from_zone(state, registry, &card_id, RunnerCardSource::Hosted(host)) {
                return Ok(Vec::new());
            }
            install_runner_card_from_zone_paying_cost(state, registry, card_id, RunnerCardSource::Hosted(host))
        }

        Effect::RedirectRunOnApproach(target) => {
            let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
            run.redirect_on_approach = Some(*target);
            Ok(Vec::new())
        }

        Effect::SetRunEndedEffect(effect) => {
            let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
            run.on_end_effect = Some(effect.clone());
            run.on_end_card = acting_card.cloned();
            run.on_end_install = ctx.acting_install;
            Ok(Vec::new())
        }

        Effect::ArmRunEndPrevention(prevention) => {
            let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
            run.end_run_prevention = Some(*prevention);
            Ok(Vec::new())
        }

        Effect::Sabotage(count) => {
            let hq = state.corp.hq.len() as u32;
            let rd = state.corp.r_and_d.len() as u32;
            let from_hq_max = (*count).min(hq);
            let from_hq_min = count.saturating_sub(rd).min(from_hq_max);
            if from_hq_max == 0 {
                // Nothing to choose: it all comes off the top of R&D.
                return evaluate_effect(state, &Effect::MillRnDAmount(Amount::Fixed(*count)), ctx, registry);
            }
            state.pending_decision = Some(PendingDecision::ChooseCards {
                side: Side::Corp,
                source: crate::dsl::CardZoneRef::OwnHq,
                filter: CardFilter::Any,
                min: from_hq_min,
                max: from_hq_max,
                reveal: false,
                shuffle_after: false,
                destination: Some(crate::dsl::CardZoneRef::OwnArchives),
                then: Some(Box::new(Effect::MillRnDAmount(Amount::RemainingAfterSelection(*count)))),
                selected: Vec::new(),
                source_card: acting_card.cloned(),
                source_install: ctx.acting_install,
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingCardSelectionOffered { side: Side::Corp, min: from_hq_min, max: from_hq_max }])
        }

        Effect::MillRnDAmount(amount) => {
            let count = resolve_amount(amount, ctx, state, registry);
            let mut events = Vec::new();
            for _ in 0..count {
                if state.corp.r_and_d.is_empty() {
                    break;
                }
                events.extend(trash_card(state, &CardTarget::TopOfStack { side: Side::Corp, zone: StackZone::RAndD }, ctx)?);
            }
            Ok(events)
        }

        Effect::HostCardOnThisCard(origin) => {
            let acting = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            let card = match origin {
                HostedCardOrigin::RandomFromHq => {
                    if state.corp.hq.is_empty() {
                        return Ok(Vec::new());
                    }
                    let index = (state.next_u64() % state.corp.hq.len() as u64) as usize;
                    state.corp.hq.remove(index)
                }
                HostedCardOrigin::TopOfStack => match state.runner.stack.pop() {
                    Some(card) => card,
                    None => return Ok(Vec::new()),
                },
            };
            let position = acting_rig_position(state, ctx)
                .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: acting.clone() })?;
            state.runner.rig[position].hosted_cards.push(card.clone());
            Ok(vec![GameEvent::CardHosted { card, host: acting }])
        }

        Effect::BypassEncounteredIce => run::bypass_encountered_ice(state),

        Effect::FlipIdentity => {
            // Whichever identity is resolving. Only an identity carries a
            // flip side, so the acting card names the side directly; the
            // Runner is the fallback because Dewi Subrotoputri was the
            // only flip identity before Nebula Talent Management.
            let side = if acting_card.is_some() && acting_card == state.corp.identity.as_ref() { Side::Corp } else { Side::Runner };
            match side {
                Side::Corp => state.corp.identity_flipped = !state.corp.identity_flipped,
                Side::Runner => state.runner.identity_flipped = !state.runner.identity_flipped,
            }
            Ok(vec![GameEvent::IdentityFlipped { side }])
        }

        Effect::AddToBottomOfStack => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            let zones = [&mut state.runner.heap, &mut state.runner.grip];
            for zone in zones {
                if let Some(position) = zone.iter().position(|c| c == &card_id) {
                    zone.remove(position);
                    // The stack draws from the end of the `Vec`, so index 0
                    // is its bottom.
                    state.runner.stack.insert(0, card_id.clone());
                    return Ok(vec![GameEvent::CardAddedToBottomOfStack { card: card_id }]);
                }
            }
            Ok(Vec::new())
        }

        Effect::HostRigCardOnInstall { card, host } => {
            if card == host {
                return Err(RulesError::InstallNotFound(*host));
            }
            if !state.runner.rig.iter().any(|c| c.install_id == *host) {
                return Err(RulesError::InstallNotFound(*host));
            }
            let hosted = state
                .runner
                .rig
                .iter_mut()
                .find(|c| c.install_id == *card)
                .ok_or(RulesError::InstallNotFound(*card))?;
            hosted.hosted_on_program = Some(*host);
            let hosted_card = hosted.card.clone();
            let host_card = state.runner.rig.iter().find(|c| c.install_id == *host).map(|c| c.card.clone()).unwrap();
            Ok(vec![GameEvent::CardHosted { card: hosted_card, host: host_card }])
        }

        Effect::InstallRunnerCardFromGrip => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            // Eligibility may have shifted since the selection was offered
            // (`CardFilter::InstallableRunnerCard` checked it then — but
            // Mutual Favor's fetched icebreaker was never filtered on
            // affordability at all). If the pick is not installable, the
            // card simply stays in the grip: the same "nothing to do"
            // leniency `PromptChooseCards`'s fewer-than-`min` case
            // establishes, never an error that would fail the decision
            // resolving it.
            if !crate::rules::engine::can_install_runner_card_from_grip(state, registry, &card_id) {
                return Ok(Vec::new());
            }
            crate::rules::engine::install_runner_card_from_grip_paying_cost(state, registry, card_id)
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
            let installed =
                acting_corp_install_mut(state, ctx).ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            installed.advancement_tokens = installed.advancement_tokens.saturating_add(*amount);
            let advancement_tokens = installed.advancement_tokens;
            Ok(vec![GameEvent::CardAdvanced { card: card_id.clone(), advancement_tokens }])
        }

        Effect::BoostStrength { amount, duration } => {
            let acting = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            require_encounter(state)?;
            let position = acting_rig_position(state, ctx)
                .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: acting.clone() })?;
            // A hosted card can lengthen the boost (GAMEDRAGON™ Pro:
            // "abilities that increase its strength last for the remainder
            // of the run"). Only ever a lengthening — see
            // `HostedBreakerBonus::boosts_last_the_run`.
            let host_install = state.runner.rig[position].install_id;
            let lasts_the_run = hosted_breaker_bonuses(state, registry, host_install).any(|bonus| bonus.boosts_last_the_run);
            let duration = match duration {
                BoostDuration::Encounter if lasts_the_run => BoostDuration::Run,
                other => *other,
            };
            let card = &mut state.runner.rig[position];
            match duration {
                BoostDuration::Encounter => card.encounter_strength_buff += *amount as i32,
                BoostDuration::Run => card.run_strength_buff += *amount as i32,
                BoostDuration::Turn => card.turn_strength_buff += *amount as i32,
            }
            let new_strength = card.effective_strength();
            Ok(vec![GameEvent::StrengthBoosted {
                card_id: acting.clone(),
                new_strength,
                delta: *amount as i32,
                duration,
            }])
        }

        Effect::BreakSubroutines { count, restrict_to } => {
            let acting = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
            if run.phase != RunPhase::EncounterIce {
                return Err(RulesError::NotInEncounter);
            }

            let breaker = acting_rig_card(state, ctx)
                .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: acting.clone() })?;
            let breaker_strength = computed_runner_strength(breaker, state, registry);

            let run = state.active_run.as_ref().unwrap();
            let ice = &run.ice[run.position];
            let (ice_card_id, ice_strength, ice_type, ice_install) =
                (ice.card_id.clone(), ice.current_strength, ice.ice_type, ice.install_id);
            if let Some(expected) = restrict_to
                && *expected != ice_type
                && !ice_gains_subtype_from_hosted(state, registry, ice_install, *expected)
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
            // transition_subroutine's &mut state. A subroutine only a
            // printed subtype may break (Semak-samun) stays pending for a
            // breaker without it.
            let breaker_def = registry.get(acting);
            let pending: Vec<usize> = ice
                .subroutines
                .iter()
                .filter(|s| s.status == SubroutineStatus::Pending && subroutine_breakable_by(s, breaker_def))
                .map(|s| s.id)
                .collect();
            if pending.is_empty() {
                return Err(RulesError::NoBreakableSubroutine { ice: ice_card_id });
            }

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
            let breaker_def = acting_card.and_then(|card| registry.get(card));
            let pending: Vec<usize> = ice
                .subroutines
                .iter()
                .filter(|s| s.status == SubroutineStatus::Pending && subroutine_breakable_by(s, breaker_def))
                .map(|s| s.id)
                .collect();
            if pending.is_empty() {
                return Err(RulesError::NoBreakableSubroutine { ice: ice.card_id.clone() });
            }
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
                initiating_install: ctx.acting_install,
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
                // A rules no-op, not a modelling gap: a breach of Archives
                // accesses every card there and a breach of a remote every
                // card in its root, so "one additional card" changes
                // nothing. No event either — this used to emit
                // `AdditionalAccessGranted` here, recording a grant that
                // had no effect.
                ServerId::Archives | ServerId::Remote(_) => return Ok(Vec::new()),
            }
            Ok(vec![GameEvent::AdditionalAccessGranted { server: *server, count: *count }])
        }

        Effect::SetAccessReplacement { server, effect, optional } => {
            let run = state.active_run.as_mut().ok_or(RulesError::NoActiveRun)?;
            run.access_replacement = Some((*server, (**effect).clone(), *optional));
            Ok(vec![GameEvent::AccessReplacementSet { server: *server }])
        }

        Effect::Sequence(effects) => {
            let mut events = Vec::new();
            for (index, inner) in effects.iter().enumerate() {
                events.extend(evaluate_effect(state, inner, ctx, registry)?);
                // Stops at `GameOver`: Clearinghouse's `[DealDamage,
                // TrashCard(ThisCard)]` used to run its trash against a
                // finished game.
                if state.is_over() {
                    break;
                }
                // That effect parked something spanning future
                // `PlayerAction`s: the rest of the sequence is queued as a
                // continuation on the deferred-trigger queue, pinned to the
                // acting card, and drains once the decision resolves — see
                // `Effect::Sequence`'s doc comment. Only a resolution with an
                // acting card can be pinned; a bare effect evaluation stops
                // here as it always did.
                if state.is_resolution_blocked() {
                    let rest = &effects[index + 1..];
                    if let (Some(card), false) = (ctx.acting_card, rest.is_empty()) {
                        state.deferred_triggers.push(crate::rules::state::DeferredTrigger {
                            card: card.clone(),
                            trigger: Trigger::OnPlay,
                            target: None,
                            install: ctx.acting_install,
                            target_install: None,
                            event: ctx.triggering_event.cloned(),
                            continuation: Some(Effect::Sequence(rest.to_vec())),
                        });
                    }
                    break;
                }
            }
            Ok(events)
        }

        Effect::GainCreditsAmount(side, amount) => {
            let amount = resolve_amount(amount, ctx, state, registry);
            gain_credits_from_ability(state, registry, *side, amount, ctx)
        }

        Effect::LoseCredits(side, amount) => {
            let before = state.resources(*side).credits.0;
            // What was actually removed, not the printed amount — recorded
            // for `Amount::CreditsLostThisResolution` (Account Siphon's
            // "2[c] for each credit lost"), and emitted, since "loses 3"
            // against a 2-credit pool loses 2.
            let lost = (*amount).min(before);
            state.resources_mut(*side).credits = Credits(before - lost);
            ctx.credits_lost = lost;
            Ok(vec![GameEvent::CreditsLost { side: *side, amount: lost }])
        }

        Effect::LoseCreditsAmount(side, amount) => {
            let amount = resolve_amount(amount, ctx, state, registry);
            evaluate_effect(state, &Effect::LoseCredits(*side, amount), ctx, registry)
        }

        Effect::PurgeVirusCounters => Ok(vec![crate::rules::engine::purge_all_virus_counters(state, registry)]),

        // Rewritten into the `PresentChoice` it is shorthand for: each option
        // followed by the same offer over the rest, one fewer to resolve.
        Effect::ResolveSomeOf { chooser, count, options } => {
            if *count == 0 || options.is_empty() {
                return Ok(Vec::new());
            }
            let expanded: Vec<Effect> = options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    let remaining: Vec<Effect> =
                        options.iter().enumerate().filter(|(other, _)| *other != index).map(|(_, e)| e.clone()).collect();
                    Effect::Sequence(vec![
                        option.clone(),
                        Effect::ResolveSomeOf { chooser: *chooser, count: count - 1, options: remaining },
                    ])
                })
                .collect();
            evaluate_effect(state, &Effect::PresentChoice { chooser: *chooser, options: expanded }, ctx, registry)
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
            if let Some(run) = state.active_run.as_mut() {
                run.initiated_by = acting_card.cloned();
            }
            let run_initiated_event = GameEvent::RunInitiated { server: *server };
            let mut events = vec![run_initiated_event.clone()];
            events.extend(crate::rules::dispatcher::dispatch_event(state, registry, &run_initiated_event)?);
            Ok(events)
        }

        Effect::EffectIf { condition, effect } => {
            let side = acting_side(acting_card, registry);
            if check_requirement(state, condition, side, ctx, registry).is_ok() {
                evaluate_effect(state, effect, ctx, registry)
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
                source_install: ctx.acting_install,
                resume: PendingPaidChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingPaidChoiceOffered { side: *side }])
        }

        Effect::PresentChoice { chooser, options } => {
            state.pending_decision = Some(PendingDecision::ChooseEffect {
                chooser: *chooser,
                options: options.clone(),
                source_card: acting_card.cloned(),
                source_install: ctx.acting_install,
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingChoicePresented { chooser: *chooser, option_count: options.len() }])
        }

        Effect::GainCreditsPerCardAccessedThisRun(side) => {
            let amount = state.last_completed_run.as_ref().map_or(0, |run| run.cards_accessed);
            gain_credits_from_ability(state, registry, *side, amount, ctx)
        }

        Effect::PromptChooseCards { side, source, filter, min, max, reveal, shuffle_after, destination, then } => {
            let available = crate::rules::pending_choice::eligible_positions(state, registry, *side, source, filter, ctx.acting_install);
            if available.is_empty() || available.len() < *min as usize {
                // Nothing to do — same "silently no-op" leniency
                // `DrawCards`/`TrashCard`'s "already gone" case establish.
                // e.g. Hansei Review's "if there are any cards in HQ".
                // Nothing eligible at all is the same case even for a
                // `min: 0` offer (Bumi 1.0 with no trojan in the rig):
                // parking a choice whose only resolution is an empty
                // confirmation costs the player a decision for nothing.
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
                source_install: ctx.acting_install,
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingCardSelectionOffered { side: *side, min: *min, max: *max }])
        }

        Effect::PromptChooseServer {
            chooser,
            rez_cost_delta,
            bonus_run_credits,
            allowed_servers,
            on_success,
            on_start,
            exclude_servers_run_this_turn,
        } => {
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
            //
            // Shares `run::check_run_may_begin` with `start_run` itself
            // rather than restating the condition — the two MUST agree, or
            // this parks a decision `start_run` will refuse. See that
            // function's doc comment; a narrower copy here is exactly what
            // caused the original deadlock.
            run::check_run_may_begin(state)?;
            // Narrow the offer here, for the same reason as the check
            // above: an offer with nothing in it is a decision nothing can
            // resolve, and refusing to park makes the probe drop the
            // ability. The narrowed list is what the decision carries, so
            // resolution's re-check and the candidate filter need no
            // knowledge of why a server is missing.
            let allowed_servers = if *exclude_servers_run_this_turn {
                let already_run = &state.runner.servers_run_this_turn;
                // `None` means every server — enumerated the way
                // `legal_actions` offers them, fresh remote included.
                let every_server = || {
                    let existing = crate::rules::legal_actions::existing_remote_ids(state);
                    let mut servers = vec![ServerId::Hq, ServerId::RnD, ServerId::Archives];
                    servers.extend(existing.iter().copied().map(ServerId::Remote));
                    servers.push(ServerId::Remote(crate::rules::legal_actions::fresh_remote_id(&existing)));
                    servers
                };
                let offered: Vec<ServerId> = allowed_servers
                    .clone()
                    .unwrap_or_else(every_server)
                    .into_iter()
                    .filter(|server| !already_run.contains(server))
                    .collect();
                if offered.is_empty() {
                    return Err(RulesError::NoServerLeftToRun);
                }
                Some(offered)
            } else {
                allowed_servers.clone()
            };
            state.pending_decision = Some(PendingDecision::ChooseServer {
                chooser: *chooser,
                rez_cost_delta: *rez_cost_delta,
                bonus_run_credits: *bonus_run_credits,
                allowed_servers,
                on_success: on_success.clone(),
                on_start: on_start.clone(),
                install: None,
                source_card: acting_card.cloned(),
                source_install: ctx.acting_install,
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingServerChoiceOffered { chooser: *chooser }])
        }

        Effect::PromptInstallCorpCard { origin_zone, ignore_costs, discount, then, remote_only } => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            // First match by position: two copies of one card in HQ are
            // indistinguishable and interchangeable, so "the copy the Corp
            // just selected" and "the first copy" are the same card.
            let position = match origin_zone {
                crate::dsl::CardZoneRef::OwnHq => state.corp.hq.iter().position(|c| c == &card_id),
                crate::dsl::CardZoneRef::OwnArchives => {
                    state.corp.archives.iter().position(|a| a.card == card_id)
                }
                // Poétrï Luxury Brands installs one of the top 3 cards of
                // R&D — the selection narrowed the offer to the top three
                // (`CardFilter::TopOfZone`), but the card is taken from
                // R&D as a whole, so this is a first-match lookup like
                // HQ's.
                crate::dsl::CardZoneRef::OwnRAndD => state.corp.r_and_d.iter().position(|c| c == &card_id),
                _ => return Err(RulesError::UnresolvedCardTarget),
            };
            // Card gone from the zone, an uninstallable type, or (for ICE)
            // no affordable destination: nothing to offer — the same
            // "nothing to do" leniency `PromptChooseCards` establishes. A
            // fresh remote is always free, so this only bites for a type
            // that cannot be installed at all.
            let Some(position) = position else { return Ok(Vec::new()) };
            let Some(card_def) = registry.get(&card_id) else { return Ok(Vec::new()) };
            let mut allowed = crate::rules::engine::corp_install_destinations(state, registry, card_def, *ignore_costs);
            if *remote_only {
                allowed.retain(|server| matches!(server, crate::rules::run::ServerId::Remote(_)));
            }
            if allowed.is_empty() {
                return Ok(Vec::new());
            }
            state.pending_decision = Some(PendingDecision::ChooseServer {
                chooser: Side::Corp,
                rez_cost_delta: 0,
                bonus_run_credits: 0,
                allowed_servers: Some(allowed),
                on_success: None,
                on_start: None,
                install: Some(crate::rules::state::PendingInstallFromZone {
                    origin: origin_zone.clone(),
                    position,
                    pay_cost: !ignore_costs,
                    discount: *discount,
                    remote_only: *remote_only,
                    then: then.clone(),
                }),
                // Deliberately NOT the chosen card: `source_card` passes
                // through the masked view, and the pick out of HQ is
                // hidden information until it lands. The install payload
                // above names it by position instead.
                source_card: None,
                source_install: ctx.acting_install,
                resume: PendingChoiceResume::None,
            });
            Ok(vec![GameEvent::PendingServerChoiceOffered { chooser: Side::Corp }])
        }

        Effect::MoveThisCardToRoot(server) => {
            let Some(position) = acting_corp_position(state, ctx) else { return Ok(Vec::new()) };
            let installed = &state.corp.installed[position];
            if installed.slot != crate::rules::state::InstallSlot::Root || installed.server == *server {
                return Ok(Vec::new());
            }
            let (card, from) = (installed.card.clone(), installed.server);
            state.corp.installed[position].server = *server;
            Ok(vec![GameEvent::CardMoved { card, from, to: *server }])
        }

        Effect::PlayOperation { from } => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            let from_archives = matches!(from, crate::dsl::CardZoneRef::OwnArchives);
            // Lenient, like every other "you may" here: an operation whose
            // cost or `play_requirement` has stopped holding since the
            // offer resolves to nothing rather than failing the action
            // that got here.
            if !crate::rules::engine::can_play_operation(state, registry, &card_id, from_archives) {
                return Ok(Vec::new());
            }
            crate::rules::engine::play_operation_card(state, registry, card_id, from_archives)
        }

        Effect::RezInstalled { install, pay_cost, discount } => {
            // Shares `engine::rez_install` with `PlayerAction::RezIce`, so a
            // discounted rez pays through the same waterfall (a region's
            // hosted rez credits before the wallet) and fires `OnRez`
            // identically. Unaffordable is a no-op, not an error: Mycoweb's
            // "you may rez ... paying 2[c] less" and both branches of
            // Biawak's forfeit choice must resolve to *something*, and a
            // failing effect would leave the decision that parked them
            // unresolvable. `AlreadyRezzed`/`InstallNotFound` still error —
            // those mean the card was named wrongly, not priced wrongly.
            match crate::rules::engine::rez_install(state, registry, *install, *pay_cost, *discount) {
                Err(RulesError::NotEnoughCredits { .. }) => Ok(Vec::new()),
                other => other,
            }
        }

        Effect::ResolveSubroutineOfSelectedIce => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?;
            let Some(card_def) = registry.get(card_id) else { return Ok(Vec::new()) };
            let options: Vec<Effect> = card_def.subroutines.iter().map(|sub| sub.effect.clone()).collect();
            // A rezzed piece of ice with no subroutines resolves nothing
            // rather than parking an empty choice — the same leniency an
            // empty `PromptChooseCards` offer takes.
            if options.is_empty() {
                return Ok(Vec::new());
            }
            // One subroutine is not a choice; resolve it directly rather
            // than asking the Corp to confirm the only option.
            if let [only] = options.as_slice() {
                let only = only.clone();
                return evaluate_effect(state, &only, ctx, registry);
            }
            evaluate_effect(state, &Effect::PresentChoice { chooser: Side::Corp, options }, ctx, registry)
        }

        Effect::GainClicksNextTurn(side, amount) => {
            // Runner-side is authored nowhere and banked nowhere: the
            // Runner's allotment has no field of its own to add to, and no
            // card prints it. Recorded as a no-op rather than a panic.
            if *side == Side::Corp {
                state.corp.extra_clicks_next_turn = state.corp.extra_clicks_next_turn.saturating_add(*amount);
            }
            Ok(Vec::new())
        }

        Effect::ForfeitAgendas(count) => {
            let mut events = Vec::new();
            for _ in 0..*count {
                // Lowest printed points first, ties to the fewest agenda
                // counters — see the variant's doc comment for why the
                // Corp is not asked which.
                let chosen = state
                    .corp
                    .scored_agendas
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, scored)| {
                        (crate::rules::win::agenda_value(&scored.card, registry).unwrap_or(0), scored.agenda_counters)
                    })
                    .map(|(index, _)| index);
                let Some(index) = chosen else { break };
                let forfeited = state.corp.scored_agendas.remove(index);
                state.corp.removed_from_game.push(forfeited.card.clone());
                // The agenda's own "when you forfeit this" reaction
                // (Greenmail) fires as the card, with no install: it has
                // already left the score area, and a `Some` install that
                // is gone resolves to nothing.
                let forfeited_event = GameEvent::AgendaForfeited { card: forfeited.card.clone() };
                events.push(forfeited_event.clone());
                events.push(GameEvent::CardRemovedFromGame { side: Side::Corp, card: forfeited.card.clone() });
                events.extend(dispatcher::dispatch_event(state, registry, &forfeited_event)?);
            }
            Ok(events)
        }

        Effect::InstallAgendaFromRunnerScoreArea => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            let Some(position) = state.runner.scored_agendas.iter().position(|c| c == &card_id) else {
                return Ok(Vec::new());
            };
            let points = crate::rules::win::agenda_value(&card_id, registry).unwrap_or(0);
            // The tags are the price, and the offer was filtered so they
            // are there — but check anyway: a parked selection resolves
            // later than it was built, and paying half a cost is worse
            // than doing nothing.
            if state.runner.tags < points {
                return Ok(Vec::new());
            }
            state.runner.scored_agendas.remove(position);
            state.runner.resources.agenda_points =
                crate::rules::state::AgendaPoints(state.runner.resources.agenda_points.0.saturating_sub(points));
            let mut events = evaluate_effect(state, &Effect::RemoveTags(points), ctx, registry)?;
            // A fresh remote: see the variant's doc comment for why the
            // Corp is not asked where.
            let existing = crate::rules::legal_actions::existing_remote_ids(state);
            let server = crate::rules::run::ServerId::Remote(crate::rules::legal_actions::fresh_remote_id(&existing));
            events.extend(crate::rules::engine::place_corp_card(
                state,
                registry,
                card_id,
                server,
                crate::rules::state::InstallSlot::Root,
                false,
                0,
            )?);
            Ok(events)
        }

        Effect::MoveRunToOutermost(server) => {
            crate::rules::run::move_run_to_outermost(state, registry, *server)
        }

        Effect::SwapApproachedIceWithCard { origin } => {
            let card_id = acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();
            crate::rules::run::swap_approached_ice_with_card(state, registry, &card_id, origin)
        }

        Effect::DealDamageAmount(damage_type, amount) => {
            let resolved = resolve_amount(amount, ctx, state, registry);
            evaluate_effect(state, &Effect::DealDamage(*damage_type, resolved as usize), ctx, registry)
        }

        Effect::AddAdditionalAccessAmount { server, amount } => {
            let resolved = resolve_amount(amount, ctx, state, registry);
            evaluate_effect(state, &Effect::AddAdditionalAccess { server: *server, count: resolved }, ctx, registry)
        }

        Effect::BoostStrengthAmount { amount, duration } => {
            let resolved = resolve_amount(amount, ctx, state, registry);
            evaluate_effect(state, &Effect::BoostStrength { amount: resolved, duration: *duration }, ctx, registry)
        }
    }
}

/// The engine-level "you are encountering ICE right now" guard, shared by
/// the effects that only make sense mid-encounter.
///
/// The backstop half of a deliberate two-level split: `EffectRequirement::
/// DuringEncounter` on an icebreaker's `AbilityDef` gates whether the
/// ability is *offered* (soft, silent, and readable without evaluating
/// anything — which is what `paid_ability::has_usable_paid_ability` needs),
/// while this gates whether the effect can actually *resolve*, however it
/// was reached. `Effect::BreakSubroutines` and `ModifyStrength` already
/// enforced this inline; `BoostStrength` did not, which is why Cleaver's
/// "+1 strength" was a legal action on the Corp's turn.
fn require_encounter(state: &GameState) -> Result<(), RulesError> {
    let run = state.active_run.as_ref().ok_or(RulesError::NoActiveRun)?;
    if run.phase != RunPhase::EncounterIce {
        return Err(RulesError::NotInEncounter);
    }
    Ok(())
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
        // Stop on a finished game, or when a subroutine we just fired parked
        // a Trace or a PendingPrevention — either spans future
        // PlayerActions, so the next pending subroutine must not fire
        // underneath it. `rules::trace::submit_runner_bid`/`paid_ability::
        // close_window`'s `Prevention` arm call this function again once
        // resolved, resuming the loop.
        if state.resolution_halted() {
            break;
        }

        // Immutable read only — ends before any mutation below, so it
        // never overlaps with the `&mut state` passed to transition_subroutine/evaluate_effect.
        let Some((index, install)) = state.active_run.as_ref().and_then(|run| {
            // A subroutine fired above may have removed this ICE from play
            // (or `run::reconcile_ice` moved the run for another reason):
            // the run then stands on the *next* ICE, in `ApproachIce`, with
            // all of its subroutines `Pending`. Firing those here would be
            // wrong twice over — and `transition_subroutine` would refuse
            // with `NotInEncounter`, failing the `PassPriority` that got
            // here and leaving the priority holder no legal action.
            if run.phase != RunPhase::EncounterIce {
                return None;
            }
            let ice = run.ice.get(run.position)?;
            let index = ice.subroutines.iter().position(|s| s.status == SubroutineStatus::Pending)?;
            Some((index, ice.install_id))
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
        // The other site a subroutine resolves at (`run::engine::step_subroutine`
        // is the first) — `EffectRequirement::SubroutineResolvedThisRun`.
        if let Some(run) = state.active_run.as_mut() {
            run.subroutine_resolved = true;
        }
        // The *install* too, not just the card: Mycoweb's "another rezzed
        // code gate" (`CardFilter::NotSourceCard`) has to know which copy
        // is asking, and "this card" lookups inside a subroutine resolve
        // against the encountered ice rather than the first install
        // sharing its name — the same per-install exactness the Rules
        // Audit gave activated abilities.
        let fired_events = evaluate_effect(state, &effect, &mut ResolutionContext::for_install(install, &card_id), registry)?;
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
    triggering_event: Option<&GameEvent>,
) -> Result<Vec<GameEvent>, RulesError> {
    let due = DeferredTrigger {
        card: card_id.clone(),
        trigger,
        target: None,
        install: None,
        target_install: None,
        event: triggering_event.cloned(),
        continuation: None,
    };
    fire_card_triggers(state, registry, &due, false)
}

/// The one loop behind `process_card_triggers` and every firing in
/// `dispatcher`. `target` is `Some` when the reacting card's effects act
/// on another card — the one case where "who reacts" and "what the effect
/// acts on" differ: Cookbook's "whenever you install a virus program, you
/// may place 1 virus counter on it" reacts as Cookbook but acts on the
/// just-installed program. The requirement is checked and consumed as
/// `card_id`; only the effects' context changes. With `announce`, a
/// `GameEvent::TriggerFired { card, trigger }` precedes each
/// `TriggeredEffect` that actually fires — after its requirement passed,
/// before its effects — which is the exact record the coverage harness
/// counts (`netrunner_session::Coverage::triggers_fired`). It used to
/// *infer* firings from the event that would have offered the trigger,
/// which could not see a failed requirement or a `still_applies` bail-out.
/// The two plain entry points do not announce: they are what a unit test
/// drives directly, and dozens of them assert exact event vectors that
/// gain nothing from the marker.
pub(crate) fn fire_card_triggers(
    state: &mut GameState,
    registry: &CardRegistry,
    due: &DeferredTrigger,
    announce: bool,
) -> Result<Vec<GameEvent>, RulesError> {
    let card_id = &due.card;
    let trigger = due.trigger;
    let triggering_event = due.event.as_ref();
    let Some(card) = registry.get(card_id) else {
        return Ok(Vec::new());
    };
    let mut events = Vec::new();
    let card_side = card.side;
    for triggered in card.triggers.iter().filter(|t| t.trigger == trigger) {
        // The requirement is checked as the *reacting* card, the effects
        // resolve as the target (the card itself, unless `target` says
        // otherwise) — separate contexts, and one pair per
        // `TriggeredEffect`, so nothing a trigger's own effects accumulate
        // leaks into the next trigger on the same card.
        let owner_ctx = ResolutionContext::for_install_trigger(due.install, Some(card_id), triggering_event);
        if let Some(requirement) = &triggered.requirement
            && check_requirement(state, requirement, card_side, &owner_ctx, registry).is_err()
        {
            // Soft gate (see `TriggeredEffect::requirement`'s doc comment):
            // unmet just means no bonus this time, not an error propagated
            // to the caller — and no per-turn flag is consumed, since it was
            // never available to begin with.
            continue;
        }
        if announce {
            events.push(GameEvent::TriggerFired { card: card_id.clone(), trigger });
        }
        let mut effect_ctx = match &due.target {
            Some(target) => ResolutionContext::for_install_trigger(due.target_install, Some(target), triggering_event),
            None => ResolutionContext::for_install_trigger(due.install, Some(card_id), triggering_event),
        };
        for effect in &triggered.effects {
            events.extend(evaluate_effect(state, effect, &mut effect_ctx, registry)?);
            // A trigger's effect list is a `Sequence` in all but name and
            // stops for the same reasons: a parked decision (the next effect
            // would resolve underneath it) or a finished game. It had no
            // stop condition at all before.
            if state.resolution_halted() {
                break;
            }
        }
        if let Some(requirement) = &triggered.requirement {
            consume_requirement(state, requirement, card_side, &owner_ctx);
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
    ctx: &ResolutionContext<'_>,
) -> Result<Vec<GameEvent>, RulesError> {
    let acting_card = ctx.acting_card;
    state.pending_prevention = Some(PendingPrevention {
        kind: PendingPreventionKind::Damage { damage_type, amount, prevented: 0 },
        source_card: acting_card.cloned(),
        source_install: ctx.acting_install,
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
    ctx: &ResolutionContext<'_>,
) -> Result<Vec<GameEvent>, RulesError> {
    let acting_card = ctx.acting_card;
    let priority = owning_side_of_target(&target, acting_card, registry);
    state.pending_prevention = Some(PendingPrevention {
        kind: PendingPreventionKind::Trash { target: target.clone(), prevented: false },
        source_card: acting_card.cloned(),
        source_install: ctx.acting_install,
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
        // Never reaches a prevention window (see `Effect::TrashCard`'s
        // arm); the host is the Runner's.
        CardTarget::HostedOnThisCard => Side::Runner,
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
    ctx: &ResolutionContext<'_>,
) -> Result<(InstallId, CardId, ServerId), RulesError> {
    match target {
        CardTarget::CorpInstalled { card, server } => {
            let installed = state
                .corp
                .installed
                .iter()
                .find(|c| &c.card == card && c.server == *server)
                .ok_or_else(|| RulesError::CardNotInstalled { card: card.clone() })?;
            Ok((installed.install_id, card.clone(), *server))
        }
        CardTarget::HostIce => {
            // The acting trojan's host, by install — exact even with two
            // copies of the host ICE on the table.
            let host = acting_rig_card(state, ctx).and_then(|c| c.hosted_on_ice).ok_or(RulesError::UnresolvedCardTarget)?;
            let installed = state.find_corp_install(host).ok_or(RulesError::InstallNotFound(host))?;
            Ok((host, installed.card.clone(), installed.server))
        }
        // A `PromptChooseCards::then` over the Corp's installs runs with the
        // chosen install as the acting one — Maglectric Rapid's "derez 1
        // installed Corp card" names it as `ThisCard`, the convention
        // `TrashCard(ThisCard)` already follows.
        CardTarget::ThisCard => {
            let install = ctx.acting_install.ok_or(RulesError::UnresolvedCardTarget)?;
            let installed = state.find_corp_install(install).ok_or(RulesError::UnresolvedCardTarget)?;
            Ok((install, installed.card.clone(), installed.server))
        }
        CardTarget::RunnerRig(_) | CardTarget::TopOfStack { .. } | CardTarget::HostedOnThisCard => {
            Err(RulesError::UnresolvedCardTarget)
        }
    }
}

/// Fires `Trigger::OnDamageDealt` for each `DamageTaken` among `events` —
/// the hook AU Co.: The Gold Standard in Clones counts damage with.
///
/// Called where the damage actually lands rather than from
/// `dispatch_event`'s own table, because `DamageTaken` is produced deep
/// inside `damage::apply_damage` and returned, never dispatched: only
/// `DamageAboutToResolve` (the prevention window) goes through the
/// dispatcher. Nothing fires once the damage has ended the game.
pub(crate) fn dispatch_damage_taken(
    state: &mut GameState,
    registry: &CardRegistry,
    events: &[GameEvent],
) -> Result<Vec<GameEvent>, RulesError> {
    let mut fired = Vec::new();
    for event in events {
        if matches!(event, GameEvent::DamageTaken { .. }) && !state.is_over() {
            fired.extend(dispatcher::dispatch_event(state, registry, event)?);
        }
    }
    Ok(fired)
}

/// `Effect::AddCounters`/`RemoveCounters`'s shared implementation:
/// saturating-applies `delta` (negative to remove) to `acting_card`'s
/// `counters` field, wherever it's currently installed/rigged. Mirrors
/// `trash_this_card`'s "try Corp installed, then Runner rig" search order,
/// but doesn't need `trash_this_card`'s hand/deck arms — counters only ever
/// live on an installed/rigged card, never in a hand or deck zone.
/// Credits gained by a resolving card, as opposed to by a click or a
/// trace payout: pays them, emits `CreditsGained`, and — when there is an
/// acting card to name — `GameEvent::AbilityGainedCredits` with its
/// dispatch, which is what The Zwicky Group: Invisible Hands draws off.
///
/// Every credit-gaining `Effect` goes through here, so a card that gains
/// through a counter (Regolith Mining License) or a formula (Ritual)
/// counts the same as a flat `GainCredits`. The dispatch is re-entrant in
/// principle — a reaction that itself gained credits would come back
/// round — and safe in practice because the one reader is gated
/// `OncePerTurn`; a second reader that gains credits must carry the same
/// gate.
fn gain_credits_from_ability(
    state: &mut GameState,
    registry: &CardRegistry,
    side: Side,
    amount: u32,
    ctx: &ResolutionContext<'_>,
) -> Result<Vec<GameEvent>, RulesError> {
    state.resources_mut(side).credits = state.resources(side).credits.gain(amount);
    let mut events = vec![GameEvent::CreditsGained { side, amount }];
    if let Some(card) = ctx.acting_card {
        let gained = GameEvent::AbilityGainedCredits { side, card: card.clone() };
        events.push(gained.clone());
        events.extend(dispatcher::dispatch_event(state, registry, &gained)?);
    }
    Ok(events)
}

fn modify_counters(
    state: &mut GameState,
    ctx: &ResolutionContext<'_>,
    delta: i64,
) -> Result<Vec<GameEvent>, RulesError> {
    let card_id = ctx.acting_card.ok_or(RulesError::UnresolvedCardTarget)?.clone();

    let counters = if let Some(position) = acting_corp_position(state, ctx) {
        &mut state.corp.installed[position].counters
    } else if let Some(position) = acting_rig_position(state, ctx) {
        &mut state.runner.rig[position].counters
    } else if let Some(position) = acting_scored_position(state, ctx) {
        &mut state.corp.scored_agendas[position].agenda_counters
    } else if acting_is_corp_identity(state, ctx) {
        &mut state.corp.identity_counters
    } else {
        return Err(RulesError::CardNotEligibleForCounters(card_id));
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
    ctx: &ResolutionContext<'_>,
) -> Result<Vec<GameEvent>, RulesError> {
    match target {
        CardTarget::ThisCard => {
            ctx.acting_card.ok_or(RulesError::MissingActingCardContext)?;
            trash_this_card(state, ctx)
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
            let install = state.corp.installed[position].install_id;
            state.corp.installed.remove(position);
            state.corp.archives.push(orient(card.clone(), seen));
            let mut events = vec![GameEvent::CardTrashed { side: Side::Corp, card: card.clone() }];
            events.extend(cascade_trash_hosted_programs(state, install));
            Ok(events)
        }

        // "The ICE I'm hosted on" — resolve `acting_card`'s host, then
        // trash it exactly like `CorpInstalled` (cascade included, since
        // it recurses into that same arm).
        CardTarget::HostIce => {
            let (_, host, server) = resolve_corp_installed_target(state, target, ctx)?;
            trash_card(state, &CardTarget::CorpInstalled { card: host, server }, ctx)
        }

        // Handled by `evaluate_effect`'s `TrashCard` arm before it gets
        // here (it needs the registry to route each card home).
        CardTarget::HostedOnThisCard => Err(RulesError::UnresolvedCardTarget),

        CardTarget::RunnerRig(card) => {
            let position = state
                .runner
                .rig
                .iter()
                .position(|c| &c.card == card)
                .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: card.clone() })?;
            let removed = state.runner.rig.remove(position);
            state.runner.heap.push(removed.card.clone());
            let mut events = vec![GameEvent::CardTrashed { side: Side::Runner, card: card.clone() }];
            events.extend(cascade_trash_hosted_on_rig_card(state, &removed));
            Ok(events)
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
/// piece of ICE from `CorpState::installed` — `trash_card`, `trash_this_card`
/// and `pending_choice::remove_installed_card` (a selection-trash used to
/// skip it, stranding the trojan with a dangling `hosted_on_ice` that
/// Tranquilizer's `DerezCard(HostIce)` then failed on). A no-op (returns no
/// events) if nothing is hosted on `host_card_id`, so this is zero-overhead
/// for the overwhelming majority of Corp cards that never host anything.
///
/// Keyed by the host's `InstallId`, so with two copies of one ICE installed
/// only the trojans on the copy that left go — `hosted_on_ice` used to be a
/// `CardId` and both copies' trojans went together.
/// The rig-side twin of `cascade_trash_hosted_programs`: when the rig card
/// `host` leaves the rig, every card hosted on it
/// (`InstalledRunnerCard::hosted_on_program == Some(host)`) is trashed too
/// — GAMEDRAGON™ Pro goes with the icebreaker it sits on. Called from
/// every site that removes a rig card. A no-op for the overwhelming
/// majority of rig cards, which host nothing.
/// `Effect::TrashCard(CardTarget::HostedOnThisCard)` — Bling's "trash all
/// hosted cards". Each card returns to its owner's discard pile: a Runner
/// card to the heap, a Corp card (a Detente-style host) faceup to
/// Archives, since it sat faceup on the table.
fn trash_hosted_cards(state: &mut GameState, registry: &CardRegistry, ctx: &ResolutionContext<'_>) -> Result<Vec<GameEvent>, RulesError> {
    let position = acting_rig_position(state, ctx).ok_or(RulesError::UnresolvedCardTarget)?;
    let hosted = std::mem::take(&mut state.runner.rig[position].hosted_cards);
    let mut events = Vec::with_capacity(hosted.len());
    for card in hosted {
        let side = registry.get(&card).map_or(Side::Runner, |def| def.side);
        match side {
            Side::Runner => state.runner.heap.push(card.clone()),
            Side::Corp => state.corp.archives.push(ArchivedCard::faceup(card.clone())),
        }
        events.push(GameEvent::CardTrashed { side, card });
    }
    Ok(events)
}

pub(crate) fn cascade_trash_hosted_on_rig_card(state: &mut GameState, removed: &InstalledRunnerCard) -> Vec<GameEvent> {
    let host = removed.install_id;
    let mut events = Vec::new();
    // Cards hosted *uninstalled* on the host (Madani's programs) are
    // trashed with it too — they were in no other zone, and they left the
    // rig inside `removed`, which is why this takes the card and not its id.
    for hosted in &removed.hosted_cards {
        state.runner.heap.push(hosted.clone());
        events.push(GameEvent::CardTrashed { side: Side::Runner, card: hosted.clone() });
    }
    while let Some(position) = state.runner.rig.iter().position(|c| c.hosted_on_program == Some(host)) {
        let removed = state.runner.rig.remove(position);
        state.runner.heap.push(removed.card.clone());
        events.push(GameEvent::CardTrashed { side: Side::Runner, card: removed.card });
    }
    events
}

pub(crate) fn cascade_trash_hosted_programs(state: &mut GameState, host: InstallId) -> Vec<GameEvent> {
    let mut events = Vec::new();
    while let Some(position) = state.runner.rig.iter().position(|c| c.hosted_on_ice == Some(host)) {
        let removed = state.runner.rig.remove(position);
        state.runner.heap.push(removed.card.clone());
        events.push(GameEvent::CardTrashed { side: Side::Runner, card: removed.card });
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

fn trash_this_card(state: &mut GameState, ctx: &ResolutionContext<'_>) -> Result<Vec<GameEvent>, RulesError> {
    let card_id = &ctx.acting_card.ok_or(RulesError::MissingActingCardContext)?.clone();
    if let Some(position) = acting_corp_position(state, ctx) {
        // Same rezzed-or-not rule as `CardTarget::CorpInstalled` above,
        // widened for the access case: a card the Runner is accessing right
        // now has been seen regardless of rez state.
        let seen = state.corp.installed[position].rezzed || runner_is_accessing(state, card_id);
        let install = state.corp.installed[position].install_id;
        state.corp.installed.remove(position);
        state.corp.archives.push(orient(card_id.clone(), seen));
        let mut events = vec![GameEvent::CardTrashed { side: Side::Corp, card: card_id.clone() }];
        events.extend(cascade_trash_hosted_programs(state, install));
        return Ok(events);
    }
    if ctx.acting_install.is_some() && acting_rig_position(state, ctx).is_none() {
        // See the matching guard below the rig arm.
        return Ok(Vec::new());
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
    if let Some(position) = acting_rig_position(state, ctx) {
        let removed = state.runner.rig.remove(position);
        state.runner.heap.push(removed.card.clone());
        let mut events = vec![GameEvent::CardTrashed { side: Side::Runner, card: card_id.clone() }];
        events.extend(cascade_trash_hosted_on_rig_card(state, &removed));
        return Ok(events);
    }
    // An install that has already left play is gone: its hand/deck
    // namesakes are other cards, and trashing one of them would be exactly
    // the sibling-copy aliasing this context exists to end.
    if ctx.acting_install.is_some() {
        return Ok(Vec::new());
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
/// Whether `side` could pay `cost` right now, without paying it.
///
/// A non-mutating mirror of [`pay_cost`]'s preconditions. **The two must
/// agree** — same pairing discipline as `run::check_run_may_begin` and
/// `Effect::PromptChooseServer`'s park-time check. Disagreement here is
/// only wasteful rather than fatal (a `WindowCheckpoint::PostAction`
/// opening with nothing to do still closes on two passes), but it is the
/// kind of drift that gets expensive later, so add new `Cost` variants to
/// both or neither.
///
/// Costs with no resource precondition (`TrashSelf`, `RemoveSelfFromGame`,
/// `TakeTags`, `ClearTags`) are always payable and answer `true`.
pub(crate) fn cost_is_affordable(
    state: &GameState,
    side: Side,
    cost: &Cost,
    ctx: &ResolutionContext<'_>,
) -> bool {
    match cost {
        Cost::Credits(amount) => {
            let bp = state.active_run.as_ref().map_or(0, |run| run.bad_publicity_credits);
            state.resources(side).credits.0 + bp >= *amount
        }
        Cost::Clicks(amount) => state.resources(side).clicks.0 >= *amount,
        Cost::RemoveCounters(amount) => counters_of(state, ctx).is_some_and(|counters| counters >= *amount),
        // Any one alternative being payable is enough — the payer picks.
        Cost::AnyOf(options) => options.iter().any(|option| cost_is_affordable(state, side, option, ctx)),
        // Every part must be payable — read against the same state, which
        // is exact for the shapes in the pool (clicks plus a self-trash
        // draw on different resources).
        Cost::AllOf(parts) => parts.iter().all(|part| cost_is_affordable(state, side, part, ctx)),
        Cost::TrashSelf | Cost::RemoveSelfFromGame | Cost::TakeTags(_) | Cost::ClearTags => true,
        Cost::TrashRandomFromHq(count) => state.corp.hq.len() as u32 >= *count,
    }
}

/// `pay_cost_ctx` for a payer with no install to name — an operation or
/// event being played, an install being paid for. Anything paying *as an
/// installed card* (an activated ability, a parked choice's cost) must use
/// `pay_cost_ctx` with the install, or `Cost::RemoveCounters`/`TrashSelf`
/// act on the first copy of the card.
pub fn pay_cost(
    state: &mut GameState,
    side: Side,
    cost: &Cost,
    acting_card: Option<&CardId>,
) -> Result<Vec<GameEvent>, RulesError> {
    pay_cost_ctx(state, side, cost, &ResolutionContext::for_card(acting_card))
}

pub fn pay_cost_ctx(
    state: &mut GameState,
    side: Side,
    cost: &Cost,
    ctx: &ResolutionContext<'_>,
) -> Result<Vec<GameEvent>, RulesError> {
    let acting_card = ctx.acting_card;
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
            acting_card.ok_or(RulesError::MissingActingCardContext)?;
            trash_this_card(state, ctx)
        }

        Cost::TrashRandomFromHq(count) => {
            if (state.corp.hq.len() as u32) < *count {
                return Err(RulesError::NotEnoughCardsInHq { required: *count, available: state.corp.hq.len() as u32 });
            }
            let mut events = Vec::new();
            for _ in 0..*count {
                let index = (state.next_u64() % state.corp.hq.len() as u64) as usize;
                let card = state.corp.hq.remove(index);
                // Revealed as it is trashed, so it lands faceup.
                state.corp.archives.push(ArchivedCard::faceup(card.clone()));
                events.push(GameEvent::CardTrashed { side: Side::Corp, card });
            }
            Ok(events)
        }

        Cost::RemoveSelfFromGame => {
            let card_id = acting_card.ok_or(RulesError::MissingActingCardContext)?;
            let position =
                acting_corp_position(state, ctx).ok_or_else(|| RulesError::CardNotInstalled { card: card_id.clone() })?;
            state.corp.installed.remove(position);
            // Deliberately not Archives — see `Cost::RemoveSelfFromGame`.
            state.corp.removed_from_game.push(card_id.clone());
            Ok(vec![GameEvent::CardRemovedFromGame { side, card: card_id.clone() }])
        }

        Cost::ClearTags => {
            state.runner.tags = 0;
            Ok(vec![GameEvent::TagsCleared { side }])
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

        // Paid part by part, in authored order. `cost_is_affordable` has
        // already vouched for every part where a probe precedes payment;
        // a direct submission that can pay the first part but not a later
        // one errors there, and `apply_action`'s clone-on-write discards
        // the partial payment.
        Cost::AllOf(parts) => {
            let mut events = Vec::new();
            for part in parts {
                events.extend(pay_cost_ctx(state, side, part, ctx)?);
            }
            Ok(events)
        }

        Cost::RemoveCounters(amount) => {
            let card_id = acting_card.ok_or(RulesError::MissingActingCardContext)?;
            let available = counters_of(state, ctx).unwrap_or(0);
            if available < *amount {
                return Err(RulesError::InsufficientCounters { card: card_id.clone(), required: *amount, available });
            }
            modify_counters(state, ctx, -i64::from(*amount))
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
    ctx: &ResolutionContext<'_>,
    registry: &CardRegistry,
) -> Result<(), RulesError> {
    match requirement {
        EffectRequirement::IsTagged => {
            if !state.runner.is_tagged() {
                return Err(RulesError::RunnerNotTagged);
            }
            Ok(())
        }
        EffectRequirement::CurrentlyAccessingNonAgenda => {
            check_requirement(state, &EffectRequirement::CurrentlyAccessingACard, side, ctx, registry)?;
            let accessing = state.active_run.as_ref().and_then(|run| run.access_state.as_ref()).and_then(|access| {
                match &access.phase {
                    run::AccessPhase::PendingChoice { card_id, .. } => Some(card_id.clone()),
                    _ => None,
                }
            });
            let is_agenda = accessing.and_then(|card| registry.get(&card)).is_some_and(|def| def.card_type == crate::dsl::CardType::Agenda);
            if is_agenda { Err(RulesError::RequirementNotMet) } else { Ok(()) }
        }
        EffectRequirement::SubroutineResolvedThisRun => {
            if state.active_run.as_ref().is_some_and(|run| run.subroutine_resolved) { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::IdentityFlipped => {
            // The asking side's own flip state — `side` is the resolving
            // card's controller, so a Corp identity reads the Corp's.
            let flipped = match side {
                Side::Corp => state.corp.identity_flipped,
                Side::Runner => state.runner.identity_flipped,
            };
            if flipped { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::MemoryFull => {
            if crate::rules::memory::available_memory(state, registry) == 0 { Ok(()) } else { Err(RulesError::RequirementNotMet) }
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
            if used.contains(&OncePerTurnKey { tag: tag.clone(), install: ctx.acting_install }) {
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
        EffectRequirement::ZoneHasAtLeast { zone, count, filter } => {
            let found = match filter {
                Some(filter) => {
                    crate::rules::pending_choice::eligible_positions(state, registry, side, zone, filter, ctx.acting_install).len()
                }
                None => crate::rules::pending_choice::zone_card_ids(state, side, zone, ctx.acting_install).len(),
            };
            if found < *count as usize {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::Not(inner) => match check_requirement(state, inner, side, ctx, registry) {
            Ok(()) => Err(RulesError::RequirementNotMet),
            Err(_) => Ok(()),
        },
        EffectRequirement::And(a, b) => {
            check_requirement(state, a, side, ctx, registry)?;
            check_requirement(state, b, side, ctx, registry)
        }
        EffectRequirement::RezzedDuringRunAgainstThisServer => {
            let own_server = acting_corp_install(state, ctx).map(|c| c.server).ok_or(RulesError::RequirementNotMet)?;
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
            // Read from the resolution in flight, not from `GameState`:
            // *Diviner* asks about the `DealDamage` immediately preceding
            // it in its own `Sequence`. A `ctx` with nothing recorded means
            // no damage was dealt in this resolution, which is correctly
            // "requirement not met" rather than a stale answer from some
            // earlier action's damage.
            let trashed_odd_cost = ctx
                .damage_discarded
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
        EffectRequirement::AccessingArchives => {
            let accessing_archives = state
                .active_run
                .as_ref()
                .and_then(|run| run.access_state.as_ref())
                .is_some_and(|access| access.server == ServerId::Archives);
            if accessing_archives { Ok(()) } else { Err(RulesError::RequirementNotMet) }
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
            let current = counters_of(state, ctx).unwrap_or(0);
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
            let current = counters_of(state, ctx).unwrap_or(0);
            if current < *amount {
                return Err(RulesError::RequirementNotMet);
            }
            Ok(())
        }
        EffectRequirement::EncounteringHostIce => {
            let Some(host) = acting_rig_card(state, ctx).and_then(|c| c.hosted_on_ice) else {
                return Err(RulesError::RequirementNotMet);
            };
            let matches = state.active_run.as_ref().is_some_and(|run| {
                run.phase == RunPhase::EncounterIce && run.ice.get(run.position).is_some_and(|ice| ice.install_id == host)
            });
            if matches { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::PlayedOperationThisTurn => {
            if state.corp.played_operation_this_turn { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::DuringRun => {
            // Not merely `active_run.is_some()`: once the Runner is
            // accessing, or the run has ended but not been cleared, there
            // is nothing left to move.
            let running = state
                .active_run
                .as_ref()
                .is_some_and(|run| !matches!(run.phase, RunPhase::AccessingCard | RunPhase::Ended));
            if running { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::DuringEncounter => {
            let encountering =
                state.active_run.as_ref().is_some_and(|run| run.phase == RunPhase::EncounterIce);
            if encountering { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::AccessedAnyCardDuringLastRun => {
            let accessed = state.last_completed_run.as_ref().is_some_and(|run| run.cards_accessed > 0);
            if accessed { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::ThisCardIsInstalled => {
            // Deliberately off the *install* the dispatch named, never a
            // first-match CardId fallback — see the variant's doc comment.
            let installed = ctx.acting_install.is_some_and(|install| {
                state.find_corp_install(install).is_some() || state.find_rig_install(install).is_some()
            });
            if installed { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::WasFirstAdvancementThisCard => {
            // Answered from the triggering event itself: `CardAdvanced`
            // already carries the running total, and `== 1` *is* "this was
            // the first advancement". The `GameState` field this replaced
            // held nothing the event didn't.
            let was_first = matches!(
                ctx.triggering_event,
                Some(GameEvent::CardAdvanced { advancement_tokens: 1, .. })
            );
            if was_first { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::CorpCreditsAtLeast(amount) => {
            if state.corp.resources.credits.0 >= *amount { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::RunEventActive => {
            let active = state
                .active_run
                .as_ref()
                .and_then(|run| run.initiated_by.as_ref())
                .and_then(|card| registry.get(card))
                .is_some_and(|def| def.card_type == CardType::Event && def.subtypes.contains(&CardSubtype::Run));
            if active { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::InstalledWithoutSpendingCredits => {
            let free = matches!(
                ctx.triggering_event,
                Some(
                    GameEvent::ProgramInstalled { credits_paid: 0, .. }
                        | GameEvent::HardwareInstalled { credits_paid: 0, .. }
                        | GameEvent::ResourceInstalled { credits_paid: 0, .. }
                )
            );
            if free { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::TriggeringCardMatches(filter) => {
            let matches = ctx
                .triggering_event
                .and_then(triggering_card)
                .and_then(|card| registry.get(card))
                .is_some_and(|def| crate::dsl::card_matches_filter(def, filter));
            if matches { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::AmountAtLeast(amount, min) => {
            if resolve_amount(amount, ctx, state, registry) >= *min { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::NoActionTakenThisTurn => {
            if state.actions_taken_this_turn == 0 { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::AgendaCameFromThisCardsServer => {
            // `AgendaScored` names the server; a steal names none, so it
            // comes off the run the steal necessarily happened during.
            let from = match ctx.triggering_event {
                Some(GameEvent::AgendaScored { server, .. }) => Some(*server),
                Some(GameEvent::AgendaStolen { .. }) => state.active_run.as_ref().map(|run| run.server),
                _ => None,
            };
            let here = acting_corp_install(state, ctx).map(|installed| installed.server);
            if from.is_some() && from == here { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::ScoreAreaHasAtLeast(count) => {
            if state.corp.scored_agendas.len() as u32 >= *count { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::CurrentlyAccessingInstalledCard { rezzed_only } => {
            // `AccessState::pending_install` is set when the card being
            // accessed is a root install, and is still set while its
            // `OnAccessed` and `OnTrashedFromAccess` triggers dispatch —
            // which is what lets Aggressive Trendsetting tell "trashed an
            // installed card" from "trashed a card found in HQ".
            let access = state.active_run.as_ref().and_then(|run| run.access_state.as_ref());
            let install = access.and_then(|access| access.pending_install);
            // Rezzed-ness comes off the access, not the table: the card may
            // already be in Archives by the time the question is asked
            // (Public Access Plaza, from `OnTrashedFromAccess`).
            let ok = match install {
                None => false,
                Some(_) if *rezzed_only => access.is_some_and(|access| access.pending_install_rezzed),
                Some(_) => true,
            };
            if ok { Ok(()) } else { Err(RulesError::RequirementNotMet) }
        }
        EffectRequirement::PlayedFromArchives => {
            if matches!(ctx.triggering_event, Some(GameEvent::OperationPlayed { from_archives: true, .. })) {
                Ok(())
            } else {
                Err(RulesError::RequirementNotMet)
            }
        }
    }
}

/// The card a triggering event is *about*, for
/// `EffectRequirement::TriggeringCardMatches`. Only the events a Runner-side
/// reaction can currently be keyed off are listed; an event with no single
/// subject card answers `None`, which fails the requirement.
fn triggering_card(event: &GameEvent) -> Option<&CardId> {
    match event {
        GameEvent::IceRezzed { card, .. }
        | GameEvent::CardInstalled { card, .. }
        | GameEvent::ProgramInstalled { card, .. }
        | GameEvent::HardwareInstalled { card, .. }
        | GameEvent::ResourceInstalled { card, .. }
        | GameEvent::EventPlayed { card, .. }
        | GameEvent::OperationPlayed { card, .. }
        | GameEvent::CardTrashed { card, .. }
        | GameEvent::CardDerezzed { card }
        // The card whose ability paid out — The Zwicky Group asks whether
        // it was an agenda or an operation.
        | GameEvent::AbilityGainedCredits { card, .. } => Some(card),
        _ => None,
    }
}

/// `acting_card`'s current generic counter total, wherever it's currently
/// installed/rigged — `None` if it's neither (already trashed, or never
/// resolvable), mirroring `modify_counters`'s "try Corp installed, then
/// Runner rig" search order but read-only.
fn counters_of(state: &GameState, ctx: &ResolutionContext<'_>) -> Option<u32> {
    acting_corp_install(state, ctx)
        .map(|c| c.counters)
        .or_else(|| acting_rig_card(state, ctx).map(|c| c.counters))
        .or_else(|| acting_scored_position(state, ctx).map(|position| state.corp.scored_agendas[position].agenda_counters))
        .or_else(|| acting_is_corp_identity(state, ctx).then_some(state.corp.identity_counters))
}

/// `acting_card`'s current advancement token total, if it's a Corp
/// installed card — `None` otherwise (a Runner rig card, or already
/// trashed). Read-only counterpart to `advance_card`'s mutation.
fn advancement_tokens_of(state: &GameState, ctx: &ResolutionContext<'_>) -> Option<u32> {
    acting_corp_install(state, ctx).map(|c| c.advancement_tokens)
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
            // Rising Tide: one per fracter in the heap, live.
            StrengthModifier::PerFracterInHeap(per) => {
                per * state
                    .runner
                    .heap
                    .iter()
                    .filter(|id| registry.get(id).is_some_and(|def| def.subtypes.contains(&crate::dsl::CardSubtype::Fracter)))
                    .count() as i32
            }
            // The three Corp-ICE modifiers never apply to a rig card.
            StrengthModifier::WhileProtectingRemote(_)
            | StrengthModifier::WhileOnlyIceProtectingServer(_)
            | StrengthModifier::WhileHostedAdvancementsAtLeast { .. }
            | StrengthModifier::PerHostedAdvancement(_) => 0,
        })
        .unwrap_or(0);
    let hosted: i32 = hosted_breaker_bonuses(state, registry, card.install_id).map(|bonus| bonus.strength).sum();
    card.effective_strength() + bonus + hosted
}

/// The `HostedBreakerBonus` of every rig card hosted on `host`
/// (`InstalledRunnerCard::hosted_on_program == Some(host)`) — GAMEDRAGON™
/// Pro on an icebreaker. Live, like `StrengthModifier`: read at every
/// strength query and at every boost, never baked into the host.
fn hosted_breaker_bonuses<'s>(
    state: &'s GameState,
    registry: &'s CardRegistry,
    host: InstallId,
) -> impl Iterator<Item = crate::dsl::HostedBreakerBonus> + 's {
    state
        .runner
        .rig
        .iter()
        .filter(move |card| card.hosted_on_program == Some(host))
        .filter_map(move |card| registry.get(&card.card).and_then(|def| def.hosted_breaker_bonus))
}

/// Whether a Trojan hosted on the ICE install `ice` grants it `subtype`
/// (`CardDefinition::host_ice_gains_subtypes` — Chromatophores). The ICE's
/// effective subtypes are its printed `IceType` plus these grants; only
/// `Effect::BreakSubroutines`' `restrict_to` check consults them today.
fn ice_gains_subtype_from_hosted(state: &GameState, registry: &CardRegistry, ice: InstallId, subtype: IceType) -> bool {
    state
        .runner
        .rig
        .iter()
        .filter(|card| card.hosted_on_ice == Some(ice))
        .filter_map(|card| registry.get(&card.card))
        .any(|def| def.host_ice_gains_subtypes.contains(&subtype))
}

/// Drains up to `amount` credits from the hosted-credit pools whose card
/// `usable` accepts, rig order, trashing a pool that empties when its card
/// says so (`CardDefinition::trash_when_empty` — Open Market). Returns the
/// events and how much was drained; the caller pays the remainder from the
/// wallet. The one drain every purpose-restricted pool goes through — see
/// `CardDefinition::hosted_credits_usable_for` for why it is not in
/// `pay_cost`.
pub(crate) fn drain_hosted_credit_pools(
    state: &mut GameState,
    registry: &CardRegistry,
    amount: u32,
    usable: impl Fn(&crate::dsl::CardDefinition) -> bool,
) -> Result<(Vec<GameEvent>, u32), RulesError> {
    let pools: Vec<(InstallId, u32, bool)> = state
        .runner
        .rig
        .iter()
        .filter(|card| card.counters > 0)
        .filter_map(|card| registry.get(&card.card).map(|def| (card, def)))
        .filter(|(_, def)| usable(def))
        .map(|(card, def)| (card.install_id, card.counters, def.trash_when_empty))
        .collect();
    let mut events = Vec::new();
    let mut remaining = amount;
    for (install, credits, trash_when_empty) in pools {
        if remaining == 0 {
            break;
        }
        let spend = credits.min(remaining);
        let ctx = ResolutionContext::for_parked(Some(install), None);
        events.extend(spend_hosted_credits(state, &ctx, spend)?);
        remaining -= spend;
        if trash_when_empty && spend == credits {
            let card_id = state.runner.rig.iter().find(|c| c.install_id == install).map(|c| c.card.clone());
            if let Some(card_id) = card_id {
                let ctx = ResolutionContext::for_parked(Some(install), Some(&card_id));
                events.extend(trash_this_card(state, &ctx)?);
            }
        }
    }
    Ok((events, amount - remaining))
}

/// `drain_hosted_credit_pools` for the Corp's table — Mahkota Langit
/// Grid's rez credits, drained by `engine::rez_ice`. `usable` sees the
/// install as well as the definition, because a Corp pool's purpose is
/// tied to *where* the card sits (its own server). Rezzed installs only:
/// an unrezzed upgrade's text is not active. Table order, like the rig.
pub(crate) fn drain_corp_hosted_credit_pools(
    state: &mut GameState,
    registry: &CardRegistry,
    amount: u32,
    usable: impl Fn(&InstalledCard, &crate::dsl::CardDefinition) -> bool,
) -> Result<(Vec<GameEvent>, u32), RulesError> {
    let pools: Vec<(InstallId, u32)> = state
        .corp
        .installed
        .iter()
        .filter(|card| card.rezzed && card.counters > 0)
        .filter(|card| registry.get(&card.card).is_some_and(|def| usable(card, def)))
        .map(|card| (card.install_id, card.counters))
        .collect();
    let mut events = Vec::new();
    let mut remaining = amount;
    for (install, credits) in pools {
        if remaining == 0 {
            break;
        }
        let spend = credits.min(remaining);
        let card_id = state.corp.installed.iter().find(|c| c.install_id == install).map(|c| c.card.clone());
        let ctx = ResolutionContext::for_parked(Some(install), card_id.as_ref());
        events.extend(modify_counters(state, &ctx, -i64::from(spend))?);
        remaining -= spend;
    }
    Ok((events, amount - remaining))
}

/// Whether `breaker` (its definition, if it has one — a click has none)
/// may break `subroutine` under `SubroutineDef::only_breakable_by`.
fn subroutine_breakable_by(subroutine: &crate::rules::run::EncounteredSubroutine, breaker: Option<&crate::dsl::CardDefinition>) -> bool {
    match subroutine.definition.only_breakable_by {
        None => true,
        Some(subtype) => breaker.is_some_and(|def| def.subtypes.contains(&subtype)),
    }
}

/// Spends `amount` of the acting rig card's hosted credits (its generic
/// counters) towards a cost the card's text lets them pay — the
/// purpose-restricted pool drain `run::access::resolve_trash` uses for
/// `HostedCreditUse::TrashCosts`. The counters go through the same path
/// `Cost::RemoveCounters` uses, so the event stream reads the same.
pub(crate) fn spend_hosted_credits(
    state: &mut GameState,
    ctx: &ResolutionContext<'_>,
    amount: u32,
) -> Result<Vec<GameEvent>, RulesError> {
    if amount == 0 {
        return Ok(Vec::new());
    }
    let position = acting_rig_position(state, ctx).ok_or(RulesError::MissingActingCardContext)?;
    let card_id = state.runner.rig[position].card.clone();
    let ctx = ResolutionContext::for_parked(Some(state.runner.rig[position].install_id), Some(&card_id));
    modify_counters(state, &ctx, -i64::from(amount))
}

pub(crate) fn resolve_amount(amount: &Amount, ctx: &ResolutionContext<'_>, state: &GameState, registry: &CardRegistry) -> u32 {
    match amount {
        Amount::ClicksRemaining => match state.phase {
            crate::rules::GamePhase::Action(side) => state.resources(side).clicks.0,
            _ => 0,
        },
        Amount::PrintedInstallCost => ctx.acting_card.and_then(|card| registry.get(card)).map_or(0, |def| def.cost),
        Amount::RemainingAfterSelection(total) => total.saturating_sub(ctx.selected_count),
        Amount::Fixed(n) => *n,
        Amount::AgendaPointsScoredThisTurn => state.corp.agenda_points_scored_this_turn,
        Amount::HostedCounters => counters_of(state, ctx).unwrap_or(0),
        Amount::HostedAdvancementTokens => advancement_tokens_of(state, ctx).unwrap_or(0),
        Amount::InstalledIcebreakerCount => installed_icebreaker_count(state, registry),
        Amount::FacedownCardsInArchives => state.corp.archives.iter().filter(|a| a.facedown).count() as u32,
        Amount::CreditsLostThisResolution => ctx.credits_lost,
        // The greater of the two scores, in agenda points — Null Signal
        // Games' *Elevation* threat-level rule. Read off the score areas
        // through the registry, the same way `win::check_for_winner` does,
        // rather than a running counter that a forfeit would have to
        // decrement.
        Amount::RunnerTags => state.runner.tags,
        Amount::ThreatLevel => {
            let corp: u32 = state.corp.scored_agendas.iter().filter_map(|s| crate::rules::win::agenda_value(&s.card, registry)).sum();
            let runner: u32 = state.runner.scored_agendas.iter().filter_map(|c| crate::rules::win::agenda_value(c, registry)).sum();
            corp.max(runner)
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
pub(crate) fn consume_requirement(
    state: &mut GameState,
    requirement: &EffectRequirement,
    side: Side,
    ctx: &ResolutionContext<'_>,
) {
    match requirement {
        EffectRequirement::IsTagged => {}
        EffectRequirement::FirstInstallThisTurn => state.corp.first_install_used_this_turn = true,
        EffectRequirement::FirstSuccessfulHqRunThisTurn => state.runner.first_hq_run_used_this_turn = true,
        EffectRequirement::OncePerTurn(tag) => {
            let used = match side {
                Side::Corp => &mut state.corp.once_per_turn_used,
                Side::Runner => &mut state.runner.once_per_turn_used,
            };
            used.insert(OncePerTurnKey { tag: tag.clone(), install: ctx.acting_install });
        }
        EffectRequirement::And(a, b) => {
            consume_requirement(state, a, side, ctx);
            consume_requirement(state, b, side, ctx);
        }
        EffectRequirement::RunnerCreditsAtMost(_)
        | EffectRequirement::IdentityFlipped
        | EffectRequirement::CurrentlyAccessingNonAgenda
        | EffectRequirement::CurrentlyAccessingInstalledCard { .. }
        | EffectRequirement::ScoreAreaHasAtLeast(_)
        | EffectRequirement::AgendaCameFromThisCardsServer
        | EffectRequirement::SubroutineResolvedThisRun
        | EffectRequirement::MemoryFull
        | EffectRequirement::RunnerClicksAtLeast(_)
        | EffectRequirement::ZoneHasAtLeast { .. }
        | EffectRequirement::Not(_)
        | EffectRequirement::RezzedDuringRunAgainstThisServer
        | EffectRequirement::RunnerMadeSuccessfulRunLastTurn
        | EffectRequirement::LastDamageTrashedOddCostCard
        | EffectRequirement::LastRunWasOnHqOrRnD
        | EffectRequirement::StoleAgendaDuringLastRun
        | EffectRequirement::ArchivesHasFacedownCard
        | EffectRequirement::AccessingArchives
        | EffectRequirement::AccessedAnyCardDuringLastRun
        | EffectRequirement::ThisCardIsInstalled
        | EffectRequirement::MadeSuccessfulRunThisTurn
        | EffectRequirement::ThisCardCountersAtMost(_)
        | EffectRequirement::CurrentlyAccessingACard
        | EffectRequirement::ThisCardCountersAtLeast(_)
        | EffectRequirement::EncounteringHostIce
        | EffectRequirement::DuringEncounter
        | EffectRequirement::DuringRun
        | EffectRequirement::PlayedOperationThisTurn
        | EffectRequirement::WasFirstAdvancementThisCard
        | EffectRequirement::CorpCreditsAtLeast(_)
        | EffectRequirement::RunEventActive
        | EffectRequirement::InstalledWithoutSpendingCredits
        | EffectRequirement::TriggeringCardMatches(_)
        | EffectRequirement::AmountAtLeast(..)
        | EffectRequirement::NoActionTakenThisTurn
        | EffectRequirement::PlayedFromArchives => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::state::CompletedRun;
    use crate::rules::state::InstallId;
    use crate::rules::test_support::fixture_install_id;
    use crate::dsl::{AbilityDef, CardDefinition, CardId, CardType, DamageType, IceType, SubroutineDef, TriggeredEffect};
    use crate::rules::run::{EncounteredSubroutine, RunIce, RunPhase as RP, RunState, ServerId, SubroutineStatus};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, GamePhase, InstalledCard, InstalledRunnerCard,
        MemoryUnits, PlayerResources, RunnerState,
    };

    fn installed_runner_card(id: &str, base_strength: i32) -> InstalledRunnerCard {
        InstalledRunnerCard {
            install_id: fixture_install_id(id),
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
            ..Default::default()
        }
    }

    #[test]
    fn gain_credits_targets_the_named_side() {
        let mut state = game_state();
        let events = evaluate_effect(&mut state, &Effect::GainCredits(Side::Corp, 3), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(8));
        assert_eq!(state.runner.resources.credits, Credits(5));
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Corp, amount: 3 }]);
    }

    #[test]
    fn deal_damage_delegates_to_apply_damage() {
        let mut state = game_state();
        state.runner.grip = vec![CardId("card_0".to_string()), CardId("card_1".to_string())];

        let events = evaluate_effect(&mut state, &Effect::DealDamage(DamageType::Net, 1), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

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
            abilities: vec![AbilityDef { trigger: Trigger::Paid, cost: None, requirement: None, effect, cost_discount_if: None, used_by: None }],
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

        let events = evaluate_effect(&mut state, &Effect::DealDamage(DamageType::Net, 1), &mut ResolutionContext::for_card(None), &registry).unwrap();

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

        evaluate_effect(&mut state, &Effect::DealDamage(DamageType::Net, 2), &mut ResolutionContext::for_card(None), &registry).unwrap();

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
            source_install: None,
            resume: PreventionResume::None,
        });

        evaluate_effect(&mut state, &Effect::PreventDamage(1), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(
            state.pending_prevention.map(|p| p.kind),
            Some(PendingPreventionKind::Damage { damage_type: DamageType::Net, amount: 3, prevented: 1 })
        );
    }

    #[test]
    fn prevent_damage_with_no_pending_prevention_errors() {
        let mut state = game_state();
        let result = evaluate_effect(&mut state, &Effect::PreventDamage(1), &mut ResolutionContext::for_card(None), &CardRegistry::new());
        assert_eq!(result, Err(RulesError::NoPendingPrevention));
    }

    #[test]
    fn prevent_damage_against_a_pending_trash_errors_prevention_kind_mismatch() {
        let mut state = game_state();
        state.pending_prevention = Some(PendingPrevention {
            kind: PendingPreventionKind::Trash { target: CardTarget::RunnerRig(CardId("corroder".to_string())), prevented: false },
            source_card: None,
            source_install: None,
            resume: PreventionResume::None,
        });

        let result = evaluate_effect(&mut state, &Effect::PreventDamage(1), &mut ResolutionContext::for_card(None), &CardRegistry::new());

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
        evaluate_effect(&mut state, &Effect::TrashCard(target.clone()), &mut ResolutionContext::for_card(None), &registry).unwrap();

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
            source_install: None,
            resume: PreventionResume::None,
        });

        evaluate_effect(&mut state, &Effect::PreventTrash, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.pending_prevention.map(|p| p.kind), Some(PendingPreventionKind::Trash { target, prevented: true }));
    }

    #[test]
    fn draw_cards_stops_silently_on_an_empty_deck() {
        let mut state = game_state();
        state.runner.stack = vec![CardId("only_card".to_string())];

        let events = evaluate_effect(&mut state, &Effect::DrawCards(Side::Runner, 3), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

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

        let events = evaluate_effect(&mut state, &Effect::EndTheRun, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert!(state.active_run.is_none());
        assert_eq!(events, vec![GameEvent::RunEndedByEffect { server: ServerId::Hq }]);
    }

    /// Ending a run that is already over does nothing rather than
    /// erroring: this used to be `Err(NoActiveRun)`, which made *Biawak*'s
    /// second subroutine fail the whole action after its first had ended
    /// the run — and, when that action was a parked selection's
    /// confirmation, left a decision nothing could resolve (the view-path
    /// sweep's seed-19 stall).
    #[test]
    fn end_the_run_with_no_active_run_does_nothing() {
        let mut state = game_state();
        assert_eq!(evaluate_effect(&mut state, &Effect::EndTheRun, &mut ResolutionContext::for_card(None), &CardRegistry::new()), Ok(Vec::new()));
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
            evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 }, &mut ResolutionContext::for_card(None), &CardRegistry::new())
                .unwrap();
        assert_eq!(state.active_run.as_ref().unwrap().additional_hq_access, 1);
        assert_eq!(events, vec![GameEvent::AdditionalAccessGranted { server: ServerId::Hq, count: 1 }]);

        let events =
            evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::RnD, count: 2 }, &mut ResolutionContext::for_card(None), &CardRegistry::new())
                .unwrap();
        assert_eq!(state.active_run.as_ref().unwrap().additional_rd_access, 2);
        assert_eq!(events, vec![GameEvent::AdditionalAccessGranted { server: ServerId::RnD, count: 2 }]);
    }

    #[test]
    fn add_additional_access_stacks_additively() {
        let mut state = game_state();
        state.active_run = Some(active_run_state());

        evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 }, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();
        evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 }, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.active_run.as_ref().unwrap().additional_hq_access, 2);
    }

    #[test]
    fn add_additional_access_no_ops_for_archives_and_remote() {
        let mut state = game_state();
        state.active_run = Some(active_run_state());

        let archives = evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Archives, count: 3 }, &mut ResolutionContext::for_card(None), &CardRegistry::new())
            .unwrap();
        let remote = evaluate_effect(
            &mut state,
            &Effect::AddAdditionalAccess { server: ServerId::Remote(0), count: 3 }, &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
        .unwrap();

        let run = state.active_run.as_ref().unwrap();
        assert_eq!(run.additional_hq_access, 0);
        assert_eq!(run.additional_rd_access, 0);
        // A breach of either server accesses everything there already, so
        // the grant changes nothing — and records nothing. It used to emit
        // `AdditionalAccessGranted` for a no-op.
        assert!(archives.is_empty() && remote.is_empty(), "{archives:?} {remote:?}");
    }

    #[test]
    fn add_additional_access_without_an_active_run_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::AddAdditionalAccess { server: ServerId::Hq, count: 1 }, &mut ResolutionContext::for_card(None), &CardRegistry::new()),
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
            &Effect::SetAccessReplacement { server: ServerId::Hq, effect: Box::new(replacement.clone()), optional: false }, &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
        .unwrap();

        assert_eq!(
            state.active_run.as_ref().unwrap().access_replacement,
            Some((ServerId::Hq, replacement, false))
        );
        assert_eq!(events, vec![GameEvent::AccessReplacementSet { server: ServerId::Hq }]);
    }

    #[test]
    fn set_access_replacement_without_an_active_run_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::SetAccessReplacement { optional: false,
                    server: ServerId::Hq,
                    effect: Box::new(Effect::GainCredits(Side::Runner, 8)),
                }, &mut ResolutionContext::for_card(None),
                &CardRegistry::new()),
            Err(RulesError::NoActiveRun)
        );
    }

    #[test]
    fn give_tags_always_targets_the_runner() {
        let mut state = game_state();
        let events = evaluate_effect(&mut state, &Effect::GiveTags(2), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.tags, 2);
        assert_eq!(events, vec![GameEvent::TagsGiven { side: Side::Runner, amount: 2 }]);
    }

    #[test]
    fn remove_tags_saturates_at_zero() {
        let mut state = game_state();
        state.runner.tags = 1;
        let events = evaluate_effect(&mut state, &Effect::RemoveTags(5), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.tags, 0);
        // The event reports the tag that actually came off, not the five
        // asked for — Synapse Global: Faster than Thought reacts to a
        // removal, and must not react to a request that removed nothing.
        assert_eq!(events, vec![GameEvent::TagsRemoved { side: Side::Runner, amount: 1 }]);
    }

    #[test]
    fn give_bad_publicity_increases_the_counter() {
        let mut state = game_state();
        let events = evaluate_effect(&mut state, &Effect::GiveBadPublicity(2), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.bad_publicity, 2);
        assert_eq!(events, vec![GameEvent::BadPublicityGiven { amount: 2 }]);
    }

    #[test]
    fn remove_bad_publicity_saturates_at_zero() {
        let mut state = game_state();
        state.corp.bad_publicity = 1;
        let events = evaluate_effect(&mut state, &Effect::RemoveBadPublicity(5), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.bad_publicity, 0);
        assert_eq!(events, vec![GameEvent::BadPublicityRemoved { amount: 5 }]);
    }

    #[test]
    fn is_tagged_requirement_fails_with_zero_tags_and_succeeds_with_a_tag() {
        let mut state = game_state();
        assert_eq!(
            check_requirement(&state, &EffectRequirement::IsTagged, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::RunnerNotTagged)
        );

        state.runner.tags = 1;
        assert_eq!(check_requirement(&state, &EffectRequirement::IsTagged, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn once_per_turn_requirement_fires_once_then_is_silently_skipped_on_a_second_attempt() {
        let mut state = game_state();
        let requirement = EffectRequirement::OncePerTurn("test_tag".to_string());

        assert_eq!(check_requirement(&state, &requirement, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()), Ok(()));
        consume_requirement(&mut state, &requirement, Side::Runner, &ResolutionContext::default());

        assert_eq!(
            check_requirement(&state, &requirement, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );
        // The Corp's own set is untouched — OncePerTurn is per-side.
        assert_eq!(check_requirement(&state, &requirement, Side::Corp, &ResolutionContext::for_card(None), &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn once_per_turn_requirement_resets_at_the_next_turn_start() {
        let mut state = game_state();
        state.phase = GamePhase::Action(Side::Runner);
        let requirement = EffectRequirement::OncePerTurn("docklands_pass".to_string());
        state.runner.once_per_turn_used.insert(OncePerTurnKey { tag: "docklands_pass".to_string(), install: None });
        assert_eq!(check_requirement(&state, &requirement, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()), Err(RulesError::RequirementNotMet));

        crate::rules::turn::enter_start_of_turn(&mut state, &mut Vec::new(), Side::Runner, &CardRegistry::new()).unwrap();

        assert_eq!(check_requirement(&state, &requirement, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn zone_has_at_least_requirement_counts_the_acting_sides_zone() {
        let mut state = game_state();
        state.corp.hq = vec![CardId("a".to_string())];
        let requirement = EffectRequirement::ZoneHasAtLeast { zone: crate::dsl::CardZoneRef::OwnHq, count: 2, filter: None };
        assert_eq!(
            check_requirement(&state, &requirement, Side::Corp, &ResolutionContext::default(), &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );
        state.corp.hq.push(CardId("b".to_string()));
        assert_eq!(check_requirement(&state, &requirement, Side::Corp, &ResolutionContext::default(), &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn runner_credits_at_most_requirement() {
        let mut state = game_state();
        state.runner.resources.credits = Credits(7);
        assert_eq!(
            check_requirement(&state, &EffectRequirement::RunnerCreditsAtMost(6), Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );

        state.runner.resources.credits = Credits(6);
        assert_eq!(check_requirement(&state, &EffectRequirement::RunnerCreditsAtMost(6), Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()), Ok(()));
    }

    #[test]
    fn not_requirement_inverts_the_inner_result() {
        let state = game_state();
        assert_eq!(
            check_requirement(&state, &EffectRequirement::Not(Box::new(EffectRequirement::IsTagged)), Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Ok(())
        );

        let mut tagged = game_state();
        tagged.runner.tags = 1;
        assert_eq!(
            check_requirement(&tagged, &EffectRequirement::Not(Box::new(EffectRequirement::IsTagged)), Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
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
            install_id: crate::rules::InstallId::PLACEHOLDER,
            card_id: CardId(card_id.to_string()),
            current_strength: strength,
            ice_type,
            subroutines: (0..subroutine_count)
                .map(|id| EncounteredSubroutine {
                    id,
                    definition: SubroutineDef {
                        text: format!("Subroutine {id}"),
                        effect: Effect::EndTheRun,
                        only_breakable_by: None,
                    },
                    status: SubroutineStatus::Pending,
                })
                .collect(),
            rezzed,
        }
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
                GameEvent::AbilityGainedCredits { side: Side::Corp, card: CardId("ice_wall".to_string()) },
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

        let events = evaluate_effect(&mut state, &Effect::ModifyStrength(2), &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

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
            evaluate_effect(&mut state, &Effect::ModifyStrength(2), &mut ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::NotInEncounter)
        );
    }

    #[test]
    fn modify_strength_with_no_active_run_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::ModifyStrength(2), &mut ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::NoActiveRun)
        );
    }

    #[test]
    fn trash_card_this_card_without_acting_card_is_rejected_not_panicked() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::TrashCard(CardTarget::ThisCard), &mut ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::MissingActingCardContext)
        );
    }

    #[test]
    fn trash_card_this_card_with_acting_card_moves_it_to_the_heap() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("gordian_blade", 2)];
        let acting = CardId("gordian_blade".to_string());

        let events =
            evaluate_effect(&mut state, &Effect::TrashCard(CardTarget::ThisCard), &mut ResolutionContext::for_card(Some(&acting)), &CardRegistry::new()).unwrap();

        assert!(state.runner.rig.is_empty());
        assert_eq!(state.runner.heap, vec![acting.clone()]);
        assert_eq!(events, vec![GameEvent::CardTrashed { side: Side::Runner, card: acting }]);
    }

    #[test]
    fn trash_card_corp_installed_moves_card_to_archives() {
        let mut state = game_state();
        state.corp.installed.push(InstalledCard {
            install_id: InstallId(1082),
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
            }), &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
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
            evaluate_effect(&mut state, &Effect::TrashCard(CardTarget::RunnerRig(CardId("gordian_blade".to_string()))), &mut ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::CardNotInRig { side: Side::Runner, card: CardId("gordian_blade".to_string()) })
        );
    }

    #[test]
    fn trash_card_runner_rig_moves_card_to_heap() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("gordian_blade", 2)];

        let events = evaluate_effect(
            &mut state,
            &Effect::TrashCard(CardTarget::RunnerRig(CardId("gordian_blade".to_string()))), &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
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
            &Effect::TrashCard(CardTarget::TopOfStack { side: Side::Corp, zone: StackZone::RAndD }), &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
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
                &Effect::TrashCard(CardTarget::TopOfStack { side: Side::Corp, zone: StackZone::Stack }), &mut ResolutionContext::for_card(None),
                &CardRegistry::new()),
            Err(RulesError::EmptyZone { side: Side::Corp, zone: StackZone::Stack })
        );
    }

    #[test]
    fn trash_card_top_of_stack_runner_mills_from_the_stack() {
        let mut state = game_state();
        state.runner.stack = vec![CardId("clone_chip".to_string()), CardId("sure_gamble".to_string())];

        let events = evaluate_effect(
            &mut state,
            &Effect::TrashCard(CardTarget::TopOfStack { side: Side::Runner, zone: StackZone::Stack }), &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
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
                &Effect::TrashCard(CardTarget::TopOfStack { side: Side::Corp, zone: StackZone::RAndD }), &mut ResolutionContext::for_card(None),
                &CardRegistry::new()),
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
    fn pay_clear_tags_zeroes_the_counter() {
        let mut state = game_state();
        state.runner.tags = 3;

        let events = pay_cost(&mut state, Side::Runner, &Cost::ClearTags, None).unwrap();

        assert_eq!(state.runner.tags, 0);
        assert_eq!(events, vec![GameEvent::TagsCleared { side: Side::Runner }]);
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
            None,
        )
        .unwrap();

        assert_eq!(state.runner.tags, 1);
        assert_eq!(state.corp.resources.credits, Credits(7));
        assert_eq!(
            events,
            vec![
                GameEvent::TagsGiven { side: Side::Runner, amount: 1 },
                GameEvent::CreditsGained { side: Side::Corp, amount: 2 },
                GameEvent::AbilityGainedCredits { side: Side::Corp, card: CardId("snare".to_string()) },
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
            None,
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
            None,
        )
        .unwrap();

        assert!(events.is_empty());
    }

    #[test]
    fn add_counters_on_a_runner_rig_card_increments_its_counters() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("gorman_drip", 0)];
        let acting = CardId("gorman_drip".to_string());

        let events = evaluate_effect(&mut state, &Effect::AddCounters(2), &mut ResolutionContext::for_card(Some(&acting)), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.rig[0].counters, 2);
        assert_eq!(events, vec![GameEvent::CountersAdded { card: acting, amount: 2 }]);
    }

    #[test]
    fn remove_counters_on_a_runner_rig_card_decrements_and_saturates_at_zero() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("gorman_drip", 0)];
        state.runner.rig[0].counters = 1;
        let acting = CardId("gorman_drip".to_string());

        let events = evaluate_effect(&mut state, &Effect::RemoveCounters(3), &mut ResolutionContext::for_card(Some(&acting)), &CardRegistry::new()).unwrap();

        assert_eq!(state.runner.rig[0].counters, 0);
        assert_eq!(events, vec![GameEvent::CountersRemoved { card: acting, amount: 3 }]);
    }

    #[test]
    fn add_counters_on_a_corp_installed_card_increments_its_counters() {
        let mut state = game_state();
        state.corp.installed = vec![InstalledCard {
            install_id: InstallId(1083),
            card: CardId("some_asset".to_string()),
            server: ServerId::Remote(0),
            rezzed: true,
            ..Default::default()
        }];
        let acting = CardId("some_asset".to_string());

        evaluate_effect(&mut state, &Effect::AddCounters(3), &mut ResolutionContext::for_card(Some(&acting)), &CardRegistry::new()).unwrap();

        assert_eq!(state.corp.installed[0].counters, 3);
    }

    #[test]
    fn add_counters_without_acting_card_errors_unresolved_card_target() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::AddCounters(1), &mut ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::UnresolvedCardTarget)
        );
    }

    #[test]
    fn add_counters_for_a_card_neither_installed_nor_rigged_errors() {
        let mut state = game_state();
        let acting = CardId("nowhere".to_string());
        assert_eq!(
            evaluate_effect(&mut state, &Effect::AddCounters(1), &mut ResolutionContext::for_card(Some(&acting)), &CardRegistry::new()),
            Err(RulesError::CardNotEligibleForCounters(acting))
        );
    }

    #[test]
    fn boost_strength_encounter_increments_buff_and_effective_strength() {
        // Boosting requires an encounter (`require_encounter`) — an
        // icebreaker's abilities are only usable while encountering ICE.
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 2, 1);
        let acting = CardId("corroder".to_string());

        let events = evaluate_effect(
            &mut state,
            &Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
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

    /// ROADMAP Rules Audit T8: "until the end of this encounter" buffs used
    /// to survive an unbroken "end the run" subroutine, because only the
    /// normal ICE pass reset them. Every way a run ends now goes through
    /// `run::end_run`, which resets them; `Turn`-duration buffs are the
    /// turn's business and stay.
    #[test]
    fn end_the_run_clears_encounter_strength_buffs_but_not_turn_buffs() {
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 2, 1);
        state.runner.rig[0].encounter_strength_buff = 3;
        state.runner.rig[0].turn_strength_buff = 1;

        evaluate_effect(&mut state, &Effect::EndTheRun, &mut ResolutionContext::for_card(None), &CardRegistry::new())
            .expect("a run is active");

        assert!(state.active_run.is_none());
        assert!(state.last_completed_run.is_some(), "the ended run is still snapshotted for OnRunEnded");
        assert_eq!(state.runner.rig[0].encounter_strength_buff, 0);
        assert_eq!(state.runner.rig[0].turn_strength_buff, 1);
    }

    #[test]
    fn boost_strength_turn_increments_turn_buff() {
        // Boosting requires an encounter (`require_encounter`) — an
        // icebreaker's abilities are only usable while encountering ICE.
        let mut state = ice_encounter_state(vec![installed_runner_card("corroder", 2)], 2, 1);
        let acting = CardId("corroder".to_string());

        evaluate_effect(
            &mut state,
            &Effect::BoostStrength { amount: 2, duration: BoostDuration::Turn }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
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
                &Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter }, &mut ResolutionContext::for_card(None),
                &CardRegistry::new()),
            Err(RulesError::UnresolvedCardTarget)
        );
    }

    #[test]
    fn boost_strength_acting_card_not_in_rig_errors_card_not_in_rig() {
        // In an encounter, so the rig lookup is the operative check rather
        // than `require_encounter` short-circuiting first.
        let mut state = ice_encounter_state(Vec::new(), 2, 1);
        let acting = CardId("corroder".to_string());
        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter }, &mut ResolutionContext::for_card(Some(&acting)),
                &CardRegistry::new()),
            Err(RulesError::CardNotInRig { side: Side::Runner, card: acting })
        );
    }

    /// The engine-level half of gating icebreaker abilities to encounters.
    /// `EffectRequirement::DuringEncounter` on the ability stops it being
    /// *offered*; this stops the effect *resolving* however it was reached.
    /// Before both, Cleaver's "+1 strength" was a legal action on the
    /// Corp's turn — affordable, permitted, and pointless.
    #[test]
    fn boost_strength_outside_an_encounter_errors_not_in_encounter() {
        let mut state = game_state();
        state.runner.rig = vec![installed_runner_card("corroder", 2)];
        let acting = CardId("corroder".to_string());

        assert_eq!(
            evaluate_effect(
                &mut state,
                &Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter }, &mut ResolutionContext::for_card(Some(&acting)),
                &CardRegistry::new()),
            Err(RulesError::NoActiveRun)
        );
        assert_eq!(state.runner.rig[0].encounter_strength_buff, 0, "and nothing was mutated");
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
            &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(2), restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
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
            &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(2), restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
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
            &Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
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
                &Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: None }, &mut ResolutionContext::for_card(Some(&CardId("corroder".to_string()))),
                &CardRegistry::new()),
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
                &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
                &CardRegistry::new()),
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
                &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
                &CardRegistry::new()),
            Err(RulesError::BreakerStrengthTooLow {
                breaker: acting.clone(),
                breaker_strength: 1,
                ice: CardId("ice_wall".to_string()),
                ice_strength: 2,
            })
        );

        evaluate_effect(
            &mut state,
            &Effect::BoostStrength { amount: 1, duration: BoostDuration::Encounter }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
        .unwrap();

        let events = evaluate_effect(
            &mut state,
            &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
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
            &Effect::BreakSubroutines { count: SubroutineBreakCount::All, restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
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
            }, &mut ResolutionContext::for_card(Some(&acting)),
            &CardRegistry::new())
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
                }, &mut ResolutionContext::for_card(Some(&acting)),
                &CardRegistry::new()),
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
                &Effect::BreakSubroutines { count: SubroutineBreakCount::Fixed(1), restrict_to: None }, &mut ResolutionContext::for_card(Some(&acting)),
                &CardRegistry::new())
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

        let events = evaluate_effect(&mut state, &effect, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

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
        evaluate_effect(&mut state, &Effect::Trace { base: 3, on_success: Box::new(Effect::GiveTags(1)) }, &mut ResolutionContext::for_card(None), &CardRegistry::new())
            .unwrap();

        let result =
            evaluate_effect(&mut state, &Effect::Trace { base: 5, on_success: Box::new(Effect::GiveTags(2)) }, &mut ResolutionContext::for_card(None), &CardRegistry::new());

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

        let events = evaluate_effect(&mut state, &effect, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

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

        let events = evaluate_effect(&mut state, &effect, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

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

        let events = evaluate_effect(&mut state, &effect, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

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

        let events = evaluate_effect(&mut state, &effect, &mut ResolutionContext::for_card(None), &CardRegistry::new()).unwrap();

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
        state.last_completed_run = Some(CompletedRun { server: ServerId::Hq, cards_accessed: 3, agendas_stolen: 0, persistent_trashed_upgrades: Vec::new(), accessed_cards: Vec::new(), on_end_effect: None, on_end_card: None, on_end_install: None });

        let events = evaluate_effect(
            &mut state,
            &Effect::GainCreditsPerCardAccessedThisRun(Side::Runner), &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
        .unwrap();

        assert_eq!(state.runner.resources.credits, Credits(8));
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Runner, amount: 3 }]);
    }

    #[test]
    fn gain_credits_per_card_accessed_this_run_is_zero_with_no_completed_run() {
        let mut state = game_state();

        let events = evaluate_effect(
            &mut state,
            &Effect::GainCreditsPerCardAccessedThisRun(Side::Runner), &mut ResolutionContext::for_card(None),
            &CardRegistry::new())
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
            install_id: InstallId(1084),
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
                Side::Corp, &ResolutionContext::for_card(Some(&ping)),
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
                Side::Corp, &ResolutionContext::for_card(Some(&ping)),
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
                Side::Corp, &ResolutionContext::for_card(Some(&ping)),
                &CardRegistry::new()
            ),
            Ok(())
        );
    }

    /// A second `DealDamage` in the same `Sequence` overwrites the first's
    /// discards rather than accumulating them, so
    /// `LastDamageTrashedOddCostCard` always answers about the most recent
    /// damage — the "last" its name promises.
    #[test]
    fn a_second_deal_damage_replaces_the_first_ones_discards_in_the_context() {
        let mut registry = CardRegistry::new();
        for (id, cost) in [("odd_cost", 3u32), ("even_a", 2), ("even_b", 4)] {
            registry.insert(crate::cards::common::base_card(id, id, Side::Runner, crate::dsl::CardType::Event, cost));
        }

        let mut state = game_state();
        // Grip is drawn from randomly, so stack it with one card per hit to
        // make which card each `DealDamage` discards deterministic.
        state.runner.grip = vec![CardId("odd_cost".to_string())];
        let mut ctx = ResolutionContext::default();

        evaluate_effect(&mut state, &Effect::DealDamage(DamageType::Net, 1), &mut ctx, &registry).unwrap();
        assert_eq!(ctx.damage_discarded, vec![CardId("odd_cost".to_string())]);
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastDamageTrashedOddCostCard, Side::Corp, &ctx, &registry),
            Ok(()),
            "the odd-cost card was just discarded"
        );

        state.runner.grip = vec![CardId("even_a".to_string())];
        evaluate_effect(&mut state, &Effect::DealDamage(DamageType::Net, 1), &mut ctx, &registry).unwrap();
        assert_eq!(ctx.damage_discarded, vec![CardId("even_a".to_string())], "replaced, not appended");
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastDamageTrashedOddCostCard, Side::Corp, &ctx, &registry),
            Err(RulesError::RequirementNotMet),
            "the *last* damage trashed an even-cost card, so the earlier odd one must not carry over"
        );
    }

    #[test]
    fn last_damage_trashed_odd_cost_card_requirement_checks_registry_cost() {
        let state = game_state();
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

        // The discards now live on the resolution in flight, not on
        // `GameState` — same assertions, read from the new home.
        let mut ctx = ResolutionContext {
            damage_discarded: vec![CardId("even_cost".to_string())],
            ..ResolutionContext::default()
        };
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastDamageTrashedOddCostCard, Side::Corp, &ctx, &registry),
            Err(RulesError::RequirementNotMet)
        );

        ctx.damage_discarded = vec![CardId("even_cost".to_string()), CardId("odd_cost".to_string())];
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastDamageTrashedOddCostCard, Side::Corp, &ctx, &registry),
            Ok(())
        );

        // A resolution that dealt no damage answers "not met" rather than
        // inheriting some earlier action's discards — the stale read the
        // old `GameState` field allowed.
        assert_eq!(
            check_requirement(
                &state,
                &EffectRequirement::LastDamageTrashedOddCostCard,
                Side::Corp,
                &ResolutionContext::default(),
                &registry
            ),
            Err(RulesError::RequirementNotMet)
        );
    }

    #[test]
    fn last_run_was_on_hq_or_rnd_requirement() {
        let mut state = game_state();
        state.last_completed_run = Some(CompletedRun { server: ServerId::Archives, cards_accessed: 0, agendas_stolen: 0, persistent_trashed_upgrades: Vec::new(), accessed_cards: Vec::new(), on_end_effect: None, on_end_card: None, on_end_install: None });
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastRunWasOnHqOrRnD, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );

        state.last_completed_run = Some(CompletedRun { server: ServerId::Hq, cards_accessed: 2, agendas_stolen: 0, persistent_trashed_upgrades: Vec::new(), accessed_cards: Vec::new(), on_end_effect: None, on_end_card: None, on_end_install: None });
        assert_eq!(
            check_requirement(&state, &EffectRequirement::LastRunWasOnHqOrRnD, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
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
        assert_eq!(check_requirement(&state, &req, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()), Ok(()));

        state.runner.resources.credits = Credits(11);
        assert_eq!(
            check_requirement(&state, &req, Side::Runner, &ResolutionContext::for_card(None), &CardRegistry::new()),
            Err(RulesError::RequirementNotMet)
        );
    }
}
