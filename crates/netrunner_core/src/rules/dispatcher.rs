//! Central event-driven trigger dispatch.
//!
//! `dispatch_event` fires what an event triggers. *Which* cards that is, and
//! in what order, is not decided here: `listeners::plan_for` reads it off
//! the cards — see that module for the rule. This module is the firing: one
//! side's triggers at a time, the order of a side's own simultaneous
//! triggers offered to that side, and whatever a parked decision interrupts
//! queued on `GameState::deferred_triggers` until it clears.
//!
//! **This used to be the audience too** — a `match` that named, for each
//! `GameEvent`, the cards to ask, in up to four separately fired steps (the
//! scored agenda, then the identity, then the rezzed table, then the
//! Runner's side). Three things were wrong with that beyond its size, and
//! running the listener scan beside it over both 256-seed sweeps is what
//! found them (ROADMAP Rules Audit, jinteki comparison item 1):
//!
//! - **An agenda installed faceup heard another agenda being scored.** The
//!   rezzed table was asked `OnAgendaScored`, BANGUN installs agendas
//!   faceup, and "when you score **this** agenda" had no way to say *this*:
//!   Orbital Superiority did its 4 meat damage from a remote when a
//!   different agenda was scored.
//! - **A side's simultaneous triggers were the player's to order only
//!   within one step.** The scored agenda and the identity were two steps,
//!   so their order was fixed, and a step fired by `fire_direct` skipped
//!   the blocked-resolution guard altogether and resolved under whatever
//!   the step before it had parked.
//! - **The active player's triggers did not always come first.** A stolen
//!   agenda's own reaction, and a rezzed card's, resolved before the
//!   Runner's during the Runner's turn.
//!
//! There is deliberately no registry of "active behaviours" kept beside
//! `GameState`: `CorpState::installed`, `RunnerState::rig` and the score
//! area are already the truth about what is in play, so the plan is
//! re-derived from them on every call, the way `win::check_win_conditions`
//! re-derives a win.

use crate::cards::CardRegistry;
use crate::dsl::Trigger;
use crate::rules::ability;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::listeners;
use crate::rules::state::{DeferredTrigger, GameState, PendingDecision, Side};

/// Fires every trigger `event` is an occurrence of and returns the
/// resulting `GameEvent`s, in firing order.
///
/// Most events are an occurrence of nothing (`listeners::moments` lists
/// them by name), which is not an error: `Ok(Vec::new())`.
pub fn dispatch_event(
    state: &mut GameState,
    registry: &CardRegistry,
    event: &GameEvent,
) -> Result<Vec<GameEvent>, RulesError> {
    let mut events = resolve_run_riders(state, registry, event)?;
    // Planned after the riders: an access bonus or a trash they resolve is
    // part of the state the triggers happen in.
    let plan = listeners::plan_for(state, registry, event);
    // One side at a time, the active player's first (the plan's order). A
    // side orders its own simultaneous triggers; the order *between* the
    // sides is the rules', never a choice.
    //
    // `OnPlay` is a step of its own, ahead of its side's triggers: it is how
    // the DSL spells an event's or operation's *resolution*, which is not a
    // triggered ability and so is nobody's to order against one. Hedge Fund
    // under Building a Better World is not a question.
    let step = |(side, due): &(Side, DeferredTrigger)| (*side, due.trigger == Trigger::OnPlay);
    let mut remaining = plan.as_slice();
    while let Some(first) = remaining.first() {
        let (side, _) = step(first);
        let length = remaining.iter().take_while(|entry| step(entry) == step(first)).count();
        let (group, rest) = remaining.split_at(length);
        let group: Vec<DeferredTrigger> = group.iter().map(|(_, due)| due.clone()).collect();
        events.extend(fire_plan(state, registry, side, &group)?);
        remaining = rest;
    }
    Ok(events)
}

