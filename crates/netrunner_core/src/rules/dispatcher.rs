//! Central event-driven trigger dispatch.
//!
//! `dispatch_event` is the single place that answers "given this
//! `GameEvent`, which installed cards react, and in what order" — it maps
//! each dispatch-relevant `GameEvent` variant to a `dsl::Trigger` and the
//! card(s) that trigger applies to, then delegates the actual firing to
//! `ability::process_card_triggers`.
//!
//! This deliberately isn't a stateful registry that tracks "active
//! behaviors" separately from `GameState` — `CorpState::installed` and
//! `RunnerState::rig` are already the single source of truth for what's in
//! play, so every candidate set here is re-derived fresh from `GameState` on
//! each call, the same "pure re-derivation" convention `win::
//! check_win_conditions` already follows. CardDefinition *behavior* itself stays
//! entirely data-driven (`dsl::TriggeredEffect`/`AbilityDef`/`Effect`) —
//! this module adds only the event-to-audience mapping and firing order.

use crate::cards::CardRegistry;
use crate::dsl::{CardId, CardSubtype, Trigger};
use crate::rules::ability;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::ServerId;
use crate::rules::state::{DeferredTrigger, GamePhase, GameState, InstallId, InstallSlot, PendingDecision, Side};

/// Given `event`, fires every installed card's matching `Trigger`s and
/// returns the resulting `GameEvent`s, in firing order.
///
/// Most events have a single, unambiguous audience (the card the event is
/// about, or one side's identity) computed directly from the event's own
/// fields — no separate registration/lookup step needed. A `GameEvent`
/// variant with no card reactions defined for it (most of them — most
/// `GameEvent`s describe a state change with nothing left to react to) is
/// not an error: yields `Ok(Vec::new())`, mirroring `process_card_triggers`'s
/// own "no matching trigger" convention.
pub fn dispatch_event(
    state: &mut GameState,
    registry: &CardRegistry,
    event: &GameEvent,
) -> Result<Vec<GameEvent>, RulesError> {
    match event {
        GameEvent::EventPlayed { card, .. } => {
            fire_direct(state, registry, card, None, Trigger::OnPlay, event)
        }

        GameEvent::OperationPlayed { card, .. } => {
            let mut events = fire_direct(state, registry, card, None, Trigger::OnPlay, event)?;
            let is_transaction =
                registry.get(card).is_some_and(|c| c.subtypes.contains(&CardSubtype::Transaction));
            if is_transaction && let Some(identity) = state.corp.identity.clone() {
                events.extend(fire_direct(state, registry, &identity, None, Trigger::OnTransactionPlayed, event)?);
            }
            Ok(events)
        }

        GameEvent::ProgramInstalled { card, .. } => {
            // Fires `OnInstall` against the just-installed Program itself
            // — mirrors `ResourceInstalled`'s "the card needs to react to
            // its own install" widening below (previously only identities
            // reacted to a Program install, via `OnVirusInstalled`) — e.g.
            // Botulus/Tranquilizer/Fermenter's "when you install this
            // program... place 1 virus counter on this program."
            let mut events = fire_direct(state, registry, card, newest_rig_install(state, card), Trigger::OnInstall, event)?;
            let is_virus = registry.get(card).is_some_and(|c| c.subtypes.contains(&CardSubtype::Virus));
            if is_virus {
                if let Some(identity) = state.runner.identity.clone() {
                    events.extend(fire_direct(state, registry, &identity, None, Trigger::OnVirusInstalled, event)?);
                }
                // Every OTHER rig card also gets a chance to react to a
                // virus install, but — unlike the identity reaction above
                // — its effect targets the just-installed virus program
                // itself, not the reacting card. e.g. Cookbook's "you may
                // place 1 virus counter on it." Excludes `card` itself to
                // avoid a virus program reacting to its own installation.
                let installed = newest_rig_install(state, card);
                let plan: Vec<DeferredTrigger> = state
                    .runner
                    .rig
                    .iter()
                    .filter(|c| Some(c.install_id) != installed)
                    .map(|owner| DeferredTrigger {
                        card: owner.card.clone(),
                        install: Some(owner.install_id),
                        trigger: Trigger::OnVirusInstalled,
                        target: Some(card.clone()),
                        target_install: installed,
                        event: Some(event.clone()),
                    })
                    .collect();
                events.extend(fire_plan(state, registry, &plan)?);
            }
            Ok(events)
        }

        // The just-installed Hardware reacts to its own install, the same
        // widening `ProgramInstalled`/`ResourceInstalled` got — GAMEDRAGON™
        // Pro's "when you install this hardware ... you may host it".
        GameEvent::HardwareInstalled { card, .. } => {
            fire_direct(state, registry, card, newest_rig_install(state, card), Trigger::OnInstall, event)
        }

        GameEvent::CardInstalled { side, .. } => {
            let identity = match side {
                Side::Corp => state.corp.identity.clone(),
                Side::Runner => state.runner.identity.clone(),
            };
            match identity {
                Some(identity) => fire_direct(state, registry, &identity, None, Trigger::OnInstall, event),
                None => Ok(Vec::new()),
            }
        }

        // Unlike `CardInstalled` above (Corp-only today, identity-only
        // audience), a Resource needs `OnInstall` to fire against *itself*
        // — e.g. Red Team/Telework Contract's own "when you install this
        // resource, load N credits onto it" — so this widens the same
        // `Trigger::OnInstall` to also reach the just-installed card,
        // mirroring the "fire on card + identity" convention already used
        // by `AgendaScored`/`CardTrashedFromAccess`.
        GameEvent::ResourceInstalled { card, .. } => {
            let mut events = fire_direct(state, registry, card, newest_rig_install(state, card), Trigger::OnInstall, event)?;
            if let Some(identity) = state.runner.identity.clone() {
                events.extend(fire_direct(state, registry, &identity, None, Trigger::OnInstall, event)?);
            }
            Ok(events)
        }

        GameEvent::CardAccessed { card, server, install } => {
            // The access pinned the instance; the by-`CardId` lookup is the
            // fallback for an event that carries none.
            let install = install.or_else(|| root_install_of(state, card, *server));
            fire_direct(state, registry, card, install, Trigger::OnAccessed, event)
        }

        GameEvent::CardTrashedFromAccess { card, .. } => {
            // Only the Runner ever accesses and trashes a card this way, so
            // the identity to react is unambiguously theirs — mirrors
            // `AgendaScored`'s "also fire the owning identity" widening
            // below, e.g. René "Loup" Arcemont's "the first time each turn
            // you trash a card you are accessing, gain 1 credit and draw 1
            // card."
            let mut events = fire_direct(state, registry, card, None, Trigger::OnTrashedFromAccess, event)?;
            if let Some(identity) = state.runner.identity.clone() {
                events.extend(fire_direct(state, registry, &identity, None, Trigger::OnTrashedFromAccess, event)?);
            }
            Ok(events)
        }

        GameEvent::AgendaScored { card, server, .. } => {
            let mut events = fire_direct(state, registry, card, None, Trigger::OnAgendaScored, event)?;
            if let Some(identity) = state.corp.identity.clone() {
                events.extend(fire_direct(state, registry, &identity, None, Trigger::OnAgendaScored, event)?);
            }
            // Every other rezzed Root-slot install on the scored agenda's
            // own server also gets a chance to react — e.g. Malapert Data
            // Vault's "whenever you score an agenda from the root of this
            // server." Same audience-computation shape as `OnApproachServer`
            // above (rezzed Root installs on a given server), reused here
            // rather than a bespoke `EffectRequirement`.
            let root_installs: Vec<(Option<InstallId>, CardId)> = state
                .corp
                .installed
                .iter()
                .filter(|installed| {
                    installed.rezzed && installed.server == *server && installed.slot == InstallSlot::Root
                })
                .map(|installed| (Some(installed.install_id), installed.card.clone()))
                .collect();
            events.extend(fire_each(state, registry, &root_installs, Trigger::OnAgendaScored, event)?);
            // Runner-side widening (M5): the Runner's own identity and rig
            // also get a chance to react to a Corp agenda score — e.g.
            // Pantograph's "whenever an agenda is scored or stolen, gain 1
            // credit." Previously deferred (see `AgendaStolen`'s own
            // matching widening below) until a real card needed it.
            events.extend(fire_runner_side(state, registry, Trigger::OnAgendaScored, event)?);
            Ok(events)
        }

        GameEvent::AgendaStolen { card, .. } => {
            // Fires against the stolen agenda's own trigger (e.g. Send a
            // Message's "when this agenda is scored or stolen...") in
            // addition to the Corp identity's — mirrors `AgendaScored`'s
            // own "also fire the card itself" shape, which `AgendaStolen`
            // was previously missing.
            let mut events = fire_direct(state, registry, card, None, Trigger::OnAgendaStolen, event)?;
            if let Some(identity) = state.corp.identity.clone() {
                events.extend(fire_direct(state, registry, &identity, None, Trigger::OnAgendaStolen, event)?);
            }
            // Runner-side widening (M5): the Runner's own identity and rig
            // react to their own steal — e.g. Tāo Salonga: Telepresence
            // Magician, Pantograph's "whenever an agenda is scored or
            // stolen, gain 1 credit."
            events.extend(fire_runner_side(state, registry, Trigger::OnAgendaStolen, event)?);
            Ok(events)
        }

        GameEvent::TurnStarted { side, .. } => {
            let candidates: Vec<(Option<InstallId>, CardId)> = match side {
                Side::Corp => state
                    .corp
                    .installed
                    .iter()
                    .filter(|installed| installed.rezzed)
                    .map(|installed| (Some(installed.install_id), installed.card.clone()))
                    .collect(),
                Side::Runner => state.runner.rig.iter().map(|card| (Some(card.install_id), card.card.clone())).collect(),
            };
            fire_each(state, registry, &candidates, Trigger::OnTurnStart, event)
        }

        // "Whenever a run begins": the Runner's identity and every rig card
        // — Side Hustle loads a credit on each run. Used to reach the
        // identity alone; no System Gateway rig card reacted to a run
        // starting.
        GameEvent::RunInitiated { .. } => {
            let mut candidates: Vec<(Option<InstallId>, CardId)> = Vec::new();
            if let Some(identity) = state.runner.identity.clone() {
                candidates.push((None, identity));
            }
            candidates.extend(state.runner.rig.iter().map(|card| (Some(card.install_id), card.card.clone())));
            fire_each(state, registry, &candidates, Trigger::OnRunStart, event)
        }

        GameEvent::IceEncountered { card_id, .. } => {
            fire_direct(state, registry, card_id, encountered_install(state), Trigger::OnEncounter, event)
        }

        GameEvent::RunSucceeded { server } => {
            // Any broadcast "on a successful run" reaction (any server, e.g.
            // Desperado) and any HQ-specific reaction (Gabriel Santiago's
            // identity ability, or a non-identity card like Docklands Pass)
            // both key off this one event — every candidate is tried against
            // both triggers; `process_card_triggers`'s own "no matching
            // TriggeredEffect on this card" no-op means a card only ever
            // reacts to the trigger it actually declares, whether or not
            // it's the identity (collected as a single ordered candidate
            // list; both Runner-side today, so `order_active_first` is a
            // no-op in practice until a Corp-side "runner made a successful
            // run" reactor exists).
            state.runner.made_successful_run_this_turn = true;

            // An "if successful, ..." rider attached to the run itself
            // rather than to an installed card — e.g. Jailbreak, an Event,
            // which is never installed and so can't carry a
            // `Trigger::OnSuccessfulRun` of its own. Taken (not cloned) so
            // it fires exactly once even if a run somehow re-enters
            // `Success`. Resolved before the card-trigger sweep below so an
            // access bonus it grants is in place for the same breach.
            let on_success = state.active_run.as_mut().and_then(|run| {
                run.on_success_effect.take().map(|effect| (effect, run.on_success_card.take(), run.on_success_install.take()))
            });
            let mut events = Vec::new();
            if let Some((effect, card, install)) = on_success {
                let mut ctx = ability::ResolutionContext::for_install_trigger(install, card.as_ref(), Some(event));
                events.extend(ability::evaluate_effect(state, &effect, &mut ctx, registry)?);
            }

            let mut candidates: Vec<(Side, (Option<InstallId>, CardId))> = Vec::new();
            if let Some(identity) = state.runner.identity.clone() {
                candidates.push((Side::Runner, (None, identity)));
            }
            candidates.extend(state.runner.rig.iter().map(|card| (Side::Runner, (Some(card.install_id), card.card.clone()))));

            // Built as one flat plan rather than fired inline, so a
            // blockage landing *between* two of the four triggers on the
            // same card queues exactly what's left — see `fire_plan`.
            let mut plan: Vec<DeferredTrigger> = Vec::new();
            for (install, card_id) in order_active_first(Side::Runner, candidates) {
                let due = |trigger| DeferredTrigger {
                    card: card_id.clone(),
                    install,
                    trigger,
                    target: None,
                    target_install: None,
                    event: Some(event.clone()),
                };
                plan.push(due(Trigger::OnSuccessfulRun));
                if *server == ServerId::Hq {
                    plan.push(due(Trigger::OnSuccessfulRunOnHq));
                }
                if *server == ServerId::RnD {
                    plan.push(due(Trigger::OnSuccessfulRunOnRnD));
                }
                if matches!(server, ServerId::Hq | ServerId::RnD | ServerId::Archives) {
                    plan.push(due(Trigger::OnSuccessfulRunOnCentralServer));
                }
            }
            events.extend(fire_plan(state, registry, &plan)?);
            Ok(events)
        }

        // The approach-server step, before the run is successful. Audience
        // for `Trigger::OnApproachServer` is every rezzed Corp Root-slot
        // install in `server` (an Upgrade/Asset sitting in its root), not
        // the ICE and not the identity — Manegarm Skunkworks, Anoetic Void.
        // Used to be fired from `RunSucceeded`; see that event's doc for why
        // the order matters.
        GameEvent::ServerApproached { server } => {
            let root_installs: Vec<(Option<InstallId>, CardId)> = state
                .corp
                .installed
                .iter()
                .filter(|installed| {
                    installed.rezzed && installed.server == *server && installed.slot == InstallSlot::Root
                })
                .map(|installed| (Some(installed.install_id), installed.card.clone()))
                .collect();
            fire_each(state, registry, &root_installs, Trigger::OnApproachServer, event)
        }

        GameEvent::IceRezzed { card, install, .. } => {
            fire_direct(state, registry, card, Some(*install), Trigger::OnRez, event)
        }

        GameEvent::CardAdvanced { .. } => match state.corp.identity.clone() {
            Some(identity) => fire_direct(state, registry, &identity, None, Trigger::OnAdvance, event),
            None => Ok(Vec::new()),
        },

        // Only the "normal" run conclusions dispatch `Trigger::OnRunEnded`
        // (`RunCompleted`/`RunJackedOut`/`RunEndedByEffect`, each fired from
        // its own call site with `GameState::last_completed_run` snapshotted
        // immediately beforehand) — a flatline/agenda-point win mid-access
        // (`run::access::finish_if_game_over`) does not, since resolving
        // more card triggers after `GamePhase::GameOver` is already set
        // would mutate a concluded game for no observable benefit (Mayfly
        // self-trashing or Zahya gaining credits post-game-over changes
        // nothing about the outcome).
        GameEvent::RunCompleted { .. } | GameEvent::RunJackedOut { .. } | GameEvent::RunEndedByEffect { .. } => {
            let mut candidates: Vec<(Option<InstallId>, CardId)> = Vec::new();
            if let Some(identity) = state.runner.identity.clone() {
                candidates.push((None, identity));
            }
            candidates.extend(state.runner.rig.iter().map(|card| (Some(card.install_id), card.card.clone())));
            // Corp-side reactors on the server that was just run: rezzed
            // Root-slot installs still in play (same audience shape as
            // `OnApproachServer`), plus any `persistent_after_trash` card
            // the Runner trashed *during* this run — the latter is no
            // longer in `CorpState::installed` at all, which is exactly
            // what AMAZE Amusements' "this ability still applies for the
            // remainder of this run" requires. Both are read from the
            // `CompletedRun` snapshot, since `active_run` is already
            // cleared by the time this dispatches.
            if let Some(completed) = state.last_completed_run.clone() {
                candidates.extend(
                    state
                        .corp
                        .installed
                        .iter()
                        .filter(|installed| {
                            installed.rezzed
                                && installed.slot == InstallSlot::Root
                                && installed.server == completed.server
                        })
                        .map(|installed| (Some(installed.install_id), installed.card.clone())),
                );
                candidates.extend(completed.persistent_trashed_upgrades.iter().cloned().map(|card| (None, card)));
            }
            fire_each(state, registry, &candidates, Trigger::OnRunEnded, event)
        }

        // "When your discard phase ends" — fires against that side's own
        // identity only (the Corp's, for Jinteki: Restoring Humanity).
        GameEvent::DiscardPhaseEnded { side } => {
            let identity = match side {
                Side::Corp => state.corp.identity.clone(),
                Side::Runner => state.runner.identity.clone(),
            };
            match identity {
                Some(identity) => fire_direct(state, registry, &identity, None, Trigger::OnDiscardPhaseEnd, event),
                None => Ok(Vec::new()),
            }
        }

        // Through `fire_each` (not `fire_direct`) although the audience is
        // one card: a tag can be *paid* as a cost (`Cost::TakeTags`,
        // Funhouse), and `pending_choice::resolve_accept` dispatches this
        // event after the choice's own effect — which may itself have
        // parked something. `fire_plan`'s blocked-resolution guard then
        // queues the reaction instead of firing it under the parked state.
        GameEvent::TagsGiven { side: Side::Runner, .. } => match state.corp.identity.clone() {
            Some(identity) => fire_each(state, registry, &[(None, identity)], Trigger::OnTagsGiven, event),
            None => Ok(Vec::new()),
        },

        GameEvent::BasicDrawActionTaken { side } => {
            let identity = match side {
                Side::Corp => state.corp.identity.clone(),
                Side::Runner => state.runner.identity.clone(),
            };
            let mut candidates: Vec<(Option<InstallId>, CardId)> = identity.into_iter().map(|id| (None, id)).collect();
            if *side == Side::Runner {
                candidates.extend(state.runner.rig.iter().map(|card| (Some(card.install_id), card.card.clone())));
            }
            fire_each(state, registry, &candidates, Trigger::OnBasicDrawAction, event)
        }

        // Both of these reach cards on *either* side, so unlike the
        // single-side audiences above they need the active-player-first
        // rule applied explicitly — see `order_active_first`.
        GameEvent::DamageAboutToResolve { .. } => {
            let candidates = order_active_first(turn_active_side(state), both_sides_candidates(state));
            fire_each(state, registry, &candidates, Trigger::OnDamageAboutToResolve, event)
        }

        GameEvent::TrashAboutToResolve { .. } => {
            let candidates = order_active_first(turn_active_side(state), both_sides_candidates(state));
            fire_each(state, registry, &candidates, Trigger::OnTrashAboutToResolve, event)
        }

        _ => Ok(Vec::new()),
    }
}

