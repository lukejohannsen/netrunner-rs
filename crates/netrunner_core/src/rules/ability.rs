use crate::cards::CardRegistry;
use crate::dsl::{CardId, CardTarget, Cost, Effect, StackZone, Trigger};
use crate::rules::damage;
use crate::rules::error::RulesError;
use crate::rules::event::GameEvent;
use crate::rules::run::{self, RunPhase, SubroutineStatus};
use crate::rules::state::{Credits, GamePhase, GameState, Side};

/// Applies a single, already-resolved `Effect` to `state` in place.
///
/// A deliberate new hybrid mutation convention: mutate-in-place like
/// `damage::apply_damage`/`run::access_server` (the caller has already
/// cloned/validated phase, so this never needs to reclone), but fallible
/// unlike them — some `Effect` arms genuinely can fail against a
/// well-formed state (`TrashCard` naming a target that isn't where it's
/// claimed to be) while others structurally cannot.
///
/// `CardTarget::ThisCard` is a known signature gap: nothing here
/// identifies "which card is currently resolving." The future dispatch
/// layer that calls `evaluate_effect` already knows which card/ability is
/// resolving, so it should rewrite `ThisCard` into a concrete
/// `CorpInstalled`/`RunnerRig` target before calling in, rather than
/// widening this signature with a `source` parameter every other `Effect`
/// arm would ignore. Reaching this arm as-is returns
/// `RulesError::UnresolvedCardTarget` rather than panicking, per AGENTS.md's
/// "no panics in engine code" rule.
pub fn evaluate_effect(state: &mut GameState, effect: &Effect) -> Result<Vec<GameEvent>, RulesError> {
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

        Effect::TrashCard(target) => trash_card(state, target),
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
pub fn resolve_unbroken_subroutines(state: &mut GameState) -> Result<Vec<GameEvent>, RulesError> {
    let mut events = Vec::new();

    loop {
        if matches!(state.phase, GamePhase::GameOver(_)) {
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
        let fired_events = evaluate_effect(state, &effect)?;
        events.push(GameEvent::SubroutineFired { card_id, index, effect });
        events.extend(fired_events);
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
        for effect in &triggered.effects {
            events.extend(evaluate_effect(state, effect)?);
        }
    }
    Ok(events)
}

fn trash_card(state: &mut GameState, target: &CardTarget) -> Result<Vec<GameEvent>, RulesError> {
    match target {
        CardTarget::ThisCard => Err(RulesError::UnresolvedCardTarget),

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
                .position(|c| c == card)
                .ok_or_else(|| RulesError::CardNotInRig { side: Side::Runner, card: card.clone() })?;
            state.runner.rig.remove(position);
            state.runner.heap.push(card.clone());
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

/// Pays `cost` on `side`'s behalf, mutating `state` in place. Kept as a
/// function separate from `evaluate_effect` — mirroring `AbilityDef`
/// itself already modeling cost and effect as two separate fields — so a
/// future dispatch path calls `pay_cost` then, only on success,
/// `evaluate_effect`, matching real Netrunner's "costs are paid first,
/// then the ability resolves" structure.
pub fn pay_cost(state: &mut GameState, side: Side, cost: &Cost) -> Result<Vec<GameEvent>, RulesError> {
    match cost {
        Cost::Credits(amount) => {
            let available = state.resources(side).credits.0;
            if available < *amount {
                return Err(RulesError::NotEnoughCredits { side, available, requested: *amount });
            }
            state.resources_mut(side).credits = Credits(available - amount);
            Ok(vec![GameEvent::CreditsSpent { side, amount: *amount }])
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

        // Same "which card is this" gap as CardTarget::ThisCard.
        Cost::TrashSelf => Err(RulesError::UnresolvedCardTarget),

        Cost::PurgeTags => {
            state.runner.tags = 0;
            Ok(vec![GameEvent::TagsPurged { side }])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{Card, CardId, CardType, DamageType, SubroutineDef, TriggeredEffect};
    use crate::rules::run::{EncounteredSubroutine, RunIce, RunPhase as RP, RunState, ServerId, SubroutineStatus};
    use crate::rules::state::{
        AgendaPoints, Clicks, CorpState, GamePhase, InstallSlot, InstalledCard, MemoryUnits,
        PlayerResources, RunnerState,
    };

    fn game_state() -> GameState {
        GameState {
            corp: CorpState {
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
            runner: RunnerState {
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
            },
            phase: GamePhase::Action(Side::Runner),
            active_run: None,
            seed: 0,
            rng_step: 0,
        }
    }

    #[test]
    fn gain_credits_targets_the_named_side() {
        let mut state = game_state();
        let events = evaluate_effect(&mut state, &Effect::GainCredits(Side::Corp, 3)).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(8));
        assert_eq!(state.runner.resources.credits, Credits(5));
        assert_eq!(events, vec![GameEvent::CreditsGained { side: Side::Corp, amount: 3 }]);
    }

    #[test]
    fn deal_damage_delegates_to_apply_damage() {
        let mut state = game_state();
        state.runner.grip = vec![CardId("card_0".to_string()), CardId("card_1".to_string())];

        let events = evaluate_effect(&mut state, &Effect::DealDamage(DamageType::Net, 1)).unwrap();

        assert_eq!(state.runner.grip.len(), 1);
        assert_eq!(state.runner.heap.len(), 1);
        assert!(matches!(events[0], GameEvent::DamageTaken { damage_type: DamageType::Net, amount: 1 }));
    }

    #[test]
    fn draw_cards_stops_silently_on_an_empty_deck() {
        let mut state = game_state();
        state.runner.stack = vec![CardId("only_card".to_string())];

        let events = evaluate_effect(&mut state, &Effect::DrawCards(Side::Runner, 3)).unwrap();

        assert_eq!(state.runner.grip, vec![CardId("only_card".to_string())]);
        assert!(state.runner.stack.is_empty());
        assert_eq!(events, vec![GameEvent::CardDrawn { side: Side::Runner }]);
    }

    #[test]
    fn end_the_run_clears_active_run_and_emits_event() {
        let mut state = game_state();
        state.active_run = Some(RunState { access_state: None, server: ServerId::Hq, phase: RP::ApproachIce, ice: Vec::new(), position: 0 });

        let events = evaluate_effect(&mut state, &Effect::EndTheRun).unwrap();

        assert!(state.active_run.is_none());
        assert_eq!(events, vec![GameEvent::RunEndedByEffect { server: ServerId::Hq }]);
    }

    #[test]
    fn end_the_run_with_no_active_run_errors() {
        let mut state = game_state();
        assert_eq!(evaluate_effect(&mut state, &Effect::EndTheRun), Err(RulesError::NoActiveRun));
    }

    #[test]
    fn give_tags_always_targets_the_runner() {
        let mut state = game_state();
        let events = evaluate_effect(&mut state, &Effect::GiveTags(2)).unwrap();

        assert_eq!(state.runner.tags, 2);
        assert_eq!(events, vec![GameEvent::TagsGiven { side: Side::Runner, amount: 2 }]);
    }

    fn test_ice(card_id: &str, strength: i32, subroutine_count: usize) -> RunIce {
        RunIce {
            card_id: CardId(card_id.to_string()),
            current_strength: strength,
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
        }
    }

    #[test]
    fn break_subroutine_breaks_the_targeted_pending_subroutine() {
        let mut state = game_state();
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 2)],
            position: 0,
        });

        let events = evaluate_effect(&mut state, &Effect::BreakSubroutine(0)).unwrap();

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
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![test_ice("ice_wall", 0, 1)],
            position: 0,
        });

        assert_eq!(
            evaluate_effect(&mut state, &Effect::BreakSubroutine(1)),
            Err(RulesError::InvalidSubroutineIndex(1))
        );
    }

    #[test]
    fn break_subroutine_already_broken_errors() {
        let mut state = game_state();
        let mut ice = test_ice("ice_wall", 0, 1);
        ice.subroutines[0].status = SubroutineStatus::Broken;
        state.active_run =
            Some(RunState { access_state: None, server: ServerId::Hq, phase: RP::EncounterIce, ice: vec![ice], position: 0 });

        assert_eq!(
            evaluate_effect(&mut state, &Effect::BreakSubroutine(0)),
            Err(RulesError::SubroutineAlreadyHandled)
        );
    }

    #[test]
    fn break_subroutine_with_no_active_run_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::BreakSubroutine(0)),
            Err(RulesError::NoActiveRun)
        );
    }

    #[test]
    fn break_subroutine_outside_encounter_ice_errors() {
        let mut state = game_state();
        state.active_run =
            Some(RunState { access_state: None, server: ServerId::Hq, phase: RP::ApproachIce, ice: Vec::new(), position: 0 });

        assert_eq!(
            evaluate_effect(&mut state, &Effect::BreakSubroutine(0)),
            Err(RulesError::NotInEncounter)
        );
    }

    #[test]
    fn resolve_unbroken_subroutines_resolves_each_pending_subroutine_in_order() {
        let mut state = game_state();
        let mut ice = test_ice("ice_wall", 0, 2);
        ice.subroutines[0].definition.effect = Effect::GiveTags(2);
        ice.subroutines[1].definition.effect = Effect::GainCredits(Side::Corp, 3);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![ice],
            position: 0,
        });

        let events = resolve_unbroken_subroutines(&mut state).unwrap();

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
        let mut ice = test_ice("ice_wall", 0, 2);
        ice.subroutines[0].definition.effect = Effect::EndTheRun;
        ice.subroutines[1].definition.effect = Effect::GiveTags(5);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![ice],
            position: 0,
        });

        let events = resolve_unbroken_subroutines(&mut state).unwrap();

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
        let mut ice = test_ice("ice_wall", 0, 2);
        ice.subroutines[0].status = SubroutineStatus::Broken;
        ice.subroutines[1].definition.effect = Effect::GiveTags(1);
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![ice],
            position: 0,
        });

        let events = resolve_unbroken_subroutines(&mut state).unwrap();

        assert_eq!(events.len(), 2);
        let run = state.active_run.unwrap();
        assert_eq!(run.ice[0].subroutines[0].status, SubroutineStatus::Broken);
        assert_eq!(run.ice[0].subroutines[1].status, SubroutineStatus::Resolved);
    }

    #[test]
    fn modify_strength_updates_current_strength_and_emits_event() {
        let mut state = game_state();
        state.active_run = Some(RunState { access_state: None,
            server: ServerId::Hq,
            phase: RP::EncounterIce,
            ice: vec![test_ice("ice_wall", 3, 0)],
            position: 0,
        });

        let events = evaluate_effect(&mut state, &Effect::ModifyStrength(2)).unwrap();

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
            Some(RunState { access_state: None, server: ServerId::Hq, phase: RP::ApproachIce, ice: Vec::new(), position: 0 });

        assert_eq!(
            evaluate_effect(&mut state, &Effect::ModifyStrength(2)),
            Err(RulesError::NotInEncounter)
        );
    }

    #[test]
    fn modify_strength_with_no_active_run_errors() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::ModifyStrength(2)),
            Err(RulesError::NoActiveRun)
        );
    }

    #[test]
    fn trash_card_this_card_is_rejected_not_panicked() {
        let mut state = game_state();
        assert_eq!(
            evaluate_effect(&mut state, &Effect::TrashCard(CardTarget::ThisCard)),
            Err(RulesError::UnresolvedCardTarget)
        );
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
            evaluate_effect(&mut state, &Effect::TrashCard(CardTarget::RunnerRig(CardId("gordian_blade".to_string())))),
            Err(RulesError::CardNotInRig { side: Side::Runner, card: CardId("gordian_blade".to_string()) })
        );
    }

    #[test]
    fn trash_card_runner_rig_moves_card_to_heap() {
        let mut state = game_state();
        state.runner.rig = vec![CardId("gordian_blade".to_string())];

        let events = evaluate_effect(
            &mut state,
            &Effect::TrashCard(CardTarget::RunnerRig(CardId("gordian_blade".to_string()))),
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
            ),
            Err(RulesError::EmptyZone { side: Side::Corp, zone: StackZone::RAndD })
        );
    }

    #[test]
    fn pay_credits_deducts_and_errors_when_insufficient() {
        let mut state = game_state();
        let events = pay_cost(&mut state, Side::Corp, &Cost::Credits(3)).unwrap();

        assert_eq!(state.corp.resources.credits, Credits(2));
        assert_eq!(events, vec![GameEvent::CreditsSpent { side: Side::Corp, amount: 3 }]);

        assert_eq!(
            pay_cost(&mut state, Side::Corp, &Cost::Credits(10)),
            Err(RulesError::NotEnoughCredits { side: Side::Corp, available: 2, requested: 10 })
        );
    }

    #[test]
    fn pay_clicks_spends_the_requested_amount() {
        let mut state = game_state();
        let events = pay_cost(&mut state, Side::Runner, &Cost::Clicks(2)).unwrap();

        assert_eq!(state.runner.resources.clicks, Clicks(2));
        assert_eq!(events, vec![GameEvent::ClickSpent { side: Side::Runner }, GameEvent::ClickSpent { side: Side::Runner }]);
    }

    #[test]
    fn pay_purge_tags_zeroes_the_counter() {
        let mut state = game_state();
        state.runner.tags = 3;

        let events = pay_cost(&mut state, Side::Runner, &Cost::PurgeTags).unwrap();

        assert_eq!(state.runner.tags, 0);
        assert_eq!(events, vec![GameEvent::TagsPurged { side: Side::Runner }]);
    }

    #[test]
    fn pay_trash_self_is_rejected_not_panicked() {
        let mut state = game_state();
        assert_eq!(
            pay_cost(&mut state, Side::Runner, &Cost::TrashSelf),
            Err(RulesError::UnresolvedCardTarget)
        );
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
}
