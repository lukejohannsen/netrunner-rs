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
//! check_win_conditions` already follows. Card *behavior* itself stays
//! entirely data-driven (`dsl::TriggeredEffect`/`AbilityDef`/`Effect`) —
//! this module adds only the event-to-audience mapping and firing order.

use crate::cards::CardRegistry;
use crate::dsl::{CardId, CardSubtype, Trigger};
use crate::rules::ability;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::ServerId;
use crate::rules::state::{GameState, Side};

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
            let is_virus = registry.get(card).is_some_and(|c| c.subtypes.contains(&CardSubtype::Virus));
            if is_virus && let Some(identity) = state.runner.identity.clone() {
                ability::process_card_triggers(state, registry, &identity, Trigger::OnVirusInstalled)
            } else {
                Ok(Vec::new())
            }
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

        GameEvent::CardAccessed { card, .. } => {
            ability::process_card_triggers(state, registry, card, Trigger::OnAccessed)
        }

        GameEvent::CardTrashedFromAccess { card, .. } => {
            ability::process_card_triggers(state, registry, card, Trigger::OnTrashedFromAccess)
        }

        GameEvent::AgendaScored { card, .. } => {
            let mut events = ability::process_card_triggers(state, registry, card, Trigger::OnAgendaScored)?;
            if let Some(identity) = state.corp.identity.clone() {
                events.extend(ability::process_card_triggers(state, registry, &identity, Trigger::OnAgendaScored)?);
            }
            Ok(events)
        }

        GameEvent::AgendaStolen { .. } => match state.corp.identity.clone() {
            Some(identity) => ability::process_card_triggers(state, registry, &identity, Trigger::OnAgendaStolen),
            None => Ok(Vec::new()),
        },

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
            // Gabriel Santiago-style identity reaction (only on HQ) and any
            // broadcast "on a successful run" rig reaction (any server, e.g.
            // Desperado) both key off this one event — collected as a single
            // ordered candidate list (both Runner-side today, so
            // `order_active_first` is a no-op in practice until a Corp-side
            // "runner made a successful run" reactor exists) rather than two
            // separate passes.
            let mut candidates: Vec<(Side, CardId)> = Vec::new();
            if *server == ServerId::Hq && let Some(identity) = state.runner.identity.clone() {
                candidates.push((Side::Runner, identity));
            }
            candidates.extend(state.runner.rig.iter().map(|card| (Side::Runner, card.card.clone())));

            let mut events = Vec::new();
            for card_id in order_active_first(Side::Runner, candidates) {
                let trigger = if state.runner.identity.as_ref() == Some(&card_id) {
                    Trigger::OnSuccessfulRunOnHq
                } else {
                    Trigger::OnSuccessfulRun
                };
                events.extend(ability::process_card_triggers(state, registry, &card_id, trigger)?);
            }
            Ok(events)
        }

        _ => Ok(Vec::new()),
    }
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
    use crate::dsl::{Card, CardType, Effect, TriggeredEffect};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, Credits, GamePhase, InstalledRunnerCard, MemoryUnits, PlayerResources,
        RunnerState,
    };

    fn empty_state() -> GameState {
        GameState {
            corp: CorpState {
                identity: None,
                bad_publicity: 0,
                first_install_used_this_turn: false,
                recurring_credits: 0,
                recurring_credits_max: 0,
                scored_agendas: Vec::new(),
                resources: PlayerResources { credits: Credits(5), clicks: Clicks(3), agenda_points: AgendaPoints(0) },
                hq: Vec::new(),
                r_and_d: Vec::new(),
                archives: Vec::new(),
                installed: Vec::new(),
            },
            runner: RunnerState {
                identity: None,
                scored_agendas: Vec::new(),
                resources: PlayerResources { credits: Credits(5), clicks: Clicks(4), agenda_points: AgendaPoints(0) },
                memory_units: MemoryUnits(0),
                brain_damage: 0,
                tags: 0,
                grip: Vec::new(),
                stack: Vec::new(),
                rig: Vec::new(),
                heap: Vec::new(),
                link_strength: 0,
                first_hq_run_used_this_turn: false,
                first_install_discount_used_this_turn: false,
            },
            phase: GamePhase::Action(Side::Runner),
            active_run: None,
            paid_ability_window: None,
            active_trace: None,
            seed: 0,
            rng_step: 0,
        }
    }

    fn card_with_trigger(id: &str, side: Side, trigger: Trigger, effect: Effect) -> Card {
        Card {
            id: CardId(id.to_string()),
            title: id.to_string(),
            side,
            card_type: CardType::Program,
            cost: 0,
            triggers: vec![TriggeredEffect { trigger, effects: vec![effect], requirement: None }],
            abilities: Vec::new(),
            trash_cost: None,
            steal_cost: None,
            advancement_requirement: None,
            agenda_points: None,
            min_deck_size: None,
            strength: None,
            subroutines: Vec::new(),
            interactive_on_access: None,
            subtypes: Vec::new(),
            play_requirement: None,
            recurring_credits: None,
            first_install_discount: None,
        }
    }

    fn rig_card(id: &str) -> InstalledRunnerCard {
        InstalledRunnerCard { card: CardId(id.to_string()), base_strength: 0, encounter_strength_buff: 0, turn_strength_buff: 0 }
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
}