/// Rezzed Corp installs ∪ full Runner rig — the same audience `TurnStarted`'s
/// arm collects per-side, unioned here since a prevention trigger could in
/// principle belong to either side.
fn both_sides_candidates(state: &GameState) -> Vec<(Side, (Option<InstallId>, CardId))> {
    state
        .corp
        .installed
        .iter()
        .filter(|installed| installed.rezzed)
        .map(|installed| (Side::Corp, (Some(installed.install_id), installed.card.clone())))
        .chain(state.runner.rig.iter().map(|card| (Side::Runner, (Some(card.install_id), card.card.clone()))))
        .collect()
}

/// `fire_card_triggers` for a single, immediately-fired reaction — the
/// `DeferredTrigger` is built here so every dispatch, deferred or not, goes
/// through the same record and the same announce path. `install` is the
/// copy of `card` that reacts; `None` for an identity or a card that has
/// left play.
fn fire_direct(
    state: &mut GameState,
    registry: &CardRegistry,
    card: &CardId,
    install: Option<InstallId>,
    trigger: Trigger,
    event: &GameEvent,
) -> Result<Vec<GameEvent>, RulesError> {
    let due = DeferredTrigger {
        card: card.clone(),
        install,
        trigger,
        target: None,
        target_install: None,
        event: Some(event.clone()),
    };
    ability::fire_card_triggers(state, registry, &due, true)
}

