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
use crate::rules::state::{GameState, InstallSlot, Side};

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
            ability::process_card_triggers(state, registry, card, Trigger::OnPlay)
        }

        GameEvent::OperationPlayed { card, .. } => {
            let mut events = ability::process_card_triggers(state, registry, card, Trigger::OnPlay)?;
            let is_transaction =
                registry.get(card).is_some_and(|c| c.subtypes.contains(&CardSubtype::Transaction));
            if is_transaction && let Some(identity) = state.corp.identity.clone() {
                events.extend(ability::process_card_triggers(state, registry, &identity, Trigger::OnTransactionPlayed)?);
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
            let mut events = ability::process_card_triggers(state, registry, card, Trigger::OnInstall)?;
            let is_virus = registry.get(card).is_some_and(|c| c.subtypes.contains(&CardSubtype::Virus));
            if is_virus {
                if let Some(identity) = state.runner.identity.clone() {
                    events.extend(ability::process_card_triggers(state, registry, &identity, Trigger::OnVirusInstalled)?);
                }
                // Every OTHER rig card also gets a chance to react to a
                // virus install, but — unlike the identity reaction above
                // — its effect targets the just-installed virus program
                // itself, not the reacting card. e.g. Cookbook's "you may
                // place 1 virus counter on it." Excludes `card` itself to
                // avoid a virus program reacting to its own installation.
                let other_rig_cards: Vec<CardId> =
                    state.runner.rig.iter().map(|c| c.card.clone()).filter(|id| id != card).collect();
                for owner in other_rig_cards {
                    events.extend(ability::process_card_triggers_targeting(
                        state,
                        registry,
                        &owner,
                        Trigger::OnVirusInstalled,
                        card,
                    )?);
                }
            }
            Ok(events)
        }

        GameEvent::CardInstalled { side, .. } => {
            let identity = match side {
                Side::Corp => state.corp.identity.clone(),
                Side::Runner => state.runner.identity.clone(),
            };
            match identity {
                Some(identity) => ability::process_card_triggers(state, registry, &identity, Trigger::OnInstall),
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
            let mut events = ability::process_card_triggers(state, registry, card, Trigger::OnInstall)?;
            if let Some(identity) = state.runner.identity.clone() {
                events.extend(ability::process_card_triggers(state, registry, &identity, Trigger::OnInstall)?);
            }
            Ok(events)
        }

        GameEvent::CardAccessed { card, .. } => {
            ability::process_card_triggers(state, registry, card, Trigger::OnAccessed)
        }

        GameEvent::CardTrashedFromAccess { card, .. } => {
            // Only the Runner ever accesses and trashes a card this way, so
            // the identity to react is unambiguously theirs — mirrors
            // `AgendaScored`'s "also fire the owning identity" widening
            // below, e.g. René "Loup" Arcemont's "the first time each turn
            // you trash a card you are accessing, gain 1 credit and draw 1
            // card."
            let mut events = ability::process_card_triggers(state, registry, card, Trigger::OnTrashedFromAccess)?;
            if let Some(identity) = state.runner.identity.clone() {
                events.extend(ability::process_card_triggers(state, registry, &identity, Trigger::OnTrashedFromAccess)?);
            }
            Ok(events)
        }

        GameEvent::AgendaScored { card, server, .. } => {
            let mut events = ability::process_card_triggers(state, registry, card, Trigger::OnAgendaScored)?;
            if let Some(identity) = state.corp.identity.clone() {
                events.extend(ability::process_card_triggers(state, registry, &identity, Trigger::OnAgendaScored)?);
            }
            // Every other rezzed Root-slot install on the scored agenda's
            // own server also gets a chance to react — e.g. Malapert Data
            // Vault's "whenever you score an agenda from the root of this
            // server." Same audience-computation shape as `OnApproachServer`
            // above (rezzed Root installs on a given server), reused here
            // rather than a bespoke `EffectRequirement`.
            let root_installs: Vec<CardId> = state
                .corp
                .installed
                .iter()
                .filter(|installed| {
                    installed.rezzed && installed.server == *server && installed.slot == InstallSlot::Root
                })
                .map(|installed| installed.card.clone())
                .collect();
            events.extend(fire_each(state, registry, &root_installs, Trigger::OnAgendaScored)?);
            // Runner-side widening (M5): the Runner's own identity and rig
            // also get a chance to react to a Corp agenda score — e.g.
            // Pantograph's "whenever an agenda is scored or stolen, gain 1
            // credit." Previously deferred (see `AgendaStolen`'s own
            // matching widening below) until a real card needed it.
            events.extend(fire_runner_side(state, registry, Trigger::OnAgendaScored)?);
            Ok(events)
        }

        GameEvent::AgendaStolen { card, .. } => {
            // Fires against the stolen agenda's own trigger (e.g. Send a
            // Message's "when this agenda is scored or stolen...") in
            // addition to the Corp identity's — mirrors `AgendaScored`'s
            // own "also fire the card itself" shape, which `AgendaStolen`
            // was previously missing.
            let mut events = ability::process_card_triggers(state, registry, card, Trigger::OnAgendaStolen)?;
            if let Some(identity) = state.corp.identity.clone() {
                events.extend(ability::process_card_triggers(state, registry, &identity, Trigger::OnAgendaStolen)?);
            }
            // Runner-side widening (M5): the Runner's own identity and rig
            // react to their own steal — e.g. Tāo Salonga: Telepresence
            // Magician, Pantograph's "whenever an agenda is scored or
            // stolen, gain 1 credit."
            events.extend(fire_runner_side(state, registry, Trigger::OnAgendaStolen)?);
            Ok(events)
        }

        GameEvent::TurnStarted { side, .. } => {
            let candidates: Vec<CardId> = match side {
                Side::Corp => state
                    .corp
                    .installed
                    .iter()
                    .filter(|installed| installed.rezzed)
                    .map(|installed| installed.card.clone())
                    .collect(),
                Side::Runner => state.runner.rig.iter().map(|card| card.card.clone()).collect(),
            };
            fire_each(state, registry, &candidates, Trigger::OnTurnStart)
        }

        GameEvent::RunInitiated { .. } => match state.runner.identity.clone() {
            Some(identity) => ability::process_card_triggers(state, registry, &identity, Trigger::OnRunStart),
            None => Ok(Vec::new()),
        },

        GameEvent::IceEncountered { card_id, .. } => {
            ability::process_card_triggers(state, registry, card_id, Trigger::OnEncounter)
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
            let on_success = state.active_run.as_mut().and_then(|run| run.on_success_effect.take());
            let mut events = Vec::new();
            if let Some(effect) = on_success {
                events.extend(ability::evaluate_effect(state, &effect, None, registry)?);
            }

            let mut candidates: Vec<(Side, CardId)> = Vec::new();
            if let Some(identity) = state.runner.identity.clone() {
                candidates.push((Side::Runner, identity));
            }
            candidates.extend(state.runner.rig.iter().map(|card| (Side::Runner, card.card.clone())));

            for card_id in order_active_first(Side::Runner, candidates) {
                events.extend(ability::process_card_triggers(state, registry, &card_id, Trigger::OnSuccessfulRun)?);
                if *server == ServerId::Hq {
                    events.extend(ability::process_card_triggers(state, registry, &card_id, Trigger::OnSuccessfulRunOnHq)?);
                }
                if *server == ServerId::RnD {
                    events.extend(ability::process_card_triggers(state, registry, &card_id, Trigger::OnSuccessfulRunOnRnD)?);
                }
                if matches!(server, ServerId::Hq | ServerId::RnD | ServerId::Archives) {
                    events.extend(ability::process_card_triggers(
                        state,
                        registry,
                        &card_id,
                        Trigger::OnSuccessfulRunOnCentralServer,
                    )?);
                }
            }

            // `Trigger::OnApproachServer` deliberately reuses this same
            // event (see the trigger's own doc comment) — audience is every
            // rezzed Corp Root-slot install in `server` (an Upgrade/Asset
            // sitting in its root), not the ICE and not the identity, e.g.
            // Manegarm Skunkworks/Anoetic Void.
            let root_installs: Vec<CardId> = state
                .corp
                .installed
                .iter()
                .filter(|installed| {
                    installed.rezzed && installed.server == *server && installed.slot == InstallSlot::Root
                })
                .map(|installed| installed.card.clone())
                .collect();
            events.extend(fire_each(state, registry, &root_installs, Trigger::OnApproachServer)?);

            Ok(events)
        }

        GameEvent::IceRezzed { card, .. } => {
            ability::process_card_triggers(state, registry, card, Trigger::OnRez)
        }

        GameEvent::CardAdvanced { .. } => match state.corp.identity.clone() {
            Some(identity) => ability::process_card_triggers(state, registry, &identity, Trigger::OnAdvance),
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
            let mut candidates: Vec<CardId> = Vec::new();
            if let Some(identity) = state.runner.identity.clone() {
                candidates.push(identity);
            }
            candidates.extend(state.runner.rig.iter().map(|card| card.card.clone()));
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
                        .map(|installed| installed.card.clone()),
                );
                candidates.extend(completed.persistent_trashed_upgrades.iter().cloned());
            }
            fire_each(state, registry, &candidates, Trigger::OnRunEnded)
        }

        // "When your discard phase ends" — fires against that side's own
        // identity only (the Corp's, for Jinteki: Restoring Humanity).
        GameEvent::DiscardPhaseEnded { side } => {
            let identity = match side {
                Side::Corp => state.corp.identity.clone(),
                Side::Runner => state.runner.identity.clone(),
            };
            match identity {
                Some(identity) => ability::process_card_triggers(state, registry, &identity, Trigger::OnDiscardPhaseEnd),
                None => Ok(Vec::new()),
            }
        }

        GameEvent::TagsGiven { side: Side::Runner, .. } => match state.corp.identity.clone() {
            Some(identity) => ability::process_card_triggers(state, registry, &identity, Trigger::OnTagsGiven),
            None => Ok(Vec::new()),
        },

        GameEvent::BasicDrawActionTaken { side } => {
            let identity = match side {
                Side::Corp => state.corp.identity.clone(),
                Side::Runner => state.runner.identity.clone(),
            };
            let mut candidates: Vec<CardId> = identity.into_iter().collect();
            if *side == Side::Runner {
                candidates.extend(state.runner.rig.iter().map(|card| card.card.clone()));
            }
            fire_each(state, registry, &candidates, Trigger::OnBasicDrawAction)
        }

        GameEvent::DamageAboutToResolve { .. } => fire_each(state, registry, &both_sides_candidates(state), Trigger::OnDamageAboutToResolve),

        GameEvent::TrashAboutToResolve { .. } => fire_each(state, registry, &both_sides_candidates(state), Trigger::OnTrashAboutToResolve),

        _ => Ok(Vec::new()),
    }
}