/// The two effects a run carries for itself rather than on a card:
/// `RunState::on_success_effect` ("if successful, …" on an Event such as
/// Jailbreak, which is never installed and so can declare no trigger) and
/// `CompletedRun::on_end_effect` (`Effect::SetRunEndedEffect`, Charm
/// Offensive). Each is taken, so it resolves once, and resolves before the
/// cards' own triggers — an access bonus has to be in place for the breach
/// the same success begins.
///
/// They are here, not in `listeners`, because they are not an audience:
/// nothing is listening, the run is doing what it was told to.
fn resolve_run_riders(
    state: &mut GameState,
    registry: &CardRegistry,
    event: &GameEvent,
) -> Result<Vec<GameEvent>, RulesError> {
    match event {
        GameEvent::RunSucceeded { .. } => {
            state.runner.made_successful_run_this_turn = true;
            let rider = state.active_run.as_mut().and_then(|run| {
                run.on_success_effect.take().map(|effect| (effect, run.on_success_card.take(), run.on_success_install.take()))
            });
            let Some((effect, card, install)) = rider else { return Ok(Vec::new()) };
            let mut ctx = ability::ResolutionContext::for_install_trigger(install, card.as_ref(), Some(event));
            ability::evaluate_effect(state, &effect, &mut ctx, registry)
        }
        // Read off the snapshot: `active_run` is already cleared by the time
        // a run's end is dispatched.
        GameEvent::RunCompleted { .. } | GameEvent::RunJackedOut { .. } | GameEvent::RunEndedByEffect { .. } => {
            let rider = state.last_completed_run.as_mut().and_then(|completed| {
                completed.on_end_effect.take().map(|effect| (effect, completed.on_end_card.clone(), completed.on_end_install))
            });
            let Some((effect, card, install)) = rider else { return Ok(Vec::new()) };
            let mut ctx = ability::ResolutionContext::for_parked(install, card.as_ref());
            ability::evaluate_effect(state, &effect, &mut ctx, registry)
        }
        _ => Ok(Vec::new()),
    }
}