/// The most recently installed rig copy of `card` — the one a
/// `*Installed { card }` event is about, since `allocate_install_id` is
/// monotonic and the install handlers push last.
fn newest_rig_install(state: &GameState, card: &CardId) -> Option<InstallId> {
    state.runner.rig.iter().rev().find(|c| &c.card == card).map(|c| c.install_id)
}

/// The root install of `card` on `server`, for a `CardAccessed` reaction —
/// `None` when the accessed card sits in a hidden zone rather than a root.
fn root_install_of(state: &GameState, card: &CardId, server: ServerId) -> Option<InstallId> {
    state
        .corp
        .installed
        .iter()
        .find(|c| &c.card == card && c.server == server && c.slot == InstallSlot::Root)
        .map(|c| c.install_id)
}

/// The install of the ICE being encountered, for an `IceEncountered`
/// reaction.
fn encountered_install(state: &GameState) -> Option<InstallId> {
    state.active_run.as_ref().and_then(|run| run.ice.get(run.position)).map(|ice| ice.install_id)
}

/// Whose turn it currently is, for `order_active_first`'s benefit.
///
/// Every `GamePhase` variant carries a `Side`, so unlike `legal_actions::
/// current_actor` (which returns `None` during `StartOfTurn`) this is
/// total — which is exactly why the ordering sites read `phase` rather
/// than asking who may act right now. Those differ mid-window: the Corp
/// can hold priority during the Runner's turn, but the Runner is still the
/// active player whose reactions resolve first.
///
/// `GameOver(side)` names the *winner* rather than an active player.
/// Harmless: nothing dispatches triggers after the game has ended.
fn turn_active_side(state: &GameState) -> Side {
    match state.phase {
        GamePhase::Mulligan(side)
        | GamePhase::StartOfTurn(side)
        | GamePhase::Action(side)
        | GamePhase::Discard { side, .. }
        | GamePhase::GameOver(side) => side,
    }
}

/// Fires `trigger` against each of `candidates` in order, collecting events.
///
/// Stops the moment one of them parks something blocking (a decision, a
/// paid choice, a prevention window, a trace) and **queues the untouched
/// remainder** on `GameState::deferred_triggers` rather than firing them
/// underneath it. `drain_deferred_triggers` picks them back up once the
/// blockage clears.
///
/// Before the queue existed this loop had no such guard, so e.g.
/// *Clearinghouse*'s `OnTurnStart` (a `PresentChoice`, which parks a
/// `PendingDecision`) let every later Corp `OnTurnStart` card resolve
/// during its pending choice.
fn fire_each(
    state: &mut GameState,
    registry: &CardRegistry,
    candidates: &[(Option<InstallId>, CardId)],
    trigger: Trigger,
    event: &GameEvent,
) -> Result<Vec<GameEvent>, RulesError> {
    let plan: Vec<DeferredTrigger> = candidates
        .iter()
        .map(|(install, card)| DeferredTrigger {
            card: card.clone(),
            install: *install,
            trigger,
            target: None,
            target_install: None,
            event: Some(event.clone()),
        })
        .collect();
    fire_plan(state, registry, &plan)
}

/// Fires an ordered plan of triggers, stopping and queueing the untouched
/// remainder the moment one of them parks something blocking.
///
/// The single guarded primitive every dispatch site funnels through — a
/// flat `(card, trigger, target)` plan rather than a bare card list,
/// because `RunSucceeded` fires up to four different triggers per card and
/// a blockage can land *between* two of them on the same card. Building the
/// whole plan up front makes "what's left" a simple slice in every case.
fn fire_plan(
    state: &mut GameState,
    registry: &CardRegistry,
    plan: &[DeferredTrigger],
) -> Result<Vec<GameEvent>, RulesError> {
    if let Some(events) = offer_trigger_order(state, registry, plan)? {
        return Ok(events);
    }
    let mut events = Vec::new();
    for (index, due) in plan.iter().enumerate() {
        // A finished game fires nothing further and queues nothing either
        // — `win::end_game` has just emptied the queue, and refilling it
        // would leave triggers to drain into a game that is over.
        if state.is_over() {
            break;
        }
        if state.is_resolution_blocked() {
            state.deferred_triggers.extend(plan[index..].iter().cloned());
            break;
        }
        events.extend(fire_one(state, registry, due)?);
    }
    Ok(events)
}