/// Rezzed Corp installs ∪ full Runner rig — the same audience `TurnStarted`'s
/// arm collects per-side, unioned here since a prevention trigger could in
/// principle belong to either side.
fn both_sides_candidates(state: &GameState) -> Vec<CardId> {
    state
        .corp
        .installed
        .iter()
        .filter(|installed| installed.rezzed)
        .map(|installed| installed.card.clone())
        .chain(state.runner.rig.iter().map(|card| card.card.clone()))
        .collect()
}

/// Fires `trigger` against each of `candidates` in order, collecting events.
fn fire_each(
    state: &mut GameState,
    registry: &CardRegistry,
    candidates: &[CardId],
    trigger: Trigger,
) -> Result<Vec<GameEvent>, RulesError> {
    let mut events = Vec::new();
    for card_id in candidates {
        events.extend(ability::process_card_triggers(state, registry, card_id, trigger)?);
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
) -> Result<Vec<GameEvent>, RulesError> {
    let mut candidates: Vec<CardId> = state.runner.identity.iter().cloned().collect();
    candidates.extend(state.runner.rig.iter().map(|c| c.card.clone()));
    fire_each(state, registry, &candidates, trigger)
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
fn order_active_first(active: Side, mut candidates: Vec<(Side, CardId)>) -> Vec<CardId> {
    candidates.sort_by_key(|(side, _)| *side != active);
    candidates.into_iter().map(|(_, id)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            pending_prevention: None, pending_paid_choice: None, pending_decision: None, last_discarded_cards: Vec::new(), last_completed_run: None, last_advancement_was_first: false,
            seed: 0,
            rng_step: 0,
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
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Runner, amount: 1 }]);
    }

    #[test]
    fn dispatch_wires_up_previously_dead_on_run_start() {
        let mut registry = CardRegistry::new();
        let mut identity = card_with_trigger("runner_id", Side::Runner, Trigger::OnRunStart, Effect::GainCredits(Side::Runner, 2));
        identity.card_type = CardType::Identity;
        registry.insert(identity);

        let mut state = empty_state();
        state.runner.identity = Some(CardId("runner_id".to_string()));

        let events = dispatch_event(&mut state, &registry, &GameEvent::RunInitiated { server: ServerId::Hq }).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(7));
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Runner, amount: 2 }]);
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
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Corp, amount: 3 }]);
    }

    #[test]
    fn dispatch_wires_up_previously_dead_on_successful_run() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger("desperado", Side::Runner, Trigger::OnSuccessfulRun, Effect::GainCredits(Side::Runner, 1)));

        let mut state = empty_state();
        state.runner.rig = vec![rig_card("desperado")];

        let events = dispatch_event(&mut state, &registry, &GameEvent::RunSucceeded { server: ServerId::RnD }).unwrap();

        assert_eq!(state.runner.resources.credits, Credits(6));
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Runner, amount: 1 }]);
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
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Runner, amount: 1 }]);
    }

    #[test]
    fn ice_rezzed_dispatches_on_rez_against_the_rezzed_card() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger("ping", Side::Corp, Trigger::OnRez, Effect::GiveTags(1)));

        let mut state = empty_state();
        let events =
            dispatch_event(&mut state, &registry, &GameEvent::IceRezzed { card: CardId("ping".to_string()), server: ServerId::Hq })
                .unwrap();

        assert_eq!(state.runner.tags, 1);
        assert_eq!(events, vec![GameEvent::TagsGiven { side: Side::Runner, amount: 1 }]);
    }

    #[test]
    fn run_succeeded_dispatches_on_approach_server_against_rezzed_root_installs_only() {
        let mut registry = CardRegistry::new();
        registry.insert(card_with_trigger("manegarm", Side::Corp, Trigger::OnApproachServer, Effect::GainCredits(Side::Corp, 3)));
        registry.insert(card_with_trigger("unrezzed_upgrade", Side::Corp, Trigger::OnApproachServer, Effect::GainCredits(Side::Corp, 99)));

        let mut state = empty_state();
        state.corp.installed = vec![
            crate::rules::state::InstalledCard {
                card: CardId("manegarm".to_string()),
                slot: crate::rules::state::InstallSlot::Root,
                rezzed: true,
                ..Default::default()
            },
            crate::rules::state::InstalledCard {
                card: CardId("unrezzed_upgrade".to_string()),
                slot: crate::rules::state::InstallSlot::Root,
                ..Default::default()
            },
        ];

        let events = dispatch_event(&mut state, &registry, &GameEvent::RunSucceeded { server: ServerId::Hq }).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(8), "only the rezzed root install fired");
        assert!(events.contains(&GameEvent::CreditsGained { side: Side::Corp, amount: 3 }));
        assert!(!events.iter().any(|e| matches!(e, GameEvent::CreditsGained { amount: 99, .. })));
        assert!(state.runner.made_successful_run_this_turn);
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