/// Fires one side's ordered plan of triggers, stopping and queueing the
/// untouched remainder the moment one of them parks something blocking (a
/// decision, a paid choice, a prevention window, a trace) rather than
/// firing the rest underneath it. `drain_deferred_triggers` picks them back
/// up once the blockage clears.
///
/// Before the queue existed there was no such guard, so *Clearinghouse*'s
/// `OnTurnStart` (a `PresentChoice`, which parks a `PendingDecision`) let
/// every later Corp `OnTurnStart` card resolve during its pending choice.
///
/// A flat `(card, trigger)` plan rather than a list of cards, because one
/// event can be several triggers to one card — a successful run on HQ is up
/// to four — and a blockage can land *between* two of them. With the whole
/// plan built up front, "what's left" is a slice in every case.
fn fire_plan(
    state: &mut GameState,
    registry: &CardRegistry,
    side: Side,
    plan: &[DeferredTrigger],
) -> Result<Vec<GameEvent>, RulesError> {
    if let Some(events) = offer_trigger_order(state, registry, side, plan) {
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

/// Parks a `PendingDecision::ChooseTriggerOrder` if `side` has two or more
/// triggers in `plan` that would fire, returning `Some` to say the dispatch
/// has been handed to the player.
///
/// Real Netrunner gives a player the order of their own simultaneous
/// triggers, and only theirs — which is why a plan is one side's.
///
/// **What counts is a trigger whose requirement passes now**
/// (`ability::would_fire`). This used to count every card that declared the
/// trigger, on the argument that offering a no-op was harmless and picking
/// an order the player was owed was not. That held while an order was only
/// ever offered inside one hand-written step; with a side's whole plan in
/// one place it asked the Corp to order Hostile Takeover against a Malapert
/// Data Vault in another server, on every score. A condition that is false
/// when the event happens has not triggered, so there is no order owed.
/// The plan itself is not thinned: an entry that does not count still fires
/// in its turn, where its requirement is checked again.
fn offer_trigger_order(state: &mut GameState, registry: &CardRegistry, side: Side, plan: &[DeferredTrigger]) -> Option<Vec<GameEvent>> {
    if state.resolution_halted() {
        return None;
    }
    let (live, idle): (Vec<DeferredTrigger>, Vec<DeferredTrigger>) =
        plan.iter().cloned().partition(|due| ability::would_fire(state, registry, due));
    if live.len() < 2 {
        return None;
    }
    // `ChooseTriggerToResolve` is indexed by position in `live`, and
    // `ActionSpace` reserves `CHOOSE_TRIGGER_LEN` slots for it. Each card
    // can contribute several entries (one per success-trigger variant it
    // declares), so the bound is not "installed cards" — this is where an
    // overrun would first become visible, ahead of the index sweep's
    // "no index for a legal action" panic.
    debug_assert!(
        live.len() <= crate::rules::action_mask::CHOOSE_TRIGGER_LEN,
        "{} simultaneous triggers exceed the ActionSpace segment ({})",
        live.len(),
        crate::rules::action_mask::CHOOSE_TRIGGER_LEN
    );
    // What does not count is still owed its turn: queued, so it drains
    // after the ordered ones and re-checks its requirement then.
    state.deferred_triggers.extend(idle);
    state.pending_decision = Some(PendingDecision::ChooseTriggerOrder {
        chooser: side,
        pending: live,
        resume: crate::rules::state::PendingChoiceResume::None,
    });
    Some(vec![GameEvent::TriggerOrderPending { chooser: side }])
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
    // A queued continuation (the rest of a `Sequence`) resolves as the
    // card it was pinned to, with the event it had — no trigger lookup, no
    // requirement, no `TriggerFired`: the trigger already fired.
    if let Some(effect) = &due.continuation {
        let mut ctx = ability::ResolutionContext::for_install_trigger(due.install, Some(&due.card), due.event.as_ref());
        return ability::evaluate_effect(state, effect, &mut ctx, registry);
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
    // A trigger pinned to an install that has since left play stands
    // down: "Knickknack" O'Brian trashing Side Hustle at the start of a run
    // both reacted to left Side Hustle's deferred `OnRunStart` firing on a
    // card that was no longer in the rig, and its `AddCounters` errored —
    // which failed the *Knickknack* selection that trashed it, leaving the
    // Runner a decision whose only resolving action was illegal (the
    // *Elevation* Stage 3 deep sweep, seed 182). A persistent-after-trash
    // upgrade fires with no install pinned, so it is unaffected.
    // A continuation is exempt: it is the rest of an ability that already
    // started resolving, and an ability resolves to its end even when its
    // source leaves play mid-way — Humanoid Resources trashes itself as
    // part of its own cost and then installs and plays cards.
    let present = match due.install {
        Some(install) if due.continuation.is_none() => {
            state.runner.rig.iter().any(|c| c.install_id == install)
                || state.corp.installed.iter().any(|c| c.install_id == install)
                || state.corp.find_scored(install).is_some()
        }
        _ => true,
    };
    // A bypass ends the encounter's "when encountered" step for everyone
    // (Fransofia Ward's parenthetical): a reaction to the encounter that was
    // waiting behind the bypass offer no longer applies.
    let bypassed = due.trigger == Trigger::OnEncounter
        && matches!(due.event, Some(GameEvent::IceEncountered { .. }))
        && state.active_run.as_ref().is_some_and(|run| run.ice_bypassed);
    !state.is_over() && present && !bypassed && (!run_scoped || state.active_run.is_some())
}

/// Fires whatever `fire_plan` had to queue, once whatever blocked it has
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::state::InstallId;
    use crate::rules::test_support::fixture_install_id;
    use crate::cards::CardRegistry;
    use crate::dsl::{CardDefinition, CardId, CardType, Effect, TriggeredEffect};
    use crate::rules::run::ServerId;
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
            triggers: vec![TriggeredEffect { subject: None, text: None, trigger, effects: vec![effect], requirement: None }],
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
        assert_eq!(events, vec![GameEvent::TriggerFired { card: CardId("reacts".to_string()), trigger: Trigger::OnTurnStart }, GameEvent::CreditsGained { side: Side::Runner, amount: 1 }, GameEvent::AbilityGainedCredits { side: Side::Runner, card: CardId("reacts".to_string()) }]);
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
        assert_eq!(events, vec![GameEvent::TriggerFired { card: CardId("runner_id".to_string()), trigger: Trigger::OnRunStart }, GameEvent::CreditsGained { side: Side::Runner, amount: 2 }, GameEvent::AbilityGainedCredits { side: Side::Runner, card: CardId("runner_id".to_string()) }]);
    }

    #[test]
    fn dispatch_wires_up_previously_dead_on_encounter() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger("ice_wall", Side::Corp, Trigger::OnEncounter, Effect::GainCredits(Side::Corp, 3)));

        let mut state = empty_state();
        // An encounter happens in a run: a "when encountered" with no run
        // to apply to stands down (`still_applies`).
        state.active_run = Some(crate::rules::RunState { server: ServerId::Hq, ..Default::default() });
        let events = dispatch_event(
            &mut state,
            &registry,
            &GameEvent::IceEncountered { card_id: CardId("ice_wall".to_string()), strength: 1, subroutine_count: 0 },
        )
        .unwrap();

        assert_eq!(state.corp.resources.credits, Credits(8));
        assert_eq!(events, vec![GameEvent::TriggerFired { card: CardId("ice_wall".to_string()), trigger: Trigger::OnEncounter }, GameEvent::CreditsGained { side: Side::Corp, amount: 3 }, GameEvent::AbilityGainedCredits { side: Side::Corp, card: CardId("ice_wall".to_string()) }]);
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
        assert_eq!(events, vec![GameEvent::TriggerFired { card: CardId("desperado".to_string()), trigger: Trigger::OnSuccessfulRun }, GameEvent::CreditsGained { side: Side::Runner, amount: 1 }, GameEvent::AbilityGainedCredits { side: Side::Runner, card: CardId("desperado".to_string()) }]);
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
        assert_eq!(events, vec![GameEvent::TriggerFired { card: CardId("interface".to_string()), trigger: Trigger::OnDamageAboutToResolve }, GameEvent::CreditsGained { side: Side::Runner, amount: 1 }, GameEvent::AbilityGainedCredits { side: Side::Runner, card: CardId("interface".to_string()) }]);
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
                GameEvent::AbilityGainedCredits { side: Side::Runner, card: CardId("runner_reactor".to_string()) },
                GameEvent::TriggerFired { card: CardId("corp_reactor".to_string()), trigger: crate::dsl::Trigger::OnDamageAboutToResolve },
                GameEvent::CreditsGained { side: Side::Corp, amount: 1 },
                GameEvent::AbilityGainedCredits { side: Side::Corp, card: CardId("corp_reactor".to_string()) },
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
                GameEvent::AbilityGainedCredits { side: Side::Corp, card: CardId("corp_reactor".to_string()) },
                GameEvent::TriggerFired { card: CardId("runner_reactor".to_string()), trigger: Trigger::OnDamageAboutToResolve },
                GameEvent::CreditsGained { side: Side::Runner, amount: 1 },
                GameEvent::AbilityGainedCredits { side: Side::Runner, card: CardId("runner_reactor".to_string()) },
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
                texts: Vec::new(),
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
                continuation: None,
                // …and what it heard the event as, decided when it happened.
                heard: crate::rules::state::Heard::AsBystander,
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
            continuation: None,
            heard: Default::default(),
        }];
        let credits_before = state.corp.resources.credits;

        let events = drain_deferred_triggers(&mut state, &registry).unwrap();

        assert_eq!(state.corp.resources.credits, credits_before.gain(1));
        assert_eq!(events, vec![GameEvent::TriggerFired { card: CardId("pad_campaign".to_string()), trigger: Trigger::OnTurnStart }, GameEvent::CreditsGained { side: Side::Corp, amount: 1 }, GameEvent::AbilityGainedCredits { side: Side::Corp, card: CardId("pad_campaign".to_string()) }]);
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
            continuation: None,
            heard: Default::default(),
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
            continuation: None,
            heard: Default::default(),
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
                install: crate::rules::InstallId::PLACEHOLDER,
                card: Some(CardId("some_agenda".to_string())),
                advancement_tokens,
            }),
            continuation: None,
            heard: Default::default(),
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
            continuation: None,
            heard: Default::default(),
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
                texts: Vec::new(),
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
            continuation: None,
            heard: Default::default(),
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
        // Pinned to a copy that is on the table: a reaction whose install
        // has left play stands down.
        let install = fixture_install_id("ping");
        state.corp.installed.push(crate::rules::state::InstalledCard {
            install_id: install,
            card: CardId("ping".to_string()),
            rezzed: true,
            ..Default::default()
        });
        let events =
            dispatch_event(&mut state, &registry, &GameEvent::IceRezzed { card: CardId("ping".to_string()), server: ServerId::Hq, install })
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
            accessed_cards: Vec::new(), on_end_effect: None, on_end_card: None, on_end_install: None,
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