/// Whether `due`'s card actually declares the trigger it's planned for.
///
/// A plan is built from every card that *could* react (every rezzed
/// install, every rig card); most of them declare no matching trigger and
/// `process_card_triggers` no-ops on them. Filtering by this before
/// counting is what keeps `ChooseTriggerOrder` from parking a pointless
/// decision every time a player has two cards installed.
///
/// A cheap registry lookup, deliberately *not* a dry run: it does not
/// evaluate `TriggeredEffect::requirement`, so a card whose requirement
/// fails still counts. That over-counts rather than under-counts —
/// offering a choice between two triggers where one turns out to no-op is
/// harmless; silently picking an order the player was entitled to choose
/// would not be.
fn declares_trigger(registry: &CardRegistry, due: &DeferredTrigger) -> bool {
    registry.get(&due.card).is_some_and(|card| card.triggers.iter().any(|t| t.trigger == due.trigger))
}

/// Parks a `PendingDecision::ChooseTriggerOrder` if `plan` holds two or
/// more genuinely-reacting triggers belonging to **one** side, returning
/// `Some` to say the dispatch has been handed to the player.
///
/// Real Netrunner gives a player the order of their own simultaneous
/// triggers. Cross-side order is not theirs to choose — it is fixed by
/// rule (`order_active_first`) — so a plan spanning both sides parks
/// nothing and fires in the already-correct order.
///
/// `None` (fire immediately, no decision) whenever the order can't matter:
/// fewer than two reacting cards, or a mixed-side plan.
fn offer_trigger_order(
    state: &mut GameState,
    registry: &CardRegistry,
    plan: &[DeferredTrigger],
) -> Result<Option<Vec<GameEvent>>, RulesError> {
    if state.resolution_halted() {
        return Ok(None);
    }
    let reacting: Vec<DeferredTrigger> =
        plan.iter().filter(|due| declares_trigger(registry, due)).cloned().collect();
    if reacting.len() < 2 {
        return Ok(None);
    }
    let Some(chooser) = single_side_of(state, &reacting) else { return Ok(None) };
    // `ChooseTriggerToResolve` is indexed by position in `reacting`, and
    // `ActionSpace` reserves `CHOOSE_TRIGGER_LEN` slots for it. Each card
    // can contribute several entries (one per success-trigger variant it
    // declares), so the bound is not "installed cards" — this is where an
    // overrun would first become visible, ahead of the index sweep's
    // "no index for a legal action" panic.
    debug_assert!(
        reacting.len() <= crate::rules::action_mask::CHOOSE_TRIGGER_LEN,
        "{} simultaneous triggers exceed the ActionSpace segment ({})",
        reacting.len(),
        crate::rules::action_mask::CHOOSE_TRIGGER_LEN
    );

    // Anything in `plan` that doesn't actually react still needs to not be
    // lost — it no-ops, but queueing it keeps the plan's shape honest and
    // costs nothing.
    state.deferred_triggers.extend(plan.iter().filter(|due| !declares_trigger(registry, due)).cloned());
    state.pending_decision = Some(PendingDecision::ChooseTriggerOrder {
        chooser,
        pending: reacting,
        resume: crate::rules::state::PendingChoiceResume::None,
    });
    Ok(Some(vec![GameEvent::TriggerOrderPending { chooser }]))
}

/// The one side every card in `plan` belongs to, or `None` if they span
/// both. A card is Corp's if it's among their installs, Runner's if it's
/// in the rig or is their identity.
fn single_side_of(state: &GameState, plan: &[DeferredTrigger]) -> Option<Side> {
    let side_of = |card: &CardId| {
        if state.corp.installed.iter().any(|c| &c.card == card) || state.corp.identity.as_ref() == Some(card) {
            Some(Side::Corp)
        } else if state.runner.rig.iter().any(|c| &c.card == card) || state.runner.identity.as_ref() == Some(card) {
            Some(Side::Runner)
        } else {
            None
        }
    };
    let first = side_of(&plan.first()?.card)?;
    plan.iter().all(|due| side_of(&due.card) == Some(first)).then_some(first)
}

/// `fire_one` for callers outside this module — `pending_choice` firing
/// the trigger a player just picked out of a `ChooseTriggerOrder`.
pub(crate) fn fire_deferred(
    state: &mut GameState,
    registry: &CardRegistry,
    due: &DeferredTrigger,
) -> Result<Vec<GameEvent>, RulesError> {
    fire_one(state, registry, due)
}

/// Fires one planned trigger, routing through the targeting variant when
/// the reacting card and the card its effect acts on differ.
fn fire_one(
    state: &mut GameState,
    registry: &CardRegistry,
    due: &DeferredTrigger,
) -> Result<Vec<GameEvent>, RulesError> {
    if !still_applies(state, due) {
        return Ok(Vec::new());
    }
    // `due.event` is what makes a deferred trigger indistinguishable from
    // one that fired immediately: it rebuilds the same
    // `ability::ResolutionContext`, so a requirement reading the triggering
    // event gets the same answer either way.
    // Announced: every trigger the *game* fires passes through here, so
    // this is where `GameEvent::TriggerFired` becomes an exact record.
    ability::fire_card_triggers(state, registry, due, true)
}

/// Whether a trigger planned against a run's own events still has a run to
/// apply to.
///
/// A plan fires one card at a time, and a card can end the run partway
/// through it — Anoetic Void's "trash 2 cards from HQ and pay 2[c]: end the
/// run" is an `OnApproachServer` reaction, and so is Manegarm Skunkworks'
/// "pay 2[click] or 5[c] or end the run". With both protecting one remote,
/// Anoetic resolved first (parking a card selection), ended the run, and
/// the queued Skunkworks trigger then fired against *no run*, parking a
/// paid choice whose decline resolved `EndTheRun` into `NoActiveRun` and
/// whose acceptance the Runner could not afford. No legal action for the
/// Runner, deterministically — found by the index-path sweep's
/// random-vs-random seating at seed 85 on the mechanic-coverage decks.
///
/// Real Netrunner agrees: once the run has ended, remaining "when the
/// Runner approaches" abilities have nothing to react to. The check lives
/// here, in the one function every planned or deferred trigger fires
/// through, rather than in `drain_deferred_triggers` alone — `fire_plan`'s
/// own loop has the same exposure when the run-ending card does not park.
///
/// Only the run-scoped events are guarded. `Trigger::OnRunEnded` is fired
/// from `RunCompleted`/`RunJackedOut`/`RunEndedByEffect`, which by
/// definition arrive with no active run, so those must keep firing.
fn still_applies(state: &GameState, due: &DeferredTrigger) -> bool {
    let run_scoped = matches!(
        due.event,
        Some(
            GameEvent::ServerApproached { .. }
                | GameEvent::RunSucceeded { .. }
                | GameEvent::IceEncountered { .. }
                | GameEvent::RunInitiated { .. }
        )
    );
    // And nothing applies to a game that is over — a trigger drained after
    // the flatline that ended the game has no game to act on.
    !state.is_over() && (!run_scoped || state.active_run.is_some())
}

/// Fires whatever `fire_each` had to queue, once whatever blocked it has
/// been resolved.
///
/// Called from exactly one place — `engine::apply_action`, after the action
/// handler returns — rather than from each of the ~6 resolution paths in
/// `pending_choice`. Same reasoning as `apply_action`'s existing
/// `active_trace` guard: one centralized call is simpler and harder to miss
/// than threading it through every handler, and this one additionally
/// covers trace and prevention-window resolution, which a
/// `pending_choice`-only drain would miss entirely.
///
/// Stops as soon as a drained trigger parks something new, leaving the rest
/// queued for the next action — so a chain of parking triggers resolves one
/// player decision at a time instead of deadlocking or dropping any.
pub(crate) fn drain_deferred_triggers(
    state: &mut GameState,
    registry: &CardRegistry,
) -> Result<Vec<GameEvent>, RulesError> {
    let mut events = Vec::new();
    while !state.resolution_halted() && !state.deferred_triggers.is_empty() {
        let due = state.deferred_triggers.remove(0);
        events.extend(fire_one(state, registry, &due)?);
    }
    Ok(events)
}

/// The Runner's identity plus every rig card — the audience `AgendaScored`/
/// `AgendaStolen` widen to reach (M5), so a Runner-side card can react to
/// either side's agenda-scoring event exactly like `AgendaScored`'s
/// Corp-side identity/root-install audience already does.
fn fire_runner_side(
    state: &mut GameState,
    registry: &CardRegistry,
    trigger: Trigger,
    event: &GameEvent,
) -> Result<Vec<GameEvent>, RulesError> {
    let mut candidates: Vec<(Option<InstallId>, CardId)> = state.runner.identity.iter().cloned().map(|id| (None, id)).collect();
    candidates.extend(state.runner.rig.iter().map(|c| (Some(c.install_id), c.card.clone())));
    fire_each(state, registry, &candidates, trigger, event)
}

/// Orders trigger candidates so `active`'s cards resolve before the other
/// side's — Netrunner/Null Signal Games priority rule 4 ("active player's
/// reactions resolve first"). A stable sort, so each side's own relative
/// (declaration/install) order is preserved.
///
/// `active` is passed explicitly by each call site rather than derived from
/// `GameState` internally: `legal_actions::current_actor` returns `None`
/// during `GamePhase::StartOfTurn`, which is exactly when the broadcast
/// `OnTurnStart` dispatch needs an answer, so no single internal derivation
/// serves every dispatch site correctly.
fn order_active_first<T>(active: Side, mut candidates: Vec<(Side, T)>) -> Vec<T> {
    candidates.sort_by_key(|(side, _)| *side != active);
    candidates.into_iter().map(|(_, id)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::state::InstallId;
    use crate::rules::test_support::fixture_install_id;
    use crate::cards::CardRegistry;
    use crate::dsl::{CardDefinition, CardType, Effect, TriggeredEffect};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, Credits, GamePhase, InstalledRunnerCard, MemoryUnits, PlayerResources,
        RunnerState,
    };

    fn empty_state() -> GameState {
        GameState {
            corp: CorpState {
                resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
                ..Default::default()
            },
            runner: RunnerState {
                resources: PlayerResources { credits: Credits(5), clicks: Clicks(4), agenda_points: AgendaPoints(0) },
                memory_units: MemoryUnits(0),
                ..Default::default()
            },
            phase: GamePhase::Action(Side::Runner),
            ..Default::default()
        }
    }

    fn card_with_trigger(id: &str, side: Side, trigger: Trigger, effect: Effect) -> CardDefinition {
        CardDefinition {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type: CardType::Program,
            triggers: vec![TriggeredEffect { trigger, effects: vec![effect], requirement: None }],
            is_playable: true,
            ..Default::default()
        }
    }

    fn rig_card(id: &str) -> InstalledRunnerCard {
        InstalledRunnerCard {
            install_id: fixture_install_id(id),
            card: CardId(id.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn dispatch_only_fires_cards_with_a_matching_trigger() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger("reacts", Side::Runner, Trigger::OnTurnStart, Effect::GainCredits(Side::Runner, 1)));
        registry.insert(card_with_trigger("silent", Side::Runner, Trigger::OnPlay, Effect::GainCredits(Side::Runner, 99)));

        let mut state = empty_state();
        state.runner.rig = vec![rig_card("reacts"), rig_card("silent")];

        let events = dispatch_event(&mut state, &registry, &GameEvent::TurnStarted { side: Side::Runner, clicks: 4 }).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(6));
        assert_eq!(events, vec![GameEvent::TriggerFired { card: CardId("reacts".to_string()), trigger: Trigger::OnTurnStart }, GameEvent::CreditsGained { side: Side::Runner, amount: 1 }]);
    }

    /// A plan stops at `GameOver`: the rest of the reacting cards neither
    /// fire nor get queued. Before `win::end_game`/`resolution_halted`, a
    /// flatline mid-plan left the remaining triggers firing into a finished
    /// game — and an `Err` from one of them rejected the flatlining action.
    #[test]
    fn fire_plan_stops_once_the_game_is_over() {
        // Mixed sides so the order is fixed by rule (active side first) and
        // the plan fires straight through rather than parking a
        // `ChooseTriggerOrder` for one controller.
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger(
            "lethal",
            Side::Runner,
            Trigger::OnDamageAboutToResolve,
            Effect::DealDamage(crate::dsl::DamageType::Net, 5),
        ));
        registry.insert(card_with_trigger("payout", Side::Corp, Trigger::OnDamageAboutToResolve, Effect::GainCredits(Side::Corp, 1)));

        let mut state = empty_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.rig = vec![rig_card("lethal")];
        state.corp.installed = vec![crate::rules::state::InstalledCard {
            install_id: InstallId(1074),
            card: CardId("payout".to_string()),
            rezzed: true,
            ..Default::default()
        }];
        assert!(state.runner.grip.is_empty(), "any damage flatlines");

        let damage = GameEvent::DamageAboutToResolve { damage_type: crate::dsl::DamageType::Net, amount: 1 };
        let events = dispatch_event(&mut state, &registry, &damage).unwrap();

        assert_eq!(state.phase, GamePhase::GameOver(Side::Corp));
        assert_eq!(state.corp.resources.credits, Credits(5), "the second reactor never fired");
        assert!(state.deferred_triggers.is_empty(), "and was not queued for later either");
        assert_eq!(events.iter().filter(|e| matches!(e, GameEvent::GameOver { .. })).count(), 1);
        assert!(!events.iter().any(|e| matches!(e, GameEvent::CreditsGained { .. })));
    }

    #[test]
    fn dispatch_wires_up_previously_dead_on_run_start() {
        let mut registry = CardRegistry::new();
        let mut identity = card_with_trigger("runner_id", Side::Runner, Trigger::OnRunStart, Effect::GainCredits(Side::Runner, 2));
        identity.card_type = CardType::Identity;
        registry.insert(identity);

        let mut state = empty_state();
        state.runner.identity = Some(CardId("runner_id".to_string()));
        // `RunInitiated` is run-scoped (`still_applies`): the engine emits
        // it with the run already in `active_run`, so the fixture must too.
        state.active_run = Some(crate::rules::RunState { server: ServerId::Hq, ..Default::default() });

        let events = dispatch_event(&mut state, &registry, &GameEvent::RunInitiated { server: ServerId::Hq }).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(7));
        assert_eq!(events, vec![GameEvent::TriggerFired { card: CardId("runner_id".to_string()), trigger: Trigger::OnRunStart }, GameEvent::CreditsGained { side: Side::Runner, amount: 2 }]);
    }

    #[test]
    fn dispatch_wires_up_previously_dead_on_encounter() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger("ice_wall", Side::Corp, Trigger::OnEncounter, Effect::GainCredits(Side::Corp, 3)));

        let mut state = empty_state();
        let events = dispatch_event(
            &mut state,
            &registry,
            &GameEvent::IceEncountered { card_id: CardId("ice_wall".to_string()), strength: 1, subroutine_count: 0 },
        )
        .unwrap();

        assert_eq!(state.corp.resources.credits, Credits(8));
        assert_eq!(events, vec![GameEvent::TriggerFired { card: CardId("ice_wall".to_string()), trigger: Trigger::OnEncounter }, GameEvent::CreditsGained { side: Side::Corp, amount: 3 }]);
    }

    #[test]
    fn dispatch_wires_up_previously_dead_on_successful_run() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger("desperado", Side::Runner, Trigger::OnSuccessfulRun, Effect::GainCredits(Side::Runner, 1)));

        let mut state = empty_state();
        state.active_run = Some(crate::rules::RunState { server: ServerId::RnD, ..Default::default() });
        state.runner.rig = vec![rig_card("desperado")];

        let events = dispatch_event(&mut state, &registry, &GameEvent::RunSucceeded { server: ServerId::RnD }).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(6));
        assert_eq!(events, vec![GameEvent::TriggerFired { card: CardId("desperado".to_string()), trigger: Trigger::OnSuccessfulRun }, GameEvent::CreditsGained { side: Side::Runner, amount: 1 }]);
    }

    #[test]
    fn order_active_first_puts_the_active_sides_candidates_before_the_others() {
        let candidates = vec![
            (Side::Corp, CardId("corp_card".to_string())),
            (Side::Runner, CardId("runner_card".to_string())),
        ];

        let ordered = order_active_first(Side::Runner, candidates.clone());
        assert_eq!(ordered, vec![CardId("runner_card".to_string()), CardId("corp_card".to_string())]);

        let ordered = order_active_first(Side::Corp, candidates);
        assert_eq!(ordered, vec![CardId("corp_card".to_string()), CardId("runner_card".to_string())]);
    }

    #[test]
    fn order_active_first_is_stable_within_a_side() {
        let candidates = vec![
            (Side::Runner, CardId("first".to_string())),
            (Side::Corp, CardId("corp".to_string())),
            (Side::Runner, CardId("second".to_string())),
        ];

        let ordered = order_active_first(Side::Runner, candidates);
        assert_eq!(
            ordered,
            vec![CardId("first".to_string()), CardId("second".to_string()), CardId("corp".to_string())]
        );
    }

    #[test]
    fn dispatch_with_no_matching_trigger_is_a_harmless_no_op() {
        let mut state = empty_state();
        let events =
            dispatch_event(&mut state, &CardRegistry::new(), &GameEvent::RunJackedOut { server: ServerId::Hq }).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn damage_about_to_resolve_dispatches_on_damage_about_to_resolve_trigger() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger(
            "interface",
            Side::Runner,
            Trigger::OnDamageAboutToResolve,
            Effect::GainCredits(Side::Runner, 1),
        ));

        let mut state = empty_state();
        state.runner.rig = vec![crate::rules::state::InstalledRunnerCard {
            card: CardId("interface".to_string()),
            ..Default::default()
        }];

        let events = dispatch_event(
            &mut state,
            &registry,
            &GameEvent::DamageAboutToResolve { damage_type: crate::dsl::DamageType::Net, amount: 1 },
        )
        .unwrap();

        assert_eq!(state.runner.resources.credits, Credits(6));
        assert_eq!(events, vec![GameEvent::TriggerFired { card: CardId("interface".to_string()), trigger: Trigger::OnDamageAboutToResolve }, GameEvent::CreditsGained { side: Side::Runner, amount: 1 }]);
    }

    /// `DamageAboutToResolve`/`TrashAboutToResolve` are the only dispatches
    /// whose audience spans both sides, so they are the only ones where
    /// active-player-first is observable. It used to be ignored here:
    /// `both_sides_candidates` emitted Corp before Runner unconditionally.
    ///
    /// No card in the current pool declares either trigger, so this pins
    /// behavior that is unreachable in a real game today — it exists so the
    /// first card that does declare one resolves in rules order.
    #[test]
    fn damage_about_to_resolve_fires_the_active_sides_reactions_first() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger(
            "corp_reactor",
            Side::Corp,
            Trigger::OnDamageAboutToResolve,
            Effect::GainCredits(Side::Corp, 1),
        ));
        registry.insert(card_with_trigger(
            "runner_reactor",
            Side::Runner,
            Trigger::OnDamageAboutToResolve,
            Effect::GainCredits(Side::Runner, 1),
        ));

        let both_installed = |state: &mut GameState| {
            state.corp.installed = vec![crate::rules::state::InstalledCard {
                install_id: InstallId(1073),
                card: CardId("corp_reactor".to_string()),
                rezzed: true,
                ..Default::default()
            }];
            state.runner.rig = vec![rig_card("runner_reactor")];
        };
        let damage = GameEvent::DamageAboutToResolve { damage_type: crate::dsl::DamageType::Net, amount: 1 };

        let mut runner_turn = empty_state();
        runner_turn.phase = GamePhase::Action(Side::Runner);
        both_installed(&mut runner_turn);
        let events = dispatch_event(&mut runner_turn, &registry, &damage).unwrap();
        assert_eq!(
            events,
            vec![
                GameEvent::TriggerFired { card: CardId("runner_reactor".to_string()), trigger: crate::dsl::Trigger::OnDamageAboutToResolve },
                GameEvent::CreditsGained { side: Side::Runner, amount: 1 },
                GameEvent::TriggerFired { card: CardId("corp_reactor".to_string()), trigger: crate::dsl::Trigger::OnDamageAboutToResolve },
                GameEvent::CreditsGained { side: Side::Corp, amount: 1 },
            ],
            "on the Runner's turn the Runner's reaction resolves first"
        );

        let mut corp_turn = empty_state();
        corp_turn.phase = GamePhase::Action(Side::Corp);
        both_installed(&mut corp_turn);
        let events = dispatch_event(&mut corp_turn, &registry, &damage).unwrap();
        assert_eq!(
            events,
            vec![
                GameEvent::TriggerFired { card: CardId("corp_reactor".to_string()), trigger: Trigger::OnDamageAboutToResolve },
                GameEvent::CreditsGained { side: Side::Corp, amount: 1 },
                GameEvent::TriggerFired { card: CardId("runner_reactor".to_string()), trigger: Trigger::OnDamageAboutToResolve },
                GameEvent::CreditsGained { side: Side::Runner, amount: 1 },
            ],
            "on the Corp's turn the Corp's reaction resolves first"
        );
    }

    /// The *Clearinghouse* bug, in miniature: a trigger that parks a
    /// decision must not let later triggers in the same dispatch resolve
    /// underneath it. Clearinghouse's `OnTurnStart` is an
    /// `Effect::PresentChoice`, which parks a `PendingDecision`, and before
    /// the deferred-trigger queue every later reacting card fired anyway.
    ///
    /// Uses a deliberately **cross-side** audience
    /// (`DamageAboutToResolve`), so no `ChooseTriggerOrder` is offered —
    /// cross-side order is fixed by rule, not the player's to pick. That
    /// isolates the deferral guard from the ordering layer built on top of
    /// it; the same-side case is covered by
    /// `two_same_side_reacting_cards_let_their_controller_pick_the_order`.
    #[test]
    fn a_trigger_that_parks_a_decision_defers_the_rest_instead_of_firing_under_it() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger(
            "runner_parks_a_choice",
            Side::Runner,
            Trigger::OnDamageAboutToResolve,
            Effect::PresentChoice {
                chooser: Side::Runner,
                options: vec![Effect::GainCredits(Side::Runner, 5), Effect::Sequence(Vec::new())],
            },
        ));
        registry.insert(card_with_trigger(
            "corp_reactor",
            Side::Corp,
            Trigger::OnDamageAboutToResolve,
            Effect::GainCredits(Side::Corp, 1),
        ));

        let mut state = empty_state();
        state.phase = GamePhase::Action(Side::Runner);
        state.runner.rig = vec![rig_card("runner_parks_a_choice")];
        state.corp.installed = vec![crate::rules::state::InstalledCard {
            install_id: InstallId(1074),
            card: CardId("corp_reactor".to_string()),
            rezzed: true,
            ..Default::default()
        }];
        let corp_credits_before = state.corp.resources.credits;

        let damage = GameEvent::DamageAboutToResolve { damage_type: crate::dsl::DamageType::Net, amount: 1 };
        dispatch_event(&mut state, &registry, &damage).unwrap();

        assert!(state.pending_decision.is_some(), "the Runner's card parked its choice, resolving first");
        assert_eq!(
            state.corp.resources.credits, corp_credits_before,
            "the Corp's card must NOT have resolved underneath the pending choice"
        );
        assert_eq!(
            state.deferred_triggers,
            vec![DeferredTrigger {
                // …and the install it reacts as, so the deferred copy is the
                // one that was on the table, not the first with that name.
                install: Some(InstallId(1074)),
                target_install: None,
                card: CardId("corp_reactor".to_string()),
                trigger: Trigger::OnDamageAboutToResolve,
                target: None,
                // The queued trigger carries the event that fired it, so
                // when it eventually resolves it rebuilds the same
                // `ResolutionContext` it would have had immediately — a
                // requirement reading the triggering event cannot tell
                // whether it was deferred.
                event: Some(damage.clone()),
            }],
            "the untouched remainder is queued, not dropped"
        );
    }

    /// Two of one player's own cards reacting to the same event is the
    /// case the rules hand to that player: they pick the order. Reachable
    /// with any two Corp `OnTurnStart` assets (7 System Gateway cards carry
    /// that trigger).
    #[test]
    fn two_same_side_reacting_cards_let_their_controller_pick_the_order() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger(
            "pad_campaign",
            Side::Corp,
            Trigger::OnTurnStart,
            Effect::GainCredits(Side::Corp, 1),
        ));
        registry.insert(card_with_trigger(
            "nico_campaign",
            Side::Corp,
            Trigger::OnTurnStart,
            Effect::GainCredits(Side::Corp, 3),
        ));

        let mut state = empty_state();
        let rezzed = |id: &str| crate::rules::state::InstalledCard {
            install_id: InstallId(1075),
            card: CardId(id.to_string()),
            rezzed: true,
            ..Default::default()
        };
        state.corp.installed = vec![rezzed("pad_campaign"), rezzed("nico_campaign")];
        let credits_before = state.corp.resources.credits;

        dispatch_event(&mut state, &registry, &GameEvent::TurnStarted { side: Side::Corp, clicks: 3 }).unwrap();

        match state.pending_decision.as_ref() {
            Some(PendingDecision::ChooseTriggerOrder { chooser, pending, .. }) => {
                assert_eq!(*chooser, Side::Corp);
                assert_eq!(pending.len(), 2);
            }
            other => panic!("expected a ChooseTriggerOrder, got {other:?}"),
        }
        assert_eq!(state.corp.resources.credits, credits_before, "nothing resolves until the order is picked");
    }

    /// The cost guard: a single reacting card is not a choice, so no
    /// decision is parked. Without this the engine would interrupt for a
    /// one-option "decision" every time one card reacted to anything.
    #[test]
    fn a_single_reacting_card_fires_directly_with_no_order_decision() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger(
            "pad_campaign",
            Side::Corp,
            Trigger::OnTurnStart,
            Effect::GainCredits(Side::Corp, 1),
        ));

        let mut state = empty_state();
        let rezzed = |id: &str| crate::rules::state::InstalledCard {
            install_id: InstallId(1076),
            card: CardId(id.to_string()),
            rezzed: true,
            ..Default::default()
        };
        // A second install that declares no `OnTurnStart` at all — it is in
        // the candidate list but must not count toward "contestable".
        state.corp.installed = vec![rezzed("pad_campaign"), rezzed("inert_card")];
        let credits_before = state.corp.resources.credits;

        dispatch_event(&mut state, &registry, &GameEvent::TurnStarted { side: Side::Corp, clicks: 3 }).unwrap();

        assert!(state.pending_decision.is_none(), "one reacting card is no choice at all");
        assert_eq!(state.corp.resources.credits, credits_before.gain(1), "it just fired");
    }

    /// The queue is only a *deferral*, never a loss: draining it fires
    /// exactly what was owed, once the blockage is gone.
    #[test]
    fn draining_fires_the_queued_triggers_once_the_blockage_clears() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger(
            "pad_campaign",
            Side::Corp,
            Trigger::OnTurnStart,
            Effect::GainCredits(Side::Corp, 1),
        ));

        let mut state = empty_state();
        state.corp.installed = vec![crate::rules::state::InstalledCard {
            install_id: InstallId(1077),
            card: CardId("pad_campaign".to_string()),
            rezzed: true,
            ..Default::default()
        }];
        state.deferred_triggers = vec![DeferredTrigger { install: None, target_install: None,
            card: CardId("pad_campaign".to_string()),
            trigger: Trigger::OnTurnStart,
            target: None, event: None,
        }];
        let credits_before = state.corp.resources.credits;

        let events = drain_deferred_triggers(&mut state, &registry).unwrap();

        assert_eq!(state.corp.resources.credits, credits_before.gain(1));
        assert_eq!(events, vec![GameEvent::TriggerFired { card: CardId("pad_campaign".to_string()), trigger: Trigger::OnTurnStart }, GameEvent::CreditsGained { side: Side::Corp, amount: 1 }]);
        assert!(state.deferred_triggers.is_empty(), "a fully drained queue is left empty");
    }

    /// The seed-85 deadlock, at unit scale: a trigger queued against
    /// `RunSucceeded` must not fire once a sibling has ended the run. With
    /// a run still active the same trigger fires normally.
    #[test]
    fn a_deferred_approach_trigger_does_not_fire_once_the_run_has_ended() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger(
            "manegarm_skunkworks",
            Side::Corp,
            Trigger::OnApproachServer,
            Effect::GainCredits(Side::Corp, 1),
        ));
        let due = DeferredTrigger { install: None, target_install: None,
            card: CardId("manegarm_skunkworks".to_string()),
            trigger: Trigger::OnApproachServer,
            target: None,
            event: Some(GameEvent::ServerApproached { server: ServerId::Remote(0) }),
        };

        let mut ended = empty_state();
        ended.active_run = None;
        ended.deferred_triggers = vec![due.clone()];
        let before = ended.corp.resources.credits;
        let events = drain_deferred_triggers(&mut ended, &registry).unwrap();
        assert!(events.is_empty(), "nothing to react to once the run is over: {events:?}");
        assert_eq!(ended.corp.resources.credits, before);
        assert!(ended.deferred_triggers.is_empty(), "the stale trigger is dropped, not left queued");

        let mut live = empty_state();
        live.active_run = Some(crate::rules::RunState { server: ServerId::Remote(0), ..Default::default() });
        live.deferred_triggers = vec![due];
        drain_deferred_triggers(&mut live, &registry).unwrap();
        assert_eq!(live.corp.resources.credits, before.gain(1), "with the run still live it fires as before");
    }

    /// `OnRunEnded` reacts to the run *having* ended, so the staleness
    /// guard above must leave it alone.
    #[test]
    fn a_deferred_run_ended_trigger_still_fires_with_no_active_run() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger("mayfly", Side::Runner, Trigger::OnRunEnded, Effect::GainCredits(Side::Runner, 1)));
        let mut state = empty_state();
        state.active_run = None;
        state.runner.rig = vec![rig_card("mayfly")];
        state.deferred_triggers = vec![DeferredTrigger { install: None, target_install: None,
            card: CardId("mayfly".to_string()),
            trigger: Trigger::OnRunEnded,
            target: None,
            event: Some(GameEvent::RunEndedByEffect { server: ServerId::Hq }),
        }];
        let before = state.runner.resources.credits;
        drain_deferred_triggers(&mut state, &registry).unwrap();
        assert_eq!(state.runner.resources.credits, before.gain(1));
    }

    /// A deferred trigger must be indistinguishable from one that fired
    /// immediately, including for a requirement that reads the *event* that
    /// fired it.
    ///
    /// This is what `DeferredTrigger::event` exists for. Without it a
    /// deferred `OnAdvance` would resolve with no triggering event and
    /// `WasFirstAdvancementThisCard` would silently report "not first" for
    /// an advancement that was — the stale-read class of bug that removing
    /// `GameState::last_advancement_was_first` was meant to end, reappearing
    /// at the defer boundary.
    #[test]
    fn a_deferred_trigger_still_sees_the_event_that_fired_it() {
        let mut registry = CardRegistry::new();
        let mut card = card_with_trigger(
            "built_to_last",
            Side::Corp,
            Trigger::OnAdvance,
            Effect::GainCredits(Side::Corp, 2),
        );
        card.triggers[0].requirement = Some(crate::dsl::EffectRequirement::WasFirstAdvancementThisCard);
        registry.insert(card);

        let queue_with = |advancement_tokens: u32| DeferredTrigger { install: None, target_install: None,
            card: CardId("built_to_last".to_string()),
            trigger: Trigger::OnAdvance,
            target: None,
            event: Some(GameEvent::CardAdvanced {
                card: CardId("some_agenda".to_string()),
                advancement_tokens,
            }),
        };

        // First advancement: the requirement is met even though the trigger
        // is resolving a whole `PlayerAction` after it was queued.
        let mut state = empty_state();
        state.deferred_triggers = vec![queue_with(1)];
        let before = state.corp.resources.credits;
        drain_deferred_triggers(&mut state, &registry).unwrap();
        assert_eq!(state.corp.resources.credits, before.gain(2), "first advancement pays out");

        // Second advancement: the same deferred path must decline.
        let mut state = empty_state();
        state.deferred_triggers = vec![queue_with(2)];
        let before = state.corp.resources.credits;
        drain_deferred_triggers(&mut state, &registry).unwrap();
        assert_eq!(state.corp.resources.credits, before, "a later advancement pays nothing");

        // And a trigger queued with no event at all declines rather than
        // guessing — the honest answer when the context is genuinely absent.
        let mut state = empty_state();
        state.deferred_triggers = vec![DeferredTrigger { install: None, target_install: None,
            card: CardId("built_to_last".to_string()),
            trigger: Trigger::OnAdvance,
            target: None,
            event: None,
        }];
        let before = state.corp.resources.credits;
        drain_deferred_triggers(&mut state, &registry).unwrap();
        assert_eq!(state.corp.resources.credits, before, "no event, no payout");
    }

    /// A queued trigger that itself parks stops the drain and leaves
    /// everything after it queued — so a chain of parking triggers resolves
    /// one player decision at a time rather than dropping any.
    #[test]
    fn draining_stops_at_the_next_parking_trigger_and_keeps_the_rest_queued() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger(
            "parks_a_choice",
            Side::Corp,
            Trigger::OnTurnStart,
            Effect::PresentChoice {
                chooser: Side::Corp,
                options: vec![Effect::GainCredits(Side::Corp, 5), Effect::Sequence(Vec::new())],
            },
        ));
        registry.insert(card_with_trigger(
            "pad_campaign",
            Side::Corp,
            Trigger::OnTurnStart,
            Effect::GainCredits(Side::Corp, 1),
        ));

        let mut state = empty_state();
        let rezzed = |id: &str| crate::rules::state::InstalledCard {
            install_id: InstallId(1078),
            card: CardId(id.to_string()),
            rezzed: true,
            ..Default::default()
        };
        state.corp.installed = vec![rezzed("parks_a_choice"), rezzed("pad_campaign")];
        let queued = |id: &str| DeferredTrigger { install: None, target_install: None,
            card: CardId(id.to_string()),
            trigger: Trigger::OnTurnStart,
            target: None, event: None,
        };
        state.deferred_triggers = vec![queued("parks_a_choice"), queued("pad_campaign")];

        drain_deferred_triggers(&mut state, &registry).unwrap();

        assert!(state.pending_decision.is_some());
        assert_eq!(state.deferred_triggers, vec![queued("pad_campaign")], "the rest stays queued");
    }

    #[test]
    fn ice_rezzed_dispatches_on_rez_against_the_rezzed_card() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger("ping", Side::Corp, Trigger::OnRez, Effect::GiveTags(1)));

        let mut state = empty_state();
        let events =
            dispatch_event(&mut state, &registry, &GameEvent::IceRezzed { card: CardId("ping".to_string()), server: ServerId::Hq, install: InstallId::PLACEHOLDER })
                .unwrap();

        assert_eq!(state.runner.tags, 1);
        assert_eq!(events, vec![GameEvent::TriggerFired { card: CardId("ping".to_string()), trigger: Trigger::OnRez }, GameEvent::TagsGiven { side: Side::Runner, amount: 1 }]);
    }

    #[test]
    fn server_approached_dispatches_on_approach_server_against_rezzed_root_installs_only() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger("manegarm", Side::Corp, Trigger::OnApproachServer, Effect::GainCredits(Side::Corp, 3)));
        registry.insert(card_with_trigger("unrezzed_upgrade", Side::Corp, Trigger::OnApproachServer, Effect::GainCredits(Side::Corp, 99)));

        let mut state = empty_state();
        state.active_run = Some(crate::rules::RunState { server: ServerId::Hq, ..Default::default() });
        state.corp.installed = vec![
            crate::rules::state::InstalledCard {
                install_id: InstallId(1079),
                card: CardId("manegarm".to_string()),
                slot: crate::rules::state::InstallSlot::Root,
                rezzed: true,
                ..Default::default()
            },
            crate::rules::state::InstalledCard {
                install_id: InstallId(1080),
                card: CardId("unrezzed_upgrade".to_string()),
                slot: crate::rules::state::InstallSlot::Root,
                ..Default::default()
            },
        ];

        let events = dispatch_event(&mut state, &registry, &GameEvent::ServerApproached { server: ServerId::Hq }).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(8), "only the rezzed root install fired");
        assert!(events.contains(&GameEvent::CreditsGained { side: Side::Corp, amount: 3 }));
        assert!(!events.iter().any(|e| matches!(e, GameEvent::CreditsGained { amount: 99, .. })));
        assert!(!state.runner.made_successful_run_this_turn, "approaching is not succeeding");
    }

    #[test]
    fn run_completed_dispatches_on_run_ended_against_identity_and_rig() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger("mayfly", Side::Runner, Trigger::OnRunEnded, Effect::TrashCard(crate::dsl::CardTarget::ThisCard)));

        let mut state = empty_state();
        state.runner.rig = vec![rig_card("mayfly")];

        let events = dispatch_event(&mut state, &registry, &GameEvent::RunCompleted { server: ServerId::Hq }).unwrap();

        assert!(state.runner.rig.is_empty(), "mayfly should have trashed itself");
        assert!(state.runner.heap.contains(&CardId("mayfly".to_string())));
        assert!(events.iter().any(|e| matches!(e, GameEvent::CardTrashed { .. })));
    }

    #[test]
    fn run_jacked_out_and_run_ended_by_effect_also_dispatch_on_run_ended() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger("mayfly", Side::Runner, Trigger::OnRunEnded, Effect::GainCredits(Side::Runner, 1)));

        for event in [GameEvent::RunJackedOut { server: ServerId::Hq }, GameEvent::RunEndedByEffect { server: ServerId::Hq }] {
            let mut state = empty_state();
            state.runner.rig = vec![rig_card("mayfly")];
            let events = dispatch_event(&mut state, &registry, &event).unwrap();
            assert_eq!(state.runner.resources.credits, Credits(6), "{event:?} should dispatch OnRunEnded");
            assert!(events.contains(&GameEvent::CreditsGained { side: Side::Runner, amount: 1 }));
        }
    }

    #[test]
    fn on_run_ended_still_reaches_only_the_runner_side_when_no_corp_root_card_reacts() {
        // Guards the `OnRunEnded` audience widening (Corp Root installs on
        // the ended run's server + `persistent_trashed_upgrades`) against
        // regressing the pre-existing Runner-side consumers (Mayfly, Zahya).
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger("mayfly", Side::Runner, Trigger::OnRunEnded, Effect::GainCredits(Side::Runner, 1)));
        let mut inert = card_with_trigger("pad_campaign", Side::Corp, Trigger::OnTurnStart, Effect::GainCredits(Side::Corp, 1));
        inert.card_type = CardType::Asset;
        registry.insert(inert);

        let mut state = empty_state();
        state.runner.rig = vec![rig_card("mayfly")];
        state.corp.installed = vec![crate::rules::InstalledCard {
            install_id: InstallId(1081),
            card: CardId("pad_campaign".to_string()),
            rezzed: true,
            ..Default::default()
        }];
        state.last_completed_run = Some(crate::rules::state::CompletedRun {
            server: ServerId::Hq,
            cards_accessed: 0,
            agendas_stolen: 0,
            persistent_trashed_upgrades: Vec::new(),
        });

        dispatch_event(&mut state, &registry, &GameEvent::RunCompleted { server: ServerId::Hq }).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(6), "the Runner-side OnRunEnded consumer still fires");
        assert_eq!(state.corp.resources.credits, Credits(5), "a Corp Root card with no OnRunEnded trigger stays inert");
    }

    #[test]
    fn tags_given_to_the_runner_dispatches_on_tags_given_against_the_corp_identity() {
        let mut registry = CardRegistry::new();
        let mut identity =
            card_with_trigger("nbn_reality_plus", Side::Corp, Trigger::OnTagsGiven, Effect::GainCredits(Side::Corp, 2));
        identity.card_type = CardType::Identity;
        registry.insert(identity);

        let mut state = empty_state();
        state.corp.identity = Some(CardId("nbn_reality_plus".to_string()));

        let events =
            dispatch_event(&mut state, &registry, &GameEvent::TagsGiven { side: Side::Runner, amount: 1 }).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(7));
        assert!(events.contains(&GameEvent::CreditsGained { side: Side::Corp, amount: 2 }));
    }

    #[test]
    fn tags_given_to_the_corp_does_not_dispatch_on_tags_given() {
        // This engine has no mechanic that gives the Corp tags, but the
        // dispatcher arm is deliberately scoped to `side: Side::Runner`
        // only — confirm a (hypothetical) Corp-side TagsGiven is a no-op.
        let mut state = empty_state();
        let events =
            dispatch_event(&mut state, &CardRegistry::new(), &GameEvent::TagsGiven { side: Side::Corp, amount: 1 }).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn basic_draw_action_taken_dispatches_on_basic_draw_action() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger(
            "verbal_plasticity",
            Side::Runner,
            Trigger::OnBasicDrawAction,
            Effect::DrawCards(Side::Runner, 1),
        ));

        let mut state = empty_state();
        state.runner.rig = vec![rig_card("verbal_plasticity")];
        state.runner.stack = vec![CardId("extra_card".to_string())];

        let events =
            dispatch_event(&mut state, &registry, &GameEvent::BasicDrawActionTaken { side: Side::Runner }).unwrap();

        assert_eq!(state.runner.grip, vec![CardId("extra_card".to_string())]);
        assert!(events.contains(&GameEvent::CardDrawn { side: Side::Runner }));
    }
}
